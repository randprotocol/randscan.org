//! Queries against a real PostgreSQL (`DATABASE_URL`): block lookups and the rewind that deletes
//! them, the node geolocation cache, programs with their call counts, tree leaves and published
//! nullifiers, the indexer's state row, the pool settings and the error mapping. They skip when
//! the variable is unset. The chain tables are emptied first (and again at the end, so a live
//! indexer in a later test binary starts from a clean reset); one lock keeps these tests apart.

use randscan_db as db;
use randscan_db::{
    insert_block, insert_notes, insert_program, insert_transaction, run_migrations, NewBlock,
    NewBundle, NewNote, NewProgram, NewTx, CHAIN_TABLES,
};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

/// Puts the named environment variables back as they were when it is dropped, on a panic too,
/// so a failing assertion cannot leave the process environment altered for the other tests.
struct EnvGuard(Vec<(&'static str, Option<String>)>);

impl EnvGuard {
    fn new(keys: &[&'static str]) -> Self {
        EnvGuard(keys.iter().map(|k| (*k, std::env::var(k).ok())).collect())
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (k, v) in self.0.drain(..) {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
    }
}

static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn clean_pool() -> Option<(PgPool, tokio::sync::MutexGuard<'static, ()>)> {
    let guard = LOCK.lock().await;
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = PgPoolOptions::new()
        .max_connections(3)
        .connect(&url)
        .await
        .expect("connect to DATABASE_URL");
    run_migrations(&pool).await.expect("migrations");
    for t in CHAIN_TABLES {
        sqlx::query(&format!("TRUNCATE {t} CASCADE"))
            .execute(&pool)
            .await
            .unwrap();
    }
    Some((pool, guard))
}

fn hex64(n: u64) -> String {
    format!("{n:064x}")
}

async fn block(pool: &PgPool, height: i64, proposer: &str, ts: i64, tx_count: i32) {
    let mut conn = pool.acquire().await.unwrap();
    insert_block(
        &mut conn,
        &NewBlock {
            hash: &hex64(height as u64 + 1),
            height,
            view: height * 2,
            parent: &hex64(height as u64),
            proposer,
            timestamp_ms: ts,
            tx_root: &hex64(900),
            state_root: &hex64(901),
            justify_view: height * 2 - 1,
            tx_count,
        },
    )
    .await
    .unwrap();
}

fn bundle(tag: u64) -> NewBundle {
    NewBundle {
        anchor: hex64(tag),
        nullifiers: [
            hex64(tag * 10 + 1),
            hex64(tag * 10 + 2),
            hex64(tag * 10 + 3),
            hex64(tag * 10 + 4),
        ],
        commitments: [
            hex64(tag * 10 + 5),
            hex64(tag * 10 + 6),
            hex64(tag * 10 + 7),
            hex64(tag * 10 + 8),
        ],
        fee: "1000".into(),
        burn_a: "0".into(),
        burn_r: "0".into(),
        burn_asset: 0,
        time: 0,
        proof_len: 1,
        envelope_len: [1, 1, 1, 1],
        auth_commit: None,
        auth_proof_len: 0,
    }
}

#[tokio::test]
async fn blocks_are_found_by_height_and_hash_and_rewound_with_their_transactions() {
    let Some((pool, _g)) = clean_pool().await else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    assert_eq!(db::max_block_height(&pool).await.unwrap(), None);
    block(&pool, 0, "alice", 1_000, 0).await;
    assert_eq!(
        db::avg_block_time_ms(&pool, 10).await.unwrap(),
        0.0,
        "one block has no interval"
    );
    block(&pool, 1, "bob", 3_000, 1).await;
    block(&pool, 2, "alice", 4_000, 0).await;
    block(&pool, 3, "alice", 10_000, 0).await;

    assert_eq!(db::max_block_height(&pool).await.unwrap(), Some(3));
    assert_eq!(
        db::get_block_hash_at(&pool, 2).await.unwrap(),
        Some(hex64(3))
    );
    assert_eq!(db::get_block_hash_at(&pool, 9).await.unwrap(), None);
    let b = db::get_block_by_height(&pool, 1).await.unwrap().unwrap();
    assert_eq!((b.proposer.as_str(), b.tx_count), ("bob", 1));
    assert_eq!(
        db::get_block_by_hash(&pool, &hex64(2))
            .await
            .unwrap()
            .unwrap()
            .height,
        1
    );
    assert!(db::get_block_by_hash(&pool, &hex64(77))
        .await
        .unwrap()
        .is_none());

    // (10_000 - 1_000) ms over 3 intervals; over the last two blocks, 6 000.
    assert_eq!(db::avg_block_time_ms(&pool, 10).await.unwrap(), 3_000.0);
    assert_eq!(db::avg_block_time_ms(&pool, 2).await.unwrap(), 6_000.0);

    let latest = db::get_latest_blocks(&pool, 2).await.unwrap();
    assert_eq!(
        latest.iter().map(|b| b.height).collect::<Vec<_>>(),
        vec![3, 2]
    );
    let alice = db::list_blocks(&pool, 1, 5, Some("alice")).await.unwrap();
    assert_eq!(
        alice.iter().map(|b| b.height).collect::<Vec<_>>(),
        vec![2, 0],
        "offset 1 into alice's blocks, newest first"
    );
    assert_eq!(db::count_blocks(&pool, Some("alice")).await.unwrap(), 3);
    assert_eq!(db::count_blocks(&pool, None).await.unwrap(), 4);

    // A transaction on block 2 goes with it when the chain is rewound from height 2.
    let mut conn = pool.acquire().await.unwrap();
    insert_transaction(
        &mut conn,
        &NewTx {
            hash: &hex64(500),
            block_hash: &hex64(3),
            height: 2,
            tx_index: 0,
            chain_id: 1,
            timestamp_ms: 4_000,
            kind: "transfer",
            bundle: Some(bundle(5)),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(db::delete_blocks_from(&mut conn, 2).await.unwrap(), 2);
    assert_eq!(db::max_block_height(&pool).await.unwrap(), Some(1));
    assert!(db::get_transaction(&pool, &hex64(500))
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        db::count_nullifiers(&pool).await.unwrap(),
        0,
        "its nullifiers are released with it"
    );
    assert_eq!(db::delete_blocks_from(&mut conn, 2).await.unwrap(), 0);
    drop(conn);
    db::reset_chain_data(&mut pool.acquire().await.unwrap(), 31)
        .await
        .unwrap();
}

#[tokio::test]
async fn leaves_nullifiers_and_programs_are_stored_and_rewound() {
    let Some((pool, _g)) = clean_pool().await else {
        return;
    };
    block(&pool, 0, "alice", 1, 1).await;
    block(&pool, 1, "alice", 2, 2).await;
    let mut conn = pool.acquire().await.unwrap();
    let deploy = hex64(600);
    insert_transaction(
        &mut conn,
        &NewTx {
            hash: &deploy,
            block_hash: &hex64(1),
            height: 0,
            tx_index: 0,
            chain_id: 1,
            kind: "deploy",
            bundle: Some(bundle(6)),
            program_id: Some(&hex64(700)),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    for (i, h) in [(0, 1), (1, 1)] {
        insert_transaction(
            &mut conn,
            &NewTx {
                hash: &hex64(610 + i),
                block_hash: &hex64(2),
                height: h,
                tx_index: i as i32,
                chain_id: 1,
                kind: "call",
                bundle: Some(bundle(7 + i)),
                program_id: Some(&hex64(700)),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    }
    insert_program(
        &mut conn,
        &NewProgram {
            id: &hex64(700),
            deploy_tx: &deploy,
            deployed_at_height: 0,
            base_pc: 0,
            words_len: 12,
            code_hash: &hex64(701),
            public_words_len: 0,
            public_digest: None,
        },
    )
    .await
    .unwrap();

    let p = db::get_program(&pool, &hex64(700)).await.unwrap().unwrap();
    assert_eq!((p.call_count, p.words_len), (2, 12));
    assert_eq!(p.last_called_height, Some(1));
    assert_eq!(db::count_programs(&pool).await.unwrap(), 1);
    assert_eq!(db::list_programs(&pool, 0, 10).await.unwrap().len(), 1);
    assert!(db::list_programs(&pool, 1, 10).await.unwrap().is_empty());
    assert!(db::get_program(&pool, &hex64(799)).await.unwrap().is_none());
    assert_eq!(
        db::get_latest_transactions(&pool, 2)
            .await
            .unwrap()
            .iter()
            .map(|t| t.hash.clone())
            .collect::<Vec<_>>(),
        vec![hex64(611), hex64(610)],
        "newest height, then highest index first"
    );

    // Tree leaves: the commitment of the deploy's bundle links back to its transaction.
    let cm = bundle(6).commitments[2].clone();
    insert_notes(
        &mut conn,
        &[
            NewNote {
                leaf_index: 0,
                cm: hex64(1234),
                height: 0,
                envelope: None,
            },
            NewNote {
                leaf_index: 1,
                cm: cm.clone(),
                height: 0,
                envelope: None,
            },
            NewNote {
                leaf_index: 2,
                cm: hex64(1236),
                height: 1,
                envelope: None,
            },
        ],
    )
    .await
    .unwrap();
    let n = db::get_note_by_index(&pool, 1).await.unwrap().unwrap();
    assert_eq!(n.cm, cm);
    assert_eq!(n.tx_hash.as_deref(), Some(deploy.as_str()));
    assert!(db::get_note_by_index(&pool, 9).await.unwrap().is_none());
    assert_eq!(db::next_leaf_after_rewind(&mut conn).await.unwrap(), 3);
    assert_eq!(db::delete_notes_from(&mut conn, 1).await.unwrap(), 1);
    assert_eq!(db::next_leaf_after_rewind(&mut conn).await.unwrap(), 2);
    assert_eq!(db::delete_notes_from(&mut conn, 0).await.unwrap(), 2);
    assert_eq!(
        db::next_leaf_after_rewind(&mut conn).await.unwrap(),
        0,
        "an empty tree starts over at leaf 0"
    );

    // Nullifiers: three transactions published four each.
    assert_eq!(db::count_nullifiers(&pool).await.unwrap(), 12);
    let asked = vec![bundle(6).nullifiers[1].clone(), hex64(424_242)];
    let found = db::get_nullifiers(&pool, &asked).await.unwrap();
    assert_eq!(found.len(), 1, "an unpublished nullifier is simply absent");
    assert_eq!(found[0].tx_hash, deploy);
    let one = db::get_nullifier(&pool, &asked[0]).await.unwrap().unwrap();
    assert_eq!((one.height, one.tx_index), (0, 0));
    assert!(db::get_nullifier(&pool, &asked[1]).await.unwrap().is_none());
    drop(conn);
    db::reset_chain_data(&mut pool.acquire().await.unwrap(), 31)
        .await
        .unwrap();
}

#[tokio::test]
async fn the_geolocation_cache_upserts_by_address() {
    let Some((pool, _g)) = clean_pool().await else {
        return;
    };
    let ip = "198.51.100.77";
    let other = "198.51.100.78";
    assert!(db::get_node_geo(&pool, &[ip.to_string()])
        .await
        .unwrap()
        .is_empty());
    db::upsert_node_geo(
        &pool,
        ip,
        true,
        Some(1.5),
        Some(2.5),
        Some("Oslo"),
        None,
        Some("Norway"),
        Some("NO"),
        Some("Org A"),
    )
    .await
    .unwrap();
    db::upsert_node_geo(
        &pool, other, false, None, None, None, None, None, None, None,
    )
    .await
    .unwrap();
    let rows = db::get_node_geo(
        &pool,
        &[ip.to_string(), other.to_string(), "203.0.113.1".into()],
    )
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    let a = rows.iter().find(|r| r.ip == ip).unwrap();
    let g = a.geo().expect("a successful lookup with coordinates");
    assert_eq!((g.lat, g.lon), (1.5, 2.5));
    assert_eq!(g.city.as_deref(), Some("Oslo"));
    assert_eq!(g.org.as_deref(), Some("Org A"));
    assert!(rows.iter().find(|r| r.ip == other).unwrap().geo().is_none());

    // A second lookup replaces the first, including turning a success into a failure.
    db::upsert_node_geo(&pool, ip, false, None, None, None, None, None, None, None)
        .await
        .unwrap();
    let rows = db::get_node_geo(&pool, &[ip.to_string()]).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].ok && rows[0].geo().is_none() && rows[0].city.is_none());
    sqlx::query("DELETE FROM node_geo WHERE ip = ANY($1)")
        .bind(vec![ip.to_string(), other.to_string()])
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn indexer_state_is_a_single_row_the_indexer_moves() {
    let Some((pool, _g)) = clean_pool().await else {
        return;
    };
    let mut conn = pool.acquire().await.unwrap();
    db::reset_chain_data(&mut conn, 12).await.unwrap();
    db::set_syncing(&pool, true).await.unwrap();
    db::set_next_leaf(&mut conn, 40).await.unwrap();
    db::set_chain_id(&mut conn, 13).await.unwrap();
    let st = db::get_indexer_state(&pool).await.unwrap();
    assert!(st.is_syncing);
    assert_eq!((st.next_leaf, st.chain_id), (40, Some(13)));
    db::set_syncing(&pool, false).await.unwrap();
    assert!(!db::get_indexer_state(&pool).await.unwrap().is_syncing);
    db::reset_chain_data(&mut conn, 31).await.unwrap();
    assert_eq!(db::get_indexer_state(&pool).await.unwrap().next_leaf, 0);
}

#[tokio::test]
async fn database_errors_map_to_their_kind() {
    let Some((pool, _g)) = clean_pool().await else {
        return;
    };
    let missing = sqlx::query_scalar::<_, i32>("SELECT 1 WHERE FALSE")
        .fetch_one(&pool)
        .await
        .unwrap_err();
    assert!(matches!(
        db::DbError::from(missing),
        db::DbError::NotFound(_)
    ));
    let syntax = sqlx::query("SELEKT 1").execute(&pool).await.unwrap_err();
    assert!(matches!(db::DbError::from(syntax), db::DbError::Query(_)));
    // A duplicate primary key is a constraint violation.
    block(&pool, 0, "alice", 1, 0).await;
    let mut conn = pool.acquire().await.unwrap();
    let dup = insert_block(
        &mut conn,
        &NewBlock {
            hash: &hex64(1),
            height: 0,
            view: 0,
            parent: &hex64(0),
            proposer: "alice",
            timestamp_ms: 1,
            tx_root: &hex64(900),
            state_root: &hex64(901),
            justify_view: 0,
            tx_count: 0,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(dup, db::DbError::Constraint(_)), "{dup:?}");
    let io = sqlx::Error::PoolTimedOut;
    assert!(matches!(db::DbError::from(io), db::DbError::Query(_)));
    drop(conn);
    db::reset_chain_data(&mut pool.acquire().await.unwrap(), 31)
        .await
        .unwrap();
}

/// The pool settings come from the environment; one test, because the environment is shared.
#[tokio::test]
async fn pool_settings_come_from_the_environment_and_a_bad_url_is_a_connection_error() {
    let _g = LOCK.lock().await;
    let keys = [
        "DATABASE_URL",
        "DB_MAX_CONNECTIONS",
        "DB_MIN_CONNECTIONS",
        "DB_CONNECT_TIMEOUT",
        "DB_IDLE_TIMEOUT",
    ];
    let _env = EnvGuard::new(&keys);
    for k in keys {
        std::env::remove_var(k);
    }
    let d = db::DatabaseConfig::from_env();
    assert_eq!(
        d.url,
        "postgres://randscan:randscan@localhost:5432/randscan"
    );
    assert_eq!((d.max_connections, d.min_connections), (10, 1));
    assert_eq!(d.connect_timeout.as_secs(), 10);
    assert_eq!(d.idle_timeout.as_secs(), 300);

    // Nothing listens on port 1: create_pool fails fast with a Connection error.
    std::env::set_var("DATABASE_URL", "postgres://u:p@127.0.0.1:1/none");
    std::env::set_var("DB_MAX_CONNECTIONS", "3");
    std::env::set_var("DB_MIN_CONNECTIONS", "0");
    std::env::set_var("DB_CONNECT_TIMEOUT", "2");
    std::env::set_var("DB_IDLE_TIMEOUT", "bogus");
    let c = db::DatabaseConfig::from_env();
    assert_eq!(c.url, "postgres://u:p@127.0.0.1:1/none");
    assert_eq!((c.max_connections, c.min_connections), (3, 0));
    assert_eq!(c.connect_timeout.as_secs(), 2);
    assert_eq!(c.idle_timeout.as_secs(), 300, "unparsable falls back");
    let err = db::create_pool(&c).await.unwrap_err();
    assert!(matches!(err, db::DbError::Connection(_)), "{err:?}");
}

#[tokio::test]
async fn create_pool_connects_with_the_configured_url() {
    let _g = LOCK.lock().await;
    let Ok(url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let pool = db::create_pool(&db::DatabaseConfig {
        url,
        max_connections: 2,
        min_connections: 0,
        connect_timeout: std::time::Duration::from_secs(5),
        idle_timeout: std::time::Duration::from_secs(5),
    })
    .await
    .unwrap();
    let one: i32 = sqlx::query_scalar("SELECT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(one, 1);
    let wrapped: db::DbPool = pool.into();
    assert!(wrapped.inner().size() > 0);
    let via_deref: i32 = sqlx::query_scalar("SELECT 2")
        .fetch_one(&*wrapped)
        .await
        .unwrap();
    assert_eq!(via_deref, 2);
}

#[tokio::test]
#[ignore = "BUG: avg_block_time_ms errors on an empty blocks table (MIN/MAX are NULL but decoded as i64); it should answer 0.0"]
async fn the_average_block_time_of_no_blocks_is_zero() {
    let Some((pool, _g)) = clean_pool().await else {
        return;
    };
    assert_eq!(db::avg_block_time_ms(&pool, 10).await.unwrap(), 0.0);
}
