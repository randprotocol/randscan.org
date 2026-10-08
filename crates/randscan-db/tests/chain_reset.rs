//! Chain reset keeps user data. Runs against `DATABASE_URL`; skips when unset.

use randscan_db::{
    get_indexer_state, insert_block, insert_notes, insert_transaction, reset_chain_data,
    run_migrations, set_chain_id, set_next_height, set_next_leaf, NewBlock, NewBundle, NewNote,
    NewTx, CHAIN_TABLES,
};
use sqlx::postgres::PgPoolOptions;

#[tokio::test]
async fn reset_chain_data_truncates_chain_tables_and_keeps_users() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .unwrap();
    run_migrations(&pool).await.unwrap();
    let mut conn = pool.acquire().await.unwrap();

    // One block with a bridge_burn (the chain-14 single hidden-asset bundle: four slots, its
    // `burn_a`/`burn_asset` carrying the redemption), one tree leaf, and a user that must survive.
    for t in CHAIN_TABLES {
        sqlx::query(&format!("TRUNCATE {t} CASCADE"))
            .execute(&mut *conn)
            .await
            .unwrap();
    }
    insert_block(
        &mut conn,
        &NewBlock {
            hash: &"ab".repeat(32),
            height: 0,
            view: 0,
            parent: &"00".repeat(32),
            proposer: "2nRd",
            timestamp_ms: 1,
            tx_root: &"00".repeat(32),
            state_root: &"00".repeat(32),
            justify_view: 0,
            tx_count: 1,
        },
    )
    .await
    .unwrap();
    let bundle = |tag: &str| NewBundle {
        anchor: "aa".repeat(32),
        nullifiers: [
            format!("{tag}1").repeat(32),
            format!("{tag}2").repeat(32),
            format!("{tag}3").repeat(32),
            format!("{tag}4").repeat(32),
        ],
        commitments: [
            format!("{tag}5").repeat(32),
            format!("{tag}6").repeat(32),
            format!("{tag}7").repeat(32),
            format!("{tag}8").repeat(32),
        ],
        fee: "2000000".into(),
        burn_a: "500".into(),
        burn_r: "0".into(),
        burn_asset: 2,
        time: 0,
        proof_len: 302857,
        envelope_len: [1380, 1380, 1380, 1380],
        auth_commit: Some("c0".repeat(32)),
        auth_proof_len: 1_360_000,
    };
    insert_transaction(
        &mut conn,
        &NewTx {
            hash: &"cd".repeat(32),
            block_hash: &"ab".repeat(32),
            height: 0,
            tx_index: 0,
            chain_id: 4,
            timestamp_ms: 1,
            kind: "bridge_burn",
            bundle: Some(bundle("d")),
            asset_index: Some(2),
            amount: Some("400".into()),
            relayer_fee: Some("100".into()),
            to_chain: Some(2),
            bridge_to: Some(&"00".repeat(32)),
            bridge_token: Some(&"ee".repeat(32)),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    insert_notes(
        &mut conn,
        &[NewNote {
            leaf_index: 0,
            cm: "d5".repeat(32),
            height: 0,
            envelope: None,
        }],
    )
    .await
    .unwrap();
    set_next_leaf(&mut conn, 1).await.unwrap();
    set_next_height(&mut conn, 1, Some(&"ab".repeat(32)))
        .await
        .unwrap();
    set_chain_id(&mut conn, 4).await.unwrap();
    // The database is shared across runs and users survive the reset by design, so drop any
    // copy of this user a previous run left behind before inserting it again.
    sqlx::query("DELETE FROM users WHERE email = 'reset@example.com'")
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (email, password_hash) VALUES ('reset@example.com', 'x')")
        .execute(&mut *conn)
        .await
        .unwrap();

    let stored: (String, Option<String>, Option<i32>, bool) = sqlx::query_as(
        "SELECT kind, amount::text, to_chain, has_bundle FROM transactions WHERE hash = $1",
    )
    .bind("cd".repeat(32))
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    assert_eq!(
        stored,
        ("bridge_burn".into(), Some("400".into()), Some(2), true)
    );
    let nfs: i64 = sqlx::query_scalar("SELECT count(*) FROM nullifiers")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(
        nfs, 4,
        "the one hidden-asset bundle's four slots are recorded"
    );
    let (burn_a, burn_asset): (String, i64) =
        sqlx::query_as("SELECT burn_a::text, burn_asset FROM transactions WHERE hash = $1")
            .bind("cd".repeat(32))
            .fetch_one(&mut *conn)
            .await
            .unwrap();
    assert_eq!((burn_a, burn_asset), ("500".into(), 2));
    let linked: Option<String> =
        sqlx::query_scalar("SELECT tx_hash FROM notes WHERE leaf_index = 0")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
    assert_eq!(
        linked.as_deref(),
        Some("cd".repeat(32).as_str()),
        "a leaf is linked to the tx that carried its commitment"
    );

    reset_chain_data(&mut conn, 5).await.unwrap();

    for t in CHAIN_TABLES {
        let n: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {t}"))
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(n, 0, "{t} not emptied");
    }
    let st = get_indexer_state(&pool).await.unwrap();
    assert_eq!(st.next_height, 0);
    assert_eq!(st.next_leaf, 0);
    assert_eq!(st.last_indexed_hash, None);
    assert_eq!(st.chain_id, Some(5));
    let users: i64 =
        sqlx::query_scalar("SELECT count(*) FROM users WHERE email = 'reset@example.com'")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
    assert_eq!(users, 1);
    let (chain_id, height, notes): (i64, i64, i64) =
        sqlx::query_as("SELECT chain_id, height, notes FROM network_stats WHERE id = 1")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
    assert_eq!((chain_id, height, notes), (5, 0, 0));
}
