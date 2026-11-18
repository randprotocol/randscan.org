# randscan deploy

`push-to-vps.sh <ip> [domain]` rsyncs the repo, builds, and runs `vps-setup.sh`, which installs
Postgres, Node 20, Caddy and the `randscan-api` / `randscan-frontend` units (see the top-level
README, "Deploy on a node").

## randscan-activity (the chain-activity timer)

`randscan-activity.timer` fires `randscan-activity.service` every 5 minutes; it runs
`chain-activity.sh`, which mints from the faucet and sends one proved transaction (transfer,
deploy or call in rotation) and checks that randscan indexed it.

**Status: the timer is DISABLED on host e since 2026-10-27.** The old unit proved every bundle
locally with `rand` 0.6.7 on a host with about 4 GB free: 1,176 OOM kills, and once it killed
validator e. Do not re-enable it until all of these hold:

1. A current `rand` (v0.8.0 or later) is installed in `BIN` (the script warns when `BIN/rand` is
   older than the running node).
2. Each wallet key (`W1`, `W2`) is paired with the prover pool: `rand prover pair --trusted --key
   <wallet>`.
3. The unit's limits are installed: `MemoryMax=1G`, `MemorySwapMax=0`, `OOMScoreAdjust=1000`.
4. Know which steps use the pool. The unit sets `PROVE=--prover`, which `send` (transfer) and
   `program deploy` pass, so those two prove on the pool. `rand call` ALWAYS proves on the host
   (`--prover` does not move it; about 23 GB at tier 16), so the default cycle is `transfer deploy`
   only. `ACTIVITY_CALL=1` adds the call step: it needs at least 8 GB free (far more at tier 16)
   and must never be set on a validator host; under the 1G cap it would be OOM-killed. A bare
   `chain-activity.sh call` without it is refused. `PROVE=` (empty) would prove locally too.

Then: `systemctl daemon-reload && systemctl enable --now randscan-activity.timer`.

## SCAN-2: the history page never sends a nullifier (randprotocol/monorepo#16)

Since 66bc1a7 the history page downloads the public nullifier set (`GET /api/v1/nullifiers`, pages
of 5,000) and matches the wallet's own nullifiers in the browser; `POST /api/v1/nullifiers/lookup`
is removed and the frontend has no single-nullifier request (`frontend/tests/spent.test.mjs` fails
the build if one comes back). Two scripts make the owner's remaining steps checkable:

1. **After a deploy**, from any machine (read-only, three kinds of HTTP request):

       deploy/scan2-verify.sh --record https://randscan.org

   It passes only when the lookup route answers 404/405, the paged set is served, and no
   JavaScript chunk the served history page references contains a nullifier lookup. Paste the
   `SCAN-2 deploy check: … passed …` line into the issue as the deploy record.
2. **On the explorer host**, as root: `deploy/scan2-log-audit.sh` (a dry run: counts per source,
   never prints a matching line), then `deploy/scan2-log-audit.sh --purge --before <deploy day>`
   if it found any. `--purge` drops the whole journal older than that day (journald cannot delete
   single lines) and the matching lines of the plain log files. The units ship with
   `tower_http=warn` and the Caddyfile has no `log` directive, so a host that always ran this
   configuration should report zero; the script is how that is shown rather than assumed.

Both are tested offline: `bash deploy/tests/scan2-verify.test.sh`, `bash deploy/tests/scan2-log-audit.test.sh`.
