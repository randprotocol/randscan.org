//! The indexer against a node that misbehaves or changes under it, scripted with `MockChain`:
//! the validator-set and rotation actions, a program or receipt the node cannot give, the
//! optional status reads failing while the stats carry on, and a reorg that makes the indexer
//! rewind to the fork point. Live indexer, needs `DATABASE_URL`. One lock: each test resets the
//! shared database to its own chain.

mod common;

use axum::http::StatusCode;
use common::mock_node::*;
use common::*;
use serde_json::{json, Value};
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(40);

static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn get(live: &LiveApp, path: &str) -> (StatusCode, Value) {
    let (status, _, body) = call(&live.app, json_req("GET", path, None, None)).await;
    (status, body)
}

#[tokio::test]
async fn governance_actions_and_unreadable_extras_are_indexed_anyway() {
    let _g = LOCK.lock().await;
    let mut chain = MockChain::new(32, "gov");
    let p_null = h("program-null");
    let p_fail = h("program-fail");
    let deploy_null = tx(
        32,
        "d1",
        Some(bundle("g1")),
        json!({ "kind": "deploy", "program": p_null, "words": 10, "public_words_len": 0 }),
    );
    let deploy_fail = tx(
        32,
        "d2",
        Some(bundle("g2")),
        json!({ "kind": "deploy", "program": p_fail, "words": 11, "public_words_len": 0 }),
    );
    let call_null = tx(
        32,
        "c1",
        Some(bundle("g3")),
        json!({ "kind": "call", "program": p_null, "proof_len": 5, "input_envelope_len": 1 }),
    );
    let call_fail = tx(
        32,
        "c2",
        Some(bundle("g4")),
        json!({ "kind": "call", "program": p_null, "proof_len": 6, "input_envelope_len": 1 }),
    );
    chain.push_block(vec![
        deploy_null.clone(),
        deploy_fail.clone(),
        call_null.clone(),
        call_fail.clone(),
    ]);
    // The node does not know the first program or the first call's receipt, and errors on the
    // second's; the explorer stores the transactions regardless.
    chain
        .null
        .push(("rand_getProgram".into(), Some(p_null.clone())));
    chain
        .fail
        .push(("rand_getProgram".into(), Some(p_fail.clone())));
    chain.null.push((
        "rand_getReceipt".into(),
        Some(call_null["hash"].as_str().unwrap().into()),
    ));
    chain.fail.push((
        "rand_getReceipt".into(),
        Some(call_fail["hash"].as_str().unwrap().into()),
    ));
    // The optional reads all fail; the stats must still be served.
    for m in [
        "rand_getEpoch",
        "rand_getLimits",
        "rand_getVersion",
        "rand_getGenesisHash",
        "rand_getSupply",
        "rand_getTokens",
        "rand_getBridgeState",
    ] {
        chain.fail.push((m.into(), None));
    }

    let candidate = "ByDkxsEfDCR5DrmDufKftvcRsgvufypnZ4SgDQzJAQ7Z";
    let admit = tx(
        32,
        "admit",
        None,
        json!({ "kind": "admit_validator", "candidate": candidate, "candidate_key": "ab".repeat(8), "voters": [VALIDATOR] }),
    );
    let slash = tx(
        32,
        "slash",
        None,
        json!({ "kind": "slash_equivocation", "offender": candidate, "view": 12,
                "first": { "hash": h("hdr-1"), "height": 6 }, "second": { "hash": h("hdr-2"), "height": 6 } }),
    );
    let pq_v1 = tx(
        32,
        "pq1",
        None,
        json!({ "kind": "rotate_pq_guardians", "new_pq_guardians": ["aa", "bb"], "nonce": 3, "pq_signers": [0, 1] }),
    );
    let pq_v2 = tx(
        32,
        "pq2",
        None,
        json!({ "kind": "rotate_pq_guardians_v2", "new_pq_guardians": ["cc"], "possession_signatures": 2, "nonce": 4, "pq_signers": [1] }),
    );
    let pause_v1 = tx(
        32,
        "pk1",
        None,
        json!({ "kind": "rotate_pause_key", "new_pause_key": "dd".repeat(4), "nonce": 5, "pq_signers": [0] }),
    );
    let pause_v2 = tx(
        32,
        "pk2",
        None,
        json!({ "kind": "rotate_pause_key_v2", "new_pause_key": "ee".repeat(4), "nonce": 6, "pq_signers": [0, 1] }),
    );
    let cancel = tx(
        32,
        "cancel",
        None,
        json!({ "kind": "cancel_rotation", "rotation_kind": "pause_key", "nonce": 6 }),
    );
    chain.push_block(vec![
        admit.clone(),
        slash.clone(),
        pq_v1.clone(),
        pq_v2.clone(),
        pause_v1.clone(),
        pause_v2.clone(),
        cancel.clone(),
    ]);
    let node = start_mock_node(chain).await;

    let Some(live) = live_app(test_config(), &node.url).await else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    live.start();
    live.wait_for("/api/v1/health", WAIT, |b| {
        b["indexer"]["current_height"] == 2
    })
    .await;

    let detail = |t: &Value| format!("/api/v1/transactions/{}", t["hash"].as_str().unwrap());

    let (s, d) = get(&live, &detail(&admit)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(d["kind"], "admit_validator");
    assert_eq!(d["validator"], candidate);
    assert_eq!(d["staking_action"]["kind"], "admit_validator");
    assert_eq!(d["staking_action"]["voters"], json!([VALIDATOR]));

    let (_, d) = get(&live, &detail(&slash)).await;
    assert_eq!(d["kind"], "slash_equivocation");
    assert_eq!(d["validator"], candidate);
    assert_eq!(d["staking_action"]["view"], 12);
    assert_eq!(d["staking_action"]["first"]["hash"], h("hdr-1"));
    assert_eq!(d["staking_action"]["second"]["height"], 6);

    let (_, d) = get(&live, &detail(&pq_v1)).await;
    assert_eq!(d["kind"], "rotate_pq_guardians");
    assert_eq!(d["bridge_governance"]["nonce"], 3);
    assert_eq!(d["bridge_governance"]["pq_signers"], json!([0, 1]));
    assert_eq!(
        d["bridge_governance"]["new_pq_guardians"],
        json!(["aa", "bb"])
    );
    let (_, d) = get(&live, &detail(&pq_v2)).await;
    assert_eq!(d["kind"], "rotate_pq_guardians_v2");
    assert_eq!(d["bridge_governance"]["possession_signatures"], 2);
    assert_eq!(d["bridge_governance"]["nonce"], 4);
    let (_, d) = get(&live, &detail(&pause_v1)).await;
    assert_eq!(d["kind"], "rotate_pause_key");
    assert_eq!(d["bridge_governance"]["new_pause_key"], "dd".repeat(4));
    let (_, d) = get(&live, &detail(&pause_v2)).await;
    assert_eq!(d["kind"], "rotate_pause_key_v2");
    assert_eq!(d["bridge_governance"]["pq_signers"], json!([0, 1]));
    let (_, d) = get(&live, &detail(&cancel)).await;
    assert_eq!(d["kind"], "cancel_rotation");
    assert_eq!(d["bridge_governance"]["nonce"], 6);

    // A program the node cannot describe is kept under its own id as its code hash.
    for (p, words) in [(&p_null, 10), (&p_fail, 11)] {
        let (s, d) = get(&live, &format!("/api/v1/programs/{p}")).await;
        assert_eq!(s, StatusCode::OK, "{p}");
        assert_eq!(d["code_hash"], *p);
        assert_eq!(d["base_pc"], 0);
        assert_eq!(d["words_len"], words);
    }
    // The calls are stored without a receipt.
    for t in [&call_null, &call_fail] {
        let (s, d) = get(&live, &detail(t)).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(d["kind"], "call");
        assert!(d["receipt"].is_null(), "{d}");
    }

    // The stats carry on without what the node would not answer.
    let stats = live
        .wait_for("/api/v1/stats", WAIT, |s| s["height"] == 2)
        .await;
    assert_eq!(stats["chain_id"], 32);
    assert!(stats["limits"].is_null());
    assert!(stats["node_version"].is_null());
    assert!(stats["genesis_hash"].is_null());
    assert!(stats["epoch"].is_null());
    assert_eq!(stats["total_transactions"], 11);
}

#[tokio::test]
async fn a_reorg_rewinds_the_indexer_to_the_fork_point() {
    let _g = LOCK.lock().await;
    let mut chain = MockChain::new(33, "reorg-a");
    for i in 0..4 {
        chain.push_block(vec![tx(
            33,
            &format!("a{i}"),
            Some(bundle(&format!("ra{i}"))),
            json!({ "kind": "none" }),
        )]);
    }
    let old_hash = chain.blocks[3]["hash"].as_str().unwrap().to_string();
    let node = start_mock_node(chain).await;
    let Some(live) = live_app(test_config(), &node.url).await else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    live.start();
    live.wait_for("/api/v1/health", WAIT, |b| {
        b["indexer"]["current_height"] == 4
    })
    .await;
    let (_, b3) = get(&live, "/api/v1/blocks/3").await;
    assert_eq!(b3["hash"], old_hash);

    // The node replaces blocks 3 and 4 with others and extends the chain.
    let new_hash = node.with_chain(|c| {
        c.blocks.truncate(3);
        c.salt = "reorg-b".into();
        c.push_block(vec![tx(
            33,
            "b3",
            Some(bundle("rb3")),
            json!({ "kind": "none" }),
        )]);
        c.push_block(vec![]);
        c.push_block(vec![tx(
            33,
            "b5",
            Some(bundle("rb5")),
            json!({ "kind": "none" }),
        )]);
        c.blocks[3]["hash"].as_str().unwrap().to_string()
    });
    assert_ne!(new_hash, old_hash);
    live.wait_for("/api/v1/blocks/5", WAIT, |b| b["height"] == 5)
        .await;

    let (_, b3) = get(&live, "/api/v1/blocks/3").await;
    assert_eq!(b3["hash"], new_hash, "the orphaned block is replaced");
    let (_, list) = get(&live, "/api/v1/blocks?limit=10").await;
    assert_eq!(
        list["pagination"]["total"], 6,
        "no duplicate or missing heights"
    );
    let (_, orphan) = get(&live, &format!("/api/v1/blocks/{old_hash}")).await;
    assert!(
        orphan.get("height").is_none(),
        "the old hash is gone: {orphan}"
    );
    let (_, txs) = get(&live, "/api/v1/transactions?height=3").await;
    assert_eq!(txs["pagination"]["total"], 1);
    // The orphaned block's nullifiers were released with it; the new chain's are stored.
    let (s, _) = get(&live, &format!("/api/v1/nullifiers/{}", h("nf1-ra3"))).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = get(&live, &format!("/api/v1/nullifiers/{}", h("nf1-rb3"))).await;
    assert_eq!(s, StatusCode::OK);
}
