#!/usr/bin/env bash
# Idle-RAM benchmark (Linux). Launches a CEF-based binary with N tabs, waits for the page
# to settle, then sums PSS (proportional set size, so shared pages are not double-counted)
# and RSS over the whole process tree.
#   scripts/bench-linux.sh <label> <settle-seconds> <binary> [extra args...]
# Expects the binary to accept --urls=a,b,c and --profile=<dir>, and CEF_PATH to be set.
set -euo pipefail
label=$1; settle=$2; bin=$3; shift 3
here=$(cd "$(dirname "$0")/.." && pwd)
probe="$here/target/release/emerald-probe"; [ -x "$probe" ] || probe="$here/target/debug/emerald-probe"
for n in ${COUNTS:-1 5 20}; do
  urls=$(head -n $n "$here/scripts/bench-urls.txt" | paste -sd, -)
  prof=$(mktemp -d)
  "$bin" --profile="$prof" --urls="$urls" "$@" >/dev/null 2>&1 &
  pid=$!
  sleep "$settle"
  read -r procs rss pss < <("$probe" mem $pid | python3 -c "import json,sys;d=json.load(sys.stdin);print(d['processes'],d['rss_mb'],d['pss_mb'])")
  echo "$label tabs=$n processes=$procs pss_mb=$pss rss_mb=$rss"
  kill $pid; wait $pid 2>/dev/null || true; sleep 3; rm -rf "$prof" 2>/dev/null || true
done
