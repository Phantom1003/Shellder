# The reading of a host, after common.sh. What a command cannot say is
# left empty. Linux reads `/proc`, macOS its `sysctl`s, `vm_stat` and
# `top` (which costs a second, the sample it takes); `df -P` and `ps -eo`
# are what everyone has (BusyBox's `ps` lacks the cpu column, so that
# section is empty there). `lsblk` (macOS: `diskutil list external`) says
# which disks are removable and which of their volumes are not mounted.
#
# Each piece is tagged with the part of the Console it serves: a card of
# the Status tab (`PARTS`), a tab (`TAB_PARTS`), or `always`. A part the
# window does not show is not read, which spares the host its commands
# (`top`'s second, `nvidia-smi`, a `df` that stalls on a dead NFS mount,
# `ps` and its sort, the batch scheduler's RPCs).
#
# `$timed` runs what may hang (a dead NFS mount, a docker daemon that does
# not answer, a scheduler that is down) for eight seconds at most, where
# the host has a `timeout` that takes `timeout 8 command`: not macOS, not
# a BusyBox before 1.30 (`timeout -t 8`).

#@ always
tab=$(printf "\t");
timed=""; timeout 1 true 2>/dev/null && timed="timeout 8";
S hostname; hostname 2>/dev/null;
S uname; uname -sr 2>/dev/null;
# In a subshell: a syntax error in it would end a POSIX sh, and every
# section after it with it.
S os;
if [ -r /etc/os-release ]; then
  (. /etc/os-release 2>/dev/null; printf "%s\n" "$PRETTY_NAME");
elif command -v sw_vers >/dev/null 2>&1; then
  printf "macOS %s\n" "$(sw_vers -productVersion 2>/dev/null)";
fi;
S now; date +%s 2>/dev/null;
S uptime; cat /proc/uptime 2>/dev/null || sysctl -n kern.boottime 2>/dev/null;
S load; cat /proc/loadavg 2>/dev/null || sysctl -n vm.loadavg 2>/dev/null;
S ncpu; nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || getconf _NPROCESSORS_ONLN 2>/dev/null;

#@ cpu
S stat; head -n 1 /proc/stat 2>/dev/null;
S topcpu; [ -r /proc/stat ] || top -l 2 -n 0 -s 1 2>/dev/null | grep "CPU usage" | tail -n 1;

#@ memory
S mem; cat /proc/meminfo 2>/dev/null || { sysctl -n hw.memsize hw.pagesize 2>/dev/null; vm_stat 2>/dev/null; };
S swap; [ -r /proc/meminfo ] || sysctl -n vm.swapusage 2>/dev/null;
S swaps; cat /proc/swaps 2>/dev/null;

#@ disks
S df; $timed df -P -k 2>/dev/null;
S blk;
if command -v lsblk >/dev/null 2>&1; then
  lsblk -b -p -P -o NAME,TYPE,FSTYPE,SIZE,LABEL,MOUNTPOINT,RM,HOTPLUG,TRAN,PKNAME 2>/dev/null || lsblk -b -p -P -o NAME,TYPE,FSTYPE,SIZE,LABEL,MOUNTPOINT,RM 2>/dev/null;
elif command -v diskutil >/dev/null 2>&1; then
  echo diskutil; diskutil list external 2>/dev/null;
fi;

#@ network
S net; cat /proc/net/dev 2>/dev/null || netstat -ibn 2>/dev/null;
S addr; ip -o addr 2>/dev/null || ifconfig 2>/dev/null;

#@ gpu
S gpu; command -v nvidia-smi >/dev/null 2>&1 && $timed nvidia-smi --query-gpu=index,name,utilization.gpu,memory.used,memory.total --format=csv,noheader,nounits 2>/dev/null;

# The busiest by CPU and the busiest by memory, so that the page's table
# sorts by either and means something; the same process in both is
# listed once.
#@ processes
S ps; p=$(ps -eo pid,user,pcpu,pmem,comm 2>/dev/null | tail -n +2);
printf "%s\n" "$p" | sort -k3 -rn 2>/dev/null | head -n 20;
printf "%s\n" "$p" | sort -k4 -rn 2>/dev/null | head -n 20;

# Who is logged in, from where the host keeps it: utmp, which `who`
# reads, or on a Linux without one (Ubuntu 26.04 keeps its logins in
# systemd-logind alone, and its `who` says nobody) logind's sessions,
# one block of properties each. The two do not count the same: utmp has
# a line a terminal, logind a session an ssh connection, however many
# terminals it carries, and one with none too (the app's own master).
# awk's program is in double quotes, since the script has no single
# quote: `\$1` is its field.
#@ logins
if [ -e /run/utmp ] || [ -e /var/run/utmp ] || [ -z "$(command -v loginctl)" ]; then
S who; who 2>/dev/null;
else
S logind; ids=$(loginctl list-sessions --no-legend 2>/dev/null | awk "{ print \$1 }");
[ -n "$ids" ] && loginctl show-session $ids -p Id -p Name -p Class -p TTY -p RemoteHost -p Timestamp 2>/dev/null;
fi;

# tmux: one line per window, its session's fields first (every session
# has a window); `-u`, or a host without a UTF-8 locale gets every tab
# (and every letter beyond ASCII) as `_`. screen: `screen -ls`, then the
# current directory of the
# first process in each session (Linux, through /proc). VNC: the servers
# among the processes, and `screensharing` on a Mac whose Screen Sharing
# or Remote Management is on (`launchctl print`, which any user may ask
# for: launchd holds its port as root until a viewer connects).
#@ sessions
S tmux;
command -v tmux >/dev/null 2>&1 && {
  echo present;
  tmux -u list-windows -a -F "#{session_name}${tab}#{session_attached}${tab}#{session_activity}${tab}#{window_index}${tab}#{window_name}${tab}#{window_active}${tab}#{pane_current_command}${tab}#{window_panes}${tab}#{pane_current_path}" 2>/dev/null;
};
S screen;
command -v screen >/dev/null 2>&1 && {
  echo present; l=$(screen -ls 2>/dev/null); printf "%s\n" "$l";
  S screencwd;
  for p in $(printf "%s\n" "$l" | sed -n "s/^[ $tab]*\([0-9][0-9]*\)\..*/\1/p"); do
    c=$(pgrep -P "$p" 2>/dev/null | head -n 1);
    [ -n "$c" ] && printf "%s\t%s\n" "$p" "$(readlink /proc/$c/cwd 2>/dev/null)";
  done;
};
S vnc; ps -eo user,pid,args 2>/dev/null | grep -E "(^| |/)(Xvnc|Xvnc4|Xtigervnc|Xtightvnc|Xrealvnc|x11vnc|vncserver-x11|vncserver-x11-core) ";
launchctl print system/com.apple.screensharing >/dev/null 2>&1 && echo screensharing;

# The jobs of this account: one `squeue` (tabs between the fields, since
# a job's name may hold any other character), or a `qselect` and one
# `qstat -f` of what it found. Both ask the scheduler, which a cluster
# does not want asked in a loop: only while the window shows the jobs.
#@ jobs
S slurm;
command -v squeue >/dev/null 2>&1 && {
  echo present;
  $timed squeue -h -u "$(id -un)" -o "%i${tab}%j${tab}%T${tab}%M${tab}%D${tab}%R${tab}%P${tab}%Z" 2>/dev/null;
};
S pbs;
command -v qstat >/dev/null 2>&1 && command -v qselect >/dev/null 2>&1 && {
  echo present; ids=$($timed qselect -u "$(id -un)" 2>/dev/null);
  [ -n "$ids" ] && $timed qstat -f $ids 2>/dev/null;
};

# The containers, then the bind mounts of each, by the ids of that list.
#@ containers
S docker; D=docker; command -v docker >/dev/null 2>&1 || D=podman;
command -v $D >/dev/null 2>&1 && {
  echo present $D;
  l=$($timed $D ps --format "{{.ID}}${tab}{{.Names}}${tab}{{.Image}}${tab}{{.Status}}${tab}{{.Ports}}" 2>/dev/null);
  printf "%s\n" "$l"; ids=$(printf "%s\n" "$l" | cut -f 1);
  [ -n "$ids" ] && $timed $D inspect --format "M${tab}{{.Name}}{{range .Mounts}}{{if eq .Type \"bind\"}}${tab}{{.Source}}{{end}}{{end}}" $ids 2>/dev/null;
};

# What listens on TCP: `ss` (no -H, which an older one refuses); without
# it a `netstat` that knows `-l` (net-tools, BusyBox: "only servers");
# and last `lsof`, told not to stat what it finds (-b), which would hang
# on a dead NFS mount. macOS has no `ss` and a `netstat` that lists no
# TCP here. BusyBox's own `lsof` takes no options and lists every open
# file: only the LISTEN lines are kept.
#@ ports
S ports; o=$(ss -ltnp 2>/dev/null);
if [ -n "$o" ]; then
  printf "%s\n" "$o";
elif o=$(netstat -ltnp 2>/dev/null || netstat -ltn 2>/dev/null) && printf "%s\n" "$o" | grep -q "only servers"; then
  printf "%s\n" "$o";
elif command -v lsof >/dev/null 2>&1; then
  echo lsof; $timed lsof -b -w -nP -iTCP -sTCP:LISTEN 2>/dev/null | grep LISTEN;
fi;

#@ always
true
