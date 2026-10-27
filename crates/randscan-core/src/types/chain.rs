//! Chain identity and the "is this site on the live chain and up to date" check
//! (`docs/superpowers/specs/2026-10-27-site-sync-design.md` in the monorepo, §1–§3).
//!
//! A chain is identified by its id *and* its genesis hash: this project cuts chains often enough
//! that an id alone has been reused by a misconfigured node. Every site pins the chain it expects
//! ([`ChainPin`]), derives a [`ChainView`] of what it is showing, and classifies the pair with the
//! same rules ([`classify`]), so randscan.org, zusd.money, randbridge.org and randprotocol.org
//! agree on what "live", "behind", "stalled" and "wrong chain" mean.

use serde::{Deserialize, Serialize};

/// A head older than this is stalled: 24 blocks at the chain's 2.5 s block time.
pub const STALL_MS: i64 = 60_000;

/// An indexer more than this many blocks behind its node is behind.
pub const MAX_LAG: i64 = 10;

/// The chain a deployment expects when nothing configures another: chain 21, cut 2026-10-12.
/// Every pin moves at a chain cut; the monorepo's site-sync cron fails while one is left behind.
pub const DEFAULT_EXPECTED_CHAIN_ID: i64 = 21;
pub const DEFAULT_EXPECTED_GENESIS: &str =
    "ca76c2766ed6090ac6ba5dad86b724dd5d2b80c0edfaed287cd8b129e663ca9a";

/// Lowercase hex without a `0x` prefix, so two spellings of one hash compare equal.
pub fn normalize_hash(h: &str) -> String {
    let h = h.trim();
    h.strip_prefix("0x")
        .or_else(|| h.strip_prefix("0X"))
        .unwrap_or(h)
        .to_ascii_lowercase()
}

/// Whether two hex hashes are the same hash, ignoring case and a `0x` prefix.
pub fn same_hash(a: &str, b: &str) -> bool {
    normalize_hash(a) == normalize_hash(b)
}

/// The chain a deployment expects to be showing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainPin {
    pub chain_id: i64,
    pub genesis_hash: String,
}

impl Default for ChainPin {
    fn default() -> Self {
        Self {
            chain_id: DEFAULT_EXPECTED_CHAIN_ID,
            genesis_hash: DEFAULT_EXPECTED_GENESIS.to_string(),
        }
    }
}

impl ChainPin {
    pub fn new(chain_id: i64, genesis_hash: &str) -> Self {
        Self {
            chain_id,
            genesis_hash: normalize_hash(genesis_hash),
        }
    }

    /// `RANDSCAN_EXPECTED_CHAIN_ID` and `RANDSCAN_EXPECTED_GENESIS`, each falling back to the
    /// default pin when unset, empty or unparsable.
    pub fn from_env() -> Self {
        let d = Self::default();
        let chain_id = std::env::var("RANDSCAN_EXPECTED_CHAIN_ID")
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(d.chain_id);
        let genesis_hash = std::env::var("RANDSCAN_EXPECTED_GENESIS")
            .ok()
            .map(|v| normalize_hash(&v))
            .filter(|v| !v.is_empty())
            .unwrap_or(d.genesis_hash);
        Self {
            chain_id,
            genesis_hash,
        }
    }

    /// Whether `(chain_id, genesis_hash)` is the pinned chain.
    pub fn matches(&self, chain_id: i64, genesis_hash: &str) -> bool {
        self.chain_id == chain_id && same_hash(&self.genesis_hash, genesis_hash)
    }
}

/// What a site is showing (§2). `genesis_hash` and `head_*` describe the data the site serves;
/// `node_*` and `lag` describe the node it reads through, when there is one in between (here the
/// indexer). The node's own chain id and genesis are randscan's addition: they differ from the
/// stored ones exactly when the indexer has stopped writing because the node changed chains.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainView {
    pub chain_id: i64,
    pub genesis_hash: Option<String>,
    pub head_height: i64,
    pub head_timestamp_ms: Option<i64>,
    #[serde(default)]
    pub node_height: Option<i64>,
    #[serde(default)]
    pub lag: Option<i64>,
    #[serde(default)]
    pub node_chain_id: Option<i64>,
    #[serde(default)]
    pub node_genesis_hash: Option<String>,
}

/// The classifier's verdict, worst first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChainState {
    Unreachable,
    WrongChain,
    Stalled,
    Behind,
    Live,
}

impl ChainState {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChainState::Unreachable => "unreachable",
            ChainState::WrongChain => "wrong_chain",
            ChainState::Stalled => "stalled",
            ChainState::Behind => "behind",
            ChainState::Live => "live",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Classification {
    pub state: ChainState,
    /// One English sentence naming what is wrong (or that nothing is).
    pub detail: String,
}

/// Whether the view is not the pinned chain: a different chain id, a missing or different
/// genesis, or a node (behind an indexer) that reports a different chain than the pin.
pub fn is_wrong_chain(view: &ChainView, expected: &ChainPin) -> bool {
    let stored = view.chain_id == expected.chain_id
        && view
            .genesis_hash
            .as_deref()
            .is_some_and(|g| same_hash(g, &expected.genesis_hash));
    let node_ok = view.node_chain_id.is_none_or(|c| c == expected.chain_id)
        && view
            .node_genesis_hash
            .as_deref()
            .is_none_or(|g| same_hash(g, &expected.genesis_hash));
    !(stored && node_ok)
}

/// Whether the head is older than [`STALL_MS`] at `now_ms`, or has no timestamp at all.
pub fn is_stalled(view: &ChainView, now_ms: i64) -> bool {
    match view.head_timestamp_ms {
        Some(ts) => now_ms - ts > STALL_MS,
        None => true,
    }
}

fn short(h: &str) -> String {
    let h = normalize_hash(h);
    if h.len() > 8 {
        format!("{}…", &h[..8])
    } else {
        h
    }
}

/// §3: `None` is a source that could not be read (fetch failed, timed out, did not parse).
/// Checked in order: unreachable, wrong chain, stalled, behind, live.
pub fn classify(view: Option<&ChainView>, expected: &ChainPin, now_ms: i64) -> Classification {
    let Some(v) = view else {
        return Classification {
            state: ChainState::Unreachable,
            detail: "The chain source could not be reached.".into(),
        };
    };
    if is_wrong_chain(v, expected) {
        let shown = match &v.genesis_hash {
            Some(g) => format!("chain {} (genesis {})", v.chain_id, short(g)),
            None => format!("chain {} (no genesis hash)", v.chain_id),
        };
        let mut detail = format!(
            "Showing {shown}; expected chain {} (genesis {}).",
            expected.chain_id,
            short(&expected.genesis_hash)
        );
        if let (Some(c), Some(g)) = (v.node_chain_id, v.node_genesis_hash.as_deref()) {
            let differs =
                c != v.chain_id || v.genesis_hash.as_deref().is_none_or(|s| !same_hash(s, g));
            if differs {
                detail = format!(
                    "{} The node now serves chain {c} (genesis {}); indexing stopped.",
                    detail,
                    short(g)
                );
            }
        }
        return Classification {
            state: ChainState::WrongChain,
            detail,
        };
    }
    if is_stalled(v, now_ms) {
        let detail = match v.head_timestamp_ms {
            Some(ts) => format!(
                "No new block for {} s since #{}.",
                (now_ms - ts) / 1000,
                v.head_height
            ),
            None => "No block has been indexed yet.".into(),
        };
        return Classification {
            state: ChainState::Stalled,
            detail,
        };
    }
    if let Some(lag) = v.lag.filter(|l| *l > MAX_LAG) {
        return Classification {
            state: ChainState::Behind,
            detail: format!("{lag} blocks behind the node, at #{}.", v.head_height),
        };
    }
    Classification {
        state: ChainState::Live,
        detail: format!("Live at #{}.", v.head_height),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const G: &str = DEFAULT_EXPECTED_GENESIS;
    const NOW: i64 = 1_800_000_000_000;

    fn pin() -> ChainPin {
        ChainPin::default()
    }

    fn live_view() -> ChainView {
        ChainView {
            chain_id: 21,
            genesis_hash: Some(G.into()),
            head_height: 495_700,
            head_timestamp_ms: Some(NOW - 2_500),
            node_height: Some(495_700),
            lag: Some(0),
            node_chain_id: Some(21),
            node_genesis_hash: Some(G.into()),
        }
    }

    fn state(v: Option<&ChainView>) -> ChainState {
        classify(v, &pin(), NOW).state
    }

    #[test]
    fn a_fetch_error_is_unreachable() {
        assert_eq!(state(None), ChainState::Unreachable);
    }

    #[test]
    fn a_fresh_head_on_the_pinned_chain_is_live() {
        let c = classify(Some(&live_view()), &pin(), NOW);
        assert_eq!(c.state, ChainState::Live);
        assert_eq!(c.detail, "Live at #495700.");
    }

    #[test]
    fn the_stall_boundary_is_sixty_seconds_inclusive() {
        let mut v = live_view();
        v.head_timestamp_ms = Some(NOW - 60_000);
        assert_eq!(
            state(Some(&v)),
            ChainState::Live,
            "exactly 60 000 ms is fresh"
        );
        v.head_timestamp_ms = Some(NOW - 60_001);
        assert_eq!(state(Some(&v)), ChainState::Stalled, "60 001 ms is stalled");
    }

    #[test]
    fn a_missing_head_timestamp_is_stalled() {
        let mut v = live_view();
        v.head_timestamp_ms = None;
        let c = classify(Some(&v), &pin(), NOW);
        assert_eq!(c.state, ChainState::Stalled);
        assert_eq!(c.detail, "No block has been indexed yet.");
    }

    #[test]
    fn the_lag_boundary_is_ten_blocks_inclusive() {
        let mut v = live_view();
        v.lag = Some(10);
        assert_eq!(state(Some(&v)), ChainState::Live);
        v.lag = Some(11);
        assert_eq!(state(Some(&v)), ChainState::Behind);
        v.lag = None;
        assert_eq!(state(Some(&v)), ChainState::Live, "no indexer, no lag");
    }

    #[test]
    fn a_missing_genesis_is_the_wrong_chain() {
        let mut v = live_view();
        v.genesis_hash = None;
        assert_eq!(state(Some(&v)), ChainState::WrongChain);
    }

    #[test]
    fn a_different_genesis_is_the_wrong_chain() {
        let mut v = live_view();
        v.genesis_hash = Some("ab".repeat(32));
        assert_eq!(state(Some(&v)), ChainState::WrongChain);
    }

    #[test]
    fn a_chain_id_mismatch_with_a_matching_genesis_is_the_wrong_chain() {
        let mut v = live_view();
        v.chain_id = 20;
        assert_eq!(state(Some(&v)), ChainState::WrongChain);
    }

    #[test]
    fn a_node_that_changed_chains_under_the_indexer_is_the_wrong_chain() {
        let mut v = live_view();
        v.node_genesis_hash = Some("cd".repeat(32));
        let c = classify(Some(&v), &pin(), NOW);
        assert_eq!(c.state, ChainState::WrongChain);
        assert!(c.detail.contains("indexing stopped"), "{}", c.detail);
        let mut v = live_view();
        v.node_chain_id = Some(22);
        assert_eq!(state(Some(&v)), ChainState::WrongChain);
    }

    #[test]
    fn hashes_compare_without_case_or_prefix() {
        let mut v = live_view();
        v.genesis_hash = Some(format!("0x{}", G.to_ascii_uppercase()));
        assert_eq!(state(Some(&v)), ChainState::Live);
        assert!(ChainPin::new(21, &format!("0X{G}")).matches(21, G));
    }

    #[test]
    fn the_order_is_wrong_chain_then_stalled_then_behind() {
        let mut v = live_view();
        v.lag = Some(50);
        v.head_timestamp_ms = Some(NOW - 600_000);
        assert_eq!(state(Some(&v)), ChainState::Stalled, "stalled beats behind");
        v.chain_id = 20;
        assert_eq!(
            state(Some(&v)),
            ChainState::WrongChain,
            "wrong chain beats stalled"
        );
    }

    #[test]
    fn the_states_serialize_as_the_spec_spells_them() {
        for (s, name) in [
            (ChainState::Unreachable, "unreachable"),
            (ChainState::WrongChain, "wrong_chain"),
            (ChainState::Stalled, "stalled"),
            (ChainState::Behind, "behind"),
            (ChainState::Live, "live"),
        ] {
            assert_eq!(serde_json::to_value(s).unwrap(), name);
            assert_eq!(s.as_str(), name);
        }
    }

    #[test]
    fn the_default_pin_is_chain_21() {
        let p = ChainPin::default();
        assert_eq!(p.chain_id, 21);
        assert_eq!(p.genesis_hash, G);
    }
}
