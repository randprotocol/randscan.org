//! Audit v14: an envelope that opens is not a row the key owns or made.
//!
//! - SCAN-841: anyone can seal a note it owns itself to a published address. The KEM part then
//!   opens under the address holder's key, but the note's `pk` is the sealer's: it is not
//!   received, has no nullifier of this key's that will ever reach the chain, and must never
//!   count toward a balance. The test needs nothing of the victim's but its public address.
//! - VK-751: whoever holds a wallet's viewing key holds its `ovk`, and can wrap the transaction
//!   key of a payment of its own under it. The sender part then opens under the victim's key, but
//!   the note's `from` is the planter's `pk`: it is not sent by this key.
//!
//! The crate has no sealing side, so the envelopes are sealed here, the way the fullnode's
//! `viewing::Envelope::seal` does (ML-KEM-768 to the address's key, the transaction key wrapped
//! under the shared secret and under the sender's `ovk`, the note under the transaction key,
//! each ChaCha20-Poly1305 with a 12-byte nonce prepended and `tag ‖ cm` as associated data).

use randscan_viewing::*;
use serde_json::Value;

fn vectors() -> Value {
    serde_json::from_str(include_str!("vectors.json")).unwrap()
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap()
}

fn aead_seal(key: &[u8; 32], tag: &[u8], cm: &Word8, pt: &[u8], nonce: u8) -> String {
    use chacha20poly1305::aead::{Aead, KeyInit, Payload};
    use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
    let nonce = [nonce; 12];
    let aad = [tag, &words_to_bytes(cm)].concat();
    let ct = ChaCha20Poly1305::new(&Key::from(*key)).encrypt(&Nonce::from(nonce), Payload { msg: pt, aad: &aad }).unwrap();
    hex::encode([&nonce[..], &ct].concat())
}

/// Seal `note` to the **public** `address` (its ML-KEM key is the bytes after the 32-byte pk),
/// with the transaction key wrapped under `ovk` for the sender. Returns the leaf and its envelope.
fn seal(ovk: &[u8; 32], address: &str, note: &Note) -> (String, String) {
    use ml_kem::kem::Encapsulate;
    type Ek = ml_kem::ml_kem_768::EncapsulationKey;
    let raw = bs58::decode(address.strip_prefix("rand1").unwrap()).into_vec().unwrap();
    let ek = Ek::new(&ml_kem::kem::Key::<Ek>::try_from(&raw[32..]).expect("1184-byte encapsulation key")).expect("valid key");
    let (kem_ct, ss) = ek.encapsulate_with_rng(&mut rand::rng());
    let ss: [u8; 32] = ss.into();
    let tx_key = [0x5a_u8; 32];
    let cm = note.commitment();
    let env = serde_json::json!({
        "kem_ct": hex::encode(&kem_ct[..]),
        "to_receiver": aead_seal(&ss, b"rand-envelope-receiver", &cm, &tx_key, 1),
        "to_sender": aead_seal(ovk, b"rand-envelope-sender", &cm, &tx_key, 2),
        "body": aead_seal(&tx_key, b"rand-envelope-body", &cm, &words_to_bytes(&note.words()), 3),
    });
    (word8_to_hex(&cm), env.to_string())
}

fn open(cm: &str, env: &str, nk: &str) -> Value {
    serde_json::from_str(&open_note(cm, env, "viewing", nk).unwrap()).unwrap()
}

fn key(seed: u32) -> ViewingKey {
    ViewingKey::from_spend_key(&[seed; 8])
}

const AMOUNT: u64 = 1_000_000 * 100_000_000;

#[test]
fn a_note_sealed_to_a_published_address_but_owned_by_its_sender_is_not_received() {
    // SCAN-841. Everything the attacker uses of the victim's is its published address.
    let v = vectors();
    let victim_address = s(&v, "address_b");
    let attacker = key(41);
    let phantom = Note { pk: attacker.pk(), from: attacker.pk(), amount: AMOUNT, asset: 0, time: 9, r: [3; 8] };
    let (cm, env) = seal(&attacker.ovk(), victim_address, &phantom);

    // The envelope really does open under the victim's key: that is the whole trap.
    let victim = ViewingKey { nk: word8_from_hex(s(&v, "nk_b")).unwrap() };
    let e = Envelope::from_hex(&serde_json::from_str(&env).unwrap()).unwrap();
    assert_eq!(e.open_as_receiver(&word8_from_hex(&cm).unwrap(), &victim).map(|(_, n)| n), Some(phantom));

    // But the note is not the victim's: no received row, no nullifier, nothing to add to a balance.
    let opened = open(&cm, &env, s(&v, "nk_b"));
    assert!(opened.is_null(), "a note owned by its sender was reported to the address holder: {opened}");
}

#[test]
fn a_note_owned_by_the_key_is_still_received_with_its_nullifier() {
    let v = vectors();
    let victim = ViewingKey { nk: word8_from_hex(s(&v, "nk_b")).unwrap() };
    let payer = key(42);
    let note = Note { pk: victim.pk(), from: payer.pk(), amount: 7, asset: 0, time: 9, r: [4; 8] };
    let (cm, env) = seal(&payer.ovk(), s(&v, "address_b"), &note);
    let opened = open(&cm, &env, s(&v, "nk_b"));
    assert_eq!(opened["role"], "received");
    assert_eq!(opened["pk"], word8_to_hex(&victim.pk()));
    assert_eq!(opened["nullifier"], word8_to_hex(&victim.nullifier(&note.commitment())));
    // And for the payer, whose `pk` is the note's creator, it is sent.
    let opened = open(&cm, &env, &word8_to_hex(&payer.nk));
    assert_eq!(opened["role"], "sent");
    assert!(opened["nullifier"].is_null());
}

#[test]
fn a_payment_planted_under_the_keys_ovk_by_another_creator_is_not_sent() {
    // VK-751. The planter holds the victim's viewing key (a prover it delegated to, an auditor),
    // hence its ovk, and wraps the key of its own payment to `payee` under it.
    let v = vectors();
    let victim = ViewingKey { nk: word8_from_hex(s(&v, "nk_b")).unwrap() };
    let planter = key(43);
    let payee = key(44);
    let planted = Note { pk: payee.pk(), from: planter.pk(), amount: AMOUNT, asset: 0, time: 9, r: [5; 8] };
    let (cm, env) = seal(&victim.ovk(), &payee.address(), &planted);

    let e = Envelope::from_hex(&serde_json::from_str(&env).unwrap()).unwrap();
    assert_eq!(e.open_as_sender(&word8_from_hex(&cm).unwrap(), &victim).map(|(_, n)| n), Some(planted), "the sender part opens");

    let opened = open(&cm, &env, s(&v, "nk_b"));
    assert!(opened.is_null(), "a payment created by another key was reported as this key's: {opened}");
    // The payee still receives it.
    assert_eq!(open(&cm, &env, &word8_to_hex(&payee.nk))["role"], "received");
}

#[test]
fn a_chain_computed_note_sealed_under_the_keys_ovk_is_shown_but_never_as_sent() {
    // A relayed bridge deposit, an RPL mint, an RPL-2 payout and a perp withdrawal are notes
    // the chain computes with a fixed `from` word; the submitter seals the envelope under its
    // own ovk. The creator field cannot say who that was, and anyone holding the ovk can seal
    // one, so the row is "sent_unverified": listed, never Sent.
    let v = vectors();
    let relayer = ViewingKey { nk: word8_from_hex(s(&v, "nk_b")).unwrap() };
    let recipient = key(45);
    for (i, from) in [DEPOSIT_FROM, MINT_FROM, PROGRAM_FROM, PERP_FROM].into_iter().enumerate() {
        let note = Note { pk: recipient.pk(), from, amount: 5, asset: 1, time: 9, r: [10 + i as u32; 8] };
        let (cm, env) = seal(&relayer.ovk(), &recipient.address(), &note);
        let opened = open(&cm, &env, s(&v, "nk_b"));
        assert_eq!(opened["role"], "sent_unverified", "from word {i}");
        assert!(opened["nullifier"].is_null());
        assert_eq!(open(&cm, &env, &word8_to_hex(&recipient.nk))["role"], "received", "from word {i}");
    }
}

#[test]
fn a_note_the_key_created_stays_sent_even_when_its_envelope_was_sealed_to_the_keys_own_address() {
    // The receiver part opening for a note owned by someone else must not hide the sender path:
    // the two openings are independent (the wallet's `classify` falls through the same way).
    let v = vectors();
    let me = ViewingKey { nk: word8_from_hex(s(&v, "nk_b")).unwrap() };
    let payee = key(46);
    let note = Note { pk: payee.pk(), from: me.pk(), amount: 5, asset: 0, time: 9, r: [6; 8] };
    let (cm, env) = seal(&me.ovk(), s(&v, "address_b"), &note);
    let opened = open(&cm, &env, s(&v, "nk_b"));
    assert_eq!(opened["role"], "sent");
    assert!(opened["nullifier"].is_null());
}
