//! A native (non-wasm) re-implementation of the fullnode's note-commitment hash
//! (`crates/randprotocol-zkvm/src/{hash,notes}.rs`), just enough of it to recompute the ONE
//! commitment the node does not publish directly for a `token_mint` or a `register_token` initial
//! mint (unlike a `bridge_attest`'s deposit, whose `commitment` field the node already serves in
//! `tx_json`). The indexer uses this to link that leaf — a real tree entry the node appends and
//! serves an envelope for via `rand_getCommitments`, same as any other note — back to the
//! transaction that created it.
//!
//! Byte-for-byte the same algorithm `crates/randscan-viewing` implements for the browser (that
//! crate is wasm32-only and excluded from this workspace, so this is a second, independent copy
//! rather than a shared dependency); [`tests::note_commitment_matches_the_shared_test_vector`]
//! checks both against the same fixture (`crates/randscan-viewing/tests/vectors.json`).

use p3_field::{PrimeCharacteristicRing, PrimeField64};
use p3_goldilocks::{Goldilocks, Poseidon2Goldilocks};
use p3_symmetric::{CryptographicHasher, PaddingFreeSponge};
use std::sync::OnceLock;

pub type Word8 = [u32; 8];
type Val = Goldilocks;
type Perm = Poseidon2Goldilocks<8>;

/// `domain::CM`: the note-commitment domain tag.
const DOMAIN_CM: u32 = 4;

fn perm() -> &'static Perm {
    static PERM: OnceLock<Perm> = OnceLock::new();
    // The committed table (audit ZKV-2), not a seeded RNG draw: `rand` does not promise its
    // standard generator is stable across releases, so a draw could silently move every hash.
    PERM.get_or_init(crate::poseidon2_constants::permutation)
}

fn split_digest(elems: [Val; 4]) -> Word8 {
    let mut out = [0u32; 8];
    for (i, e) in elems.iter().enumerate() {
        let v = e.as_canonical_u64();
        out[2 * i] = v as u32;
        out[2 * i + 1] = (v >> 32) as u32;
    }
    out
}

/// The `POSEIDON2` syscall's sponge: rate 4, capacity 4, overwrite mode, no padding.
fn sponge_hash(msg: &[u32]) -> Word8 {
    let sponge = PaddingFreeSponge::<_, 8, 4, 4>::new(perm().clone());
    let elems: Vec<Val> = msg.iter().copied().map(Val::from_u32).collect();
    split_digest(sponge.hash_iter(elems))
}

/// `H(domain, msg)`: the domain tag is the first absorbed word.
fn hash(domain: u32, msg: &[u32]) -> Word8 {
    let mut full = Vec::with_capacity(1 + msg.len());
    full.push(domain);
    full.extend_from_slice(msg);
    sponge_hash(&full)
}

pub fn word8_to_hex(w: &Word8) -> String {
    let bytes: Vec<u8> = w.iter().flat_map(|x| x.to_le_bytes()).collect();
    hex::encode(bytes)
}

pub fn word8_from_hex(s: &str) -> Option<Word8> {
    let b = hex::decode(s.trim().trim_start_matches("0x")).ok()?;
    word8_from_bytes(&b)
}

fn word8_from_bytes(b: &[u8]) -> Option<Word8> {
    if b.len() != 32 {
        return None;
    }
    let mut w = [0u32; 8];
    for (i, c) in b.chunks(4).enumerate() {
        w[i] = u32::from_le_bytes(c.try_into().unwrap());
    }
    Some(w)
}

/// What a note records: `pk(8) from(8) amount_lo amount_hi asset time r(8)`, 28 words — the
/// fullnode's `randprotocol_zkvm::notes::Note`.
pub struct Note {
    pub pk: Word8,
    pub from: Word8,
    pub amount: u64,
    pub asset: u32,
    pub time: u32,
    pub r: Word8,
}

impl Note {
    pub const WORDS: usize = 28;

    fn words(&self) -> [u32; Self::WORDS] {
        let mut w = [0u32; Self::WORDS];
        w[0..8].copy_from_slice(&self.pk);
        w[8..16].copy_from_slice(&self.from);
        w[16] = self.amount as u32;
        w[17] = (self.amount >> 32) as u32;
        w[18] = self.asset;
        w[19] = self.time;
        w[20..28].copy_from_slice(&self.r);
        w
    }

    pub fn commitment(&self) -> Word8 {
        hash(DOMAIN_CM, &self.words())
    }
}

/// `randprotocol_core::ledger::tokens::MINT_FROM`: the ASCII tag `rpl-mint`, little-endian in the
/// first two words and zero-padded — every RPL mint's note records this as `from`, since a mint
/// (unlike a transfer) has no sender.
pub const MINT_FROM: Word8 = [
    u32::from_le_bytes(*b"rpl-"),
    u32::from_le_bytes(*b"mint"),
    0,
    0,
    0,
    0,
    0,
    0,
];

/// The `pk` half of a `rand1…` shielded address: `rand1` + base58(pk(32 bytes) ‖ ML-KEM-768
/// encapsulation key). `None` if the string does not start with `rand1` or does not decode to at
/// least 32 bytes.
pub fn pk_from_address(address: &str) -> Option<Word8> {
    let body = address.strip_prefix("rand1")?;
    let raw = bs58::decode(body).into_vec().ok()?;
    if raw.len() < 32 {
        return None;
    }
    word8_from_bytes(&raw[..32])
}

/// `randprotocol_core::ledger::tokens::mint_commitment`: the commitment of the one note an RPL
/// mint creates (a registration's initial supply or a later `TokenMint`) — public because every
/// input is public on the wire (the recipient's address, the amount, the token's registry index,
/// the action's own `time`, and the blinding `r`), which is exactly what lets a recipient (or this
/// indexer) rebuild the leaf the chain appended without opening any envelope. `None` when
/// `recipient` does not parse as a shielded address.
#[allow(clippy::too_many_arguments)]
pub fn mint_commitment(
    recipient: &str,
    amount: u64,
    index: u32,
    time: u32,
    r: &Word8,
) -> Option<Word8> {
    let pk = pk_from_address(recipient)?;
    Some(
        Note {
            pk,
            from: MINT_FROM,
            amount,
            asset: index,
            time,
            r: *r,
        }
        .commitment(),
    )
}

/// Convenience: the hex commitment, or `None` on a bad address.
pub fn mint_commitment_hex(
    recipient: &str,
    amount: u64,
    index: u32,
    time: u32,
    r_hex: &str,
) -> Option<String> {
    let r = word8_from_hex(r_hex)?;
    mint_commitment(recipient, amount, index, time, &r).map(|cm| word8_to_hex(&cm))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shared fixture `crates/randscan-viewing/tests/vectors.json`'s `note`/`cm` fields,
    /// produced by the fullnode itself (constraint set 11, fullnode 60a7d03): `Note { pk: pk_b,
    /// from: pk_a, amount: 1500000000, asset: 0, time: 66, r: aa..22 }`. Cross-checks this crate's hash against the same
    /// vector `randscan-viewing`'s own tests open, so a drift in either copy is caught.
    #[test]
    fn note_commitment_matches_the_shared_test_vector() {
        let note = Note {
            pk: word8_from_hex("e4ed010b677dee856638ef991244116666da47248986444ddcbc86045472f64c")
                .unwrap(),
            from: word8_from_hex(
                "6a9958fe0337649400adab0f9ce3b95bb472353c0b0e103b9fd353a2678f68b1",
            )
            .unwrap(),
            amount: 1_500_000_000,
            asset: 0,
            time: 66,
            r: word8_from_hex("aa000000bb000000cc000000dd000000ee000000ff0000001100000022000000")
                .unwrap(),
        };
        assert_eq!(
            word8_to_hex(&note.commitment()),
            "f8514c636114a0bcf79f11f94c0ad0c7e104dbaea54c9ca36fa19c7be5fc4767"
        );
    }

    /// Audit ZKV-2 / scan R2: the round constants are the committed table, never a seeded RNG
    /// draw at runtime (`rand` does not promise its standard generator is stable across releases).
    #[test]
    fn notecommit_draws_no_constants_at_runtime() {
        let src = include_str!("notecommit.rs");
        for pattern in [
            concat!("new_from", "_rng"),
            concat!("Std", "Rng"),
            concat!("seed_from", "_u64"),
        ] {
            assert!(
                !src.contains(pattern),
                "notecommit.rs still derives constants: {pattern}"
            );
        }
    }

    /// The fullnode's and circuits' known-answer vectors for the width-8 permutation
    /// (`poseidon2_constants::tests::the_permutation_answers_its_known_vectors`, constraint set 11's
    /// Grain table at fullnode 60a7d03; the seeded draw of sets ≤ 10 answered `0x1e4b2c9e…` for
    /// zero), run through the permutation this crate hashes with.
    #[test]
    fn the_permutation_answers_the_fullnode_known_vectors() {
        use p3_symmetric::Permutation;
        let vectors: [([u64; 8], [u64; 8]); 3] = [
            (
                [0; 8],
                [
                    0x4411ec57c44145f5,
                    0x5ff55b96baa2f47b,
                    0xdee0e2ae35662802,
                    0x023c96b32c07981d,
                    0x777e4afeaaf2e6a1,
                    0x606c248e5ef427da,
                    0x862e82242b2c5001,
                    0x61ea532cc4c908c7,
                ],
            ),
            (
                [0, 1, 2, 3, 4, 5, 6, 7],
                [
                    0x020cf04a1b214d14,
                    0x84e14aaaeacaed25,
                    0x1ae0f640e81c7457,
                    0xa4d204cbaeb0d8a5,
                    0x0cf637b627b3a7ff,
                    0x788d304d948b486b,
                    0x7327133ea1949af4,
                    0xf415abb924da395b,
                ],
            ),
            (
                [0xffff_ffff_0000_0000; 8],
                [
                    0x5a986ad1feb0d7b1,
                    0x76379c3befced08d,
                    0xcebc0f4e8ae21d67,
                    0x8f9cc577b6f2cd12,
                    0xe0e58584b289c1dd,
                    0xf1400ffbb6875b78,
                    0x2014ea6e78f1bd58,
                    0xbaa54d6e51fd54f1,
                ],
            ),
        ];
        for (input, want) in vectors {
            assert_eq!(
                perm().permute(Val::new_array(input)),
                Val::new_array(want),
                "permute({input:x?})"
            );
        }
    }

    /// The openings of two alloc notes of the chain-15 genesis (fullnode `deploy/genesis-chain15.json`,
    /// `from` the zero word): a RAND note (asset 0) and the carried-over zUSD note (asset 1). The
    /// commitments are constraint set 11's, computed by the fullnode crate at 60a7d03 — under set 11
    /// that file no longer builds (its set-9 commitments were `be14787d…` and `d75053df…`).
    #[test]
    fn note_commitments_of_the_chain_15_openings_match_the_fullnode() {
        let cases = [
            (
                "47bd82b0401f3bc5b33e043fc7d076cca4d88f3233698ead959fa6009f284d36",
                1_000_000_000_000u64,
                0u32,
                "21764e9e43246765ae9464120b7bc134a80d62d680c613b649e6d0468fb632f2",
                "579c82febccbda9641690171c3672267e8cea24142e17d77cad8021e4299749b",
            ),
            (
                "f4a2c7c39a812c47e569cad909a5074076a8530785b1f7c10a4b6ffdcf3c7c96",
                1_000_000_000,
                1,
                "32f0d22777439fa85cd3fd04c3afa03295848c08b1eadd805844f3fa81752ab2",
                "5c0d78d1a2005d339c7098bfa148695eac8780bad6d416fc6f66a4d4ece585ae",
            ),
        ];
        for (pk, amount, asset, r, cm) in cases {
            let note = Note {
                pk: word8_from_hex(pk).unwrap(),
                from: [0; 8],
                amount,
                asset,
                time: 0,
                r: word8_from_hex(r).unwrap(),
            };
            assert_eq!(word8_to_hex(&note.commitment()), cm);
        }
    }

    /// The 86 round constants are constraint set 11's Grain table (fullnode
    /// `crates/randprotocol-zkvm/src/poseidon2_constants.rs` at 60a7d03): SHA-256 over INITIAL (4×8), INTERNAL (22), TERMINAL (4×8), each value
    /// little-endian, computed from that file, not from this crate's copy.
    #[test]
    fn the_constants_are_the_circuits_table() {
        use crate::poseidon2_constants::{INITIAL, INTERNAL, TERMINAL};
        use sha2::{Digest, Sha256};
        let values: Vec<u64> = INITIAL
            .iter()
            .flatten()
            .chain(INTERNAL.iter())
            .chain(TERMINAL.iter().flatten())
            .copied()
            .collect();
        assert_eq!(values.len(), 86);
        let mut h = Sha256::new();
        for v in values {
            h.update(v.to_le_bytes());
        }
        assert_eq!(
            hex::encode(h.finalize()),
            "a0289c4f0bccd6f759f818fe2e9e245aa1617a9a248cf3460496e2a18a002f82"
        );
    }

    #[test]
    fn mint_from_is_the_ascii_tag_rpl_dash_mint() {
        assert_eq!(
            word8_to_hex(&MINT_FROM),
            hex::encode(b"rpl-mint\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0")
        );
        assert_ne!(MINT_FROM, [0u32; 8]);
    }

    #[test]
    fn pk_from_address_reads_the_first_32_bytes() {
        // "rand1" + base58 of 40 bytes of 0x01 followed by 0x02 (a fake but well-formed address).
        let mut raw = vec![1u8; 32];
        raw.extend_from_slice(&[2u8; 8]);
        let addr = format!("rand1{}", bs58::encode(&raw).into_string());
        let pk = pk_from_address(&addr).unwrap();
        assert_eq!(word8_to_hex(&pk), hex::encode([1u8; 32]));
        assert!(pk_from_address("notrand1xyz").is_none());
        assert!(pk_from_address("rand1").is_none());
    }

    #[test]
    fn mint_commitment_hex_round_trips_through_a_real_address() {
        let mut raw = vec![1u8; 32];
        raw.extend_from_slice(&[2u8; 8]);
        let addr = format!("rand1{}", bs58::encode(&raw).into_string());
        let r = "00".repeat(32);
        let cm1 = mint_commitment_hex(&addr, 700, 3, 41, &r).unwrap();
        let cm2 = mint_commitment_hex(&addr, 700, 3, 41, &r).unwrap();
        assert_eq!(cm1, cm2, "deterministic");
        assert_eq!(cm1.len(), 64);
        // A different amount, index or time all move the commitment.
        assert_ne!(cm1, mint_commitment_hex(&addr, 701, 3, 41, &r).unwrap());
        assert_ne!(cm1, mint_commitment_hex(&addr, 700, 4, 41, &r).unwrap());
        assert_ne!(cm1, mint_commitment_hex(&addr, 700, 3, 42, &r).unwrap());
        assert!(mint_commitment_hex("not-an-address", 700, 3, 41, &r).is_none());
    }
}
