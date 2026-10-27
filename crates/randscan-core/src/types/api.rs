use super::{ChainState, ChainView};
use serde::{Deserialize, Serialize};

/// Pagination query parameters (`page`, `limit`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pagination {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_page() -> u32 {
    1
}

fn default_limit() -> u32 {
    20
}

impl Default for Pagination {
    fn default() -> Self {
        Self { page: 1, limit: 20 }
    }
}

impl Pagination {
    pub fn page(&self) -> u32 {
        self.page.max(1)
    }

    pub fn limit(&self) -> i64 {
        self.limit.clamp(1, 100) as i64
    }

    pub fn offset(&self) -> i64 {
        (self.page() as i64 - 1) * self.limit()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub pagination: PaginationInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationInfo {
    pub page: u32,
    pub limit: u32,
    pub total: i64,
    pub total_pages: u32,
    pub has_next: bool,
    pub has_prev: bool,
}

impl PaginationInfo {
    pub fn new(p: &Pagination, total: i64) -> Self {
        let page = p.page();
        let limit = p.limit() as u32;
        let total_pages = ((total.max(0) as f64) / (limit as f64)).ceil() as u32;
        Self {
            page,
            limit,
            total,
            total_pages,
            has_next: page < total_pages,
            has_prev: page > 1,
        }
    }
}

/// API error body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub error: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

impl ApiError {
    pub fn not_found(resource: &str) -> Self {
        Self {
            error: "not_found".into(),
            message: format!("{} not found", resource),
            code: Some("NOT_FOUND".into()),
        }
    }

    pub fn bad_request(message: &str) -> Self {
        Self {
            error: "bad_request".into(),
            message: message.into(),
            code: Some("BAD_REQUEST".into()),
        }
    }

    pub fn internal(message: &str) -> Self {
        Self {
            error: "internal_error".into(),
            message: message.into(),
            code: Some("INTERNAL_ERROR".into()),
        }
    }

    /// Generic error with `code` derived from `error` (`invalid_credentials` -> `INVALID_CREDENTIALS`).
    pub fn new(error: &str, message: &str) -> Self {
        Self {
            error: error.into(),
            message: message.into(),
            code: Some(error.to_ascii_uppercase()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub q: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    #[serde(rename = "type")]
    pub result_type: SearchResultType,
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    pub url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchResultType {
    Block,
    Transaction,
    Validator,
    Program,
    Note,
    Nullifier,
}

// Note: `#[serde(flatten)]` does not work with query strings (numbers arrive as strings),
// so the paginated query structs repeat `page` and `limit`.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockQuery {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_limit")]
    pub limit: u32,
    pub proposer: Option<String>,
}

impl BlockQuery {
    pub fn pagination(&self) -> Pagination {
        Pagination {
            page: self.page,
            limit: self.limit,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionQuery {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_limit")]
    pub limit: u32,
    pub kind: Option<String>,
    pub height: Option<i64>,
    /// bond / unbond / withdraw / mint touching this validator address
    pub validator: Option<String>,
    /// deploy / call of this program id
    pub program: Option<String>,
}

impl TransactionQuery {
    pub fn pagination(&self) -> Pagination {
        Pagination {
            page: self.page,
            limit: self.limit,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitQuery {
    #[serde(default = "default_latest_limit")]
    pub limit: u32,
}

fn default_latest_limit() -> u32 {
    10
}

impl LimitQuery {
    pub fn limit(&self) -> i64 {
        self.limit.clamp(1, 100) as i64
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub database: bool,
    pub indexer: IndexerHealth,
    /// The genesis the indexed data belongs to (`None` before the indexer first reached a node).
    pub genesis_hash: Option<String>,
    /// The latest indexed block's timestamp (`None` with nothing indexed).
    pub head_timestamp_ms: Option<i64>,
    /// No indexed block for more than `STALL_MS`, or none at all.
    pub stalled: bool,
    /// The indexed chain is not the configured pin, or the node has moved to another chain and
    /// the indexer stopped writing.
    pub wrong_chain: bool,
}

/// `GET /api/v1/chain`: the [`ChainView`] of the indexed data, classified server-side against
/// the configured pin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainResponse {
    #[serde(flatten)]
    pub view: ChainView,
    pub state: ChainState,
    pub detail: String,
    pub expected_chain_id: i64,
    pub expected_genesis_hash: String,
    /// The indexer has stopped writing because the node reports a different chain than the one
    /// stored.
    pub halted: bool,
    /// Whether the indexer's last request to the node succeeded.
    pub node_connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexerHealth {
    pub connected: bool,
    pub synced: bool,
    pub current_height: i64,
    pub node_height: i64,
    pub lag: i64,
}
