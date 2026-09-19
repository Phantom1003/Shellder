#!/bin/sh
# An unlocked host whose master the server closes right after login must
# switch off with the error instead of retrying, and the keep-alive mode must
# stay as it was (there is no automatic escalation). Runs a throwaway sshd in
# Docker and an isolated GUI instance, see AGENTS.md.
# Usage: Tests/drop-unlocked.sh [path/to/shellder]
set -eu
BIN=${1:-build/Shellder.app/Contents/MacOS/shellder}
HOST=shjt-drop
DIR=/tmp/shjt
export SHELLDER_SSH_CONFIG=$DIR/config SHELLDER_STATE_DIR=$DIR SHELLDER_PREFS_SUITE=local.shellder.test
LOG=$DIR/shellder.log

cleanup() {
    [ -n "${APP:-}" ] && kill "$APP" 2>/dev/null || true
    "$BIN" del-secret $HOST password >/dev/null 2>&1 || true
    "$BIN" unlock $HOST >/dev/null 2>&1 || true
    defaults delete $SHELLDER_PREFS_SUITE idleModes >/dev/null 2>&1 || true
    docker rm -f shjt >/dev/null 2>&1 || true
    rm -rf $DIR
}
trap cleanup EXIT
cleanup
mkdir -p $DIR
docker run -d --name shjt -p 127.0.0.1:2222:22 alpine sh -c \
    'apk add -q openssh && ssh-keygen -A && adduser -D t && echo t:pw | chpasswd && /usr/sbin/sshd -D -e' >/dev/null
cat > $DIR/config <<CFG
Host $HOST
    HostName 127.0.0.1
    Port 2222
    User t
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
"$BIN" test $HOST | grep -q 'login OK' || { echo "FAIL: one-shot login"; "$BIN" test $HOST -v 2>&1 | tail -5; exit 1; }

"$BIN" lock $HOST >/dev/null
: > $LOG    # the GUI truncates it anyway; make sure no earlier run's lines are read
"$BIN" --background & APP=$!
for i in $(seq 1 30); do grep -q "$HOST: master established" $LOG 2>/dev/null && break; sleep 1; done
grep -q "$HOST: master established" $LOG || { echo "FAIL: master never came up"; tail -20 $LOG; exit 1; }
"$BIN" unlock $HOST >/dev/null
sleep 4
grep -q "locked set changed outside the app" $LOG || { echo "FAIL: unlock not picked up"; exit 1; }

# Kill the server side of the session: the client sees "closed by remote host".
docker exec shjt ps -o pid,user,args
docker exec shjt sh -c 'pkill -f "^sshd(-session)?: t"' || echo "pkill rc=$?"
sleep 6
echo "--- log after the drop"
grep "$HOST" $LOG | tail -6
if grep -q "$HOST: switched off — connection dropped" $LOG && ! grep -qi "retry" $LOG \
   && [ "$(grep -c "$HOST: spawned master" $LOG)" = 1 ] \
   && ! defaults read local.shellder.test idleModes 2>/dev/null | grep -q "$HOST"; then
    echo "PASS: unlocked host switched off with the error, no retry, keep-alive mode unchanged"
else
    echo "FAIL: unlocked host retried or did not switch off"; exit 1
fi
