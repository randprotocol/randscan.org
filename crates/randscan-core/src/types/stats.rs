use serde::{Deserialize, Serialize};

/// Network statistics: a mix of indexed counts and the node's live status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStats {
    pub chain_id: i64,
    pub symbol: String,
    pub decimals: i16,
    pub height: i64,
    pub view: i64,
    pub total_transactions: i64,
    /// Leaves in the commitment tree: every note the chain has ever created.
    pub notes: i64,
    /// Nullifiers published: every note the chain has ever spent.
    pub nullifiers: i64,
    /// Register entries (every validator that has bonded, active or not).
    pub validator_count: i64,
    /// Validators in the set running the current epoch.
    pub active_validator_count: i64,
    /// Sum of the active set's stake, units.
    pub total_stake: String,
    /// The supply audit's `total_supply` (units), or "0" on a node without `rand_getSupply`.
    pub total_supply: String,
    /// The supply audit's `pool_value`: what the notes in the tree are worth in total.
    pub pool_value: Option<String>,
    pub program_count: i64,
    pub avg_block_time_ms: f64,
    pub peer_count: i32,
    pub mempool_size: i32,
    pub node_syncing: bool,
    pub faucet: bool,
    pub confidential: bool,
    pub current_leader: Option<String>,
    /// The commitment tree's current root.
    pub tree_root: Option<String>,
    /// The bundle guest every proof on this chain is checked against.
    pub hc_bundle: Option<String>,
    /// The auth guest the genesis pins (split authorisation, chain 17+): every bundle then
    /// carries an `auth_commit` and an auth proof over the spend key. `None` on a chain without
    /// one, and on a node too old to report it.
    pub hc_auth: Option<String>,
    /// The tip ledger's current gas prices (`rand_status.gas_prices`), refreshed every commit:
    /// under a `gas` section whose prices move per block by fullness these are the live ones,
    /// not the genesis snapshot `limits` was read with. `None` on a chain without a `gas`
    /// section.
    pub gas_prices: Option<GasPrices>,
    pub epoch: Option<i64>,
    pub epoch_blocks: Option<i64>,
    /// Block 0's hash as the node reports it (`rand_getGenesisHash`); a chain id alone does not
    /// tell two cuts apart.
    pub genesis_hash: Option<String>,
    /// The node's crate version and the commit it was built from (`rand_getVersion`).
    pub node_version: Option<String>,
    pub node_git_sha: Option<String>,
    pub fri_profile: Option<String>,
    /// The chain's call limits, or `None` on a node without `rand_getLimits`.
    pub limits: Option<ChainLimits>,
    pub updated_at: String,
}

/// The tip's gas prices under a chain's `gas` section (fullnode spec 2026-09-28 §7.1, §8):
/// RAND units per gas and per KiB of call proof plus input envelope, decimal strings like every
/// other amount on this API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GasPrices {
    #[serde(deserialize_with = "crate::amount::amount")]
    pub gas_price: String,
    #[serde(deserialize_with = "crate::amount::amount")]
    pub byte_price: String,
}

/// What a wallet needs from the chain's genesis to build a transaction (`rand_getLimits`): the
/// size caps (v0.4; constants before chain 13 — chain 14 runs 65 535 words, 8 MiB proofs, 20 MiB
/// blocks, 64 KiB call envelopes and 32 768 public words), and since chain 16/17/18 the envelope
/// format, the v0.6 switch, the auth guest and the gas section. Every field past the five caps is
/// absent on a node that predates it, which serde reads as `None` / `false`; a JSON `null` from
/// a newer node means the chain has no such setting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainLimits {
    /// The most code words a program may have.
    pub max_program_words: u64,
    /// The largest proof, bundle or call.
    pub max_proof_bytes: u64,
    /// The largest block, and so the largest transaction.
    pub max_block_bytes: u64,
    /// The largest sealed call-input envelope.
    pub max_call_envelope_bytes: u64,
    /// The most public words a deploy may fix; 0 means no program has a public input.
    pub max_program_public_words: u64,
    /// The exact size every note envelope must have (fullnode spec 2026-09-26 §2.4): `1860` on a
    /// chain whose notes carry an encrypted memo (chain 18); `None` where wallets seal the
    /// legacy 1 348-byte form and a memo is merely tolerated.
    #[serde(default)]
    pub envelope_bytes: Option<u64>,
    /// The v0.6 validity rules (the pc window, canonical proof shapes, the call binding, the
    /// program-table floor) as consensus rules rather than pool policy. Chains 16+.
    #[serde(default)]
    pub hardening_v6: bool,
    /// The auth guest the genesis pins (split authorisation; chain 17+), hex, or `None`. Set,
    /// `hc_bundle` is bundle guest v3 and every bundle carries `auth_commit` and an auth proof.
    #[serde(default)]
    pub hc_auth: Option<String>,
    /// RAND units per gas: the chain's own `gas` section's current price (`gas_metering`
    /// `"circuit"`), or the node's own policy (`"header"`), or `None` with neither.
    #[serde(default, deserialize_with = "crate::amount::amount_opt")]
    pub gas_price: Option<String>,
    /// RAND units per KiB (or part of one) of call proof and input envelope, from byte 0.
    #[serde(default, deserialize_with = "crate::amount::amount_opt")]
    pub byte_price: Option<String>,
    /// `"circuit"` on a chain whose genesis carries a `gas` section (the in-circuit meter; the
    /// prices above are consensus state and every node answers the same), `"header"` while only
    /// this node's policy prices a call's `gas_max` off its proof header (two nodes may answer
    /// differently), `None` with neither.
    #[serde(default)]
    pub gas_metering: Option<String>,
    /// The bundle guest's flat declared gas every bundle proof must publish (`20479` on chain 18,
    /// the tier-14 hash-free ceiling); `None` without a `gas` section.
    #[serde(default)]
    pub bundle_gas_limit: Option<u64>,
    /// The dynamic controller's per-block step in basis points (`1250` = 12.5% on chain 18);
    /// `None` on a chain whose prices never move, a `gas` section without `dynamic` included.
    #[serde(default)]
    pub adjust_bps: Option<u32>,
    /// RPL-2 (fullnode v0.6.8): the genesis `program_state` section and the `invoke` limits that
    /// come with it; `None` on a chain without the section (every `invoke` refused) and on a node
    /// predating the field.
    #[serde(default)]
    pub program_state: Option<ProgramStateLimits>,
    /// Audit v6 (POOL-2): the ceilings the dynamic controller never lifts a price over, decimal
    /// strings; `None` where the genesis sets none.
    #[serde(default, deserialize_with = "crate::amount::amount_opt")]
    pub max_gas_price: Option<String>,
    #[serde(default, deserialize_with = "crate::amount::amount_opt")]
    pub max_byte_price: Option<String>,
    /// `"paying"` when only a call's proof and input envelope move `byte_price` (POOL-2).
    #[serde(default)]
    pub byte_load: Option<String>,
    /// Audit v6 (STAKE-2): a bond that registers a new key needs the validator set's vote
    /// (`admit_validator`) first.
    #[serde(default)]
    pub admission_by_vote: bool,
    /// The genesis `testnet` marker (STAKE-2): what lets a faucet sit beside a bridge.
    #[serde(default)]
    pub testnet: bool,
    /// Audit v6 (STAKE-1): the genesis `staking.slashing` section, `None` without.
    #[serde(default)]
    pub slashing: Option<SlashingLimits>,
    /// Audit v6 (BIND-1): `0` where bindings and signed messages carry the chain id alone
    /// (chains 14–19), `1` where they carry the genesis hash. `None` on a node predating it.
    #[serde(default)]
    pub binding_domain: Option<u32>,
    /// Issue #118: how old, in blocks, a bundle's anchor and `time` may be (256..4 096); `None`
    /// where the genesis leaves both at 256.
    #[serde(default)]
    pub proof_window_blocks: Option<u64>,
    /// Fee feedback (fullnode `feat/fee-feedback`, unreleased; `docs/fees.md` §1.3): the genesis
    /// `fees` section's flags. `None` on a chain without the section (or none of its flags set)
    /// and on a node predating the field. Informational: no fee a wallet pays changes.
    #[serde(default)]
    pub fee_rules: Option<FeeRules>,
}

/// The `fee_rules` group of `rand_getLimits` (fee feedback). Each flag reads `false` when absent,
/// so a node that adds a flag later is still read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeeRules {
    /// A bundle's `BUNDLE_BASE` is destroyed instead of paid to the proposer.
    #[serde(default)]
    pub burn_base: bool,
    /// The aggregation subsidy carries only the minted part, the schedule's shortfall over the
    /// proving share.
    #[serde(default)]
    pub subsidy_net_of_fees: bool,
    /// The whole settled floor is burned (only ever beside `burn_base`).
    #[serde(default)]
    pub burn_floor: bool,
}

/// The `slashing` group of `rand_getLimits` (audit v6, STAKE-1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlashingLimits {
    pub equivocation_bps: u32,
    pub jail_epochs: u64,
}

/// The `program_state` group of `rand_getLimits` (RPL-2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgramStateLimits {
    /// RAND units an invoke's fee floor gains per cell it creates (a non-zero write to a cell
    /// that read as zeros), decimal string.
    #[serde(deserialize_with = "crate::amount::amount")]
    pub cell_fee: String,
    pub max_reads: u32,
    pub max_writes: u32,
    /// pays and mints together
    pub max_payouts: u32,
}

impl ChainLimits {
    /// Whether the chain's genesis carries a `gas` section: prices are consensus state, every
    /// call pays its declared limit, every bundle declares `bundle_gas_limit`.
    pub fn has_gas_section(&self) -> bool {
        self.gas_metering.as_deref() == Some("circuit")
    }
}

/// The node's supply audit (`rand_getSupply`, phase S2): every crossing of the pool boundary is
/// public, so these are exact. All amounts are unit strings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Supply {
    pub height: i64,
    pub genesis_deposited: String,
    pub genesis_staked: String,
    pub faucet_minted: String,
    pub withdraw_deposited: String,
    pub fees_paid: String,
    pub burned: String,
    pub pool_value: String,
    pub register_total: String,
    pub total_supply: String,
    pub invariant_holds: bool,
    /// Genesis vesting (chain 17+, fullnode `docs/vesting.md`): what genesis issued into the
    /// vesting register, what claims and revokes released into the pool, what the register still
    /// holds, and what of that has not unlocked yet at the head. `None` on a node that predates
    /// the fields; `"0"` on a chain without a `vesting` section.
    #[serde(default, deserialize_with = "crate::amount::amount_opt")]
    pub vesting_issued: Option<String>,
    #[serde(default, deserialize_with = "crate::amount::amount_opt")]
    pub vesting_released: Option<String>,
    #[serde(default, deserialize_with = "crate::amount::amount_opt")]
    pub vesting_in_register: Option<String>,
    #[serde(default, deserialize_with = "crate::amount::amount_opt")]
    pub vesting_locked: Option<String>,
    /// RPL-2 (program state): RAND that invokes have paid out of program vaults as notes (pool
    /// side, beside `withdraw_deposited`), and what the vaults still hold (register side, inside
    /// `total_supply`). `"0"` on a chain without the section, `None` on a node predating them.
    #[serde(default, deserialize_with = "crate::amount::amount_opt")]
    pub program_rand_out: Option<String>,
    #[serde(default, deserialize_with = "crate::amount::amount_opt")]
    pub program_rand_held: Option<String>,
    /// Fee feedback (fullnode `feat/fee-feedback`, unreleased): every bundle base (under
    /// `fees.burn_floor`, every settled floor) burned under the genesis `fees.burn_base`, inside
    /// `burned`. `"0"` on a chain without the flag, `None` on a node predating the field.
    #[serde(default, deserialize_with = "crate::amount::amount_opt")]
    pub base_fees_burned: Option<String>,
}
