#!/bin/sh
# Builds the documentation site into build/site: the front page
# (docs/site) with its demo, the English and the Chinese book (docs/en,
# docs/zh, mdBook) and the pictures (docs/images), and with --api the
# Rust side's API reference under api/ (rustdoc, private items included;
# it compiles the crate, so it wants what a build wants: ./build.sh
# --deps, the icons). mdBook is installed with cargo when it is missing.
# The demo is the app's own pages: ui/ copied to app/, each page with
# docs/site/demo/shim.js put in front of it, which answers what the page
# asks the app from docs/site/demo/backend.js. --front builds the front
# page and its demo alone, for working on them: no book, no mdBook.
#
#   docs/build.sh [--front] [--api] [--serve [PORT]]
#
# --serve serves the result at http://127.0.0.1:PORT/Shellder/ (8700),
# the path it has on GitHub Pages, until interrupted.
set -eu
cd "$(dirname "$0")/.."
API=
FRONT=
SERVE=
PORT=8700
while [ $# -gt 0 ]; do
    case "$1" in
        --api) API=1 ;;
        --front) FRONT=1 ;;
        --serve)
            SERVE=1
            case "${2:-}" in [0-9]*) PORT=$2; shift ;; esac ;;
        *) echo "usage: docs/build.sh [--front] [--api] [--serve [PORT]]" >&2; exit 2 ;;
    esac
    shift
done

OUT=build/site
rm -rf "$OUT"
mkdir -p "$OUT"
if [ -z "$FRONT" ]; then
    command -v mdbook >/dev/null 2>&1 || cargo install mdbook --locked
    for lang in en zh; do
        mdbook build "docs/$lang"
    done
fi
cp -R docs/site/. "$OUT/"
cp -R docs/images "$OUT/images"
# The demo's windows: the app's pages as they are, the stand-in for the
# app loaded before anything of theirs.
cp -R ui "$OUT/app"
for page in "$OUT"/app/*.html; do
    sed 's|<head>|<head><script src="../demo/shim.js"></script>|' "$page" > "$page.new"
    mv "$page.new" "$page"
done
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
