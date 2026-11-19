//! Opens envelopes sealed by the fullnode's own crate (`tests/vectors.json`, produced with
//! `randprotocol-zkvm` at fullnode `feat/cs11-revendor` 60a7d03 — constraint set 11, chain 22,
//! from the same keys, note, salt and inputs as before; set 11's Grain constants moved every
//! Poseidon2 value in it — the sponge, nk/pk/ovk, the address, the commitment, the nullifier and
//! H_IN. Through set 10 it was produced at d40fb94 (v0.6.6) and first at e2aae20): the only thing
//! that pins this re-implementation to the wire format the chain uses.

use randscan_viewing::*;
use serde_json::Value;

fn vectors() -> Value {
    serde_json::from_str(include_str!("vectors.json")).unwrap()
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap()
}

#[test]
fn the_sponge_and_key_derivations_match_the_node() {
    let v = vectors();
    assert_eq!(word8_to_hex(&sponge_hash(&[])), s(&v, "sponge_empty"));
    assert_eq!(word8_to_hex(&sponge_hash(&[1, 2, 3])), s(&v, "sponge_1_2_3"));
    let vk_a = ViewingKey::from_spend_key(&word8_from_hex(s(&v, "sk_a")).unwrap());
    assert_eq!(word8_to_hex(&vk_a.nk), s(&v, "nk_a"));
    assert_eq!(word8_to_hex(&vk_a.pk()), s(&v, "pk_a"));
    assert_eq!(hex::encode(vk_a.ovk()), s(&v, "ovk_a"));
    let vk_b = ViewingKey { nk: word8_from_hex(s(&v, "nk_b")).unwrap() };
    assert_eq!(word8_to_hex(&vk_b.pk()), s(&v, "pk_b"));
    assert_eq!(vk_b.address(), s(&v, "address_b"), "ML-KEM key from the seed, base58 address");
    let cm = word8_from_hex(s(&v, "cm")).unwrap();
    assert_eq!(word8_to_hex(&vk_b.nullifier(&cm)), s(&v, "nullifier_b"));
}

#[test]
fn the_receiver_the_sender_and_the_tx_key_open_the_envelope() {
    let v = vectors();
    let e: EnvelopeHex = serde_json::from_value(v["envelope"].clone()).unwrap();
    let env = Envelope::from_hex(&e).unwrap();
    let cm = word8_from_hex(s(&v, "cm")).unwrap();
    let vk_a = ViewingKey { nk: word8_from_hex(s(&v, "nk_a")).unwrap() };
    let vk_b = ViewingKey { nk: word8_from_hex(s(&v, "nk_b")).unwrap() };
    let tx_key: [u8; 32] = hex::decode(s(&v, "tx_key")).unwrap().try_into().unwrap();

    let (k, note) = env.open_as_receiver(&cm, &vk_b).expect("receiver opens");
    assert_eq!(k, tx_key);
    assert_eq!(note.amount, v["note"]["amount"].as_u64().unwrap());
    assert_eq!(note.time, 66);
    assert_eq!(word8_to_hex(&note.pk), s(&v["note"], "pk"));
    assert_eq!(word8_to_hex(&note.from), s(&v["note"], "from"));
    assert_eq!(word8_to_hex(&note.r), s(&v["note"], "r"));
    assert_eq!(note.commitment(), cm);

    let (k2, note2) = env.open_as_sender(&cm, &vk_a).expect("sender opens");
    assert_eq!((k2, note2), (tx_key, note));
    assert_eq!(env.open_with_tx_key(&cm, &tx_key), Some(note));

    // The wrong party, a wrong key and a wrong commitment all fail closed.
    assert!(env.open_as_receiver(&cm, &vk_a).is_none());
    assert!(env.open_as_sender(&cm, &vk_b).is_none());
    assert!(env.open_with_tx_key(&cm, &[0u8; 32]).is_none());
    let mut other = cm;
    other[0] ^= 1;
    assert!(env.open_with_tx_key(&other, &tx_key).is_none());
}

#[test]
fn the_call_transcript_opens_for_the_caller_the_auditor_and_the_call_key() {
    let v = vectors();
    let e: CallEnvelopeHex = serde_json::from_value(v["call_envelope"].clone()).unwrap();
    let h_in = word8_from_hex(s(&v, "h_in")).unwrap();
    let salt: [u32; 4] = serde_json::from_value(v["salt"].clone()).unwrap();
    let inputs: Vec<u32> = serde_json::from_value(v["inputs"].clone()).unwrap();
    assert_eq!(input_digest(salt, &inputs), h_in, "H_IN recomputes");
    let call_key: [u8; 32] = hex::decode(s(&v, "call_key")).unwrap().try_into().unwrap();
    assert_eq!(open_call_with_key(&e, &h_in, &call_key), Some((salt, inputs.clone())));
    let vk_a = ViewingKey { nk: word8_from_hex(s(&v, "nk_a")).unwrap() };
    let vk_b = ViewingKey { nk: word8_from_hex(s(&v, "nk_b")).unwrap() };
    assert_eq!(open_call_as_sender(&e, &h_in, &vk_a).map(|(k, ..)| k), Some(call_key));
    assert_eq!(open_call_as_auditor(&e, &h_in, &vk_b).map(|(k, ..)| k), Some(call_key));
    assert!(open_call_as_sender(&e, &h_in, &vk_b).is_none());
}

#[test]
fn the_wasm_surface_reports_roles_and_verification() {
    let v = vectors();
    let env = v["envelope"].to_string();
    let cm = s(&v, "cm");
    let opened: Value = serde_json::from_str(&open_note(cm, &env, "viewing", s(&v, "nk_b")).unwrap()).unwrap();
    assert_eq!(opened["role"], "received");
    assert_eq!(opened["amount"], "1500000000");
    assert_eq!(opened["nullifier"], s(&v, "nullifier_b"));
    assert_eq!(opened["verified"], true);
    let opened: Value = serde_json::from_str(&open_note(cm, &env, "spend", s(&v, "sk_a")).unwrap()).unwrap();
    assert_eq!(opened["role"], "sent");
    assert!(opened["nullifier"].is_null());
    let file = format!("{{\"version\":2,\"spend_key\":\"{}\"}}", s(&v, "sk_b"));
    let opened: Value = serde_json::from_str(&open_note(cm, &env, "file", &file).unwrap()).unwrap();
    assert_eq!(opened["role"], "received");
    let opened: Value = serde_json::from_str(&open_note(cm, &env, "tx", s(&v, "tx_key")).unwrap()).unwrap();
    assert_eq!(opened["role"], "tx_key");
    assert_eq!(open_note(cm, &env, "tx", &"00".repeat(32)).unwrap(), "null");
    assert!(open_note(cm, &env, "viewing", "zz").is_err());

    let info: Value = serde_json::from_str(&key_info(&file, "file").unwrap()).unwrap();
    assert_eq!(info["nk"], s(&v, "nk_b"));
    assert_eq!(info["address"], s(&v, "address_b"));
    assert_eq!(info["from_spend_key"], true);

    let ce = v["call_envelope"].to_string();
    let c: Value = serde_json::from_str(&open_call(s(&v, "h_in"), &ce, "viewing", s(&v, "nk_a")).unwrap()).unwrap();
    assert_eq!(c["role"], "caller");
    assert_eq!(c["faithful"], true);
    assert_eq!(c["inputs"], v["inputs"]);
    let c: Value = serde_json::from_str(&open_call(s(&v, "h_in"), &ce, "viewing", s(&v, "nk_b")).unwrap()).unwrap();
    assert_eq!(c["role"], "auditor");
    assert_eq!(nullifier_of("viewing", s(&v, "nk_b"), cm).unwrap(), s(&v, "nullifier_b"));
}
