//! Tolerant deserialization for the node's amount-shaped fields.
//!
//! The node is not consistent about whether the same conceptual field is a JSON number or a
//! decimal string across its own RPC methods — `rand_getBridgeState`/`rand_getAssets`'
//! `asset_json` sends `locked`/`minted_today`/`mint_cap_per_day` as **numbers**
//! (`crates/randprotocol-node/src/rpc.rs`'s `asset_json`, pinned by its own test:
//! `"locked": 600, "mint_cap_per_day": 100_000u64 * 100_000_000, "minted_today": 1_000`), while
//! the token RPC's `backing_json` for the *same* backing sends `locked`/`minted_today` as
//! **strings** and `mint_cap_per_day` as a **number** — three fields, two conventions, mixed
//! within one JSON object. Every deserializer here accepts either encoding for a given field, so
//! this crate does not have to track which renderer is which or fail closed (silently keeping
//! the bridge/token cache empty) when they disagree. Outbound serialization is unaffected: this
//! API keeps emitting its own established convention (decimal strings for amounts, plain numbers
//! for the few chain-wide constants like `registration_fee`) — only the *incoming* parse is
//! tolerant.

use serde::{de::Error as _, Deserialize, Deserializer};
use serde_json::Value;

/// A decimal-string amount: accepts a JSON number or a numeral string, normalises to a `String`
/// (unsigned, ASCII digits only) — this API's own convention for amounts everywhere else
/// (`docs/api.md`: "Amounts are strings of units").
pub fn amount<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let v = Value::deserialize(d)?;
    from_value(&v).ok_or_else(|| D::Error::custom(format!("not an amount: {v}")))
}

/// As [`amount`], but the field may be absent (`#[serde(default)]`) or `null`.
pub fn amount_opt<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    match Option::<Value>::deserialize(d)? {
        None | Some(Value::Null) => Ok(None),
        Some(v) => from_value(&v)
            .map(Some)
            .ok_or_else(|| D::Error::custom(format!("not an amount: {v}"))),
    }
}

/// A small integer field (`registration_fee`, and any sibling that turns out to need the same
/// tolerance later), accepting a JSON number or a numeral string, normalised to `i64`. The field
/// may be absent or `null`.
pub fn int_opt<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    match Option::<Value>::deserialize(d)? {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => n
            .as_i64()
            .map(Some)
            .ok_or_else(|| D::Error::custom(format!("integer out of range: {n}"))),
        Some(Value::String(s)) => s
            .trim()
            .parse::<i64>()
            .map(Some)
            .map_err(|_| D::Error::custom(format!("not an integer: {s:?}"))),
        Some(other) => Err(D::Error::custom(format!("not an integer: {other}"))),
    }
}

fn from_value(v: &Value) -> Option<String> {
    match v {
        // Only an integer: `n.is_u64()`/`is_i64()` is false for anything serde_json stored as
        // f64 (a literal with a `.` or exponent, or one too large for either integer variant),
        // so a fractional amount is rejected rather than silently truncated or reproduced as
        // "1.5". `Number::to_string()` on an integer variant reproduces its exact digits
        // (PosInt/NegInt, never routed through f64), so this loses no precision for any amount
        // that fits u64/i64 — the same assumption the rest of this API already makes for every
        // other amount field (e.g. the indexer's own `Units` wrapper does the same).
        Value::Number(n) if n.is_u64() || n.is_i64() => Some(n.to_string()),
        Value::String(s) if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) => {
            Some(s.clone())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Req {
        #[serde(deserialize_with = "amount")]
        a: String,
    }

    #[derive(Deserialize)]
    struct Opt {
        #[serde(default, deserialize_with = "amount_opt")]
        a: Option<String>,
    }

    #[derive(Deserialize)]
    struct Int {
        #[serde(default, deserialize_with = "int_opt")]
        a: Option<i64>,
    }

    #[test]
    fn amount_accepts_a_number_or_a_string() {
        assert_eq!(
            serde_json::from_value::<Req>(serde_json::json!({ "a": 600 }))
                .unwrap()
                .a,
            "600"
        );
        assert_eq!(
            serde_json::from_value::<Req>(serde_json::json!({ "a": "600" }))
                .unwrap()
                .a,
            "600"
        );
        assert_eq!(
            serde_json::from_value::<Req>(serde_json::json!({ "a": 100_000u64 * 100_000_000 }))
                .unwrap()
                .a,
            "10000000000000"
        );
        assert!(serde_json::from_value::<Req>(serde_json::json!({ "a": "not-a-number" })).is_err());
        assert!(serde_json::from_value::<Req>(serde_json::json!({ "a": null })).is_err());
        assert!(serde_json::from_value::<Req>(serde_json::json!({ "a": 1.5 })).is_err());
    }

    #[test]
    fn amount_opt_defaults_on_absence_or_null() {
        assert_eq!(
            serde_json::from_value::<Opt>(serde_json::json!({}))
                .unwrap()
                .a,
            None
        );
        assert_eq!(
            serde_json::from_value::<Opt>(serde_json::json!({ "a": null }))
                .unwrap()
                .a,
            None
        );
        assert_eq!(
            serde_json::from_value::<Opt>(serde_json::json!({ "a": 600 }))
                .unwrap()
                .a,
            Some("600".into())
        );
        assert_eq!(
            serde_json::from_value::<Opt>(serde_json::json!({ "a": "600" }))
                .unwrap()
                .a,
            Some("600".into())
        );
    }

    #[test]
    fn int_opt_accepts_a_number_or_a_numeral_string() {
        assert_eq!(
            serde_json::from_value::<Int>(serde_json::json!({}))
                .unwrap()
                .a,
            None
        );
        assert_eq!(
            serde_json::from_value::<Int>(serde_json::json!({ "a": 1_000_000_000u64 }))
                .unwrap()
                .a,
            Some(1_000_000_000)
        );
        assert_eq!(
            serde_json::from_value::<Int>(serde_json::json!({ "a": "1000000000" }))
                .unwrap()
                .a,
            Some(1_000_000_000)
        );
        assert!(serde_json::from_value::<Int>(serde_json::json!({ "a": "not-a-number" })).is_err());
    }
}
