use serde::{Deserialize, Serialize};

use super::TxKind;

/// One leaf of the commitment tree. Its value and owner are hidden; `tx_hash` is the indexed
/// transaction that created it when the commitment was on the wire (bundle outputs, mints), and
/// `null` for genesis, withdraw and bridge deposit notes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub leaf_index: i64,
    pub cm: String,
    pub height: i64,
    pub tx_hash: Option<String>,
}

/// A published nullifier and the transaction that spent the note.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Nullifier {
    pub nullifier: String,
    pub tx_hash: String,
    pub height: i64,
    pub tx_index: i32,
}

/// A tree leaf with the envelope its transaction published: what a viewing key or a
/// per-transaction key opens (in the browser; the explorer holds no keys).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteEnvelope {
    pub leaf_index: i64,
    pub cm: String,
    pub height: i64,
    pub tx_hash: Option<String>,
    /// `{ kem_ct, to_receiver, to_sender, body }` as hex, or `null` for a leaf indexed before
    /// envelopes were stored (re-fetched on the next pass).
    pub envelope: Option<serde_json::Value>,
    /// For a note the *chain* computed (a bridge deposit, a bridge fee note, an RPL mint or
    /// initial mint, an RPL-2 payout): every word of it but its owner, from the transaction's
    /// public fields, so a key holder rebuilds it with nothing decrypted (`rebuild_note` in the
    /// viewing wasm) — the only way to find a fee note, which has no envelope, and the recovery
    /// path for a deposit whose envelope is junk. `None` for a note a bundle created.
    #[serde(default)]
    pub public: Option<PublicNote>,
}

/// The public opening of a chain-computed note, less `pk` (the owner's, which the browser
/// supplies from its own key and checks by recomputing the commitment against the leaf) and
/// `from` (fixed by `source`: zero for `bridge_deposit` / `bridge_fee`, `MINT_FROM` for
/// `token_mint` / `initial_mint`, `PROGRAM_FROM` for `payout`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicNote {
    pub source: String,
    /// units of `asset`, decimal string
    pub amount: String,
    pub asset: i64,
    pub time: i64,
    pub r: String,
}

/// The fields of the transaction that created a leaf, as far as [`public_note_for`] needs them.
#[derive(Debug, Clone, Default)]
pub struct LeafTx<'a> {
    pub kind: &'a str,
    pub derived_cm: Option<&'a str>,
    pub fee_cm: Option<&'a str>,
    pub fee_note: Option<&'a serde_json::Value>,
    pub transition: Option<&'a serde_json::Value>,
    pub amount: Option<&'a str>,
    pub deposit_amount: Option<&'a str>,
    pub asset_index: Option<i64>,
    pub note_time: Option<i64>,
    pub deposit_r: Option<&'a str>,
}

/// The public opening of the leaf `cm` that `tx` created, when the chain computed it.
pub fn public_note_for(cm: &str, tx: &LeafTx<'_>) -> Option<PublicNote> {
    let eq = |a: Option<&str>| a.is_some_and(|a| a.eq_ignore_ascii_case(cm));
    let from_json = |source: &str, v: &serde_json::Value| -> Option<PublicNote> {
        let amount = match v.get("amount")? {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Number(n) => n.to_string(),
            _ => return None,
        };
        Some(PublicNote {
            source: source.into(),
            amount,
            asset: v.get("asset")?.as_i64()?,
            time: v.get("time")?.as_i64()?,
            r: v.get("r")?.as_str()?.to_string(),
        })
    };
    if eq(tx.fee_cm) {
        return from_json("bridge_fee", tx.fee_note?);
    }
    if eq(tx.derived_cm) {
        let source = match tx.kind {
            "bridge_attest" => "bridge_deposit",
            "token_mint" => "token_mint",
            "register_token" => "initial_mint",
            _ => return None,
        };
        // A deposit's note carries the net under `bridge.fees`; `amount` stays the gross.
        let amount = if tx.kind == "bridge_attest" {
            tx.deposit_amount.or(tx.amount)
        } else {
            tx.amount
        };
        return Some(PublicNote {
            source: source.into(),
            amount: amount?.to_string(),
            asset: tx.asset_index?,
            time: tx.note_time?,
            r: tx.deposit_r?.to_string(),
        });
    }
    if tx.kind == "invoke" {
        let t = tx.transition?;
        let payouts = ["pays", "mints"]
            .into_iter()
            .filter_map(|k| t.get(k)?.as_array())
            .flatten();
        for p in payouts {
            if eq(p.get("cm").and_then(|c| c.as_str())) {
                return from_json("payout", p);
            }
        }
    }
    None
}

/// Everything a key can be tried against for one transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionEnvelopes {
    pub hash: String,
    pub kind: TxKind,
    pub notes: Vec<NoteEnvelope>,
    /// For a call: the receipt's `h_in` and the sealed input transcript
    /// (`{ kem_ct, to_sender, to_auditor, body }`), when the caller published one.
    pub h_in: Option<String>,
    pub call_envelope: Option<serde_json::Value>,
}

/// A page of leaves with envelopes, oldest first, for a history scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteEnvelopePage {
    pub from_leaf: i64,
    pub next_leaf: Option<i64>,
    pub total_leaves: i64,
    pub notes: Vec<NoteEnvelope>,
}

#[cfg(test)]
mod public_note_tests {
    use super::*;

    /// Chain 20, block 1664: the bridge_attest's net deposit (leaf 19) and its fee note (leaf 20).
    #[test]
    fn a_fee_split_deposit_serves_both_openings() {
        let fee = serde_json::json!({ "amount": "100000", "asset": 1, "time": 1599,
            "r": "ecee001c9a2a03505fe2d1cb35b040f52ed6e3dbb5409a4ba716f00ac6811d63",
            "commitment": "47c3ad478fa92b55348204ac0d9884500466aba53d19875b782ada67d2ca6ee4" });
        let tx = LeafTx {
            kind: "bridge_attest",
            derived_cm: Some("d8f146fd390c839e5cc084a2c560c42f1901968864b834062e8aa28972ace5e2"),
            fee_cm: Some("47c3ad478fa92b55348204ac0d9884500466aba53d19875b782ada67d2ca6ee4"),
            fee_note: Some(&fee),
            amount: Some("100000000"),
            deposit_amount: Some("99900000"),
            asset_index: Some(1),
            note_time: Some(1599),
            deposit_r: Some("63a44e149332d851011f20ac2c2622e814297177c8b18f7447fe9a0484edb7a0"),
            ..Default::default()
        };
        let d = public_note_for(
            "d8f146fd390c839e5cc084a2c560c42f1901968864b834062e8aa28972ace5e2",
            &tx,
        )
        .unwrap();
        assert_eq!(
            (d.source.as_str(), d.amount.as_str(), d.asset, d.time),
            ("bridge_deposit", "99900000", 1, 1599)
        );
        let f = public_note_for(
            "47c3ad478fa92b55348204ac0d9884500466aba53d19875b782ada67d2ca6ee4",
            &tx,
        )
        .unwrap();
        assert_eq!(
            (f.source.as_str(), f.amount.as_str()),
            ("bridge_fee", "100000")
        );
        assert!(
            public_note_for(&"00".repeat(32), &tx).is_none(),
            "a bundle slot has no public opening"
        );
    }

    #[test]
    fn an_invoke_payout_is_found_by_its_commitment() {
        let t = serde_json::json!({ "reads": [], "writes": [], "inflow": "none",
            "pays": [{ "asset": 0, "amount": "300", "recipient": "rand1x", "time": 41, "r": "aa", "cm": "bb" }],
            "mints": [{ "asset": 2, "amount": "40", "recipient": "rand1x", "time": 41, "r": "cc", "cm": "dd" }] });
        let tx = LeafTx {
            kind: "invoke",
            transition: Some(&t),
            ..Default::default()
        };
        let p = public_note_for("dd", &tx).unwrap();
        assert_eq!(
            (p.source.as_str(), p.amount.as_str(), p.asset, p.r.as_str()),
            ("payout", "40", 2, "cc")
        );
    }
}
