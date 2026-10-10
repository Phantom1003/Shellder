#!/bin/bash
# Build Shellder with the Tauri CLI.
#   ./build.sh            -> build/Shellder.app (macOS) or build/*.AppImage, *.deb (Linux)
#   ./build.sh --install  -> also copy to ~/Applications and launch it (macOS)
#   ./build.sh --debug    -> a debug build, unsigned, no updater artifacts
set -euo pipefail
cd "$(dirname "$0")"

if ! cargo tauri --version >/dev/null 2>&1; then
    echo "the Tauri CLI is missing: cargo install tauri-cli --version '^2' --locked" >&2
    exit 1
fi

# Icons from scripts/icon-source.png (or .jpg), cropped to a centred square
# and scaled to every size the bundler wants. The menu bar pictures
# (src-tauri/icons/tray*.png) are checked in: they come from
# scripts/menubar-template.swift, which only runs on macOS.
SRC=""
for f in scripts/icon-source.png scripts/icon-source.jpg scripts/icon-source.jpeg; do [ -f "$f" ] && SRC="$f" && break; done
if [ -z "$SRC" ]; then
    echo "error: scripts/icon-source.png (or .jpg) is missing" >&2
    exit 1
fi
if [ ! -f src-tauri/icons/icon.icns ] || [ "$SRC" -nt src-tauri/icons/icon.icns ]; then
    cargo tauri icon "$SRC" -o src-tauri/icons >/dev/null
    echo "icons from $SRC"
fi

DEBUG=""
INSTALL=""
for a in "$@"; do
    case "$a" in
        --debug) DEBUG=1 ;;
        --install) INSTALL=1 ;;
    esac
done

ARGS=()
[ -n "$DEBUG" ] && ARGS+=(--debug)
case "$(uname -s)" in
    Darwin)
        ARGS+=(--bundles app)
        # Signing identity, in order of preference: $SHELLDER_SIGN_IDENTITY,
        # then an Apple-issued identity (Apple Development / Developer ID):
        # it carries a Team ID, so keychain items get a stable "teamid:"
        # partition and never ask for the keychain password again after a
        # rebuild. There is no ad hoc fallback for a release build: an ad
        # hoc build is a new identity on every rebuild, and the vault, the
        # login item and the system permissions granted to the app are all
        # keyed to the signature. SHELLDER_SIGN_IDENTITY=- asks for ad hoc
        # explicitly; a --debug build is left unsigned.
        IDENTITY="${SHELLDER_SIGN_IDENTITY:-}"
        if [ -z "$IDENTITY" ] && [ -z "$DEBUG" ]; then
            IDENTITY=$(security find-identity -v -p codesigning 2>/dev/null \
                | grep -oE '"(Apple Development|Developer ID Application): [^"]+"' | head -1 | tr -d '"' || true)
        fi
        if [ -z "$IDENTITY" ] && [ -z "$DEBUG" ]; then
            cat >&2 <<'MSG'
no Apple signing identity found, refusing to sign ad hoc.
Sign into Xcode (Settings > Accounts > Manage Certificates > + > Apple
Development; a free Personal Team is enough) or import your certificate
into the login keychain, or set SHELLDER_SIGN_IDENTITY to one that
"security find-identity -v -p codesigning" lists (or to - for ad hoc).
MSG
            exit 1
        fi
        if [ -n "$IDENTITY" ]; then
            echo "signing with: $IDENTITY"
            export APPLE_SIGNING_IDENTITY="$IDENTITY"
        fi
        ;;
    Linux)
        ARGS+=(--bundles deb,appimage)
        ;;
esac

# The updater artifact (Shellder.app.tar.gz + .sig) needs the maintainer's
# minisign key (TAURI_SIGNING_PRIVATE_KEY); without it, build the bundle alone.
if [ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
    ARGS+=(--config '{"bundle":{"createUpdaterArtifacts":false}}')
fi

(cd src-tauri && cargo tauri build "${ARGS[@]}")

PROFILE=release
[ -n "$DEBUG" ] && PROFILE=debug
rm -rf build
mkdir -p build
case "$(uname -s)" in
    Darwin)
        APP="build/Shellder.app"
        cp -R "src-tauri/target/$PROFILE/bundle/macos/Shellder.app" "$APP"
        TEAM=$(codesign -dv --verbose=2 "$APP" 2>&1 | grep -E '^TeamIdentifier=' || true)
        echo "${TEAM:-TeamIdentifier=not set}"
        case "$TEAM" in
            TeamIdentifier=not\ set|"")
                echo "warning: no Team ID in the signature, the keychain will ask again after every rebuild" >&2 ;;
        esac
        # Tell LaunchServices/Finder about the new bundle (otherwise a
        # cached, icon-less registration can linger).
        touch "$APP"
        /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$APP" 2>/dev/null || true
        echo "built $APP"
        if [ -n "$INSTALL" ]; then
            mkdir -p "$HOME/Applications"
            # Stop a running copy first (ignore errors), then replace and relaunch.
            pkill -x shellder 2>/dev/null || true
            sleep 1
            rm -rf "$HOME/Applications/Shellder.app"
            cp -R "$APP" "$HOME/Applications/Shellder.app"
            echo "installed to ~/Applications/Shellder.app"
            open "$HOME/Applications/Shellder.app"
        fi
        ;;
    Linux)
        cp "src-tauri/target/$PROFILE/bundle/deb/"*.deb "src-tauri/target/$PROFILE/bundle/appimage/"*.AppImage build/ 2>/dev/null || true
        ls -1 build
        ;;
esac
