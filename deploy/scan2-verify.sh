#!/usr/bin/env bash
# SCAN-2 (randprotocol/monorepo#16): is the explorer at <base-url> the build that no longer takes
# a visitor's nullifiers?  Read-only: three kinds of HTTP request, nothing written anywhere.
#
#   deploy/scan2-verify.sh https://randscan.org            # prints PASS/FAIL per check, exit 0/1
#   deploy/scan2-verify.sh --record https://randscan.org   # also prints a dated line for the issue
#
# Checks:
#   1. POST /api/v1/nullifiers/lookup answers 404 or 405 (the bulk lookup route is gone). The body
#      sent names one all-zero nullifier, which belongs to no wallet.
#   2. GET /api/v1/nullifiers?limit=1 answers 200 with {"nullifiers": [...], "next": ...} (the
#      paged public set the browser matches against).
#   3. The history page's JavaScript, as served, contains no "nullifiers/lookup" and no
#      single-nullifier request builder: every /_next/static chunk the page references is
#      downloaded and searched.
#
# Exit 0 only when all three pass; 2 on a usage error. Needs curl. Tested offline against a fake
# explorer: deploy/tests/scan2-verify.test.sh.
set -u

record=0
base=""
for a in "$@"; do
  case "$a" in
    --record) record=1 ;;
    -h|--help) sed -n '2,20p' "$0"; exit 0 ;;
    http://*|https://*) base=${a%/} ;;
    *) echo "scan2-verify: unknown argument '$a'" >&2; exit 2 ;;
  esac
done
[ -n "$base" ] || { echo "usage: scan2-verify.sh [--record] <base-url>" >&2; exit 2; }
command -v curl >/dev/null 2>&1 || { echo "scan2-verify: curl is not installed" >&2; exit 2; }

tmp=$(mktemp -d "${TMPDIR:-/tmp}/scan2-verify.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
fail=0
pass() { echo "PASS  $1"; }
bad()  { echo "FAIL  $1"; fail=1; }
get()  { local c; c=$(curl -sS -m "${SCAN2_TIMEOUT:-20}" -o "$2" -w '%{http_code}' "$1" 2>"$tmp/err") || c=000; echo "$c"; }

# 1. the removed route
zero=0000000000000000000000000000000000000000000000000000000000000000
code=$(curl -sS -m "${SCAN2_TIMEOUT:-20}" -o "$tmp/lookup" -w '%{http_code}' -X POST \
  -H 'content-type: application/json' --data "{\"nullifiers\":[\"$zero\"]}" \
  "$base/api/v1/nullifiers/lookup" 2>"$tmp/err") || code=000
case "$code" in
  404|405) pass "POST /api/v1/nullifiers/lookup is gone (HTTP $code)" ;;
  000) bad "POST /api/v1/nullifiers/lookup: no answer ($(head -c 200 "$tmp/err"))" ;;
  *) bad "POST /api/v1/nullifiers/lookup answered HTTP $code: the lookup route is still served" ;;
esac

# 2. the paged set
code=$(get "$base/api/v1/nullifiers?limit=1" "$tmp/page")
if [ "$code" = 200 ] && grep -q '"nullifiers"[[:space:]]*:' "$tmp/page" && grep -q '"next"[[:space:]]*:' "$tmp/page"; then
  pass "GET /api/v1/nullifiers?limit=1 serves the paged set"
else
  bad "GET /api/v1/nullifiers?limit=1 answered HTTP $code without {nullifiers, next}: the browser cannot match locally"
fi

# 3. the served frontend
code=$(get "$base/en/viewing" "$tmp/viewing.html")
if [ "$code" != 200 ]; then
  bad "GET /en/viewing answered HTTP $code: the history page could not be read"
else
  grep -oE '/_next/static/[A-Za-z0-9_./%@()~-]+\.js' "$tmp/viewing.html" | sort -u > "$tmp/chunks"
  n=0; hits=""
  while IFS= read -r c; do
    [ -n "$c" ] || continue
    code=$(get "$base$c" "$tmp/chunk.js")
    [ "$code" = 200 ] || { bad "chunk $c answered HTTP $code"; continue; }
    n=$((n + 1))
    # the old client's two request builders: the bulk lookup, and /nullifiers/<encodeURIComponent(nf)>
    if grep -qE 'nullifiers/lookup|/nullifiers/"\.concat\(encodeURIComponent|/nullifiers/\$\{encodeURIComponent' "$tmp/chunk.js"; then
      hits="$hits $c"
    fi
  done < "$tmp/chunks"
  if [ "$n" -eq 0 ]; then
    bad "/en/viewing references no /_next/static chunk: nothing was checked"
  elif [ -n "$hits" ]; then
    bad "the served frontend can still send a nullifier (in:$hits)"
  else
    pass "the served history page ($n chunks) has no nullifier lookup"
  fi
fi

if [ "$fail" -eq 0 ]; then
  [ "$record" -eq 0 ] || echo "SCAN-2 deploy check: $base passed all three checks at $(date -u +%FT%TZ) (deploy/scan2-verify.sh)"
  exit 0
fi
[ "$record" -eq 0 ] || echo "SCAN-2 deploy check: $base FAILED at $(date -u +%FT%TZ) (deploy/scan2-verify.sh)"
exit 1
