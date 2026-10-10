#!/bin/sh
# Assemble the updater's latest.json from the signed bundles of a release.
#   scripts/latest-json.sh VERSION NOTES_FILE ASSET_URL_BASE OUT.json ARTIFACT...
# Each ARTIFACT is PLATFORM=path/to/bundle (the .tar.gz or .AppImage next
# to which the bundler wrote a .sig), e.g. darwin-aarch64=Shellder.app.tar.gz.
set -eu
VERSION=$1; NOTES=$2; BASE=$3; OUT=$4; shift 4
{
    printf '{\n  "version": "%s",\n  "notes": %s,\n  "pub_date": "%s",\n  "platforms": {\n' \
        "$VERSION" "$(python3 -c 'import json,sys; print(json.dumps(open(sys.argv[1]).read()))' "$NOTES")" \
        "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    first=1
    for a in "$@"; do
        platform=${a%%=*}; file=${a#*=}
        [ $first = 1 ] || printf ',\n'; first=0
        printf '    "%s": {\n      "signature": "%s",\n      "url": "%s/%s"\n    }' \
            "$platform" "$(cat "$file.sig")" "$BASE" "$(basename "$file")"
    done
    printf '\n  }\n}\n'
} > "$OUT"
