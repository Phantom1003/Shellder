#!/bin/sh
# What drawing a terminal costs: Tests/render-bench/ gives xterm.js the
# same output with no renderer, the DOM renderer and the WebGL one, in the
# system's WebKit (the app's engine) in a window that is on no screen, and
# this prints the rates, the share of the time that went into drawing,
# the frames and the longest stall. Needs macOS with swiftc and python3;
# nothing of the app is built or started. See AGENTS.md.
# Usage: Tests/render-bench.sh [megabytes of the largest run, 128]
set -eu
cd "$(dirname "$0")/.."
MB=${1:-128}
WORK=$(mktemp -d /tmp/shjt-render.XXXXXX)
PORT=$((8700 + $$ % 200))

cleanup() {
    if [ -n "${SERVER:-}" ]; then
        kill "$SERVER" 2>/dev/null || true
        wait "$SERVER" 2>/dev/null || true
    fi
    rm -rf "$WORK"
}
trap cleanup EXIT

swiftc -O Tests/render-bench/webkit.swift -o "$WORK/webkit" 2>"$WORK/swiftc.log" || { cat "$WORK/swiftc.log" >&2; exit 1; }
python3 -m http.server "$PORT" --bind 127.0.0.1 >/dev/null 2>&1 &
SERVER=$!
n=0
until curl -s -o /dev/null "http://127.0.0.1:$PORT/Tests/render-bench/index.html"; do
    n=$((n + 1))
    [ "$n" -lt 50 ] || { echo "the page's server did not come up" >&2; exit 1; }
    sleep 0.1
done
"$WORK/webkit" "http://127.0.0.1:$PORT/Tests/render-bench/index.html?mb=$MB" 900
