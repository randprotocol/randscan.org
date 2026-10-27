# RandScan API for early users

RandScan indexes the Rand Protocol RAND chain and serves what it has indexed over a plain HTTPS
JSON API and a WebSocket feed. This guide is for people who want to read the chain from a script,
a bot, an exchange backend, or a wallet without running their own `rand-node`.

Base URL: `https://randscan.org/api/v1`
WebSocket: `wss://randscan.org/ws`

Public read endpoints need no key. Anonymous traffic is limited to 60 requests per minute per IP.
For more, create a free account at https://randscan.org/signup and an API key at
https://randscan.org/dashboard: keyed requests get 600 requests per minute. Use the WebSocket feed
instead of tight polling when you need to react to new blocks.

## What is public on a shielded chain

RAND is a fully shielded chain (fullnode `docs/shielded.md`). Every balance is a set of notes
in a commitment tree, and every transaction is a **hidden-asset bundle** (chain 14: four spent
notes and four created notes, dummies included, a public fee and a STARK proof) plus an optional
public **action**. There are **no accounts, no balances, no senders and no recipients** anywhere
in this API, and — since chain 14 — **no public asset on a bundle**: slots 1–2 carry a private
asset (RAND or any RPL token) and slots 3–4 always RAND, so a transfer of RAND and a transfer of
any RPL token are the same shape, byte for byte. What the explorer can show is:

- every block, and every transaction's bundle *as published*: the anchor, four nullifiers, four
  commitments, the fee, what (if anything) was burned, the target height, and the proof and
  envelope sizes;
- the action a transaction carried and its public fields: a deploy's program, a call's program
  and receipt, a validator's bond / unbond / withdraw amounts, a faucet mint's amount, a bridge
  deposit's amount, a bridge burn's destination, and an RPL token's registration / mint / burn
  (spec §4 — a token's registration and its mints are public by design, the way a bridge deposit
  is) — these are the only amounts ever public, and none of them is a *transfer*;
- the RPL token registry (`GET /tokens`, `GET /tokens/:id`): every registered token's name,
  symbol, decimals, authority and total supply, with a bridged token's per-backing locked amount;
- the commitment tree leaf by leaf, the nullifier set, the validator register (stake, rewards,
  unbonding queue), the supply audit, and the bridge's public state (including its mint pause and
  PQ guardian set, bridge hardening B1/B3/B4).

To see a balance, a transfer's parties, or **which asset a transfer moved**, you need a viewing
key or a transaction key and a wallet (or this site's own key-paste tools); the explorer holds no
key and decrypts nothing server-side. `rand_checkTransaction`-style disclosure (`docs/rpc.md` §4)
is the *only* place an asset index is ever revealed for a transfer — never the public page.

## Conventions

- **Amounts are strings of units.** 1 RAND = 1,000,000,000 units (9 decimals). Amounts are
  unsigned 128-bit integers and are always serialised as decimal strings, never as JSON numbers.
  `"10000000000"` is 10 RAND. An amount naming a token registry index other than 0
  (`asset_index`) — a bridge deposit/burn or an RPL `token_mint`/`token_burn`/`register_token` —
  is in **that token's own smallest unit** (its registered `decimals`, from `GET /tokens/:id`),
  not RAND's 9; resolve the index before dividing. `registration_fee` and `next_index` are the
  two exceptions served as plain JSON numbers, not decimal strings (they are small, chain-wide
  constants, not values that grow with usage).
- **Timestamps** are `timestamp_ms`: Unix milliseconds, set by the block proposer.
- **Hashes, program ids, commitments, nullifiers and anchors** are 64 lowercase hex characters,
  no `0x` prefix. Inputs accept an optional `0x` and uppercase.
- **Validator addresses** are base58 strings of 32 to 44 characters (for example
  `2nRdFChBXRmKoe2sQE3ZYDzvdg53QmBZJJ9iweY7hk1v`). **Shielded addresses** (`rand1…`, about
  1,700 characters) appear only as a bridge deposit's recipient and a validator's payout address.
- **Heights** are decimal integers starting at 0. Only committed blocks are indexed; there is no
  finality window to wait for. A block that appears in the API is final.
- **Pagination.** List endpoints take `page` (from 1) and `limit` (1 to 100, default 20) and return
  `{ "data": [...], "pagination": { page, limit, total, total_pages, has_next, has_prev } }`.
  Lists are newest first.
- **Errors** are JSON with an HTTP status:

  ```json
  { "error": "not_found", "message": "block not found", "code": "NOT_FOUND" }
  ```

  | status | `error` | when |
  |---|---|---|
  | 400 | `bad_request` | malformed hash, unknown `kind`, and so on |
  | 404 | `not_found` | the block, transaction, note, nullifier, validator or program does not exist |
  | 410 | `no_accounts` | `/accounts/...`: this chain has no accounts |
  | 500 | `internal_error` | something failed on our side; retry with backoff |

- CORS is open, so browsers can call the API directly. Responses are gzip compressed when the
  client accepts it.

## Quick start

```bash
# Is the explorer healthy and caught up with the chain?
curl -s https://randscan.org/api/v1/health

# Current chain height, tree size, validator count, supply audit
curl -s https://randscan.org/api/v1/stats

# A transaction by hash
curl -s https://randscan.org/api/v1/transactions/ba03893d4b5b4f03ab8ab85aa9e8df4b46102986f15ef56653f7450b98bdba34

# Which transaction spent a note (by nullifier), or created one (by commitment)
curl -s https://randscan.org/api/v1/nullifiers/8c04…d1
curl -s https://randscan.org/api/v1/notes/2a9f…07
```

Python:

```python
import requests

API = "https://randscan.org/api/v1"
UNITS = 10**9

page = requests.get(f"{API}/transactions", params={"kind": "mint", "limit": 50}, timeout=10).json()
for tx in page["data"]:
    # A mint is one of the few actions with a public amount; a transfer has none.
    print(tx["height"], tx["hash"][:12], int(tx["amount"]) / UNITS, "RAND minted")

validators = requests.get(f"{API}/validators", timeout=10).json()
for v in validators:
    print(v["address"], int(v["stake"]) / UNITS, "RAND staked", "active" if v["active"] else "inactive")
```

## Endpoints

### Health and network

| method and path | returns |
|---|---|
| `GET /health` | explorer status and indexer lag |
| `GET /stats` | network statistics |
| `GET /supply` | the node's supply audit (404 on a node that does not serve it) |
| `GET /perps` | the perps exchange's public state: collateral asset, markets, proved root and height, oracle medians (404 on a chain without a `perps` section) |
| `GET /price` | the validator set's voted RAND/USD price (404 on a chain without `fees.usd_subsidy`) |
| `GET /multisig/:id` | one multisig account read live from the node: signers in order, threshold, nonce, vault (404 for an unknown id or a chain without a `multisig` section; 400 for an id that is not 64 hex) |
| `GET /bridge` | the bridge's public state and asset registry |
| `GET /bridge/assets` | every backing of every bridged token with its burns and what it holds, and the token's deposits |
| `GET /bridge/tokens` | the tokens the bridge accepts, with their contract addresses on each chain |
| `GET /tokens` | the whole RPL token registry, as cached from the node |
| `GET /tokens/:id` | one token by registry index, 64-hex id or `rpl1…` text form, its deploy transaction and its public supply history |
| `GET /nodes` | the explorer node and its peers, with geolocation |
| `GET /provers` | the delegated provers the explorer knows of: whether each answers, its capacity and fee, and where its hosts are |

`GET /health`:

```json
{
  "status": "healthy",
  "version": "0.3.0",
  "database": true,
  "indexer": { "connected": true, "synced": true, "current_height": 20147, "node_height": 20147, "lag": 0 }
}
```

`status` is `healthy` when the database is up and the indexer is within 2 blocks of the node,
`degraded` when the indexer is behind or disconnected, and `unhealthy` when the database is
unreachable. Check `indexer.lag` before trusting "latest" data during an outage.

`GET /stats`:

```json
{
  "chain_id": 7, "symbol": "RAND", "decimals": 9,
  "height": 20147, "view": 23263,
  "total_transactions": 40, "notes": 91, "nullifiers": 62,
  "validator_count": 5, "active_validator_count": 4, "total_stake": "400000000000000",
  "total_supply": "1400100000000000", "pool_value": "1000097000000000",
  "program_count": 1, "avg_block_time_ms": 1004.35,
  "peer_count": 4, "mempool_size": 0, "node_syncing": false,
  "faucet": true, "confidential": true,
  "current_leader": "F6rYLexPhyMmwPNqbEmyyp5FiTmtQqDgZyqScUqYY4F6",
  "tree_root": "6b1d…c4", "hc_bundle": "f07a…19",
  "epoch": 20, "epoch_blocks": 1000,
  "genesis_hash": "1cff3b7d…c7ff",
  "node_version": "0.1.0", "node_git_sha": "b3c594cd5872bbc132cfafcd7314614436e6131a",
  "fri_profile": "production",
  "limits": { "max_program_words": 65535, "max_proof_bytes": 4194304, "max_block_bytes": 20971520,
              "max_call_envelope_bytes": 65536, "max_program_public_words": 32768,
              "envelope_bytes": 1860, "hardening_v6": true, "hc_auth": "60af…3fce",
              "gas_price": "100", "byte_price": "800", "gas_metering": "circuit",
              "bundle_gas_limit": 20479, "adjust_bps": 1250 },
  "hc_auth": "60af…3fce",
  "gas_prices": { "gas_price": "112", "byte_price": "900" },
  "updated_at": "2026-09-12T05:20:38.591804+00:00"
}
```

`notes` is every note the chain has ever created (leaves of the commitment tree) and
`nullifiers` every note it has ever spent. `total_stake` sums the *active* set. `total_supply`
and `pool_value` come from the node's supply audit and are `"0"` / `null` on a node that does
not serve it. `tree_root` is the current commitment-tree root and `hc_bundle` the digest of the
bundle guest every proof on this chain is checked against. `current_leader` is the validator
expected to propose the next block.

`genesis_hash` is block 0's hash as the node reports it — a chain id alone does not tell two
cuts apart. `node_version` and `node_git_sha` say which build the explorer's node runs (`-dirty`
is appended when that build's tree was not clean), and `fri_profile` which proof profile.
`limits` are the size caps the chain's genesis sets (the node's `rand_getLimits`; they were
constants before chain 13): the most code words a program may have, the largest proof, the
largest block and so the largest transaction, the largest sealed call-input envelope, and the
most public words a deploy may fix (`0` means no program on the chain has a public input). Each
of the five is `null` on a node too old to serve it, and they are re-read on every stats refresh,
so a same-chain node update shows up here without a re-index.

The fields after the five caps came with chains 16, 17 and 18 and are `null`/`false` both on a
chain without the setting and on a node too old to report it:

- `envelope_bytes` — the exact size of every note envelope. `1860` on a chain whose notes carry an
  encrypted memo (chain 18: `note ‖ 512-byte memo field`); `null` where wallets seal the legacy
  1 348-byte form and a memo-carrying envelope from elsewhere is merely tolerated.
- `hardening_v6` — the v0.6 rules (the pc window, canonical proof shapes, the call binding, the
  program-table floor) as validity rules rather than pool policy (chains 16+).
- `hc_auth` — the auth guest the genesis pins (split authorisation, chains 17+), hex. Set, the
  chain's `hc_bundle` is bundle guest v3 and every bundle publishes an `auth_commit` with a second
  proof over the spend key beside it (see the bundle below).
- `gas_price`, `byte_price`, `gas_metering` — how a confidential call is priced. `gas_metering`
  is `"circuit"` on a chain whose genesis carries a `gas` section (chain 18: every call pays
  `BUNDLE_BASE + gas_price · declared_gas + byte_price · ⌈bytes / 1024⌉`, the meter runs inside
  the proof, and the prices are consensus state every node reports alike), `"header"` while only
  the explorer's node prices `gas_max` of a proof's header as its own admission policy (two nodes
  may answer differently), `null` with neither. Prices are RAND units, decimal strings.
- `bundle_gas_limit` — the flat gas every bundle proof must declare (`20479` on chain 18);
  `adjust_bps` — the dynamic controller's per-block step in basis points (`1250` = 12.5%), `null`
  when the prices never move. Both `null` without a `gas` section.
- `program_state` — RPL-2 (fullnode v0.6.8): `{ "cell_fee": "10000000", "max_reads": 8,
  "max_writes": 8, "max_payouts": 4 }` on a chain whose genesis has the section (`cell_fee` is
  the RAND units an invoke's fee floor gains per cell it creates), `null` without it — where
  every `invoke` is refused — and absent on a node predating the field.

`hc_auth` (top level) is the same guest as `rand_status` reports it, and `gas_prices` are the
**tip's** live prices, refreshed every commit: under a dynamic `gas` section they move per block
by fullness and can differ from the genesis prices `limits` reports; `null` on a chain without a
`gas` section. A call's own declared gas limit is not on the node's RPC (it is a public value of
the proof, not a field of the receipt), so the explorer cannot show it per transaction.

`GET /supply` returns the audit verbatim (all units strings):

```json
{ "height": 1998,
  "genesis_deposited": "…", "genesis_staked": "…", "faucet_minted": "…",
  "withdraw_deposited": "…", "fees_paid": "…", "burned": "…",
  "pool_value": "…", "register_total": "…", "total_supply": "…", "invariant_holds": true,
  "vesting_issued": "…", "vesting_released": "…", "vesting_in_register": "…", "vesting_locked": "…",
  "program_rand_out": "…", "program_rand_held": "…",
  "base_fees_burned": "…",
  "multisig_issued": "…", "multisig_rand_in": "…", "multisig_rand_out": "…",
  "multisig_base_out": "…", "multisig_rand_held": "…",
  "perps_rand_out": "…", "perps_rand_held": "…" }
```

Fullnode v0.8.0's counters: `base_fees_burned` (bundle bases burned under `fees.burn_base`), the
five `multisig_*` (what genesis seeded accounts with, RAND creates and deposits put in, RAND paid
out as notes, bases paid to proposers, what the accounts hold) and `perps_rand_out` /
`perps_rand_held` (RAND only; `"0"` on a chain whose collateral is a token, as chain 21's zUSD
is). Each is `"0"` on a chain without its section and `null` from a node predating it.

`program_rand_out` and `program_rand_held` are RPL-2: RAND that invokes have paid out of program
vaults as notes (pool side, beside `withdraw_deposited`) and what the vaults still hold (register
side, inside `total_supply`); `"0"` on a chain without a `program_state` section, `null` on a
node predating them.

Note values are hidden, but every crossing of the pool boundary is public, so these are exact.
`invariant_holds` false would be a chain bug, never a legitimate state. The four `vesting_*`
fields are genesis vesting (chains 17+): what genesis issued into the vesting register, what
claims and revokes released into the pool, what the register still holds, and what of that has
not unlocked yet; `"0"` on a chain without a `vesting` section and `null` on an older node.

`GET /bridge`:

```json
{
  "enabled": true, "emitter": "01…", "emitters": { "2": "02…" },
  "guardian_set_index": 0, "guardians": ["aabb…"],
  "pq_guardians": ["…", "…"],
  "mint_paused": false, "pause_nonce": 0, "list_nonce": 0, "pause_key": "dd…",
  "registration_fee": 1000000000,
  "burn_sequence": 1, "next_index": null,
  "rotation_nonce": 0,
  "rules_v2": { "global_mint_cap_per_window": "400000000000", "cap_window_secs": 86400 },
  "min_inbound_sequence": { "2": 1, "3": 1, "4": 1, "5": 4 },
  "assets": [{ "index": 1, "chain": 2, "token": "aaaa…", "asset_id": "…",
               "decimals": 6, "locked": "600", "minted_today": "0", "mint_day": 20345 }],
  "endpoints": [{
    "chain": 4, "chain_name": "Tron",
    "emitter": "0000000000000000000000006410797df959987a5baf65b5fab97edeb34d5163",
    "address": "TK6JJv55CCkFjNHq7WwoU91GKaZEiC93me",
    "explorer_url": "https://tronscan.org/#/contract/TK6JJv55CCkFjNHq7WwoU91GKaZEiC93me",
    "min_inbound_sequence": 1
  }]
}
```

`assets[].index` is the `asset_index` a bridged note carries; index 0 is RAND. There are no
balances: bridged value is notes. A chain without a bridge reports `{ "enabled": false }`.
`pq_guardians`, `mint_paused`, `pause_nonce`, `list_nonce`, `pause_key`, `registration_fee` and
`assets[].decimals`/`locked`/`minted_today`/`mint_day` are bridge hardening B1/B3/B4 — absent or
`null`/empty on a node predating them. `mint_paused: true` means every transfer attest is refused
(burns and guardian-set rotations stay open); lifting the pause needs the PQ guardian quorum, the
pause key alone can never unpause. `registration_fee` and `next_index` are the two fields served
as plain numbers here, not decimal strings (`next_index` is `null` since chain 14: a bridged token
is listed before it can be deposited, so there is no index left to predict).

`rotation_nonce` and `rules_v2` are bridge rules v2: the nonce the next PQ-set or pause-key
rotation must carry, and the cap on what every backing of every token together may mint per
window (bridge units, a decimal string; `rules_v2` is `null` on a chain without the group).
`min_inbound_sequence` is the genesis replay floor: per source chain, the lowest sequence a lock
may carry to be minted here (`null` on a chain without one). A chain cut from a predecessor sets
it past every lock the predecessor already minted.

On a node after v0.6.7 (audit v6, BRG-19) every backing row here, in `GET /bridge/assets` and in
a token's `backings` also carries `minted_in_window` and `mint_window_secs` (the rolling-window
count and the window under bridge rules v2; `null` on a day-counter chain) and `mint_headroom`,
the largest deposit the caps admit to that backing right now — read that, not
`mint_cap_per_day − minted_today`, which ignores the registry-wide cap; and `rules_v2` gains
`global_minted_in_window` and `global_mint_headroom`. All `null` (or absent) on an older node.

`endpoints` is the explorer's own, not the node's: one row per entry of `emitters`, in chain id
order, with the emitter as its chain prints it — `0x` + 20 bytes (lowercase) for Ethereum and BSC,
base58check for Tron, base58 for Solana — the page of that contract or program on the chain's
explorer, and the chain's replay floor. `address` and `explorer_url` are `null` when the 32-byte
word is not a well-formed address of that chain, `chain_name` when the chain id has no name here.
**Chain 19 (2026-10)** is chain 18 re-cut on the same build for the Ethereum, BSC and Tron bridge
contracts redeployed on 2026-09-30; the values above are its genesis. The contracts chains 14–18
trusted (`0xd6ebd21c…4892` on Ethereum and BSC, `TAqq2i8K…PpkU` on Tron) are not trusted by chain
19, and a lock made there is never minted on it.

`GET /bridge/assets` joins that registry with the indexed `bridge_attest` and `bridge_burn`
transactions, **one row per backing** (source coin) in registry order (an empty array on a chain
without a bridge). Since chain 14 one bridged token — one `index` — may have several backings:
zUSD is index 1 with seven of them.

```json
[{
  "index": 1, "chain": 3, "token": "00000000000000000000000055d39832…", "asset_id": "…",
  "symbol": "USDT", "name": "Tether USD", "decimals": 18, "backings": 7,
  "deposits": null, "deposited": null, "burns": 0, "burned": "0",
  "outstanding": "100000000",
  "token_deposits": 3, "token_deposited": "300000000", "token_burns": 0, "token_burned": "0",
  "first_height": 1040, "last_height": 19877,
  "locked": "100000000", "minted_today": "100000000", "mint_cap_per_day": "10000000000000"
}]
```

`chain` is the bridge chain id of the token's home: 2 Ethereum, 3 BSC, 4 Tron, 5 Solana. `symbol`,
`name` and `decimals` are set when `(chain, token)` is on the approved list below and `null`
otherwise. Amounts are strings of **bridge units, always 8 decimals** whatever the token's own
decimals at home; the bridge contracts convert on the way in and out. A guardian-set rotation is a
`bridge_attest` with no asset and is not counted.

What is a backing's own and what is the whole token's:

- `burns` / `burned` are **this backing's**. A burn names the coin it redeems, `(to_chain, token)`,
  and the ledger holds that pair to one of the token's backings.
- **Bridge fees (fullnode v0.6.8, chain 20's `bridge.fees`).** `deposited` / `token_deposited` are
  the gross each attestation signed — what `locked` and custody grew by. `burned` /
  `token_burned` are the **release amounts**, what left `locked` (the burn less the fee).
  `burn_fees`, `token_deposit_fees` and `token_burn_fees` are the fee notes the chain minted to the
  fee recipient; they stay in circulation, so `supply == Σ locked == custody` still holds
  (`outstanding == deposited − burned` for a token with one backing).
- `outstanding` is **this backing's**: what it holds for the chain right now, the registry's
  `locked`. (On a node too old to serve `locked` it is rebuilt as `deposited - burned`, which is
  exact there because such a node has one backing per token.) `locked`, `minted_today` and
  `mint_cap_per_day` are the registry's figures verbatim (bridge hardening B1).
- `deposits` / `deposited` are this backing's **only when it is the token's one backing**
  (`backings` = 1), and `null` otherwise. A deposit publishes the token it minted and not the coin
  that was locked for it — that is inside the attestation bytes, which the node serves by length
  alone — so with several backings it cannot be laid at one of them.
- `token_deposits`, `token_deposited`, `token_burns`, `token_burned`, `first_height` and
  `last_height` are **the whole token's**, identical on every row that shares the `index`. To total
  them across rows, count each `index` once; a token's per-backing `burns` add up to its
  `token_burns`.

**Changed 2026-09-20.** Before this, `deposits`, `deposited`, `burns`, `burned` and `outstanding`
were tallied per `index` and repeated on every backing of the token, so on chain 14 each of
zUSD's seven rows reported the whole token's figures (and a client summing the rows counted every
deposit seven times), while `outstanding` disagreed with `locked` on the same row. Breaking for a
client that read `deposits`/`deposited` as always present: they are now nullable.

`GET /bridge/tokens` is the allowlist, a static list that does not depend on the node:

```json
[{
  "symbol": "USDT", "name": "Tether USD", "chain": 2, "chain_name": "Ethereum", "standard": "ERC-20",
  "address": "0xdAC17F958D2ee523a2206206994597C13D831ec7",
  "token": "000000000000000000000000dac17f958d2ee523a2206206994597c13d831ec7",
  "decimals": 6, "status": "allowed",
  "explorer_url": "https://etherscan.io/token/0xdac17f958d2ee523a2206206994597c13d831ec7"
}]
```

`address` is the contract (or Solana mint) as the chain's explorers print it; `token` is the same
address as the registry stores it, a 32-byte word (an Ethereum, BSC or Tron address left-padded
with 12 zero bytes; a Solana mint verbatim), so it can be matched against `assets[].token`.
`status` is `allowed` or `discontinued`; a discontinued entry carries a `note` saying why and is
listed so the address is on record, not because deposits are accepted. Today the list is USDT and
USDC on Ethereum, BSC, Tron and Solana, with USDC on Tron discontinued.

### RPL tokens

`GET /tokens` (the whole registry, as cached from the node — this call tells the node nothing
about which token you care about, unlike a per-token lookup):

```json
{
  "enabled": true, "registration_fee": 1000000000, "next_index": 4,
  "tokens": [
    { "index": 3, "id": "aabb…", "id_text": "rpl1…", "name": "zUSD", "symbol": "zUSD",
      "decimals": 6,
      "authority": { "kind": "bridge", "backings": [
        { "chain": 2, "token": "bbcc…", "decimals": 6, "locked": "600",
          "minted_today": "0", "mint_day": 20345, "mint_cap_per_day": "10000000000" }
      ] },
      "mint_nonce": 1, "total_supply": "5700", "registered_at": 3 }
  ]
}
```

`index` is the `asset` word a note of this token carries — 0 is RAND and never appears here.
`id_text` is the checksummed `rpl1…` text form (bech32m, HRP `rpl`, 62 characters). `authority` is
`{"kind":"none"}` (fixed supply, or renounced), `{"kind":"key","key","address"}`,
`{"kind":"bridge","backings":[…]}` (each backing's **source** decimals and locked amount — the
token itself is always eight decimals on Rand) or `{"kind":"program","program"}`.
`registration_fee` and `next_index` are plain numbers; every other amount is a decimal string. A
chain without a token registry (or a node predating this endpoint) reports
`{ "enabled": false, "tokens": [] }`.

`GET /tokens/:id` — `:id` is a registry index, a 64-hex id, or an `rpl1…` text form:

```json
{
  "index": 3, "id": "aabb…", "id_text": "rpl1…", "name": "zUSD", "symbol": "zUSD", "decimals": 6,
  "authority": { "kind": "bridge", "backings": [ … ] },
  "mint_nonce": 1, "total_supply": "5700", "registered_at": 3,
  "deploy_tx": "4f2c…e7",
  "supply_history": [
    { "tx_hash": "4f2c…e7", "height": 3, "timestamp_ms": 1789000003000, "kind": "register_token", "delta": "5000" },
    { "tx_hash": "9a1b…", "height": 10, "timestamp_ms": 1789000010000, "kind": "token_mint", "delta": "700" },
    { "tx_hash": "c3d4…", "height": 12, "timestamp_ms": 1789000012000, "kind": "token_burn", "delta": "-400" }
  ]
}
```

`deploy_tx` is the `register_token` (or `register_bridged_token`) transaction, `null` when it
predates this indexer's tracking. `supply_history` is every public mint/burn event naming this
token, oldest first, `delta` signed in the token's own smallest unit — the same events
`GET /tokens` sums into `total_supply`. 404 when no token matches `:id`.

**Privacy:** `GET /tokens/:id` tells the node which token you asked about; a wallet resolving a
transfer's asset reads the whole `GET /tokens` list instead, which costs the same whichever token
is meant. Neither endpoint says anything about which *notes* hold a token — that stays private,
disclosed only by a viewing key or a transaction key, never by index alone.

`GET /nodes` returns an array of:

```json
{
  "peer_id": "12D3KooW...", "ip": "167.172.65.63", "port": 30303,
  "connected_secs": 86400, "is_self": false, "role": "peer",
  "geo": { "lat": 1.29, "lon": 103.85, "city": "Singapore", "region": null, "country": "Singapore", "country_code": "SG", "org": "DigitalOcean" }
}
```

`role` is `validator` or `observer` for the explorer's own node and `peer` for remote peers, whose
role is not known. `geo` is `null` for private or unresolved addresses.

### Blocks

| method and path | returns |
|---|---|
| `GET /blocks?page&limit&proposer` | paginated `BlockSummary`, newest first |
| `GET /blocks/latest?limit=10` | array of the newest `BlockSummary` (limit 1 to 100) |
| `GET /blocks/:id` | `BlockDetail`; `:id` is a height or a block hash |

`BlockSummary`:

```json
{
  "hash": "b171d72a96923673b6d96793530f942c07d62fa9571315c73187e8b4556215eb",
  "height": 20147, "view": 23256,
  "parent": "5460c3ca5899aa2880986f06b48e7c8d5781d26048b30149cc9d1881905b01f1",
  "proposer": "2nRdFChBXRmKoe2sQE3ZYDzvdg53QmBZJJ9iweY7hk1v",
  "timestamp_ms": 1789017624229, "tx_count": 0, "justify_view": 23255
}
```

`BlockDetail` adds `tx_root`, `state_root`, and `transactions` (an array of `TransactionSummary`
in block order). `view` and `justify_view` are HotStuff consensus views; most integrations only
need `height`.

### Transactions

| method and path | returns |
|---|---|
| `GET /transactions?page&limit&kind&height&validator&program` | paginated `TransactionSummary`, newest first |
| `GET /transactions/latest?limit=10` | array of the newest `TransactionSummary` |
| `GET /transactions/:hash` | `TransactionDetail` |
| `GET /tx/:hash` | alias of the above; the hash may carry a `0x` prefix. The page `/tx/:hash` redirects (308) to `/transactions/:hash` the same way |

Filters: `kind` is one of `transfer`, `mint`, `deploy`, `call`, `bond`, `unbond`, `withdraw`,
`bridge_attest`, `bridge_burn`, `register_token`, `token_mint`, `set_authority`, `token_burn`,
`pause_mints`, `unpause_mints`, `register_bridged_token`, `list_backing`; `height` restricts to
one block; `validator` to the staking actions (and mints) of one validator address; `program` to
the deploy and calls of one program. There is no `token_transfer` filter: a transfer of any RPL
token is `transfer`, indistinguishable from a RAND payment.

`TransactionSummary`:

```json
{
  "hash": "ba03893d4b5b4f03ab8ab85aa9e8df4b46102986f15ef56653f7450b98bdba34",
  "height": 19340,
  "block_hash": "890715533e41e9ddbeac0edc0d2722847248e3affba70816fff31ca54bc1b80f",
  "tx_index": 0,
  "kind": "transfer", "fee": "1000000", "timestamp_ms": 1789009705401,
  "has_bundle": true,
  "program": null, "validator": null, "amount": null, "asset_index": null
}
```

There is no sender, recipient, nonce or (for a transfer) amount: a stored transfer has no such
field, and its `asset_index` is always `null` — a transfer's asset is private, full stop. Every
kind's amount whose `asset_index` names a token other than RAND (index 0) is in that token's own
smallest unit, not RAND's 9 decimals; resolve `asset_index` through `GET /tokens/:id` first.
`has_bundle` is false for the validator/authority-signed and PQ-guardian-only actions (`mint`,
`unbond`, `withdraw`, `pause_mints`, `unpause_mints`), which carry no bundle and pay no fee. The
kinds and which fields they fill:

| kind | what it is | `amount` | other summary fields | detail-only fields |
|---|---|---|---|---|
| `transfer` | a plain shielded transfer of RAND or any RPL token (the node's `none` action) — the asset is private | null | | `bundle` |
| `mint` | testnet faucet deposit, signed by a validator | RAND units of the new note | `validator` = the minter | `cm` |
| `deploy` | a zkVM program deployment, paid by the bundle | null | `program` = the new program id | `words_len` |
| `call` | a confidential call, paid by the bundle | null | `program` | `call_proof_len`, `input_envelope_len`, `receipt` |
| `bond` | stake leaving the pool into a validator's register entry | RAND units | `validator` | `registered` (a first-time registration) |
| `unbond` | stake moved to the unbonding queue (validator-signed) | RAND units | `validator` | `action_nonce` |
| `withdraw` | released stake and rewards deposited as a new note (validator-signed) | RAND units | `validator` | `action_nonce`, `note_time` (the deposit note's time word) |
| `bridge_attest` | a guardian-signed inbound message depositing a bridged note | that token's units, the **gross** the guardians signed (null for a guardian-set rotation) | `asset_index` | `attestation_len`, `recipient`, `note_time`, `deposit_r`, `commitment` (the net deposit's leaf), `pq_signers`, `deposit_amount` (net), `fee_note` |
| `bridge_burn` | a bridged asset burned to another chain, one bundle | that token's units, what the bundle burned | `asset_index` | `relayer_fee`, `to_chain`, `bridge_to`, `bridge_token`, `release_amount`, `fee_note` |
| `register_token` | an RPL token registered, with an optional initial mint | the initial mint's amount, or null | `asset_index` = the new index | `token_action`, `recipient`, `note_time`, `deposit_r` |
| `token_mint` | a signed mint of an existing RPL token | that token's units | `asset_index` | `token_action`, `recipient`, `note_time`, `deposit_r`, `action_nonce` |
| `set_authority` | an RPL token's mint authority changed (or renounced) | null | `asset_index` | `token_action`, `action_nonce` |
| `token_burn` | a holder burn of an RPL token, public by design | that token's units | `asset_index` | `token_action` |
| `pause_mints` | the bridge's genesis pause key pauses all transfer attests | null | | `bridge_governance`, `action_nonce` |
| `unpause_mints` | the PQ guardian quorum lifts the mint pause | null | | `bridge_governance`, `pq_signers`, `action_nonce` |
| `register_bridged_token` | a new bridged token listed after genesis by the PQ guardian quorum | null | | `bridge_governance`, `pq_signers` |
| `list_backing` | another source-chain backing added to an already-listed bridged token | null | `asset_index` | `bridge_governance`, `pq_signers` |
| `invoke` | RPL-2: a confidential call whose proof vouches for one declared state transition of the program, which the ledger applied | null | `program` | `call_proof_len`, `input_envelope_len`, `receipt`, `transition` |
| `admit_validator` | audit v6: the validator set's vote admitting a key to register | null | `validator` = the candidate | `staking_action` (`candidate`, `candidate_key`, `voters`) |
| `slash_equivocation` | audit v6: two headers one key signed for one view | null | `validator` = the offender | `staking_action` (`offender`, `view`, `first`, `second`) |
| `rotate_pq_guardians` / `rotate_pause_key` | bridge rules v2: a new PQ guardian set / pause key | null | | `bridge_governance`, `pq_signers`, `action_nonce` |
| `rotate_pq_guardians_v2` / `rotate_pause_key_v2` | audit v6: the same with proofs of possession, effective after the genesis delay | null | | `bridge_governance` (`possession_signatures` on the PQ one), `pq_signers`, `action_nonce` |
| `cancel_rotation` | audit v6: a pending rotation withdrawn | null | | `bridge_governance` (`rotation_kind`), `action_nonce` |
| `perp_deposit` | RPL-3: opens or tops up a trading account with the bundle's collateral burn | null | | `perp_action` (`trading_key`, `account`), `bundle` (the burn) |
| `perp_order` | RPL-3: an order recorded for the proved engine (bundle-less) | null | | `perp_action` (`account`, `body`: `nonce`, `market`, `side` 0 buy / 1 sell, `kind` 0 limit / 1 market, `tif` 0 GTC / 1 IOC / 2 post-only, `reduce_only`, `price`, `size`), `action_nonce` |
| `perp_cancel` | RPL-3: cancels the order with nonce `target` | null | | `perp_action` (`account`, `nonce`, `target`), `action_nonce` |
| `perp_withdraw` | RPL-3: a withdrawal request, paid when a state proof covers it | null (collateral units, in `perp_action.amount`) | | `perp_action` (`account`, `nonce`, `amount`, `recipient`, `time`), `recipient`, `note_time`, `action_nonce` |
| `perp_oracle` | RPL-3: a validator's prices, one per market | null | `validator` | `perp_action` (`prices`), `action_nonce` |
| `perp_state_proof` | RPL-3: a prover's state transition over a window of blocks | null | | `perp_action` (`from_height`, `to_height`, `new_root`, `payouts`, `fees`, `proof_bytes`) |
| `create_multisig` | a new M-of-N account, funded by the bundle's burn | null | | `multisig_action` (`salt`, `threshold`, `signer_count`) |
| `multisig_deposit` | the bundle's burn into an existing account | null | | `multisig_action` (`account`) |
| `multisig_pay` | notes paid out of an account, signed by a threshold of its signers (bundle-less) | null | | `multisig_action` (`account`, `nonce`, `time`, `pays` — each note as an invoke's payout, its leaf linked to this transaction — `signers` by position), `note_time`, `action_nonce` |
| `multisig_rotate` | a new signer set and threshold under the same id (fee-less) | null | | `multisig_action` (`account`, `nonce`, `threshold`, `signer_count`, `signed_by`), `action_nonce` |
| `set_rand_price` | the validator set's vote on the RAND/USD price (`fees.usd_subsidy`) | null | | `staking_action` (`price_micros_per_rand`, `nonce`, `voters`), `action_nonce` |
| `aggregate` | one proof standing in for the bundle proofs of the blocks it covers | RAND units of `payout_total` (null from a node before v0.8.0) | | `aggregate` (`covers`, `proof_len`, `aggregator`, `nonce`, `time`, `payout_total`, `layout` `flat` / `tree`), `action_nonce` |
| `other` | a kind newer than this explorer build | null | | none |

An `invoke`'s `transition` is the node's rendering, verbatim but for amounts as decimal strings:

```json
"transition": {
  "reads":  [{ "key": "0100…00", "value": "0000…00" }],
  "writes": [{ "key": "0100…00", "value": "0500…00" }],
  "inflow": "deposit",
  "pays":  [{ "asset": 0, "amount": "300", "recipient": "rand1…", "time": 41, "r": "…", "cm": "…" }],
  "mints": [{ "asset": 2, "amount": "40", "recipient": "rand1…", "time": 41, "r": "…", "cm": "…" }] }
```

`reads` are the cells the program read with the values it read (the ledger refused the
transaction unless they still held — a read of 64 zeros is of a cell that did not exist),
`writes` the cells it wrote (64 zeros deletes the cell). What came **in** is the bundle's:
`burn_r` is RAND deposited into the program's vault, and `burn_a` of `burn_asset` is the token
`inflow` names — `deposit` into the vault, `burn` destroyed (the program's own token), `none`
when `burn_a` is 0. What went **out** is one chain-computed note per payout, `pays` (out of the
vault) then `mints` (new units of a token whose mint authority is the program), in that order in
the tree; `time` is the bundle's and `cm` the leaf the chain appended, which this explorer links
to the transaction like a `token_mint`'s note (`GET /notes/:cm`, and among the transaction's
`envelopes`). The receipt is a call's.

`TransactionDetail` adds `chain_id`, `bundle` and the per-kind fields above (null when they do
not apply). The bundle is the transaction's public face — chain 14's hidden-asset bundle, four
slots, dummies included, **with no public asset field**:

```json
"bundle": {
  "anchor": "6b1d…c4",
  "nullifiers": ["8c04…d1", "5e77…20", "03aa…6f", "e19b…42"],
  "commitments": ["2a9f…07", "b310…88", "77c1…0e", "5d20…b3"],
  "fee": "1000000", "burn_a": "0", "burn_r": "0", "burn_asset": 0, "time": 5,
  "proof_len": 302857, "envelope_len": [1860, 1860, 1860, 1860],
  "auth_commit": "c0f1…9a", "auth_proof_len": 1360512
}
```

`auth_commit` and `auth_proof_len` are split authorisation (chains 17+, `limits.hc_auth` set):
the bundle proof is made from the viewing key and publishes `c = H(AUTH, nk, salt)`; a second,
small proof over the spend key, bound to the same transaction, must match it — which is what lets
a delegated prover make the bundle proof without ever holding the spend key. On a chain without
an auth guest the commit is all zeros and the length `0`; `auth_commit` is `null` for a
transaction indexed from a node that predates the field. The auth proof, like the bundle proof,
is reported by length only.

`anchor` is the tree root the proof was made against; `nullifiers` mark the four spent notes
(a dummy input still publishes one, so every bundle looks alike, and slots 1–2 carry a private
asset while slots 3–4 always carry RAND); `commitments` are the four notes created. `burn_a` is
the private asset burned (non-zero only on a `token_burn` or a `bridge_burn`, `burn_asset` naming
which registry index) and `burn_r` is RAND burned (non-zero only on a `bond`); `time` is the
height the sender targeted. The proof and the four encrypted envelopes are reported by size only;
**nothing in a bundle names a sender, receiver, amount or asset** — a transfer of RAND and a
transfer of any RPL token are the identical shape. A `bridge_burn` burns through this one bundle
(`burn_a`/`burn_asset` are the action's own asset and amount; there is no second bundle since
chain 14); `bridge_token` is the backing being redeemed, hex, and `bridge_to` a 32-byte hex
address on `to_chain` (1 Rand, 2 Ethereum, 3 BSC, 4 Tron, 5 Solana; 20-byte addresses left-padded
with zeros).

RPL token actions (`token_action`) and bridge-governance actions (`bridge_governance`) carry the
action's own fields in full, tagged by `kind`:

```json
"token_action": {
  "kind": "register_token", "name": "zUSD", "symbol": "zUSD", "decimals": 6, "authority": "bridge",
  "index": 3, "initial_amount": "5000",
  "initial": { "amount": "5000", "recipient": "rand1…", "time": 3, "r": "bb…" }
}
```

`register_token`'s `authority` here is the bare tag (`"none" | "key" | "bridge" | "program"`); the
full authority (with its key or backings) is `GET /tokens/:id`'s. `initial` is `null` for a
registration with no initial mint. `token_mint` adds `asset`, `amount`, `recipient`, `time`, `r`,
`nonce`; `set_authority` adds `asset`, `nonce`, `new_authority` (`null` is a renunciation — the
token can never be minted again); `token_burn` adds `asset`, `amount`. Every word of a minted
note (`register_token`'s initial mint or a `token_mint`) is here — `recipient`, `note_time`
(top-level) and `deposit_r`, alongside `amount` — so a recipient rebuilds the note with nothing
decrypted, whatever envelope the minter published; the note's own commitment is not published
directly (unlike a `bridge_attest`'s), so look for it among `GET /transactions/:hash/envelopes`'s
notes instead.

`bridge_governance`'s four kinds (`pause_mints`, `unpause_mints`, `register_bridged_token`,
`list_backing`) each carry their own signed fields plus `nonce`; all but `pause_mints` (the
genesis pause key's own signature) are authorised by the PQ guardian quorum, reported at the
top level as `pq_signers` (the co-signing guardians' indices) — the same field a `bridge_attest`
carries.

The receipt of a confidential call:

```json
{
  "tx": "b08244b044e8d719aa4e2c1bd22a92914924ae7a6cd41c4c363a608e211f09b0",
  "program": "675adeea7e4242d8dc48bf56faedb7bea14a4f832d7c8a973f942fa7dd850065",
  "tier": 14, "outputs": [1, 0, 200, 0, 0, 0, 0, 0],
  "height": 105, "index": 0, "h_in": "9c0e…7f", "h_pub": null
}
```

`outputs` are the program's eight public output words; they no longer move value (the old
effect kind 1 is gone with the accounts). `h_pub` is the digest of the program's deploy-time
public input the proof was checked against — the program's `public_digest` — and `null` when the
program has none, in which case the proof was checked against the digest of the empty input.
`h_in` is the proof's salted commitment to the call's
private inputs; `input_envelope_len` says whether the caller published a sealed transcript of
those inputs (null when not). The transcript opens only for the caller's viewing key, the
per-call key, or the auditor the caller named; the node serves the bytes
(`rand_getCallEnvelope`), the explorer only their size. Proof sizes depend on the zkVM
constraint set the chain runs (about 1.2 MB per call proof at constraint set 5); the explorer
reports what the node reports.

### Notes and nullifiers

| method and path | returns |
|---|---|
| `GET /notes?page&limit` | paginated leaves of the commitment tree, newest first |
| `GET /notes/:id` | one leaf by commitment (64 hex) or by leaf index (decimal) |
| `GET /nullifiers/:nf` | the transaction that published a nullifier |
| `GET /nullifiers?after&limit` | the published nullifier set in nullifier order, up to 5000 a page: `{"nullifiers": [...], "next": "…"}` (`next` is the `after` of the next page, `null` on the last). There is no way to ask about particular nullifiers in bulk: a nullifier is derived from a wallet's key, so a client downloads the set and matches its own locally |

```json
{ "leaf_index": 40, "cm": "2a9f…07", "height": 37, "tx_hash": "4f2c…e7" }
{ "nullifier": "8c04…d1", "tx_hash": "4f2c…e7", "height": 41, "tx_index": 0 }
```

`tx_hash` of a note is the transaction whose bundle, mint, or chain-computed note (a
`bridge_attest` deposit, a `token_mint`, or a `register_token`'s initial mint) carried or created
the commitment, and `null` for a genesis deposit or a validator's withdraw deposit (whose
commitment is computed by the chain and not on the wire, and whose envelope this indexer does not
attribute to a transaction). The leaf index is what a wallet uses to ask the node for a Merkle
witness. A wallet that wants to detect payments scans envelopes with its viewing key; the
explorer cannot do that for you.

### Envelopes and viewing keys

| method and path | returns |
|---|---|
| `GET /transactions/:hash/envelopes` | the notes a transaction created, each with its envelope; for a call also `h_in` and the sealed input transcript |
| `GET /envelopes?from_leaf&limit` | leaves with envelopes, oldest first (limit 1 to 1000, default 500), for a history scan |

```json
{
  "hash": "5e90…ead", "kind": "transfer",
  "notes": [{ "leaf_index": 8, "cm": "da4f…", "height": 114, "tx_hash": "5e90…ead",
              "envelope": { "kem_ct": "…", "to_receiver": "…", "to_sender": "…", "body": "…" } }, …],
  "h_in": null, "call_envelope": null
}
```

An envelope is public chain data: the note plaintext under a per-transaction key, that key
wrapped to the receiver's ML-KEM-768 address and under the sender's outgoing viewing key, all
ChaCha20-Poly1305 with the commitment as associated data. Only a key opens it, and the explorer
holds none: the transaction page and the History page do the opening in your browser with a
WebAssembly build of the node's own code (`crates/randscan-viewing`). A **viewing key** (`nk`,
64 hex) opens every note that party sent or received; a **transaction key** (32 bytes hex) opens
one transaction; a **call key** opens one call's inputs. The transaction page takes only a
viewing key or a transaction key (tried as the call key too on a call), and the History page
only a viewing key; neither asks for a spend key or the wallet key file. An opened note is verified by recomputing its commitment,
so what the page shows is what the chain committed to, not what a ciphertext claims. To build
your own tool, fetch these endpoints and use the crate; nothing about a key ever goes over the
network.

`GET /transactions/:hash/envelopes`'s `notes` array holds the bundle's four slots (dummies
included) **plus** the one chain-computed note a `bridge_attest` deposit, a `token_mint` or a
`register_token`'s initial mint appends — its envelope rides in the *action* on the wire, not the
bundle, but the node indexes and serves it the same way as any other leaf. A dummy input or
output opens for no key at all (it is sealed to nobody), which the transaction page shows the
same way as "not opened by this key" — it never says "dummy" outright, since the chain gives no
way to tell a real note that is simply not yours from a genuine dummy.

Once a key opens a note, its disclosed `asset` is the token registry index — resolve it through
`GET /tokens/:id` (or the cached `GET /tokens` list) for the symbol and decimals: index 3 at
6 decimals renders "12.50 zUSD". This is the **only** place the API ever ties an amount to a
specific asset for a *transfer* — the public transaction page and every list view never do,
because they do not have the key.

`GET /envelopes` pages with `next_leaf` (`null` on the last page) and reports `total_leaves`.
`envelope` is `null` for a leaf indexed before envelopes were stored; it fills in on the next
indexer pass.

**Chain-computed notes (`public`, fullnode v0.6.8).** Some leaves are notes the *chain* computed
from a transaction's public fields rather than a bundle's output: a bridge deposit, a bridge fee
note (chain 20's `bridge.fees`), an RPL `token_mint` or `register_token` initial mint, and an
RPL-2 invoke's payouts. Each such leaf (in both envelope endpoints) carries

```json
"public": { "source": "bridge_deposit", "amount": "99900000", "asset": 1, "time": 1599,
            "r": "63a44e149332d851011f20ac2c2622e814297177c8b18f7447fe9a0484edb7a0" }
```

— every word of the note but its owner `pk` and its `from` word, which `source` fixes
(`bridge_deposit` / `bridge_fee`: zero; `token_mint` / `initial_mint`: `MINT_FROM`; `payout`:
`PROGRAM_FROM`). `public` is `null` for a bundle's notes. A **bridge fee note has no envelope at
all** (it is sealed to nobody), so trial decryption never finds it; a deposit's envelope is the
submitter's and may be junk. The viewing wasm's `rebuild_note(cm, public_json, key_kind, key)`
rebuilds the note with the key's *own* `pk` and returns it (in `open_note`'s shape, `role`
`received`, with its nullifier, plus `source`) only when it commits to the leaf `cm` — so a wrong
`public` can hide a note but never credit one. A deposit's `amount` here is the net (the gross
less the bridge fee), which is what the note holds.

### Accounts

`GET /accounts/:address` and `GET /accounts/:address/transactions` answer **410**
`{ "error": "no_accounts" }`. There are no accounts on this chain. For deposit detection, run a
wallet with your viewing key (`rand sync`); to watch stake, read `/validators`.

### Provers

`GET /provers` lists the delegated provers (fullnode `docs/prover.md`) this explorer knows of —
the chain keeps no record of them, and a pool hides its hosts behind one URL, so the list is the
explorer's own (`KNOWN_PROVERS` in `randscan-core`). Each endpoint's `prover_info` is polled every
`NODES_INTERVAL_SECS`; each host is geolocated from its public IP, which is not served.

```json
[{
  "name": "RandProtocol prover pool", "url": "https://prover.randprotocol.org",
  "operator": "Rand Protocol validators", "fingerprint": "RGTF-7HKJ-XZFV-GQ1J",
  "pairing_url": "https://prover.randprotocol.org/.well-known/rand-prover.json",
  "up": true, "fingerprint_matches": true, "error": null,
  "checked_at_ms": 1790825354832, "last_up_ms": 1790825354832,
  "info": { "version": "0.6.7", "kem_fingerprint": "RGTF-7HKJ-XZFV-GQ1J", "hc_bundles": ["60af…"],
            "backend": "cpu", "witness_kinds": ["viewing_key"],
            "queue": { "depth": 0, "max": 5, "proving": 0 }, "fee": null },
  "members": [{ "label": "rand-node-a", "geo": { "lat": 1.29, "lon": 103.85, "city": "Singapore", … } }]
}]
```

`up` is whether the endpoint answered on the last poll; a prover that stops answering keeps its
last `info` and `last_up_ms`, with the poll's `error`. Behind a pool `queue` is the router's sum
over the members that answered its own last poll (`max` is the job slots, one per member there).
`fee` is `null` for a free prover, else `{ amount, address }` with RAND base units.

### Validators

| method and path | returns |
|---|---|
| `GET /validators` | the register: array of `Validator`, in address order |
| `GET /validators/:address` | `Validator` plus `recent_blocks` (last 10 `BlockSummary`) |
| `GET /validators/admitted` | audit v6: `{ admission_by_vote, max, admitted: [address…] }`, the keys voted in by `admit_validator` that have not registered yet (read from the node) |

```json
{
  "address": "2nRdFChBXRmKoe2sQE3ZYDzvdg53QmBZJJ9iweY7hk1v",
  "stake": "100000000000000", "rewards": "4000000",
  "pending": [{ "release_epoch": 41, "amount": "5000000000" }],
  "payout": "rand1…", "nonce": 3, "active": true,
  "share_percent": 25.0, "blocks_proposed": 5372,
  "last_proposed_height": 20151, "last_proposed_timestamp_ms": 1789017639030,
  "sort_index": 0
}
```

The register is the one place the chain stores amounts in the clear. `stake` is in RAND units,
`rewards` the bundle fees credited to the validator as proposer and not yet withdrawn, `pending`
the unbonding queue (oldest first), `payout` the shielded address a withdraw pays to, `nonce`
what its next signed unbond or withdraw must carry, and `active` whether it is in the set
running the current epoch. The leader of view `v` is the active validator at index
`v mod active_validator_count` in address order. `share_percent` is over the active set's stake
(0 for an inactive entry). On a node before phase S2 `pending` is empty, `payout` null and every
entry active.

### Programs

| method and path | returns |
|---|---|
| `GET /programs?page&limit` | paginated `ProgramSummary` |
| `GET /programs/:id` | `ProgramSummary` plus `recent_calls` (last 10 `TransactionSummary`, calls and invokes) and `program_state` |
| `GET /programs/:id/cells?after&limit` | a page of the program's state cells in key order, live from the node (RPL-2); 404 on a chain without a `program_state` section |

```json
{
  "id": "675adeea7e4242d8dc48bf56faedb7bea14a4f832d7c8a973f942fa7dd850065",
  "deploy_tx": "dc97d2696981f4e49d822dee06143725444752e13fc42abdce60b72056f7342e",
  "deployed_at_height": 10,
  "base_pc": 0, "words_len": 42,
  "code_hash": "675adeea7e4242d8dc48bf56faedb7bea14a4f832d7c8a973f942fa7dd850065",
  "public_words_len": 0, "public_digest": null,
  "call_count": 3, "last_called_height": 105,
  "invoke_count": 1, "last_invoked_height": 120,
  "recent_calls": [ … ],
  "program_state": {
    "vault": [{ "asset": 0, "amount": "300000000" }, { "asset": 2, "amount": "500" }],
    "cells": [{ "key": "0100…00", "value": "0500…00" }],
    "cells_next": null
  }
}
```

`invoke_count`, `last_invoked_height` and `program_state` are RPL-2 (fullnode v0.6.8, a
genesis-gated `program_state` section; on no chain as of 2026-10-01). `program_state` is read
live from the node when the page is served — a program's state is public chain state that moves
with every invoke: `vault` is what the program holds per asset (registry index, 0 is RAND; units
of that asset; a row at zero does not exist), `cells` the first 100 of its cells in key order
(each a 64-hex `Word8` key and value; a cell written to zeros is gone), and `cells_next` the last
key served when more follow — pass it as `after` to `GET /programs/:id/cells` (`limit` 1 to
1000, default 100), which answers `{ "cells": [...], "next": "<key>" | null }`. `program_state`
is `null` on a chain without the section and on a node predating it.

Programs are content addressed and immutable; `id` never changes. There is no deployer: a deploy
is paid by a shielded bundle, so the chain does not know who deployed it.

A program's public input is fixed at deploy and bound into its `id`: `public_words_len` is its
length in words (`0` without one) and `public_digest` its digest, `null` without one. Every call
to the program is proved over those words, and each receipt repeats the digest as `h_pub`. The
words themselves are not stored here; the node serves them (`rand_getProgramPublic`).

### Search

`GET /search?q=` returns an array of matches across blocks, transactions, programs, notes,
nullifiers and validators. A decimal query is treated as a height, a 64-hex query as a hash,
program id, commitment or nullifier, and a base58 query as a validator address.

```json
[{ "type": "nullifier", "id": "8c04…d1", "title": "Nullifier", "subtitle": "note spent at #41", "url": "/transactions/4f2c…e7" }]
```

## Authentication and API keys

1. Sign up at https://randscan.org/signup (email and password; no verification email).
2. On https://randscan.org/dashboard create a key. It looks like `rsk_` followed by 48 letters and
   digits and is shown once. Store it like a password; revoke it from the dashboard if it leaks.
3. Send it on every request, either way:

   ```bash
   curl -H "Authorization: Bearer rsk_..." https://randscan.org/api/v1/stats
   curl -H "X-API-Key: rsk_..." https://randscan.org/api/v1/stats
   ```

A request that carries an unknown or revoked key is refused with 401 `invalid_api_key`; it is not
downgraded to anonymous, so a typo is caught immediately. Keys cannot manage keys or read your
account: those endpoints accept only the browser session.

Each account may hold 10 active keys. The dashboard shows when a key was last used and how many
requests it has made.

### Quotas

| identity | limit |
|---|---|
| anonymous, per IP | 60 requests per minute |
| API key | 600 requests per minute |
| sign-in and sign-up, per IP | 10 attempts per minute |

Limits are fixed 60-second windows. Every response includes `X-RateLimit-Limit` and
`X-RateLimit-Remaining`. Over the limit you get 429 with a `Retry-After` header and body
`{"error":"rate_limited",...}`; wait that many seconds and retry. Need more? Open an issue on the
repository with your use case.

### Account endpoints

These are what the website uses; you can drive them from scripts too, but keys are the intended
way for machines.

| method and path | body | result |
|---|---|---|
| `POST /auth/signup` | `{ "email", "password" }` | 201 `{ "user" }`, sets cookie `randscan_session` |
| `POST /auth/login` | `{ "email", "password" }` | 200 `{ "user" }`, sets cookie |
| `POST /auth/logout` | | 204 |
| `GET /auth/me` | | 200 `{ "user" }` or 401 |
| `POST /auth/forgot` | `{ "email" }` | 202 always (503 `email_disabled` if the explorer has no mail provider) |
| `POST /auth/reset` | `{ "token", "password" }` | 200 `{ "user" }`, sets cookie; 400 `invalid_token` / `weak_password` |
| `POST /auth/password` | `{ "current_password", "new_password" }` | 204; signs out every other session |
| `GET /keys` | | `ApiKey[]` (never includes the secret) |
| `POST /keys` | `{ "name" }` | 201 `ApiKey` plus `key`, once |
| `DELETE /keys/:id` | | 204 |

Password reset: `/auth/forgot` emails a single-use link to `/reset?token=…` that is valid for one
hour and answers 202 whether or not the address is registered. `/auth/reset` logs the user out
everywhere and signs them in with a fresh cookie. Both share the sign-in rate limit.

`user`: `{ id, email, created_at, last_login_at }`. `ApiKey`: `{ id, name, prefix, created_at,
last_used_at, request_count, revoked_at }`. Passwords are 10 to 128 characters.

## WebSocket feed

Connect to `wss://randscan.org/ws` and send JSON messages. Channels: `blocks`, `transactions`,
`stats`.

Client to server:

```json
{ "type": "subscribe", "channel": "blocks" }
{ "type": "unsubscribe", "channel": "blocks" }
{ "type": "ping" }
```

Server to client:

```json
{ "type": "subscribed", "channel": "blocks", "subscription_id": "…" }
{ "type": "unsubscribed", "channel": "blocks" }
{ "type": "pong" }
{ "type": "error", "message": "bad message: …" }
{ "type": "new_block", "block": BlockSummary }
{ "type": "new_transaction", "transaction": TransactionSummary }
{ "type": "stats_update", "stats": NetworkStats }
```

Node.js example that prints every faucet mint and staking move as it commits (the only kinds
with a public amount):

```js
const ws = new WebSocket("wss://randscan.org/ws");
ws.onopen = () => ws.send(JSON.stringify({ type: "subscribe", channel: "transactions" }));
ws.onmessage = (ev) => {
  const msg = JSON.parse(ev.data);
  if (msg.type === "new_transaction" && msg.transaction.amount !== null) {
    const t = msg.transaction;
    console.log(t.height, t.kind, t.validator ?? "", Number(t.amount) / 1e9, "RAND");
  }
};
```

Send a `ping` every 30 seconds to keep idle connections open through proxies. If the socket
drops, reconnect and backfill with `GET /transactions?height=` for any heights you missed; the
feed has no replay.

## Rate limits and reliability

- Rate limits are enforced per IP and per API key; see [Quotas](#quotas) above. Abusive traffic
  will also be blocked at the proxy.
- The explorer runs next to a full node. `GET /health` reports `indexer.lag`; during a resync the
  lists are still served from the database, so a nonzero lag means the newest blocks are missing,
  not that older data is wrong.
- Retry 5xx responses with exponential backoff. 4xx responses will not succeed on retry.

## Running your own

Everything in this document is served by the open-source explorer in this repository. To run it
against your own `rand-node`, see the README. The indexer needs only the node's JSON-RPC
endpoint (`rand_*` methods) and a PostgreSQL database.
