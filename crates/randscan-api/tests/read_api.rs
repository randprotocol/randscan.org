//! The read-only endpoints the one big indexer scenario in `mock_node.rs` leaves alone: block
//! listing and lookup, the latest-transaction feed, search by every id shape, the validator
//! register and its detail page, the peer map and the prover list, the program pages' refusals.
//! A live indexer follows a scripted node (chain 31); needs `DATABASE_URL`.

mod common;

use axum::http::StatusCode;
use common::mock_node::*;
use common::*;
use randscan_core::KNOWN_PROVERS;
use randscan_db as db;
use serde_json::{json, Value};
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(40);

async fn get(app: &axum::Router, path: &str) -> (StatusCode, Value) {
    let (status, _, body) = call(app, json_req("GET", path, None, None)).await;
    (status, body)
}

fn search_ids(found: &Value) -> Vec<(String, String)> {
    found
        .as_array()
        .expect("search answers an array")
        .iter()
        .map(|r| {
            (
                r["type"].as_str().unwrap().to_string(),
                r["id"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[tokio::test]
async fn read_endpoints_serve_what_the_indexer_stored() {
    let mut chain = MockChain::new(31, "read-a");
    let transfer = tx(
        31,
        "transfer",
        Some(bundle("rt")),
        json!({ "kind": "none" }),
    );
    let mint = tx(
        31,
        "mint",
        None,
        json!({ "kind": "mint", "cm": h("cm-read-mint"), "amount": 5_000_000_000u64, "minter": VALIDATOR }),
    );
    let b1 = chain.push_block(vec![transfer.clone(), mint.clone()]);
    let program = h("program-read");
    let deploy = tx(
        31,
        "deploy",
        Some(bundle("rd")),
        json!({ "kind": "deploy", "program": program, "words": 64, "public_words_len": 0 }),
    );
    let call_tx = tx(
        31,
        "call",
        Some(bundle("rc")),
        json!({ "kind": "call", "program": program, "proof_len": 1000, "input_envelope_len": 8 }),
    );
    let b2 = chain.push_block(vec![deploy.clone(), call_tx.clone()]);
    let b3 = chain.push_block(vec![]);
    // A second, inactive validator on the register, so both search titles are exercised.
    let mut inactive = MockValidator::new(VALIDATOR_B, "7000000000");
    inactive.active = false;
    chain.validators.push(inactive);
    // Peers: a public one, a private one (placed with this node), one without an address.
    chain.peers = vec![
        json!({ "peer_id": "12D3KooWpublicpeer", "connected_secs": 42,
                "addrs": ["/ip4/10.1.2.3/tcp/30303", "/ip4/203.0.113.9/tcp/30304"] }),
        json!({ "peer_id": "12D3KooWprivatepeer", "connected_secs": 7,
                "addrs": ["/ip4/192.168.1.20/tcp/30303"] }),
        json!({ "peer_id": "12D3KooWaddrless", "connected_secs": 1, "addrs": [] }),
    ];
    let node = start_mock_node(chain).await;

    let Some(live) = live_app_with_ip(test_config(), &node.url, "203.0.113.5").await else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    // Fresh geolocation rows for the public addresses, so nothing asks ipwho.is.
    let geo = |ip: &'static str, city: &'static str, lat: f64| {
        db::upsert_node_geo(
            &live.pool,
            ip,
            true,
            Some(lat),
            Some(10.0),
            Some(city),
            Some("Region"),
            Some("Country"),
            Some("CC"),
            Some("Example Org"),
        )
    };
    geo("203.0.113.5", "Selfville", 50.0).await.unwrap();
    geo("203.0.113.9", "Peerton", 51.0).await.unwrap();
    for p in KNOWN_PROVERS {
        for m in p.members {
            geo(m.ip, "Proverburg", 52.0).await.unwrap();
        }
    }
    live.start();
    live.wait_for("/api/v1/health", WAIT, |b| {
        b["indexer"]["current_height"] == 3 && b["indexer"]["synced"] == true
    })
    .await;

    // ----- blocks ---------------------------------------------------------------------------
    let (status, page) = get(&live.app, "/api/v1/blocks?limit=2").await;
    assert_eq!(status, StatusCode::OK);
    let heights: Vec<u64> = page["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["height"].as_u64().unwrap())
        .collect();
    assert_eq!(heights, vec![3, 2], "newest first");
    assert_eq!(page["pagination"]["total"], 4);
    assert_eq!(page["pagination"]["has_next"], true);
    assert_eq!(page["pagination"]["has_prev"], false);
    let (_, page2) = get(&live.app, "/api/v1/blocks?limit=2&page=2").await;
    let heights: Vec<u64> = page2["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["height"].as_u64().unwrap())
        .collect();
    assert_eq!(heights, vec![1, 0]);
    assert_eq!(page2["pagination"]["has_prev"], true);
    assert_eq!(page2["pagination"]["has_next"], false);

    let (_, by_proposer) = get(&live.app, &format!("/api/v1/blocks?proposer={VALIDATOR}")).await;
    assert_eq!(by_proposer["pagination"]["total"], 4);
    let (_, nobody) = get(&live.app, "/api/v1/blocks?proposer=NobodyAtAll").await;
    assert_eq!(nobody["pagination"]["total"], 0);
    assert!(nobody["data"].as_array().unwrap().is_empty());

    let (_, latest) = get(&live.app, "/api/v1/blocks/latest?limit=3").await;
    let latest = latest.as_array().unwrap();
    assert_eq!(latest.len(), 3);
    assert_eq!(latest[0]["hash"], b3);
    assert_eq!(latest[2]["height"], 1);

    let (status, by_hash) = get(&live.app, &format!("/api/v1/blocks/{b2}")).await;
    assert_eq!(status, StatusCode::OK);
    let (_, by_height) = get(&live.app, "/api/v1/blocks/2").await;
    assert_eq!(by_hash, by_height, "a block by hash is a block by height");
    assert_eq!(by_hash["height"], 2);
    assert_eq!(by_hash["tx_count"], 2);
    let kinds: Vec<&str> = by_hash["transactions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, vec!["deploy", "call"]);
    assert_eq!(by_hash["tx_root"], h("txroot-2"));
    let (_, b1_detail) = get(&live.app, &format!("/api/v1/blocks/{}", b1.to_uppercase())).await;
    assert_eq!(
        b1_detail["height"], 1,
        "hashes are matched case-insensitively"
    );

    let (status, _) = get(&live.app, "/api/v1/blocks/not-a-block").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = get(&live.app, "/api/v1/blocks/99999").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = get(&live.app, &format!("/api/v1/blocks/{}", h("no-such-block"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // ----- transaction feed -----------------------------------------------------------------
    let (_, feed) = get(&live.app, "/api/v1/transactions/latest?limit=2").await;
    let feed = feed.as_array().unwrap();
    assert_eq!(feed.len(), 2);
    assert_eq!(
        feed[0]["height"], 2,
        "the newest block's transactions first"
    );
    let (status, _) = get(&live.app, "/api/v1/transactions/xyz").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = get(&live.app, &format!("/api/v1/transactions/{}", h("no-tx"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = get(&live.app, "/api/v1/transactions?program=zz").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (_, by_height) = get(&live.app, "/api/v1/transactions?height=2").await;
    assert_eq!(by_height["pagination"]["total"], 2);
    let (_, all_kinds) = get(&live.app, "/api/v1/transactions?kind=all").await;
    assert_eq!(all_kinds["pagination"]["total"], 4);

    // ----- search ---------------------------------------------------------------------------
    let (_, found) = get(&live.app, "/api/v1/search?q=2").await;
    assert_eq!(search_ids(&found), vec![("block".into(), "2".into())]);
    assert_eq!(found[0]["title"], "Block #2");
    assert_eq!(found[0]["subtitle"], "2 transactions");
    assert_eq!(found[0]["url"], "/blocks/2");
    let (_, found) = get(&live.app, "/api/v1/search?q=424242").await;
    assert!(search_ids(&found).is_empty());

    let (_, found) = get(&live.app, &format!("/api/v1/search?q={b3}")).await;
    assert_eq!(search_ids(&found), vec![("block".into(), b3.clone())]);
    assert_eq!(found[0]["url"], "/blocks/3");

    let deploy_hash = deploy["hash"].as_str().unwrap();
    let (_, found) = get(&live.app, &format!("/api/v1/search?q={deploy_hash}")).await;
    assert_eq!(
        search_ids(&found),
        vec![("transaction".into(), deploy_hash.into())]
    );
    assert_eq!(found[0]["title"], "deploy transaction");
    assert_eq!(found[0]["subtitle"], "block #2");

    let (_, found) = get(&live.app, &format!("/api/v1/search?q={program}")).await;
    assert_eq!(
        search_ids(&found),
        vec![("program".into(), program.clone())]
    );
    assert_eq!(found[0]["subtitle"], "deployed at #2, 1 calls");
    assert_eq!(found[0]["url"], format!("/programs/{program}"));

    // A commitment of a bundle the indexer has stored; whether its leaf page was fetched yet
    // decides which of the two note results answers, both are a "note".
    let (_, found) = get(&live.app, &format!("/api/v1/search?q={}", h("cm1-rt"))).await;
    let ids = search_ids(&found);
    assert_eq!(ids.len(), 1, "{found}");
    assert_eq!(ids[0], ("note".into(), h("cm1-rt")));
    let (_, found) = get(&live.app, &format!("/api/v1/search?q={}", h("nf3-rt"))).await;
    assert_eq!(search_ids(&found), vec![("nullifier".into(), h("nf3-rt"))]);
    assert_eq!(found[0]["subtitle"], "note spent at #1");
    assert_eq!(
        found[0]["url"],
        format!("/transactions/{}", transfer["hash"].as_str().unwrap())
    );

    let (_, found) = get(&live.app, &format!("/api/v1/search?q={VALIDATOR}")).await;
    assert_eq!(found[0]["title"], "Validator");
    assert_eq!(found[0]["url"], format!("/validators/{VALIDATOR}"));
    let (_, found) = get(&live.app, &format!("/api/v1/search?q={VALIDATOR_B}")).await;
    assert_eq!(found[0]["title"], "Validator (inactive)");
    assert_eq!(found[0]["subtitle"], "stake 7000000000");
    let (_, found) = get(&live.app, "/api/v1/search?q=not%20anything%20at%20all").await;
    assert!(search_ids(&found).is_empty());

    // ----- validators -----------------------------------------------------------------------
    let (status, vs) = get(&live.app, "/api/v1/validators").await;
    assert_eq!(status, StatusCode::OK);
    let vs = vs.as_array().unwrap();
    assert_eq!(vs.len(), 2);
    let a = vs.iter().find(|v| v["address"] == VALIDATOR).unwrap();
    let b = vs.iter().find(|v| v["address"] == VALIDATOR_B).unwrap();
    assert_eq!(a["active"], true);
    assert_eq!(b["active"], false);
    let (status, detail) = get(&live.app, &format!("/api/v1/validators/{VALIDATOR}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["address"], VALIDATOR);
    let recent = detail["recent_blocks"].as_array().unwrap();
    assert_eq!(
        recent.len(),
        4,
        "it proposed every block of the scripted chain"
    );
    assert_eq!(recent[0]["height"], 3);
    let (_, detail_b) = get(&live.app, &format!("/api/v1/validators/{VALIDATOR_B}")).await;
    assert!(detail_b["recent_blocks"].as_array().unwrap().is_empty());
    let (status, _) = get(&live.app, "/api/v1/validators/NoSuchValidator").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // The mock predates `rand_getAdmitted`: the empty, vote-less set.
    let (status, admitted) = get(&live.app, "/api/v1/validators/admitted").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(admitted["admission_by_vote"], false);
    assert!(admitted["admitted"].as_array().unwrap().is_empty());

    // ----- programs -------------------------------------------------------------------------
    let (_, programs) = get(&live.app, "/api/v1/programs").await;
    assert_eq!(programs["pagination"]["total"], 1);
    assert_eq!(programs["data"][0]["id"], program);
    let (status, _) = get(&live.app, "/api/v1/programs/short").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = get(&live.app, &format!("/api/v1/programs/{}", h("other"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = get(&live.app, "/api/v1/programs/short/cells").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    // No `program_state` section on this chain: a 404, not a server error.
    let (status, _) = get(&live.app, &format!("/api/v1/programs/{program}/cells")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, detail) = get(&live.app, &format!("/api/v1/programs/{program}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(detail["program_state"].is_null());
    assert_eq!(detail["recent_calls"].as_array().unwrap().len(), 1);

    // ----- the peer map ---------------------------------------------------------------------
    let nodes = live
        .wait_for("/api/v1/nodes", WAIT, |n| {
            n.as_array().is_some_and(|a| a.len() == 4)
        })
        .await;
    let nodes = nodes.as_array().unwrap();
    let me = &nodes[0];
    assert_eq!(me["is_self"], true);
    assert_eq!(me["role"], "observer");
    assert_eq!(me["ip"], "203.0.113.5");
    assert_eq!(me["geo"]["city"], "Selfville");
    let public = nodes
        .iter()
        .find(|n| n["peer_id"] == "12D3KooWpublicpeer")
        .unwrap();
    assert_eq!(
        public["ip"], "203.0.113.9",
        "a public address beats a private one"
    );
    assert_eq!(public["port"], 30304);
    assert_eq!(public["connected_secs"], 42);
    assert_eq!(public["role"], "peer");
    assert_eq!(public["geo"]["city"], "Peerton");
    let private = nodes
        .iter()
        .find(|n| n["peer_id"] == "12D3KooWprivatepeer")
        .unwrap();
    assert_eq!(private["ip"], "192.168.1.20");
    assert_eq!(private["geo"]["org"], "private network of this node");
    assert_eq!(private["geo"]["city"], "Selfville");
    let addrless = nodes
        .iter()
        .find(|n| n["peer_id"] == "12D3KooWaddrless")
        .unwrap();
    assert!(addrless["ip"].is_null() && addrless["geo"].is_null());

    // ----- provers: every known one is listed, polled or not ---------------------------------
    let (status, provers) = get(&live.app, "/api/v1/provers").await;
    assert_eq!(status, StatusCode::OK);
    let provers = provers.as_array().unwrap();
    assert_eq!(provers.len(), KNOWN_PROVERS.len());
    for (view, known) in provers.iter().zip(KNOWN_PROVERS) {
        assert_eq!(view["url"], known.url);
        assert_eq!(view["fingerprint"], known.fingerprint);
    }
}
