#!/bin/sh
# Retakes the README's pictures, docs/images/main-window.png (the main
# window) and docs/images/console-window.png (the Console of the first
# host, opened with ⌘T): an isolated copy of the app with a throwaway ssh
# config (three made-up hosts, nothing from ~/.ssh/config) and a throwaway
# sshd in Docker that the first host, devbox, logs into with a stored
# password. Nothing in the picture is a real host or account. Each capture
# is put on a blurred backdrop with a shadow by readme-frame.swift
# (CoreImage, nothing to install).
#
# Runs on macOS, from a desktop session: the test copy is re-signed with the
# Apple Development identity (so the vault opens without a dialog), shown
# through `open`, sized with System Events and captured with screencapture,
# none of which works over ssh. The password is stored under the alias
# "devbox" in the real vault for the duration of the run and deleted again.
#
# Usage: scripts/readme-screenshot.sh [path/to/Shellder.app]   (default: build/Shellder.app)
set -eu
cd "$(dirname "$0")/.."
SRC=${1:-build/Shellder.app}
HOST=devbox
DIR=/tmp/shjt           # short: ControlPath is a unix socket path
APP=$DIR/ShellderTest.app
BIN=$APP/Contents/MacOS/shellder
PROC="ShellderTest.app/Contents/MacOS/shellder"   # pgrep pattern: `open` runs it as /private/tmp/..., so not ^$BIN
TEST_ID=local.shellder.test
MAIN_OUT=docs/images/main-window.png
MAIN_W=795 MAIN_H=532         # points; framed, the README shows it 720px wide
CONSOLE_OUT=docs/images/console-window.png
CONSOLE_W=1000 CONSOLE_H=640  # the window's default size; shown 880px wide
export SHELLDER_SSH_CONFIG=$DIR/config SHELLDER_STATE_DIR=$DIR
LOG=$DIR/shellder.log

[ -d "$SRC" ] || { echo "no app bundle at $SRC (run ./build.sh first)" >&2; exit 1; }
[ "$(uname -s)" = Darwin ] || { echo "macOS only" >&2; exit 1; }

cleanup() {
    pkill -f "$PROC" 2>/dev/null || true
    [ -x "$BIN" ] && "$BIN" del-secret $HOST password >/dev/null 2>&1 || true
    docker rm -f shjt >/dev/null 2>&1 || true
    defaults delete $TEST_ID >/dev/null 2>&1 || true
    rm -rf $DIR
}
trap cleanup EXIT
cleanup
mkdir -p $DIR

# The test copy: another bundle id, so window lookups do not land on the
# user's running Shellder, signed with the same identity and identifier as
# the real app so it reads the vault without a keychain dialog.
IDENTITY="${SHELLDER_SIGN_IDENTITY:-}"
if [ -z "$IDENTITY" ]; then
    IDENTITY=$(security find-identity -v -p codesigning 2>/dev/null \
        | grep -oE '"(Apple Development|Developer ID Application): [^"]+"' | head -1 | tr -d '"' || true)
fi
[ -n "$IDENTITY" ] || { echo "no Apple signing identity (see build.sh)" >&2; exit 1; }
cp -R "$SRC" "$APP"
/usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier $TEST_ID" "$APP/Contents/Info.plist"
codesign --force --sign "$IDENTITY" --identifier local.shellder "$APP"

docker run -d --name shjt --hostname $HOST -p 127.0.0.1:2222:22 alpine sh -c \
    'apk add -q openssh && ssh-keygen -A && adduser -D alice && echo alice:pw | chpasswd && /usr/sbin/sshd -D -e' >/dev/null
cat > $DIR/config <<CFG
Host $HOST
    HostName 127.0.0.1
    Port 2222

Host gpu-01
    HostName 10.20.0.7

Host lab
    HostName lab.example.edu

Host *
    User alice
    ControlMaster auto
    ControlPath $DIR/cm-%C
    StrictHostKeyChecking no
    UserKnownHostsFile /dev/null
    LogLevel ERROR
CFG
# docker-proxy answers on the port before apk has finished; wait for sshd itself.
for i in $(seq 1 60); do docker exec shjt pgrep -x sshd >/dev/null 2>&1 && break; sleep 1; done
sleep 1
printf 'pw\n' | "$BIN" add-secret $HOST password >/dev/null
"$BIN" lock $HOST >/dev/null

# English in the picture whatever this Mac's language: the pages format
# times with the WebView's language, which is this per-app override.
defaults write $TEST_ID AppleLanguages -array en

# `open` puts the app in the desktop session (a GUI started from a shell is
# invisible to System Events); the variables go along with --env.
open -n --env SHELLDER_SSH_CONFIG=$SHELLDER_SSH_CONFIG --env SHELLDER_STATE_DIR=$SHELLDER_STATE_DIR "$APP"
for i in $(seq 1 30); do grep -q "$HOST: master established" $LOG 2>/dev/null && break; sleep 1; done
grep -q "$HOST: master established" $LOG || { echo "FAIL: master never came up"; tail -20 $LOG; exit 1; }
sleep 3    # "Connected · 3s", and the list has settled

PID=$(pgrep -f "$PROC" | head -1)

# Place the test copy's window with this title and capture it. The id for
# screencapture -l comes from the window list of the desktop session: the
# entry with the test copy's pid and that name.
capture() {    # title width height output
    osascript - "$TEST_ID" "$1" "$2" "$3" <<'AS'
on run {bundleId, title, w, h}
    tell application "System Events"
        set p to first process whose bundle identifier is bundleId
        set frontmost of p to true
        tell (first window of p whose name is title)
            set position to {120, 120}
            set size to {w as integer, h as integer}
        end tell
    end tell
end run
AS
    sleep 1
    WID=$(osascript -l JavaScript - "$PID" "$1" <<'JS'
function run(argv) {
    ObjC.import('CoreGraphics');
    const pid = Number(argv[0]), title = argv[1];
    const raw = ObjC.deepUnwrap($.CGWindowListCopyWindowInfo($.kCGWindowListOptionOnScreenOnly, 0));
    const list = Array.isArray(raw) ? raw : [];
    const w = list.find(w => w.kCGWindowOwnerPID === pid && w.kCGWindowLayer === 0 && w.kCGWindowName === title);
    return w ? String(w.kCGWindowNumber) : '';
}
JS
)
    [ -n "$WID" ] || { echo "FAIL: no window '$1' for pid $PID"; exit 1; }
    screencapture -x -o -l "$WID" "$4"
    swift scripts/readme-frame.swift "$4" "$4"    # on a blurred backdrop, with a shadow
    echo "wrote $4 ($(sips -g pixelWidth -g pixelHeight "$4" | awk '/pixel/ {printf "%s ", $2}')pixels)"
}

capture Shellder $MAIN_W $MAIN_H $MAIN_OUT

# The Console of the selected host (devbox, the first row): ⌘T in the main
# window. Its title is the host's name. A few seconds for the first
# readings of the Status tab and the shell's prompt.
osascript -e 'tell application "System Events" to keystroke "t" using command down'
for i in $(seq 1 30); do
    osascript -e "tell application \"System Events\" to exists (first window of (first process whose bundle identifier is \"$TEST_ID\") whose name is \"$HOST\")" 2>/dev/null | grep -q true && break
    sleep 1
done
sleep 6
capture $HOST $CONSOLE_W $CONSOLE_H $CONSOLE_OUT
