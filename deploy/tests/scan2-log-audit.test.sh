#!/usr/bin/env bash
# Offline test of deploy/scan2-log-audit.sh: a fake journalctl on a private PATH and log files in a
# temp directory. Nothing outside the temp directory is read or written.
set -u
here=$(cd "$(dirname "$0")" && pwd -P)
audit="$here/../scan2-log-audit.sh"
tmp=$(mktemp -d "${TMPDIR:-/tmp}/scan2-log-test.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS + 1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL + 1)); echo "  FAIL $1"; [ ! -f "$tmp/out" ] || sed 's/^/         /' "$tmp/out"; }
NF=8c04aa11223344556677889900aabbccddeeff00112233445566778899aabbd1

mkdir -p "$tmp/bin" "$tmp/log"
cat > "$tmp/bin/journalctl" <<EOF
#!/usr/bin/env bash
echo "\$*" >> "$tmp/journalctl.calls"
case "\$*" in
  *"-u randscan-api"*) cat "$tmp/journal.api" 2>/dev/null ;;
  *--rotate*|*--vacuum-time*) ;;
esac
exit 0
EOF
chmod +x "$tmp/bin/journalctl"
run() { JOURNALCTL="$tmp/bin/journalctl" UNITS="randscan-api caddy" LOG_GLOBS="$tmp/log/*.log" "$audit" "$@" > "$tmp/out" 2>&1; }
seed() {
  printf '%s\n' \
    "2026-10-20T10:00:00+0000 host randscan-api[1]: GET /api/v1/nullifiers/$NF 200" \
    "2026-10-21T10:00:00+0000 host randscan-api[1]: POST /api/v1/nullifiers/lookup 200" \
    "2026-11-10T10:00:00+0000 host randscan-api[1]: GET /api/v1/nullifiers?after=$NF&limit=5000 200" > "$tmp/journal.api"
  printf '%s\n' "a GET /api/v1/blocks 200" "b GET /api/v1/nullifiers/$NF 200" "c GET /api/v1/nullifiers?limit=5000 200" > "$tmp/log/access.log"
  printf '%s\n' "z POST /api/v1/nullifiers/lookup 200" | gzip > "$tmp/log/access.log.1.gz"
  printf '%s\n' "clean line" > "$tmp/log/other.log"
  : > "$tmp/journalctl.calls"
}


seed; run; got=$?
if [ "$got" -eq 3 ] && grep -q 'DRY RUN: 4 matching lines' "$tmp/out"; then ok "the dry run counts 2 journal + 1 file + 1 gz lines and exits 3 (exit $got)"; else bad "the dry run counts 2 journal + 1 file + 1 gz lines and exits 3 (exit $got)"; fi
if grep -q 'randscan-api .*2 matching lines, 2026-10-20 .. 2026-10-21' "$tmp/out"; then ok "the journal's date range is reported"; else bad "the journal's date range is reported"; fi
if ! grep -q "$NF" "$tmp/out"; then ok "no nullifier appears in the output"; else bad "no nullifier appears in the output"; fi
if ! grep -qE -- '--rotate|--vacuum' "$tmp/journalctl.calls"; then ok "the dry run never rotates or vacuums"; else bad "the dry run never rotates or vacuums"; fi
if [ "$(wc -l < "$tmp/log/access.log" | tr -d ' ')" = 3 ] && [ -f "$tmp/log/access.log.1.gz" ]; then ok "the dry run leaves the files alone"; else bad "the dry run leaves the files alone"; fi

seed; run --purge; got=$?
if [ "$got" -eq 2 ]; then ok "--purge without --before is refused"; else bad "--purge without --before is refused"; fi
seed; run --purge --before tomorrow; got=$?
if [ "$got" -eq 2 ]; then ok "a junk --before is refused"; else bad "a junk --before is refused"; fi
seed; run --purge --before 2999-01-01; got=$?
if [ "$got" -eq 2 ] && ! grep -qE -- '--rotate' "$tmp/journalctl.calls"; then ok "a future --before is refused before anything is rotated (exit $got)"; else bad "a future --before is refused before anything is rotated (exit $got)"; fi

seed; run --purge --before 2026-11-01; got=$?
if [ "$got" -eq 0 ]; then ok "the purge exits 0 (exit $got)"; else bad "the purge exits 0 (exit $got)"; fi
if grep -q -- '--rotate' "$tmp/journalctl.calls" && grep -qE -- '--vacuum-time=[0-9]+s' "$tmp/journalctl.calls"; then ok "the purge rotates, then vacuums by age"; else bad "the purge rotates, then vacuums by age"; fi
if [ "$(cat "$tmp/log/access.log")" = "$(printf '%s\n' "a GET /api/v1/blocks 200" "c GET /api/v1/nullifiers?limit=5000 200")" ]; then ok "only the matching line left the plain log"; else bad "only the matching line left the plain log"; fi
if [ ! -e "$tmp/log/access.log.1.gz" ]; then ok "the matching compressed rotation is removed"; else bad "the matching compressed rotation is removed"; fi
if [ "$(cat "$tmp/log/other.log")" = "clean line" ]; then ok "a log with no match is untouched"; else bad "a log with no match is untouched"; fi

: > "$tmp/journal.api"; run; got=$?
if [ "$got" -eq 0 ] && grep -q 'DRY RUN: nothing found' "$tmp/out"; then ok "after the purge the dry run finds nothing (exit $got)"; else bad "after the purge the dry run finds nothing (exit $got)"; fi

echo "scan2-log-audit test: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
