//! Fullnode v0.8.0's client-facing changes, through the live indexer and the API, against a
//! scripted node serving the node's own shapes (`tx_json`, `rand_getLimits`, `rand_getSupply`,
//! `rand_getPerps`, `rand_getRandPrice`, `rand_getMultisig` at fullnode 37c4fdd3, `docs/rpc.md`):
//! chain 21's cut, which carries `fees`, `perps` and `multisig`, and chain 20 after the flag-day
//! roll, where the same build answers `null`, `{"enabled": false}` and `"0"`. The perps and
//! price kinds cannot be produced by a cheap real node (a perps genesis needs the engine's
//! genesis root and a state proof a real prover run), so they are covered here; the real-node
//! suite covers a multisig create and payment. Needs `DATABASE_URL`.

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

fn detail(t: &Value) -> String {
    format!("/api/v1/transactions/{}", t["hash"].as_str().unwrap())
}

#[tokio::test]
async fn chain_21s_kinds_limits_supply_and_reads() {
    let _g = LOCK.lock().await;
    let mut chain = MockChain::new(21, "v080");
    chain.v080 = Some(true);
    let acct = h("perp-account");
    let w = |b: &str| b.repeat(32);
    let deposit = tx(
        21,
        "pd",
        Some(bundle("pd")),
        json!({ "kind": "perp_deposit", "trading_key": "ab".repeat(40), "account": acct }),
    );
    let order = tx(
        21,
        "po",
        None,
        json!({ "kind": "perp_order", "account": acct,
                "body": { "nonce": 3, "market": 0, "side": 1, "kind": 0, "tif": 2, "reduce_only": false,
                          "price": "6425000000000", "size": "100000" } }),
    );
    let cancel = tx(
        21,
        "pc",
        None,
        json!({ "kind": "perp_cancel", "account": acct, "nonce": 4, "target": 3 }),
    );
    let withdraw = tx(
        21,
        "pw",
        None,
        json!({ "kind": "perp_withdraw", "account": acct, "nonce": 5, "amount": "2000000",
                "recipient": shielded_address("perp-payee"), "time": 1 }),
    );
    let oracle = tx(
        21,
        "pr",
        None,
        json!({ "kind": "perp_oracle", "validator": VALIDATOR,
                "prices": [{ "market": 0, "price": "6425000000000" }], "nonce": 9 }),
    );
    let proof = tx(
        21,
        "ps",
        None,
        json!({ "kind": "perp_state_proof", "from_height": 0, "to_height": 1, "new_root": h("perp-root"),
                "payouts": [{ "request": h("req"), "amount": "2000000" }], "fees": "1500", "proof_bytes": 412345 }),
    );
    let create = tx(
        21,
        "mc",
        Some(bundle("mc")),
        json!({ "kind": "create_multisig", "salt": w("05"), "threshold": 2, "signer_count": 3 }),
    );
    let ms_deposit = tx(
        21,
        "md",
        Some(bundle("md")),
        json!({ "kind": "multisig_deposit", "account": MOCK_TREASURY }),
    );
    let pay_cm = h("multisig-payout");
    let pay = tx(
        21,
        "mp",
        None,
        json!({ "kind": "multisig_pay", "account": MOCK_TREASURY, "nonce": 0, "time": 1,
                "pays": [{ "asset": 0, "amount": "1000000000", "recipient": shielded_address("ms-payee"),
                           "time": 1, "r": w("07"), "cm": pay_cm }],
                "signers": [0, 2] }),
    );
    let rotate = tx(
        21,
        "mr",
        None,
        json!({ "kind": "multisig_rotate", "account": MOCK_TREASURY, "nonce": 1, "threshold": 2,
                "signer_count": 3, "signed_by": [1, 2] }),
    );
    let price = tx(
        21,
        "sp",
        None,
        json!({ "kind": "set_rand_price", "price_micros_per_rand": "150000", "nonce": 1, "voters": [VALIDATOR] }),
    );
    let aggregate = tx(
        21,
        "ag",
        None,
        json!({ "kind": "aggregate", "covers": 8, "proof_len": 1234567, "aggregator": VALIDATOR,
                "nonce": 0, "time": 1, "payout_total": "300000000", "layout": "tree" }),
    );
    chain.push_block(vec![
        deposit.clone(),
        order.clone(),
        cancel.clone(),
        withdraw.clone(),
        oracle.clone(),
    ]);
    chain.push_block(vec![
        proof.clone(),
        create.clone(),
        ms_deposit.clone(),
        pay.clone(),
        rotate.clone(),
        price.clone(),
        aggregate.clone(),
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

    // Perps: every kind under its own tag, its detail whole.
    let (s, d) = get(&live, &detail(&deposit)).await;
    assert_eq!(s, StatusCode::OK, "{d}");
    assert_eq!(d["kind"], "perp_deposit");
    assert_eq!(d["perp_action"]["account"], acct);
    assert_eq!(d["has_bundle"], true);
    let (_, d) = get(&live, &detail(&order)).await;
    assert_eq!(d["kind"], "perp_order");
    assert_eq!(d["action_nonce"], 3);
    assert_eq!(d["perp_action"]["body"]["side"], 1);
    assert_eq!(d["perp_action"]["body"]["tif"], 2);
    assert_eq!(d["perp_action"]["body"]["price"], "6425000000000");
    assert_eq!(d["has_bundle"], false);
    assert_eq!(d["fee"], "0");
    let (_, d) = get(&live, &detail(&cancel)).await;
    assert_eq!(
        (d["kind"].clone(), d["perp_action"]["target"].clone()),
        (json!("perp_cancel"), json!(3))
    );
    let (_, d) = get(&live, &detail(&withdraw)).await;
    assert_eq!(d["kind"], "perp_withdraw");
    assert_eq!(d["perp_action"]["amount"], "2000000");
    assert!(
        d["amount"].is_null(),
        "collateral units never reach the RAND amount: {d}"
    );
    assert_eq!(d["recipient"], shielded_address("perp-payee"));
    let (_, d) = get(&live, &detail(&oracle)).await;
    assert_eq!(d["kind"], "perp_oracle");
    assert_eq!(d["validator"], VALIDATOR);
    assert_eq!(d["perp_action"]["prices"][0]["price"], "6425000000000");
    let (_, d) = get(&live, &detail(&proof)).await;
    assert_eq!(d["kind"], "perp_state_proof");
    assert_eq!(d["perp_action"]["payouts"][0]["amount"], "2000000");
    assert_eq!(d["perp_action"]["proof_bytes"], 412345);

    // Multisig: the payment's payout leaf links back to it.
    let (_, d) = get(&live, &detail(&create)).await;
    assert_eq!(d["kind"], "create_multisig");
    assert_eq!(d["multisig_action"]["signer_count"], 3);
    let (_, d) = get(&live, &detail(&ms_deposit)).await;
    assert_eq!(d["multisig_action"]["account"], MOCK_TREASURY);
    let (_, d) = get(&live, &detail(&pay)).await;
    assert_eq!(d["kind"], "multisig_pay");
    assert_eq!(d["multisig_action"]["pays"][0]["cm"], pay_cm);
    assert_eq!(d["multisig_action"]["signers"], json!([0, 2]));
    assert_eq!(d["action_nonce"], 0);
    let note = live
        .wait_for(&format!("/api/v1/notes/{pay_cm}"), WAIT, |n| {
            n["tx_hash"].is_string()
        })
        .await;
    assert_eq!(note["tx_hash"], pay["hash"]);
    let (_, d) = get(&live, &detail(&rotate)).await;
    assert_eq!(d["multisig_action"]["signed_by"], json!([1, 2]));

    // The price vote and the aggregate.
    let (_, d) = get(&live, &detail(&price)).await;
    assert_eq!(d["kind"], "set_rand_price");
    assert_eq!(d["staking_action"]["price_micros_per_rand"], "150000");
    assert_eq!(d["staking_action"]["voters"], json!([VALIDATOR]));
    let (_, d) = get(&live, &detail(&aggregate)).await;
    assert_eq!(d["kind"], "aggregate");
    assert_eq!(d["amount"], "300000000");
    assert_eq!(d["aggregate"]["layout"], "tree");
    assert_eq!(d["aggregate"]["payout_total"], "300000000");

    // A kind filter on a new kind.
    let (s, l) = get(&live, "/api/v1/transactions?kind=perp_order").await;
    assert_eq!(s, StatusCode::OK, "{l}");
    assert_eq!(l["pagination"]["total"], 1);
    assert_eq!(l["data"][0]["hash"], order["hash"]);

    // The limits, supply and the new reads.
    let stats = live
        .wait_for("/api/v1/stats", WAIT, |s| s["limits"]["perps"].is_object())
        .await;
    let l = &stats["limits"];
    assert_eq!(l["perps"]["collateral_asset"], 1);
    assert_eq!(l["multisig"]["create_fee"], "1000000000");
    assert_eq!(l["fee_rules"]["burn_base"], true);
    assert_eq!(
        l["fee_rules"]["usd_subsidy"]["usd_micros_per_sealed_block"],
        "4791"
    );
    assert!(l["fee_rules"]["prove_base"].is_null(), "{l}");
    let supply = live
        .wait_for("/api/v1/supply", WAIT, |s| s["multisig_issued"].is_string())
        .await;
    assert_eq!(supply["multisig_issued"], "100000000000000");
    assert_eq!(supply["multisig_rand_held"], "99998999000000");
    assert_eq!(supply["base_fees_burned"], "2000000");
    assert_eq!(supply["perps_rand_held"], "0");
    let perps = live
        .wait_for("/api/v1/perps", WAIT, |p| p["enabled"] == true)
        .await;
    assert_eq!(perps["markets"][0]["symbol"], "BTC-PERP");
    assert_eq!(perps["medians"][0]["price"], "6425000000000");
    let price = live
        .wait_for("/api/v1/price", WAIT, |p| {
            p["price_micros_per_rand"].is_string()
        })
        .await;
    assert_eq!(
        (price["fresh"].clone(), price["nonce"].clone()),
        (json!(true), json!(1))
    );
    let (s, m) = get(&live, &format!("/api/v1/multisig/{MOCK_TREASURY}")).await;
    assert_eq!(s, StatusCode::OK, "{m}");
    assert_eq!(
        (
            m["threshold"].clone(),
            m["signers"].as_array().unwrap().len()
        ),
        (json!(2), 3)
    );
    assert_eq!(m["vault"][0]["amount"], "99998999000000");
    let (s, _) = get(&live, &format!("/api/v1/multisig/{}", "ab".repeat(32))).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = get(&live, "/api/v1/multisig/xyz").await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

/// Chain 20 after the roll: the same build, no gated section. Every new group reads `null`,
/// every counter `"0"`, the new reads 404, and nothing else changes.
#[tokio::test]
async fn chain_20_after_the_roll_has_none_of_it() {
    let _g = LOCK.lock().await;
    let mut chain = MockChain::new(20, "v080-off");
    chain.v080 = Some(false);
    let transfer = tx(20, "t", Some(bundle("t")), json!({ "kind": "none" }));
    chain.push_block(vec![transfer.clone()]);
    let node = start_mock_node(chain).await;
    let Some(live) = live_app(test_config(), &node.url).await else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    live.start();
    live.wait_for("/api/v1/health", WAIT, |b| {
        b["indexer"]["current_height"] == 1
    })
    .await;
    let (_, d) = get(&live, &detail(&transfer)).await;
    assert_eq!(d["kind"], "transfer");
    assert!(
        d["perp_action"].is_null() && d["multisig_action"].is_null() && d["aggregate"].is_null()
    );
    let stats = live
        .wait_for("/api/v1/stats", WAIT, |s| {
            s["chain_id"] == 20 && s["limits"].is_object()
        })
        .await;
    let l = &stats["limits"];
    assert!(
        l["perps"].is_null() && l["multisig"].is_null() && l["fee_rules"].is_null(),
        "{l}"
    );
    let supply = live
        .wait_for("/api/v1/supply", WAIT, |s| s["multisig_issued"].is_string())
        .await;
    assert_eq!(supply["multisig_issued"], "0");
    assert_eq!(supply["perps_rand_out"], "0");
    for path in ["/api/v1/perps", "/api/v1/price"] {
        let (s, b) = get(&live, path).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{path}: {b}");
    }
    let (s, _) = get(&live, &format!("/api/v1/multisig/{MOCK_TREASURY}")).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}
