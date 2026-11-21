//! RandScan Core - wire types shared by the indexer, API, WebSocket server and clients.
//!
//! The chain is the Rand Protocol RAND chain served by `rand-node` (JSON-RPC `rand_*`).

pub mod amount;
pub mod error;
pub mod notecommit;
// Byte-identical to fullnode's copy (see its header), whose rows rustfmt would rewrap.
#[rustfmt::skip]
pub mod poseidon2_constants;
pub mod types;

pub use error::*;
pub use types::*;

/// Native token symbol.
pub const TOKEN_SYMBOL: &str = "RAND";

/// Native token decimals (1 RAND = 10^9 units).
pub const TOKEN_DECIMALS: u8 = 9;

/// Units per RAND.
pub const UNITS_PER_TOKEN: u128 = 1_000_000_000;

/// Format an amount in units (decimal string) as a RAND decimal string, trimming zeros.
pub fn format_units(units: &str) -> String {
    let value: u128 = units.parse().unwrap_or(0);
    let whole = value / UNITS_PER_TOKEN;
    let frac = value % UNITS_PER_TOKEN;
    if frac == 0 {
        return whole.to_string();
    }
    let frac = format!("{:09}", frac);
    format!("{}.{}", whole, frac.trim_end_matches('0'))
}

/// Format an amount in a token's own smallest unit (decimal string) as a decimal string with
/// that token's `decimals`, trimming trailing zeros — the general form of [`format_units`], which
/// is fixed to RAND's 9. Used for RPL token amounts (`register_token`/`token_mint`/`token_burn`'s
/// public amount, and a disclosed note's), whose unit is never RAND's.
pub fn format_token_units(units: &str, decimals: u32) -> String {
    let value: u128 = units.parse().unwrap_or(0);
    if decimals == 0 {
        return value.to_string();
    }
    let scale = 10u128.pow(decimals);
    let whole = value / scale;
    let frac = value % scale;
    if frac == 0 {
        return whole.to_string();
    }
    let frac = format!("{:0width$}", frac, width = decimals as usize);
    format!("{}.{}", whole, frac.trim_end_matches('0'))
}

/// Classify a search query the same way the API does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryKind {
    /// Decimal block height.
    Height(i64),
    /// 64 hex characters (block hash, transaction hash, program id), normalised to lowercase without 0x.
    Hash(String),
    /// Base58 address (32-44 chars).
    Address(String),
    Unknown,
}

pub fn classify_query(q: &str) -> QueryKind {
    let q = q.trim();
    if q.is_empty() {
        return QueryKind::Unknown;
    }
    if q.chars().all(|c| c.is_ascii_digit()) {
        if let Ok(h) = q.parse::<i64>() {
            return QueryKind::Height(h);
        }
    }
    let hex = q.strip_prefix("0x").unwrap_or(q);
    if hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return QueryKind::Hash(hex.to_ascii_lowercase());
    }
    let is_b58 = |c: char| c.is_ascii_alphanumeric() && !matches!(c, '0' | 'O' | 'I' | 'l');
    if (32..=44).contains(&q.len()) && q.chars().all(is_b58) {
        return QueryKind::Address(q.to_string());
    }
    QueryKind::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_units() {
        assert_eq!(format_units("0"), "0");
        assert_eq!(format_units("1500000000"), "1.5");
        assert_eq!(format_units("100000000225"), "100.000000225");
        assert_eq!(format_units("25"), "0.000000025");
    }

    #[test]
    fn formats_token_units_at_arbitrary_decimals() {
        // zUSD-shaped: 6 decimals.
        assert_eq!(format_token_units("1250000", 6), "1.25");
        assert_eq!(format_token_units("0", 6), "0");
        assert_eq!(format_token_units("1000000", 6), "1");
        assert_eq!(format_token_units("700", 0), "700");
        assert_eq!(format_token_units("5", 8), "0.00000005");
    }

    #[test]
    fn classifies_queries() {
        assert_eq!(classify_query("19"), QueryKind::Height(19));
        let h = "d4c75efbe141567eae72d5f639a1d444eb074d8b58a6592d95e01cd39a2364e3";
        assert_eq!(
            classify_query(&format!("0x{}", h.to_uppercase())),
            QueryKind::Hash(h.into())
        );
        assert_eq!(
            classify_query("ByDkxsEfDCR5DrmDufKftvcRsgvufypnZ4SgDQzJAQ7Z"),
            QueryKind::Address("ByDkxsEfDCR5DrmDufKftvcRsgvufypnZ4SgDQzJAQ7Z".into())
        );
        assert_eq!(classify_query("hello world"), QueryKind::Unknown);
    }
}
