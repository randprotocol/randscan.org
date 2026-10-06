//! Indexer, database, API and broadcast fan-out working together against a scripted shielded
//! node: every action kind of phases S1–S3, an unknown future kind, a block at the consensus
//! transaction limit, the commitment tree and nullifier set, the S2 register/epoch/supply and
//! the S3-only register shape, and two hard forks under a running indexer (new chain id; same
//! chain id with a new genesis). Needs `DATABASE_URL`.

mod common;

use common::mock_node::*;
use common::*;
use randscan_core::{BroadcastEvent, TxKind};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(40);

#[tokio::test]
async fn indexes_every_shielded_kind_and_survives_hard_forks() {
    // ----- phase 1: chain 7 with every action the node can serve ---------------------------
    let mut chain = MockChain::new(7, "genesis-a");
    let transfer = tx(7, "transfer", Some(bundle("t")), json!({ "kind": "none" }));
    let mint = tx(7, "mint", None, json!({ "kind": "mint", "cm": h("cm-mint"), "amount": 100000000000u64, "minter": VALIDATOR }));
    chain.push_block(vec![transfer.clone(), mint.clone()]);

    let program = h("program-1");
    let deploy = tx(7, "deploy", Some(bundle("d")), json!({ "kind": "deploy", "program": program, "words": 412, "public_words_len": 27151 }));
    let call_tx = tx(7, "call", Some(bundle("c")), json!({ "kind": "call", "program": program, "proof_len": 1202416, "input_envelope_len": 1280 }));
    let bond = tx(7, "bond", Some(bundle("b")), json!({ "kind": "bond", "validator": VALIDATOR_B, "amount": 1000000000000u64, "registered": true }));
    let unbond = tx(7, "unbond", None, json!({ "kind": "unbond", "validator": VALIDATOR_B, "amount": 5000000000u64, "nonce": 1 }));
    let withdraw = tx(7, "withdraw", None, json!({ "kind": "withdraw", "validator": VALIDATOR, "amount": "9000000", "nonce": 3, "time": 1994 }));
    chain.push_block(vec![deploy.clone(), call_tx.clone(), bond.clone(), unbond.clone(), withdraw.clone()]);

    let attest = tx(7, "attest", Some(bundle("a")), json!({
        "kind": "bridge_attest", "attestation_len": 520, "recipient": SHIELDED_ADDR, "asset": 1, "asset_index": 1,
        "amount": 1000, "time": 2, "r": "aa".repeat(32), "commitment": h("deposit-attest"), "pq_signers": [0, 1]
    }));
    let rotation = tx(7, "rotation", Some(bundle("r")), json!({
        "kind": "bridge_attest", "attestation_len": 700, "recipient": SHIELDED_ADDR, "asset": null, "asset_index": null,
        "amount": null, "time": 2, "r": "00".repeat(32), "commitment": null, "pq_signers": [0, 1]
    }));
    let evm_to = format!("{}{}", "0".repeat(24), "f10befe1e0794722d3baf8bfd5bdac47b2a33148");
    let burn = tx(7, "burn", Some(bundle("bb")), json!({
        "kind": "bridge_burn", "asset": 1, "amount": 400, "relayer_fee": 100, "to_chain": 2, "token": "cc".repeat(32), "to": evm_to
    }));
    let future = tx(7, "future", Some(bundle("f")), json!({ "kind": "slash", "evidence": "opaque" }));
    // RPL-2: an invoke of the deployed program — the shape of the node's own pinned test
    // (`rpc.rs`, "invoke"): one cell read as absent and written, 1 000 RAND units and 500 of
    // token 3 deposited through the bundle, 300 units of RAND paid out of the vault and 40 of
    // token 3 minted, each a chain-computed note whose `cm` the node renders.
    chain.program_state = true;
    let cell_key = format!("01{}", "00".repeat(31));
    let mut invoke_bundle = bundle("iv");
    invoke_bundle["burn_r"] = json!(1000);
    invoke_bundle["burn_a"] = json!("500");
    invoke_bundle["burn_asset"] = json!(3);
    let invoke_recipient = shielded_address("invoke-payee");
    let invoke = tx(7, "invoke", Some(invoke_bundle), json!({
        "kind": "invoke", "program": program, "proof_len": 268123, "input_envelope_len": null,
        "transition": {
            "reads": [{ "key": cell_key, "value": "00".repeat(32) }],
            "writes": [{ "key": cell_key, "value": format!("05{}", "00".repeat(31)) }],
            "inflow": "deposit",
            "pays": [{ "asset": 0, "amount": "300", "recipient": invoke_recipient, "time": 2, "r": "a1".repeat(32), "cm": h("payout-0") }],
            "mints": [{ "asset": 3, "amount": 40, "recipient": invoke_recipient, "time": 2, "r": "a2".repeat(32), "cm": h("payout-1") }],
        }
    }));

    // The RPL token standard (spec §4/§6) and bridge hardening's governance actions (B1/B4): a
    // token registered with an initial mint, a later mint, an authority change, a holder burn,
    // the mint pause and its lifting, and a second bridged token listed after genesis.
    // The token RPC's own `backing_json` (crates/randprotocol-node/src/rpc.rs) sends
    // `locked`/`minted_today` as decimal strings but `mint_cap_per_day` as a JSON number — the
    // literal pinned by its test `the_token_listing_serves_every_row_paged`
    // (`"locked": "600", "mint_cap_per_day": 100_000u64 * 100_000_000, "minted_today": "1000"`).
    chain.push_token(json!({
        "index": 3, "id": h("zusd-id"), "id_text": "rpl1zusdexampleexampleexampleexampleexampleexampleexampleeez",
        "name": "zUSD", "symbol": "zUSD", "decimals": 6,
        "authority": { "kind": "bridge", "backings": [{
            "chain": 2, "token": "bb".repeat(32), "decimals": 6, "locked": "600",
            "minted_today": "700", "mint_day": 20345, "mint_cap_per_day": 100_000u64 * 100_000_000,
        }] },
        "mint_nonce": 1, "total_supply": "5700", "registered_at": 3
    }));
    let zusd_register_recipient = shielded_address("zusd-register-recipient");
    let zusd_mint_recipient = shielded_address("zusd-mint-recipient");
    let register_token = tx(7, "register-zusd", Some(bundle("rt")), json!({
        "kind": "register_token", "name": "zUSD", "symbol": "zUSD", "decimals": 6, "authority": "bridge",
        "index": 3, "initial_amount": 5000,
        "initial": { "amount": 5000, "recipient": zusd_register_recipient, "time": 3, "r": "bb".repeat(32) }
    }));
    let token_mint = tx(7, "mint-zusd", Some(bundle("tm")), json!({
        "kind": "token_mint", "asset": 3, "amount": 700, "recipient": zusd_mint_recipient, "time": 3, "r": "cc".repeat(32), "nonce": 0
    }));
    let set_authority = tx(7, "set-auth-zusd", Some(bundle("sa")), json!({
        "kind": "set_authority", "asset": 3, "nonce": 1, "new_authority": VALIDATOR
    }));
    let mut token_burn_bundle = bundle("tb");
    token_burn_bundle["burn_a"] = json!(400);
    token_burn_bundle["burn_asset"] = json!(3);
    let token_burn = tx(7, "burn-zusd", Some(token_burn_bundle), json!({ "kind": "token_burn", "asset": 3, "amount": 400 }));
    let pause = tx(7, "pause", None, json!({ "kind": "pause_mints", "nonce": 4 }));
    let unpause = tx(7, "unpause", None, json!({ "kind": "unpause_mints", "nonce": 5, "pq_signers": [0, 2] }));
    let register_bridged = tx(7, "register-bridged-2", Some(bundle("rb")), json!({
        "kind": "register_bridged_token", "name": "zUSD2", "symbol": "zUSD2", "salt": "ee".repeat(4),
        "chain": 3, "token": "ff".repeat(32), "decimals": 6, "nonce": 6, "asset_id": h("zusd2-asset"), "pq_signers": [1]
    }));
    let list_backing = tx(7, "list-backing", Some(bundle("lb")), json!({
        "kind": "list_backing", "token_index": 3, "chain": 4, "token": "11".repeat(32), "decimals": 6, "nonce": 7, "pq_signers": [2]
    }));

    chain.push_block(vec![
        attest.clone(), rotation.clone(), burn.clone(), future.clone(),
        register_token.clone(), token_mint.clone(), set_authority.clone(), token_burn.clone(),
        pause.clone(), unpause.clone(), register_bridged.clone(), list_backing.clone(), invoke.clone(),
    ]);
    // 2000 transactions per block is a consensus rule; the explorer must take a full block.
    let fat: Vec<Value> = (0..2000)
        .map(|i| tx(7, &format!("fat-{i}"), Some(bundle(&format!("fat-{i}"))), json!({ "kind": "none" })))
        .collect();
    chain.push_block(fat);
    let expected_leaves = chain.leaves.len();
    chain.validators.push(MockValidator {
        address: VALIDATOR_B.into(),
        stake: "995000000000".into(),
        rewards: "0".into(),
        pending: vec![(3, "5000000000".into())],
        active: false,
    });
    chain.validators[0].rewards = "3000000".into();

    let node = start_mock_node(chain).await;
    let Some(live) = live_app(test_config(), &node.url).await else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };

    // A previous run may have left this exact chain behind; start from nothing so every block
    // below is indexed (and broadcast) by this run.
    {
        let mut conn = live.pool.acquire().await.unwrap();
        randscan_db::reset_chain_data(&mut conn, 0).await.unwrap();
    }

    // Collect broadcast events continuously (the channel would drop old ones under 2000 txs).
    let events: Arc<Mutex<Vec<BroadcastEvent>>> = Arc::default();
    {
        let mut rx = live.broadcaster.subscribe();
        let events = events.clone();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(e) => events.lock().unwrap().push(e),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
        });
    }
    live.start();

    live.wait_for("/api/v1/health", WAIT, |b| b["indexer"]["current_height"] == 4).await;

    // A plain transfer: a bundle and nothing else.
    let transfer_hash = transfer["hash"].as_str().unwrap();
    let (status, _, d) = call_api(&live.app, &format!("/api/v1/transactions/{transfer_hash}")).await;
    assert_eq!(status, 200, "{d}");
    assert_eq!(d["kind"], "transfer");
    assert_eq!(d["has_bundle"], true);
    assert_eq!(d["fee"], "1000000");
    // /tx/:hash is an alias of /transactions/:hash, with or without 0x.
    let (status, _, alias) = call_api(&live.app, &format!("/api/v1/tx/0x{transfer_hash}")).await;
    assert_eq!(status, 200, "{alias}");
    assert_eq!(alias, d);
    assert_eq!(d["bundle"]["nullifiers"], json!([h("nf1-t"), h("nf2-t"), h("nf3-t"), h("nf4-t")]));
    assert_eq!(d["bundle"]["commitments"][0], h("cm1-t"));
    assert_eq!(d["bundle"]["commitments"][3], h("cm4-t"));
    assert_eq!(d["bundle"]["proof_len"], 302857);
    assert_eq!(d["bundle"]["envelope_len"], json!([1380, 1380, 1380, 1380]));
    // Chain 17+: split authorisation's commitment and the auth proof, by length.
    assert_eq!(d["bundle"]["auth_commit"], h("auth-t"));
    assert_eq!(d["bundle"]["auth_proof_len"], 1360512);
    assert!(d["bundle"].get("asset").is_none(), "a transfer's asset must never be served: {d}");
    assert!(d["amount"].is_null() && d["validator"].is_null() && d["program"].is_null(), "{d}");
    assert!(d.get("sender").is_none() && d.get("nonce").is_none() && d.get("to").is_none(), "no account fields: {d}");
    assert_eq!(d["chain_id"], 7);

    // A mint: no bundle, a public amount, the minting validator.
    let mint_hash = mint["hash"].as_str().unwrap();
    let (_, _, d) = call_api(&live.app, &format!("/api/v1/transactions/{mint_hash}")).await;
    assert_eq!(d["kind"], "mint");
    assert_eq!(d["has_bundle"], false);
    assert!(d["bundle"].is_null());
    assert_eq!(d["fee"], "0");
    assert_eq!(d["amount"], "100000000000");
    assert_eq!(d["cm"], h("cm-mint"));
    assert_eq!(d["validator"], VALIDATOR);

    let (_, _, d) = call_api(&live.app, &format!("/api/v1/transactions/{}", deploy["hash"].as_str().unwrap())).await;
    assert_eq!(d["kind"], "deploy");
    assert_eq!(d["program"], program);
    assert_eq!(d["words_len"], 412);
    let (_, _, d) = call_api(&live.app, &format!("/api/v1/transactions/{}", call_tx["hash"].as_str().unwrap())).await;
    assert_eq!(d["kind"], "call");
    assert_eq!(d["program"], program);
    assert_eq!(d["call_proof_len"], 1202416, "a constraint-set-5 proof size is stored as reported");
    assert_eq!(d["input_envelope_len"], 1280);
    // The receipt is the node's, with the public digest the proof was checked against (v0.4).
    assert_eq!(d["receipt"]["tier"], 14, "{d}");
    assert_eq!(d["receipt"]["h_in"], h("h_in"));
    assert_eq!(d["receipt"]["h_pub"], public_digest(&program));

    let (_, _, d) = call_api(&live.app, &format!("/api/v1/transactions/{}", bond["hash"].as_str().unwrap())).await;
    assert_eq!(d["kind"], "bond");
    assert_eq!(d["validator"], VALIDATOR_B);
    assert_eq!(d["amount"], "1000000000000");
    assert_eq!(d["registered"], true);
    let (_, _, d) = call_api(&live.app, &format!("/api/v1/transactions/{}", unbond["hash"].as_str().unwrap())).await;
    assert_eq!(d["kind"], "unbond");
    assert_eq!(d["has_bundle"], false);
    assert_eq!(d["action_nonce"], 1);
    let (_, _, d) = call_api(&live.app, &format!("/api/v1/transactions/{}", withdraw["hash"].as_str().unwrap())).await;
    assert_eq!(d["kind"], "withdraw");
    assert_eq!(d["amount"], "9000000");
    assert_eq!(d["action_nonce"], 3);
    assert_eq!(d["note_time"], 1994, "a withdraw's deposit note publishes its time word");

    let (_, _, d) = call_api(&live.app, &format!("/api/v1/transactions/{}", attest["hash"].as_str().unwrap())).await;
    assert_eq!(d["kind"], "bridge_attest");
    assert_eq!(d["attestation_len"], 520);
    assert_eq!(d["recipient"], SHIELDED_ADDR);
    assert_eq!(d["asset_index"], 1);
    assert_eq!(d["amount"], "1000");
    assert_eq!(d["note_time"], 2);
    assert_eq!(d["commitment"], h("deposit-attest"));
    assert_eq!(d["pq_signers"], json!([0, 1]));
    let (_, _, d) = call_api(&live.app, &format!("/api/v1/transactions/{}", rotation["hash"].as_str().unwrap())).await;
    assert_eq!(d["kind"], "bridge_attest");
    assert!(d["asset_index"].is_null() && d["amount"].is_null(), "a rotation deposits nothing: {d}");
    assert!(d["commitment"].is_null(), "a rotation deposits nothing: {d}");

    let burn_hash = burn["hash"].as_str().unwrap();
    let (_, _, d) = call_api(&live.app, &format!("/api/v1/transactions/{burn_hash}")).await;
    assert_eq!(d["kind"], "bridge_burn");
    assert_eq!(d["asset_index"], 1);
    assert_eq!(d["amount"], "400");
    assert_eq!(d["relayer_fee"], "100");
    assert_eq!(d["to_chain"], 2);
    assert_eq!(d["bridge_to"], evm_to);
    assert_eq!(d["bridge_token"], "cc".repeat(32), "the coin being redeemed, chain 14's single-bundle burn");
    assert!(d.get("asset_bundle").is_none(), "chain 14 has one bundle per transaction, no more asset_bundle");
    assert_eq!(d["bundle"]["nullifiers"][0], h("nf1-bb"));
    assert_eq!(d["bundle"]["nullifiers"].as_array().unwrap().len(), 4);

    // The RPL token standard (spec §4/§6): registration + initial mint, a later mint, an
    // authority change, a holder burn — all public by design, none of it a "transfer".
    let (_, _, d) = call_api(&live.app, &format!("/api/v1/transactions/{}", register_token["hash"].as_str().unwrap())).await;
    assert_eq!(d["kind"], "register_token");
    assert_eq!(d["asset_index"], 3);
    assert_eq!(d["amount"], "5000", "the initial mint's amount");
    assert_eq!(d["recipient"], zusd_register_recipient);
    assert_eq!(d["token_action"]["kind"], "register_token");
    assert_eq!(d["token_action"]["name"], "zUSD");
    assert_eq!(d["token_action"]["initial"]["amount"], "5000");
    let (_, _, d) = call_api(&live.app, &format!("/api/v1/transactions/{}", token_mint["hash"].as_str().unwrap())).await;
    assert_eq!(d["kind"], "token_mint");
    assert_eq!(d["asset_index"], 3);
    assert_eq!(d["amount"], "700");
    assert_eq!(d["recipient"], zusd_mint_recipient);
    assert_eq!(d["action_nonce"], 0);
    assert_eq!(d["token_action"]["r"], "cc".repeat(32));
    let (_, _, d) = call_api(&live.app, &format!("/api/v1/transactions/{}", set_authority["hash"].as_str().unwrap())).await;
    assert_eq!(d["kind"], "set_authority");
    assert_eq!(d["asset_index"], 3);
    assert_eq!(d["action_nonce"], 1);
    assert_eq!(d["token_action"]["new_authority"], VALIDATOR);
    let (_, _, d) = call_api(&live.app, &format!("/api/v1/transactions/{}", token_burn["hash"].as_str().unwrap())).await;
    assert_eq!(d["kind"], "token_burn");
    assert_eq!(d["asset_index"], 3);
    assert_eq!(d["amount"], "400");
    assert_eq!(d["bundle"]["burn_a"], "400");
    assert_eq!(d["bundle"]["burn_asset"], 3);
    assert_eq!(d["bundle"]["burn_r"], "0");

    // Bridge hardening B1/B4: bundle-less pause/unpause, and a second bridged token + backing
    // listed after genesis by the PQ guardian quorum.
    let (_, _, d) = call_api(&live.app, &format!("/api/v1/transactions/{}", pause["hash"].as_str().unwrap())).await;
    assert_eq!(d["kind"], "pause_mints");
    assert_eq!(d["has_bundle"], false);
    assert_eq!(d["bridge_governance"]["nonce"], 4);
    let (_, _, d) = call_api(&live.app, &format!("/api/v1/transactions/{}", unpause["hash"].as_str().unwrap())).await;
    assert_eq!(d["kind"], "unpause_mints");
    assert_eq!(d["pq_signers"], json!([0, 2]));
    let (_, _, d) = call_api(&live.app, &format!("/api/v1/transactions/{}", register_bridged["hash"].as_str().unwrap())).await;
    assert_eq!(d["kind"], "register_bridged_token");
    assert_eq!(d["bridge_governance"]["name"], "zUSD2");
    assert_eq!(d["pq_signers"], json!([1]));
    let (_, _, d) = call_api(&live.app, &format!("/api/v1/transactions/{}", list_backing["hash"].as_str().unwrap())).await;
    assert_eq!(d["kind"], "list_backing");
    assert_eq!(d["asset_index"], 3);
    assert_eq!(d["bridge_governance"]["chain"], 4);

    // Privacy (task S1 item 3): a plain transfer's whole JSON never says which asset moved.
    let dump = serde_json::to_string(&call_api(&live.app, &format!("/api/v1/transactions/{transfer_hash}")).await.2).unwrap();
    assert!(!dump.contains("\"asset\""), "a transfer must never carry an asset field anywhere: {dump}");

    let future_hash = future["hash"].as_str().unwrap();
    let (_, _, d) = call_api(&live.app, &format!("/api/v1/transactions/{future_hash}")).await;
    assert_eq!(d["kind"], "other", "unknown node kinds are indexed, not dropped: {d}");
    assert_eq!(d["has_bundle"], true);
    let raw: String = sqlx::query_scalar("SELECT kind FROM transactions WHERE hash = $1")
        .bind(future_hash)
        .fetch_one(&live.pool)
        .await
        .unwrap();
    assert_eq!(raw, "slash", "the node's own tag is kept for a later backfill");

    // Filters: kind, validator, program; `sender` is gone and `none` is not a filter.
    let (_, _, list) = call_api(&live.app, "/api/v1/transactions?kind=bridge_burn").await;
    assert_eq!(list["pagination"]["total"], 1);
    assert_eq!(list["data"][0]["hash"], burn_hash);
    let (_, _, list) = call_api(&live.app, &format!("/api/v1/transactions?validator={VALIDATOR_B}")).await;
    assert_eq!(list["pagination"]["total"], 2, "{list}");
    let (_, _, list) = call_api(&live.app, &format!("/api/v1/transactions?program={program}")).await;
    assert_eq!(list["pagination"]["total"], 3, "the deploy, the call and the invoke: {list}");
    let (_, _, list) = call_api(&live.app, "/api/v1/transactions?kind=transfer").await;
    assert_eq!(list["pagination"]["total"], 2001);
    for bad in ["none", "slash", "other"] {
        let (status, _, _) = call_api(&live.app, &format!("/api/v1/transactions?kind={bad}")).await;
        assert_eq!(status, 400, "{bad} is not a valid filter");
    }

    // The full-size block is served whole.
    let (status, _, b) = call_api(&live.app, "/api/v1/blocks/4").await;
    assert_eq!(status, 200);
    assert_eq!(b["tx_count"], 2000);
    assert_eq!(b["transactions"].as_array().unwrap().len(), 2000);

    // Nullifiers: all four slots of the burn's one bundle, and the lookup names the spending
    // transaction.
    let (status, _, nf) = call_api(&live.app, &format!("/api/v1/nullifiers/{}", h("nf2-bb"))).await;
    assert_eq!(status, 200, "{nf}");
    assert_eq!(nf["tx_hash"], burn_hash);
    assert_eq!(nf["height"], 3);
    let (status, _, nf4) = call_api(&live.app, &format!("/api/v1/nullifiers/{}", h("nf4-bb"))).await;
    assert_eq!(status, 200, "{nf4}");
    assert_eq!(nf4["tx_hash"], burn_hash);
    let (status, _, _) = call_api(&live.app, &format!("/api/v1/nullifiers/{}", h("never"))).await;
    assert_eq!(status, 404);
    // The batch lookup answers for a whole history in one request: the published ones come
    // back, the unpublished one is just absent.
    let lookup = json!({ "nullifiers": [h("nf2-bb"), h("never"), h("nf4-bb")] });
    let (status, _, found) =
        call(&live.app, json_req("POST", "/api/v1/nullifiers/lookup", Some(lookup), None)).await;
    assert_eq!(status, 200, "{found}");
    let mut spent: Vec<&str> =
        found["spent"].as_array().unwrap().iter().map(|n| n["nullifier"].as_str().unwrap()).collect();
    spent.sort();
    let mut want = vec![h("nf2-bb"), h("nf4-bb")];
    want.sort();
    assert_eq!(spent, want);
    assert_eq!(found["spent"][0]["tx_hash"], burn_hash);
    let too_many = json!({ "nullifiers": vec![h("never"); 1001] });
    let (status, _, _) =
        call(&live.app, json_req("POST", "/api/v1/nullifiers/lookup", Some(too_many), None)).await;
    assert_eq!(status, 400);

    // The commitment tree was paged in (1000 rows per page) and linked to the transactions.
    let notes = live
        .wait_for("/api/v1/notes?limit=5", WAIT, |n| n["pagination"]["total"] == expected_leaves)
        .await;
    assert_eq!(notes["data"][0]["leaf_index"], expected_leaves as i64 - 1, "newest first: {notes}");
    let (status, _, n) = call_api(&live.app, &format!("/api/v1/notes/{}", h("cm-mint"))).await;
    assert_eq!(status, 200, "{n}");
    assert_eq!(n["tx_hash"], mint_hash);
    assert_eq!(n["height"], 1);
    let (_, _, n0) = call_api(&live.app, "/api/v1/notes/0").await;
    assert_eq!(n0["height"], 0);
    assert!(n0["tx_hash"].is_null(), "a genesis deposit note has no transaction: {n0}");
    let (_, _, n) = call_api(&live.app, &format!("/api/v1/notes/{}", h("cm2-bb"))).await;
    assert_eq!(n["tx_hash"], burn_hash, "the bundle's four output slots are all leaves");

    // The chain-computed notes (task S1 item 1/3): a register_token's initial mint and a
    // token_mint each add ONE note whose envelope rides in the action, not the bundle — but it
    // is still a real tree leaf, linked back to its transaction by the indexer's own recomputed
    // commitment (there is no wire field for it, unlike a bridge_attest's).
    let register_hash = register_token["hash"].as_str().unwrap();
    let (_, _, env) = call_api(&live.app, &format!("/api/v1/transactions/{register_hash}/envelopes")).await;
    assert_eq!(env["notes"].as_array().unwrap().len(), 5, "4 bundle slots + the initial mint: {env}");
    let mint_zusd_hash = token_mint["hash"].as_str().unwrap();
    let (_, _, env) = call_api(&live.app, &format!("/api/v1/transactions/{mint_zusd_hash}/envelopes")).await;
    assert_eq!(env["notes"].as_array().unwrap().len(), 5, "4 bundle slots + the minted note: {env}");
    let derived_cm = mint_note_cm(&zusd_mint_recipient, 700, 3, 3, &"cc".repeat(32));
    assert!(
        env["notes"].as_array().unwrap().iter().any(|n| n["cm"] == derived_cm),
        "the minted note's own commitment must be among the transaction's notes: {env}"
    );

    // Envelopes: every leaf carries the node's envelope, a transaction lists the leaves it
    // created, and a call gets its sealed transcript from the node on demand.
    let (status, _, env) = call_api(&live.app, &format!("/api/v1/transactions/{mint_hash}/envelopes")).await;
    assert_eq!(status, 200, "{env}");
    assert_eq!(env["kind"], "mint");
    assert_eq!(env["notes"].as_array().unwrap().len(), 1);
    assert_eq!(env["notes"][0]["cm"], h("cm-mint"));
    assert_eq!(env["notes"][0]["envelope"]["kem_ct"], "00");
    assert!(env["call_envelope"].is_null() && env["h_in"].is_null());
    let (_, _, env) = call_api(&live.app, &format!("/api/v1/transactions/{burn_hash}/envelopes")).await;
    assert_eq!(env["notes"].as_array().unwrap().len(), 4, "the one hidden-asset bundle's four slots: {env}");
    let (_, _, env) = call_api(&live.app, &format!("/api/v1/transactions/{}/envelopes", call_tx["hash"].as_str().unwrap())).await;
    assert_eq!(env["kind"], "call");
    assert_eq!(env["call_envelope"]["body"], "0b".repeat(60));
    assert_eq!(env["h_in"], h("h_in"), "the transcript opens against the receipt's h_in");
    let (_, _, page) = call_api(&live.app, "/api/v1/envelopes?from_leaf=0&limit=1000").await;
    assert_eq!(page["total_leaves"], expected_leaves);
    assert_eq!(page["notes"].as_array().unwrap().len(), 1000);
    assert_eq!(page["next_leaf"], 1000);
    let (_, _, page) = call_api(&live.app, &format!("/api/v1/envelopes?from_leaf={}&limit=1000", expected_leaves - 5)).await;
    assert_eq!(page["notes"].as_array().unwrap().len(), 5);
    assert!(page["next_leaf"].is_null());

    // Search finds a commitment and a nullifier, and no accounts.
    let (_, _, found) = call_api(&live.app, &format!("/api/v1/search?q={}", h("cm-mint"))).await;
    assert_eq!(found[0]["type"], "note", "{found}");
    let (_, _, found) = call_api(&live.app, &format!("/api/v1/search?q={}", h("nf1-t"))).await;
    assert_eq!(found[0]["type"], "nullifier", "{found}");
    let (_, _, found) = call_api(&live.app, &format!("/api/v1/search?q={VALIDATOR_B}")).await;
    assert_eq!(found[0]["type"], "validator");
    assert_eq!(found[0]["title"], "Validator (inactive)");
    let (status, _, gone) = call_api(&live.app, &format!("/api/v1/accounts/{VALIDATOR}")).await;
    assert_eq!(status, 410, "{gone}");
    assert_eq!(gone["error"], "no_accounts");
    let (status, _, _) = call_api(&live.app, &format!("/api/v1/accounts/{VALIDATOR}/transactions")).await;
    assert_eq!(status, 410);

    // Stats carry the tree, the register and the supply audit.
    let stats = live.wait_for("/api/v1/stats", WAIT, |s| s["chain_id"] == 7 && s["height"] == 4 && s["validator_count"] == 2).await;
    assert_eq!(stats["total_transactions"], 2020, "{stats}");
    // The chain's identity and limits, and the node's build (v0.3 / v0.4).
    let (_, _, genesis) = call_api(&live.app, "/api/v1/blocks/0").await;
    assert_eq!(stats["genesis_hash"], genesis["hash"], "{stats}");
    assert!(stats["genesis_hash"].is_string());
    assert_eq!(stats["node_version"], "0.1.0");
    assert_eq!(stats["node_git_sha"], MOCK_GIT_SHA);
    assert_eq!(stats["fri_profile"], "test");
    // Chain 18's `rand_getLimits`, served whole: the caps, the memo envelope, the v0.6 switch, the
    // auth guest and the gas section's genesis parameters (prices as decimal strings).
    assert_eq!(stats["limits"], json!({ "max_program_words": 65535, "max_proof_bytes": 8388608,
        "max_block_bytes": 20971520, "max_call_envelope_bytes": 65536, "max_program_public_words": 32768,
        "envelope_bytes": 1860, "hardening_v6": true, "hc_auth": h("hc_auth"),
        "gas_price": "100", "byte_price": "800", "gas_metering": "circuit",
        "bundle_gas_limit": 20479, "adjust_bps": 1250,
        "program_state": { "cell_fee": "10000000", "max_reads": 8, "max_writes": 8, "max_payouts": 4 },
        // Audit v6's fields, absent from this mock's (chain-18-shaped) reply: their defaults.
        "max_gas_price": null, "max_byte_price": null, "byte_load": null, "admission_by_vote": false,
        "testnet": false, "slashing": null, "binding_domain": null, "proof_window_blocks": null,
        // Fee feedback: the mock serves `null`, a chain without a `fees` section.
        "fee_rules": null }));
    // Off `rand_status`, refreshed every commit: the auth guest, and the tip's live prices (one
    // 12.5% step above the genesis prices the limits report — a dynamic chain moves them).
    assert_eq!(stats["hc_auth"], h("hc_auth"));
    assert_eq!(stats["gas_prices"], json!({ "gas_price": "112", "byte_price": "900" }));
    // RPL-2: the `program_state` group rides inside the stored limits.
    assert_eq!(stats["limits"]["program_state"], json!({ "cell_fee": "10000000", "max_reads": 8, "max_writes": 8, "max_payouts": 4 }));
    assert_eq!(stats["notes"], expected_leaves);
    // 2015 bundle-carrying transactions (2000 transfers, 1 mint's bundle-less action aside, plus
    // deploy/call/bond/attest/rotation/burn/future/register_token/token_mint/set_authority/
    // token_burn/register_bridged/list_backing/invoke = 15 more), each four slots; pause_mints
    // and unpause_mints carry none.
    assert_eq!(stats["nullifiers"], 2015 * 4, "{stats}");
    assert_eq!(stats["active_validator_count"], 1);
    assert_eq!(stats["total_stake"], "100000000000000", "only the active set counts");
    assert_eq!(stats["total_supply"], "101100000000000");
    assert_eq!(stats["pool_value"], "1099997000000");
    assert_eq!(stats["epoch"], 0);
    assert_eq!(stats["epoch_blocks"], 1000);
    assert_eq!(stats["hc_bundle"], h("hc_bundle"));
    assert_eq!(stats["current_leader"], VALIDATOR);
    assert!(stats.get("total_accounts").is_none());

    let (_, _, v) = call_api(&live.app, "/api/v1/validators").await;
    let v = v.as_array().unwrap();
    assert_eq!(v.len(), 2, "{v:?}");
    assert_eq!(v[0]["address"], VALIDATOR);
    assert_eq!(v[0]["rewards"], "3000000");
    assert_eq!(v[0]["active"], true);
    assert_eq!(v[0]["share_percent"], 100.0);
    assert_eq!(v[1]["address"], VALIDATOR_B);
    assert_eq!(v[1]["active"], false);
    assert_eq!(v[1]["pending"], json!([{ "release_epoch": 3, "amount": "5000000000" }]));
    assert_eq!(v[1]["payout"], SHIELDED_ADDR);
    assert_eq!(v[1]["share_percent"], 0.0);
    let (_, _, supply) = call_api(&live.app, "/api/v1/supply").await;
    assert_eq!(supply["invariant_holds"], true);
    assert_eq!((&supply["program_rand_out"], &supply["program_rand_held"]), (&json!("300"), &json!("700")), "RPL-2 vault counters: {supply}");
    assert_eq!(supply["base_fees_burned"], "0", "fee feedback's burn counter passes through: {supply}");
    let (_, _, bridge) = call_api(&live.app, "/api/v1/bridge").await;
    assert_eq!(bridge["enabled"], true);
    assert_eq!(bridge["assets"][0]["index"], 1);
    // Bridge hardening B1/B3/B4 (task S1 item 4): pause state, the PQ guardian count and
    // per-backing locked/minted_today/cap all come straight through from `rand_getBridgeState`.
    assert_eq!(bridge["mint_paused"], false);
    assert_eq!(bridge["pause_nonce"], 0);
    assert_eq!(bridge["list_nonce"], 0);
    assert_eq!(bridge["registration_fee"], 1_000_000_000u64);
    assert!(bridge["pause_key"].as_str().is_some());
    // Chain 19: bridge rules v2 and the genesis replay floor come through as the node sent
    // them, and each trusted emitter is also served the way its own chain prints it — the
    // redeployed Ethereum/BSC/Tron endpoints and the unchanged Solana program, in chain order,
    // each with its floor.
    assert_eq!(bridge["rotation_nonce"], 0);
    assert_eq!(bridge["rules_v2"], json!({ "global_mint_cap_per_window": "400000000000", "cap_window_secs": 86400,
                                           "global_minted_in_window": "1000", "global_mint_headroom": "399999999000" }));
    assert_eq!(bridge["assets"][0]["mint_headroom"], "999000");
    assert_eq!(bridge["assets"][1]["mint_headroom"], Value::Null, "an older row has no headroom");
    assert_eq!(bridge["min_inbound_sequence"], json!({ "2": 1, "3": 1, "4": 1, "5": 4 }));
    assert_eq!(bridge["emitters"]["4"], "0000000000000000000000006410797df959987a5baf65b5fab97edeb34d5163");
    assert_eq!(
        bridge["endpoints"],
        json!([
            { "chain": 2, "chain_name": "Ethereum",
              "emitter": "0000000000000000000000007af6b17047c1db6cb54347fdea45cf9179075bfa",
              "address": "0x7af6b17047c1db6cb54347fdea45cf9179075bfa",
              "explorer_url": "https://etherscan.io/address/0x7af6b17047c1db6cb54347fdea45cf9179075bfa",
              "min_inbound_sequence": 1 },
            { "chain": 3, "chain_name": "BSC",
              "emitter": "0000000000000000000000007af6b17047c1db6cb54347fdea45cf9179075bfa",
              "address": "0x7af6b17047c1db6cb54347fdea45cf9179075bfa",
              "explorer_url": "https://bscscan.com/address/0x7af6b17047c1db6cb54347fdea45cf9179075bfa",
              "min_inbound_sequence": 1 },
            { "chain": 4, "chain_name": "Tron",
              "emitter": "0000000000000000000000006410797df959987a5baf65b5fab97edeb34d5163",
              "address": "TK6JJv55CCkFjNHq7WwoU91GKaZEiC93me",
              "explorer_url": "https://tronscan.org/#/contract/TK6JJv55CCkFjNHq7WwoU91GKaZEiC93me",
              "min_inbound_sequence": 1 },
            { "chain": 5, "chain_name": "Solana",
              "emitter": "d3e58f1e9317bbc3c69b63fadff558ea82ba5d00765f1f1e483d705d209b413a",
              "address": "FGA3kY3RjfDKjUszJESMYtYXAbsnkFhhoxM3Mb34vycu",
              "explorer_url": "https://solscan.io/account/FGA3kY3RjfDKjUszJESMYtYXAbsnkFhhoxM3Mb34vycu",
              "min_inbound_sequence": 4 },
        ])
    );
    // decimals 8 / locked 600 / mint_cap_per_day 1e13 / minted_today 1000 are the node's own
    // pinned test's literal values (rpc.rs's `bridge_state_reports_guardians_emitters_and_the_
    // registry`), sent as JSON numbers on the wire — this API's own `/bridge` output still
    // normalises `locked` to a decimal string, its established convention regardless of how the
    // node encoded it (`randscan_core::amount`'s tolerant deserializer).
    assert_eq!(bridge["assets"][0]["decimals"], 8);
    assert_eq!(bridge["assets"][0]["locked"], "600");
    assert_eq!(bridge["assets"][0]["mint_cap_per_day"], (100_000u64 * 100_000_000).to_string());
    assert_eq!(bridge["assets"][0]["minted_today"], "1000");
    // Registry rows joined with the indexed flows: index 1 saw one 1000-unit deposit and one
    // 400-unit burn (the rotation carries no asset and is not counted); index 2 is Ethereum
    // USDT, registered but never used, so it is named and zero.
    let (_, _, assets) = call_api(&live.app, "/api/v1/bridge/assets").await;
    let assets = assets.as_array().expect("array");
    assert_eq!(assets.len(), 3, "{assets:?}");
    // Token 1 has two backings (chain 14: one zUSD, many coins). The burn named (chain 2, cc…),
    // so it is that backing's alone; the other coin of the same token saw nothing. A row must
    // never repeat the whole token's tally as its own.
    assert_eq!((&assets[0]["index"], &assets[0]["chain"]), (&json!(1), &json!(2)));
    assert_eq!((&assets[1]["index"], &assets[1]["chain"]), (&json!(1), &json!(3)));
    assert_eq!(assets[0]["burns"], 1);
    assert_eq!(assets[0]["burned"], "400");
    assert_eq!(assets[1]["burns"], 0, "a burn is its own backing's: {:?}", assets[1]);
    assert_eq!(assets[1]["burned"], "0");
    // What a backing holds is the registry's `locked`, not a figure rebuilt from token-wide sums.
    assert_eq!(assets[0]["outstanding"], "600");
    assert_eq!(assets[1]["outstanding"], "0");
    // Audit v6: the window's count and the headroom ride along per backing where the node
    // serves them, and are null where it does not.
    assert_eq!((&assets[0]["minted_in_window"], &assets[0]["mint_window_secs"], &assets[0]["mint_headroom"]), (&json!("1000"), &json!(86_400), &json!("999000")));
    assert_eq!((&assets[1]["minted_in_window"], &assets[1]["mint_headroom"]), (&Value::Null, &Value::Null));
    // A deposit publishes the token it minted, not the coin that was locked for it, so with
    // several backings it cannot be laid at one of them: per backing it is null, and the whole
    // token's figure is served under its own name, the same on each of the token's rows.
    for row in &assets[..2] {
        assert_eq!(row["backings"], 2);
        assert_eq!(row["deposits"], Value::Null, "{row}");
        assert_eq!(row["deposited"], Value::Null);
        assert_eq!(row["token_deposits"], 1);
        assert_eq!(row["token_deposited"], "1000");
        assert_eq!(row["token_burns"], 1);
        assert_eq!(row["token_burned"], "400");
    }
    assert_eq!(assets[0]["symbol"], Value::Null);
    assert_eq!(assets[0]["first_height"], assets[0]["last_height"]);
    assert_eq!(assets[0]["locked"], "600", "the registry's own figure, alongside the indexed flows");
    assert_eq!(assets[0]["minted_today"], "1000");
    assert_eq!(assets[0]["mint_cap_per_day"], (100_000u64 * 100_000_000).to_string());
    // Token 2 has one backing, so the token's deposits are that backing's.
    assert_eq!(assets[2]["index"], 2);
    assert_eq!(assets[2]["symbol"], "USDT");
    assert_eq!(assets[2]["name"], "Tether USD");
    assert_eq!(assets[2]["decimals"], 6);
    assert_eq!(assets[2]["backings"], 1);
    assert_eq!(assets[2]["deposits"], 0);
    assert_eq!(assets[2]["deposited"], "0");
    assert_eq!(assets[2]["outstanding"], "0");
    assert_eq!(assets[2]["last_height"], Value::Null);

    // The RPL token registry (task S1 item 2): the list page and one token's detail, its
    // backing's locked/minted_today, its deploy transaction and its public supply history.
    let (status, _, tokens_list) = call_api(&live.app, "/api/v1/tokens").await;
    assert_eq!(status, 200, "{tokens_list}");
    assert_eq!(tokens_list["enabled"], true);
    assert_eq!(tokens_list["tokens"].as_array().unwrap().len(), 1);
    assert_eq!(tokens_list["tokens"][0]["symbol"], "zUSD");
    assert_eq!(tokens_list["tokens"][0]["decimals"], 6);
    assert_eq!(tokens_list["tokens"][0]["authority"]["kind"], "bridge");
    assert_eq!(tokens_list["tokens"][0]["authority"]["backings"][0]["locked"], "600");
    assert_eq!(tokens_list["tokens"][0]["authority"]["backings"][0]["minted_today"], "700");
    // The node sent this one as a bare number (unlike locked/minted_today, strings, on the same
    // backing row) — this API's output still normalises it to a decimal string regardless.
    assert_eq!(
        tokens_list["tokens"][0]["authority"]["backings"][0]["mint_cap_per_day"],
        (100_000u64 * 100_000_000).to_string()
    );

    for key in ["3", &h("zusd-id"), "rpl1zusdexampleexampleexampleexampleexampleexampleexampleeez"] {
        let (status, _, d) = call_api(&live.app, &format!("/api/v1/tokens/{key}")).await;
        assert_eq!(status, 200, "token lookup by {key}: {d}");
        assert_eq!(d["symbol"], "zUSD", "{key}");
        assert_eq!(d["deploy_tx"], register_hash, "the register_token transaction, by asset_index: {d}");
        let history = d["supply_history"].as_array().unwrap();
        assert_eq!(history.len(), 3, "register (initial 5000) + mint (700) + burn (400): {d}");
        assert_eq!(history[0]["kind"], "register_token");
        assert_eq!(history[0]["delta"], "5000");
        assert_eq!(history[1]["kind"], "token_mint");
        assert_eq!(history[1]["delta"], "700");
        assert_eq!(history[2]["kind"], "token_burn");
        assert_eq!(history[2]["delta"], "-400");
    }
    let (status, _, notfound) = call_api(&live.app, "/api/v1/tokens/999").await;
    assert_eq!(status, 404, "{notfound}");

    // `kind=register_token` (and every other new tag) is a valid filter, distinct from `transfer`.
    let (_, _, list) = call_api(&live.app, "/api/v1/transactions?kind=register_token").await;
    assert_eq!(list["pagination"]["total"], 1, "{list}");
    let (_, _, list) = call_api(&live.app, "/api/v1/transactions?kind=token_mint").await;
    assert_eq!(list["pagination"]["total"], 1, "{list}");
    let (_, _, tokens) = call_api(&live.app, "/api/v1/bridge/tokens").await;
    let tokens = tokens.as_array().expect("array");
    assert_eq!(tokens.len(), 8);
    assert_eq!(tokens[0], json!({
        "symbol": "USDT", "name": "Tether USD", "chain": 2, "chain_name": "Ethereum", "standard": "ERC-20",
        "address": "0xdAC17F958D2ee523a2206206994597C13D831ec7",
        "token": format!("{}dac17f958d2ee523a2206206994597c13d831ec7", "0".repeat(24)),
        "decimals": 6, "status": "allowed",
        "explorer_url": "https://etherscan.io/token/0xdac17f958d2ee523a2206206994597c13d831ec7"
    }));
    assert_eq!(tokens.iter().filter(|t| t["status"] == "discontinued").count(), 1);
    let (_, _, p) = call_api(&live.app, &format!("/api/v1/programs/{program}")).await;
    assert_eq!(p["call_count"], 1);
    assert!(p.get("deployer").is_none(), "{p}");
    // RPL-2: the invoke is counted apart from the calls and listed with them; the program's
    // vault and cells come live from the node, and the cells page again.
    assert_eq!(p["invoke_count"], 1);
    assert_eq!(p["last_invoked_height"], 3);
    assert_eq!(p["recent_calls"].as_array().unwrap().len(), 2, "{p}");
    assert_eq!(p["recent_calls"][0]["kind"], "invoke");
    assert_eq!(p["program_state"]["vault"], json!([{ "asset": 0, "amount": "700" }, { "asset": 3, "amount": "500" }]));
    assert_eq!(p["program_state"]["cells"], json!([{ "key": cell_key, "value": format!("05{}", "00".repeat(31)) }]));
    assert!(p["program_state"]["cells_next"].is_null());
    let (status, _, cells) = call_api(&live.app, &format!("/api/v1/programs/{program}/cells?limit=1")).await;
    assert_eq!(status, 200, "{cells}");
    assert_eq!(cells["cells"][0]["key"], cell_key);
    assert!(cells["next"].is_null());
    let (status, _, cells) = call_api(&live.app, &format!("/api/v1/programs/{program}/cells?after={cell_key}")).await;
    assert_eq!((status.as_u16(), cells["cells"].as_array().map(Vec::len)), (200, Some(0)), "{cells}");
    let (status, _, _) = call_api(&live.app, &format!("/api/v1/programs/{program}/cells?after=nothex")).await;
    assert_eq!(status, 400);

    // The invoke itself: a call's fields plus the transition, served as the node sent it (amounts
    // as decimal strings), its receipt a call's, and its two payout notes linked to it — by
    // commitment, and in its envelope list beside the bundle's four slots.
    let invoke_hash = invoke["hash"].as_str().unwrap();
    let (_, _, d) = call_api(&live.app, &format!("/api/v1/transactions/{invoke_hash}")).await;
    assert_eq!(d["kind"], "invoke");
    assert_eq!(d["program"], program);
    assert_eq!(d["call_proof_len"], 268123);
    assert!(d["input_envelope_len"].is_null());
    assert_eq!(d["bundle"]["burn_r"], "1000");
    assert_eq!((&d["bundle"]["burn_a"], &d["bundle"]["burn_asset"]), (&json!("500"), &json!(3)));
    assert_eq!(d["transition"]["inflow"], "deposit");
    assert_eq!(d["transition"]["reads"], json!([{ "key": cell_key, "value": "00".repeat(32) }]));
    assert_eq!(d["transition"]["writes"][0]["value"], format!("05{}", "00".repeat(31)));
    assert_eq!(d["transition"]["pays"], json!([{ "asset": 0, "amount": "300", "recipient": invoke_recipient, "time": 2, "r": "a1".repeat(32), "cm": h("payout-0") }]));
    assert_eq!(d["transition"]["mints"][0]["amount"], "40", "a numeric amount on the wire is a decimal string here");
    assert_eq!(d["transition"]["mints"][0]["cm"], h("payout-1"));
    assert_eq!(d["receipt"]["tier"], 14, "an invoke has a call's receipt: {d}");
    assert_eq!(d["receipt"]["program"], program);
    for cm in [h("payout-0"), h("payout-1")] {
        let (_, _, note) = call_api(&live.app, &format!("/api/v1/notes/{cm}")).await;
        assert_eq!(note["tx_hash"], invoke_hash, "a payout leaf links back to its invoke: {note}");
    }
    let (_, _, env) = call_api(&live.app, &format!("/api/v1/transactions/{invoke_hash}/envelopes")).await;
    assert_eq!(env["kind"], "invoke");
    let cms: Vec<&str> = env["notes"].as_array().unwrap().iter().map(|n| n["cm"].as_str().unwrap()).collect();
    assert_eq!(cms.len(), 6, "four bundle slots and two payouts: {env}");
    assert!(cms.contains(&h("payout-0").as_str()) && cms.contains(&h("payout-1").as_str()));
    let (_, _, list) = call_api(&live.app, "/api/v1/transactions?kind=invoke").await;
    assert_eq!(list["pagination"]["total"], 1, "{list}");
    assert_eq!(list["data"][0]["hash"], invoke_hash);
    // The deploy-time public input (v0.4): the length is the action's, the digest the node's.
    assert_eq!(p["public_words_len"], 27151, "{p}");
    assert_eq!(p["public_digest"], public_digest(&program));
    assert_eq!(p["code_hash"], h(&format!("code-{program}")));

    // Broadcast fan-out (what /ws subscribers receive) carried every kind.
    let got = {
        let ev = events.lock().unwrap();
        let mut kinds = Vec::new();
        for e in ev.iter() {
            if let BroadcastEvent::NewTransaction(t) = e {
                kinds.push(t.kind);
            }
        }
        kinds
    };
    for k in [
        TxKind::Transfer, TxKind::Mint, TxKind::Deploy, TxKind::Call, TxKind::Bond, TxKind::Unbond,
        TxKind::Withdraw, TxKind::BridgeAttest, TxKind::BridgeBurn, TxKind::RegisterToken, TxKind::TokenMint,
        TxKind::SetAuthority, TxKind::TokenBurn, TxKind::PauseMints, TxKind::UnpauseMints,
        TxKind::RegisterBridgedToken, TxKind::ListBacking, TxKind::Invoke, TxKind::Other,
    ] {
        assert!(got.contains(&k), "broadcast kinds miss {k}: {got:?}");
    }

    // ----- phase 2: a user and an API key exist; the fleet hard-forks to chain 8 (an S3-only
    // node: register without pending/active, no epoch, no supply) ----------------------------
    let body = json!({ "email": unique_email(), "password": "correct horse battery" });
    let (status, headers, _) = call(&live.app, json_req("POST", "/api/v1/auth/signup", Some(body), None)).await;
    assert_eq!(status, 201);
    let cookie = session_cookie_from(&headers);
    let (status, _, created) = call(&live.app, json_req("POST", "/api/v1/keys", Some(json!({ "name": "bot" })), Some(&cookie))).await;
    assert_eq!(status, 201, "{created}");
    let key = created["key"].as_str().unwrap().to_string();

    node.with_chain(|c| {
        *c = MockChain::new(8, "genesis-b");
        c.pre_s2 = true;
        c.push_block(vec![tx(8, "t", Some(bundle("t8")), json!({ "kind": "none" }))]);
    });

    let stats = live.wait_for("/api/v1/stats", WAIT, |s| s["chain_id"] == 8 && s["height"] == 1).await;
    assert_eq!(stats["total_transactions"], 1, "old chain data gone: {stats}");
    assert_eq!(stats["notes"], 5, "the genesis note plus the one bundle's four output slots");
    assert!(stats["epoch"].is_null() && stats["pool_value"].is_null(), "S2 fields absent on an S3 node: {stats}");
    // A node older than v0.3 / v0.4 has none of the three methods; the old chain's answers must
    // not linger. The FRI profile still comes from `rand_status`.
    assert!(stats["limits"].is_null() && stats["genesis_hash"].is_null() && stats["node_git_sha"].is_null(), "{stats}");
    assert!(stats["hc_auth"].is_null() && stats["gas_prices"].is_null(), "no auth guest, no gas section: {stats}");
    assert_eq!(stats["fri_profile"], "test");
    assert_eq!(stats["total_supply"], "0");
    assert_eq!(stats["active_validator_count"], 1, "every register entry is active before S2");
    let (status, _, _) = call_api(&live.app, &format!("/api/v1/transactions/{burn_hash}")).await;
    assert_eq!(status, 404);
    let (status, _, _) = call_api(&live.app, &format!("/api/v1/notes/{}", h("cm-mint"))).await;
    assert_eq!(status, 404);
    let (status, _, _) = call_api(&live.app, "/api/v1/supply").await;
    assert_eq!(status, 404);
    let (status, _, b0) = call_api(&live.app, "/api/v1/blocks/0").await;
    assert_eq!(status, 200);
    assert_eq!(b0["hash"], h("block-8-genesis-b-0"));
    let (stored_chain, next_leaf): (Option<i64>, i64) =
        sqlx::query_as("SELECT chain_id, next_leaf FROM indexer_state WHERE id = 1")
            .fetch_one(&live.pool)
            .await
            .unwrap();
    assert_eq!(stored_chain, Some(8));
    assert_eq!(next_leaf, 5);

    // Users, sessions and keys survived the fork.
    let (status, _, me) = call(&live.app, json_req("GET", "/api/v1/auth/me", None, Some(&cookie))).await;
    assert_eq!(status, 200, "{me}");
    let (_, _, keys) = call(&live.app, json_req("GET", "/api/v1/keys", None, Some(&cookie))).await;
    assert_eq!(keys.as_array().unwrap().len(), 1);
    let req = axum::http::Request::builder()
        .uri("/api/v1/stats")
        .header("authorization", format!("Bearer {key}"))
        .body(axum::body::Body::empty())
        .unwrap();
    let (status, _, _) = call(&live.app, req).await;
    assert_eq!(status, 200);

    // ----- phase 3: same chain id, re-created genesis, node head below the indexed height ---
    node.with_chain(|c| {
        c.push_block(vec![]);
        c.push_block(vec![]);
    });
    live.wait_for("/api/v1/health", WAIT, |b| b["indexer"]["current_height"] == 3).await;
    node.with_chain(|c| *c = MockChain::new(8, "genesis-c"));
    // The forced re-check is rate limited (10 s), so allow for that.
    let b0 = live
        .wait_for("/api/v1/blocks/0", WAIT, |b| b["hash"] == h("block-8-genesis-c-0"))
        .await;
    assert_eq!(b0["height"], 0);
    let (status, _, _) = call_api(&live.app, "/api/v1/blocks/3").await;
    assert_eq!(status, 404);
    let notes = live.wait_for("/api/v1/notes", WAIT, |n| n["pagination"]["total"] == 1).await;
    assert_eq!(notes["data"][0]["cm"], h("genesis-note-8-genesis-c"), "the tree was re-read from leaf 0");
    let (status, _, _) = call(&live.app, json_req("GET", "/api/v1/auth/me", None, Some(&cookie))).await;
    assert_eq!(status, 200);
}

async fn call_api(app: &axum::Router, path: &str) -> (axum::http::StatusCode, axum::http::HeaderMap, Value) {
    call(app, json_req("GET", path, None, None)).await
}
