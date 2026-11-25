#!/usr/bin/env bash
# Offline test of deploy/scan2-verify.sh against deploy/tests/fake_explorer.py (127.0.0.1 only).
set -u
here=$(cd "$(dirname "$0")" && pwd -P)
verify="$here/../scan2-verify.sh"
tmp=$(mktemp -d "${TMPDIR:-/tmp}/scan2-verify-test.XXXXXX")
pids=""
# shellcheck disable=SC2086
trap 'kill $pids 2>/dev/null; rm -rf "$tmp"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS + 1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL + 1)); echo "  FAIL $1"; [ ! -f "$tmp/out" ] || sed 's/^/         /' "$tmp/out"; }

nserve=0
serve() { # mode -> sets $url
  nserve=$((nserve + 1))
  python3 -I "$here/fake_explorer.py" "$1" > "$tmp/port.$nserve" &
  pids="$pids $!"
  for _ in $(seq 1 50); do [ -s "$tmp/port.$nserve" ] && break; sleep 0.1; done
  url="http://127.0.0.1:$(cat "$tmp/port.$nserve")"
}
expect() { # name, want exit, want regex, mode, [args]
  local name=$1 want=$2 re=$3 mode=$4; shift 4
  serve "$mode"
  "$verify" "$@" "$url" > "$tmp/out" 2>&1; local got=$?
  if [ "$got" -eq "$want" ] && grep -qE "$re" "$tmp/out"; then ok "$name"; else bad "$name (exit $got, wanted $want): $(tr '\n' '|' < "$tmp/out")"; fi
}

expect "the fixed build passes all three checks" 0 'PASS  the served history page \(/viewing, 2 chunks\)' fixed
expect "--record prints a dated line on a pass" 0 '^SCAN-2 deploy check: http://127.0.0.1:[0-9]+ passed all three checks at 20[0-9-]+T[0-9:]+Z' fixed --record
expect "the old build fails" 1 'FAIL  POST /api/v1/nullifiers/lookup answered HTTP 200' old
expect "the old build's frontend is named" 1 'FAIL  the served frontend can still send a nullifier \(in: /_next/static/chunks/app/%5Blang%5D/viewing/page-1a2b.js\)' old
expect "a fixed API behind an old frontend fails" 1 'FAIL  the served frontend can still send' old-frontend
expect "a fixed frontend over an old API fails" 1 'FAIL  POST /api/v1/nullifiers/lookup answered HTTP 200' old-api
expect "a missing paged set fails" 1 'FAIL  GET /api/v1/nullifiers\?limit=1 answered HTTP 404' no-page
expect "a page with no chunks is a failure, not a vacuous pass" 1 'references no /_next/static chunk' no-chunks
# the route: /viewing first, /en/viewing for builds before the i18n routing, redirects on a leash
expect "a build that only serves /en/viewing passes by the fallback" 0 'PASS  the served history page \(/en/viewing, 2 chunks\)' legacy-path
expect "a same-origin redirect to a localised history page is followed" 0 'PASS  the served history page \(/id/viewing, 2 chunks\)' absolute-redirect
expect "a redirect off the origin fails" 1 'FAIL  GET /viewing answered HTTP 308 to https://example.invalid/viewing: a redirect off http://127.0.0.1:[0-9]+ is not followed' redirect-offsite
expect "a scheme-relative redirect fails" 1 'FAIL  GET /viewing answered HTTP 308 to //example.invalid/viewing: a redirect off' redirect-schemeless
expect "a redirect to a page that is not the history page fails" 1 'FAIL  GET /viewing answered HTTP 308 to /: not the history page' redirect-home
expect "a redirect loop fails" 1 'FAIL  GET /viewing: more than 3 redirects' redirect-loop
expect "a 200 that is not the history page fails" 1 'references no viewing/page chunk: it is not the history page' other-page
expect "--record prints FAILED on a failure" 1 '^SCAN-2 deploy check: .* FAILED at ' old --record

"$verify" > "$tmp/out" 2>&1; got=$?
if [ "$got" -eq 2 ]; then ok "no URL is a usage error"; else bad "no URL is a usage error"; fi
"$verify" http://127.0.0.1:1 > "$tmp/out" 2>&1; got=$?
if [ "$got" -eq 1 ] && grep -q 'no answer' "$tmp/out"; then ok "an unreachable explorer fails (exit $got)"; else bad "an unreachable explorer fails (exit $got)"; fi

echo "scan2-verify test: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
