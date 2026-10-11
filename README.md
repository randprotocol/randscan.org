# RandScan

[![coverage](https://codecov.io/gh/randprotocol/randscan.org/graph/badge.svg)](https://codecov.io/gh/randprotocol/randscan.org)

Line coverage of the Rust workspace, measured by the `coverage` job in `.github/workflows/ci.yml` (cargo-llvm-cov) on every push to `main` and published to Codecov.

Block explorer for the Rand Protocol RAND chain (the network served by
[`rand-node`](../fullnode)). Live at https://randscan.org.

- **Backend**: Rust — axum REST API + WebSocket, SQLx/PostgreSQL, and an in-process indexer that
  follows a `rand-node` JSON-RPC endpoint (`rand_*` methods).
- **Frontend**: Next.js 14, TypeScript, Tailwind, SWR; Leaflet for the nodes map.
- **Database**: PostgreSQL 16.

## What it indexes

RAND is a fully shielded chain (fullnode shielded-pool phases S1–S3): there are no accounts,
and every transaction is a shielded bundle (anchor, two nullifiers, two commitments, fee, burn,
proof) plus an optional public action. The explorer indexes committed blocks (hash, height,
HotStuff view, proposer, roots, `justify_view`), every transaction's public bundle fields and its
action (`transfer`, `mint`, `deploy`, `call`, `bond`, `unbond`, `withdraw`, `bridge_attest`,
`bridge_burn`), confidential-call receipts (tier, outputs, `h_in`), the commitment tree leaf by
leaf and the nullifier set, the validator register (stake, rewards, unbonding queue, active
set), deployed programs, the supply audit and bridge state, network stats, and the node's libp2p
peers with geolocated IPs. Balances and a transfer's parties are visible only to a viewing key,
which the explorer does not hold. Design: `docs/superpowers/specs/2026-09-12-shielded-chain-design.md`.

## Run locally

Requirements: Rust 1.75+, Node 20+, PostgreSQL, and a reachable `rand-node` RPC
(e.g. `ssh -N -L 8545:127.0.0.1:8545 root@<node>` to tunnel a remote node).

```bash
createdb randscan
cp .env.example .env            # DATABASE_URL, RPC_URL, API_PORT ...
cargo run --release --bin randscan-api          # API + indexer + WebSocket on :3000

cd frontend
npm ci
NEXT_PUBLIC_API_URL=http://localhost:3000 NEXT_PUBLIC_WS_URL=ws://localhost:3000/ws npm run dev   # :3001
```

Set `COOKIE_SECURE=false` in `.env` for local http so sign-in works.

The schema (`migrations/001_initial_schema.sql`) is created automatically on first start. The
indexer catches up from height 0, then polls `rand_getHead` every `POLL_INTERVAL_MS`.

### Integration tests

`crates/randscan-api/tests/mock_node.rs` runs the indexer, API and broadcast against a scripted
shielded node (every action kind, the tree, the register, two hard forks); `tests/real_node.rs`
spawns a real shielded `rand-node` plus the `rand` wallet (faucet, a proved transfer, a
deploy and a confidential call) and compares every public number. Both need `DATABASE_URL`;
the real-node test also needs `RAND_NODE_BIN` (a build of the fullnode's `shielded-s3` branch
or later, with `rand` beside it or `RAND_CLI` set).

### Integration test against a real node

`tests/real_node.rs` is also the contract test for the two sites that read this API with no tests
of their own: zusd.money's `src/lib/balance-sheet.mjs` (rendered by `BalanceSheet.astro`) and randprotocol.org's `BridgeReserves.astro`
(`/bridge` → `enabled`, `mint_paused`; `/bridge/assets` → `index`, `chain`, `symbol`, `locked`;
`/tokens/{index}` → `total_supply`) and randprotocol.org's `balance.js`
(`/envelopes?from_leaf=&limit=` → `notes`, `total_leaves`, `next_leaf`, and per leaf `leaf_index`,
`cm`, `height`, `tx_hash`, `envelope`, `public`). On a chain without a bridge it pins what those
pages must handle (`enabled: false`, `[]`, a 404 for an unknown token); it also checks that
`/stats` carries `limits` and `gas_prices`, that every `/supply` amount is the node's decimal
string, and that the node's newer fields (`rand_getLimits.fee_rules`,
`rand_getSupply.base_fees_burned`) pass through when served and are not required when not.

```bash
gh release download --repo randprotocol/fullnode --pattern rand-node --pattern rand --dir /tmp/fullnode
chmod +x /tmp/fullnode/rand-node /tmp/fullnode/rand
DATABASE_URL=postgres://randscan:randscan@localhost:5432/randscan_it \
RAND_NODE_BIN=/tmp/fullnode/rand-node RAND_CLI=/tmp/fullnode/rand \
  cargo test -p randscan-api --test real_node -- --nocapture
```

| Variable | Meaning |
|---|---|
| `DATABASE_URL` | a PostgreSQL database the test may re-index (use a scratch one: the chain tables are truncated on a chain switch) |
| `RAND_NODE_BIN` | the `rand-node` binary to spawn |
| `RAND_CLI` | the `rand` wallet; defaults to `rand` beside `RAND_NODE_BIN` |
| `RAND_BLOCK_INTERVAL_MS` | block interval, default `500`; raise to `1000`–`1500` on a slow prover |
| `RANDSCAN_REQUIRE_REAL_NODE` | fail instead of skipping when either of the first two is unset |

The wallet proves locally under the chain's `test` FRI profile; a run takes a few minutes. A
check the given build has no flag for (an RPL-2 invoke on a pre-v0.6.8 node, `fee_rules` on a
node before fee feedback) is skipped with a printed reason. CI's `real-node` job runs this against
the latest fullnode release on every push and pull request, and gates on it.

### Environment

| Variable | Default | Description |
|---|---|---|
| `DATABASE_URL` | `postgres://randscan:randscan@localhost:5432/randscan` | PostgreSQL connection |
| `RPC_URL` | `http://127.0.0.1:8545` | `rand-node` JSON-RPC endpoint |
| `API_HOST` / `API_PORT` | `0.0.0.0` / `3000` | API bind address |
| `POLL_INTERVAL_MS` | `1000` | head polling interval when caught up |
| `BATCH_SIZE` | `200` | blocks per pass while catching up |
| `STATS_INTERVAL_SECS` | `5` | minimum interval between stats refreshes |
| `NODES_INTERVAL_SECS` | `60` | peer list / geolocation refresh interval |
| `NODE_PUBLIC_IP` | auto-detected | public IP of the node the explorer runs on |
| `RUST_LOG` | `info` | log filter |
| `COOKIE_SECURE` | `true` | set the session cookie `Secure` (requires https; use `false` for local http) |
| `TRUST_PROXY` | `false` | read the client IP from `X-Forwarded-For` (true behind Caddy/nginx) |
| `PUBLIC_URL` | `https://randscan.org` | site origin used in password-reset links |
| `MAIL_FROM` | `RandScan <no-reply@randscan.org>` | sender of password-reset email (domain must be verified in Resend) |
| `RESEND_API_KEY` | unset | Resend API key; unset disables password reset by email |
| `ANON_RATE_LIMIT_RPM` | `60` | requests per minute per IP for anonymous traffic |
| `KEY_RATE_LIMIT_RPM` | `600` | requests per minute per API key |
| `AUTH_RATE_LIMIT_RPM` | `10` | sign-in/sign-up attempts per minute per IP |
| `GEOIP_DB` (frontend) | unset | DB-IP Country Lite `.mmdb` for the first-visit language redirect; unset means English |

### Languages

The site is served in twenty languages: English at `/…`, and `ru zh zh-hk ko id ms ja ar fa es pt de
fr it pl hi ur ps ta` at `/{code}/…` with the same path (Arabic, Persian, Urdu and Pashto right to left) (design: `docs/superpowers/specs/2026-10-02-explorer-i18n-design.md`).
A first visit to `/` goes to the language of the visitor's country, looked up in process in the
offline database `GEOIP_DB`; a language chosen from the globe menu is kept in the `lang` cookie
and wins from then on. Strings live in `frontend/src/i18n/messages/{code}.json`, English being
the source. After changing English text, run `cd frontend && node scripts/i18n/translate.mjs`
(it re-translates only what changed, with `claude -p`) and `npm test` (the parity test).
`npm run lint` fails on hard-coded English in `src/app` and `src/components`.
`frontend/scripts/i18n/fetch-geoip.sh` fetches the database for local runs.

## API

REST under `/api/v1` (`health`, `stats`, `supply`, `bridge`, `blocks`, `blocks/latest`,
`blocks/:id`, `transactions`, `transactions/latest`, `transactions/:hash`, `notes`, `notes/:id`,
`nullifiers/:nf`, `transactions/:hash/envelopes`, `envelopes`, `validators`, `validators/:address`,
`programs`, `programs/:id`, `nodes`,
`search?q=`; `accounts/*` answers 410) and a WebSocket at `/ws` (channels `blocks`,
`transactions`, `stats`). Amounts are strings of units (1 RAND = 10^9 units); timestamps are
`timestamp_ms`. Full shapes in `docs/superpowers/specs/2026-09-12-shielded-chain-design.md`; a
guide for integrators with examples in [docs/api.md](docs/api.md). Two documents from the
account chain remain for history: how the fullnode review fixes shaped confidential calls
([docs/confidential-transactions-after-review-fixes.md](docs/confidential-transactions-after-review-fixes.md))
and the guardian bridge end to end with a diagram ([docs/bridge-architecture.md](docs/bridge-architecture.md));
their account-side details (balances, recipients, effects) no longer apply.

Accounts and API keys: sign up at `/signup`, create keys at `/dashboard`; keyed requests use
`Authorization: Bearer rsk_...`. Details and quotas in [docs/api.md](docs/api.md).

### Password reset

Users reset their own password from `/forgot`: the API emails a single-use link (valid one hour)
through [Resend](https://resend.com) when `RESEND_API_KEY` is set, and signed-in users can change
their password from `/dashboard`. On a node deployed with `deploy/vps-setup.sh`, put the key in
`/etc/randscan/api.secrets.env` (created empty by the script, never overwritten) and restart
`randscan-api`. Without a key the site reports that reset by email is not enabled and the
runbook below is the fallback.

### Operator runbook: reset a password

```bash
read -rs PW && printf '%s' "$PW" | randscan-api hash-password      # prints $argon2id$...
psql "$DATABASE_URL" -c "UPDATE users SET password_hash = '<paste>' WHERE email = 'user@example.com';" \
                     -c "DELETE FROM sessions WHERE user_id = (SELECT id FROM users WHERE email = 'user@example.com');"
```

`read -rs` prompts for the new password without echoing it, and `printf` (rather than `echo`)
avoids appending a trailing newline — together this keeps the password out of shell history and
process listings.

### Chain switch (testnet hard fork)

The indexer records the chain id of the data it holds (`indexer_state.chain_id`). When the node it
follows serves a different chain id, or a different genesis block under the same id, the indexer
logs a warning, truncates only the chain-derived tables (blocks, transactions, nullifiers, notes,
receipts, programs, validators) and re-indexes from height 0 and tree leaf 0. Users, sessions,
API keys, password resets and the peer geolocation cache are kept. Nothing to do by hand: restart
(or re-point) the node and, if you want it picked up immediately rather than within ten seconds,
`systemctl restart randscan-api`. Do **not** drop the database; that would delete user accounts
and API keys.

The switch to the shielded chain is also a schema change: migration `005_shielded_chain.sql`
drops and recreates the chain tables (users and keys untouched) the first time this build
starts. This build reads only the shielded node's RPC (bundles, actions, `rand_getCommitments`,
the register); pointed at an account-chain node it will not index. Deploy it together with the
shielded node.

Transaction kinds the node serves that this build does not decode are indexed as kind `other`
(hash, bundle, fee and block only) so a newer node never stalls the explorer.

## Deploy on a node

The explorer runs on the same machine as a synced `rand-node` (its RPC is bound to localhost):

```bash
deploy/push-to-vps.sh <ip> [domain]     # rsync, build, Postgres + Node 20 + Caddy, systemd units
```

`deploy/vps-setup.sh` installs `randscan-api` (:3000) and `randscan-frontend` (:3001) as systemd
services and a Caddyfile that serves `<domain>` with automatic HTTPS (`/api/*` and `/ws` to the
API, everything else to Next.js; `www.` and `randscan.com` redirect). Point the domain's A records
at the server before running it so Caddy can obtain certificates.

## Docker

`docker compose up --build` runs Postgres, the API and the frontend (set `RPC_URL` to a reachable
node).

## Project structure

```
crates/randscan-core      wire types, helpers (amount formatting, query classification)
crates/randscan-db        schema, row models, queries
crates/randscan-indexer   RPC client, block processor, sync service, peer tracker, broadcaster
crates/randscan-api       axum routes/handlers, binary randscan-api
crates/randscan-ws        WebSocket manager/handler
crates/randscan-viewing    envelope opener compiled to WebAssembly (viewing keys in the browser); built on its own, see its README
frontend/                 Next.js app
migrations/               SQL schema
deploy/                   VPS install script, systemd units, Caddyfile
```

## License

MIT
