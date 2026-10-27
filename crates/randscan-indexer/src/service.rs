//! Sync loop: follow the node head, index new blocks and tree leaves, refresh the validator
//! register, the bridge state, the supply audit and the stats.

use crate::{BlockProcessor, Broadcaster, IndexerConfig, NodeTracker, RpcClient};
use anyhow::{anyhow, Result};
use randscan_core::{
    normalize_hash, same_hash, BridgeState, ChainPin, NetworkStats, NodeInfo, PerpsInfo, RandPrice,
    Supply, TokenList,
};
use randscan_db::{self as db, DbPool, NewValidator, StatsUpdate};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};

#[derive(Debug, Clone, Default)]
pub struct IndexerState {
    /// Highest indexed height (-1 if nothing indexed).
    pub current_height: i64,
    pub node_height: i64,
    pub is_syncing: bool,
    pub is_connected: bool,
    /// The chain id and genesis hash the node reported on the last pass (`None` before the first).
    pub node_chain_id: Option<i64>,
    pub node_genesis_hash: Option<String>,
    /// The node reports a different chain than the database holds, so the indexer has stopped
    /// writing until the node comes back to the stored chain or moves to the pinned one.
    pub halted: bool,
}

/// What a pass's chain check decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChainCheck {
    /// The node serves the chain the database holds; index on.
    Same,
    /// The chain data was dropped and the node's chain recorded; start a fresh pass.
    Reset,
    /// The node serves another chain; write nothing.
    Halted,
}

pub struct IndexerService {
    config: IndexerConfig,
    rpc: RpcClient,
    processor: BlockProcessor,
    pool: DbPool,
    broadcaster: Broadcaster,
    state: Arc<RwLock<IndexerState>>,
    chain: RwLock<Option<(i64, String, i16)>>,
    last_stats: RwLock<Option<Instant>>,
    /// The node's public bridge state and supply audit, refreshed with the stats.
    bridge: RwLock<Option<BridgeState>>,
    supply: RwLock<Option<Supply>>,
    /// The RPL token registry (`rand_getTokens`, paged to completion), refreshed with the stats.
    /// Cached rather than looked up per request: a token page tells the node nothing about which
    /// token any one caller cares about (unlike `rand_getToken`), and a token list page is what
    /// a wallet's own `resolve_asset` reads through.
    tokens: RwLock<Option<TokenList>>,
    /// Fullnode v0.8.0: the perps exchange's public state (`rand_getPerps`) and the voted RAND/USD
    /// price (`rand_getRandPrice`), refreshed with the stats. `None` on a chain without the
    /// section, or a node without the method.
    perps: RwLock<Option<PerpsInfo>>,
    rand_price: RwLock<Option<RandPrice>>,
    nodes: Arc<NodeTracker>,
}

impl IndexerService {
    pub fn new(config: IndexerConfig, pool: DbPool, broadcaster: Broadcaster) -> Self {
        let rpc = RpcClient::new(&config.rpc_url);
        let processor = BlockProcessor::new(pool.clone(), rpc.clone());
        let nodes = Arc::new(NodeTracker::new(
            rpc.clone(),
            pool.clone(),
            config.nodes_interval,
        ));
        Self {
            config,
            rpc,
            processor,
            pool,
            broadcaster,
            state: Arc::new(RwLock::new(IndexerState {
                current_height: -1,
                ..Default::default()
            })),
            chain: RwLock::new(None),
            last_stats: RwLock::new(None),
            bridge: RwLock::new(None),
            supply: RwLock::new(None),
            tokens: RwLock::new(None),
            perps: RwLock::new(None),
            rand_price: RwLock::new(None),
            nodes,
        }
    }

    pub fn rpc(&self) -> &RpcClient {
        &self.rpc
    }

    /// The chain this deployment expects (the configured pin).
    pub fn expected(&self) -> &ChainPin {
        &self.config.expected
    }

    /// The RPL token registry as last read from the node (`None` before the first refresh, or on
    /// a node without the method).
    pub async fn tokens(&self) -> Option<TokenList> {
        self.tokens.read().await.clone()
    }

    /// Current view of this node and its peers (nodes map).
    pub async fn nodes(&self) -> Vec<NodeInfo> {
        self.nodes.nodes().await
    }

    /// The known delegated provers as last polled, members geolocated (provers map).
    pub async fn provers(&self) -> Vec<randscan_core::ProverView> {
        self.nodes.provers().await
    }

    /// The bridge's public state as last read from the node (`None` before the first refresh).
    pub async fn bridge(&self) -> Option<BridgeState> {
        self.bridge.read().await.clone()
    }

    /// The supply audit as last read from the node (`None` on a node without it).
    pub async fn supply(&self) -> Option<Supply> {
        self.supply.read().await.clone()
    }

    /// The perps exchange as last read from the node: `None` on a chain without a `perps`
    /// section (the node's `{"enabled": false}`) and on a node predating the method.
    pub async fn perps(&self) -> Option<PerpsInfo> {
        self.perps.read().await.clone()
    }

    /// The validator set's voted RAND/USD price: `None` on a chain without `fees.usd_subsidy`.
    pub async fn rand_price(&self) -> Option<RandPrice> {
        self.rand_price.read().await.clone()
    }

    pub async fn get_state(&self) -> IndexerState {
        self.state.read().await.clone()
    }

    pub async fn run(&self) -> Result<()> {
        info!("indexer starting against {}", self.config.rpc_url);
        let st = db::get_indexer_state(self.pool.inner()).await?;
        self.state.write().await.current_height = st.next_height - 1;
        info!(
            "resuming from height {} (leaf {})",
            st.next_height, st.next_leaf
        );

        let tracker = self.nodes.clone();
        tokio::spawn(async move { tracker.run().await });

        loop {
            match self.sync_once().await {
                Ok(idle) => {
                    if idle {
                        sleep(self.config.poll_interval).await;
                    }
                }
                Err(e) => {
                    error!("sync error: {:#}", e);
                    self.state.write().await.is_connected = false;
                    sleep(Duration::from_secs(3)).await;
                }
            }
        }
    }

    /// The node's chain id and genesis hash. The genesis comes from `rand_getGenesisHash`, or
    /// from block 0's hash on a node predating that method (v0.3) or failing it; the two are the
    /// same hash.
    async fn node_identity(&self) -> Result<(i64, String)> {
        let chain_id = self.rpc.chain_id().await? as i64;
        let genesis = match self.rpc.genesis_hash().await.ok().flatten() {
            Some(g) if !g.trim().is_empty() => g,
            _ => self
                .rpc
                .block_by_height(0)
                .await?
                .map(|b| b.hash)
                .ok_or_else(|| anyhow!("the node serves no genesis hash and no block 0"))?,
        };
        Ok((chain_id, normalize_hash(&genesis)))
    }

    /// Make sure the database and the node are on one chain before anything is written. Runs on
    /// every pass (two cheap RPC calls), so a node re-pointed at another chain is caught before
    /// its first block lands.
    ///
    /// * No `chain_meta` row (a fresh database, or a reset that was cut short): the node's chain
    ///   is recorded. Data already in the database from a different chain id or genesis block is
    ///   dropped first, as the indexer always did when it met a new chain; that data never had a
    ///   recorded identity to defend. (Migration 015 records the identity of a database that
    ///   already holds blocks, so the live explorer arrives here only when it is empty.)
    /// * The node serves the recorded chain: index on.
    /// * The node serves the pinned chain and the database holds another: the operator moved the
    ///   pin for a chain cut, so the chain data is dropped (users, sessions and keys stay) and the
    ///   new chain indexed from height 0.
    /// * Anything else: the node is on a chain nobody asked for. Nothing is written until it comes
    ///   back; `/api/v1/chain` reports `wrong_chain` and `/api/v1/health` `degraded`.
    async fn check_chain(&self) -> Result<ChainCheck> {
        let (node_chain, node_genesis) = self.node_identity().await?;
        {
            let mut s = self.state.write().await;
            s.node_chain_id = Some(node_chain);
            s.node_genesis_hash = Some(node_genesis.clone());
        }
        let pool = self.pool.inner();
        let check = match db::get_chain_meta(pool).await? {
            Some(m) if m.chain_id == node_chain && same_hash(&m.genesis_hash, &node_genesis) => {
                ChainCheck::Same
            }
            Some(m) if self.config.expected.matches(node_chain, &node_genesis) => {
                warn!(
                    "the node moved from chain {} (genesis {}) to the pinned chain {} (genesis {}); \
                     resetting chain data and re-indexing from height 0",
                    m.chain_id, m.genesis_hash, node_chain, node_genesis
                );
                self.reset_to(node_chain, &node_genesis).await?;
                ChainCheck::Reset
            }
            Some(m) => {
                if !self.state.read().await.halted {
                    error!(
                        "the node serves chain {} (genesis {}) but the database holds chain {} \
                         (genesis {}) and the pin is chain {} (genesis {}); indexing stopped",
                        node_chain,
                        node_genesis,
                        m.chain_id,
                        m.genesis_hash,
                        self.config.expected.chain_id,
                        self.config.expected.genesis_hash
                    );
                }
                ChainCheck::Halted
            }
            None => {
                let st = db::get_indexer_state(pool).await?;
                let stored_genesis = db::get_block_hash_at(pool, 0).await?;
                let other_id = st.chain_id.filter(|c| *c != node_chain);
                let other_genesis = stored_genesis.filter(|g| !same_hash(g, &node_genesis));
                if other_id.is_some() || other_genesis.is_some() {
                    warn!(
                        "the database holds chain {:?} (genesis {:?}) but the node serves chain {} \
                         (genesis {}); resetting chain data and re-indexing from height 0",
                        st.chain_id, other_genesis, node_chain, node_genesis
                    );
                    self.reset_to(node_chain, &node_genesis).await?;
                    ChainCheck::Reset
                } else {
                    let mut conn = pool.acquire().await?;
                    if st.chain_id.is_none() {
                        db::set_chain_id(&mut conn, node_chain).await?;
                    }
                    db::set_chain_meta(&mut conn, node_chain, &node_genesis).await?;
                    info!("indexing chain {} (genesis {})", node_chain, node_genesis);
                    ChainCheck::Same
                }
            }
        };
        let halted = check == ChainCheck::Halted;
        {
            let mut s = self.state.write().await;
            if s.halted && !halted {
                info!("the node is back on the indexed chain; indexing resumed");
            }
            s.halted = halted;
        }
        Ok(check)
    }

    /// Drop the chain data, record `(chain_id, genesis)` as the indexed chain and forget every
    /// cached read of the old chain.
    async fn reset_to(&self, chain_id: i64, genesis: &str) -> Result<()> {
        self.processor.reset_chain(chain_id).await?;
        {
            let mut conn = self.pool.inner().acquire().await?;
            db::set_chain_meta(&mut conn, chain_id, genesis).await?;
        }
        *self.chain.write().await = None; // symbol/decimals may differ too
        *self.bridge.write().await = None;
        *self.supply.write().await = None;
        *self.tokens.write().await = None;
        *self.perps.write().await = None;
        *self.rand_price.write().await = None;
        self.state.write().await.current_height = -1;
        Ok(())
    }

    /// One pass. Returns true when caught up (caller sleeps).
    async fn sync_once(&self) -> Result<bool> {
        let head = self.rpc.head().await?;
        let node_height = head.height as i64;
        {
            let mut s = self.state.write().await;
            s.is_connected = true;
            s.node_height = node_height;
        }
        match self.check_chain().await? {
            ChainCheck::Same => {}
            ChainCheck::Reset => return Ok(false),
            ChainCheck::Halted => return Ok(true),
        }

        // A head below what we indexed, on the same chain, is a node that lost blocks or is
        // still catching up after a restart: wait for it (the check above rules out a new chain).
        let next = db::get_indexer_state(self.pool.inner()).await?.next_height;
        let behind = node_height - next + 1;
        let catching_up = behind > 5;
        {
            let mut s = self.state.write().await;
            if s.is_syncing != catching_up {
                s.is_syncing = catching_up;
                let _ = db::set_syncing(self.pool.inner(), catching_up).await;
                if catching_up {
                    info!(
                        "catching up: {} blocks behind ({} -> {})",
                        behind, next, node_height
                    );
                }
            }
        }
        if next > node_height {
            self.sync_notes().await;
            self.maybe_refresh_stats(false).await;
            return Ok(true);
        }

        let end = (next + self.config.batch_size as i64 - 1).min(node_height);
        let mut h = next;
        while h <= end {
            let block = match self.rpc.block_by_height(h as u64).await? {
                Some(b) => b,
                None => {
                    warn!("block {} not served yet", h);
                    return Ok(true);
                }
            };

            if h > 0 {
                let stored = db::get_block_hash_at(self.pool.inner(), h - 1).await?;
                match stored {
                    Some(ref parent) if parent == &block.parent => {}
                    Some(parent) => {
                        warn!(
                            "parent mismatch at {}: stored {} vs node {}",
                            h, parent, block.parent
                        );
                        self.processor.rewind_to(h - 1).await?;
                        return Ok(false);
                    }
                    None => {
                        warn!("missing block {} in database; rewinding", h - 1);
                        self.processor.rewind_to(h - 1).await?;
                        return Ok(false);
                    }
                }
            }

            let processed = self.processor.process_block(block).await?;
            self.state.write().await.current_height = h;
            self.broadcaster.block(processed.block);
            for tx in processed.transactions {
                self.broadcaster.transaction(tx);
            }
            if catching_up && h % 500 == 0 {
                info!("indexed height {} / {}", h, node_height);
            }
            h += 1;
        }

        if end >= node_height {
            if catching_up {
                info!("caught up at height {}", node_height);
            }
            self.sync_notes().await;
            self.maybe_refresh_stats(true).await;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Page the commitment tree from where the last pass stopped. Failures are logged: a leaf
    /// page that is missed now is fetched on the next pass.
    async fn sync_notes(&self) {
        let next_leaf = match db::get_indexer_state(self.pool.inner()).await {
            Ok(st) => st.next_leaf,
            Err(e) => {
                warn!("indexer state read failed: {}", e);
                return;
            }
        };
        if let Err(e) = self.processor.sync_notes(next_leaf).await {
            warn!("note sync from leaf {} failed: {:#}", next_leaf, e);
        }
    }

    async fn maybe_refresh_stats(&self, force: bool) {
        let due = {
            let last = self.last_stats.read().await;
            force
                || last
                    .map(|t| t.elapsed() >= self.config.stats_interval)
                    .unwrap_or(true)
        };
        if !due {
            return;
        }
        *self.last_stats.write().await = Some(Instant::now());
        if let Err(e) = self.refresh_validators().await {
            warn!("validator refresh failed: {:#}", e);
        }
        match self.rpc.bridge_state().await {
            Ok(b) => *self.bridge.write().await = Some(b),
            Err(e) => warn!("bridge state refresh failed: {:#}", e),
        }
        match self.rpc.supply().await {
            Ok(s) => *self.supply.write().await = s,
            Err(e) => warn!("supply refresh failed: {:#}", e),
        }
        match self.rpc.tokens_all().await {
            Ok(t) => *self.tokens.write().await = Some(t),
            Err(e) => warn!("token registry refresh failed: {:#}", e),
        }
        match self.rpc.perps().await {
            Ok(p) => *self.perps.write().await = p,
            Err(e) => warn!("perps refresh failed: {:#}", e),
        }
        match self.rpc.rand_price().await {
            Ok(p) => *self.rand_price.write().await = p,
            Err(e) => warn!("RAND price refresh failed: {:#}", e),
        }
        match self.refresh_stats().await {
            Ok(stats) => self.broadcaster.stats(stats),
            Err(e) => warn!("stats refresh failed: {:#}", e),
        }
    }

    async fn refresh_validators(&self) -> Result<()> {
        let register = self.rpc.validators().await?;
        let rows: Vec<NewValidator> = register
            .into_iter()
            .map(|v| NewValidator {
                pending: serde_json::Value::Array(
                    v.pending
                        .iter()
                        .map(|p| {
                            serde_json::json!({ "release_epoch": p.release_epoch, "amount": p.amount.0 })
                        })
                        .collect(),
                ),
                address: v.address,
                stake: v.stake.0,
                rewards: v.rewards.0,
                payout: v.payout,
                nonce: v.nonce as i64,
                // Before S2 the register is the genesis set and every entry is active.
                active: v.active.unwrap_or(true),
            })
            .collect();
        let mut conn = self.pool.inner().acquire().await?;
        db::replace_validators(&mut conn, &rows).await?;
        Ok(())
    }

    async fn chain_info(&self) -> Result<(i64, String, i16)> {
        if let Some(c) = self.chain.read().await.clone() {
            return Ok(c);
        }
        let id = self.rpc.chain_id().await? as i64;
        let token = self.rpc.token_info().await?;
        // The coin is RAND whatever string the node reports; only the decimals come from it.
        if token.symbol != randscan_core::TOKEN_SYMBOL {
            warn!(
                "node reports token symbol {:?}; the explorer shows {}",
                token.symbol,
                randscan_core::TOKEN_SYMBOL
            );
        }
        let c = (
            id,
            randscan_core::TOKEN_SYMBOL.to_string(),
            token.decimals as i16,
        );
        *self.chain.write().await = Some(c.clone());
        Ok(c)
    }

    async fn refresh_stats(&self) -> Result<NetworkStats> {
        let pool = self.pool.inner();
        let status = self.rpc.status().await?;
        let epoch = match self.rpc.epoch().await {
            Ok(e) => e,
            Err(e) => {
                warn!("epoch fetch failed: {:#}", e);
                None
            }
        };
        // The chain's identity and limits and the node's build. None of them stalls the stats:
        // a node without the method (before v0.3 / v0.4) or a failed read leaves the field
        // absent. They are re-read every refresh rather than cached per chain, because a
        // same-chain node update changes the build without any reset to hang an invalidation on.
        let limits = self.rpc.limits().await.unwrap_or_else(|e| {
            warn!("limits fetch failed: {:#}", e);
            None
        });
        let version = self.rpc.version().await.unwrap_or_else(|e| {
            warn!("version fetch failed: {:#}", e);
            None
        });
        let genesis_hash = self.rpc.genesis_hash().await.unwrap_or_else(|e| {
            warn!("genesis hash fetch failed: {:#}", e);
            None
        });
        let non_empty = |s: &String| !s.is_empty();
        let supply = self.supply.read().await.clone();
        let (chain_id, symbol, decimals) = self.chain_info().await?;
        let height = db::max_block_height(pool).await?.unwrap_or(-1).max(0);
        let total_transactions = db::count_transactions(pool, &db::TxFilter::default()).await?;
        let validators = db::list_validators(pool).await?;
        let (validator_count, active_validator_count) = db::count_validators(pool).await?;
        let total_stake = db::total_stake(pool).await?;
        let program_count = db::count_programs(pool).await?;
        let avg_block_time_ms = db::avg_block_time_ms(pool, 100).await?;
        // The leader of view v is entry v mod n of the active set in address order.
        let active: Vec<&str> = validators
            .iter()
            .filter(|v| v.active)
            .map(|v| v.address.as_str())
            .collect();
        let current_leader = if active.is_empty() {
            None
        } else {
            active
                .get((status.view % active.len() as u64) as usize)
                .map(|a| a.to_string())
        };
        let total_supply = supply
            .as_ref()
            .map(|s| s.total_supply.clone())
            .unwrap_or_else(|| "0".to_string());
        let pool_value = supply.as_ref().map(|s| s.pool_value.clone());

        db::update_network_stats(
            pool,
            &StatsUpdate {
                chain_id,
                symbol: &symbol,
                decimals,
                height,
                view: status.view as i64,
                total_transactions,
                notes: status.notes as i64,
                nullifiers: status.nullifiers as i64,
                validator_count,
                active_validator_count,
                total_stake: &total_stake,
                total_supply: &total_supply,
                pool_value: pool_value.as_deref(),
                program_count,
                avg_block_time_ms,
                peer_count: status.peer_count as i32,
                mempool_size: status.mempool_size as i32,
                node_syncing: status.syncing,
                faucet: status.faucet,
                confidential: status.confidential,
                current_leader: current_leader.as_deref(),
                tree_root: Some(&status.tree_root)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.as_str()),
                hc_bundle: Some(&status.hc_bundle)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.as_str()),
                epoch: epoch.as_ref().map(|e| e.epoch as i64),
                epoch_blocks: epoch.as_ref().map(|e| e.epoch_blocks as i64),
                genesis_hash: genesis_hash.as_deref(),
                node_version: version.as_ref().map(|v| v.version.as_str()),
                node_git_sha: version
                    .as_ref()
                    .map(|v| &v.git_sha)
                    .filter(|s| non_empty(s))
                    .map(|s| s.as_str()),
                // `rand_status` has carried the profile since before `rand_getVersion` existed.
                fri_profile: Some(&status.fri_profile)
                    .filter(|s| non_empty(s))
                    .map(|s| s.as_str()),
                limits: limits.as_ref().and_then(|l| serde_json::to_value(l).ok()),
                hc_auth: status.hc_auth.as_deref().filter(|s| !s.is_empty()),
                gas_prices: status.gas_prices.as_ref().map(|p| {
                    serde_json::json!({ "gas_price": p.gas_price.0, "byte_price": p.byte_price.0 })
                }),
            },
        )
        .await?;

        Ok(db::get_network_stats(pool).await?.into())
    }
}
