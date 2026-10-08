//! The tokens the bridge accepts, one entry per (token, home chain), with the contract address
//! in two forms: as the chain's own explorers print it, and as the bridge registry stores it (a
//! 32-byte word: EVM and Tron addresses left-padded with 12 zero bytes, a Solana mint verbatim;
//! see `bridge/spec/ATTESTATION.md` §3.6). The bridge repo keeps this list only as admin-written
//! on-chain state per endpoint, so the explorer carries its own copy to name registry rows and
//! to tell users where to send funds. Addresses were checked against Etherscan, BscScan,
//! Tronscan, Solscan and Circle's developer docs on 2026-09-13.

use serde::{Deserialize, Serialize};

/// Bridged balances on Rand are held in attested units: always 8 decimals, whatever the token's
/// native decimals on its home chain (`ATTESTATION.md` §3.7).
pub const BRIDGE_DECIMALS: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenStatus {
    /// Deposits of this token are accepted.
    Allowed,
    /// Listed so the address is on record, but the bridge must not accept it.
    Discontinued,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovedToken {
    pub symbol: &'static str,
    pub name: &'static str,
    /// bridge chain id (2 Ethereum, 3 BSC, 4 Tron, 5 Solana)
    pub chain: i64,
    pub chain_name: &'static str,
    /// the token standard on that chain (ERC-20, BEP-20, TRC-20, SPL)
    pub standard: &'static str,
    /// the contract address (or mint) as the chain's explorers print it
    pub address: &'static str,
    /// the same address as the bridge registry stores it: 32 bytes, lowercase hex, no 0x
    pub token: &'static str,
    /// the token's decimals on its home chain; on Rand every bridged amount has 8
    pub decimals: u8,
    pub status: TokenStatus,
    /// the token's page on the chain's explorer
    pub explorer_url: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<&'static str>,
}

const fn evm(
    symbol: &'static str,
    name: &'static str,
    chain: i64,
    chain_name: &'static str,
    standard: &'static str,
    address: &'static str,
    token: &'static str,
    decimals: u8,
    explorer_url: &'static str,
) -> ApprovedToken {
    ApprovedToken {
        symbol,
        name,
        chain,
        chain_name,
        standard,
        address,
        token,
        decimals,
        status: TokenStatus::Allowed,
        explorer_url,
        note: None,
    }
}

/// The allowlist, grouped by home chain in bridge chain id order.
pub const APPROVED_TOKENS: &[ApprovedToken] = &[
    // ---- Ethereum (2) ----------------------------------------------------------------------
    evm("USDT", "Tether USD", 2, "Ethereum", "ERC-20",
        "0xdAC17F958D2ee523a2206206994597C13D831ec7",
        "000000000000000000000000dac17f958d2ee523a2206206994597c13d831ec7", 6,
        "https://etherscan.io/token/0xdac17f958d2ee523a2206206994597c13d831ec7"),
    evm("USDC", "USD Coin", 2, "Ethereum", "ERC-20",
        "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
        "000000000000000000000000a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", 6,
        "https://etherscan.io/token/0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
    // ---- BSC (3): Binance-Peg tokens, 18 decimals ------------------------------------------
    evm("USDT", "Binance-Peg BSC-USD", 3, "BSC", "BEP-20",
        "0x55d398326f99059fF775485246999027B3197955",
        "00000000000000000000000055d398326f99059ff775485246999027b3197955", 18,
        "https://bscscan.com/token/0x55d398326f99059ff775485246999027b3197955"),
    evm("USDC", "Binance-Peg USD Coin", 3, "BSC", "BEP-20",
        "0x8AC76a51cc950d9822D68b83fE1Ad97B32Cd580d",
        "0000000000000000000000008ac76a51cc950d9822d68b83fe1ad97b32cd580d", 18,
        "https://bscscan.com/token/0x8ac76a51cc950d9822d68b83fe1ad97b32cd580d"),
    // ---- Tron (4): base58check minus the 0x41 prefix, left-padded --------------------------
    evm("USDT", "Tether USD", 4, "Tron", "TRC-20",
        "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t",
        "000000000000000000000000a614f803b6fd780986a42c78ec9c7f77e6ded13c", 6,
        "https://tronscan.org/#/token20/TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t"),
    ApprovedToken {
        symbol: "USDC", name: "USD Coin", chain: 4, chain_name: "Tron", standard: "TRC-20",
        address: "TEkxiTehnzSmSe2XqrBj4w32RUN966rdz8",
        token: "0000000000000000000000003487b63d30b5b2c87fb7ffa8bcfade38eaac1abe",
        decimals: 6,
        status: TokenStatus::Discontinued,
        explorer_url: "https://tronscan.org/#/token20/TEkxiTehnzSmSe2XqrBj4w32RUN966rdz8",
        note: Some("Circle stopped minting USDC on Tron in February 2024 and ended redemptions in February 2025. The address is on record; deposits are not accepted."),
    },
    // ---- Solana (5): the mint pubkey is the 32-byte word itself ----------------------------
    evm("USDT", "Tether USD", 5, "Solana", "SPL",
        "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB",
        "ce010e60afedb22717bd63192f54145a3f965a33bb82d2c7029eb2ce1e208264", 6,
        "https://solscan.io/token/Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB"),
    evm("USDC", "USD Coin", 5, "Solana", "SPL",
        "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
        "c6fa7af3bedbad3a3d65f36aabc97431b1bbe4c2d2f6e0e47ca60203452f5d61", 6,
        "https://solscan.io/token/EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"),
];

/// The approved token a registry row `(chain, token)` names, if any. `token` is compared as the
/// registry stores it (32 bytes hex), case-insensitively and with an optional `0x`.
pub fn approved_token(chain: i64, token: &str) -> Option<&'static ApprovedToken> {
    let token = token.trim_start_matches("0x").to_ascii_lowercase();
    APPROVED_TOKENS
        .iter()
        .find(|t| t.chain == chain && t.token == token)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

    fn base58(s: &str) -> Vec<u8> {
        let mut out: Vec<u8> = vec![];
        for c in s.bytes() {
            let mut carry = ALPHABET.iter().position(|&a| a == c).expect("base58 digit") as u32;
            for b in out.iter_mut() {
                carry += (*b as u32) * 58;
                *b = (carry & 0xff) as u8;
                carry >>= 8;
            }
            while carry > 0 {
                out.push((carry & 0xff) as u8);
                carry >>= 8;
            }
        }
        for _ in s.bytes().take_while(|&c| c == b'1') {
            out.push(0);
        }
        out.reverse();
        out
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// The registry form of every entry follows from its printed address by the rule in
    /// ATTESTATION.md §3.6, so a typo in one form cannot hide behind the other.
    #[test]
    fn wire_form_follows_from_the_printed_address() {
        for t in APPROVED_TOKENS {
            let expected = match t.chain {
                2 | 3 => format!(
                    "{}{}",
                    "0".repeat(24),
                    t.address.trim_start_matches("0x").to_ascii_lowercase()
                ),
                4 => {
                    let raw = base58(t.address);
                    assert_eq!(
                        raw.len(),
                        25,
                        "{}: 21-byte body + 4-byte checksum",
                        t.address
                    );
                    assert_eq!(raw[0], 0x41, "{}: Tron mainnet prefix", t.address);
                    format!("{}{}", "0".repeat(24), hex(&raw[1..21]))
                }
                5 => {
                    let raw = base58(t.address);
                    assert_eq!(raw.len(), 32, "{}: a Solana pubkey", t.address);
                    hex(&raw)
                }
                other => panic!("unknown bridge chain {other}"),
            };
            assert_eq!(t.token, expected, "{} on {}", t.symbol, t.chain_name);
            assert_eq!(t.token.len(), 64);
        }
    }

    /// The Tron README's worked example (tron/README.md §3) is the USDT entry.
    #[test]
    fn tron_usdt_matches_the_bridge_readme() {
        let t = approved_token(
            4,
            "0x000000000000000000000000A614F803B6FD780986A42C78EC9C7F77E6DED13C",
        )
        .unwrap();
        assert_eq!(t.symbol, "USDT");
        assert_eq!(t.decimals, 6);
    }

    #[test]
    fn every_pair_is_listed_once_and_only_tron_usdc_is_discontinued() {
        let mut seen = std::collections::HashSet::new();
        for t in APPROVED_TOKENS {
            assert!(
                seen.insert((t.chain, t.symbol)),
                "duplicate {} on {}",
                t.symbol,
                t.chain_name
            );
            let discontinued = t.status == TokenStatus::Discontinued;
            assert_eq!(
                discontinued,
                t.chain == 4 && t.symbol == "USDC",
                "{} on {}",
                t.symbol,
                t.chain_name
            );
            assert_eq!(discontinued, t.note.is_some());
        }
        for chain in 2..=5 {
            assert_eq!(
                APPROVED_TOKENS.iter().filter(|t| t.chain == chain).count(),
                2,
                "USDT and USDC on chain {chain}"
            );
        }
        assert!(approved_token(2, "cc".repeat(32).as_str()).is_none());
        assert_eq!(
            approved_token(
                5,
                "ce010e60afedb22717bd63192f54145a3f965a33bb82d2c7029eb2ce1e208264"
            )
            .unwrap()
            .symbol,
            "USDT"
        );
    }
}
