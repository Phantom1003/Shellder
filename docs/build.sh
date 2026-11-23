#!/bin/sh
# Builds the documentation site into build/site: the front page
# (docs/site), the English and the Chinese book (docs/en, docs/zh, mdBook)
# and the pictures (docs/images), and with --api the Rust side's API
# reference under api/ (rustdoc, private items included; it compiles the
# crate, so it wants what a build wants: ./build.sh --deps, the icons).
# mdBook is installed with cargo when it is missing.
#
#   docs/build.sh [--api] [--serve [PORT]]
#
# --serve serves the result at http://127.0.0.1:PORT/Shellder/ (8700),
# the path it has on GitHub Pages, until interrupted.
set -eu
cd "$(dirname "$0")/.."
API=
SERVE=
PORT=8700
while [ $# -gt 0 ]; do
    case "$1" in
        --api) API=1 ;;
        --serve)
            SERVE=1
            case "${2:-}" in [0-9]*) PORT=$2; shift ;; esac ;;
        *) echo "usage: docs/build.sh [--api] [--serve [PORT]]" >&2; exit 2 ;;
    esac
    shift
done

command -v mdbook >/dev/null 2>&1 || cargo install mdbook --locked
OUT=build/site
rm -rf "$OUT"
mkdir -p "$OUT"
for lang in en zh; do
    mdbook build "docs/$lang"
done
cp docs/site/* "$OUT/"
cp -R docs/images "$OUT/images"
# GitHub Pages runs what it is given through Jekyll otherwise.
: > "$OUT/.nojekyll"
if [ -n "$API" ]; then
    (cd src-tauri && cargo doc --no-deps --document-private-items --lib)
    cp -R src-tauri/target/doc "$OUT/api"
fi
echo "the site is in $OUT"
if [ -n "$SERVE" ]; then
    ROOT=$(mktemp -d /tmp/shjt-docs.XXXXXX)
    trap 'rm -rf "$ROOT"' EXIT
    ln -s "$PWD/$OUT" "$ROOT/Shellder"
    echo "http://127.0.0.1:$PORT/Shellder/"
    python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$ROOT"
fi
