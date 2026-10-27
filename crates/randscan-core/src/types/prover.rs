//! Delegated provers (fullnode `docs/prover.md`): services that make a wallet's bundle proof from
//! a sealed viewing-key job, for wallets that cannot prove themselves (a browser, a phone). The
//! chain has no record of them — a prover is a separate listener, never a method of the node's
//! RPC, and a pool hides its members behind one URL — so the explorer carries the ones it knows as
//! data, like the bridge's approved tokens, polls each public endpoint's `prover_info` for whether
//! it answers and how much it can take, and places each member host on the map by its IP.
//!
//! The member IPs are geolocated and never served: three of the pool's hosts are also bridge
//! guardian hosts, and a location (city, country, provider) is what the provers page is for.

use crate::GeoInfo;
use serde::{Deserialize, Serialize};

/// A host that proves jobs for a prover endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownProverMember {
    /// The operator's own name for the host.
    pub label: &'static str,
    /// Its public IPv4, for geolocation only.
    pub ip: &'static str,
}

/// A prover endpoint the explorer knows of, with the hosts behind it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownProver {
    pub name: &'static str,
    /// The `prover_*` JSON-RPC endpoint (`POST /`).
    pub url: &'static str,
    pub operator: &'static str,
    /// The pairing key's fingerprint a wallet pins (`prover_info.kem_fingerprint` reads it).
    pub fingerprint: &'static str,
    /// The page a wallet pairs from (`/.well-known/rand-prover.json`), when the operator serves one.
    pub pairing_url: Option<&'static str>,
    pub members: &'static [KnownProverMember],
}

/// Where the pool publishes its member list (fullnode `docs/prover.md`): one entry per member
/// with its own URL (`/m/<name>`) and its own pairing-key fingerprint. The explorer reads it on
/// every poll, so a member added or rotated shows up without a release.
pub const POOL_DESCRIPTOR_URL: &str =
    "https://prover.randprotocol.org/.well-known/rand-prover-pool.json";

/// The pool's members pinned at build time (the clients' `trusted-prover-pool.json`, 2026-10-27),
/// shown when the descriptor cannot be fetched or parsed. The pool's members are the hosts' own
/// tunnels (`/m/a`, `/m/b`), so their addresses are not public and no member host is placed on
/// the map; `members` is empty. Add a prover by adding an entry; the page and the API pick it up.
pub const KNOWN_PROVERS: &[KnownProver] = &[
    KnownProver {
        name: "RandProtocol prover a",
        url: "https://prover.randprotocol.org/m/a",
        operator: "Rand Protocol validators",
        fingerprint: "4SK4-P64N-XWEN-ZSZZ",
        pairing_url: None,
        members: &[],
    },
    KnownProver {
        name: "RandProtocol prover b",
        url: "https://prover.randprotocol.org/m/b",
        operator: "Rand Protocol validators",
        fingerprint: "0XZX-NZ1J-SPMB-GWZN",
        pairing_url: None,
        members: &[],
    },
];

/// Whether a member's reported key is the right one. A member with a pinned fingerprint
/// (`KNOWN_PROVERS`, from the clients' trusted list) is checked against the pin, so a descriptor
/// that lists another key for it is a mismatch, not a healthy member; an unpinned member (one
/// added to the descriptor since the build) can only be checked against its descriptor entry.
pub fn fingerprint_ok(url: &str, listed: &str, reported: &str) -> bool {
    match KNOWN_PROVERS.iter().find(|p| p.url == url) {
        Some(pin) => pin.fingerprint == listed && pin.fingerprint == reported,
        None => listed == reported,
    }
}

/// The operator shown for the pool's members.
pub const POOL_OPERATOR: &str = "Rand Protocol validators";

/// One member of the pool descriptor (the wallet's pairing `link` and anything else is ignored,
/// and never served: it carries a bearer token).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PoolMember {
    pub name: String,
    pub url: String,
    pub fingerprint: String,
}

/// `/.well-known/rand-prover-pool.json`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PoolDescriptor {
    pub members: Vec<PoolMember>,
}

impl PoolDescriptor {
    /// The members as prover views, in the descriptor's order. A member whose URL is not https or
    /// whose fingerprint is empty is dropped; an empty result is an error (use the pinned list).
    pub fn views(&self) -> Result<Vec<ProverView>, String> {
        let out: Vec<ProverView> = self
            .members
            .iter()
            .filter(|m| m.url.starts_with("https://") && !m.fingerprint.is_empty())
            .map(|m| {
                ProverView::new(
                    format!("RandProtocol prover {}", m.name),
                    m.url.trim_end_matches('/').to_string(),
                    POOL_OPERATOR.into(),
                    m.fingerprint.clone(),
                    None,
                )
            })
            .collect();
        if out.is_empty() {
            Err("the pool descriptor lists no usable member".into())
        } else {
            Ok(out)
        }
    }
}

/// `prover_info.queue`: jobs waiting, the most that may wait, and jobs being proved. Behind a
/// pool router these are the sums over the members that answered its last poll.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProverQueue {
    pub depth: i64,
    pub max: i64,
    pub proving: i64,
}

/// `prover_info.fee`: what the prover charges per bundle, or `None` for a free prover.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProverFee {
    #[serde(deserialize_with = "crate::amount::amount")]
    pub amount: String,
    pub address: String,
}

/// The public part of a prover's `prover_info` reply (fullnode `docs/prover.md` §6.1). The ML-KEM
/// key is left out: it identifies the prover, which a wallet needs and this page does not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProverInfoReply {
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub kem_fingerprint: Option<String>,
    #[serde(default)]
    pub hc_bundles: Vec<String>,
    #[serde(default)]
    pub backend: Option<String>,
    #[serde(default)]
    pub witness_kinds: Vec<String>,
    #[serde(default)]
    pub queue: Option<ProverQueue>,
    #[serde(default)]
    pub fee: Option<ProverFee>,
}

/// A member host as served: its label and where it is, never its address.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProverMember {
    pub label: String,
    pub geo: Option<GeoInfo>,
}

/// One prover as `GET /api/v1/provers` serves it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProverView {
    pub name: String,
    pub url: String,
    pub operator: String,
    pub fingerprint: String,
    pub pairing_url: Option<String>,
    /// Whether the endpoint answered `prover_info` on the last poll.
    pub up: bool,
    /// The last answer, kept while the endpoint is down so the page can say what it was.
    pub info: Option<ProverInfoReply>,
    /// Whether the fingerprint the endpoint reports is the one listed here.
    pub fingerprint_matches: Option<bool>,
    /// The poll's error when the endpoint did not answer.
    pub error: Option<String>,
    /// Unix milliseconds of the last poll, and of the last poll it answered.
    pub checked_at_ms: Option<i64>,
    pub last_up_ms: Option<i64>,
    pub members: Vec<ProverMember>,
}

impl ProverView {
    /// The view of a prover before its first poll.
    pub fn new(
        name: String,
        url: String,
        operator: String,
        fingerprint: String,
        pairing_url: Option<String>,
    ) -> Self {
        ProverView {
            name,
            url,
            operator,
            fingerprint,
            pairing_url,
            up: false,
            info: None,
            fingerprint_matches: None,
            error: None,
            checked_at_ms: None,
            last_up_ms: None,
            members: Vec::new(),
        }
    }

    /// The view of `p` before its first poll.
    pub fn unpolled(p: &KnownProver) -> Self {
        let mut v = Self::new(
            p.name.into(),
            p.url.into(),
            p.operator.into(),
            p.fingerprint.into(),
            p.pairing_url.map(str::to_string),
        );
        v.members = p
            .members
            .iter()
            .map(|m| ProverMember {
                label: m.label.into(),
                geo: None,
            })
            .collect();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pool's reply as the router serves it (fields trimmed to those kept), including a
    /// paying prover's fee as a decimal string.
    #[test]
    fn prover_info_parses_the_wire_shape_and_drops_the_key() {
        let r: ProverInfoReply = serde_json::from_value(serde_json::json!({
            "version": "0.8.1", "kem_fingerprint": "4SK4-P64N-XWEN-ZSZZ", "kem_ek": "c9d9…",
            "hc_bundles": ["60af"], "profiles": ["test", "production"], "backend": "cpu",
            "witness_kinds": ["viewing_key"], "queue": { "depth": 0, "max": 5, "proving": 1 },
            "fee": { "amount": "1000000", "address": "rand1abc" }, "allowed_origins": ["*"],
        }))
        .unwrap();
        assert_eq!(
            r.queue,
            Some(ProverQueue {
                depth: 0,
                max: 5,
                proving: 1
            })
        );
        assert_eq!(r.fee.as_ref().map(|f| f.amount.as_str()), Some("1000000"));
        assert!(serde_json::to_value(&r).unwrap().get("kem_ek").is_none());
        let free: ProverInfoReply =
            serde_json::from_value(serde_json::json!({ "fee": null })).unwrap();
        assert!(free.fee.is_none() && free.queue.is_none());
    }

    #[test]
    fn known_provers_are_well_formed_and_never_serve_an_address() {
        for p in KNOWN_PROVERS {
            assert!(p.url.starts_with("https://"), "{}", p.name);
            for m in p.members {
                assert!(
                    m.ip.parse::<std::net::Ipv4Addr>().is_ok(),
                    "{}: {}",
                    m.label,
                    m.ip
                );
            }
            let v = serde_json::to_value(ProverView::unpolled(p)).unwrap();
            for m in p.members {
                assert!(
                    !v.to_string().contains(m.ip),
                    "{} must not be served",
                    m.label
                );
            }
        }
    }

    #[test]
    fn pinned_list_is_the_pool_and_not_the_retired_shared_key() {
        let names: Vec<_> = KNOWN_PROVERS.iter().map(|p| p.url).collect();
        assert_eq!(
            names,
            [
                "https://prover.randprotocol.org/m/a",
                "https://prover.randprotocol.org/m/b"
            ]
        );
        assert!(KNOWN_PROVERS
            .iter()
            .all(|p| p.fingerprint != "RGTF-7HKJ-XZFV-GQ1J"));
    }

    /// The descriptor as served (2026-10-27): the pairing link carries a token and is not kept.
    #[test]
    fn pool_descriptor_parses_and_drops_the_link() {
        let d: PoolDescriptor = serde_json::from_value(serde_json::json!({
            "version": 1,
            "members": [
                { "name": "a", "url": "https://prover.randprotocol.org/m/a/",
                  "fingerprint": "4SK4-P64N-XWEN-ZSZZ", "link": "randprover:secret?token=t" },
                { "name": "x", "url": "http://insecure.example/m/x", "fingerprint": "AAAA" },
                { "name": "y", "url": "https://prover.randprotocol.org/m/y", "fingerprint": "" },
            ],
            "witness_kinds": ["viewing_key"], "fee": null,
        }))
        .unwrap();
        let views = d.views().unwrap();
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].url, "https://prover.randprotocol.org/m/a");
        assert_eq!(views[0].name, "RandProtocol prover a");
        assert!(!serde_json::to_string(&views).unwrap().contains("secret"));
        let empty = PoolDescriptor { members: vec![] };
        assert!(empty.views().is_err());
    }

    #[test]
    fn fingerprint_is_checked_against_the_pin_not_the_descriptor() {
        let a = "https://prover.randprotocol.org/m/a";
        let pin = "4SK4-P64N-XWEN-ZSZZ";
        assert!(fingerprint_ok(a, pin, pin));
        // the member reports a key other than its pin, and the descriptor agrees with it
        assert!(!fingerprint_ok(
            a,
            "AAAA-AAAA-AAAA-AAAA",
            "AAAA-AAAA-AAAA-AAAA"
        ));
        // the descriptor lists another key while the member still reports the pinned one
        assert!(!fingerprint_ok(a, "AAAA-AAAA-AAAA-AAAA", pin));
        // an unpinned member falls back to its descriptor entry
        let n = "https://prover.randprotocol.org/m/new";
        assert!(fingerprint_ok(n, "X", "X") && !fingerprint_ok(n, "X", "Y"));
    }
}
