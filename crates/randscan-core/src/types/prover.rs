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

/// The provers this explorer shows. The validators' pool (fullnode `deploy/prover/README.md`,
/// live 2026-10-01): five validator-fleet hosts behind `prover.randprotocol.org`, each proving
/// one bundle at a time; the list matches the router's `PROVER_BACKENDS` tunnels on the web
/// droplet, in its order. Add a prover by adding an entry; the page and the API pick it up.
pub const KNOWN_PROVERS: &[KnownProver] = &[KnownProver {
    name: "RandProtocol prover pool",
    url: "https://prover.randprotocol.org",
    operator: "Rand Protocol validators",
    fingerprint: "RGTF-7HKJ-XZFV-GQ1J",
    pairing_url: Some("https://prover.randprotocol.org/.well-known/rand-prover.json"),
    members: &[
        KnownProverMember {
            label: "rand-node-a",
            ip: "139.59.238.151",
        },
        KnownProverMember {
            label: "rand-archive-2",
            ip: "206.81.29.236",
        },
        KnownProverMember {
            label: "rand-guardian-1",
            ip: "104.248.5.129",
        },
        KnownProverMember {
            label: "rand-guardian-2",
            ip: "137.184.12.108",
        },
        KnownProverMember {
            label: "rand-guardian-5",
            ip: "209.97.179.217",
        },
    ],
}];

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
    /// The view of `p` before its first poll.
    pub fn unpolled(p: &KnownProver) -> Self {
        ProverView {
            name: p.name.into(),
            url: p.url.into(),
            operator: p.operator.into(),
            fingerprint: p.fingerprint.into(),
            pairing_url: p.pairing_url.map(str::to_string),
            up: false,
            info: None,
            fingerprint_matches: None,
            error: None,
            checked_at_ms: None,
            last_up_ms: None,
            members: p
                .members
                .iter()
                .map(|m| ProverMember {
                    label: m.label.into(),
                    geo: None,
                })
                .collect(),
        }
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
            "version": "0.6.7", "kem_fingerprint": "RGTF-7HKJ-XZFV-GQ1J", "kem_ek": "c9d9…",
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
            assert!(!p.members.is_empty(), "{}", p.name);
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
}
