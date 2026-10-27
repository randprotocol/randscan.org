-- The chain the indexed data belongs to, by id *and* genesis hash (site-sync design, 2026-10-27).
-- One row, written by the indexer when it first reaches a node and on a reset to the pinned chain.
-- While the node reports a different chain id or genesis than this row, the indexer stops writing
-- and /api/v1/chain and /api/v1/health say so: two chains never share one database.
--
-- A database that already holds blocks adopts what it holds: the chain id the indexer recorded
-- and the hash of block 0, which is the node's genesis hash (`rand_getGenesisHash` answers exactly
-- that). On the live explorer that is chain 21's genesis, so the first deploy carries on indexing
-- rather than refusing. A database without a block 0 or a recorded chain id starts with no row and
-- takes the node's identity on the indexer's first pass.

CREATE TABLE chain_meta (
    id            INTEGER PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    chain_id      BIGINT      NOT NULL,
    genesis_hash  VARCHAR(64) NOT NULL,
    adopted_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO chain_meta (id, chain_id, genesis_hash)
SELECT 1, s.chain_id, LOWER(b.hash)
FROM indexer_state s
JOIN blocks b ON b.height = 0
WHERE s.id = 1 AND s.chain_id IS NOT NULL;
