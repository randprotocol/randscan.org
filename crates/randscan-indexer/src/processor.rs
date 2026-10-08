//! Turns an RPC block into database rows: the transaction (bundle + action), its nullifiers,
//! call receipts and deployed programs. There are no accounts to refresh on this chain.

use crate::rpc::{RpcAction, RpcBlock, RpcBundle, RpcClient};
use anyhow::{Context, Result};
use randscan_core::{BlockSummary, TransactionSummary, TxKind};
use randscan_db::{self as db, DbPool, NewBlock, NewBundle, NewTx};
use std::collections::HashMap;
use tracing::{debug, warn};

pub struct BlockProcessor {
    pool: DbPool,
    rpc: RpcClient,
}

pub struct ProcessedBlock {
    pub block: BlockSummary,
    pub transactions: Vec<TransactionSummary>,
}

struct PreparedReceipt {
    tx_hash: String,
    program: String,
    tier: i32,
    outputs: Vec<i64>,
    height: i64,
    index: i32,
    h_in: String,
    h_pub: Option<String>,
}

/// What `rand_getProgram` adds to a deploy action: the code hash, the base pc and the digest of
/// the deploy-time public input (`None` without one).
struct ProgramMeta {
    base_pc: i64,
    code_hash: String,
    public_digest: Option<String>,
}

impl BlockProcessor {
    pub fn new(pool: DbPool, rpc: RpcClient) -> Self {
        Self { pool, rpc }
    }

    /// Store one block and everything derived from it, then advance `indexer_state.next_height`.
    pub async fn process_block(&self, block: RpcBlock) -> Result<ProcessedBlock> {
        let height = block.height as i64;

        // Receipts come from the node; fetch them before opening the DB transaction.
        let mut receipts = Vec::new();
        for tx in &block.transactions {
            // An invoke is a call with a transition: its receipt is a call's.
            if let RpcAction::Call { .. } | RpcAction::Invoke { .. } = tx.action {
                match self.rpc.receipt(&tx.hash).await {
                    Ok(Some(r)) => receipts.push(PreparedReceipt {
                        tx_hash: tx.hash.clone(),
                        program: r.program,
                        tier: r.tier as i32,
                        outputs: r.outputs,
                        height: r.height as i64,
                        index: r.index as i32,
                        h_in: r.h_in,
                        h_pub: r.h_pub,
                    }),
                    Ok(None) => warn!("no receipt yet for call {} at height {}", tx.hash, height),
                    Err(e) => warn!("receipt fetch failed for {}: {}", tx.hash, e),
                }
            }
        }

        // A deploy action names the program id and its length; the code hash (a Poseidon2
        // digest, distinct from the content id) and the base pc come from the node's record.
        let mut programs: HashMap<String, ProgramMeta> = HashMap::new();
        for tx in &block.transactions {
            if let RpcAction::Deploy { program, .. } = &tx.action {
                let meta = match self.rpc.program(program).await {
                    Ok(Some(p)) => ProgramMeta {
                        base_pc: p.base_pc as i64,
                        code_hash: p.code_hash,
                        public_digest: p.public_digest,
                    },
                    Ok(None) => ProgramMeta {
                        base_pc: 0,
                        code_hash: program.clone(),
                        public_digest: None,
                    },
                    Err(e) => {
                        warn!("program fetch {} failed: {}", program, e);
                        ProgramMeta {
                            base_pc: 0,
                            code_hash: program.clone(),
                            public_digest: None,
                        }
                    }
                };
                programs.insert(program.clone(), meta);
            }
        }

        let mut dbtx = self.pool.inner().begin().await?;

        db::insert_block(
            &mut dbtx,
            &NewBlock {
                hash: &block.hash,
                height,
                view: block.view as i64,
                parent: &block.parent,
                proposer: &block.proposer,
                timestamp_ms: block.timestamp_ms as i64,
                tx_root: &block.tx_root,
                state_root: &block.state_root,
                justify_view: block.justify_view as i64,
                tx_count: block.transactions.len() as i32,
            },
        )
        .await
        .with_context(|| format!("insert block {}", height))?;

        let mut summaries = Vec::with_capacity(block.transactions.len());

        for (i, tx) in block.transactions.iter().enumerate() {
            let tx_index = i as i32;
            let f = TxFields::from_action(&tx.action);
            let bundle = tx.bundle.as_ref().map(new_bundle);
            let fee = bundle
                .as_ref()
                .map(|b| b.fee.clone())
                .unwrap_or_else(|| "0".to_string());

            db::insert_transaction(
                &mut dbtx,
                &NewTx {
                    hash: &tx.hash,
                    block_hash: &block.hash,
                    height,
                    tx_index,
                    chain_id: tx.chain_id as i64,
                    timestamp_ms: block.timestamp_ms as i64,
                    kind: f.kind_tag,
                    bundle,
                    program_id: f.program,
                    words_len: f.words_len,
                    call_proof_len: f.call_proof_len,
                    input_envelope_len: f.input_envelope_len,
                    amount: f.amount.clone(),
                    cm: f.cm,
                    validator: f.validator,
                    registered: f.registered,
                    action_nonce: f.action_nonce,
                    attestation_len: f.attestation_len,
                    recipient: f.recipient,
                    note_time: f.note_time,
                    asset_index: f.asset_index,
                    relayer_fee: f.relayer_fee.clone(),
                    to_chain: f.to_chain,
                    bridge_to: f.bridge_to,
                    bridge_token: f.bridge_token,
                    deposit_r: f.deposit_r,
                    derived_cm: f.derived_cm.clone(),
                    pq_signers: f.pq_signers.clone(),
                    token_action: f.token_action.clone(),
                    bridge_governance: f.bridge_governance.clone(),
                    transition: f.transition.clone(),
                    payout_cms: f.payout_cms.clone(),
                    deposit_amount: f.deposit_amount.clone(),
                    release_amount: f.release_amount.clone(),
                    fee_note: f.fee_note.clone(),
                    fee_cm: f.fee_cm.clone(),
                    staking_action: f.staking_action.clone(),
                },
            )
            .await
            .with_context(|| format!("insert tx {}", tx.hash))?;

            if let RpcAction::Deploy {
                program,
                words,
                public_words_len,
            } = &tx.action
            {
                let meta = programs.get(program);
                db::insert_program(
                    &mut dbtx,
                    &db::NewProgram {
                        id: program,
                        deploy_tx: &tx.hash,
                        deployed_at_height: height,
                        base_pc: meta.map(|m| m.base_pc).unwrap_or(0),
                        words_len: *words as i64,
                        code_hash: meta.map(|m| m.code_hash.as_str()).unwrap_or(program),
                        // The length is the action's own; only the digest needs the node's record.
                        public_words_len: *public_words_len as i64,
                        public_digest: meta.and_then(|m| m.public_digest.as_deref()),
                    },
                )
                .await?;
            }

            summaries.push(TransactionSummary {
                hash: tx.hash.clone(),
                height,
                block_hash: block.hash.clone(),
                tx_index,
                kind: f.kind,
                fee,
                timestamp_ms: block.timestamp_ms as i64,
                has_bundle: tx.bundle.is_some(),
                program: f.program.map(str::to_string),
                validator: f.validator.map(str::to_string),
                amount: f.amount,
                asset_index: f.asset_index,
            });
        }

        for r in &receipts {
            db::insert_receipt(
                &mut dbtx,
                &r.tx_hash,
                &r.program,
                r.tier,
                &r.outputs,
                r.height,
                r.index,
                &r.h_in,
                r.h_pub.as_deref(),
            )
            .await?;
        }

        // Leaves of this height fetched before the block (after a rewind) get their tx now.
        db::link_notes_at(&mut dbtx, height).await?;
        db::set_next_height(&mut dbtx, height + 1, Some(&block.hash)).await?;
        dbtx.commit().await?;

        debug!("indexed block {} ({} txs)", height, summaries.len());

        Ok(ProcessedBlock {
            block: BlockSummary {
                hash: block.hash,
                height,
                view: block.view as i64,
                parent: block.parent,
                proposer: block.proposer,
                timestamp_ms: block.timestamp_ms as i64,
                tx_count: summaries.len() as i32,
                justify_view: block.justify_view as i64,
            },
            transactions: summaries,
        })
    }

    /// Fetch the commitment tree from leaf `next_leaf` onwards, a page at a time, until the
    /// node's reply is short. Returns the next leaf to fetch.
    pub async fn sync_notes(&self, mut next_leaf: i64) -> Result<i64> {
        const PAGE: u64 = 1000;
        loop {
            let rows = self.rpc.commitments(next_leaf as u64, PAGE).await?;
            if rows.is_empty() {
                return Ok(next_leaf);
            }
            let batch: Vec<db::NewNote> = rows
                .iter()
                .map(|r| db::NewNote {
                    leaf_index: r.index as i64,
                    cm: r.cm.clone(),
                    height: r.height as i64,
                    envelope: r.envelope.clone(),
                })
                .collect();
            let last = batch.last().map(|r| r.leaf_index).unwrap_or(next_leaf);
            let mut dbtx = self.pool.inner().begin().await?;
            db::insert_notes(&mut dbtx, &batch).await?;
            db::set_next_leaf(&mut dbtx, last + 1).await?;
            dbtx.commit().await?;
            debug!("indexed notes {}..={}", next_leaf, last);
            next_leaf = last + 1;
            if (rows.len() as u64) < PAGE {
                return Ok(next_leaf);
            }
        }
    }

    /// Forget everything indexed and start over at height 0 for `chain_id`. Users, API keys
    /// and sessions are not chain data and survive.
    pub async fn reset_chain(&self, chain_id: i64) -> Result<()> {
        let mut conn = self.pool.inner().acquire().await?;
        db::reset_chain_data(&mut conn, chain_id).await?;
        warn!(
            "chain data reset; re-indexing chain {} from height 0",
            chain_id
        );
        Ok(())
    }

    /// Drop blocks at and above `height` so they are re-fetched. Tree leaves of those heights go
    /// too: the tree past a lost block is not the tree the node will serve.
    pub async fn rewind_to(&self, height: i64) -> Result<()> {
        let mut dbtx = self.pool.inner().begin().await?;
        let n = db::delete_blocks_from(&mut dbtx, height).await?;
        let leaves = db::delete_notes_from(&mut dbtx, height).await?;
        let next_leaf = db::next_leaf_after_rewind(&mut dbtx).await?;
        db::set_next_leaf(&mut dbtx, next_leaf).await?;
        let last_hash = if height > 0 {
            db::get_block_hash_at(self.pool.inner(), height - 1).await?
        } else {
            None
        };
        db::set_next_height(&mut dbtx, height, last_hash.as_deref()).await?;
        dbtx.commit().await?;
        warn!(
            "rewound {} block(s) and {} leaves; next height {}, next leaf {}",
            n, leaves, height, next_leaf
        );
        Ok(())
    }
}

/// The chain-14 hidden-asset bundle: four slots, no public `asset` field (see `NewBundle`'s doc).
fn new_bundle(b: &RpcBundle) -> NewBundle {
    NewBundle {
        anchor: b.anchor.clone(),
        nullifiers: b.nullifiers.clone(),
        commitments: b.commitments.clone(),
        fee: b.fee.0.clone(),
        burn_a: b.burn_a.0.clone(),
        burn_r: b.burn_r.0.clone(),
        burn_asset: b.burn_asset as i64,
        time: b.time as i64,
        proof_len: b.proof_len as i64,
        envelope_len: [
            b.envelope_len[0] as i64,
            b.envelope_len[1] as i64,
            b.envelope_len[2] as i64,
            b.envelope_len[3] as i64,
        ],
        auth_commit: b.auth_commit.clone(),
        auth_proof_len: b.auth_proof_bytes as i64,
    }
}

/// The action columns one RPC action fills. Borrowed from the RPC value where practical;
/// `derived_cm`/`token_action`/`bridge_governance` are computed/serialised, so owned.
struct TxFields<'a> {
    kind: TxKind,
    /// What goes in the `kind` column: the explorer kind, or the node's own tag for unknown kinds.
    kind_tag: &'a str,
    program: Option<&'a str>,
    words_len: Option<i64>,
    call_proof_len: Option<i64>,
    input_envelope_len: Option<i64>,
    amount: Option<String>,
    cm: Option<&'a str>,
    validator: Option<&'a str>,
    registered: Option<bool>,
    action_nonce: Option<i64>,
    attestation_len: Option<i64>,
    recipient: Option<&'a str>,
    note_time: Option<i64>,
    asset_index: Option<i64>,
    relayer_fee: Option<String>,
    to_chain: Option<i32>,
    bridge_to: Option<&'a str>,
    bridge_token: Option<&'a str>,
    deposit_r: Option<&'a str>,
    /// The chain-computed note's commitment, for linking its leaf back to this transaction (see
    /// `NewTx::derived_cm`'s doc comment). `None` when this action creates none, or when a
    /// `token_mint`/`register_token`'s recipient does not parse as a shielded address (should
    /// never happen for a transaction the chain admitted; the indexer degrades to an unlinked
    /// leaf rather than failing the whole block).
    derived_cm: Option<String>,
    pq_signers: Option<Vec<i32>>,
    token_action: Option<serde_json::Value>,
    bridge_governance: Option<serde_json::Value>,
    /// invoke (RPL-2): the transition in full, and its payout notes' commitments (pays then
    /// mints) — leaves the chain appended, linked to this transaction as a `derived_cm` is.
    transition: Option<serde_json::Value>,
    payout_cms: Vec<String>,
    /// bridge_attest (v0.6.8): the depositor's net note value; bridge_burn: the release amount.
    deposit_amount: Option<String>,
    release_amount: Option<String>,
    /// bridge_attest / bridge_burn (v0.6.8): the fee note in full, and its commitment — a leaf the
    /// chain appended (after the deposit's, or after the bundle's four), linked back to this
    /// transaction as a `derived_cm` is.
    fee_note: Option<serde_json::Value>,
    fee_cm: Option<String>,
    /// admit_validator / slash_equivocation.
    staking_action: Option<serde_json::Value>,
}

impl<'a> TxFields<'a> {
    fn empty(kind: TxKind) -> Self {
        TxFields {
            kind,
            kind_tag: kind.as_str(),
            program: None,
            words_len: None,
            call_proof_len: None,
            input_envelope_len: None,
            amount: None,
            cm: None,
            validator: None,
            registered: None,
            action_nonce: None,
            attestation_len: None,
            recipient: None,
            note_time: None,
            asset_index: None,
            relayer_fee: None,
            to_chain: None,
            bridge_to: None,
            bridge_token: None,
            deposit_r: None,
            derived_cm: None,
            pq_signers: None,
            token_action: None,
            bridge_governance: None,
            transition: None,
            payout_cms: Vec::new(),
            deposit_amount: None,
            release_amount: None,
            fee_note: None,
            fee_cm: None,
            staking_action: None,
        }
    }

    fn from_action(action: &'a RpcAction) -> Self {
        match action {
            RpcAction::None => Self::empty(TxKind::Transfer),
            RpcAction::Mint { cm, amount, minter } => TxFields {
                cm: Some(cm),
                amount: Some(amount.0.clone()),
                validator: Some(minter),
                ..Self::empty(TxKind::Mint)
            },
            RpcAction::Deploy { program, words, .. } => TxFields {
                program: Some(program),
                words_len: Some(*words as i64),
                ..Self::empty(TxKind::Deploy)
            },
            RpcAction::Call {
                program,
                proof_len,
                input_envelope_len,
            } => TxFields {
                program: Some(program),
                call_proof_len: Some(*proof_len as i64),
                input_envelope_len: input_envelope_len.map(|n| n as i64),
                ..Self::empty(TxKind::Call)
            },
            RpcAction::Invoke {
                program,
                proof_len,
                input_envelope_len,
                transition,
            } => TxFields {
                program: Some(program),
                call_proof_len: Some(*proof_len as i64),
                input_envelope_len: input_envelope_len.map(|n| n as i64),
                transition: serde_json::to_value(transition.to_core()).ok(),
                payout_cms: transition.payout_cms(),
                ..Self::empty(TxKind::Invoke)
            },
            RpcAction::Bond {
                validator,
                amount,
                registered,
            } => TxFields {
                validator: Some(validator),
                amount: Some(amount.0.clone()),
                registered: Some(*registered),
                ..Self::empty(TxKind::Bond)
            },
            RpcAction::Unbond {
                validator,
                amount,
                nonce,
            } => TxFields {
                validator: Some(validator),
                amount: Some(amount.0.clone()),
                action_nonce: Some(*nonce as i64),
                ..Self::empty(TxKind::Unbond)
            },
            RpcAction::Withdraw {
                validator,
                amount,
                nonce,
                time,
            } => TxFields {
                validator: Some(validator),
                amount: Some(amount.0.clone()),
                action_nonce: Some(*nonce as i64),
                // The deposit note's own `time` word, as a bridge deposit's and a token mint's.
                note_time: time.map(|t| t as i64),
                ..Self::empty(TxKind::Withdraw)
            },
            RpcAction::BridgeAttest {
                attestation_len,
                recipient,
                asset_index,
                amount,
                time,
                r,
                commitment,
                pq_signers,
                deposit_amount,
                fee_note,
                ..
            } => TxFields {
                attestation_len: Some(*attestation_len as i64),
                recipient: Some(recipient),
                asset_index: asset_index.map(|a| a as i64),
                amount: amount.as_ref().map(|a| a.0.clone()),
                note_time: time.map(|t| t as i64),
                deposit_r: r.as_deref(),
                // The node computes this itself and reports it directly — no hashing needed.
                derived_cm: commitment.clone(),
                pq_signers: Some(pq_signers.iter().map(|&i| i as i32).collect()),
                deposit_amount: deposit_amount.as_ref().map(|a| a.0.clone()),
                fee_note: fee_note
                    .as_ref()
                    .and_then(|n| serde_json::to_value(n.to_core()).ok()),
                fee_cm: fee_note.as_ref().map(|n| n.commitment.clone()),
                ..Self::empty(TxKind::BridgeAttest)
            },
            RpcAction::BridgeBurn {
                asset,
                amount,
                relayer_fee,
                to_chain,
                token,
                to,
                release_amount,
                fee_note,
            } => TxFields {
                asset_index: Some(*asset as i64),
                amount: Some(amount.0.clone()),
                relayer_fee: Some(relayer_fee.0.clone()),
                to_chain: Some(i32::from(*to_chain)),
                bridge_to: Some(to),
                bridge_token: Some(token),
                release_amount: release_amount.as_ref().map(|a| a.0.clone()),
                fee_note: fee_note
                    .as_ref()
                    .and_then(|n| serde_json::to_value(n.to_core()).ok()),
                fee_cm: fee_note.as_ref().map(|n| n.commitment.clone()),
                ..Self::empty(TxKind::BridgeBurn)
            },
            RpcAction::RegisterToken {
                name,
                symbol,
                decimals,
                authority,
                index,
                initial_amount,
                initial,
            } => {
                let action = randscan_core::TokenAction::RegisterToken {
                    name: name.clone(),
                    symbol: symbol.clone(),
                    decimals: *decimals as i32,
                    authority: authority.clone(),
                    index: *index as i64,
                    initial_amount: initial_amount.as_ref().map(|a| a.0.clone()),
                    initial: initial.as_ref().map(|m| randscan_core::InitialMint {
                        amount: m.amount.0.clone(),
                        recipient: m.recipient.clone(),
                        time: m.time as i64,
                        r: m.r.clone(),
                    }),
                };
                // The registration's initial mint is the one derived note register_token appends
                // (register_token itself creates no other leaf).
                let derived_cm = initial.as_ref().and_then(|m| {
                    randscan_core::notecommit::mint_commitment_hex(
                        &m.recipient,
                        m.amount.0.parse().unwrap_or(0),
                        *index,
                        m.time as u32,
                        &m.r,
                    )
                });
                TxFields {
                    asset_index: Some(*index as i64),
                    amount: initial_amount.as_ref().map(|a| a.0.clone()),
                    recipient: initial.as_ref().map(|m| m.recipient.as_str()),
                    note_time: initial.as_ref().map(|m| m.time as i64),
                    deposit_r: initial.as_ref().map(|m| m.r.as_str()),
                    derived_cm,
                    token_action: serde_json::to_value(&action).ok(),
                    ..Self::empty(TxKind::RegisterToken)
                }
            }
            RpcAction::TokenMint {
                asset,
                amount,
                recipient,
                time,
                r,
                nonce,
            } => {
                let action = randscan_core::TokenAction::TokenMint {
                    asset: *asset as i64,
                    amount: amount.0.clone(),
                    recipient: recipient.clone(),
                    time: *time as i64,
                    r: r.clone(),
                    nonce: *nonce as i64,
                };
                let derived_cm = randscan_core::notecommit::mint_commitment_hex(
                    recipient,
                    amount.0.parse().unwrap_or(0),
                    *asset,
                    *time as u32,
                    r,
                );
                TxFields {
                    asset_index: Some(*asset as i64),
                    amount: Some(amount.0.clone()),
                    recipient: Some(recipient),
                    note_time: Some(*time as i64),
                    action_nonce: Some(*nonce as i64),
                    deposit_r: Some(r),
                    derived_cm,
                    token_action: serde_json::to_value(&action).ok(),
                    ..Self::empty(TxKind::TokenMint)
                }
            }
            RpcAction::SetAuthority {
                asset,
                nonce,
                new_authority,
            } => {
                let action = randscan_core::TokenAction::SetAuthority {
                    asset: *asset as i64,
                    nonce: *nonce as i64,
                    new_authority: new_authority.clone(),
                };
                TxFields {
                    asset_index: Some(*asset as i64),
                    action_nonce: Some(*nonce as i64),
                    token_action: serde_json::to_value(&action).ok(),
                    ..Self::empty(TxKind::SetAuthority)
                }
            }
            RpcAction::TokenBurn { asset, amount } => {
                let action = randscan_core::TokenAction::TokenBurn {
                    asset: *asset as i64,
                    amount: amount.0.clone(),
                };
                TxFields {
                    asset_index: Some(*asset as i64),
                    amount: Some(amount.0.clone()),
                    token_action: serde_json::to_value(&action).ok(),
                    ..Self::empty(TxKind::TokenBurn)
                }
            }
            RpcAction::PauseMints { nonce } => {
                let action = randscan_core::BridgeGovernanceAction::PauseMints {
                    nonce: *nonce as i64,
                };
                TxFields {
                    action_nonce: Some(*nonce as i64),
                    bridge_governance: serde_json::to_value(&action).ok(),
                    ..Self::empty(TxKind::PauseMints)
                }
            }
            RpcAction::UnpauseMints { nonce, pq_signers } => {
                let action = randscan_core::BridgeGovernanceAction::UnpauseMints {
                    nonce: *nonce as i64,
                    pq_signers: pq_signers.clone(),
                };
                TxFields {
                    action_nonce: Some(*nonce as i64),
                    pq_signers: Some(pq_signers.iter().map(|&i| i as i32).collect()),
                    bridge_governance: serde_json::to_value(&action).ok(),
                    ..Self::empty(TxKind::UnpauseMints)
                }
            }
            RpcAction::RegisterBridgedToken {
                name,
                symbol,
                salt,
                chain,
                token,
                decimals,
                nonce,
                asset_id,
                pq_signers,
            } => {
                let action = randscan_core::BridgeGovernanceAction::RegisterBridgedToken {
                    name: name.clone(),
                    symbol: symbol.clone(),
                    salt: salt.clone(),
                    chain: *chain,
                    token: token.clone(),
                    decimals: *decimals as i32,
                    nonce: *nonce as i64,
                    asset_id: asset_id.clone(),
                    pq_signers: pq_signers.clone(),
                };
                TxFields {
                    action_nonce: Some(*nonce as i64),
                    pq_signers: Some(pq_signers.iter().map(|&i| i as i32).collect()),
                    bridge_governance: serde_json::to_value(&action).ok(),
                    ..Self::empty(TxKind::RegisterBridgedToken)
                }
            }
            RpcAction::ListBacking {
                token_index,
                chain,
                token,
                decimals,
                nonce,
                pq_signers,
            } => {
                let action = randscan_core::BridgeGovernanceAction::ListBacking {
                    token_index: *token_index as i64,
                    chain: *chain,
                    token: token.clone(),
                    decimals: *decimals as i32,
                    nonce: *nonce as i64,
                    pq_signers: pq_signers.clone(),
                };
                TxFields {
                    asset_index: Some(*token_index as i64),
                    action_nonce: Some(*nonce as i64),
                    pq_signers: Some(pq_signers.iter().map(|&i| i as i32).collect()),
                    bridge_governance: serde_json::to_value(&action).ok(),
                    ..Self::empty(TxKind::ListBacking)
                }
            }
            RpcAction::AdmitValidator {
                candidate,
                candidate_key,
                voters,
            } => {
                let action = randscan_core::StakingAction::AdmitValidator {
                    candidate: candidate.clone(),
                    candidate_key: candidate_key.clone(),
                    voters: voters.clone(),
                };
                TxFields {
                    validator: Some(candidate),
                    staking_action: serde_json::to_value(&action).ok(),
                    ..Self::empty(TxKind::AdmitValidator)
                }
            }
            RpcAction::SlashEquivocation {
                offender,
                view,
                first,
                second,
            } => {
                let r = |h: &crate::rpc::RpcHeaderRef| randscan_core::HeaderRef {
                    hash: h.hash.clone(),
                    height: h.height as i64,
                };
                let action = randscan_core::StakingAction::SlashEquivocation {
                    offender: offender.clone(),
                    view: *view as i64,
                    first: r(first),
                    second: r(second),
                };
                TxFields {
                    validator: Some(offender),
                    staking_action: serde_json::to_value(&action).ok(),
                    ..Self::empty(TxKind::SlashEquivocation)
                }
            }
            RpcAction::RotatePqGuardians {
                new_pq_guardians,
                possession_signatures,
                nonce,
                pq_signers,
                v2,
            } => {
                let (action, kind) = if *v2 {
                    (
                        randscan_core::BridgeGovernanceAction::RotatePqGuardiansV2 {
                            new_pq_guardians: new_pq_guardians.clone(),
                            possession_signatures: possession_signatures.unwrap_or(0) as i64,
                            nonce: *nonce as i64,
                            pq_signers: pq_signers.clone(),
                        },
                        TxKind::RotatePqGuardiansV2,
                    )
                } else {
                    (
                        randscan_core::BridgeGovernanceAction::RotatePqGuardians {
                            new_pq_guardians: new_pq_guardians.clone(),
                            nonce: *nonce as i64,
                            pq_signers: pq_signers.clone(),
                        },
                        TxKind::RotatePqGuardians,
                    )
                };
                TxFields {
                    action_nonce: Some(*nonce as i64),
                    pq_signers: Some(pq_signers.iter().map(|&i| i as i32).collect()),
                    bridge_governance: serde_json::to_value(&action).ok(),
                    ..Self::empty(kind)
                }
            }
            RpcAction::RotatePauseKey {
                new_pause_key,
                nonce,
                pq_signers,
                v2,
            } => {
                let (new_pause_key, nonce_i, signers) =
                    (new_pause_key.clone(), *nonce as i64, pq_signers.clone());
                let (action, kind) = if *v2 {
                    (
                        randscan_core::BridgeGovernanceAction::RotatePauseKeyV2 {
                            new_pause_key,
                            nonce: nonce_i,
                            pq_signers: signers,
                        },
                        TxKind::RotatePauseKeyV2,
                    )
                } else {
                    (
                        randscan_core::BridgeGovernanceAction::RotatePauseKey {
                            new_pause_key,
                            nonce: nonce_i,
                            pq_signers: signers,
                        },
                        TxKind::RotatePauseKey,
                    )
                };
                TxFields {
                    action_nonce: Some(nonce_i),
                    pq_signers: Some(pq_signers.iter().map(|&i| i as i32).collect()),
                    bridge_governance: serde_json::to_value(&action).ok(),
                    ..Self::empty(kind)
                }
            }
            RpcAction::CancelRotation {
                rotation_kind,
                nonce,
            } => {
                let action = randscan_core::BridgeGovernanceAction::CancelRotation {
                    rotation_kind: rotation_kind.clone(),
                    nonce: *nonce as i64,
                };
                TxFields {
                    action_nonce: Some(*nonce as i64),
                    bridge_governance: serde_json::to_value(&action).ok(),
                    ..Self::empty(TxKind::CancelRotation)
                }
            }
            RpcAction::Unknown { kind: tag } => TxFields {
                // The column is VARCHAR(32); a longer tag is stored truncated on a char boundary.
                kind_tag: truncate_chars(tag, 32),
                ..Self::empty(TxKind::Other)
            },
        }
    }
}

fn truncate_chars(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::{RpcInitialMint, Units};

    #[test]
    fn a_plain_transfer_is_kind_transfer_with_no_action_fields() {
        let f = TxFields::from_action(&RpcAction::None);
        assert_eq!(f.kind, TxKind::Transfer);
        assert_eq!(f.kind_tag, "transfer");
        assert!(f.amount.is_none() && f.validator.is_none() && f.program.is_none());
    }

    #[test]
    fn a_burn_keeps_the_foreign_address_and_asset_index() {
        let k = RpcAction::BridgeBurn {
            asset: 2,
            amount: Units("400".into()),
            relayer_fee: Units("100".into()),
            to_chain: 5,
            token: "cd".repeat(32),
            to: "00".repeat(32),
            release_amount: None,
            fee_note: None,
        };
        let f = TxFields::from_action(&k);
        assert_eq!(f.kind, TxKind::BridgeBurn);
        assert_eq!(f.asset_index, Some(2));
        assert_eq!(f.amount.as_deref(), Some("400"));
        assert_eq!(f.relayer_fee.as_deref(), Some("100"));
        assert_eq!(f.to_chain, Some(5));
        assert_eq!(f.bridge_to.map(str::len), Some(64));
        assert_eq!(f.bridge_token.map(str::len), Some(64));
    }

    fn bundle4() -> RpcBundle {
        RpcBundle {
            anchor: "a".into(),
            nullifiers: ["n1".into(), "n2".into(), "n3".into(), "n4".into()],
            commitments: ["c1".into(), "c2".into(), "c3".into(), "c4".into()],
            fee: Units("1000000".into()),
            burn_a: Units("0".into()),
            burn_r: Units("0".into()),
            burn_asset: 0,
            time: 9,
            proof_len: 1,
            envelope_len: [1, 1, 1, 1],
            auth_commit: None,
            auth_proof_bytes: 0,
        }
    }

    #[test]
    fn a_transfer_bundle_has_four_slots_and_no_public_asset() {
        let nb = new_bundle(&bundle4());
        assert_eq!(nb.nullifiers.len(), 4);
        assert_eq!(nb.commitments.len(), 4);
        assert_eq!(nb.envelope_len.len(), 4);
        assert_eq!(nb.burn_a, "0");
        assert_eq!(nb.burn_r, "0");
        assert_eq!(nb.burn_asset, 0);
    }

    /// A zUSD-style bridged mint: the derived note's commitment must be computed (not left
    /// `None`), since the node never publishes it directly for a `token_mint`.
    #[test]
    fn a_token_mint_gets_a_derived_commitment_for_note_linking() {
        let mut raw = vec![1u8; 32];
        raw.extend_from_slice(&[2u8; 8]);
        let recipient = format!("rand1{}", bs58::encode(&raw).into_string());
        let k = RpcAction::TokenMint {
            asset: 3,
            amount: Units("700".into()),
            recipient: recipient.clone(),
            time: 41,
            r: "aa".repeat(32),
            nonce: 0,
        };
        let f = TxFields::from_action(&k);
        assert_eq!(f.kind, TxKind::TokenMint);
        assert_eq!(f.asset_index, Some(3));
        assert_eq!(f.amount.as_deref(), Some("700"));
        assert_eq!(f.recipient, Some(recipient.as_str()));
        let cm = f
            .derived_cm
            .expect("a token_mint's note commitment must be computed");
        assert_eq!(cm.len(), 64);
        // Deterministic: recomputing from the same fields gives the same commitment.
        let f2 = TxFields::from_action(&k);
        assert_eq!(f2.derived_cm, Some(cm));
        let action = f
            .token_action
            .expect("token_action carries the full payload");
        assert_eq!(action["kind"], "token_mint");
        assert_eq!(action["asset"], 3);
    }

    /// `register_token` with an initial mint gets the same treatment; without one, no leaf.
    #[test]
    fn register_token_derives_a_commitment_only_when_it_carries_an_initial_mint() {
        let mut raw = vec![1u8; 32];
        raw.extend_from_slice(&[2u8; 8]);
        let recipient = format!("rand1{}", bs58::encode(&raw).into_string());
        let with_initial = RpcAction::RegisterToken {
            name: "zUSD".into(),
            symbol: "zUSD".into(),
            decimals: 6,
            authority: "bridge".into(),
            index: 3,
            initial_amount: Some(Units("5000".into())),
            initial: Some(RpcInitialMint {
                amount: Units("5000".into()),
                recipient: recipient.clone(),
                time: 40,
                r: "bb".repeat(32),
            }),
        };
        let f = TxFields::from_action(&with_initial);
        assert_eq!(f.kind, TxKind::RegisterToken);
        assert_eq!(f.asset_index, Some(3));
        assert!(f.derived_cm.is_some());
        assert_eq!(f.token_action.unwrap()["kind"], "register_token");

        let without_initial = RpcAction::RegisterToken {
            name: "FIX".into(),
            symbol: "FIX".into(),
            decimals: 0,
            authority: "none".into(),
            index: 4,
            initial_amount: None,
            initial: None,
        };
        let f2 = TxFields::from_action(&without_initial);
        assert!(f2.derived_cm.is_none());
        assert_eq!(f2.asset_index, Some(4));
    }

    #[test]
    fn bridge_attest_copies_the_nodes_own_commitment_rather_than_hashing() {
        let k = RpcAction::BridgeAttest {
            attestation_len: 520,
            recipient: "rand1abc".into(),
            asset: Some(1),
            asset_index: Some(1),
            amount: Some(Units("1000".into())),
            time: Some(41),
            r: Some("aa".repeat(32)),
            commitment: Some("cc".repeat(32)),
            pq_signers: vec![0, 1],
            deposit_amount: None,
            fee_note: None,
        };
        let f = TxFields::from_action(&k);
        assert_eq!(f.kind, TxKind::BridgeAttest);
        assert_eq!(f.derived_cm.as_deref(), Some("cc".repeat(32).as_str()));
        assert_eq!(f.pq_signers, Some(vec![0, 1]));
        assert!(f.fee_cm.is_none() && f.deposit_amount.is_none());
    }

    /// v0.6.8 (`bridge.fees`): the amount stays the gross (what `locked` grew by), the net and the
    /// fee note ride beside it, and the fee note's leaf is linkable to the transaction.
    #[test]
    fn a_fee_split_deposit_keeps_the_gross_and_links_its_fee_note() {
        let fee = crate::rpc::RpcFeeNote {
            amount: Units("100000".into()),
            asset: 1,
            time: 1599,
            r: "ec".repeat(32),
            commitment: "47".repeat(32),
        };
        let k = RpcAction::BridgeAttest {
            attestation_len: 586,
            recipient: "rand1abc".into(),
            asset: Some(1),
            asset_index: Some(1),
            amount: Some(Units("100000000".into())),
            time: Some(1599),
            r: Some("63".repeat(32)),
            commitment: Some("d8".repeat(32)),
            pq_signers: vec![0, 3],
            deposit_amount: Some(Units("99900000".into())),
            fee_note: Some(fee.clone()),
        };
        let f = TxFields::from_action(&k);
        assert_eq!(f.amount.as_deref(), Some("100000000"));
        assert_eq!(f.deposit_amount.as_deref(), Some("99900000"));
        assert_eq!(
            f.derived_cm.as_deref(),
            Some("d8".repeat(32).as_str()),
            "the net deposit's leaf"
        );
        assert_eq!(f.fee_cm.as_deref(), Some("47".repeat(32).as_str()));
        assert_eq!(f.fee_note.as_ref().unwrap()["amount"], "100000");

        let burn = RpcAction::BridgeBurn {
            asset: 1,
            amount: Units("500000".into()),
            relayer_fee: Units("0".into()),
            to_chain: 2,
            token: "cd".repeat(32),
            to: "00".repeat(32),
            release_amount: Some(Units("499500".into())),
            fee_note: Some(fee),
        };
        let f = TxFields::from_action(&burn);
        assert_eq!(
            (f.amount.as_deref(), f.release_amount.as_deref()),
            (Some("500000"), Some("499500"))
        );
        assert!(f.fee_cm.is_some());
    }

    #[test]
    fn the_audit_v6_actions_are_stored_with_their_kinds() {
        let a = RpcAction::AdmitValidator {
            candidate: "2nRd".into(),
            candidate_key: "ab".into(),
            voters: vec!["v1".into()],
        };
        let f = TxFields::from_action(&a);
        assert_eq!(
            (f.kind, f.validator),
            (TxKind::AdmitValidator, Some("2nRd"))
        );
        assert_eq!(f.staking_action.unwrap()["kind"], "admit_validator");
        let r = RpcAction::RotatePqGuardians {
            new_pq_guardians: vec!["k".into()],
            possession_signatures: Some(1),
            nonce: 3,
            pq_signers: vec![1],
            v2: true,
        };
        let f = TxFields::from_action(&r);
        assert_eq!(
            (f.kind, f.kind_tag),
            (TxKind::RotatePqGuardiansV2, "rotate_pq_guardians_v2")
        );
        assert_eq!(f.bridge_governance.unwrap()["possession_signatures"], 1);
        let c = RpcAction::CancelRotation {
            rotation_kind: "pq_guardians".into(),
            nonce: 4,
        };
        let f = TxFields::from_action(&c);
        assert_eq!((f.kind, f.action_nonce), (TxKind::CancelRotation, Some(4)));
    }

    #[test]
    fn pause_and_unpause_are_bundle_less_governance_actions() {
        let f = TxFields::from_action(&RpcAction::PauseMints { nonce: 4 });
        assert_eq!(f.kind, TxKind::PauseMints);
        assert_eq!(f.action_nonce, Some(4));
        assert_eq!(f.bridge_governance.unwrap()["kind"], "pause_mints");

        let unpause = RpcAction::UnpauseMints {
            nonce: 5,
            pq_signers: vec![0, 2],
        };
        let f = TxFields::from_action(&unpause);
        assert_eq!(f.kind, TxKind::UnpauseMints);
        assert_eq!(f.pq_signers, Some(vec![0, 2]));
        assert_eq!(
            f.bridge_governance.unwrap()["pq_signers"],
            serde_json::json!([0, 2])
        );
    }

    #[test]
    fn a_mint_records_the_minting_validator_as_validator() {
        let k = RpcAction::Mint {
            cm: "cm".into(),
            amount: Units("7".into()),
            minter: "2nRd".into(),
        };
        let f = TxFields::from_action(&k);
        assert_eq!(f.kind, TxKind::Mint);
        assert_eq!(f.validator, Some("2nRd"));
        assert_eq!(f.cm, Some("cm"));
    }

    #[test]
    fn unknown_kind_is_stored_under_the_node_tag() {
        let k = RpcAction::Unknown {
            kind: "x".repeat(40),
        };
        let f = TxFields::from_action(&k);
        assert_eq!(f.kind, TxKind::Other);
        assert_eq!(f.kind_tag.len(), 32);
        let k = RpcAction::Unknown {
            kind: "slash".into(),
        };
        assert_eq!(TxFields::from_action(&k).kind_tag, "slash");
    }
}
