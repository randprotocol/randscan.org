//! Row types. NUMERIC columns are selected as `::text` and carried as `String`.

use chrono::{DateTime, Utc};
use randscan_core::{
    BlockSummary, BridgeFeeNote, BridgeGovernanceAction, Bundle, GeoInfo, NetworkStats, Note,
    Nullifier, PendingStake, ProgramSummary, Receipt, StakingAction, TokenAction,
    TransactionDetail, TransactionSummary, Transition, TxKind, Validator,
};
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow)]
pub struct BlockRow {
    pub hash: String,
    pub height: i64,
    pub view: i64,
    pub parent: String,
    pub proposer: String,
    pub timestamp_ms: i64,
    pub tx_root: String,
    pub state_root: String,
    pub justify_view: i64,
    pub tx_count: i32,
}

impl From<BlockRow> for BlockSummary {
    fn from(b: BlockRow) -> Self {
        BlockSummary {
            hash: b.hash,
            height: b.height,
            view: b.view,
            parent: b.parent,
            proposer: b.proposer,
            timestamp_ms: b.timestamp_ms,
            tx_count: b.tx_count,
            justify_view: b.justify_view,
        }
    }
}

/// The columns a transaction list needs. `TxDetailRow` carries the rest.
#[derive(Debug, Clone, FromRow)]
pub struct TxRow {
    pub hash: String,
    pub block_hash: String,
    pub height: i64,
    pub tx_index: i32,
    pub kind: String,
    pub fee: String,
    pub timestamp_ms: i64,
    pub has_bundle: bool,
    pub program_id: Option<String>,
    pub validator: Option<String>,
    pub amount: Option<String>,
    pub asset_index: Option<i64>,
}

impl From<TxRow> for TransactionSummary {
    fn from(t: TxRow) -> Self {
        TransactionSummary {
            hash: t.hash,
            height: t.height,
            block_hash: t.block_hash,
            tx_index: t.tx_index,
            kind: TxKind::parse_lossy(&t.kind),
            fee: t.fee,
            timestamp_ms: t.timestamp_ms,
            has_bundle: t.has_bundle,
            program: t.program_id,
            validator: t.validator,
            amount: t.amount,
            asset_index: t.asset_index,
        }
    }
}

/// Every column of a transaction row.
#[derive(Debug, Clone, FromRow)]
pub struct TxDetailRow {
    #[sqlx(flatten)]
    pub tx: TxRow,
    pub chain_id: i64,
    pub anchor: Option<String>,
    pub nullifier_1: Option<String>,
    pub nullifier_2: Option<String>,
    pub nullifier_3: Option<String>,
    pub nullifier_4: Option<String>,
    pub commitment_1: Option<String>,
    pub commitment_2: Option<String>,
    pub commitment_3: Option<String>,
    pub commitment_4: Option<String>,
    pub burn_a: Option<String>,
    pub burn_r: Option<String>,
    pub burn_asset: Option<i64>,
    pub bundle_time: Option<i64>,
    pub proof_len: Option<i64>,
    pub envelope_len_1: Option<i64>,
    pub envelope_len_2: Option<i64>,
    pub envelope_len_3: Option<i64>,
    pub envelope_len_4: Option<i64>,
    pub words_len: Option<i64>,
    pub call_proof_len: Option<i64>,
    pub input_envelope_len: Option<i64>,
    pub cm: Option<String>,
    pub registered: Option<bool>,
    pub action_nonce: Option<i64>,
    pub attestation_len: Option<i64>,
    pub recipient: Option<String>,
    pub note_time: Option<i64>,
    pub relayer_fee: Option<String>,
    pub to_chain: Option<i32>,
    pub bridge_to: Option<String>,
    pub bridge_token: Option<String>,
    pub deposit_r: Option<String>,
    pub derived_cm: Option<String>,
    pub pq_signers: Option<Vec<i32>>,
    pub token_action: Option<serde_json::Value>,
    pub bridge_governance: Option<serde_json::Value>,
    pub auth_commit: Option<String>,
    pub auth_proof_len: i64,
    pub transition: Option<serde_json::Value>,
    pub deposit_amount: Option<String>,
    pub release_amount: Option<String>,
    pub fee_note: Option<serde_json::Value>,
    pub staking_action: Option<serde_json::Value>,
}

impl TxDetailRow {
    /// The fee bundle, when the row has one.
    pub fn bundle(&self) -> Option<Bundle> {
        if !self.tx.has_bundle {
            return None;
        }
        Some(Bundle {
            anchor: self.anchor.clone().unwrap_or_default(),
            nullifiers: [
                self.nullifier_1.clone().unwrap_or_default(),
                self.nullifier_2.clone().unwrap_or_default(),
                self.nullifier_3.clone().unwrap_or_default(),
                self.nullifier_4.clone().unwrap_or_default(),
            ],
            commitments: [
                self.commitment_1.clone().unwrap_or_default(),
                self.commitment_2.clone().unwrap_or_default(),
                self.commitment_3.clone().unwrap_or_default(),
                self.commitment_4.clone().unwrap_or_default(),
            ],
            fee: self.tx.fee.clone(),
            burn_a: self.burn_a.clone().unwrap_or_else(|| "0".into()),
            burn_r: self.burn_r.clone().unwrap_or_else(|| "0".into()),
            burn_asset: self.burn_asset.unwrap_or(0),
            time: self.bundle_time.unwrap_or(0),
            proof_len: self.proof_len.unwrap_or(0),
            envelope_len: [
                self.envelope_len_1.unwrap_or(0),
                self.envelope_len_2.unwrap_or(0),
                self.envelope_len_3.unwrap_or(0),
                self.envelope_len_4.unwrap_or(0),
            ],
            auth_commit: self.auth_commit.clone(),
            auth_proof_len: self.auth_proof_len,
        })
    }

    /// The public `commitment` field of `TransactionDetail` — **bridge_attest only**, mirroring
    /// exactly what the node's own `tx_json` renders (its deposit's leaf, copied off the wire
    /// into `derived_cm` at insert time; see `NewTx::derived_cm`'s doc comment). `derived_cm`
    /// also holds a `token_mint`'s or a `register_token`'s recomputed note commitment (this
    /// indexer's own, since the node does not publish one for those), but that value is
    /// deliberately **not** surfaced here: the node never renders a `commitment` field for
    /// those two kinds, so neither does this API — look for that note among
    /// `GET /transactions/:hash/envelopes`'s `notes` instead, the same way a caller would need to
    /// for any other note this API does not name directly.
    pub fn commitment(&self) -> Option<String> {
        match self.tx.kind.as_str() {
            "bridge_attest" => self.derived_cm.clone(),
            _ => None,
        }
    }

    pub fn into_detail(self, receipt: Option<Receipt>) -> TransactionDetail {
        let bundle = self.bundle();
        let commitment = self.commitment();
        let token_action: Option<TokenAction> = self
            .token_action
            .and_then(|v| serde_json::from_value(v).ok());
        let bridge_governance: Option<BridgeGovernanceAction> = self
            .bridge_governance
            .and_then(|v| serde_json::from_value(v).ok());
        let pq_signers = self
            .pq_signers
            .map(|v| v.into_iter().map(i64::from).collect());
        let transition: Option<Transition> =
            self.transition.and_then(|v| serde_json::from_value(v).ok());
        let fee_note: Option<BridgeFeeNote> =
            self.fee_note.and_then(|v| serde_json::from_value(v).ok());
        let staking_action: Option<StakingAction> = self
            .staking_action
            .and_then(|v| serde_json::from_value(v).ok());
        TransactionDetail {
            chain_id: self.chain_id,
            bundle,
            words_len: self.words_len,
            call_proof_len: self.call_proof_len,
            input_envelope_len: self.input_envelope_len,
            receipt,
            cm: self.cm,
            registered: self.registered,
            action_nonce: self.action_nonce,
            attestation_len: self.attestation_len,
            recipient: self.recipient,
            note_time: self.note_time,
            relayer_fee: self.relayer_fee,
            to_chain: self.to_chain,
            bridge_to: self.bridge_to,
            bridge_token: self.bridge_token,
            deposit_r: self.deposit_r,
            commitment,
            pq_signers,
            token_action,
            bridge_governance,
            transition,
            deposit_amount: self.deposit_amount,
            release_amount: self.release_amount,
            fee_note,
            staking_action,
            summary: self.tx.into(),
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct ReceiptRow {
    pub tx_hash: String,
    pub program: String,
    pub tier: i32,
    pub outputs: Vec<i64>,
    pub height: i64,
    pub tx_index: i32,
    pub h_in: String,
    pub h_pub: Option<String>,
}

impl From<ReceiptRow> for Receipt {
    fn from(r: ReceiptRow) -> Self {
        Receipt {
            tx: r.tx_hash,
            program: r.program,
            tier: r.tier,
            outputs: r.outputs,
            height: r.height,
            index: r.tx_index,
            h_in: r.h_in,
            h_pub: r.h_pub,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct NoteRow {
    pub leaf_index: i64,
    pub cm: String,
    pub height: i64,
    pub tx_hash: Option<String>,
}

impl From<NoteRow> for Note {
    fn from(n: NoteRow) -> Self {
        Note {
            leaf_index: n.leaf_index,
            cm: n.cm,
            height: n.height,
            tx_hash: n.tx_hash,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct NullifierRow {
    pub nullifier: String,
    pub tx_hash: String,
    pub height: i64,
    pub tx_index: i32,
}

impl From<NullifierRow> for Nullifier {
    fn from(n: NullifierRow) -> Self {
        Nullifier {
            nullifier: n.nullifier,
            tx_hash: n.tx_hash,
            height: n.height,
            tx_index: n.tx_index,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct ValidatorRow {
    pub address: String,
    pub stake: String,
    pub rewards: String,
    pub pending: serde_json::Value,
    pub payout: Option<String>,
    pub nonce: i64,
    pub active: bool,
    pub sort_index: i32,
    pub blocks_proposed: i64,
    pub last_proposed_height: Option<i64>,
    pub last_proposed_timestamp_ms: Option<i64>,
}

impl ValidatorRow {
    /// `active_stake` is the sum over the active set; an inactive validator's share is 0.
    pub fn into_validator(self, active_stake: u128) -> Validator {
        let stake: u128 = self.stake.parse().unwrap_or(0);
        let share_percent = if active_stake == 0 || !self.active {
            0.0
        } else {
            (stake as f64 / active_stake as f64) * 100.0
        };
        let pending: Vec<PendingStake> = serde_json::from_value(self.pending).unwrap_or_default();
        Validator {
            address: self.address,
            stake: self.stake,
            rewards: self.rewards,
            pending,
            payout: self.payout,
            nonce: self.nonce,
            active: self.active,
            share_percent,
            blocks_proposed: self.blocks_proposed,
            last_proposed_height: self.last_proposed_height,
            last_proposed_timestamp_ms: self.last_proposed_timestamp_ms,
            sort_index: self.sort_index,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct ProgramRow {
    pub id: String,
    pub deploy_tx: String,
    pub deployed_at_height: i64,
    pub base_pc: i64,
    pub words_len: i64,
    pub code_hash: String,
    pub public_words_len: i64,
    pub public_digest: Option<String>,
    pub call_count: i64,
    pub last_called_height: Option<i64>,
    pub invoke_count: i64,
    pub last_invoked_height: Option<i64>,
}

impl From<ProgramRow> for ProgramSummary {
    fn from(p: ProgramRow) -> Self {
        ProgramSummary {
            id: p.id,
            deploy_tx: p.deploy_tx,
            deployed_at_height: p.deployed_at_height,
            base_pc: p.base_pc,
            words_len: p.words_len,
            code_hash: p.code_hash,
            public_words_len: p.public_words_len,
            public_digest: p.public_digest,
            call_count: p.call_count,
            last_called_height: p.last_called_height,
            invoke_count: p.invoke_count,
            last_invoked_height: p.last_invoked_height,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct NetworkStatsRow {
    pub chain_id: i64,
    pub symbol: String,
    pub decimals: i16,
    pub height: i64,
    pub view: i64,
    pub total_transactions: i64,
    pub notes: i64,
    pub nullifiers: i64,
    pub validator_count: i64,
    pub active_validator_count: i64,
    pub total_stake: String,
    pub total_supply: String,
    pub pool_value: Option<String>,
    pub program_count: i64,
    pub avg_block_time_ms: f64,
    pub peer_count: i32,
    pub mempool_size: i32,
    pub node_syncing: bool,
    pub faucet: bool,
    pub confidential: bool,
    pub current_leader: Option<String>,
    pub tree_root: Option<String>,
    pub hc_bundle: Option<String>,
    pub epoch: Option<i64>,
    pub epoch_blocks: Option<i64>,
    pub genesis_hash: Option<String>,
    pub node_version: Option<String>,
    pub node_git_sha: Option<String>,
    pub fri_profile: Option<String>,
    pub limits: Option<serde_json::Value>,
    pub hc_auth: Option<String>,
    pub gas_prices: Option<serde_json::Value>,
    pub updated_at: DateTime<Utc>,
}

impl From<NetworkStatsRow> for NetworkStats {
    fn from(s: NetworkStatsRow) -> Self {
        NetworkStats {
            chain_id: s.chain_id,
            symbol: s.symbol,
            decimals: s.decimals,
            height: s.height,
            view: s.view,
            total_transactions: s.total_transactions,
            notes: s.notes,
            nullifiers: s.nullifiers,
            validator_count: s.validator_count,
            active_validator_count: s.active_validator_count,
            total_stake: s.total_stake,
            total_supply: s.total_supply,
            pool_value: s.pool_value,
            program_count: s.program_count,
            avg_block_time_ms: s.avg_block_time_ms,
            peer_count: s.peer_count,
            mempool_size: s.mempool_size,
            node_syncing: s.node_syncing,
            faucet: s.faucet,
            confidential: s.confidential,
            current_leader: s.current_leader,
            tree_root: s.tree_root,
            hc_bundle: s.hc_bundle,
            hc_auth: s.hc_auth,
            // Stored verbatim off `rand_status`; a shape this build cannot read is absent.
            gas_prices: s.gas_prices.and_then(|v| serde_json::from_value(v).ok()),
            epoch: s.epoch,
            epoch_blocks: s.epoch_blocks,
            genesis_hash: s.genesis_hash,
            node_version: s.node_version,
            node_git_sha: s.node_git_sha,
            fri_profile: s.fri_profile,
            // A shape this build cannot read is served as absent, not as a failed stats reply.
            limits: s.limits.and_then(|v| serde_json::from_value(v).ok()),
            updated_at: s.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct IndexerStateRow {
    pub next_height: i64,
    pub last_indexed_hash: Option<String>,
    pub is_syncing: bool,
    /// Chain id of the indexed data; `None` before the indexer first reached a node.
    pub chain_id: Option<i64>,
    /// Next commitment-tree leaf to fetch from `rand_getCommitments`.
    pub next_leaf: i64,
}

#[derive(Debug, Clone, FromRow)]
pub struct NodeGeoRow {
    pub ip: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub city: Option<String>,
    pub region: Option<String>,
    pub country: Option<String>,
    pub country_code: Option<String>,
    pub org: Option<String>,
    pub ok: bool,
    pub updated_at: DateTime<Utc>,
}

impl NodeGeoRow {
    pub fn geo(&self) -> Option<GeoInfo> {
        match (self.ok, self.lat, self.lon) {
            (true, Some(lat), Some(lon)) => Some(GeoInfo {
                lat,
                lon,
                city: self.city.clone(),
                region: self.region.clone(),
                country: self.country.clone(),
                country_code: self.country_code.clone(),
                org: self.org.clone(),
            }),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct UserRow {
    pub id: i64,
    pub email: String,
    pub password_hash: String,
    pub created_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow)]
pub struct SessionRow {
    pub token_hash: String,
    pub user_id: i64,
    pub expires_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct ApiKeyRow {
    pub id: i64,
    pub user_id: i64,
    pub name: String,
    pub prefix: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub request_count: i64,
    pub revoked_at: Option<DateTime<Utc>>,
}
