# What the Status tab does on a host (actions.rs puts each together from
# the pieces here, `sudo` first). The arguments are the command's own,
# and `ask` last when it runs on a terminal of its own.

# How a step that needs root runs: `sudo -n`, which refuses rather than
# asks; with `ask`, sudo, whose question is answered like ssh's; as root,
# as it is (a host without sudo at all).
#@ sudo
S="sudo -n"; for a in "$@"; do [ "$a" = ask ] && S=sudo; done; [ "$(id -u)" = 0 ] && S="";

# Unmount the volume `$1`: udisks, plain `umount` (an fstab entry for
# the user), then through sudo; the first refusal is what is said.
#@ um
um(){
  e="";
  if command -v udisksctl >/dev/null 2>&1; then o=$(udisksctl unmount -b "$1" 2>&1) && { printf "%s\n" "$o"; return 0; }; e="$o"; fi;
  o=$(umount "$1" 2>&1) && return 0; [ -n "$e" ] || e="$o";
  $S umount "$1" 2>/dev/null && return 0;
  printf "%s\n" "$e" >&2; return 1;
};

# Add a swap file `$1` of `$2` MB on the host and switch it on, until the
# next boot (nothing goes into fstab). `fallocate` makes it in no time
# where the file system allows swap on such a file; where it does not
# (`swapon` refuses a file with holes), `dd` writes it out. A file that
# would be left half made is removed. sudo is tried first on its own,
# since the steps hide what they say: its refusal is what tells the
# Console that a password would do (`wants_password`).
#@ addswap
f="$1"; n="$2";
e=$($S true 2>&1) || { printf "%s\n" "$e" >&2; exit 1; };
# A name already taken (the file is on as swap, or left on disk by a
# reboot that switched it off) gets a number: /shellder.swap2, 3...
# So several can be added, and nothing is touched or refused.
if [ -e "$f" ]; then b="$f"; i=2; while [ -e "$b$i" ]; do i=$((i+1)); done; f="$b$i"; fi;
mk(){ $S chmod 600 "$f" && $S mkswap "$f" >/dev/null && $S swapon "$f"; };
undo(){ $S swapoff "$f" 2>/dev/null; $S rm -f "$f" 2>/dev/null; };
if $S fallocate -l "${n}M" "$f" 2>/dev/null && mk 2>/dev/null; then :;
else undo; $S dd if=/dev/zero of="$f" bs=1M count="$n" 2>/dev/null && mk || { undo; exit 1; }; fi;
echo "$f: $n MiB of swap until the next boot"

# Switch the swap file `$1` off and remove it.
#@ removeswap
f="$1"; $S swapoff "$f" && $S rm -f "$f" && echo "$f removed"

# Send the signal `$2` to the process `$1`: as this account first, and
# through sudo where that is refused (another user's process).
#@ signal
kill -s "$2" "$1" 2>/dev/null || $S kill -s "$2" "$1"

# Mount the volume `$1`: `diskutil` on macOS; on Linux udisks (which
# picks the mount point, `/media/$USER/LABEL` or `/run/media/...`), an
# fstab entry, and last a mount under `/mnt` through sudo.
#@ mount
d="$1"; if command -v diskutil >/dev/null 2>&1; then exec diskutil mount "$d"; fi; e="";
if command -v udisksctl >/dev/null 2>&1; then o=$(udisksctl mount -b "$d" 2>&1) && { printf "%s\n" "$o"; exit 0; }; e="$o"; fi;
o=$(mount "$d" 2>&1) && exit 0; [ -n "$e" ] || e="$o";
if $S true 2>/dev/null; then m=/mnt/$(basename "$d"); o=$($S mkdir -p "$m" 2>&1 && $S mount "$d" "$m" 2>&1) && { echo "Mounted $d at $m"; exit 0; }; fi;
printf "%s\n" "$e" >&2; exit 1

# Unmount the volume `$1`.
#@ unmount
d="$1"; if command -v diskutil >/dev/null 2>&1; then exec diskutil unmount "$d"; fi;
um "$d"

# Eject the drive `$1`: every volume on it unmounted (`lsblk -l`: a list,
# not the tree whose branches would be taken for part of the name), then
# the drive powered off (`udisksctl power-off`, or `eject`, which also
# opens an optical drive), so that it can be pulled. macOS: `diskutil
# eject`.
#@ eject
d="$1"; if command -v diskutil >/dev/null 2>&1; then exec diskutil eject "$d"; fi;
lsblk -n -l -p -o NAME,MOUNTPOINT "$d" 2>/dev/null | while read -r n m; do [ -n "$m" ] || continue; um "$n" || exit 1; done || exit 1; e="";
case "$d" in /dev/sr*|/dev/cdrom*) o=$(eject "$d" 2>&1) && exit 0;; esac;
if command -v udisksctl >/dev/null 2>&1; then o=$(udisksctl power-off -b "$d" 2>&1) && exit 0; e="$o"; fi;
o=$(eject "$d" 2>&1) && exit 0; [ -n "$e" ] || e="$o"; $S eject "$d" 2>/dev/null && exit 0;
printf "%s\n" "$e" >&2; exit 1
