#!/bin/sh
# What a terminal costs in the app's own window: builds the app with the
# `probe` profile (the release build with the SHELLDER_PROBE hook left
# in), starts it beside the user's with a state directory and an ssh
# config of its own and the keychain off, and has Tests/gui-bench/probe.js
# open this machine's Console and play files into its tabs: the rate from
# the pty to the screen, the frames, the longest stall and a key's way
# round, for the DOM and the WebGL renderer, opaque and see-through.
# The windows open on a display of their own that is no screen
# (Tests/gui-bench/display.swift), so the desktop keeps what it shows;
# the app coming up takes the keyboard for a moment, and the application
# that had it is put in front again. GUI_BENCH_SCREEN=1 opens them on the
# real screen instead, to watch, and so does a Mac that makes no such
# display; there the frames only count while the screen is awake.
# See AGENTS.md.
# Usage: Tests/gui-bench.sh [megabytes of the largest file, 64]
set -eu
cd "$(dirname "$0")/.."
MB=${1:-64}
WORK=$(mktemp -d /tmp/shjt-gui.XXXXXX)
PORT=$((8900 + $$ % 200))

cleanup() {
    for pid in ${APP:-} ${SERVER:-} ${DISPLAY_PID:-}; do
        kill "$pid" 2>/dev/null || true
        wait "$pid" 2>/dev/null || true
    done
    rm -rf "$WORK"
}
trap cleanup EXIT

(cd src-tauri && cargo build --profile probe)
mkdir "$WORK/state"
: > "$WORK/config"

# The display, and the windows' places on it: the app puts a window
# where its state file says it was (window-state.json, in pixels, two
# to a point on this display).
if [ -z "${GUI_BENCH_SCREEN:-}" ] && swiftc -O Tests/gui-bench/display.swift -o "$WORK/display" 2>/dev/null; then
    mkfifo "$WORK/hold"
    "$WORK/display" 1600 1000 <"$WORK/hold" >"$WORK/frame" 2>"$WORK/display.err" &
    DISPLAY_PID=$!
    exec 9>"$WORK/hold"
    n=0
    while [ ! -s "$WORK/frame" ] && kill -0 "$DISPLAY_PID" 2>/dev/null && [ "$n" -lt 80 ]; do
        n=$((n + 1))
        sleep 0.1
    done
    if [ -s "$WORK/frame" ]; then
        read -r X Y W H <"$WORK/frame"
        place() { echo "\"$1\": {\"width\": $(($4 * 2)), \"height\": $(($5 * 2)), \"x\": $((($X + $2) * 2)), \"y\": $((($Y + $3) * 2)), \"prev_x\": 0, \"prev_y\": 0, \"maximized\": false, \"visible\": true, \"decorated\": true, \"fullscreen\": false}"; }
        echo "{$(place main 40 60 420 560), $(place console-this_machine 480 60 1080 800)}" >"$WORK/state/window-state.json"
        echo "on a display of its own, ${W}x${H} at $X,$Y"
    else
        echo "no display of its own ($(cat "$WORK/display.err")): on the screen" >&2
        DISPLAY_PID=
    fi
fi
FRONT=$(lsappinfo info -only bundleid "$(lsappinfo front 2>/dev/null)" 2>/dev/null | sed -n 's/.*="\(.*\)"/\1/p')
python3 Tests/gui-bench/serve.py "$WORK" "$PORT" "$MB" 600 &
SERVER=$!
n=0
until curl -s -o /dev/null "http://127.0.0.1:$PORT/say?l="; do
    n=$((n + 1))
    [ "$n" -lt 100 ] || { echo "the probe's server did not come up" >&2; exit 1; }
    sleep 0.1
done
SHELLDER_STATE_DIR="$WORK/state" SHELLDER_SSH_CONFIG="$WORK/config" SHELLDER_NO_KEYCHAIN=1 \
    SHELLDER_UPDATE_API="http://127.0.0.1:$PORT/none" SHELLDER_PROBE="http://127.0.0.1:$PORT" \
    src-tauri/target/probe/shellder >"$WORK/app.out" 2>&1 &
APP=$!
# The app took the keyboard as it came up: back to whoever had it.
if [ -n "${DISPLAY_PID:-}" ] && [ -n "$FRONT" ]; then
    (sleep 2; open -b "$FRONT" 2>/dev/null || true) &
fi
status=0
wait "$SERVER" || status=$?
SERVER=
# The probe told the app to quit; one that does not is stopped.
n=0
while kill -0 "$APP" 2>/dev/null && [ "$n" -lt 50 ]; do
    n=$((n + 1))
    sleep 0.2
done
[ "$status" -eq 0 ] || { echo "the app's log:" >&2; tail -20 "$WORK/state/shellder.log" >&2 || true; }
exit "$status"
