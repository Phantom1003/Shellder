# The process tree, after common.sh: who runs it (name and uid) and its
# own pids, which are left out of the tree; the roots, by one of the
# pieces after `me` (the session's name is `$1`, so that it needs no
# quoting of its own inside the single quotes; the container tool `$2`);
# then every process with its parent, as the header names the columns
# (`TT` or `TTY`, `COMMAND` or `ARGS`). The uid is what tells this
# account's processes: procps cuts a long name in the user column to
# `jinyanx+` and BusyBox to eight letters. BusyBox has neither the uid
# nor the cpu column, so the last `ps` is the one it answers.

#@ me
S me; id -un 2>/dev/null; echo $$ $PPID;
S uid; id -u 2>/dev/null;
S roots;

#@ tmux
tmux list-panes -s -t "=$1" -F "#{pane_pid}" 2>/dev/null;

#@ screen
echo "$1";

# On a Docker Desktop the container's processes are in a VM and not in
# this `ps` at all.
#@ container
"$2" inspect -f "{{.State.Pid}}" "$1" 2>/dev/null;

# The ssh logins of this account, which `parse_tree` picks.
#@ logins
echo logins;

#@ ps
S ps;
ps -eo pid,ppid,uid,user,tty,pcpu,pmem,args 2>/dev/null || ps -eo pid,ppid,user,tty,pcpu,pmem,args 2>/dev/null || ps -eo pid,ppid,user,tty,args 2>/dev/null;
true
