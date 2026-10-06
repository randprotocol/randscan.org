//! RandScan against a real shielded `rand-node` from `../fullnode` (branch `shielded-s3` or
//! later): a single-validator chain with the faucet on, a faucet mint and a shielded transfer
//! submitted through the wallet, then the explorer must agree with the node on every public
//! number: the bundle's nullifiers and commitments, the mint's note, the tree, the register.
//!
//! Needs `DATABASE_URL` and `RAND_NODE_BIN` (path to a built `rand-node`); skips when either
//! is unset, unless `RANDSCAN_REQUIRE_REAL_NODE` is set (CI), which makes that a failure. The wallet CLI (`rand`) must sit next to the node binary or be named by
//! `RAND_CLI`; it proves the bundle locally under the chain's `test` FRI profile. With the
//! wallet the test also deploys a guest, proves and submits one confidential call, and checks
//! the explorer's receipt (tier, outputs, `h_in`) against the node's — the proof is made under
//! whatever zkVM constraint set the node was built with, so this is what notices a zkVM
//! re-sync changing the receipt shape.

mod common;

use common::*;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(90);
const CHAIN_ID: u64 = 7701;

struct Node {
    child: Child,
    url: String,
    dir: tempfile::TempDir,
    /// The genesis has a `program_state` section (a build with `--program-state-cell-fee`).
    rpl2: bool,
}

impl Drop for Node {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Since fullnode v0.7 a release `rand-node` (and the wallet) refuses a genesis under the `test`
/// FRI profile unless told it is a harness; an older build ignores the variable.
const ALLOW_TEST_FRI: (&str, &str) = ("RAND_ALLOW_TEST_FRI_PROFILE", "1");

fn run(bin: &PathBuf, args: &[&str]) -> String {
    let out = Command::new(bin)
        .args(args)
        .env(ALLOW_TEST_FRI.0, ALLOW_TEST_FRI.1)
        .output()
        .expect("run binary");
    assert!(
        out.status.success(),
        "{:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

async fn rpc(client: &reqwest::Client, url: &str, method: &str, params: Value) -> Value {
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    let v: Value = client
        .post(url)
        .json(&body)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(v.get("error").is_none(), "{method}: {v}");
    v["result"].clone()
}

/// An amount as the node renders it: a JSON integer in older builds, a decimal string since the
/// amounts on `tx_json` moved to strings (v0.6). Either way, its decimal text.
fn units(v: &Value) -> String {
    match v {
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        other => panic!("not an amount: {other}"),
    }
}

/// The hash after `submitted <what> ` in the wallet's output.
fn submitted_hash(out: &str, what: &str) -> String {
    out.lines()
        .find_map(|l| l.strip_prefix(&format!("submitted {what} ")))
        .and_then(|r| r.split(' ').next())
        .unwrap_or_else(|| panic!("no `submitted {what}` line in: {out}"))
        .to_string()
}

/// keygen + genesis + init + run; returns once the node answers `rand_getHead`.
async fn start_node(bin: &PathBuf, cli: &PathBuf) -> (Node, String, String) {
    let dir = tempfile::tempdir().unwrap();
    let key = dir.path().join("validator.key.json");
    let wallet = dir.path().join("wallet.key.json");
    let genesis = dir.path().join("genesis.json");
    let datadir = dir.path().join("data");
    run(bin, &["keygen", "--out", key.to_str().unwrap()]);
    let validator_addr = serde_json::from_str::<Value>(&std::fs::read_to_string(&key).unwrap())
        .unwrap()["address"]
        .as_str()
        .unwrap()
        .to_string();
    run(cli, &["keygen", "--key", wallet.to_str().unwrap()]);
    let wallet_addr = run(cli, &["address", "--key", wallet.to_str().unwrap()])
        .trim()
        .to_string();
    assert!(
        wallet_addr.starts_with("rand1"),
        "not a shielded address: {wallet_addr}"
    );
    // Since phase S2 a genesis validator is `<key>,<stake in RAND>,<payout rand1…>` at the
    // staking minimum (1000 RAND); the S3-only builds took the key alone.
    let help = run(bin, &["genesis", "--help"]);
    let validator = if help.contains("KEY,STAKE,PAYOUT") {
        format!("{},1000,{wallet_addr}", key.to_str().unwrap())
    } else {
        key.to_str().unwrap().to_string()
    };
    let mut args = vec![
        "genesis",
        "--chain-id",
        &CHAIN_ID.to_string(),
        "--validator",
        &validator,
        "--faucet",
        "--fri-profile",
        "test",
        "--alloc",
        &format!("{wallet_addr}=1000"),
        "--out",
        genesis.to_str().unwrap(),
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<Vec<_>>();
    // A chain-18 build (v0.6.6 / v0.6.7, constraint set 8) gets chain 18's genesis: the v0.6
    // rules, bundle guest v3 with its auth guest, the encrypted memo (every envelope 1 860 B) and
    // the gas section with the dynamic controller — `deploy/cut-chain18-genesis.sh`'s flags, so
    // this is the shape the explorer meets after the cut. Chain 19 is the same build and the same
    // flags (`deploy/cut-chain19-genesis.sh` asserts every field outside the bridge section equal
    // to chain 18's); its bridge section is spliced in by that script, not by a flag, and is
    // covered by the mock node. An older node keeps the plain genesis.
    if help.contains("--gas-price") && help.contains("--auth-guest") {
        args.extend(
            [
                "--hardening-v6", "--bundle-guest", "v3", "--auth-guest",
                "--max-proof-bytes", "4194304", "--max-block-bytes", "20971520",
                "--envelope-bytes", "1860",
                "--gas-price", "100", "--byte-price", "800", "--bundle-gas-limit", "20479",
                "--gas-dynamic", "10485760,262144,1250",
            ]
            .into_iter()
            .map(str::to_string),
        );
    }
    // Audit v6 BIND-1 (fullnode v0.7): a wallet signs nothing for a chain id outside 14–19
    // unless the genesis binds its hash (`--binding-domain 1`, how every chain from 20 is cut);
    // and the chain-20 proof window (issue #118), which gives a slow CI prover four times the
    // anchor's 256 blocks. An older build has neither flag and keeps the plain genesis.
    if help.contains("--binding-domain") {
        args.extend(["--binding-domain", "1"].into_iter().map(str::to_string));
    }
    if help.contains("--proof-window-blocks") {
        args.extend(["--proof-window-blocks", "1024"].into_iter().map(str::to_string));
    }
    // RPL-2 (fullnode v0.6.8, `feat/rpl2`): a build that knows the `program_state` section gets
    // one, with the token registry it stands on, so the explorer meets an `invoke`, a program's
    // cells and its vault — the shape of the chain the feature is cut on, not chain 19's.
    let rpl2 = help.contains("--program-state-cell-fee");
    let tokens_json = dir.path().join("tokens.json");
    if rpl2 {
        std::fs::write(
            &tokens_json,
            r#"{"registration_fee":1000000000,"tokens":[],"mint_cap_per_day":10000000000000,"max_tokens":null,"burn_registration_fee":null,"bound_note_value":null}"#,
        )
        .unwrap();
        args.extend(
            ["--tokens", tokens_json.to_str().unwrap(), "--program-state-cell-fee", "10000000"]
                .into_iter()
                .map(str::to_string),
        );
    }
    run(bin, &args.iter().map(String::as_str).collect::<Vec<_>>());
    run(
        bin,
        &[
            "init",
            "--datadir",
            datadir.to_str().unwrap(),
            "--genesis",
            genesis.to_str().unwrap(),
        ],
    );
    let port = free_port();
    let url = format!("http://127.0.0.1:{port}");
    // 500 ms blocks: a bundle's anchor is valid for 256 blocks, so the wallet has about two
    // minutes to prove under the test profile before an honest transfer would be refused
    // (`time N is outside […]`). On a machine busy enough to prove slower than that,
    // `RAND_BLOCK_INTERVAL_MS` widens the window: 1000 gives four minutes, 1500 six. Keep it
    // under the 2 000 ms view timeout below, or the lone validator never commits a block.
    let block_interval = std::env::var("RAND_BLOCK_INTERVAL_MS").unwrap_or_else(|_| "500".into());
    let child = Command::new(bin)
        .args([
            "run",
            "--datadir",
            datadir.to_str().unwrap(),
            "--key",
            key.to_str().unwrap(),
            "--listen",
            "/ip4/127.0.0.1/tcp/0",
            "--rpc",
            &format!("127.0.0.1:{port}"),
            "--validator",
            "--no-mdns",
            "--block-interval-ms",
            &block_interval,
            "--view-timeout-ms",
            "2000",
        ])
        .env("RUST_LOG", "warn")
        .env(ALLOW_TEST_FRI.0, ALLOW_TEST_FRI.1)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn rand-node");
    let node = Node {
        child,
        url: url.clone(),
        dir,
        rpl2,
    };
    let client = reqwest::Client::new();
    let start = std::time::Instant::now();
    loop {
        if let Ok(r) = client
            .post(&url)
            .json(&json!({ "jsonrpc": "2.0", "id": 1, "method": "rand_getHead", "params": [] }))
            .send()
            .await
        {
            if r.status().is_success() {
                break;
            }
        }
        assert!(start.elapsed() < WAIT, "node did not start");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    (node, validator_addr, wallet_addr)
}

#[tokio::test]
async fn explorer_agrees_with_a_real_shielded_node() {
    // CI's `real-node` job sets RANDSCAN_REQUIRE_REAL_NODE so a lost variable fails the job
    // instead of passing it as a skip.
    let required = std::env::var_os("RANDSCAN_REQUIRE_REAL_NODE").is_some();
    let Ok(bin) = std::env::var("RAND_NODE_BIN") else {
        assert!(!required, "RANDSCAN_REQUIRE_REAL_NODE is set but RAND_NODE_BIN is not");
        eprintln!("skipping: RAND_NODE_BIN unset");
        return;
    };
    if std::env::var("DATABASE_URL").is_err() {
        assert!(!required, "RANDSCAN_REQUIRE_REAL_NODE is set but DATABASE_URL is not");
        eprintln!("skipping: DATABASE_URL unset");
        return;
    }
    let bin = PathBuf::from(bin);
    let cli = std::env::var("RAND_CLI")
        .map(PathBuf::from)
        .unwrap_or_else(|_| bin.with_file_name("rand"));
    assert!(
        cli.is_file(),
        "no wallet CLI at {}: set RAND_CLI",
        cli.display()
    );
    let client = reqwest::Client::new();
    let (node, validator_addr, _wallet_addr) = start_node(&bin, &cli).await;
    let wallet = node.dir.path().join("wallet.key.json");
    let wallet = wallet.to_str().unwrap();

    // The node must be producing blocks on its own before we ask it to do anything.
    let start = std::time::Instant::now();
    while rpc(&client, &node.url, "rand_getHead", json!([])).await["height"]
        .as_u64()
        .unwrap()
        < 2
    {
        assert!(start.elapsed() < WAIT, "single validator did not commit");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert_eq!(
        rpc(&client, &node.url, "rand_chainId", json!([])).await,
        json!(CHAIN_ID)
    );

    // Faucet mint into the wallet (validator-signed, no bundle), then a shielded transfer from
    // the wallet to a second wallet (a proved 2-in-2-out bundle).
    let out = run(
        &cli,
        &[
            "faucet", "--key", wallet, "--rpc", &node.url, "--amount", "50",
        ],
    );
    let mint_hash = submitted_hash(&out, "mint");
    let wallet2 = node.dir.path().join("wallet2.key.json");
    run(&cli, &["keygen", "--key", wallet2.to_str().unwrap()]);
    let wallet2_addr = run(&cli, &["address", "--key", wallet2.to_str().unwrap()])
        .trim()
        .to_string();
    // Since v0.5.10 `rand send` confirms first and refuses a non-terminal stdin without `--yes`.
    let mut send = vec!["send", &wallet2_addr, "1.25", "--key", wallet, "--rpc", &node.url];
    if run(&cli, &["send", "--help"]).contains("--yes") {
        send.push("--yes");
    }
    let out = run(&cli, &send);
    let transfer_hash = submitted_hash(&out, "transfer");

    let Some(live) = live_app(test_config(), &node.url).await else {
        return;
    };
    live.start();

    // The transfer: the explorer shows exactly the bundle's public fields, nothing more.
    let node_transfer = rpc(
        &client,
        &node.url,
        "rand_getTransaction",
        json!([transfer_hash]),
    )
    .await;
    assert!(
        node_transfer.is_object(),
        "transfer not committed: {node_transfer}"
    );
    let transfer = live
        .wait_for(
            &format!("/api/v1/transactions/{transfer_hash}"),
            WAIT,
            |t| t["kind"] == "transfer",
        )
        .await;
    let nb = &node_transfer["tx"]["bundle"];
    assert_eq!(transfer["has_bundle"], true);
    assert_eq!(transfer["bundle"]["anchor"], nb["anchor"]);
    assert_eq!(transfer["bundle"]["nullifiers"], nb["nullifiers"]);
    assert_eq!(transfer["bundle"]["commitments"], nb["commitments"]);
    assert_eq!(transfer["bundle"]["fee"], units(&nb["fee"]));
    assert_eq!(transfer["fee"], units(&nb["fee"]));
    assert_eq!(transfer["bundle"]["proof_len"], nb["proof_len"]);
    assert_eq!(transfer["bundle"]["time"], nb["time"]);
    assert_eq!(transfer["height"], node_transfer["height"]);
    assert!(
        transfer["amount"].is_null() && transfer.get("sender").is_none(),
        "{transfer}"
    );
    assert_eq!(transfer["chain_id"], CHAIN_ID);

    // The mint: no bundle, a public amount and commitment.
    let node_mint = rpc(
        &client,
        &node.url,
        "rand_getTransaction",
        json!([mint_hash]),
    )
    .await;
    let (status, _, mint) = call(
        &live.app,
        json_req(
            "GET",
            &format!("/api/v1/transactions/{mint_hash}"),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, 200, "{mint}");
    assert_eq!(mint["kind"], "mint");
    assert_eq!(mint["has_bundle"], false);
    assert_eq!(mint["amount"], "50000000000");
    assert_eq!(mint["cm"], node_mint["tx"]["action"]["cm"]);
    assert_eq!(mint["validator"], validator_addr);

    // Catch up to the node head, then compare what both sides publish.
    let head = rpc(&client, &node.url, "rand_getHead", json!([])).await["height"]
        .as_i64()
        .unwrap();
    live.wait_for("/api/v1/health", WAIT, |h| {
        h["indexer"]["current_height"].as_i64().unwrap_or(-1) >= head
    })
    .await;

    let node_block = rpc(&client, &node.url, "rand_getBlockByHeight", json!([head])).await;
    let (_, _, block) = call(
        &live.app,
        json_req("GET", &format!("/api/v1/blocks/{head}"), None, None),
    )
    .await;
    assert_eq!(block["hash"], node_block["hash"]);
    assert_eq!(block["parent"], node_block["parent"]);
    assert_eq!(block["state_root"], node_block["state_root"]);
    assert_eq!(block["proposer"], validator_addr);

    // The tree: one genesis note, one mint note, the transfer's four output slots (dummies
    // included, since chain 14's hidden-asset bundle); every leaf served.
    let tree = rpc(&client, &node.url, "rand_getTreeInfo", json!([])).await;
    assert_eq!(tree["next_index"], 6, "{tree}");
    let notes = live
        .wait_for("/api/v1/notes", WAIT, |n| n["pagination"]["total"] == 6)
        .await;
    let cm_out: Vec<&str> = nb["commitments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect();
    for cm in &cm_out {
        let (status, _, n) = call(
            &live.app,
            json_req("GET", &format!("/api/v1/notes/{cm}"), None, None),
        )
        .await;
        assert_eq!(status, 200, "{n}");
        assert_eq!(n["tx_hash"], transfer_hash);
    }
    let (_, _, genesis_note) =
        call(&live.app, json_req("GET", "/api/v1/notes/0", None, None)).await;
    assert!(genesis_note["tx_hash"].is_null(), "{genesis_note}");
    assert_eq!(notes["data"].as_array().unwrap().len(), 6);
    for nf in nb["nullifiers"].as_array().unwrap() {
        let (status, _, n) = call(
            &live.app,
            json_req(
                "GET",
                &format!("/api/v1/nullifiers/{}", nf.as_str().unwrap()),
                None,
                None,
            ),
        )
        .await;
        assert_eq!(status, 200, "{n}");
        assert_eq!(n["tx_hash"], transfer_hash);
    }

    let node_status = rpc(&client, &node.url, "rand_status", json!([])).await;
    let stats = live
        .wait_for("/api/v1/stats", WAIT, |s| {
            s["chain_id"] == CHAIN_ID && s["notes"] == 6
        })
        .await;
    assert_eq!(stats["nullifiers"], node_status["nullifiers"]);
    assert_eq!(stats["hc_bundle"], node_status["hc_bundle"]);
    // Chain 17+/18+: the auth guest and the tip's live gas prices come off `rand_status` as the
    // node serves them (`null` on a node or chain without), and the limits carry the genesis'
    // envelope format, auth guest and gas section. A chain-18 build got that genesis above.
    let null = json!(null);
    assert_eq!(stats["hc_auth"], *node_status.get("hc_auth").unwrap_or(&null), "{stats}");
    assert_eq!(stats["gas_prices"], *node_status.get("gas_prices").unwrap_or(&null), "{stats}");
    let node_limits = rpc(&client, &node.url, "rand_getLimits", json!([])).await;
    if node_limits["gas_metering"] == "circuit" {
        let l = &stats["limits"];
        assert_eq!(l["gas_metering"], "circuit", "{l}");
        assert_eq!(l["bundle_gas_limit"], 20479);
        assert_eq!(l["adjust_bps"], 1250);
        assert_eq!(l["envelope_bytes"], 1860);
        assert_eq!(l["hardening_v6"], true);
        assert_eq!(l["hc_auth"], node_status["hc_auth"]);
        assert!(l["hc_auth"].is_string(), "a v3 chain pins an auth guest: {l}");
        assert!(stats["gas_prices"]["gas_price"].is_string(), "{stats}");
        // The genesis prices are the floors; the tip's cannot be below them.
        let floor: u64 = node_limits["gas_price"].as_str().unwrap().parse().unwrap();
        let tip: u64 = stats["gas_prices"]["gas_price"].as_str().unwrap().parse().unwrap();
        assert!(tip >= floor, "{tip} < {floor}");
        // Under `hc_auth` every bundle carries an auth proof and every envelope is 1 860 B.
        assert_eq!(transfer["bundle"]["auth_commit"], nb["auth_commit"]);
        assert!(nb["auth_proof_bytes"].as_u64().unwrap() > 0, "{nb}");
        assert_eq!(transfer["bundle"]["auth_proof_len"], nb["auth_proof_bytes"]);
        assert_eq!(transfer["bundle"]["envelope_len"], json!([1860, 1860, 1860, 1860]));
    }
    assert_eq!(stats["validator_count"], 1);
    assert_eq!(stats["faucet"], true);
    assert_eq!(stats["symbol"], "RAND");
    let node_validators = rpc(&client, &node.url, "rand_getValidators", json!([])).await;
    let (_, _, validators) =
        call(&live.app, json_req("GET", "/api/v1/validators", None, None)).await;
    let list = validators.as_array().unwrap();
    assert_eq!(list[0]["address"], validator_addr);
    assert_eq!(list[0]["stake"], node_validators[0]["stake"]);
    assert_eq!(list[0]["active"], true);
    // The proposer earned the transfer's fee (and the deploy/call fees below, later).
    let rewards: u128 = list[0]["rewards"].as_str().unwrap().parse().unwrap();
    assert!(rewards >= units(&nb["fee"]).parse::<u128>().unwrap(), "{list:?}");

    let (status, _, found) = call(
        &live.app,
        json_req(
            "GET",
            &format!("/api/v1/search?q={}", cm_out[0]),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, 200, "{found}");
    assert_eq!(found[0]["type"], "note");
    let (status, _, gone) = call(
        &live.app,
        json_req(
            "GET",
            &format!("/api/v1/accounts/{validator_addr}"),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, 410, "{gone}");

    // Confidential call: deploy the private_payment guest, prove locally (test FRI profile),
    // submit with its input transcript, and compare the explorer's view with the node's.
    let program_json = node.dir.path().join("program.json");
    run(
        &cli,
        &[
            "program",
            "build",
            "--guest",
            "private_payment",
            "--arg",
            "100",
            "--out",
            program_json.to_str().unwrap(),
        ],
    );
    let out = run(
        &cli,
        &[
            "program",
            "deploy",
            program_json.to_str().unwrap(),
            "--rpc",
            &node.url,
            "--key",
            wallet,
        ],
    );
    let deploy_hash = submitted_hash(&out, "deploy");
    let program_id = out
        .lines()
        .find_map(|l| l.strip_prefix("program id: "))
        .and_then(|r| r.split(' ').next())
        .unwrap_or_else(|| panic!("no program id in deploy output: {out}"))
        .to_string();
    let out = run(
        &cli,
        &[
            "call",
            &program_id,
            "--input",
            "400",
            "--input",
            "250",
            "--input",
            "0",
            "--input",
            "0",
            "--rpc",
            &node.url,
            "--key",
            wallet,
        ],
    );
    let call_hash = submitted_hash(&out, "call");

    let node_receipt = rpc(&client, &node.url, "rand_getReceipt", json!([call_hash])).await;
    assert!(
        node_receipt.is_object(),
        "node has no receipt for {call_hash}: {node_receipt}"
    );
    let tx = live
        .wait_for(&format!("/api/v1/transactions/{call_hash}"), WAIT, |t| {
            t["receipt"].is_object()
        })
        .await;
    assert_eq!(tx["kind"], "call");
    assert_eq!(tx["program"], program_id);
    assert_eq!(tx["has_bundle"], true);
    assert!(tx["call_proof_len"].as_u64().unwrap_or(0) > 0, "{tx}");
    assert!(
        tx["input_envelope_len"].as_u64().unwrap_or(0) > 0,
        "the wallet publishes a transcript by default: {tx}"
    );
    let receipt = &tx["receipt"];
    assert_eq!(receipt["tier"], node_receipt["tier"]);
    assert_eq!(receipt["outputs"], node_receipt["outputs"]);
    assert_eq!(receipt["h_in"], node_receipt["h_in"]);
    assert_eq!(receipt["height"], node_receipt["height"]);
    assert!(
        receipt.get("effect").is_none(),
        "effect kind 1 is gone with the accounts: {receipt}"
    );

    let (_, _, deploy) = call(
        &live.app,
        json_req(
            "GET",
            &format!("/api/v1/transactions/{deploy_hash}"),
            None,
            None,
        ),
    )
    .await;
    assert_eq!(deploy["kind"], "deploy");
    assert_eq!(deploy["program"], program_id);
    let node_program = rpc(&client, &node.url, "rand_getProgram", json!([program_id])).await;
    let program = live
        .wait_for(&format!("/api/v1/programs/{program_id}"), WAIT, |p| {
            p["call_count"] == 1
        })
        .await;
    assert_eq!(program["code_hash"], node_program["code_hash"]);
    assert_eq!(program["words_len"], node_program["words_len"]);
    assert_eq!(program["deploy_tx"], deploy_hash);
    assert!(program.get("deployer").is_none(), "{program}");

    // Every leaf the node holds is indexed, including the two bundles that paid for the
    // deploy and the call (self-transfers of zero, four more commitments).
    let head = rpc(&client, &node.url, "rand_getHead", json!([])).await["height"]
        .as_i64()
        .unwrap();
    live.wait_for("/api/v1/health", WAIT, |h| {
        h["indexer"]["current_height"].as_i64().unwrap_or(-1) >= head
    })
    .await;
    let tree = rpc(&client, &node.url, "rand_getTreeInfo", json!([])).await;
    let stats = live
        .wait_for("/api/v1/stats", WAIT, |s| s["notes"] == tree["next_index"])
        .await;
    assert_eq!(stats["nullifiers"], tree["nullifiers"]);
    let (_, _, notes) = call(
        &live.app,
        json_req("GET", "/api/v1/notes?limit=1", None, None),
    )
    .await;
    assert_eq!(notes["pagination"]["total"], tree["next_index"]);

    let token_index = if node.rpl2 {
        Some(invoke_round_trip(&live, &client, &node, &cli, wallet, &wallet2_addr).await)
    } else {
        eprintln!("skipping the RPL-2 invoke round trip: this rand-node has no --program-state-cell-fee (pre-v0.6.8)");
        None
    };
    consumer_contract(&live, &client, &node, &mint_hash, &mint["cm"], token_index).await;
    drop(node);
}

/// RPL-2 on a chain with the `program_state` section: deploy the counter guest, register its
/// program token, and make ONE invoke that reads the counter's cell (absent), writes it to 1,
/// deposits 0.5 RAND into the vault, pays 0.2 of it to the second wallet and mints one unit of
/// the program token — three proofs. Then the explorer's view against the node's: the invoke
/// with its transition, a call's receipt, both payout leaves linked to it, the program's cells
/// and vault, the limits' `program_state` group and the supply's vault counters.
async fn invoke_round_trip(live: &LiveApp, client: &reqwest::Client, node: &Node, cli: &PathBuf, wallet: &str, payee: &str) -> i64 {
    let dir = node.dir.path();
    let counter_json = dir.join("counter.json");
    run(cli, &["program", "build", "--guest", "rpl2_counter", "--out", counter_json.to_str().unwrap()]);
    let out = run(cli, &["program", "deploy", counter_json.to_str().unwrap(), "--rpc", &node.url, "--key", wallet]);
    let counter = out
        .lines()
        .find_map(|l| l.strip_prefix("program id: "))
        .and_then(|r| r.split(' ').next())
        .unwrap_or_else(|| panic!("no program id in deploy output: {out}"))
        .to_string();
    let out = run(
        cli,
        &["token", "create", "--name", "Counter Share", "--symbol", "CTR", "--decimals", "0", "--program", &counter, "--rpc", &node.url, "--key", wallet],
    );
    // The wallet reports it as "submitted token registration <hash>".
    let register_hash = submitted_hash(&out, "token registration");
    let registered = live
        .wait_for(&format!("/api/v1/transactions/{register_hash}"), WAIT, |t| t["kind"] == "register_token")
        .await;
    let share_index = registered["asset_index"].as_i64().expect("the registration's index");
    assert_eq!(registered["token_action"]["authority"], "program", "{registered}");

    // The counter accepts exactly a transition that reads one cell and writes its first word
    // plus one; everything else rides along (`docs/cli.md`, "A stateful program").
    let cell_key = format!("01{}", "00".repeat(31));
    let transition = dir.join("step.json");
    std::fs::write(
        &transition,
        json!({
            "reads": [{ "key": cell_key, "value": "00".repeat(32) }],
            "writes": [{ "key": cell_key, "value": cell_key }],
            "deposit": { "rand": "500000000", "kind": "none" },
            "pays": [{ "asset": 0, "amount": "200000000", "to": payee }],
            "mints": [{ "asset": share_index, "amount": "1" }],
        })
        .to_string(),
    )
    .unwrap();
    let out = run(cli, &["program", "invoke", &counter, "--transition", transition.to_str().unwrap(), "--rpc", &node.url, "--key", wallet]);
    let invoke_hash = submitted_hash(&out, "invoke");

    let node_tx = rpc(client, &node.url, "rand_getTransaction", json!([invoke_hash])).await;
    assert_eq!(node_tx["tx"]["action"]["kind"], "invoke", "{node_tx}");
    let node_receipt = rpc(client, &node.url, "rand_getReceipt", json!([invoke_hash])).await;
    assert!(node_receipt.is_object(), "node has no receipt for {invoke_hash}: {node_receipt}");
    let tx = live
        .wait_for(&format!("/api/v1/transactions/{invoke_hash}"), WAIT, |t| t["receipt"].is_object())
        .await;
    assert_eq!(tx["kind"], "invoke");
    assert_eq!(tx["program"], counter);
    assert_eq!(tx["call_proof_len"], node_tx["tx"]["action"]["proof_len"]);
    assert_eq!(tx["bundle"]["burn_r"], "500000000", "the RAND the transition deposited is the bundle's burn: {tx}");
    let t = &tx["transition"];
    let nt = &node_tx["tx"]["action"]["transition"];
    assert_eq!(t["inflow"], "none");
    assert_eq!(t["reads"], nt["reads"]);
    assert_eq!(t["writes"], nt["writes"]);
    assert_eq!(t["writes"][0]["value"], cell_key);
    assert_eq!(t["pays"].as_array().map(Vec::len), Some(1));
    assert_eq!(t["mints"].as_array().map(Vec::len), Some(1));
    for side in ["pays", "mints"] {
        let (ours, theirs) = (&t[side][0], &nt[side][0]);
        assert_eq!(ours["cm"], theirs["cm"], "{side}: {tx}");
        assert_eq!(ours["recipient"], theirs["recipient"]);
        assert_eq!(ours["amount"], units(&theirs["amount"]));
        assert_eq!(ours["time"], node_tx["tx"]["bundle"]["time"], "a payout note's time is the bundle's");
    }
    assert_eq!(t["pays"][0]["recipient"], payee);
    assert_eq!(t["mints"][0]["asset"], share_index);
    assert_eq!(tx["receipt"]["outputs"], node_receipt["outputs"]);
    assert_eq!(tx["receipt"]["outputs"][0], 1, "the counter's new count");

    // Both payout leaves are the chain's, linked to the invoke, among its notes.
    let head = rpc(client, &node.url, "rand_getHead", json!([])).await["height"].as_i64().unwrap();
    live.wait_for("/api/v1/health", WAIT, |h| h["indexer"]["current_height"].as_i64().unwrap_or(-1) >= head).await;
    let tree = rpc(client, &node.url, "rand_getTreeInfo", json!([])).await;
    live.wait_for("/api/v1/stats", WAIT, |s| s["notes"] == tree["next_index"]).await;
    for cm in [t["pays"][0]["cm"].as_str().unwrap(), t["mints"][0]["cm"].as_str().unwrap()] {
        let (status, _, n) = call(&live.app, json_req("GET", &format!("/api/v1/notes/{cm}"), None, None)).await;
        assert_eq!(status, 200, "{n}");
        assert_eq!(n["tx_hash"], invoke_hash, "{n}");
    }
    let (_, _, env) = call(&live.app, json_req("GET", &format!("/api/v1/transactions/{invoke_hash}/envelopes"), None, None)).await;
    assert_eq!(env["notes"].as_array().map(Vec::len), Some(6), "four slots and two payouts: {env}");

    // The program: counted as an invoke, its cell and its vault as the node serves them.
    let program = live
        .wait_for(&format!("/api/v1/programs/{counter}"), WAIT, |p| p["invoke_count"] == 1)
        .await;
    assert_eq!(program["call_count"], 0);
    assert_eq!(program["recent_calls"][0]["hash"], invoke_hash);
    let node_cells = rpc(client, &node.url, "rand_getProgramCells", json!([counter, { "limit": 10 }])).await;
    let node_vault = rpc(client, &node.url, "rand_getProgramVault", json!([counter])).await;
    assert_eq!(program["program_state"]["cells"], node_cells["cells"]);
    assert_eq!(program["program_state"]["cells"][0], json!({ "key": cell_key, "value": cell_key }));
    assert_eq!(program["program_state"]["vault"], node_vault);
    assert_eq!(program["program_state"]["vault"][0], json!({ "asset": 0, "amount": "300000000" }), "0.5 in, 0.2 out");
    let (status, _, page) = call(&live.app, json_req("GET", &format!("/api/v1/programs/{counter}/cells?limit=1"), None, None)).await;
    assert_eq!(status, 200, "{page}");
    assert_eq!(page["cells"], node_cells["cells"]);

    // The limits' group and the supply's vault counters, both the node's own figures.
    let node_limits = rpc(client, &node.url, "rand_getLimits", json!([])).await;
    let stats = live.wait_for("/api/v1/stats", WAIT, |s| s["limits"]["program_state"].is_object()).await;
    assert_eq!(stats["limits"]["program_state"], node_limits["program_state"]);
    assert_eq!(stats["limits"]["program_state"]["cell_fee"], "10000000");
    let node_supply = rpc(client, &node.url, "rand_getSupply", json!([])).await;
    let supply = live.wait_for("/api/v1/supply", WAIT, |s| s["program_rand_held"] == json!("300000000")).await;
    assert_eq!(supply["program_rand_out"], units(&node_supply["program_rand_out"]));
    assert_eq!(supply["program_rand_out"], "200000000");
    assert_eq!(supply["invariant_holds"], true, "{supply}");
    share_index
}

/// The REST contract two sites read with no tests of their own, checked against what this real
/// node made of the chain above. Every message names the consumer file:
///
/// - zusd.money `src/lib/balance-sheet.mjs` (rendered by `BalanceSheet.astro`): `GET /bridge` (`enabled`, `mint_paused`),
///   `GET /bridge/assets` (`index`, `chain`, `symbol`, `locked`), `GET /tokens/{index}`
///   (`total_supply`);
/// - randprotocol.org `src/components/BridgeReserves.astro`: the same three, plus each row's
///   `minted_today` / `mint_cap_per_day`;
/// - randprotocol.org `src/scripts/balance.js`: `GET /envelopes?from_leaf=&limit=` (`notes`,
///   `total_leaves`, `next_leaf`; per note `leaf_index`, `cm`, `height`, `tx_hash`, `envelope`,
///   `public`).
///
/// Also the stats and supply objects, which carry fields the node grows: `limits.fee_rules` and
/// `base_fees_burned` (fullnode `feat/fee-feedback`, unreleased) pass through when the node serves
/// them and are not required when it does not.
async fn consumer_contract(
    live: &LiveApp,
    client: &reqwest::Client,
    node: &Node,
    mint_hash: &str,
    mint_cm: &Value,
    token_index: Option<i64>,
) {
    const ZUSD: &str = "zusd.money src/lib/balance-sheet.mjs (via BalanceSheet.astro)";
    const RESERVES: &str = "randprotocol.org src/components/BridgeReserves.astro";
    const BALANCE: &str = "randprotocol.org src/scripts/balance.js";
    let get = |path: String| async move { call(&live.app, json_req("GET", &path, None, None)).await };
    let is_units = |v: &Value| v.as_str().is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()));

    // Settle on the node's head first, so the bridge/token/supply caches have been refreshed.
    let head = rpc(client, &node.url, "rand_getHead", json!([])).await["height"].as_i64().unwrap();
    live.wait_for("/api/v1/health", WAIT, |h| h["indexer"]["current_height"].as_i64().unwrap_or(-1) >= head).await;

    // --- /bridge ---------------------------------------------------------------------------
    let node_bridge = rpc(client, &node.url, "rand_getBridgeState", json!([])).await;
    let (status, _, bridge) = get("/api/v1/bridge".into()).await;
    assert_eq!(status, 200, "{ZUSD} and {RESERVES} fetch /bridge: {bridge}");
    assert!(bridge["enabled"].is_boolean(), "{ZUSD} reads bridge.enabled as a boolean: {bridge}");
    assert!(bridge["mint_paused"].is_boolean(), "{ZUSD} reads bridge.mint_paused as a boolean: {bridge}");
    let enabled = node_bridge["enabled"].as_bool().unwrap_or(false);
    assert_eq!(bridge["enabled"], enabled, "{ZUSD}: /bridge.enabled must be the node's: node {node_bridge}");
    if !enabled {
        assert_eq!(bridge["mint_paused"], false, "{ZUSD}: a chain without a bridge reports no pause: {bridge}");
    }

    // --- /bridge/assets ----------------------------------------------------------------------
    let (status, _, assets) = get("/api/v1/bridge/assets".into()).await;
    assert_eq!(status, 200, "{ZUSD} and {RESERVES} fetch /bridge/assets: {assets}");
    let rows = assets.as_array().unwrap_or_else(|| panic!("{ZUSD} iterates /bridge/assets as an array: {assets}"));
    if !enabled {
        assert!(rows.is_empty(), "{ZUSD}: a chain without a bridge lists no backings ([]): {assets}");
    }
    for a in rows {
        assert!(a["index"].is_i64(), "{ZUSD} groups by row.index: {a}");
        assert!(a["chain"].is_i64(), "{ZUSD} keys rows by row.chain: {a}");
        assert!(a["symbol"].is_string() || a["symbol"].is_null(), "{ZUSD} filters on row.symbol (string or null): {a}");
        if a["symbol"].is_string() {
            // Only symbol'd rows are summed, with BigInt(row.locked): null there would throw.
            assert!(is_units(&a["locked"]), "{ZUSD} sums BigInt(row.locked): {a}");
            assert!(is_units(&a["minted_today"]) && is_units(&a["mint_cap_per_day"]), "{RESERVES} renders minted_today / mint_cap_per_day: {a}");
        }
    }

    // --- /tokens/{index} ---------------------------------------------------------------------
    // The sites ask for each index the bridge rows name and treat a non-2xx as "unknown"; an
    // index nobody registered must be a 404, never a 200 with a made-up supply.
    let (status, _, missing) = get("/api/v1/tokens/987654".into()).await;
    assert_eq!(status, 404, "{ZUSD} rejects a non-ok /tokens/{{index}}: {missing}");
    match token_index {
        Some(index) => {
            let node_token = rpc(client, &node.url, "rand_getToken", json!([index])).await;
            let token = live
                .wait_for(&format!("/api/v1/tokens/{index}"), WAIT, |t| t["total_supply"] == json!(units(&node_token["total_supply"])))
                .await;
            assert!(is_units(&token["total_supply"]), "{ZUSD} and {RESERVES} take BigInt(token.total_supply): {token}");
            assert_eq!(token["index"], index, "{RESERVES} reads token.index: {token}");
            assert!(token["symbol"].is_string(), "{RESERVES} reads token.symbol: {token}");
            assert_eq!(token["total_supply"], "1", "{ZUSD}: the one unit the invoke minted: {token}");
        }
        None => eprintln!("skipping /tokens/{{index}} for a registered token: no token registry on this build (pre-RPL-2)"),
    }

    // --- /envelopes ----------------------------------------------------------------------------
    // balance.js walks the tree a page at a time from leaf 0 with limit=1000, follows next_leaf
    // until it is null, and reports progress against total_leaves.
    let tree = rpc(client, &node.url, "rand_getTreeInfo", json!([])).await;
    let leaves = tree["next_index"].as_i64().unwrap();
    let page = live
        .wait_for("/api/v1/envelopes?from_leaf=0&limit=1000", WAIT, |p| p["total_leaves"] == leaves)
        .await;
    assert!(page["next_leaf"].is_null(), "{BALANCE} stops when next_leaf is null (every leaf on one page): {page}");
    let notes = page["notes"].as_array().unwrap_or_else(|| panic!("{BALANCE} iterates page.notes: {page}"));
    assert_eq!(notes.len() as i64, leaves, "{BALANCE}: one row per leaf: {page}");
    for (i, n) in notes.iter().enumerate() {
        assert_eq!(n["leaf_index"], i as i64, "{BALANCE} sorts on n.leaf_index, oldest first: {n}");
        assert!(n["cm"].is_string(), "{BALANCE} opens n.cm: {n}");
        assert!(n["height"].is_i64(), "{BALANCE} shows n.height: {n}");
        let o = n.as_object().unwrap();
        for key in ["tx_hash", "envelope", "public"] {
            assert!(o.contains_key(key), "{BALANCE} destructures n.{key} (null allowed, never missing): {n}");
        }
        assert!(n["tx_hash"].is_string() || n["tx_hash"].is_null(), "{BALANCE}: n.tx_hash: {n}");
        assert!(n["envelope"].is_object() || n["envelope"].is_null(), "{BALANCE} passes n.envelope to the opener: {n}");
    }
    let mint_row = notes
        .iter()
        .find(|n| n["cm"] == *mint_cm)
        .unwrap_or_else(|| panic!("{BALANCE}: the faucet mint's leaf is served: {page}"));
    assert_eq!(mint_row["tx_hash"], mint_hash, "{BALANCE}: the mint's leaf names its transaction: {mint_row}");
    assert!(
        notes.iter().any(|n| n["envelope"].is_object() && n["tx_hash"].is_string()),
        "{BALANCE}: at least one leaf carries an envelope to open: {page}"
    );
    // Paging: a short page points at the next leaf, and that page starts there (needs a third leaf).
    if leaves > 2 {
        let (status, _, first) = get("/api/v1/envelopes?from_leaf=0&limit=2".into()).await;
        assert_eq!(status, 200, "{BALANCE}: {first}");
        assert_eq!(first["next_leaf"], 2, "{BALANCE} follows page.next_leaf: {first}");
        let (_, _, second) = get("/api/v1/envelopes?from_leaf=2&limit=2".into()).await;
        assert_eq!(second["notes"][0]["leaf_index"], 2, "{BALANCE}: the next page starts at next_leaf: {second}");
        assert_eq!(second["total_leaves"], leaves, "{BALANCE} reports progress against total_leaves: {second}");
    }

    // --- /stats --------------------------------------------------------------------------------
    let node_limits = rpc(client, &node.url, "rand_getLimits", json!([])).await;
    let stats = live.wait_for("/api/v1/stats", WAIT, |s| s["limits"].is_object()).await;
    let so = stats.as_object().unwrap();
    assert!(so.contains_key("gas_prices"), "stats.gas_prices is served (null allowed): {stats}");
    if node_limits["gas_metering"] == "circuit" {
        assert!(is_units(&stats["gas_prices"]["gas_price"]) && is_units(&stats["gas_prices"]["byte_price"]), "a gas-section chain's tip prices: {stats}");
    }
    match node_limits.get("fee_rules") {
        // Only the flags the explorer models (`FeeRules`) are compared, so a flag a later node
        // adds beside them does not fail the gate; a `null` group must stay `null`.
        Some(Value::Null) => assert!(stats["limits"]["fee_rules"].is_null(), "limits.fee_rules is null as the node serves it: {stats}"),
        Some(rules) => {
            for flag in ["burn_base", "subsidy_net_of_fees", "burn_floor"] {
                assert_eq!(
                    stats["limits"]["fee_rules"][flag],
                    *rules.get(flag).unwrap_or(&json!(false)),
                    "limits.fee_rules.{flag} passes through as the node serves it: {stats}"
                );
            }
        }
        None => {
            eprintln!("note: this rand-node predates rand_getLimits.fee_rules (fee feedback); only its absence is tolerated");
            assert!(stats["limits"]["fee_rules"].is_null(), "{stats}");
        }
    }

    // --- /supply -------------------------------------------------------------------------------
    // Every amount the explorer serves is a decimal string and equals the node's own figure; the
    // node may serve more than the explorer models (`registration_fees_burned`, `faucet_epoch`,
    // …) and that must not stop the refresh. Wait on a supply at least as new as the head above.
    // Both sides are read until they describe the same height (the node commits every
    // `RAND_BLOCK_INTERVAL_MS`: 500 ms by default, 1000 ms in CI).
    let start = std::time::Instant::now();
    let (supply, node_supply) = loop {
        let (status, _, ours) = get("/api/v1/supply".into()).await;
        let theirs = rpc(client, &node.url, "rand_getSupply", json!([])).await;
        if status == 200 && ours["height"] == theirs["height"] {
            break (ours, theirs);
        }
        assert!(start.elapsed() < WAIT, "/api/v1/supply never caught the node's height: {ours} vs {theirs}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    for (k, v) in supply.as_object().unwrap() {
        match v {
            Value::String(_) => {
                assert!(is_units(v), "supply.{k} is a decimal string: {supply}");
                if let Some(theirs) = node_supply.get(k) {
                    assert_eq!(*v, json!(units(theirs)), "supply.{k} is the node's figure: node {node_supply}");
                }
            }
            Value::Null => assert!(node_supply.get(k).is_none_or(Value::is_null), "supply.{k} dropped: node {node_supply}"),
            _ => {}
        }
    }
    match node_supply.get("base_fees_burned") {
        Some(b) => assert_eq!(supply["base_fees_burned"], json!(units(b)), "base_fees_burned passes through: {supply}"),
        None => eprintln!("note: this rand-node predates rand_getSupply.base_fees_burned (fee feedback); the indexer unit test covers it"),
    }
    assert_eq!(supply["invariant_holds"], true, "{supply}");
    eprintln!(
        "consumer contract: bridge enabled={enabled}, {} backing rows, token {:?}, {leaves} envelope leaves, fee_rules {}, base_fees_burned {}",
        rows.len(),
        token_index,
        if node_limits.get("fee_rules").is_some() { "served" } else { "absent" },
        supply["base_fees_burned"],
    );
}
