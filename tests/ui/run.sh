#!/usr/bin/env bash
# Sandboxed UI simulation. See README.md in this folder.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"; ROOT="$(cd "$HERE/../.." && pwd)"
BIN="${WEFT_BIN:-$ROOT/target/debug/weft}"
: "${PLAYWRIGHT:?set PLAYWRIGHT to the playwright package directory}"
: "${CHROMIUM:?set CHROMIUM to a chrome / chrome-headless-shell binary}"
SB="$(mktemp -d -t weft-ui-XXXXXX)"
trap 'tmux -L weft-ui kill-server 2>/dev/null || true; pkill -f "$BIN serve $SB" 2>/dev/null || true' EXIT
# the UI as it is now: edits made while the suite runs do not reach it, and each test gets its own copy
mkdir -p "$SB/web.frozen"; cp -r "${WEFT_WEB_DIR:-$ROOT/web}/." "$SB/web.frozen/"
export XDG_CONFIG_HOME="$SB/cfg" XDG_STATE_HOME="$SB/state" WEFT_SOCKET=weft-ui WEFT_WEB_DIR="$SB/web"
export GIT_AUTHOR_NAME=weft GIT_AUTHOR_EMAIL=ui@weft.test GIT_COMMITTER_NAME=weft GIT_COMMITTER_EMAIL=ui@weft.test
mkdir -p "$SB/cfg/weft" "$SB/state"
export WEFT_UI_BIN="$BIN"
# the agents are simulated; their model lists come from a harmless printf instead of the real CLIs
write_cfg() {
cat > "$SB/cfg/weft/config.json" <<JSON
{ "agents": {
  "claude": {"cmd": "python3 $ROOT/examples/sim-agent.py --name claude --think 3"},
  "codex":  {"cmd": "python3 $ROOT/examples/sim-agent.py --name codex --think 4",
             "models_cmd": "printf 'sim-fast\\tSim Fast\\nsim-deep\\tSim Deep\\n'"},
  "agy":    {"cmd": "python3 $ROOT/examples/sim-agent.py --name agy --think 2.5",
             "models_cmd": "printf 'sim-fast\\tSim Fast\\nsim-deep\\tSim Deep\\n'"} },
  "titles": false }
JSON
}
fresh() {
  # stop the previous daemon first, or two of them end up sharing one tmux session
  [ -d "$SB/proj" ] && "$BIN" stop "$SB/proj" >/dev/null 2>&1 || true
  sleep 0.5
  rm -rf "$SB/proj" "$SB/remote.git" "$SB/state/weft" "$SB/web"; tmux -L weft-ui kill-server 2>/dev/null || true
  cp -r "$SB/web.frozen" "$SB/web"
  write_cfg
  cp -r "$ROOT/examples/textkit" "$SB/proj"
  git init -q -b main "$SB/proj"; git -C "$SB/proj" add -A; git -C "$SB/proj" commit -qm "textkit with two bugs"
  git init -q --bare "$SB/remote.git"; git -C "$SB/proj" remote add origin "$SB/remote.git"; git -C "$SB/proj" push -q -u origin main
  "$BIN" open "$SB/proj" --no-open | tail -1 | tr -d ' ' > "$SB/url.txt"
}
# dev: `dev` plays the page following edits to its files (src/dev.rs)
for t in ${TESTS:-story features pad folders boards tree repos next watch eta bar aim place dial archive chain tidy nets talk marks quit peek plea dev}; do
  echo "== $t"; fresh
  SANDBOX="$SB" node "$HERE/$t.cjs"
done
echo "sandbox: $SB"
