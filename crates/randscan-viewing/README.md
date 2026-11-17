# randscan-viewing

The open side of the shielded chain's disclosure layer, compiled to WebAssembly so a viewing key
or a per-transaction key pasted into randscan.org is used in the visitor's browser and never
sent anywhere.

It re-implements, byte for byte, what the fullnode's vendored zkVM does when a wallet opens an
envelope (`crates/randprotocol-zkvm/src/{hash,notes,viewing,call_envelope}.rs`, last checked at
commit `d40fb94` = v0.6.6, the chain-18 / constraint-set-8 build; the crate's whole `src/` tree
and its `Cargo.toml` are the same git objects at `86941a1` = v0.6.7, the release chain 18 runs
and chain 19 is cut and run with, so chain 19 needs no rebuild):
the Poseidon2 sponge with the node's seeded round constants, `nk -> pk / ovk / ML-KEM seed`,
the note layout and commitment, ChaCha20-Poly1305 with the commitment as associated data, and
the three openings (receiver through ML-KEM-768, sender through `ovk`, or the transaction key),
plus a call's sealed input transcript checked against the receipt's `H_IN`, and (fullnode
v0.6.8, `wallet::rebuilt_notes_with`) `rebuild_note`: a chain-computed note — a bridge deposit, a
bridge fee note (no envelope), an RPL mint, an RPL-2 payout — rebuilt from its public opening
(randscan's `public`) with the key's own `pk` and accepted only when it commits to the leaf
(`tests/rebuild.rs`, chain 20 block 1664's leaves 19 and 20). An opening is
not ownership or authorship (audit v14, SCAN-841 / VK-751): `classify` — the wallet's and the
node's `classify` restated — reports a note as received only when its owner `pk` is the key's
own, as sent only when its creator `from` is, as `sent_unverified` when the sender part opens
and `from` is a word the chain fixes (a relayed deposit, a mint, a payout), and otherwise not at
all (`tests/ownership.rs`, which seals its own envelopes). No sealing, no
randomness, no ledger. `tests/vectors.json` was produced by the fullnode crate itself; the
tests open it.

Built outside the workspace because the Plonky3 crates need Rust 1.98 (`rust-toolchain.toml`):

```bash
cd crates/randscan-viewing
cargo test
wasm-pack build --release --target web --out-dir ../../frontend/public/viewing --out-name randscan_viewing
```

The artifacts under `frontend/public/viewing/` are committed; the frontend loads them at runtime
(`src/lib/viewing.ts`). Re-run the build (and refresh the vectors) whenever the fullnode changes
the envelope format, the note layout or the hash.
