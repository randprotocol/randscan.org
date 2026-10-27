//! `GET /api/v1/chain` and the chain fields of `/api/v1/health` against a scripted node (site-sync
//! design, 2026-10-27): live on the pinned chain, behind by more than `MAX_LAG`, unreachable,
//! stalled, and a node that switches genesis under a running indexer, which stops writing until
//! the node comes back. Live indexer, needs `DATABASE_URL`. One lock: each test resets the shared
//! database to its own chain.

mod common;

use common::mock_node::*;
use common::*;
use randscan_core::ChainPin;
use serde_json::{json, Value};
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(40);

static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A previous run may have left this exact chain (same genesis) behind, which the first pass
/// would adopt as is; start from nothing.
async fn fresh(live: &LiveApp) {
    let mut conn = live.pool.acquire().await.unwrap();
    randscan_db::reset_chain_data(&mut conn, 0).await.unwrap();
}

async fn get(live: &LiveApp, path: &str) -> Value {
    call(&live.app, json_req("GET", path, None, None)).await.2
}

#[tokio::test]
async fn chain_view_tracks_live_behind_unreachable_and_stalled() {
    let _g = LOCK.lock().await;
    let mut chain = MockChain::new(40, "sync-a");
    for i in 0..3 {
        chain.push_block(vec![tx(
            40,
            &format!("s{i}"),
            Some(bundle(&format!("sa{i}"))),
            json!({ "kind": "none" }),
        )]);
    }
    chain.go_live();
    let genesis = h("block-40-sync-a-0");
    let node = start_mock_node(chain).await;
    let pin = ChainPin::new(40, &genesis);
    let Some(live) = live_app_pinned(test_config(), &node.url, "127.0.0.1", pin).await else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    fresh(&live).await;
    live.start();

    // Live: the stored genesis is the node's and the pin, the head is the node's head.
    let ch = live
        .wait_for("/api/v1/chain", WAIT, |c| {
            c["head_height"] == 3 && c["state"] == "live"
        })
        .await;
    assert_eq!(ch["chain_id"], 40);
    assert_eq!(ch["genesis_hash"], genesis);
    assert_eq!(ch["node_genesis_hash"], genesis);
    assert_eq!(ch["node_chain_id"], 40);
    assert_eq!(ch["node_height"], 3);
    assert_eq!(ch["lag"], 0);
    assert_eq!(ch["halted"], false);
    assert_eq!(ch["node_connected"], true);
    assert_eq!(ch["expected_chain_id"], 40);
    assert_eq!(ch["expected_genesis_hash"], genesis);
    let head_ts = node.with_chain(|c| c.blocks[3]["timestamp_ms"].clone());
    assert_eq!(ch["head_timestamp_ms"], head_ts);

    let health = live
        .wait_for("/api/v1/health", WAIT, |b| b["status"] == "healthy")
        .await;
    assert_eq!(health["genesis_hash"], genesis);
    assert_eq!(health["head_timestamp_ms"], head_ts);
    assert_eq!(health["stalled"], false);
    assert_eq!(health["wrong_chain"], false);
    // The pre-existing fields are unchanged.
    assert_eq!(health["database"], true);
    assert_eq!(health["indexer"]["current_height"], 3);
    assert!(health["version"].is_string());

    // zusd.money reads /chain from the browser: it must be readable cross-origin.
    let req = axum::http::Request::builder()
        .uri("/api/v1/chain")
        .header("origin", "https://zusd.money")
        .body(axum::body::Body::empty())
        .unwrap();
    let (status, headers, _) = call(&live.app, req).await;
    assert_eq!(status, 200);
    assert_eq!(
        headers
            .get("access-control-allow-origin")
            .and_then(|v| v.to_str().ok()),
        Some("*")
    );

    // Behind: the node announces 12 blocks it does not serve yet.
    node.with_chain(|c| {
        c.serve_up_to = Some(3);
        for _ in 0..12 {
            c.push_block(vec![]);
        }
    });
    let ch = live
        .wait_for("/api/v1/chain", WAIT, |c| c["state"] == "behind")
        .await;
    assert_eq!(ch["lag"], 12, "{ch}");
    assert_eq!(ch["node_height"], 15);
    assert_eq!(ch["head_height"], 3);

    // Caught up again.
    node.with_chain(|c| c.serve_up_to = None);
    live.wait_for("/api/v1/chain", WAIT, |c| {
        c["state"] == "live" && c["head_height"] == 15
    })
    .await;

    // Unreachable: the node stops answering while the indexed head is still fresh.
    node.with_chain(|c| c.fail.push(("rand_getHead".into(), None)));
    let ch = live
        .wait_for("/api/v1/chain", WAIT, |c| c["state"] == "unreachable")
        .await;
    assert_eq!(ch["node_connected"], false);
    assert_eq!(ch["head_height"], 15, "the last good height stays: {ch}");

    // Stalled: the same view once the head is older than STALL_MS (aged in the database rather
    // than by waiting a minute).
    sqlx::query("UPDATE blocks SET timestamp_ms = timestamp_ms - 120000")
        .execute(&live.pool)
        .await
        .unwrap();
    let ch = get(&live, "/api/v1/chain").await;
    assert_eq!(ch["state"], "stalled", "{ch}");
    let health = get(&live, "/api/v1/health").await;
    assert_eq!(health["stalled"], true);
    assert_eq!(health["status"], "degraded");
}

#[tokio::test]
async fn a_genesis_switch_stops_the_indexer_until_the_node_comes_back() {
    let _g = LOCK.lock().await;
    let mut chain = MockChain::new(41, "switch-a");
    chain.push_block(vec![tx(
        41,
        "w0",
        Some(bundle("wa0")),
        json!({ "kind": "none" }),
    )]);
    chain.go_live();
    let genesis_a = h("block-41-switch-a-0");
    let node = start_mock_node(chain).await;
    let pin = ChainPin::new(41, &genesis_a);
    let Some(live) = live_app_pinned(test_config(), &node.url, "127.0.0.1", pin).await else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    fresh(&live).await;
    live.start();
    live.wait_for("/api/v1/chain", WAIT, |c| {
        c["head_height"] == 1 && c["state"] == "live"
    })
    .await;

    // The node is re-pointed at another genesis with the same chain id and a longer chain.
    let original = node.with_chain(|c| {
        let mut other = MockChain::new(41, "switch-b");
        for _ in 0..5 {
            other.push_block(vec![]);
        }
        other.go_live();
        std::mem::replace(c, other)
    });
    let ch = live
        .wait_for("/api/v1/chain", WAIT, |c| c["halted"] == true)
        .await;
    assert_eq!(ch["state"], "wrong_chain");
    assert_eq!(ch["genesis_hash"], genesis_a);
    assert_eq!(ch["node_genesis_hash"], h("block-41-switch-b-0"));
    let health = get(&live, "/api/v1/health").await;
    assert_eq!(health["wrong_chain"], true);
    assert_eq!(health["status"], "degraded");

    tokio::time::sleep(Duration::from_secs(1)).await;
    let (count, max): (i64, Option<i64>) =
        sqlx::query_as("SELECT COUNT(*), MAX(height) FROM blocks")
            .fetch_one(&live.pool)
            .await
            .unwrap();
    assert_eq!((count, max), (2, Some(1)), "no block of the other genesis");
    let b0 = get(&live, "/api/v1/blocks/0").await;
    assert_eq!(b0["hash"], genesis_a);

    // Back on the stored chain, with a new block: indexing resumes where it stopped.
    node.with_chain(|c| {
        *c = original;
        c.push_block(vec![]);
    });
    let ch = live
        .wait_for("/api/v1/chain", WAIT, |c| {
            c["halted"] == false && c["head_height"] == 2
        })
        .await;
    assert_eq!(ch["state"], "live", "{ch}");
    assert_eq!(ch["genesis_hash"], genesis_a);
}
