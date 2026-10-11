#!/bin/sh
# The Files window's model without a window: two throwaway sshd's in Docker,
# and the app's own code driven the way the panes do (listings, drops in
# both directions and between two hosts, new folders and files, deleting,
# queueing, cancelling, the transfer list, the dates and the order of the
# rows). The first host lists with BusyBox's ls, the second with GNU's, and
# this machine's own ls stands in for the BSDs. The checks themselves are
# src-tauri/tests/files_window.rs, an ignored test this script runs once
# the hosts are up, followed by the session test against the first host
# without its ControlPath (BusyBox's shell on the session's protocol, the
# copies logging in on their own). See AGENTS.md.
# Usage: Tests/files-window.sh
set -eu
DIR=/tmp/shjt-files
export SHELLDER_SSH_CONFIG=$DIR/config SHELLDER_STATE_DIR=$DIR

cleanup() {
    ssh -F $DIR/config -O exit shjt-files >/dev/null 2>&1 || true
    ssh -F $DIR/config -O exit shjt-files-b >/dev/null 2>&1 || true
    docker rm -f shjt-files shjt-files-b >/dev/null 2>&1 || true
    rm -rf $DIR
}
trap cleanup EXIT
cleanup
mkdir -p $DIR/here
ssh-keygen -q -t ed25519 -N '' -f $DIR/id -C shjt-files
for pair in "shjt-files 2225" "shjt-files-b 2226"; do
    set -- $pair
    docker run -d --name $1 -p 127.0.0.1:$2:22 alpine sh -c \
        'apk add -q openssh && ssh-keygen -A && adduser -D t && echo t:pw | chpasswd && /usr/sbin/sshd -D -e' >/dev/null
    cat >> $DIR/config <<CFG
Host $1
    HostName 127.0.0.1
    Port $2
    User t
    IdentityFile $DIR/id
    IdentitiesOnly yes
    ControlMaster auto
    ControlPath $DIR/cm-%C
    StrictHostKeyChecking no
    UserKnownHostsFile /dev/null
    LogLevel ERROR
CFG
done
for name in shjt-files shjt-files-b; do
    for i in $(seq 1 60); do docker exec $name pgrep -x sshd >/dev/null 2>&1 && break; sleep 1; done
    docker exec $name sh -c 'mkdir -p /home/t/.ssh && chmod 700 /home/t/.ssh && chown t:t /home/t/.ssh'
    docker exec -i $name sh -c 'cat > /home/t/.ssh/authorized_keys && chmod 600 /home/t/.ssh/authorized_keys && chown t:t /home/t/.ssh/authorized_keys' < $DIR/id.pub
done
# A tree with a hidden entry, a directory with a space in its name, a
# symlinked directory and something big enough to show progress.
docker exec shjt-files su t -c 'cd $HOME && mkdir -p Shellder/sub "my dir" && echo one > Shellder/one.txt \
    && echo two > Shellder/sub/two.txt && echo h > Shellder/.hidden && ln -sf Shellder link \
    && dd if=/dev/zero of=Shellder/big.bin bs=1M count=200 2>/dev/null \
    && dd if=/dev/zero of=huge.bin bs=1M count=600 2>/dev/null'
docker exec shjt-files su t -c 'cd $HOME && echo remote > clash.txt && echo remote > two.txt'
docker exec shjt-files-b su t -c 'cd $HOME && mkdir -p Shellder'
docker exec shjt-files-b apk add -q coreutils
# Rows to sort: known sizes and dates, a directory's own date set after
# what is in it. GNU's touch takes a fraction of a second too.
for pair in "shjt-files 00" "shjt-files-b 00.5"; do
    set -- $pair
    docker exec $1 su t -c "mkdir -p \$HOME/sorted/adir \$HOME/sorted/zdir && cd \$HOME/sorted \
        && printf abc > old.txt && printf 0123456789 > new.txt && head -c 1000 /dev/zero > mid.bin \
        && echo a > adir/a.txt && echo b > adir/b.txt \
        && touch -d '2024-01-02 03:04:05' old.txt && touch -d '2025-06-01 12:00:$2' new.txt \
        && touch -d '2025-01-01 00:00:00' mid.bin && touch -d '2022-05-05 00:00:00' adir/a.txt \
        && touch -d '2022-06-06 00:00:00' adir/b.txt && touch -d '2023-01-01 00:00:00' adir \
        && touch -d '2025-12-01 00:00:00' zdir"
done
printf 'local\n' > $DIR/here/clash.txt
printf 'local\n' > $DIR/here/two.txt
# The first host once more without a ControlPath: a Files window on it
# lists through a session of its own, so BusyBox's shell sees the
# session's protocol, and its copies log in on their own
# (src-tauri/tests/session.rs, the ignored test).
cat >> $DIR/config <<CFG
Host shjt-files-plain
    HostName 127.0.0.1
    Port 2225
    User t
    IdentityFile $DIR/id
    IdentitiesOnly yes
    StrictHostKeyChecking no
    UserKnownHostsFile /dev/null
    LogLevel ERROR
CFG
ssh -F $DIR/config -M -f -N shjt-files
ssh -F $DIR/config -M -f -N shjt-files-b

cd "$(dirname "$0")/../src-tauri"
# Both run whatever the first one says; the exit status is the worse of the two.
rc=0
SHELLDER_FILES_TEST_DIR=$DIR cargo test --test files_window -- --ignored --nocapture --test-threads=1 || rc=1
SHELLDER_FILES_TEST_DIR=$DIR cargo test --test session -- --ignored --nocapture --test-threads=1 || rc=1
exit $rc
