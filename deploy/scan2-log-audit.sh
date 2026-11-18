#!/usr/bin/env bash
# SCAN-2 (randprotocol/monorepo#16), owner follow-up: logs on the explorer host written before the
# fix may hold visitors' nullifiers. Before 66bc1a7 the history page POSTed them to
# /api/v1/nullifiers/lookup (bodies; a request log shows who asked and when) and the transaction
# opener asked GET /api/v1/nullifiers/<nullifier> per note (the nullifier is in the URL).
#
# Run ON the explorer host, as root. The default is a DRY RUN: it counts matching lines per source
# and prints the date range, and never prints a matching line (so a nullifier cannot leak through
# this script's output or a terminal log).
#
#   deploy/scan2-log-audit.sh                       # dry run: counts only
#   deploy/scan2-log-audit.sh --purge --before 2026-11-10
#       rotates the journal, removes journal files older than --before (journald cannot delete
#       single lines: the whole journal up to that date goes, for every unit), and deletes the
#       matching LINES from the plain log files below. --before is the day the fixed build was
#       deployed; it is required, there is no default.
#
# Sources: journald for the units in UNITS (default: randscan-api randscan-frontend caddy), and
# the plain files matched by LOG_GLOBS (default: /var/log/caddy/*.log /var/log/nginx/*.log
# /var/log/syslog /var/log/syslog.1). Compressed rotations (*.gz) are counted with zgrep and, on
# --purge, deleted whole when they match.
#
# Exit: 0 nothing found (or purged), 3 matches found in a dry run, 2 usage error.
# Tested with a fake journalctl and log files: deploy/tests/scan2-log-audit.test.sh.
set -u

purge=0
before=""
while [ $# -gt 0 ]; do
  case "$1" in
    --purge) purge=1 ;;
    --before) shift; before=${1:-} ;;
    -h|--help) sed -n '2,26p' "$0"; exit 0 ;;
    *) echo "scan2-log-audit: unknown argument '$1'" >&2; exit 2 ;;
  esac
  shift
done
if [ "$purge" -eq 1 ]; then
  case "$before" in
    [0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]) ;;
    *) echo "scan2-log-audit: --purge needs --before YYYY-MM-DD (the day the fixed build went live)" >&2; exit 2 ;;
  esac
fi

UNITS=${UNITS:-randscan-api randscan-frontend caddy}
LOG_GLOBS=${LOG_GLOBS:-/var/log/caddy/*.log /var/log/nginx/*.log /var/log/syslog /var/log/syslog.1}
JOURNALCTL=${JOURNALCTL:-journalctl}
# A lookup POST, or a /nullifiers/<64 hex> URL. The paged GET /nullifiers?after=<cursor> is the
# fixed build's own request and is not matched.
PATTERN='/api/v1/nullifiers/(lookup|[0-9a-fA-F]{64})'

found=0
report() { printf '%-9s %-44s %s\n' "$1" "$2" "$3"; }

for u in $UNITS; do
  if ! command -v "$JOURNALCTL" >/dev/null 2>&1; then
    report journal "$u" "skipped: no journalctl"; continue
  fi
  n=$("$JOURNALCTL" -u "$u" --no-pager -o short-iso 2>/dev/null | grep -cE "$PATTERN" || true)
  if [ "${n:-0}" -gt 0 ]; then
    first=$("$JOURNALCTL" -u "$u" --no-pager -o short-iso 2>/dev/null | grep -E "$PATTERN" | head -1 | cut -c1-10)
    last=$("$JOURNALCTL" -u "$u" --no-pager -o short-iso 2>/dev/null | grep -E "$PATTERN" | tail -1 | cut -c1-10)
    report journal "$u" "$n matching lines, $first .. $last"
    found=$((found + n))
  else
    report journal "$u" "0 matching lines"
  fi
done

files=""
# LOG_GLOBS is a list of globs on purpose: unquoted so the shell expands them.
# shellcheck disable=SC2086
for f in $LOG_GLOBS; do
  [ -f "$f" ] || continue
  files="$files $f"
  for z in "$f".*.gz; do [ -f "$z" ] && files="$files $z"; done
done
filehits=""
for f in $files; do
  case "$f" in
    *.gz) n=$(zgrep -cE "$PATTERN" "$f" 2>/dev/null || true) ;;
    *)    n=$(grep -cE "$PATTERN" "$f" 2>/dev/null || true) ;;
  esac
  report file "$f" "${n:-0} matching lines"
  if [ "${n:-0}" -gt 0 ]; then found=$((found + n)); filehits="$filehits $f"; fi
done

if [ "$purge" -eq 0 ]; then
  if [ "$found" -gt 0 ]; then
    echo "DRY RUN: $found matching lines. Re-run with --purge --before <deploy day> to remove them."
    exit 3
  fi
  echo "DRY RUN: nothing found in the sources above."
  exit 0
fi

echo "PURGE: journal files older than $before, and the matching lines of:${filehits:- (no plain file)}"
now=$(date -u +%s)
cut=$(date -u -d "$before" +%s 2>/dev/null || date -u -j -f '%F %T' "$before 00:00:00" +%s) || { echo "scan2-log-audit: cannot read the date '$before'" >&2; exit 2; }
age=$((now - cut))
[ "$age" -ge 1 ] || { echo "scan2-log-audit: --before $before is not in the past" >&2; exit 2; }
"$JOURNALCTL" --rotate || { echo "scan2-log-audit: journalctl --rotate failed" >&2; exit 1; }
"$JOURNALCTL" --vacuum-time="${age}s" || { echo "scan2-log-audit: journalctl --vacuum-time failed" >&2; exit 1; }
for f in $filehits; do
  case "$f" in
    *.gz) rm -f -- "$f" && echo "removed $f" ;;
    *) grep -vE "$PATTERN" "$f" > "$f.scan2.tmp"; cat "$f.scan2.tmp" > "$f"; rm -f -- "$f.scan2.tmp"; echo "rewrote $f" ;;
  esac
done
echo "PURGE done. Run the dry run again: it must report nothing (journal lines newer than $before, if any, mean the fixed build was not live on that day)."
