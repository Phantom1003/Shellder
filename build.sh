#!/bin/bash
# Build Shellder with the Tauri CLI.
#   ./build.sh            -> build/Shellder.app (macOS) or build/*.AppImage, *.deb (Linux)
#   ./build.sh --install  -> also copy to ~/Applications and launch it (macOS)
#   ./build.sh --debug    -> a debug build, unsigned, no updater artifacts
#   ./build.sh --deps     -> only install what a build needs, then stop
set -euo pipefail
cd "$(dirname "$0")"

# What a build needs, installed when it is missing: Rust, the Tauri CLI,
# and on Linux the development packages of what Tauri links against
# (webkit2gtk 4.1 with its GTK, soup and JavaScriptCore, the appindicator
# for the tray icon, rsvg for the icons, dbus for the Secret Service
# keyring, patchelf and xdo for the AppImage). A build that finds them all
# installs nothing and asks for nothing; CI calls this first as --deps.
sudo_() {
    if [ "$(id -u)" = 0 ]; then "$@"; else sudo "$@"; fi
}

linux_packages() {
    if command -v apt-get >/dev/null 2>&1; then
        # Debian, Ubuntu: the exact list, checked package by package.
        PKGS=(build-essential pkg-config curl wget file patchelf libssl-dev
            libdbus-1-dev libglib2.0-dev libgtk-3-dev libsoup-3.0-dev
            libjavascriptcoregtk-4.1-dev libwebkit2gtk-4.1-dev
            libayatana-appindicator3-dev librsvg2-dev libxdo-dev openssh-client)
        MISSING=()
        for p in "${PKGS[@]}"; do
            [ "$(dpkg-query -W -f='${Status}' "$p" 2>/dev/null)" = "install ok installed" ] || MISSING+=("$p")
        done
        if [ ${#MISSING[@]} -gt 0 ]; then
            echo "installing: ${MISSING[*]}"
            sudo_ apt-get update
            sudo_ apt-get install -y "${MISSING[@]}"
        fi
        return
    fi
    # Other distributions: the same libraries under their own names, taken
    # from Tauri's prerequisites page. Installed when pkg-config or PATH
    # misses one of them.
    NEED=""
    for m in webkit2gtk-4.1 javascriptcoregtk-4.1 libsoup-3.0 gtk+-3.0 glib-2.0 dbus-1 librsvg-2.0 openssl; do
        pkg-config --exists "$m" 2>/dev/null || NEED=1
    done
    pkg-config --exists ayatana-appindicator3-0.1 2>/dev/null || pkg-config --exists appindicator3-0.1 2>/dev/null || NEED=1
    for c in cc pkg-config patchelf curl wget file ssh; do
        command -v "$c" >/dev/null 2>&1 || NEED=1
    done
    [ -z "$NEED" ] && return
    echo "installing the build packages"
    if command -v dnf >/dev/null 2>&1; then
        sudo_ dnf install -y @c-development pkgconf-pkg-config curl wget file patchelf openssl-devel \
            dbus-devel glib2-devel gtk3-devel libsoup3-devel javascriptcoregtk4.1-devel webkit2gtk4.1-devel \
            libappindicator-gtk3-devel librsvg2-devel libxdo-devel openssh-clients
    elif command -v pacman >/dev/null 2>&1; then
        sudo_ pacman -S --needed --noconfirm base-devel pkgconf curl wget file patchelf openssl \
            dbus glib2 gtk3 libsoup3 webkit2gtk-4.1 libayatana-appindicator librsvg xdotool openssh
    elif command -v zypper >/dev/null 2>&1; then
        sudo_ zypper --non-interactive install -t pattern devel_basis
        sudo_ zypper --non-interactive install pkgconf-pkg-config curl wget file patchelf libopenssl-devel \
            dbus-1-devel glib2-devel gtk3-devel libsoup-3_0-devel webkit2gtk3-devel \
            libappindicator3-devel librsvg-devel xdotool-devel openssh
    else
        echo "error: no apt-get, dnf, pacman or zypper here; install what Debian ships as:" >&2
        echo "  build-essential pkg-config patchelf libssl-dev libdbus-1-dev libglib2.0-dev libgtk-3-dev" >&2
        echo "  libsoup-3.0-dev libjavascriptcoregtk-4.1-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev" >&2
        echo "  librsvg2-dev libxdo-dev openssh-client" >&2
        exit 1
    fi
}

ensure_deps() {
    # The packages first: rustup needs the curl they bring.
    case "$(uname -s)" in
        Linux) linux_packages ;;
    esac
    if ! command -v cargo >/dev/null 2>&1; then
        if [ ! -x "$HOME/.cargo/bin/cargo" ]; then
            echo "installing Rust with rustup"
            curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        fi
        # shellcheck disable=SC1091
        . "$HOME/.cargo/env"
    fi
    if ! cargo tauri --version >/dev/null 2>&1; then
        echo "installing the Tauri CLI"
        cargo install tauri-cli --version '^2' --locked
    fi
}

ensure_deps
for a in "$@"; do
    [ "$a" = --deps ] && exit 0
done

# Icons from scripts/icon-source.png (or .jpg), cropped to a centred square
# and scaled to every size the bundler wants. The menu bar picture
# (src-tauri/icons/tray.png) is checked in: it comes from
# scripts/menubar-template, see docs/manual.md.
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
