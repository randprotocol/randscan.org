//! Chain-computed notes rebuilt from their public opening (fullnode v0.6.8's wallet rebuild,
//! `wallet::rebuilt_notes_with`), checked against chain 20's first fee-split deposit: block 1664's
//! `bridge_attest` minted the depositor's net note (leaf 19, 0.999 zUSD) and the treasury's fee
//! note (leaf 20, 0.001 zUSD, no envelope). The fixture is public chain data only; the owners'
//! `pk`s are read out of their public addresses (`rand1` + base58(pk ‖ ML-KEM key)). Since
//! constraint set 11 (chain 22) the leaves are those same openings re-committed under the Grain
//! constants by the fullnode crate (60a7d03), not chain 20's set-9 leaves — see the fixture's
//! `_source`.

use randscan_viewing::{rebuild, word8_from_hex, Note, PublicNote, ViewingKey, Word8, DEPOSIT_FROM, MINT_FROM, PROGRAM_FROM};

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/chain20_block1664_attest.json")).unwrap()
}

fn pk_of(address: &str) -> Word8 {
    let raw = bs58::decode(address.strip_prefix("rand1").unwrap()).into_vec().unwrap();
    let mut w = [0u32; 8];
    for (i, c) in raw[..32].chunks(4).enumerate() {
        w[i] = u32::from_le_bytes(c.try_into().unwrap());
    }
    w
}

fn hexw(s: &serde_json::Value) -> Word8 {
    word8_from_hex(s.as_str().unwrap()).unwrap()
}

#[test]
fn block_1664s_net_deposit_and_fee_note_hash_to_leaves_19_and_20() {
    let f = fixture();
    let a = &f["action"];
    assert_eq!(a["kind"], "bridge_attest");
    assert_eq!((a["amount"].as_str(), a["deposit_amount"].as_str()), (Some("100000000"), Some("99900000")));
    let leaf = |i: usize| hexw(&f["leaves"][i]["cm"]);

    let deposit = Note {
        pk: pk_of(a["recipient"].as_str().unwrap()),
        from: DEPOSIT_FROM,
        amount: 99_900_000,
        asset: 1,
        time: 1599,
        r: hexw(&a["r"]),
    };
    assert_eq!(deposit.commitment(), leaf(0), "leaf 19 is the depositor's net note");
    assert_eq!(deposit.commitment(), hexw(&a["commitment"]));

    let fee = &a["fee_note"];
    let fee_note = Note {
        pk: pk_of(f["fees"]["recipient"].as_str().unwrap()),
        from: DEPOSIT_FROM,
        amount: 100_000,
        asset: 1,
        time: 1599,
        r: hexw(&fee["r"]),
    };
    assert_eq!(fee_note.commitment(), leaf(1), "leaf 20 is the treasury's fee note");
    assert_eq!(fee_note.commitment(), hexw(&fee["commitment"]));
    assert_eq!(f["leaves"][1]["has_envelope"], false, "a fee note is sealed to nobody");

    // The gross would not have matched: the deposit note carries the net.
    assert_ne!(Note { amount: 100_000_000, ..deposit }.commitment(), leaf(0));
}

#[test]
fn a_rebuild_is_accepted_only_for_the_keys_own_note_at_that_leaf() {
    let vk = ViewingKey { nk: [7, 1, 2, 3, 4, 5, 6, 9] };
    let other = ViewingKey { nk: [8, 1, 2, 3, 4, 5, 6, 9] };
    let r = "ab".repeat(32);
    for (source, from) in [("bridge_deposit", DEPOSIT_FROM), ("bridge_fee", DEPOSIT_FROM), ("token_mint", MINT_FROM), ("initial_mint", MINT_FROM), ("payout", PROGRAM_FROM)] {
        let cm = Note { pk: vk.pk(), from, amount: 5_000, asset: 1, time: 40, r: word8_from_hex(&r).unwrap() }.commitment();
        let p: PublicNote = serde_json::from_value(serde_json::json!({ "source": source, "amount": "5000", "asset": 1, "time": 40, "r": r })).unwrap();
        let n = rebuild(&cm, &p, &vk).unwrap_or_else(|| panic!("{source}"));
        assert_eq!((n.amount, n.from), (5_000, from));
        assert!(rebuild(&cm, &p, &other).is_none(), "{source}: another key's pk does not commit to the leaf");
        let lie: PublicNote = serde_json::from_value(serde_json::json!({ "source": source, "amount": "5001", "asset": 1, "time": 40, "r": r })).unwrap();
        assert!(rebuild(&cm, &lie, &vk).is_none(), "{source}: a served amount that is not the leaf's credits nothing");
    }
    let unknown: PublicNote = serde_json::from_value(serde_json::json!({ "source": "mint", "amount": 1, "asset": 0, "time": 0, "r": r })).unwrap();
    assert!(rebuild(&[0; 8], &unknown, &vk).is_none());
}

/// With the depositor's own key (`RAND_VIEWING_TEST_KEY_FILE` = shielded-5's wallet key file,
/// kept off the repository), the full path: viewing key -> pk -> rebuilt note at leaf 19.
#[test]
fn the_depositors_key_rebuilds_leaf_19_when_given() {
    let Ok(path) = std::env::var("RAND_VIEWING_TEST_KEY_FILE") else { return };
    let kf: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let vk = ViewingKey::from_spend_key(&word8_from_hex(kf["spend_key"].as_str().unwrap()).unwrap());
    let f = fixture();
    let a = &f["action"];
    let p: PublicNote = serde_json::from_value(serde_json::json!({ "source": "bridge_deposit", "amount": a["deposit_amount"], "asset": 1, "time": 1599, "r": a["r"] })).unwrap();
    let n = rebuild(&hexw(&f["leaves"][0]["cm"]), &p, &vk).expect("the depositor's key rebuilds its deposit");
    assert_eq!(n.amount, 99_900_000);
}
