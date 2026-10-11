#!/bin/sh
# A Linux build from nothing, in Docker: a bare Ubuntu image gets what
# build.sh --deps installs (apt packages, rustup, the Tauri CLI), then the
# unit tests CI runs and a full ./build.sh with its .deb and .AppImage.
# The sources are copied into the container rather than bind-mounted: a
# directory mounted from macOS is case-insensitive, and appimagetool's
# .DirIcon link collides with the icon there. See AGENTS.md.
# Usage: Tests/linux-build.sh [UBUNTU_VERSION...]   (default: 24.04 26.04)
set -eu
cd "$(dirname "$0")/.."
[ $# -gt 0 ] || set -- 24.04 26.04
for v in "$@"; do
    NAME=shjt-linux-build-$v
    docker rm -f "$NAME" >/dev/null 2>&1 || true
    docker run -d --name "$NAME" "ubuntu:$v" sleep infinity >/dev/null
    trap 'docker rm -f "$NAME" >/dev/null 2>&1 || true' EXIT
    # The checked-in tree, not the working directory's build output.
    # COPYFILE_DISABLE keeps macOS tar from adding ._ files for extended
    # attributes: tauri_build parses every file under capabilities/.
    git ls-files -z | COPYFILE_DISABLE=1 tar --null -T - -cf - | docker exec -i "$NAME" sh -c 'mkdir -p /work && tar -C /work -xf -'
    docker exec -e DEBIAN_FRONTEND=noninteractive -e APPIMAGE_EXTRACT_AND_RUN=1 -e CARGO_TERM_COLOR=never \
        "$NAME" bash -euo pipefail -c '
        cd /work
        echo "### $(. /etc/os-release; echo "$PRETTY_NAME") $(uname -m)"
        echo "### build.sh --deps"
        ./build.sh --deps
        . "$HOME/.cargo/env"
        echo "### build.sh --deps again, must install nothing"
        ./build.sh --deps | grep -q installing && { echo "second --deps installed something"; exit 1; }
        echo "### icons and unit tests"
        cargo tauri icon scripts/icon-source.png -o src-tauri/icons >/dev/null
        (cd src-tauri && cargo test --lib --test pty_prompt 2>&1 | grep -vE "^\s*(Compiling|Downloading|Downloaded|Updating|Locking|Adding|Blocking) ")
        echo "### build.sh"
        ./build.sh 2>&1 | grep -E "Bundling|Finished [0-9]|Error|error" || true
        ls -la build
        test -n "$(ls build/*.deb)" && test -n "$(ls build/*.AppImage)"
        echo "### the AppImage runs"
        build/*.AppImage help | head -1
    '
    docker rm -f "$NAME" >/dev/null 2>&1 || true
    trap - EXIT
    echo "### ubuntu:$v ok"
done
