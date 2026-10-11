-- Fullnode v0.8.0 (chain 21's cut): RPL-3 perps (`perp_*`, tags 34–39), multisig accounts
-- (`create_multisig`, `multisig_deposit`, `multisig_pay`, `multisig_rotate`, tags 40–43), the
-- validator set's RAND/USD price vote (`set_rand_price`, tag 44, kept in `staking_action` beside
-- `admit_validator`), and the aggregate's new `payout_total` and `layout`. Each new action is kept
-- whole in its own JSONB column; a `multisig_pay`'s payout notes link to it through `payout_cms`,
-- as an invoke's do.
--
-- Every one of these is refused on chain 20, so no row there carries one. A database that already
-- indexed any of them under an older build holds them as bare kind tags with no detail; only then
-- is the chain data dropped and re-indexed from height 0 (users, sessions and API keys are kept).

ALTER TABLE transactions
    ADD COLUMN perp_action      JSONB,
    ADD COLUMN multisig_action  JSONB,
    ADD COLUMN aggregate_action JSONB;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM transactions WHERE kind IN (
        'perp_deposit', 'perp_order', 'perp_cancel', 'perp_withdraw', 'perp_oracle', 'perp_state_proof',
        'create_multisig', 'multisig_deposit', 'multisig_pay', 'multisig_rotate',
        'set_rand_price', 'aggregate')) THEN
        TRUNCATE nullifiers, notes, receipts, programs, transactions, blocks, validators RESTART IDENTITY CASCADE;
        UPDATE indexer_state SET next_height = 0, next_leaf = 0, last_indexed_hash = NULL, is_syncing = FALSE,
            updated_at = NOW() WHERE id = 1;
    END IF;
END $$;
