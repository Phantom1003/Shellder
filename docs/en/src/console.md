# The Console

The Console is a host's window of readings and terminals. It shows what the host is doing (load, memory, disks, processes), its files, its tmux and screen sessions, its jobs, containers and ports, and beside all that it holds terminals on the host in tabs. One window can be switched from host to host, and this machine has a Console too.

{{#include figures/console.html}}

## Opening a Console

Every host has a Console window of its own. There are four ways in:

- The *Console* button on a [host row](main-window.md), there while the pointer is on the row.
- ⌘T in the main window, for the selected host.
- The *Console* button of the Tools card on the host page, beside *Files*.
- The host submenu of the menu bar menu: *Console*, with *Shell* and *Files* beside it.

The window opens with a shell on the host in its first terminal tab.

## The three regions

The window is a top bar, a side bar on the left and the tab area on the right.

| Region | What it holds |
|---|---|
| **Top bar** | The host menu (the host's name) at the left and the settings button (the gear) at the right. The rest of the bar drags the window. |
| **Side bar** | The panel switcher, a strip of icons down the window's left edge, and one panel at a time beside it. |
| **Tab area** | One group of tabs, or several split apart. Each group has its tab bar and shows one of its tabs: a terminal or a preview. |

The settings button opens the Console settings window (⌘,), described in [Console settings](console-settings.md). The tab area has pages of its own: [Tabs, groups and splits](console-tabs.md) and [Inside a terminal](console-terminals.md).

## The side bar and its panels

Each panel button of the panel switcher shows its panel; its name is the button's tooltip. The panel shown is kept per host, on this machine.

| Panel | What it shows |
|---|---|
| [Status panel](console-status.md) | The host's readings, in cards: uptime and load, CPU, memory and swap, GPUs, disks, network, processes, who is logged in. |
| [Files panel](console-files.md) | The host's file tree, with its copies strip. |
| [Sessions panel](console-sessions.md) | The host's tmux and screen sessions. |
| [Jobs panel](console-sessions.md) | Your Slurm and PBS jobs. Its button is there only where the host has a Slurm or PBS client. |
| [Containers panel](console-sessions.md) | The Docker or Podman containers. Its button is there only where the host has Docker or Podman. |
| [Ports panel](console-ports.md) | The forwards, the direct connections and the host's own ports. |

### Folding the side bar

The side bar folds away to the panel switcher, as an editor's side bar does, and the tab area takes the window. Any of these folds it:

- a click on the panel button of the panel that is shown,
- ⌘⇧S,
- dragging the side bar's edge well under its minimum width.

A click on a panel button brings it back, as does ⌘⇧S or dragging the edge back out. Whether the side bar is folded is kept on this machine.

### The side bar visit

The keyboard focus can be taken into the side bar and back without the pointer.

| Key | What it does |
|---|---|
| ⌘⇧L, or ⌘⌥← from the leftmost group | The keyboard focus goes into the side bar. |
| ⌘⇧L again, ⌘⌥→, Esc | The keyboard focus goes back to the tab that had it. |
| Tab, ⇧Tab | The next, the previous panel. |
| ↑, ↓ | Walk the rows of the Sessions, Jobs, Containers and Ports panels. |
| ↩ | Does what a click on the row does. |
| ⇧↩ | Does what a double click on the row does (a session's tab is kept). |

In the Files panel the file tree has its own keys, the arrows and ↩ among them; see [the Files panel](console-files.md).

A folded side bar is unfolded for the visit and folds again when the keyboard focus leaves. It leaves by itself once a row opened a tab: the new tab takes the keyboard focus. In [immersive mode](console-window.md), where the window shows no side bar, ⌘⌥← from the leftmost group, ⌘⇧L or ⌘⇧S bring it out for a visit in the same way, and it goes back with the keyboard focus.

These are the default keys on macOS; on Linux the defaults are the Ctrl+Shift ones. All of them can be changed, see [Keys](keys.md).

## Switching the window to another host

The host menu lists this machine first, as *Local*, then every host of the host list, each with the colour of its status dot. The host the window shows is checked.

Picking a host switches the window to it, where it is and as big as it is:

- The side bar reads the new host, and what it keeps per host (the panel shown, the cards chosen) is the new host's.
- New tabs open on the new host.
- The tabs already open stay as they are, each on the host it runs on. A tab that runs on another host than the window shows names its host at its start.
- A host that already has a Console window is not shown twice: that window is brought up instead.

The [host wheel](console-window.md) does the same switch from the keyboard, or with the mouse's two buttons pressed together.

## The Console of this machine

Switched to *Local*, the Console is this machine's:

- The Status panel reads this machine the way it reads a host.
- The Files panel is this machine's own file tree. There is nothing to upload or download; a drag still copies to a host's Console or Files window.
- The Sessions panel lists the tmux and screen sessions running here.
- The Ports panel lists this machine's listening ports, opened at `127.0.0.1` with no forward.
- A new tab is your login shell, started as a terminal application starts it, with no ssh in between.

A command of the Status panel that wants a password (a signal to another account's process, say) asks in the [prompt window](prompt-window.md), as on a host.

## How the readings are obtained

The readings come from one shell script, run on the host.

- On Linux it uses `/proc`, `df -P`, `ps`, `who`, `ss`, `ip`, `tmux list-sessions`, `screen -ls`, `docker ps`, `squeue` and `qstat -f`.
- A macOS host answers through `sysctl`, `vm_stat`, `top`, `netstat`, `ifconfig` and `lsof`.

Only the parts of the Status panel that are shown are in the script, so a card switched off with the card picker costs the host nothing.

Who is logged in is what `who` says, from utmp. A Linux that keeps no utmp (Ubuntu 26.04 keeps its logins in systemd-logind alone) answers through `loginctl` instead. The two do not count the same: utmp has a line for each terminal, while logind has a session for each ssh connection, however many terminals it carries, and also for a connection that carries none. Shellder's own master to the host is one of those.

## No master needed

Neither the Console nor the [Files window](files-window.md) needs the master. When the master is up they go through it, and then without a login. Otherwise they log in on their own (see [Without a socket](without-a-socket.md)).

That login is the same as a master's: ssh asks on a controlling terminal that only Shellder reads, and the answer comes from the [keychain](keychain.md) or the [prompt window](prompt-window.md). What is typed into the Console's terminals never reaches a password question of that login.

## More about the Console

- [The Status panel](console-status.md)
- [The Files panel and preview tabs](console-files.md)
- [Sessions, jobs and containers](console-sessions.md)
- [The Ports panel](console-ports.md)
- [Tabs, groups and splits](console-tabs.md)
- [Inside a terminal](console-terminals.md)
- [The host wheel, immersive mode and the quick terminal](console-window.md)
- [Console settings](console-settings.md)
- [Keys](keys.md)
- [Terms](terms.md)
