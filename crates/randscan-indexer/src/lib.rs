//! RandScan Indexer - syncs the RAND chain from a `rand-node` JSON-RPC endpoint into PostgreSQL.

pub mod broadcast;
pub mod nodes;
pub mod processor;
pub mod rpc;
pub mod service;

pub use broadcast::*;
pub use nodes::*;
pub use processor::*;
pub use rpc::*;
pub use service::*;

use randscan_core::ChainPin;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct IndexerConfig {
    /// JSON-RPC endpoint of the node.
    pub rpc_url: String,
    /// Polling interval when caught up.
    pub poll_interval: Duration,
    /// Blocks per pass while catching up.
    pub batch_size: u64,
    /// Minimum interval between stats refreshes.
    pub stats_interval: Duration,
    /// Interval between peer/geolocation refreshes.
    pub nodes_interval: Duration,
    /// The chain this deployment expects (`RANDSCAN_EXPECTED_CHAIN_ID`,
    /// `RANDSCAN_EXPECTED_GENESIS`). The API classifies against it; the indexer re-indexes from
    /// height 0 only when the node moves *to* it (see `IndexerService::check_chain`).
    pub expected: ChainPin,
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            rpc_url: "http://127.0.0.1:8545".to_string(),
            poll_interval: Duration::from_millis(1000),
            batch_size: 200,
            stats_interval: Duration::from_secs(5),
            nodes_interval: Duration::from_secs(60),
            expected: ChainPin::default(),
        }
    }
}

impl IndexerConfig {
    pub fn from_env() -> Self {
        let d = Self::default();
        let num = |k: &str, d: u64| {
            std::env::var(k)
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(d)
        };
        Self {
            rpc_url: std::env::var("RPC_URL").unwrap_or(d.rpc_url),
            poll_interval: Duration::from_millis(num(
                "POLL_INTERVAL_MS",
                d.poll_interval.as_millis() as u64,
            )),
            batch_size: num("BATCH_SIZE", d.batch_size),
            stats_interval: Duration::from_secs(num(
                "STATS_INTERVAL_SECS",
                d.stats_interval.as_secs(),
            )),
            nodes_interval: Duration::from_secs(num(
                "NODES_INTERVAL_SECS",
                d.nodes_interval.as_secs(),
            )),
            expected: ChainPin::from_env(),
        }
    }
}
