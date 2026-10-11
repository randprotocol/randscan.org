//! An in-process stand-in for `rand-node`'s JSON-RPC on chain 14 (the hidden-asset bundle, the
//! RPL token standard, bridge hardening; `fullnode/docs/rpc.md`), scripted from the test: the
//! chain id, the committed blocks, the tree leaves, the token registry and the validator register
//! can be swapped at any time, which is how the tests simulate a hard fork under a running
//! indexer.

#![allow(dead_code)]

use axum::{extract::State, routing::post, Json, Router};
use randscan_core::notecommit;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

pub const VALIDATOR: &str = "2nRdFChBXRmKoe2sQE3ZYDzvdg53QmBZJJ9iweY7hk1v";
pub const VALIDATOR_B: &str = "ByDkxsEfDCR5DrmDufKftvcRsgvufypnZ4SgDQzJAQ7Z";
pub const SHIELDED_ADDR: &str = "rand1q9fexampleexampleexampleexampleexampleexampleexample";

/// A well-formed `rand1…` address a real `pk_from_address` decodes: 40 bytes of `seed`'s hash
/// repeated, base58 over `rand1`. Distinct seeds give distinct (and so distinct-commitment)
/// addresses.
pub fn shielded_address(seed: &str) -> String {
    let digest = Sha256::digest(seed.as_bytes());
    let mut raw = digest.to_vec();
    raw.extend_from_slice(&digest[..8]);
    format!("rand1{}", bs58::encode(&raw).into_string())
}

/// 64 lowercase hex characters derived from `seed`.
/// What the mock's `rand_getVersion` reports as the node's commit.
pub const MOCK_GIT_SHA: &str = "b3c594cd5872bbc132cfafcd7314614436e6131a";

/// The digest the mock reports for a program deployed with a public input: its `public_digest`,
/// and the `h_pub` of every receipt of a call to it.
pub fn public_digest(program: &str) -> String {
    h(&format!("public-{program}"))
}

pub fn h(seed: &str) -> String {
    hex::encode(Sha256::digest(seed.as_bytes()))
}

/// The commitment a real node would append for an RPL mint's note — used both to build a
/// realistic `token_mint`/`register_token` fixture and, by the indexer under test, to link that
/// leaf back to its transaction (`randscan_core::notecommit::mint_commitment`).
pub fn mint_note_cm(recipient: &str, amount: u64, index: u32, time: u32, r_hex: &str) -> String {
    notecommit::mint_commitment_hex(recipient, amount, index, time, r_hex)
        .expect("a well-formed shielded address")
}

/// A register entry as the S2 node serves it.
#[derive(Clone)]
pub struct MockValidator {
    pub address: String,
    pub stake: String,
    pub rewards: String,
    pub pending: Vec<(u64, String)>,
    pub active: bool,
}

impl MockValidator {
    pub fn new(address: &str, stake: &str) -> Self {
        MockValidator {
            address: address.into(),
            stake: stake.into(),
            rewards: "0".into(),
            pending: vec![],
            active: true,
        }
    }
}

pub struct MockChain {
    pub chain_id: u64,
    /// Distinguishes two genesis blocks with the same chain id (a re-created testnet).
    pub salt: String,
    pub blocks: Vec<Value>,
    pub view: u64,
    pub validators: Vec<MockValidator>,
    /// Serve the register in the S3-branch shape (`{address, stake, rewards: number}`) and
    /// answer "unknown method" to `rand_getEpoch` / `rand_getSupply`.
    pub pre_s2: bool,
    /// The commitment tree: (cm, height), leaf index = position.
    pub leaves: Vec<(String, u64)>,
    /// Every JSON-RPC method name the indexer called, in order.
    pub calls: Vec<String>,
    /// The RPL token registry (`rand_getTokens` rows, in `TokenInfo` shape); `[]` on a chain
    /// without tokens.
    pub tokens: Vec<Value>,
    pub registration_fee: u64,
    pub next_token_index: u32,
    /// Bridge governance (bridge hardening B1/B3/B4); defaults match a chain predating them.
    pub mint_paused: bool,
    pub pause_nonce: u64,
    pub list_nonce: u64,
    pub pq_guardians: Vec<String>,
    /// RPL-2: whether the genesis has a `program_state` section. With it `rand_getLimits`
    /// reports the group, `rand_getSupply` the vault counters, and the program-state methods
    /// answer; without it they answer `{"enabled": false}`, as a v0.6.8 node on chain 18 would.
    pub program_state: bool,
    /// What `rand_getPeers` answers (`{peer_id, addrs, connected_secs}` rows); none by default.
    pub peers: Vec<Value>,
}

impl MockChain {
    /// A chain with only its genesis block and one genesis deposit note.
    pub fn new(chain_id: u64, salt: &str) -> Self {
        let mut c = MockChain {
            chain_id,
            salt: salt.to_string(),
            blocks: Vec::new(),
            view: 0,
            validators: vec![MockValidator::new(VALIDATOR, "100000000000000")],
            pre_s2: false,
            leaves: vec![(h(&format!("genesis-note-{chain_id}-{salt}")), 0)],
            calls: Vec::new(),
            tokens: Vec::new(),
            registration_fee: 1_000_000_000,
            next_token_index: 1,
            mint_paused: false,
            pause_nonce: 0,
            list_nonce: 0,
            pq_guardians: vec![],
            program_state: false,
            peers: Vec::new(),
        };
        c.push_block(vec![]);
        c
    }

    pub fn head(&self) -> &Value {
        self.blocks.last().expect("genesis")
    }

    /// Register a token row (`rand_getTokens`' shape) and bump `next_token_index` past it.
    pub fn push_token(&mut self, row: Value) {
        let index = row["index"].as_u64().unwrap_or(0) as u32;
        self.next_token_index = self.next_token_index.max(index + 1);
        self.tokens.push(row);
    }

    /// Append a block holding `txs` (already built with [`tx`]); returns its hash. Every
    /// commitment the block's bundles carry becomes a tree leaf, as does the one chain-computed
    /// note a `mint`, a `bridge_attest` deposit, a `token_mint` or a `register_token` initial
    /// mint appends (chain 14: no more `asset_bundle`, one bundle per transaction).
    pub fn push_block(&mut self, txs: Vec<Value>) -> String {
        let height = self.blocks.len() as u64;
        let parent = if height == 0 {
            "0".repeat(64)
        } else {
            self.head()["hash"].as_str().unwrap().to_string()
        };
        let hash = h(&format!("block-{}-{}-{}", self.chain_id, self.salt, height));
        self.view = height * 2;
        for t in &txs {
            if let Some(cms) = t["bundle"]["commitments"].as_array() {
                for cm in cms {
                    self.leaves.push((cm.as_str().unwrap().to_string(), height));
                }
            }
            let a = &t["action"];
            match a["kind"].as_str() {
                Some("mint") => {
                    if let Some(cm) = a["cm"].as_str() {
                        self.leaves.push((cm.to_string(), height));
                    }
                }
                Some("bridge_attest") => {
                    if let Some(cm) = a["commitment"].as_str() {
                        self.leaves.push((cm.to_string(), height));
                    }
                }
                Some("token_mint") => {
                    let cm = mint_note_cm(
                        a["recipient"].as_str().unwrap(),
                        a["amount"].as_u64().unwrap(),
                        a["asset"].as_u64().unwrap() as u32,
                        a["time"].as_u64().unwrap() as u32,
                        a["r"].as_str().unwrap(),
                    );
                    self.leaves.push((cm, height));
                }
                Some("register_token") if a["initial"].is_object() => {
                    let m = &a["initial"];
                    let cm = mint_note_cm(
                        m["recipient"].as_str().unwrap(),
                        m["amount"].as_u64().unwrap(),
                        a["index"].as_u64().unwrap() as u32,
                        m["time"].as_u64().unwrap() as u32,
                        m["r"].as_str().unwrap(),
                    );
                    self.leaves.push((cm, height));
                }
                // RPL-2: one chain-computed note per payout, pays then mints, after the bundle's
                // four; the node renders each payout's `cm` itself.
                Some("invoke") => {
                    for side in ["pays", "mints"] {
                        for p in a["transition"][side].as_array().into_iter().flatten() {
                            self.leaves
                                .push((p["cm"].as_str().unwrap().to_string(), height));
                        }
                    }
                }
                _ => {}
            }
        }
        self.blocks.push(json!({
            "hash": hash,
            "height": height,
            "view": self.view,
            "parent": parent,
            "proposer": VALIDATOR,
            "timestamp_ms": 1_789_000_000_000u64 + height * 1000,
            "tx_root": h(&format!("txroot-{height}")),
            "state_root": h(&format!("state-{}-{}-{height}", self.chain_id, self.salt)),
            "justify_view": self.view.saturating_sub(1),
            "tx_count": txs.len(),
            "transactions": txs,
        }));
        hash
    }

    fn nullifier_count(&self) -> usize {
        self.blocks
            .iter()
            .flat_map(|b| b["transactions"].as_array().cloned().unwrap_or_default())
            .map(|t| if t["bundle"].is_object() { 4 } else { 0 })
            .sum()
    }

    /// Every committed transaction with its block height and its index in the block.
    fn committed(&self) -> impl Iterator<Item = (u64, usize, &Value)> + '_ {
        self.blocks.iter().flat_map(|b| {
            let height = b["height"].as_u64().unwrap_or(0);
            b["transactions"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
                .map(move |(i, t)| (height, i, t))
        })
    }

    fn dispatch(&mut self, method: &str, params: &Value) -> Result<Value, (i64, String)> {
        self.calls.push(method.to_string());
        let p = |i: usize| params.get(i).cloned().unwrap_or(Value::Null);
        Ok(match method {
            "rand_chainId" => json!(self.chain_id),
            "rand_tokenInfo" => json!({ "symbol": "RAND", "decimals": 9 }),
            "rand_getHead" => {
                let head = self.head();
                json!({ "height": head["height"], "hash": head["hash"], "view": self.view })
            }
            "rand_status" | "rand_syncStatus" => {
                let head = self.head();
                json!({
                    "height": head["height"], "head_hash": head["hash"], "view": self.view,
                    "high_qc_view": self.view.saturating_sub(1), "syncing": false,
                    "sync_target": head["height"], "peer_count": 0, "mempool_size": 0,
                    "is_validator": false, "faucet": true, "confidential": true,
                    "fri_profile": "test", "programs": 0,
                    "notes": self.leaves.len(), "nullifiers": self.nullifier_count(),
                    "tree_root": h(&format!("root-{}", self.leaves.len())),
                    "hc_bundle": h("hc_bundle"),
                    // Chain 17+: the auth guest; chain 18+: the tip's live gas prices, moved one
                    // 12.5% step above the genesis prices the limits below report.
                    "hc_auth": if self.pre_s2 { Value::Null } else { json!(h("hc_auth")) },
                    "gas_prices": if self.pre_s2 { Value::Null } else { json!({ "gas_price": "112", "byte_price": "900" }) },
                    "address": null,
                    "peer_id": "12D3KooWmockmockmockmockmockmockmockmockmock"
                })
            }
            "rand_getBlockByHeight" => {
                let height = p(0).as_u64().ok_or((-32602, "height".to_string()))? as usize;
                self.blocks.get(height).cloned().unwrap_or(Value::Null)
            }
            "rand_getBlockByHash" => {
                let hash = p(0).as_str().unwrap_or("").to_ascii_lowercase();
                self.blocks
                    .iter()
                    .find(|b| b["hash"] == hash)
                    .cloned()
                    .unwrap_or(Value::Null)
            }
            "rand_getCommitments" => {
                let from = p(0).as_u64().ok_or((-32602, "from_index".to_string()))? as usize;
                let limit = p(1).as_u64().unwrap_or(1000).min(1000) as usize;
                json!(self
                    .leaves
                    .iter()
                    .enumerate()
                    .skip(from)
                    .take(limit)
                    .map(|(i, (cm, height))| json!({
                        "index": i, "cm": cm, "height": height,
                        "envelope": { "kem_ct": "00", "to_receiver": "00", "to_sender": "00", "body": "00" }
                    }))
                    .collect::<Vec<_>>())
            }
            "rand_getTreeInfo" => json!({
                "next_index": self.leaves.len(), "root": h(&format!("root-{}", self.leaves.len())),
                "nullifiers": self.nullifier_count()
            }),
            "rand_getValidators" if self.pre_s2 => json!(self
                .validators
                .iter()
                .map(|v| json!({ "address": v.address, "stake": v.stake, "rewards": v.rewards.parse::<u64>().unwrap_or(0) }))
                .collect::<Vec<_>>()),
            "rand_getValidators" => json!(self
                .validators
                .iter()
                .map(|v| json!({
                    "address": v.address, "stake": v.stake, "rewards": v.rewards,
                    "pending": v.pending.iter().map(|(e, a)| json!({ "release_epoch": e, "amount": a })).collect::<Vec<_>>(),
                    "payout": SHIELDED_ADDR, "nonce": 0, "active": v.active
                }))
                .collect::<Vec<_>>()),
            "rand_getEpoch" if !self.pre_s2 => {
                let height = self.head()["height"].as_u64().unwrap();
                json!({ "epoch": height / 1000, "epoch_blocks": 1000,
                        "next_set": self.validators.iter().filter(|v| v.active).map(|v| v.address.clone()).collect::<Vec<_>>() })
            }
            "rand_getSupply" if !self.pre_s2 => {
                let mut s = json!({
                    "height": self.head()["height"], "genesis_deposited": "1000000000000", "genesis_staked": "100000000000000",
                    "faucet_minted": "100000000000", "withdraw_deposited": "0", "fees_paid": "3000000", "burned": "0",
                    "pool_value": "1099997000000", "register_total": "100000003000000", "total_supply": "101100000000000",
                    "invariant_holds": true,
                    // Fee feedback (fullnode `feat/fee-feedback`): `"0"` on a chain without
                    // `fees.burn_base`. Kept so this mock stays a superset of the live shape.
                    "base_fees_burned": "0"
                });
                // RPL-2 (a v0.6.8 node): what invokes paid out of vaults and what vaults hold;
                // "0" on a chain without the section.
                if self.program_state {
                    s["program_rand_out"] = json!("300");
                    s["program_rand_held"] = json!("700");
                }
                s
            }
            // The shape (and, critically, the *encoding*) is copied verbatim from the node's own
            // pinned test, `bridge_state_reports_guardians_emitters_and_the_registry`
            // (crates/randprotocol-node/src/rpc.rs): `asset_json` sends `locked`/
            // `minted_today`/`mint_cap_per_day` as JSON **numbers**, not decimal strings (unlike
            // the token RPC's `backing_json` for the same conceptual fields — see
            // `push_token`'s zUSD fixture in the test itself). `next_index` is gone from
            // `rand_getBridgeState` since the bridge's own registry no longer predicts an index.
            // The emitters, the replay floor and rules v2 are chain 19's genesis as a v0.6.7
            // node serves it: the Ethereum, BSC and Tron endpoints redeployed on 2026-09-30,
            // Solana's unchanged, each redeployed endpoint's floor past the operator's
            // sequence-0 lock and Solana's past every lock chain 18 minted.
            "rand_getBridgeState" => json!({
                "enabled": true, "emitter": "01".repeat(32),
                "emitters": {
                    "2": "0000000000000000000000007af6b17047c1db6cb54347fdea45cf9179075bfa",
                    "3": "0000000000000000000000007af6b17047c1db6cb54347fdea45cf9179075bfa",
                    "4": "0000000000000000000000006410797df959987a5baf65b5fab97edeb34d5163",
                    "5": "d3e58f1e9317bbc3c69b63fadff558ea82ba5d00765f1f1e483d705d209b413a",
                },
                "guardian_set_index": 0, "guardians": ["aa".repeat(20)],
                "pq_guardians": self.pq_guardians, "mint_paused": self.mint_paused,
                "pause_nonce": self.pause_nonce, "list_nonce": self.list_nonce,
                "pause_key": "dd".repeat(1312), "registration_fee": self.registration_fee,
                "burn_sequence": 1,
                "rotation_nonce": 0,
                // Audit v6 (BRG-19, after v0.6.7): the window's count and headroom, on the
                // group and on the first backing row; the other rows are an older node's.
                "rules_v2": { "global_mint_cap_per_window": "400000000000", "cap_window_secs": 86400,
                              "global_minted_in_window": "1000", "global_mint_headroom": "399999999000" },
                "min_inbound_sequence": { "2": 1, "3": 1, "4": 1, "5": 4 },
                "assets": [
                    { "index": 1, "chain": 2, "token": "cc".repeat(32), "asset_id": h("asset-1"),
                      "decimals": 8, "locked": 600,
                      "mint_cap_per_day": 100_000u64 * 100_000_000, "minted_today": 1_000, "mint_day": 0,
                      "minted_in_window": "1000", "mint_window_secs": 86_400, "mint_headroom": "999000" },
                    // Chain 14's shape: one bridged token, several backings. This second coin of
                    // token 1 was never deposited or redeemed.
                    { "index": 1, "chain": 3, "token": "ce".repeat(32), "asset_id": h("asset-1-bsc"),
                      "decimals": 18, "locked": 0,
                      "mint_cap_per_day": 100_000u64 * 100_000_000, "minted_today": 0, "mint_day": 0 },
                    { "index": 2, "chain": 2, "token": format!("{}dac17f958d2ee523a2206206994597c13d831ec7", "0".repeat(24)), "asset_id": h("asset-2"),
                      "decimals": 6, "locked": 0,
                      "mint_cap_per_day": 100_000u64 * 100_000_000, "minted_today": 0, "mint_day": 0 }
                ]
            }),
            "rand_getTokens" => {
                let from = p(0).as_u64().unwrap_or(0);
                let limit = p(1).as_u64().unwrap_or(1000).min(1000) as usize;
                let rows: Vec<Value> = self
                    .tokens
                    .iter()
                    .filter(|t| t["index"].as_u64().unwrap_or(0) >= from)
                    .take(limit)
                    .cloned()
                    .collect();
                json!({ "enabled": true, "registration_fee": self.registration_fee, "next_index": self.next_token_index, "tokens": rows })
            }
            "rand_getToken" => self
                .tokens
                .iter()
                .find(|t| matches_token_key(t, &p(0)))
                .cloned()
                .unwrap_or(Value::Null),
            "rand_getTokenSupply" => self
                .tokens
                .iter()
                .find(|t| matches_token_key(t, &p(0)))
                .map(|t| json!({
                    "total_supply": t["total_supply"],
                    "backings": t["authority"]["backings"].as_array().cloned().unwrap_or_default(),
                }))
                .unwrap_or(Value::Null),
            "rand_getPeers" => json!(self.peers),
            "rand_getCallEnvelope" => {
                // A transcript for every call the mock knows about, sealed to nobody (opaque bytes).
                let hash = p(0).as_str().unwrap_or("").to_string();
                let is_call = self.blocks.iter().flat_map(|b| b["transactions"].as_array().cloned().unwrap_or_default())
                    .any(|t| t["hash"] == hash && t["action"]["kind"] == "call");
                if is_call { json!({ "tx": hash, "h_in": h("h_in"), "kem_ct": "", "to_sender": "0a".repeat(60), "to_auditor": "", "body": "0b".repeat(60) }) } else { Value::Null }
            }
            // A program's record, from the deploy that created it. One deployed with a public
            // input reports its digest; one without reports `null` (v0.4).
            "rand_getProgram" => {
                let id = p(0).as_str().unwrap_or("").to_string();
                self.committed()
                    .find(|(_, _, t)| t["action"]["kind"] == "deploy" && t["action"]["program"] == id)
                    .map(|(height, _, t)| {
                        let public = t["action"]["public_words_len"].as_u64().unwrap_or(0);
                        json!({ "id": id, "base_pc": 0, "words_len": t["action"]["words"], "code_hash": h(&format!("code-{id}")),
                            "deployed_at": height, "public_words_len": public,
                            "public_digest": if public > 0 { json!(public_digest(&id)) } else { Value::Null } })
                    })
                    .unwrap_or(Value::Null)
            }
            // A call's receipt: `h_pub` is the called program's public digest, `null` without one.
            // An invoke's receipt is a call's (RPL-2).
            "rand_getReceipt" => {
                let hash = p(0).as_str().unwrap_or("").to_string();
                let call = self
                    .committed()
                    .find(|(_, _, t)| t["hash"] == hash && (t["action"]["kind"] == "call" || t["action"]["kind"] == "invoke"));
                match call {
                    None => Value::Null,
                    Some((height, index, t)) => {
                        let program = t["action"]["program"].as_str().unwrap_or("").to_string();
                        let has_public = self.committed().any(|(_, _, d)| d["action"]["kind"] == "deploy"
                            && d["action"]["program"] == program && d["action"]["public_words_len"].as_u64().unwrap_or(0) > 0);
                        json!({ "tx": hash, "program": program, "tier": 14, "outputs": [1, 0, 25, 0, 0, 0, 0, 0],
                            "height": height, "index": index, "h_in": h("h_in"),
                            "h_pub": if has_public { json!(public_digest(&program)) } else { Value::Null } })
                    }
                }
            }
            // v0.3 / v0.4: the node's build, the genesis hash and the genesis call limits (chain
            // 14's values). An older node does not know the methods.
            "rand_getVersion" if !self.pre_s2 => json!({ "version": "0.1.0", "git_sha": MOCK_GIT_SHA,
                "chain_id": self.chain_id, "hc_bundle": h("hc_bundle"), "fri_profile": "test" }),
            "rand_getGenesisHash" if !self.pre_s2 => self.blocks.first().map(|b| b["hash"].clone()).unwrap_or(Value::Null),
            // Chain 18's shape (fullnode v0.6.6): the caps, the memo envelope, the v0.6 switch,
            // the auth guest and the gas section with its genesis prices (decimal strings).
            "rand_getLimits" if !self.pre_s2 => {
                let mut l = json!({ "max_program_words": 65535, "max_proof_bytes": 8388608,
                    "max_block_bytes": 20971520, "max_call_envelope_bytes": 65536, "max_program_public_words": 32768,
                    "envelope_bytes": 1860, "hardening_v6": true, "hc_auth": h("hc_auth"),
                    "gas_price": "100", "byte_price": "800", "gas_metering": "circuit",
                    "bundle_gas_limit": 20479, "adjust_bps": 1250,
                    // Fee feedback: `null` on a chain without a `fees` section.
                    "fee_rules": null });
                // RPL-2 (a v0.6.8 node): the `program_state` group, `null` without the section.
                l["program_state"] = if self.program_state {
                    json!({ "cell_fee": "10000000", "max_reads": 8, "max_writes": 8, "max_payouts": 4 })
                } else {
                    Value::Null
                };
                l
            }
            // RPL-2: a program's vault and its cells, as the node serves them from the ledger —
            // here derived from the committed invokes: the vault is what every invoke's bundle
            // deposited less what its `pays` paid out, per asset; the cells are the latest
            // write to each key, in key order, zero-valued ones gone.
            "rand_getProgramVault" | "rand_getProgramCells" if !self.program_state => json!({ "enabled": false }),
            "rand_getProgramVault" => {
                let id = p(0).as_str().unwrap_or("").to_string();
                let mut vault: std::collections::BTreeMap<u64, i128> = Default::default();
                for (_, _, t) in self.committed().filter(|(_, _, t)| t["action"]["kind"] == "invoke" && t["action"]["program"] == id) {
                    let b = &t["bundle"];
                    let num = |v: &Value| v.as_u64().map(|n| n as i128).or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(0);
                    *vault.entry(0).or_default() += num(&b["burn_r"]);
                    if t["action"]["transition"]["inflow"] == "deposit" {
                        *vault.entry(b["burn_asset"].as_u64().unwrap_or(0)).or_default() += num(&b["burn_a"]);
                    }
                    for pay in t["action"]["transition"]["pays"].as_array().into_iter().flatten() {
                        *vault.entry(pay["asset"].as_u64().unwrap_or(0)).or_default() -= num(&pay["amount"]);
                    }
                }
                json!(vault.iter().filter(|(_, a)| **a > 0).map(|(a, n)| json!({ "asset": a, "amount": n.to_string() })).collect::<Vec<_>>())
            }
            "rand_getProgramCells" => {
                let id = p(0).as_str().unwrap_or("").to_string();
                let after = p(1)["after"].as_str().map(str::to_string);
                let limit = p(1)["limit"].as_u64().unwrap_or(1000).clamp(1, 1000) as usize;
                let mut cells: std::collections::BTreeMap<String, String> = Default::default();
                for (_, _, t) in self.committed().filter(|(_, _, t)| t["action"]["kind"] == "invoke" && t["action"]["program"] == id) {
                    for w in t["action"]["transition"]["writes"].as_array().into_iter().flatten() {
                        let (k, v) = (w["key"].as_str().unwrap().to_string(), w["value"].as_str().unwrap().to_string());
                        if v == "00".repeat(32) {
                            cells.remove(&k);
                        } else {
                            cells.insert(k, v);
                        }
                    }
                }
                let page: Vec<Value> = cells
                    .iter()
                    .filter(|(k, _)| after.as_ref().is_none_or(|a| *k > a))
                    .take(limit)
                    .map(|(k, v)| json!({ "key": k, "value": v }))
                    .collect();
                let more = cells.iter().filter(|(k, _)| after.as_ref().is_none_or(|a| *k > a)).count() > page.len();
                json!({ "cells": page, "next": if more { page.last().map(|c| c["key"].clone()).unwrap_or(Value::Null) } else { Value::Null } })
            }
            other => return Err((-32601, format!("unknown method {other}"))),
        })
    }
}

/// Whether a `rand_getTokens` row matches a `rand_getToken`-style key: an index (number or
/// decimal string), or the 64-hex `id` / `rpl1…` `id_text`, case-insensitively.
fn matches_token_key(row: &Value, key: &Value) -> bool {
    if let Some(n) = key.as_u64() {
        return row["index"].as_u64() == Some(n);
    }
    let Some(s) = key.as_str() else { return false };
    let s = s.trim();
    if let Ok(n) = s.parse::<u64>() {
        if row["index"].as_u64() == Some(n) {
            return true;
        }
    }
    let hex_key = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    row["id"]
        .as_str()
        .is_some_and(|id| id.eq_ignore_ascii_case(hex_key))
        || row["id_text"]
            .as_str()
            .is_some_and(|t| t.eq_ignore_ascii_case(s))
}

/// The public fields of the chain-14 hidden-asset bundle, unique per `seed`: four slots, no
/// public `asset` field.
pub fn bundle(seed: &str) -> Value {
    json!({
        "anchor": h(&format!("anchor-{seed}")),
        "nullifiers": [h(&format!("nf1-{seed}")), h(&format!("nf2-{seed}")), h(&format!("nf3-{seed}")), h(&format!("nf4-{seed}"))],
        "commitments": [h(&format!("cm1-{seed}")), h(&format!("cm2-{seed}")), h(&format!("cm3-{seed}")), h(&format!("cm4-{seed}"))],
        "fee": 1000000, "burn_a": 0, "burn_r": 0, "burn_asset": 0, "time": 1,
        "proof_len": 302857, "envelope_len": [1380, 1380, 1380, 1380],
        // Chain 17+ (split authorisation): the commitment the auth proof must match, and that
        // proof by length.
        "auth_commit": h(&format!("auth-{seed}")), "auth_proof_bytes": 1360512
    })
}

/// A transaction as the node serialises it inside a block: a bundle (or `null`) and an action.
pub fn tx(chain_id: u64, seed: &str, bundle: Option<Value>, action: Value) -> Value {
    let hash = h(&format!("tx-{chain_id}-{seed}-{action}"));
    json!({ "hash": hash, "chain_id": chain_id, "bundle": bundle, "action": action })
}

pub struct MockNode {
    pub chain: Arc<Mutex<MockChain>>,
    pub url: String,
}

impl MockNode {
    pub fn with_chain<R>(&self, f: impl FnOnce(&mut MockChain) -> R) -> R {
        f(&mut self.chain.lock().unwrap())
    }
}

async fn rpc(State(chain): State<Arc<Mutex<MockChain>>>, Json(req): Json<Value>) -> Json<Value> {
    let id = req["id"].clone();
    let method = req["method"].as_str().unwrap_or("");
    let params = req["params"].clone();
    let out = chain.lock().unwrap().dispatch(method, &params);
    Json(match out {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => {
            json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
        }
    })
}

/// Serve `chain` on a random localhost port for the rest of the test.
pub async fn start_mock_node(chain: MockChain) -> MockNode {
    let chain = Arc::new(Mutex::new(chain));
    let app = Router::new()
        .route("/", post(rpc))
        .with_state(chain.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    MockNode { chain, url }
}
