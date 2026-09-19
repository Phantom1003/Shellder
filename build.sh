#!/bin/bash
# Build shellder.app from the SwiftPM package.
#   ./build.sh            -> build/shellder.app
#   ./build.sh --install  -> also copy to ~/Applications and launch it
set -euo pipefail
# Fall back to the Command Line Tools when Xcode itself is unusable (e.g. its
# licence has not been accepted after an update). The CLT lack the SwiftUI
# macro plugin, so borrow it from Xcode.
SWIFTFLAGS=()
if ! xcodebuild -version >/dev/null 2>&1 && [ -d /Library/Developer/CommandLineTools ]; then
    echo "xcodebuild unusable (run 'sudo xcodebuild -license accept'?); building with the Command Line Tools" >&2
    export DEVELOPER_DIR=/Library/Developer/CommandLineTools
    PLUGINS=/Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/usr/lib/swift/host/plugins
    [ -d "$PLUGINS" ] && SWIFTFLAGS=(-Xswiftc -plugin-path -Xswiftc "$PLUGINS")
fi
cd "$(dirname "$0")"

swift build -c release ${SWIFTFLAGS[@]+"${SWIFTFLAGS[@]}"}
BIN="$(swift build -c release ${SWIFTFLAGS[@]+"${SWIFTFLAGS[@]}"} --show-bin-path)/shellder"

APP="build/Shellder.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/shellder"
cp Info.plist "$APP/Contents/Info.plist"
# Translations: Sources/shellder/Resources/<lang>.lproj/Localizable.strings,
# looked up through Bundle.main, so they go straight into Resources.
cp -R Sources/shellder/Resources/*.lproj "$APP/Contents/Resources/"
# The open panel, alerts and menus come from macOS and follow the system
# language only for languages the bundle claims to support. Claim every one
# AppKit ships with an empty .lproj, unless a translation already covers it
# (AppKit spells zh_CN what the translation spells zh-Hans).
for l in /System/Library/Frameworks/AppKit.framework/Versions/C/Resources/*.lproj; do
    n=$(basename "$l" .lproj)
    case "$n" in
        zh_CN) alt=zh-Hans ;; zh_TW) alt=zh-Hant ;; zh_HK) alt=zh-Hant-HK ;;
        *) alt=${n//_/-} ;;
    esac
    [ -d "$APP/Contents/Resources/$alt.lproj" ] || mkdir -p "$APP/Contents/Resources/$n.lproj"
done
# App icon from scripts/icon-source.png (or .jpg): cropped to a centred square
# and scaled to every size. Required.
ICONSET=$(mktemp -d)/shellder.iconset
SRC=""
for f in scripts/icon-source.png scripts/icon-source.jpg scripts/icon-source.jpeg; do [ -f "$f" ] && SRC="$f" && break; done
if [ -z "$SRC" ]; then
    echo "error: scripts/icon-source.png (or .jpg) is missing" >&2
    exit 1
fi
mkdir -p "$ICONSET"
W=$(sips -g pixelWidth "$SRC" | awk '/pixelWidth/{print $2}')
H=$(sips -g pixelHeight "$SRC" | awk '/pixelHeight/{print $2}')
SIDE=$(( W < H ? W : H ))
SQ="$(dirname "$ICONSET")/square.png"
sips -s format png --cropToHeightWidth "$SIDE" "$SIDE" "$SRC" --out "$SQ" >/dev/null
for spec in icon_16x16:16 icon_16x16@2x:32 icon_32x32:32 icon_32x32@2x:64 icon_128x128:128 icon_128x128@2x:256 \
            icon_256x256:256 icon_256x256@2x:512 icon_512x512:512 icon_512x512@2x:1024; do
    sips -z "${spec#*:}" "${spec#*:}" "$SQ" --out "$ICONSET/${spec%%:*}.png" >/dev/null
done
# Menu bar version: a monochrome template (alpha only) of the whole picture,
# tinted by macOS like its own status icons. 22 pt, plus @2x.
swift scripts/menubar-template.swift "$SQ" "$APP/Contents/Resources/menubar.png" 22 light
swift scripts/menubar-template.swift "$SQ" "$APP/Contents/Resources/menubar@2x.png" 44 light
# The same picture at 44 pt for the empty main window, dark pixels kept
# this time: on a window background the outlines read better than the fill.
swift scripts/menubar-template.swift "$SQ" "$APP/Contents/Resources/menubar-large.png" 44 dark
swift scripts/menubar-template.swift "$SQ" "$APP/Contents/Resources/menubar-large@2x.png" 88 dark
echo "icon from $SRC (${SIDE}px square)"
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/shellder.icns"
rm -rf "$(dirname "$ICONSET")"
# Signing identity, in order of preference:
#   1. $SHELLDER_SIGN_IDENTITY (e.g. "Apple Development: Name (TEAMID)")
#   2. an Apple-issued identity (Apple Development / Developer ID): carries a
#      Team ID, so keychain items get a stable "teamid:" partition and never
#      ask for the keychain password again after a rebuild
#   3. ad hoc
IDENTITY="${SHELLDER_SIGN_IDENTITY:-}"
if [ -z "$IDENTITY" ]; then
    IDENTITY=$(security find-identity -v -p codesigning 2>/dev/null \
        | grep -oE '"(Apple Development|Developer ID Application): [^"]+"' | head -1 | tr -d '"' || true)
fi
if [ -n "$IDENTITY" ]; then
    echo "signing with: $IDENTITY"
    codesign --force --sign "$IDENTITY" --identifier local.shellder "$APP"
    codesign -dv --verbose=2 "$APP" 2>&1 | grep -E '^TeamIdentifier' || true
else
    echo "no signing identity: signing ad hoc (the keychain will ask again after every rebuild)" >&2
    codesign --force --sign - --identifier local.shellder "$APP"
fi
# The bundle is rebuilt in place, so tell LaunchServices/Finder about the new
# one (otherwise a cached, icon-less registration can linger).
touch "$APP"
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$APP" 2>/dev/null || true
echo "built $APP"

if [[ "${1:-}" == "--install" ]]; then
    mkdir -p "$HOME/Applications"
    # Stop a running copy first (ignore errors), then replace and relaunch.
    pkill -x shellder 2>/dev/null || true
    sleep 1
    rm -rf "$HOME/Applications/Shellder.app"
    cp -R "$APP" "$HOME/Applications/Shellder.app"
    echo "installed to ~/Applications/Shellder.app"
    open "$HOME/Applications/Shellder.app"
fi
