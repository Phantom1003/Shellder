# The Status panel

The Status panel shows what the host is doing, in cards: uptime and load, CPU, memory and swap, the GPUs, the disks, the network, the busiest processes and who is logged in. The readings are taken again every few seconds. Some cards also act on the host: add swap, mount and eject a removable drive, send a process a signal.

{{#include figures/console-status.html}}

## The summary card

The first card names the host and sums it up: its host name, its system and kernel, how long it has been up, its load averages, the number of CPUs and how many are logged in.

## The Usage card

The Usage card has a gauge each for CPU, memory and swap: a label, a value and a bar that fills with it. The bar turns orange from 70 % and red from 90 %.

The card's title carries the controls for the whole panel:

| Control | What it does |
|---|---|
| Interval picker | How often the readings are taken: *every 2s*, *5s*, *10s* or *30s*. Kept on this machine. |
| Card picker (the eye) | Which cards are shown: *CPU*, *Memory and swap*, *GPUs*, *Disks*, *Network*, *Processes*, *Logged in*. The menu stays open for the next choice. Kept per host, on this machine. |
| Refresh button | Reads now. ⌘R does the same from anywhere in the window. |

A card switched off is not read either, so the host is spared its commands: `top`'s one-second sample on macOS, `nvidia-smi`, a `df` that waits on a dead network mount, `ps` and its sort. That matters on a host that is slow to answer or busy with something else.

When a reading fails, the panel shows the error with a *Try again* button in place of the cards, and stops reading until you press it or the host's master comes back up.

### Units

- Memory, swap and a GPU's memory are in binary units (GiB, 1024-based), as the kernel and the system's own tools count them.
- Disks, files and network traffic are in decimal units (GB), as Finder and the drive makers count them.

### Adding and removing a swap file

On a Linux host the Swap gauge has a + that adds a swap file. It asks for the size in GiB and for the file.

- The file is made with `fallocate` (`dd` where the file system will not swap on such a file), `mkswap` and `swapon`.
- It is gone at the next boot: nothing is written to fstab.
- The file is `/shellder.swap`. A name already taken gets a number (`/shellder.swap2`, `3`, …), so several can be added.
- Its name carries "shellder". That is how the Status panel tells it from the host's own swap.

Each swap file added this way is listed under the Swap gauge with a button that switches it off and removes it, after asking. What is in it goes back into memory first.

The host wants root for either. Where the account has `sudo` without a password, that is used. Otherwise `sudo` asks for one, and the question comes to the [prompt window](prompt-window.md), like ssh's own (see [When the host wants a password](#when-the-host-wants-a-password)).

## The GPUs card

Where the host has `nvidia-smi`, each GPU has its name, a *Load* gauge and a *Memory* gauge.

## The Disks card

Each mounted disk has a gauge: its mount point, the free space, the device and how much of the total is used. A double click on a disk shows it in the [Files panel](console-files.md).

### Removable drives

A volume on a removable drive (USB, a card, a disc; `lsblk` says which, `diskutil` on macOS) has two buttons:

| Button | What it does |
|---|---|
| Unmount | Unmounts the volume. |
| Eject | Unmounts every volume on the drive and powers it off, so that it can be pulled. |

The drive's volumes that are not mounted are listed under the disks, each with a mount button.

On Linux the action is tried three ways, in this order, and what the host answers is shown:

1. udisks (`udisksctl`),
2. plain `mount` or `umount` (an fstab entry with `user`),
3. `sudo -n`, where the account has `sudo` without a password.

udisks lets a user at the host's own console mount and power off. An ssh session is not that, so over ssh udisks asks polkit, which wants an administrator's password. Unmounting what you mounted yourself needs none.

### When the host wants a password

When the host would do the action for a password (polkit's or `sudo`'s), the action is run again on a terminal of Shellder's own, and what the host asks on it is answered the way ssh's questions are:

- with the password stored for the host, when there is one and the host takes it;
- otherwise in the [prompt window](prompt-window.md), which says what the action is (`mount /dev/sdb1`, `kill -TERM 1234`) above the question text. The password typed there can be saved as the host's.

Cancelling the question ends the action. Nothing opens in a tab.

### Sparing the question

A polkit rule on the host spares the question by letting the account's group through, here `plugdev`:

```
# /etc/polkit-1/rules.d/50-mount-over-ssh.rules
polkit.addRule(function (action, subject) {
    if ((action.id == "org.freedesktop.udisks2.filesystem-mount" ||
         action.id == "org.freedesktop.udisks2.filesystem-unmount-others" ||
         action.id == "org.freedesktop.udisks2.eject-media" ||
         action.id == "org.freedesktop.udisks2.power-off-drive") &&
        subject.isInGroup("plugdev"))
        return polkit.Result.YES;
});
```

Without such a rule, an fstab entry with `user` for the volume, or passwordless `sudo` for `mount`, `umount` and `eject`, does the same.

## The Network card

The Network card lists the interfaces that have an address: IPv4 and global IPv6, not the loopback and not link-local ones. Each row shows the interface's addresses and what came in and went out per second since the reading before.

The numbers are from the host's counters: `/proc/net/dev`, or `netstat -ibn` on macOS and the BSDs. The addresses are from `ip -o addr` or `ifconfig`.

## The Processes card

The Processes card is either the busiest table or the process tree. The button in the card's title turns one into the other, and the choice is kept on this machine.

### The busiest table

The table has the twenty busiest processes by CPU and the twenty busiest by memory, with the columns *PID*, *User*, *CPU %*, *Mem %* and *Cmd*.

| Action | What it does |
|---|---|
| Click on a column heading | Sorts by that column; a second click sorts the other way. Kept on this machine. |
| Click on a cell | Copies what the cell holds, whole: the full command line that the column cuts short, not the row. |
| Right click on a row | Opens the signal menu. |

### The signal menu

The signal menu offers the process a signal: *Terminate (SIGTERM)*, *Kill (SIGKILL)*, *Interrupt (SIGINT)*, *Hang up (SIGHUP)*, *Stop (SIGSTOP)* or *Continue (SIGCONT)*. A dialog asks before it is sent.

The signal is sent with `kill` as this account, or through `sudo` for another user's process. The password is asked for the way a mount's is (see [When the host wants a password](#when-the-host-wants-a-password)).

### The process tree

The process tree shows the processes under the window's active tab. What it is rooted at depends on the tab:

| Active tab | The tree shows |
|---|---|
| A tmux session tab | The panes of its tmux session. |
| A screen session tab | Its screen session. |
| A container tab | Its container, where the host's `ps` sees the container's processes (not on a Docker Desktop). |
| A shell tab, or anything else | The ssh logins of this account: the shells under an `sshd` session process and everything they run. The Console's own readings are left out. |

What the tree is rooted at is the tooltip of the card's title.

The tree differs from the table in a few ways:

- It has no user column. A process run as another user than its parent (a `sudo`) names that user before its command.
- A command shows its program by name, not by directory. The tooltip and a copy give it whole.
- It keeps its own order; the headings do not sort it.

The tree follows the tab selected and is read with every reading, in place of the busiest. A click on a cell copies it and the signal menu offers the same signals, as in the table.

## The Logged in card

The Logged in card has a row for each login: the user, the terminal, since when and from where. Where this list comes from, and why its count differs between hosts, is explained in [The Console](console.md).
