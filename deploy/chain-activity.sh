#!/usr/bin/env bash
# One unit of activity on the shielded testnet, then a check that randscan.org indexed it.
# Every run: a faucet mint (MINT_RAND, default 10) into wallet 1, ONE proved step from the
# rotation below, and a supply sanity check (`rand_getSupply` recomputed here, compared with
# the explorer's copy). One proved step per run because a proof takes ~5 min on E's four cores
# (transfer/deploy/call each land every third run, ~16 min apart at the 5-minute timer).
# The rotation kept in STATE_DIR:
#   transfer  a proved 1.5 RAND send from wallet 1 to wallet 2 (faucet-mints first when
#             wallet 1 is running low, so the loop never starves)
#   deploy    a fresh private_payment guest (a new threshold each time gives a new program id)
#   call      (only with ACTIVITY_CALL=1; proves locally) a proved call of the last program this loop deployed (input transcript published)
# Meant to be fired every few minutes by a scheduler; a lock makes overlapping runs a no-op
# (two proofs against the same wallet would race for the same notes).
#
# Usage: deploy/chain-activity.sh [transfer|deploy|call]   (no argument = next in rotation)
# Env:   BIN (a directory holding `rand`; an optional BIN/.git-rev names the commit a host-tuned copy was built from), WALLETS (or W1/W2 key files), RPC, RPC_FALLBACK, EXPLORER, STATE_DIR, ROTATION, NICE (e.g. "nice -n 10" when the
#        prover shares a box with a validator; proofs are ~1.3 MB at constraint set 5 and take
#        1.5–4 min, so run this where the node is local: a 2.6 MB hex submit over a slow uplink
#        outruns the wallet's 15 s RPC timeout).
set -uo pipefail
BIN=${BIN:-${HOME:-/root}/Github/randprotocol/fullnode/bin-03c9fb9}
WALLETS=${WALLETS:-${HOME:-/root}/Github/randprotocol/fullnode/wallets}
RPC=${RPC:-http://127.0.0.1:8545}
EXPLORER=${EXPLORER:-https://randscan.org/api/v1}
STATE_DIR=${STATE_DIR:-${TMPDIR:-/tmp}/chain-activity}
NICE=${NICE:-}
# How proofs are made. Default: on the paired prover pool (`rand prover pair --trusted` once per
# wallet key, rand v0.8.0+), so the host never proves locally. PROVE="" proves on this machine.
PROVE=${PROVE---prover}
MINT_RAND=${MINT_RAND:-10}
mkdir -p "$STATE_DIR"
W1=${W1:-$WALLETS/shielded-1.key.json}
W2=${W2:-$WALLETS/shielded-2.key.json}
LOG=$STATE_DIR/activity.log

log() { printf '%s %s\n' "$(date -u +%FT%TZ)" "$*" | tee -a "$LOG"; }
rpc() { curl -s --max-time 10 "$RPC" -H 'content-type: application/json' -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$1\",\"params\":$2}"; }
submitted() { grep "submitted $2 " <<<"$1" | awk '{print $3}' | head -1; }
# Poll the explorer until it serves the transaction with the expected kind.
explorer_tx() {
  local hash=$1 want=$2 i body
  for i in $(seq 1 30); do
    body=$(curl -s --max-time 10 "$EXPLORER/transactions/$hash")
    if grep -q "\"kind\":\"$want\"" <<<"$body"; then echo "$body"; return 0; fi
    sleep 3
  done
  echo "$body"; return 1
}

# The supply sanity check (RAND on chain): the node's audit, recomputed here from its own
# counters — total = pool + register, and total = issued − slashed where issued = genesis
# deposits + genesis stake + faucet mints + subsidies — then the explorer's copy of the audit
# must agree. A FAIL line means a consensus bug or a damaged counter, never a normal state.
field() { grep -o "\"$2\":\"\?[0-9a-z]*" <<<"$1" | head -1 | sed 's/.*://; s/"//g'; }
supply_check() {
  local s ex total pool reg gd gs fm sub sl issued
  s=$(rpc rand_getSupply '[]')
  [ -n "$s" ] || { log "SUPPLY FAIL: rand_getSupply answered nothing"; return 1; }
  total=$(field "$s" total_supply); pool=$(field "$s" pool_value); reg=$(field "$s" register_total)
  gd=$(field "$s" genesis_deposited); gs=$(field "$s" genesis_staked); fm=$(field "$s" faucet_minted)
  sub=$(field "$s" subsidised); sl=$(field "$s" slashed)
  issued=$(( gd + gs + fm + sub ))
  local ok=1
  [ "$(field "$s" invariant_holds)" = true ] || ok=0
  [ $(( pool + reg )) -eq "$total" ] || ok=0
  [ $(( issued - sl )) -eq "$total" ] || ok=0
  ex=$(curl -s --max-time 10 "$EXPLORER/supply")
  local ex_total; ex_total=$(field "$ex" total_supply)
  local agree=agrees; [ "$ex_total" = "$total" ] || agree="DISAGREES ($ex_total)"
  if [ $ok = 1 ]; then
    log "supply ok: total $total = pool $pool + register $reg = issued $issued − slashed $sl (deposited $gd, staked $gs, minted $fm, subsidised $sub); explorer $agree"
  else
    log "SUPPLY FAIL: total $total, pool $pool + register $reg = $(( pool + reg )), issued $issued − slashed $sl = $(( issued - sl )), invariant_holds $(field "$s" invariant_holds); explorer $agree"
    return 1
  fi
}

if ! mkdir "$STATE_DIR/lock" 2>/dev/null; then
  log "skip: a previous step is still running (lock $STATE_DIR/lock)"; exit 0
fi
trap 'rmdir "$STATE_DIR/lock"' EXIT

# A node that is catching up serves a stale anchor; fall back to RPC_FALLBACK (say, an SSH
# tunnel to a synced droplet: `ssh -f -N -L 18545:127.0.0.1:8545 root@<node>`) when the
# primary is down or syncing.
RPC_FALLBACK=${RPC_FALLBACK:-http://127.0.0.1:18545}
status=$(rpc rand_status '[]')
if [ -z "$status" ] || grep -q '"syncing":true' <<<"$status"; then
  alt=$(RPC=$RPC_FALLBACK rpc rand_status '[]')
  if [ -n "$alt" ] && ! grep -q '"syncing":true' <<<"$alt"; then
    log "primary node $RPC is $([ -z "$status" ] && echo down || echo syncing); using $RPC_FALLBACK"
    RPC=$RPC_FALLBACK
  fi
fi
chain=$(rpc rand_chainId '[]' | grep -o '"result":[0-9]*' | cut -d: -f2)
head=$(rpc rand_getHead '[]' | grep -o '"height":[0-9]*' | cut -d: -f2)
[ -n "$chain" ] || { log "FAIL: no node at $RPC"; exit 1; }

# A chain cut. A wallet's note store names leaves of the tree it was scanned from; on a new chain
# those leaves do not exist and every proved step fails ("no leaf at index N"), and the last
# deployed program is gone with the old chain too. So the chain the stores belong to is kept in
# STATE_DIR, and when the node serves another one the stores and the program id are moved aside
# (never deleted); the wallet rescans from leaf 0. Left in place, a stale store fails fast only
# while the new tree is shorter than it: past that, its leaf indices name other people's notes,
# and each step proves for minutes before the wallet refuses the proof's digest. The genesis hash
# tells two cuts with one chain id apart; a node without the method (before v0.3) is known by its
# chain id alone.
genesis=$(rpc rand_getGenesisHash '[]' | grep -o '"result":"[0-9a-f]*"' | cut -d'"' -f4)
identity="$chain:${genesis:-unknown}"
known=$(cat "$STATE_DIR/chain" 2>/dev/null || true)
if [ -n "$known" ] && [ "$known" != "$identity" ]; then
  for store in "$W1.notes.json" "$W2.notes.json" "$STATE_DIR/program"; do
    [ -f "$store" ] || continue
    aside="$store.chain${known%%:*}-stale"
    i=1; while [ -e "$aside" ]; do i=$((i+1)); aside="$store.chain${known%%:*}-stale-$i"; done
    mv "$store" "$aside"
    log "chain cut $known -> $identity: moved $(basename "$store") to $(basename "$aside")"
  done
fi
echo "$identity" > "$STATE_DIR/chain"

# A wallet built apart from the node. On a slow box BIN may hold a `rand` rebuilt for the host's
# CPU (the fleet builds for baseline x86-64; the prover's AVX2 field code only compiles in with
# RUSTFLAGS="-C target-cpu=x86-64-v3", and on node E that is 185 s a bundle proof instead of 309 s
# — the difference between landing inside the 256-block time window at ~1.15 s blocks and
# missing it; on v0.6.7, where a transfer is a bundle proof and an auth proof, 193 blocks
# against more than 300). Such a copy does not follow the fleet's updates, so it records the
# build it was made from in BIN/.git-rev — the `git_sha` exactly as `rand_getVersion` reports
# it, short on a release binary — and a node on another build gets a loud line here rather than
# proofs of a wire format the chain no longer takes. That line was logged every five minutes
# from chain 16 to chain 18 while every proved step failed: after a fleet update, read this log.
built=$(cat "$BIN/.git-rev" 2>/dev/null || true)
if [ -n "$built" ]; then
  node_sha=$(rpc rand_getVersion '[]' | grep -o '"git_sha":"[0-9a-f]*' | cut -d'"' -f4)
  if [ -n "$node_sha" ] && [ "$node_sha" != "$built" ]; then
    log "WARN: $BIN/rand was built from ${built:0:7} but the node runs ${node_sha:0:7}; rebuild it (see $BIN/README)"
  fi
fi

# Since v0.5.10 `rand send` asks before it sends, and refuses a stdin that is not a terminal
# unless told `--yes`; an older wallet does not know the flag.
yes=; "$BIN/rand" send --help 2>&1 | grep -q -- '--yes' && yes=--yes

n=$(cat "$STATE_DIR/n" 2>/dev/null || echo 0); n=$((n+1)); echo "$n" > "$STATE_DIR/n"
# ROTATION can drop a step, e.g. "transfer deploy" while a node build refuses calls.
# `rand call` always proves ON THIS MACHINE (--prover does not move it), a ~23 GB peak at tier 16.
# So the default cycle leaves it out; ACTIVITY_CALL=1 adds it. Never set that on a validator host.
read -r -a rotation <<<"${ROTATION:-transfer deploy${ACTIVITY_CALL:+ call}}"
step=${1:-${rotation[$(( (n-1) % ${#rotation[@]} ))]}}
log "step $n $step (chain $chain, head $head)"
t0=$(date +%s)

# The mint, every run: MINT_RAND into wallet 1 (the faucet caps a call at 100 RAND).
out=$("$BIN/rand" faucet --key "$W1" --rpc "$RPC" --amount "$MINT_RAND" 2>&1); h=$(submitted "$out" mint)
if [ -n "$h" ] && explorer_tx "$h" mint >/dev/null; then log "step $n mint ok $h ($MINT_RAND RAND to wallet 1)"; else log "step $n mint FAIL ${h:-nohash}: $(tail -1 <<<"$out")"; fi

case $step in
  transfer)
    bal=$("$BIN/rand" balance --key "$W1" --rpc "$RPC" 2>/dev/null | grep -o 'balance: [0-9.]*' | awk '{print $2}')
    if [ -n "$bal" ] && [ "${bal%%.*}" -lt 20 ]; then
      out=$("$BIN/rand" faucet --key "$W1" --rpc "$RPC" --amount 100 2>&1); h=$(submitted "$out" mint)
      if [ -n "$h" ] && explorer_tx "$h" mint >/dev/null; then log "step $n mint ok $h (wallet 1 had $bal RAND)"; else log "step $n mint FAIL ${h:-nohash}: $(tail -1 <<<"$out")"; fi
    fi
    to=$("$BIN/rand" --key "$W2" address | tail -1)
    out=$($NICE "$BIN/rand" send "$to" 1.5 $yes $PROVE --key "$W1" --rpc "$RPC" 2>&1); h=$(submitted "$out" transfer)
    if [ -n "$h" ] && body=$(explorer_tx "$h" transfer); then
      log "step $n transfer ok $h block $(grep -o '"height":[0-9]*' <<<"$body" | head -1 | cut -d: -f2) ($(( $(date +%s) - t0 )) s)"
    else log "step $n transfer FAIL ${h:-nohash}: $(tail -1 <<<"$out")"; exit 1; fi ;;
  deploy)
    pj="$STATE_DIR/pp-$n.json"
    "$BIN/rand" program build --guest private_payment --arg $((100 + n)) --out "$pj" >/dev/null 2>&1
    out=$($NICE "$BIN/rand" program deploy "$pj" $PROVE --key "$W1" --rpc "$RPC" 2>&1); h=$(submitted "$out" deploy)
    pid=$(grep -o 'program id: [0-9a-f]*' <<<"$out" | awk '{print $3}')
    if [ -n "$h" ] && explorer_tx "$h" deploy >/dev/null; then
      echo "$pid" > "$STATE_DIR/program"
      log "step $n deploy ok $h program $pid ($(( $(date +%s) - t0 )) s)"
    else log "step $n deploy FAIL ${h:-nohash}: $(tail -1 <<<"$out")"; exit 1; fi ;;
  call)
    if [ "${ACTIVITY_CALL:-}" != 1 ]; then log "step $n call refused: it proves locally; set ACTIVITY_CALL=1 on a host with >= 8 GB free (never a validator host)"; exit 2; fi
    pid=$(cat "$STATE_DIR/program" 2>/dev/null)
    if [ -z "$pid" ]; then log "step $n call skipped: nothing deployed yet by this loop"; exit 0; fi
    # 400 + 250 against a threshold of 100 + <deploy step>: outputs [1, 0, 650 - threshold, ...]
    out=$($NICE "$BIN/rand" call "$pid" --input 400 --input 250 --input 0 --input 0 --key "$W1" --rpc "$RPC" 2>&1); h=$(submitted "$out" call)
    if [ -n "$h" ] && body=$(explorer_tx "$h" call) && grep -q '"h_in":"[0-9a-f]' <<<"$body"; then
      log "step $n call ok $h program $pid outputs $(grep -o '"outputs":\[[^]]*\]' <<<"$body" | head -1) $(grep -o '"input_envelope_len":[0-9]*' <<<"$body") ($(( $(date +%s) - t0 )) s)"
    else log "step $n call FAIL ${h:-nohash}: $(tail -1 <<<"$out")"; exit 1; fi ;;
  *) log "unknown step $step"; exit 2 ;;
esac

supply_check
ex=$(curl -s --max-time 10 "$EXPLORER/health")
log "step $n explorer $(grep -o '"status":"[a-z]*"' <<<"$ex") lag $(grep -o '"lag":[0-9-]*' <<<"$ex" | cut -d: -f2) chain $(curl -s --max-time 10 "$EXPLORER/stats" | grep -o '"chain_id":[0-9]*' | cut -d: -f2)"
