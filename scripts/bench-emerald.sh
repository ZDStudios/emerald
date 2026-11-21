#!/usr/bin/env bash
# Emerald idle-RAM benchmark (Linux, X11). Scenarios, each with 1, 5 and 20 tabs from
# scripts/bench-urls.txt:
#   eager   every tab loaded (what a browser without sleeping/lazy restore would hold)
#   lazy    Emerald defaults: restored tabs stay asleep until opened
#   freed   eager, then "Free memory" (all background tabs put to sleep)
# Memory = sum of PSS over the whole process tree (shared pages counted proportionally).
#   scripts/bench-emerald.sh [settle-seconds]          (needs CEF_PATH, DISPLAY)
set -uo pipefail
settle=${1:-40}
here=$(cd "$(dirname "$0")/.." && pwd)
bin="$here/target/release/emerald"
probe="$here/target/release/emerald-probe"
port=9333
export EMERALD_CDP_PORT=$port
measure() { "$probe" mem "$1" | python3 -c "import json,sys;d=json.load(sys.stdin);print(d['processes'],d['pss_mb'],d['rss_mb'])"; }
for scenario in ${SCENARIOS:-eager lazy freed}; do
  for n in 1 5 20; do
    urls=$(head -n $n "$here/scripts/bench-urls.txt" | paste -sd, -)
    prof=$(mktemp -d)
    mkdir -p "$prof/emerald" && echo '{"first_run_done": true}' > "$prof/emerald/settings.json"
    extra=()
    [ "$scenario" != lazy ] && extra+=(--eager)
    "$bin" --profile="$prof" --urls="$urls" --remote-debugging-port=$port "${extra[@]}" >/dev/null 2>&1 &
    pid=$!
    sleep "$settle"
    # Mark the wizard done so it doesn't open a tab, then optionally free memory.
    "$probe" eval "emerald://ui/" "window.emerald.call('wizard.done',{})" >/dev/null 2>&1
    if [ "$scenario" = freed ]; then
      "$probe" eval "emerald://ui/" "window.emerald.call('memory.free',{})" >/dev/null 2>&1
      # Chromium tears down closed renderers lazily; give it time before measuring.
      sleep 45
    fi
    live=$("$probe" eval "emerald://ui/" "window.emerald.call('state',{}).then(s=>s.tabs.filter(t=>t.live).length)" 2>/dev/null | tr -d '"')
    read -r procs pss rss < <(measure $pid)
    echo "emerald scenario=$scenario tabs=$n live_tabs=$live processes=$procs pss_mb=$pss rss_mb=$rss"
    kill $pid; wait $pid 2>/dev/null; sleep 3; rm -rf "$prof" 2>/dev/null
  done
done
