use crate::{ReceiptRow, Result, TxDetailRow, TxRow};
use sqlx::{PgConnection, PgPool};

/// Columns of a transaction list row.
pub const TX_COLS: &str = "hash, block_hash, height, tx_index, kind, fee::text AS fee, timestamp_ms, has_bundle, program_id, validator, amount::text AS amount, asset_index";

/// Every column: `TX_COLS` plus the bundle and the action fields.
pub const TX_DETAIL_COLS: &str = "hash, block_hash, height, tx_index, kind, fee::text AS fee, timestamp_ms, has_bundle, program_id, validator, amount::text AS amount, asset_index,
    chain_id, anchor, nullifier_1, nullifier_2, nullifier_3, nullifier_4,
    commitment_1, commitment_2, commitment_3, commitment_4,
    burn_a::text AS burn_a, burn_r::text AS burn_r, burn_asset, bundle_time, proof_len,
    envelope_len_1, envelope_len_2, envelope_len_3, envelope_len_4,
    words_len, call_proof_len, input_envelope_len, cm, registered, action_nonce, attestation_len, recipient, note_time,
    relayer_fee::text AS relayer_fee, to_chain, bridge_to, bridge_token, deposit_r, derived_cm, pq_signers,
    token_action, bridge_governance, auth_commit, auth_proof_len, transition,
    deposit_amount::text AS deposit_amount, release_amount::text AS release_amount, fee_note, staking_action";

/// The public fields of one hidden-asset bundle (chain 14: four input slots, four output slots,
/// dummies included; no public `asset` field — see `randscan_core::Bundle`'s doc comment).
#[derive(Debug, Clone, Default)]
pub struct NewBundle {
    pub anchor: String,
    pub nullifiers: [String; 4],
    pub commitments: [String; 4],
    pub fee: String,
    pub burn_a: String,
    pub burn_r: String,
    pub burn_asset: i64,
    pub time: i64,
    pub proof_len: i64,
    pub envelope_len: [i64; 4],
    /// Split authorisation (chain 17+): the bundle proof's `auth_commit`, hex; `None` when the
    /// node reported none.
    pub auth_commit: Option<String>,
    /// Bytes of the auth proof; 0 without an auth guest.
    pub auth_proof_len: i64,
}

#[derive(Debug, Clone, Default)]
pub struct NewTx<'a> {
    pub hash: &'a str,
    pub block_hash: &'a str,
    pub height: i64,
    pub tx_index: i32,
    pub chain_id: i64,
    pub timestamp_ms: i64,
    pub kind: &'a str,
    pub bundle: Option<NewBundle>,
    pub program_id: Option<&'a str>,
    pub words_len: Option<i64>,
    pub call_proof_len: Option<i64>,
    pub input_envelope_len: Option<i64>,
    pub amount: Option<String>,
    pub cm: Option<&'a str>,
    pub validator: Option<&'a str>,
    pub registered: Option<bool>,
    pub action_nonce: Option<i64>,
    pub attestation_len: Option<i64>,
    pub recipient: Option<&'a str>,
    pub note_time: Option<i64>,
    pub asset_index: Option<i64>,
    pub relayer_fee: Option<String>,
    pub to_chain: Option<i32>,
    pub bridge_to: Option<&'a str>,
    /// bridge_burn: the backing being redeemed (source-chain token address, 32 bytes hex).
    pub bridge_token: Option<&'a str>,
    /// bridge_attest / token_mint / register_token (initial mint): the note's blinding.
    pub deposit_r: Option<&'a str>,
    /// The chain-computed note's commitment, for linking its leaf to this transaction. Set from
    /// the RPC's own `commitment` field on a `bridge_attest`, and recomputed by the indexer
    /// (`randscan_core::notecommit::mint_commitment`) on a `token_mint` or a `register_token`
    /// with an initial mint.
    pub derived_cm: Option<String>,
    /// bridge_attest / unpause_mints / register_bridged_token / list_backing.
    pub pq_signers: Option<Vec<i32>>,
    /// register_token / token_mint / set_authority / token_burn, in full.
    pub token_action: Option<serde_json::Value>,
    /// pause_mints / unpause_mints / register_bridged_token / list_backing, in full.
    pub bridge_governance: Option<serde_json::Value>,
    /// invoke (RPL-2): the transition in full, and the commitments of its payout notes (pays
    /// then mints), each a leaf the chain appended that links back to this transaction.
    pub transition: Option<serde_json::Value>,
    pub payout_cms: Vec<String>,
    /// bridge_attest (v0.6.8): the depositor's net note value (`amount` stays the gross).
    pub deposit_amount: Option<String>,
    /// bridge_burn (v0.6.8): what the source contract releases (`amount` stays what was burned).
    pub release_amount: Option<String>,
    /// bridge_attest / bridge_burn (v0.6.8): the fee note in full, and its leaf's commitment.
    pub fee_note: Option<serde_json::Value>,
    pub fee_cm: Option<String>,
    /// admit_validator / slash_equivocation, in full.
    pub staking_action: Option<serde_json::Value>,
}

/// Insert the transaction and every nullifier its bundle published.
pub async fn insert_transaction(conn: &mut PgConnection, t: &NewTx<'_>) -> Result<()> {
    let b = t.bundle.as_ref();
    let fee = b.map(|b| b.fee.as_str()).unwrap_or("0");
    sqlx::query(
        "INSERT INTO transactions (hash, block_hash, height, tx_index, chain_id, timestamp_ms, kind, fee,
             has_bundle, anchor, nullifier_1, nullifier_2, nullifier_3, nullifier_4,
             commitment_1, commitment_2, commitment_3, commitment_4,
             burn_a, burn_r, burn_asset, bundle_time,
             proof_len, envelope_len_1, envelope_len_2, envelope_len_3, envelope_len_4,
             program_id, words_len, call_proof_len, input_envelope_len, amount, cm, validator, registered,
             action_nonce, attestation_len, recipient, note_time, asset_index, relayer_fee, to_chain, bridge_to,
             bridge_token, deposit_r, derived_cm, pq_signers, token_action, bridge_governance,
             auth_commit, auth_proof_len, transition, payout_cms,
             deposit_amount, release_amount, fee_note, fee_cm, staking_action)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8::numeric,
             $9, $10, $11, $12, $13, $14, $15, $16, $17, $18,
             $19::numeric, $20::numeric, $21, $22,
             $23, $24, $25, $26, $27,
             $28, $29, $30, $31, $32::numeric, $33, $34, $35,
             $36, $37, $38, $39, $40, $41::numeric, $42, $43,
             $44, $45, $46, $47, $48, $49,
             $50, $51, $52, $53,
             $54::numeric, $55::numeric, $56, $57, $58)",
    )
    .bind(t.hash)
    .bind(t.block_hash)
    .bind(t.height)
    .bind(t.tx_index)
    .bind(t.chain_id)
    .bind(t.timestamp_ms)
    .bind(t.kind)
    .bind(fee)
    .bind(b.is_some())
    .bind(b.map(|b| b.anchor.as_str()))
    .bind(b.map(|b| b.nullifiers[0].as_str()))
    .bind(b.map(|b| b.nullifiers[1].as_str()))
    .bind(b.map(|b| b.nullifiers[2].as_str()))
    .bind(b.map(|b| b.nullifiers[3].as_str()))
    .bind(b.map(|b| b.commitments[0].as_str()))
    .bind(b.map(|b| b.commitments[1].as_str()))
    .bind(b.map(|b| b.commitments[2].as_str()))
    .bind(b.map(|b| b.commitments[3].as_str()))
    .bind(b.map(|b| b.burn_a.as_str()))
    .bind(b.map(|b| b.burn_r.as_str()))
    .bind(b.map(|b| b.burn_asset))
    .bind(b.map(|b| b.time))
    .bind(b.map(|b| b.proof_len))
    .bind(b.map(|b| b.envelope_len[0]))
    .bind(b.map(|b| b.envelope_len[1]))
    .bind(b.map(|b| b.envelope_len[2]))
    .bind(b.map(|b| b.envelope_len[3]))
    .bind(t.program_id)
    .bind(t.words_len)
    .bind(t.call_proof_len)
    .bind(t.input_envelope_len)
    .bind(t.amount.as_deref())
    .bind(t.cm)
    .bind(t.validator)
    .bind(t.registered)
    .bind(t.action_nonce)
    .bind(t.attestation_len)
    .bind(t.recipient)
    .bind(t.note_time)
    .bind(t.asset_index)
    .bind(t.relayer_fee.as_deref())
    .bind(t.to_chain)
    .bind(t.bridge_to)
    .bind(t.bridge_token)
    .bind(t.deposit_r)
    .bind(t.derived_cm.as_deref())
    .bind(t.pq_signers.as_deref())
    .bind(t.token_action.as_ref())
    .bind(t.bridge_governance.as_ref())
    .bind(b.and_then(|b| b.auth_commit.as_deref()))
    .bind(b.map(|b| b.auth_proof_len).unwrap_or(0))
    .bind(t.transition.as_ref())
    .bind((!t.payout_cms.is_empty()).then_some(&t.payout_cms))
    .bind(t.deposit_amount.as_deref())
    .bind(t.release_amount.as_deref())
    .bind(t.fee_note.as_ref())
    .bind(t.fee_cm.as_deref())
    .bind(t.staking_action.as_ref())
    .execute(&mut *conn)
    .await?;

    if let Some(bundle) = t.bundle.as_ref() {
        for nf in &bundle.nullifiers {
            // A dummy input still publishes a nullifier, and the chain rejects a repeat, so a
            // conflict here can only be a re-index of the same block.
            sqlx::query(
                "INSERT INTO nullifiers (nullifier, tx_hash, height, tx_index) VALUES ($1, $2, $3, $4)
                 ON CONFLICT (nullifier) DO NOTHING",
            )
            .bind(nf)
            .bind(t.hash)
            .bind(t.height)
            .bind(t.tx_index)
            .execute(&mut *conn)
            .await?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_receipt(
    conn: &mut PgConnection,
    tx_hash: &str,
    program: &str,
    tier: i32,
    outputs: &[i64],
    height: i64,
    tx_index: i32,
    h_in: &str,
    h_pub: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO receipts (tx_hash, program, tier, outputs, height, tx_index, h_in, h_pub)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) ON CONFLICT (tx_hash) DO NOTHING",
    )
    .bind(tx_hash)
    .bind(program)
    .bind(tier)
    .bind(outputs)
    .bind(height)
    .bind(tx_index)
    .bind(h_in)
    .bind(h_pub)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn get_transaction(pool: &PgPool, hash: &str) -> Result<Option<TxDetailRow>> {
    let sql = format!("SELECT {TX_DETAIL_COLS} FROM transactions WHERE hash = $1");
    Ok(sqlx::query_as::<_, TxDetailRow>(&sql)
        .bind(hash)
        .fetch_optional(pool)
        .await?)
}

pub async fn get_transaction_summary(pool: &PgPool, hash: &str) -> Result<Option<TxRow>> {
    let sql = format!("SELECT {TX_COLS} FROM transactions WHERE hash = $1");
    Ok(sqlx::query_as::<_, TxRow>(&sql)
        .bind(hash)
        .fetch_optional(pool)
        .await?)
}

pub async fn get_receipt(pool: &PgPool, tx_hash: &str) -> Result<Option<ReceiptRow>> {
    Ok(sqlx::query_as::<_, ReceiptRow>(
        "SELECT tx_hash, program, tier, outputs, height, tx_index, h_in, h_pub FROM receipts WHERE tx_hash = $1",
    )
    .bind(tx_hash)
    .fetch_optional(pool)
    .await?)
}

pub async fn get_transactions_for_block(pool: &PgPool, block_hash: &str) -> Result<Vec<TxRow>> {
    let sql =
        format!("SELECT {TX_COLS} FROM transactions WHERE block_hash = $1 ORDER BY tx_index ASC");
    Ok(sqlx::query_as::<_, TxRow>(&sql)
        .bind(block_hash)
        .fetch_all(pool)
        .await?)
}

pub async fn get_latest_transactions(pool: &PgPool, limit: i64) -> Result<Vec<TxRow>> {
    let sql =
        format!("SELECT {TX_COLS} FROM transactions ORDER BY height DESC, tx_index DESC LIMIT $1");
    Ok(sqlx::query_as::<_, TxRow>(&sql)
        .bind(limit)
        .fetch_all(pool)
        .await?)
}

/// Filters of a transaction list (`GET /transactions`).
#[derive(Debug, Clone, Default)]
pub struct TxFilter<'a> {
    pub kind: Option<&'a str>,
    pub height: Option<i64>,
    pub validator: Option<&'a str>,
    pub program: Option<&'a str>,
}

const FILTER_WHERE: &str = "($1::text IS NULL OR kind = $1) AND ($2::bigint IS NULL OR height = $2)
         AND ($3::text IS NULL OR validator = $3) AND ($4::text IS NULL OR program_id = $4)";

pub async fn list_transactions(
    pool: &PgPool,
    offset: i64,
    limit: i64,
    f: &TxFilter<'_>,
) -> Result<Vec<TxRow>> {
    let sql = format!(
        "SELECT {TX_COLS} FROM transactions WHERE {FILTER_WHERE}
         ORDER BY height DESC, tx_index DESC LIMIT $5 OFFSET $6"
    );
    Ok(sqlx::query_as::<_, TxRow>(&sql)
        .bind(f.kind)
        .bind(f.height)
        .bind(f.validator)
        .bind(f.program)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?)
}

pub async fn count_transactions(pool: &PgPool, f: &TxFilter<'_>) -> Result<i64> {
    let sql = format!("SELECT COUNT(*) FROM transactions WHERE {FILTER_WHERE}");
    Ok(sqlx::query_scalar(&sql)
        .bind(f.kind)
        .bind(f.height)
        .bind(f.validator)
        .bind(f.program)
        .fetch_one(pool)
        .await?)
}

/// The latest calls and invokes of a program.
pub async fn list_program_calls(pool: &PgPool, program: &str, limit: i64) -> Result<Vec<TxRow>> {
    let sql = format!(
        "SELECT {TX_COLS} FROM transactions WHERE kind IN ('call', 'invoke') AND program_id = $1 ORDER BY height DESC, tx_index DESC LIMIT $2"
    );
    Ok(sqlx::query_as::<_, TxRow>(&sql)
        .bind(program)
        .bind(limit)
        .fetch_all(pool)
        .await?)
}

/// The transaction whose bundle (or mint, chain-computed note, or invoke payout) created the
/// note with commitment `cm`.
pub async fn find_transaction_by_commitment(pool: &PgPool, cm: &str) -> Result<Option<TxRow>> {
    let sql = format!(
        "SELECT {TX_COLS} FROM transactions
         WHERE commitment_1 = $1 OR commitment_2 = $1 OR commitment_3 = $1 OR commitment_4 = $1
            OR cm = $1 OR derived_cm = $1 OR $1 = ANY(payout_cms) OR fee_cm = $1 LIMIT 1"
    );
    Ok(sqlx::query_as::<_, TxRow>(&sql)
        .bind(cm)
        .fetch_optional(pool)
        .await?)
}

/// One point of a token's public supply history: every `register_token` (its initial mint only),
/// `token_mint` and `token_burn` that named registry index `asset_index`, oldest first. `delta` is
/// this row's own field to compute in the caller (register_token/token_mint positive from
/// `amount`, token_burn negative) since the JSONB shapes differ per kind.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TokenEventRow {
    pub tx_hash: String,
    pub height: i64,
    pub timestamp_ms: i64,
    pub kind: String,
    pub amount: Option<String>,
    pub token_action: Option<serde_json::Value>,
}

/// The transaction that registered a token: a `register_token` naming this registry index, or a
/// `register_bridged_token` naming this asset id (that action's `tx_json` carries no registry
/// index — only the id the chain will derive it under — so it is matched by id instead).
pub async fn find_token_deploy_tx(
    pool: &PgPool,
    asset_index: i64,
    asset_id_hex: &str,
) -> Result<Option<String>> {
    Ok(sqlx::query_scalar(
        "SELECT hash FROM transactions
         WHERE (kind = 'register_token' AND asset_index = $1)
            OR (kind = 'register_bridged_token' AND token_action->>'asset_id' = $2)
         ORDER BY height ASC LIMIT 1",
    )
    .bind(asset_index)
    .bind(asset_id_hex)
    .fetch_optional(pool)
    .await?)
}

pub async fn list_token_events(
    pool: &PgPool,
    asset_index: i64,
    limit: i64,
) -> Result<Vec<TokenEventRow>> {
    Ok(sqlx::query_as::<_, TokenEventRow>(
        "SELECT hash AS tx_hash, height, timestamp_ms, kind, amount::text AS amount, token_action
         FROM transactions
         WHERE asset_index = $1 AND kind IN ('register_token', 'token_mint', 'token_burn')
         ORDER BY height ASC, tx_index ASC LIMIT $2",
    )
    .bind(asset_index)
    .bind(limit)
    .fetch_all(pool)
    .await?)
}
