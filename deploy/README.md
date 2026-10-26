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
3. The unit's limits are installed: `MemoryMax=1G`, `MemorySwapMax=0`, `OOMScoreAdjust=1000`. The
   unit sets `PROVE=--prover`, so every proving command passes `--prover` (proof on the pool, not on
   the host). `PROVE=` (empty) would prove locally and needs far more than 1G, so do not set it
   under these limits.

Then: `systemctl daemon-reload && systemctl enable --now randscan-activity.timer`.
