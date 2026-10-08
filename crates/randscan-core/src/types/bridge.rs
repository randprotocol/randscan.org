use crate::amount;
use serde::{Deserialize, Serialize};

/// A row of the bridge's asset registry (`rand_getAssets`, and `rand_getBridgeState.assets`).
/// `index` is the `asset` word a note of that asset carries; index 0 is RAND and never appears
/// here. `decimals`/`locked`/`minted_today`/`mint_day` are bridge hardening B1 — one row per
/// *backing* (a token with several source coins, spec §12, lists once per backing, all sharing
/// `index`); `None` on a node predating B1.
///
/// The node's own `asset_json` renders `locked`/`minted_today`/`mint_cap_per_day` as JSON
/// **numbers** here (pinned by its test: `"locked": 600, "mint_cap_per_day": 100_000u64 *
/// 100_000_000, "minted_today": 1_000`) — a different encoding from the *same* fields on
/// `TokenBacking` (the token RPC's `backing_json`, which sends `locked`/`minted_today` as
/// strings). `amount::amount_opt` accepts either, so this struct does not have to track which
/// renderer sent which.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BridgeAsset {
    pub index: i64,
    pub chain: i64,
    pub token: String,
    pub asset_id: String,
    #[serde(default)]
    pub decimals: Option<i32>,
    #[serde(default, deserialize_with = "amount::amount_opt")]
    pub locked: Option<String>,
    #[serde(default, deserialize_with = "amount::amount_opt")]
    pub minted_today: Option<String>,
    #[serde(default)]
    pub mint_day: Option<i64>,
    /// The daily mint cap for this backing (bridge hardening B1).
    #[serde(default, deserialize_with = "amount::amount_opt")]
    pub mint_cap_per_day: Option<String>,
    /// Audit v6 (BRG-19, fullnode after v0.6.7): under bridge rules v2 what this backing minted
    /// in the rolling window (`None` on a day-counter chain, and on an older node) …
    #[serde(default, deserialize_with = "amount::amount_opt")]
    pub minted_in_window: Option<String>,
    /// … the window's length in seconds (`None` likewise) …
    #[serde(default)]
    pub mint_window_secs: Option<i64>,
    /// … and the largest deposit to this backing the caps admit right now — the per-backing cap
    /// less what it minted, and no more than the registry-wide window has left. The figure to
    /// read, not `mint_cap_per_day - minted_today`, which ignores the global cap. `None` on a
    /// node predating it.
    #[serde(default, deserialize_with = "amount::amount_opt")]
    pub mint_headroom: Option<String>,
}

/// The bridge's public state (`rand_getBridgeState`). Bridged value is notes, so there are no
/// balances here; a chain without a bridge section reports `enabled: false` and nothing else.
///
/// `mint_paused`/`pause_nonce`/`list_nonce`/`pause_key`/`pq_guardians`/`registration_fee` are
/// bridge hardening B1/B3/B4 — `None`/empty on a node predating them. `rotation_nonce`/`rules_v2`
/// (bridge rules v2) and `min_inbound_sequence` (the genesis replay floor) are chains 15–16+.
/// `endpoints` is the one field that is not the node's: see [`BridgeEndpoint`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BridgeState {
    pub enabled: bool,
    #[serde(default)]
    pub emitter: Option<String>,
    /// source chain id -> the emitter address trusted there
    #[serde(default)]
    pub emitters: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub guardian_set_index: Option<i64>,
    #[serde(default)]
    pub guardians: Vec<String>,
    /// The genesis PQ (Dilithium2) guardian set, hex, index-aligned with `guardians`. Never moves
    /// on a rotation.
    #[serde(default)]
    pub pq_guardians: Vec<String>,
    /// B1: while true, every transfer attest is refused (burns and rotations stay open).
    #[serde(default)]
    pub mint_paused: bool,
    /// B1: what the next `pause_mints` / `unpause_mints` must carry.
    #[serde(default)]
    pub pause_nonce: Option<i64>,
    /// B4: what the next `list_backing` / `register_bridged_token` must carry.
    #[serde(default)]
    pub list_nonce: Option<i64>,
    /// B1: the one Dilithium2 key that may pause minting (it can never unpause), hex.
    #[serde(default)]
    pub pause_key: Option<String>,
    /// B4: what a `register_bridged_token` owes past the bundle base. A number (not a decimal
    /// string, unlike every amount above): `docs/rpc.md`'s own example renders it as one, and it
    /// is a chain-wide constant, not a value that grows with usage the way a supply does. Parsed
    /// tolerantly (`amount::int_opt`) in case a future node renders it as a string instead, the
    /// way `TokenBacking.locked` and this same struct's `locked` disagree today.
    #[serde(default, deserialize_with = "amount::int_opt")]
    pub registration_fee: Option<i64>,
    #[serde(default)]
    pub burn_sequence: Option<i64>,
    /// Gone from `rand_getBridgeState` since the bridge's own registry no longer predicts an
    /// index (a bridged token is listed, via the token registry, before it can be deposited);
    /// kept here, always `None`, for a node old enough to still send it.
    #[serde(default)]
    pub next_index: Option<i64>,
    /// Bridge rules v2 (fullnode v0.5.4): what the next `M_rotate_pq` / `M_rotate_pause` must
    /// carry; 0 on a chain without the group, `None` on a node predating it.
    #[serde(default)]
    pub rotation_nonce: Option<i64>,
    /// Bridge rules v2: the registry-wide mint cap over every backing of every token together;
    /// `None` on a chain whose genesis has no such group.
    #[serde(default)]
    pub rules_v2: Option<BridgeRulesV2>,
    /// The genesis replay floor (C15-1): source chain id -> the lowest sequence a transfer from
    /// that chain may carry. A chain cut from a predecessor sets it past every lock the
    /// predecessor already minted; chain 19 also sets it past the operator's own sequence-0 lock
    /// on each redeployed endpoint (`{"2": 1, "3": 1, "4": 1, "5": 4}`). `None` without one.
    #[serde(default)]
    pub min_inbound_sequence: Option<std::collections::BTreeMap<String, i64>>,
    #[serde(default)]
    pub assets: Vec<BridgeAsset>,
    /// fullnode v0.6.8 (genesis `bridge.fees`, chain 20): the share of every deposit and every
    /// burn the chain mints as a zUSD note to `recipient`. `None` on a chain without it (14–19).
    #[serde(default)]
    pub fees: Option<BridgeFees>,
    /// Audit v6 (BRG-14, genesis `bridge.rotation`): how long a rotation waits before it takes
    /// effect and whether it must carry the new holders' proof of possession. `None` without.
    #[serde(default)]
    pub rotation_rules: Option<RotationRules>,
    /// Audit v6 (BRG-14): rotations signed but not yet in effect, as the node renders them
    /// (`{kind, new_pq_guardians | new_pause_key, effective_at_secs}`). `None` without the group.
    #[serde(default)]
    pub pending_rotations: Option<Vec<serde_json::Value>>,
    /// Derived by the explorer, never sent by the node: each of `emitters` as its own chain
    /// prints it, with its replay floor (`BridgeState::derive_endpoints`).
    #[serde(default)]
    pub endpoints: Vec<BridgeEndpoint>,
}

/// The `rules_v2` group of a bridge genesis (`docs/bridge.md`, "Bridge rules v2").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeRulesV2 {
    /// Bridge units (8 decimals) all backings together may mint per window.
    #[serde(deserialize_with = "amount::amount")]
    pub global_mint_cap_per_window: String,
    pub cap_window_secs: i64,
    /// Audit v6 (BRG-19): what every backing together minted in the current window, and what
    /// the window has left; `None` on a node predating them.
    #[serde(default, deserialize_with = "amount::amount_opt")]
    pub global_minted_in_window: Option<String>,
    #[serde(default, deserialize_with = "amount::amount_opt")]
    pub global_mint_headroom: Option<String>,
}

/// The genesis `bridge.fees` group (fullnode `docs/bridge.md` §25): basis points of the gross
/// deposit (`mint_bps`) and of the burned amount (`burn_bps`), rounded down to a whole release unit
/// of the backing, minted as a zUSD note to `recipient` (a `rand1…` address).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeFees {
    pub mint_bps: i64,
    pub burn_bps: i64,
    pub recipient: String,
}

/// The genesis `bridge.rotation` group (audit v6, BRG-14).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RotationRules {
    pub delay_secs: i64,
    pub needs_possession: bool,
}

/// One source-chain endpoint the chain mints from: a row of `emitters`, readable. The node and
/// the attestation format know an emitter as a 32-byte word (`bridge/spec/ATTESTATION.md` §3.6:
/// an EVM or Tron address left-padded with 12 zero bytes, a Solana program id verbatim), which
/// nobody can compare with what Etherscan or Tronscan prints — and since the 2026-09-30 endpoint
/// redeploy (chain 19's reason) there are two generations of contracts to tell apart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeEndpoint {
    /// bridge chain id (2 Ethereum, 3 BSC, 4 Tron, 5 Solana)
    pub chain: i64,
    /// `None` for a chain id the explorer has no name for.
    pub chain_name: Option<String>,
    /// the emitter as the node serves it: 32 bytes, hex
    pub emitter: String,
    /// the same address as the chain's own explorers print it (`0x…`, Tron base58check, Solana
    /// base58); `None` when the word is not a well-formed address of that chain
    pub address: Option<String>,
    /// the contract's (or program's) page on the chain's explorer
    pub explorer_url: Option<String>,
    /// this chain's entry of `min_inbound_sequence`
    pub min_inbound_sequence: Option<i64>,
}

/// "Ethereum" for bridge chain 2, and so on; the names `APPROVED_TOKENS` uses.
pub fn bridge_chain_name(chain: i64) -> Option<&'static str> {
    match chain {
        2 => Some("Ethereum"),
        3 => Some("BSC"),
        4 => Some("Tron"),
        5 => Some("Solana"),
        _ => None,
    }
}

/// An emitter word as its chain prints the address: `0x` + 20 bytes for Ethereum and BSC
/// (lowercase — EIP-55 casing needs Keccak, and every explorer accepts this form), base58check
/// with the `0x41` mainnet prefix for Tron, base58 of all 32 bytes for Solana. `None` for an
/// unknown chain, a word that is not 32 bytes of hex, or an EVM/Tron word whose 12 padding bytes
/// are not zero.
pub fn emitter_address(chain: i64, emitter: &str) -> Option<String> {
    use sha2::{Digest, Sha256};
    let word: [u8; 32] = hex::decode(emitter.trim_start_matches("0x"))
        .ok()?
        .try_into()
        .ok()?;
    let padded = || word[..12].iter().all(|&b| b == 0).then(|| &word[12..]);
    match chain {
        2 | 3 => Some(format!("0x{}", hex::encode(padded()?))),
        4 => {
            let mut body = vec![0x41];
            body.extend_from_slice(padded()?);
            let check = Sha256::digest(Sha256::digest(&body));
            body.extend_from_slice(&check[..4]);
            Some(bs58::encode(body).into_string())
        }
        5 => Some(bs58::encode(word).into_string()),
        _ => None,
    }
}

/// Where a chain's explorer shows the contract (or program) at `address`.
pub fn emitter_explorer_url(chain: i64, address: &str) -> Option<String> {
    match chain {
        2 => Some(format!("https://etherscan.io/address/{address}")),
        3 => Some(format!("https://bscscan.com/address/{address}")),
        4 => Some(format!("https://tronscan.org/#/contract/{address}")),
        5 => Some(format!("https://solscan.io/account/{address}")),
        _ => None,
    }
}

impl BridgeState {
    /// `emitters` as [`BridgeEndpoint`]s, in chain id order (numeric: the map's own order is
    /// textual). An entry whose key is not a chain id is left out.
    pub fn derive_endpoints(&self) -> Vec<BridgeEndpoint> {
        let mut out: Vec<BridgeEndpoint> = self
            .emitters
            .iter()
            .filter_map(|(id, emitter)| {
                let chain: i64 = id.parse().ok()?;
                let address = emitter_address(chain, emitter);
                Some(BridgeEndpoint {
                    chain,
                    chain_name: bridge_chain_name(chain).map(str::to_string),
                    emitter: emitter.clone(),
                    explorer_url: address
                        .as_deref()
                        .and_then(|a| emitter_explorer_url(chain, a)),
                    address,
                    min_inbound_sequence: self
                        .min_inbound_sequence
                        .as_ref()
                        .and_then(|f| f.get(id).copied()),
                })
            })
            .collect();
        out.sort_by_key(|e| e.chain);
        out
    }
}

/// One *backing* of a bridged token — one source coin — with what the chain has seen of it.
/// Amounts are decimal strings of the bridged token's own smallest unit.
///
/// Since chain 14 one token (one registry `index`) may have several backings, and the chain's
/// public record does not attribute everything to one of them. A burn names the coin it redeems,
/// so `burns`/`burned` are this backing's own. A deposit publishes only the token it minted, so
/// `deposits`/`deposited` are this backing's only when it is the token's one backing, and `null`
/// otherwise; the whole token's figures are the `token_*` fields, identical on each of the
/// token's rows — sum them once per `index`, never per row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeAssetActivity {
    pub index: i64,
    /// bridge chain id of the token's home (2 Ethereum, 3 BSC, 4 Tron, 5 Solana)
    pub chain: i64,
    /// the token's address on its home chain as the registry stores it (32 bytes hex)
    pub token: String,
    pub asset_id: String,
    /// set when `(chain, token)` is on the explorer's approved list (see `APPROVED_TOKENS`)
    pub symbol: Option<String>,
    pub name: Option<String>,
    /// the token's decimals on its home chain; amounts below are still in bridge units (8)
    pub decimals: Option<u8>,
    /// How many backings the token at `index` has (rows sharing this `index`).
    pub backings: i64,
    /// This backing's deposits; `None` when the token has several backings (see above).
    /// `deposited` is the gross the guardians signed — what `locked` and custody grew by.
    pub deposits: Option<i64>,
    pub deposited: Option<String>,
    /// Burns that redeemed this backing (`(to_chain, token)` of the burn). `burned` is what left
    /// `locked` — the release amount, the burn less the bridge fee (v0.6.8; the whole burn on a
    /// chain without `bridge.fees`).
    pub burns: i64,
    pub burned: String,
    /// v0.6.8: the bridge fees the chain kept as zUSD notes out of this backing's burns.
    pub burn_fees: String,
    /// What this backing holds for the chain right now: the registry's `locked`. Only on a node
    /// that does not serve `locked` is it rebuilt as `deposited - burned`, which is exact there
    /// because such a node has one backing per token.
    pub outstanding: String,
    /// The whole token's tallies, the same on every row of the token.
    pub token_deposits: i64,
    pub token_deposited: String,
    pub token_burns: i64,
    pub token_burned: String,
    /// v0.6.8: the bridge fee notes the chain minted out of the token's deposits and burns —
    /// zUSD in circulation backed by locked coins, so `Σ locked == supply == custody` still holds:
    /// a deposit locks its gross and mints `net + fee`; a burn destroys `release + fee`, releases
    /// `release` and mints `fee` back.
    pub token_deposit_fees: String,
    pub token_burn_fees: String,
    /// First and last height of any bridge activity of the token (not of this backing alone).
    pub first_height: Option<i64>,
    pub last_height: Option<i64>,
    /// This backing's own locked amount, straight from the registry (bridge hardening B1).
    pub locked: Option<String>,
    pub minted_today: Option<String>,
    pub mint_cap_per_day: Option<String>,
    /// Audit v6: the rolling-window count, its length, and the largest deposit the caps admit
    /// to this backing now (see `BridgeAsset`); `None` on a node predating them.
    pub minted_in_window: Option<String>,
    pub mint_window_secs: Option<i64>,
    pub mint_headroom: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact JSON the node's own pinned test emits
    /// (`bridge_state_reports_guardians_emitters_and_the_registry`,
    /// `crates/randprotocol-node/src/rpc.rs`): `locked`/`minted_today`/`mint_cap_per_day` as bare
    /// JSON numbers. Before the tolerant `amount::amount_opt` deserializer this failed closed —
    /// `rand_getBridgeState`/`rand_getAssets` never parsed on any chain-14 node with a bridged
    /// token, so the bridge cache stayed empty forever.
    #[test]
    fn an_asset_row_with_numeric_amounts_parses_like_the_nodes_own_pinned_fixture() {
        let v = serde_json::json!({
            "index": 1, "chain": 2, "token": "aa".repeat(32), "asset_id": "bb".repeat(32),
            "decimals": 8, "locked": 600,
            "mint_cap_per_day": 100_000u64 * 100_000_000, "minted_today": 1_000, "mint_day": 0,
        });
        let asset: BridgeAsset =
            serde_json::from_value(v).expect("the node's own numeric encoding must parse");
        assert_eq!(asset.locked.as_deref(), Some("600"));
        assert_eq!(asset.minted_today.as_deref(), Some("1000"));
        assert_eq!(asset.mint_cap_per_day.as_deref(), Some("10000000000000"));
        assert_eq!(asset.mint_day, Some(0));
    }

    /// A hypothetical node that instead sends these as decimal strings (the token RPC's
    /// `backing_json` convention for `locked`/`minted_today`) must parse identically.
    #[test]
    fn an_asset_row_with_string_amounts_also_parses() {
        let v = serde_json::json!({
            "index": 1, "chain": 2, "token": "aa".repeat(32), "asset_id": "bb".repeat(32),
            "decimals": 8, "locked": "600", "mint_cap_per_day": "10000000000000", "minted_today": "1000", "mint_day": 0,
        });
        let asset: BridgeAsset = serde_json::from_value(v).unwrap();
        assert_eq!(asset.locked.as_deref(), Some("600"));
        assert_eq!(asset.minted_today.as_deref(), Some("1000"));
        assert_eq!(asset.mint_cap_per_day.as_deref(), Some("10000000000000"));
    }

    /// The whole `rand_getBridgeState` reply, numeric amounts and `registration_fee` a bare
    /// number, no `next_index` (gone with the bridge's own registry) — the shape a real chain-14
    /// node with a bridged token sends.
    #[test]
    fn bridge_state_parses_the_nodes_own_pinned_shape() {
        let v = serde_json::json!({
            "enabled": true, "emitter": "01".repeat(32), "emitters": { "2": "02".repeat(32) },
            "guardian_set_index": 0, "guardians": ["aa".repeat(20)],
            "pq_guardians": ["cc".repeat(1312)],
            "mint_paused": false, "pause_nonce": 0, "list_nonce": 0,
            "pause_key": "dd".repeat(1312), "registration_fee": 1_000_000_000u64,
            "burn_sequence": 1,
            "assets": [{
                "index": 1, "chain": 2, "token": "aa".repeat(32), "asset_id": "bb".repeat(32),
                "decimals": 8, "locked": 600,
                "mint_cap_per_day": 100_000u64 * 100_000_000, "minted_today": 1_000, "mint_day": 0,
            }],
        });
        let state: BridgeState = serde_json::from_value(v)
            .expect("must parse without next_index and with numeric amounts");
        assert_eq!(state.registration_fee, Some(1_000_000_000));
        assert_eq!(state.next_index, None);
        assert_eq!(state.assets[0].locked.as_deref(), Some("600"));
        // A reply this old has no rules v2, no replay floor, and nothing of the explorer's own.
        assert_eq!(
            (
                state.rotation_nonce,
                state.rules_v2,
                state.min_inbound_sequence
            ),
            (None, None, None)
        );
        assert!(state.endpoints.is_empty());
    }

    /// The bridge section chain 19 is cut with, as a v0.6.7 node serves it (read off a node
    /// initialised from `deploy/cut-chain19-genesis.sh`'s dry run): the redeployed Ethereum, BSC
    /// and Tron endpoints, Solana unchanged, the replay floor, rules v2, `registration_fee` a
    /// string, and `null` where a chain without the group has nothing.
    fn chain_19() -> BridgeState {
        serde_json::from_value(serde_json::json!({
            "enabled": true,
            "emitter": "c02df6ba70c2457a7406780da97492a6acb57dc40751e6279f003a570559d15f",
            "emitters": {
                "2": "0000000000000000000000007af6b17047c1db6cb54347fdea45cf9179075bfa",
                "3": "0000000000000000000000007af6b17047c1db6cb54347fdea45cf9179075bfa",
                "4": "0000000000000000000000006410797df959987a5baf65b5fab97edeb34d5163",
                "5": "d3e58f1e9317bbc3c69b63fadff558ea82ba5d00765f1f1e483d705d209b413a",
            },
            "guardian_set_index": 1, "guardians": ["aa".repeat(20)], "pq_guardians": ["cc".repeat(1312)],
            "mint_paused": false, "pause_nonce": 0, "list_nonce": 0, "pause_key": "dd".repeat(1312),
            "registration_fee": "1000000000", "burn_sequence": 8, "rotation_nonce": 0,
            "rules_v2": { "global_mint_cap_per_window": "400000000000", "cap_window_secs": 86400 },
            "min_inbound_sequence": { "2": 1, "3": 1, "4": 1, "5": 4 },
            "assets": [],
        }))
        .expect("chain 19's bridge state parses")
    }

    #[test]
    fn chain_19s_bridge_state_keeps_the_replay_floor_and_rules_v2() {
        let state = chain_19();
        assert_eq!(state.registration_fee, Some(1_000_000_000));
        assert_eq!(state.burn_sequence, Some(8));
        assert_eq!(state.rotation_nonce, Some(0));
        let rules = state.rules_v2.as_ref().unwrap();
        assert_eq!(
            (
                rules.global_mint_cap_per_window.as_str(),
                rules.cap_window_secs
            ),
            ("400000000000", 86400)
        );
        let floor = state.min_inbound_sequence.as_ref().unwrap();
        assert_eq!(
            floor
                .iter()
                .map(|(c, s)| (c.as_str(), *s))
                .collect::<Vec<_>>(),
            [("2", 1), ("3", 1), ("4", 1), ("5", 4)]
        );
        // A chain without either group says `null`, and a number where the node sends a string.
        let bare: BridgeState = serde_json::from_value(serde_json::json!({
            "enabled": true, "rotation_nonce": 0, "rules_v2": null, "min_inbound_sequence": null,
        }))
        .unwrap();
        assert_eq!(
            (
                bare.rotation_nonce,
                bare.rules_v2,
                bare.min_inbound_sequence
            ),
            (Some(0), None, None)
        );
        let numeric: BridgeRulesV2 =
            serde_json::from_value(serde_json::json!({ "global_mint_cap_per_window": 50_000_000_000_000u64, "cap_window_secs": 3600 })).unwrap();
        assert_eq!(numeric.global_mint_cap_per_window, "50000000000000");
        assert_eq!(
            (
                numeric.global_minted_in_window,
                numeric.global_mint_headroom
            ),
            (None, None)
        );
    }

    /// The same reply from a node after v0.6.7 (audit v6, BRG-19): rules v2 says what the whole
    /// registry minted in the window and what is left, and every asset row carries its rolling
    /// count and headroom — the node's own test's literals.
    #[test]
    fn a_post_v067_node_adds_the_windows_and_headroom() {
        let state: BridgeState = serde_json::from_value(serde_json::json!({
            "enabled": true,
            "rules_v2": { "global_mint_cap_per_window": "1000000", "cap_window_secs": 86400,
                          "global_minted_in_window": "1000", "global_mint_headroom": "999000" },
            "assets": [{
                "index": 1, "chain": 2, "token": "aa".repeat(32), "asset_id": "bb".repeat(32),
                "decimals": 8, "locked": 600, "mint_cap_per_day": 100_000u64 * 100_000_000, "minted_today": 1_000, "mint_day": 0,
                "minted_in_window": "1000", "mint_window_secs": 86_400, "mint_headroom": "999000",
            }],
        }))
        .unwrap();
        let rules = state.rules_v2.unwrap();
        assert_eq!(
            (
                rules.global_minted_in_window.as_deref(),
                rules.global_mint_headroom.as_deref()
            ),
            (Some("1000"), Some("999000"))
        );
        let a = &state.assets[0];
        assert_eq!(
            (
                a.minted_in_window.as_deref(),
                a.mint_window_secs,
                a.mint_headroom.as_deref()
            ),
            (Some("1000"), Some(86_400), Some("999000"))
        );
    }

    /// Every address is the bridge repository's own record of the deployment
    /// (`bridge/deploy/deployments/*.json`, `docs/mainnet-deployment.md`), both generations: the
    /// 2026-09-19 endpoints chains 14–18 trusted and the 2026-09-30 ones chain 19 names.
    #[test]
    fn an_emitter_reads_as_its_chain_prints_it() {
        let pad = |a: &str| format!("{}{a}", "0".repeat(24));
        // chain 19
        assert_eq!(
            emitter_address(2, &pad("7af6b17047c1db6cb54347fdea45cf9179075bfa")).unwrap(),
            "0x7af6b17047c1db6cb54347fdea45cf9179075bfa"
        );
        assert_eq!(
            emitter_address(3, &pad("7af6b17047c1db6cb54347fdea45cf9179075bfa")).unwrap(),
            "0x7af6b17047c1db6cb54347fdea45cf9179075bfa"
        );
        assert_eq!(
            emitter_address(4, &pad("6410797df959987a5baf65b5fab97edeb34d5163")).unwrap(),
            "TK6JJv55CCkFjNHq7WwoU91GKaZEiC93me"
        );
        // chains 14–18
        assert_eq!(
            emitter_address(2, &pad("d6ebd21c3df90c9175ebdc8d6b377a9361604892")).unwrap(),
            "0xd6ebd21c3df90c9175ebdc8d6b377a9361604892"
        );
        assert_eq!(
            emitter_address(4, &pad("0992df85dcce77ded2c0387f1fa9cf98ac859700")).unwrap(),
            "TAqq2i8KfYpACPUc9f5e2gAjdSgXqmPpkU"
        );
        // Solana, not redeployed: the program id is the word itself
        let solana = "d3e58f1e9317bbc3c69b63fadff558ea82ba5d00765f1f1e483d705d209b413a";
        assert_eq!(
            emitter_address(5, solana).unwrap(),
            "FGA3kY3RjfDKjUszJESMYtYXAbsnkFhhoxM3Mb34vycu"
        );
        assert_eq!(
            emitter_address(5, &format!("0x{}", solana.to_uppercase())).unwrap(),
            "FGA3kY3RjfDKjUszJESMYtYXAbsnkFhhoxM3Mb34vycu"
        );
        // Not an address of that chain: a 20-byte chain's word with padding that is not zero, a
        // word of the wrong length, something that is not hex, a chain the explorer cannot name.
        assert_eq!(emitter_address(2, solana), None);
        assert_eq!(emitter_address(4, solana), None);
        assert_eq!(
            emitter_address(2, "7af6b17047c1db6cb54347fdea45cf9179075bfa"),
            None
        );
        assert_eq!(emitter_address(5, "not hex"), None);
        assert_eq!(
            emitter_address(9, &pad("7af6b17047c1db6cb54347fdea45cf9179075bfa")),
            None
        );
    }

    #[test]
    fn chain_19s_endpoints_are_derived_in_chain_order_with_their_floors() {
        let mut state = chain_19();
        // A chain the explorer cannot name, and one whose id sorts before "2" as text.
        state.emitters.insert("10".into(), "ee".repeat(32));
        state.emitters.insert("not a chain".into(), "ee".repeat(32));
        let endpoints = state.derive_endpoints();
        let row = |e: &BridgeEndpoint| {
            (
                e.chain,
                e.chain_name.clone(),
                e.address.clone(),
                e.explorer_url.clone(),
                e.min_inbound_sequence,
            )
        };
        let some = |s: &str| Some(s.to_string());
        assert_eq!(
            endpoints.iter().map(row).collect::<Vec<_>>(),
            [
                (
                    2,
                    some("Ethereum"),
                    some("0x7af6b17047c1db6cb54347fdea45cf9179075bfa"),
                    some("https://etherscan.io/address/0x7af6b17047c1db6cb54347fdea45cf9179075bfa"),
                    Some(1)
                ),
                (
                    3,
                    some("BSC"),
                    some("0x7af6b17047c1db6cb54347fdea45cf9179075bfa"),
                    some("https://bscscan.com/address/0x7af6b17047c1db6cb54347fdea45cf9179075bfa"),
                    Some(1)
                ),
                (
                    4,
                    some("Tron"),
                    some("TK6JJv55CCkFjNHq7WwoU91GKaZEiC93me"),
                    some("https://tronscan.org/#/contract/TK6JJv55CCkFjNHq7WwoU91GKaZEiC93me"),
                    Some(1)
                ),
                (
                    5,
                    some("Solana"),
                    some("FGA3kY3RjfDKjUszJESMYtYXAbsnkFhhoxM3Mb34vycu"),
                    some("https://solscan.io/account/FGA3kY3RjfDKjUszJESMYtYXAbsnkFhhoxM3Mb34vycu"),
                    Some(4)
                ),
                (10, None, None, None, None),
            ]
        );
        assert_eq!(endpoints[0].emitter, state.emitters["2"]);
        // No floor in the genesis: every endpoint is there, none has one.
        state.min_inbound_sequence = None;
        assert!(state
            .derive_endpoints()
            .iter()
            .all(|e| e.min_inbound_sequence.is_none()));
    }
}
