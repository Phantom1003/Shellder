#!/bin/sh
# Retakes docs/images/main-window.png, the README's picture of the main
# window: an isolated copy of the app with a throwaway ssh config (three
# made-up hosts, nothing from ~/.ssh/config) and a throwaway sshd in Docker
# that the first host, devbox, logs into with a stored password. Nothing in
# the picture is a real host or account.
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
OUT=docs/images/main-window.png
WIDTH=795 HEIGHT=532    # points; the README shows the picture 640px wide
export SHELLDER_SSH_CONFIG=$DIR/config SHELLDER_STATE_DIR=$DIR
LOG=$DIR/shellder.log

[ -d "$SRC" ] || { echo "no app bundle at $SRC (run ./build.sh first)" >&2; exit 1; }
[ "$(uname -s)" = Darwin ] || { echo "macOS only" >&2; exit 1; }

cleanup() {
    pkill -f "$PROC" 2>/dev/null || true
    [ -x "$BIN" ] && "$BIN" del-secret $HOST password >/dev/null 2>&1 || true
    docker rm -f shjt >/dev/null 2>&1 || true
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

docker run -d --name shjt -p 127.0.0.1:2222:22 alpine sh -c \
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

# `open` puts the app in the desktop session (a GUI started from a shell is
# invisible to System Events); the variables go along with --env.
open -n --env SHELLDER_SSH_CONFIG=$SHELLDER_SSH_CONFIG --env SHELLDER_STATE_DIR=$SHELLDER_STATE_DIR "$APP"
for i in $(seq 1 30); do grep -q "$HOST: master established" $LOG 2>/dev/null && break; sleep 1; done
grep -q "$HOST: master established" $LOG || { echo "FAIL: master never came up"; tail -20 $LOG; exit 1; }
sleep 3    # "Connected · 3s", and the list has settled

osascript - "$TEST_ID" "$WIDTH" "$HEIGHT" <<'AS'
on run {bundleId, w, h}
    tell application "System Events"
        set p to first process whose bundle identifier is bundleId
        set frontmost of p to true
        tell window 1 of p
            set position to {120, 120}
            set size to {w as integer, h as integer}
        end tell
    end tell
end run
AS
sleep 1

# The window's id for screencapture -l: the window list of the desktop
# session, the test copy's entry by its pid.
PID=$(pgrep -f "$PROC" | head -1)
WID=$(osascript -l JavaScript - "$PID" <<'JS'
function run(argv) {
    ObjC.import('CoreGraphics');
    const pid = Number(argv[0]);
    const raw = ObjC.deepUnwrap($.CGWindowListCopyWindowInfo($.kCGWindowListOptionOnScreenOnly, 0));
    const list = Array.isArray(raw) ? raw : [];
    const w = list.find(w => w.kCGWindowOwnerPID === pid && w.kCGWindowLayer === 0);
    return w ? String(w.kCGWindowNumber) : '';
}
JS
)
[ -n "$WID" ] || { echo "FAIL: no window found for pid $PID"; exit 1; }
screencapture -x -o -l "$WID" "$OUT"
echo "wrote $OUT ($(sips -g pixelWidth -g pixelHeight "$OUT" | awk '/pixel/ {printf "%s ", $2}')pixels)"
