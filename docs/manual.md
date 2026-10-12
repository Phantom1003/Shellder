# shellder manual

shellder keeps `ssh -M -N` master connections open in the background so that every
later `ssh host`, `scp`, `rsync`, VS Code Remote, … reuses the socket and never
asks for a password again. When a master has to be (re)established it answers
the password / key passphrase / TOTP prompt itself from your keychain, and
falls back to asking you in a dialog for anything it cannot answer (unknown
host keys included). It runs on macOS and Linux. A host without a
`ControlPath`, or any host on a platform whose OpenSSH has no ControlMaster
(Windows), is kept connected all the same, for the port forwards its config
asks for: the same `ssh -N`, only without the socket, so other ssh commands
cannot reuse it. The Console and Files windows then log in on their own, with
shellder answering the prompts, see "Console and Files" below.

## Principles

* Your `~/.ssh/config` is the only configuration. shellder lists every
  `Host` block (following `Include`), resolves it with `ssh -G`, and never
  writes to that file. ControlMaster/ControlPath/ControlPersist come from your
  config. If a host has no `ControlPath`, its connection is kept alive
  without a socket (the host page shows the snippet that would give it one).
* The master runs as `ssh -M -N <alias>` plus a few process-only options
  (`ControlPersist=no` so shellder owns the process, `NumberOfPasswordPrompts=1`,
  `LogLevel=ERROR`, and `ServerAliveInterval` only if your config leaves it at 0).
* Secrets live in the keychain in a single item, the "shellder vault"
  (service `shellder:vault`, a JSON object keyed by host): a generic password
  in the login keychain on macOS, a Secret Service item on Linux. One item
  means the keychain asks for permission at most once.
* Secrets only ever leave the keychain towards an ssh the app itself started.
  Each master gets a pseudo-terminal of its own as its controlling terminal,
  which is where ssh asks its questions (`/dev/tty`); the app reads them off
  that terminal and writes the answer back. There is no helper program and
  no socket, so nothing outside the app can ask it for a secret. The CLI has
  no command that prints a secret or logs in with one.
* State outside the keychain: `~/Library/Application Support/shellder/` on
  macOS, `~/.local/state/shellder/` on Linux (`prefs.json`, the lock, the
  last TOTP code, `shellder.log`), and optionally the login item
  (`~/Library/LaunchAgents/local.shellder.plist`,
  `~/.config/autostart/local.shellder.desktop`).

## The switch and the lock

Every host has a *Connect* switch and a lock button.

**The switch means connect.** Switching it on is one connection attempt.

* If the attempt succeeds the master stays up, and the switch turns green.
  The connection lasts as long as it lasts: when the link drops, the switch
  goes off again.
* If the attempt fails (wrong password, cancelled prompt, declined host key,
  unreachable, no ControlPath) the switch goes back off and nothing is
  retried. The reason is shown in the host list.
* Switching it off closes the master. *Disconnect* does the same, and *Close
  socket* additionally runs `ssh -O exit`, which also works for a master
  started by an interactive `ssh`.

The switch is not remembered: after a restart every host starts off.

The dot next to a host and its switch wear the same colour: yellow while an
attempt or a retry is under way, green once the master is up, blue once it
is up and locked, cyan when the master up is not shellder's own (one left
by an interactive `ssh` with `ControlMaster auto`; the lock does not keep
that one, and *Take over* replaces it). A failed attempt turns the dot red
until the switch goes on again; the switch itself is plain grey whenever it
is off.

**The lock keeps a connection connected.** Locking a host switches it on if
it is off, and from then on shellder brings the connection back by itself: a
dropped link is reconnected with back-off (from 5 s up to 10 min), and a
locked host is switched on again when shellder starts. Only a link that
keeps dying right after connecting gives up after three tries.

The lock goes with the switch. Whatever turns the switch off, whether you or
a failed attempt, clears the lock. Unlocking by hand leaves the connection as
it is, it just stops being brought back.

Locked hosts are the only thing remembered across launches (and what
`shellder lock`, `unlock` and `status` work on).

Some servers close a session-less `ssh -N` connection within seconds. The
master then dies like any other drop (an unlocked host switches off with the
error, a locked one retries and gives up after a few quick failures). If that
is what you see, pick another *keep-alive mode* in the Keep-alive tool on the
host's page. The choice is remembered per host:

| mode | what the server sees | notes |
|---|---|---|
| `-N` (default) | a connection with no session | cleanest, but some servers reject it |
| idle shell | `sshd: you@pts/N → -bash`, an idle login in `w` | indistinguishable from an open terminal, SIGHUP on disconnect |
| cat | `sshd: you@notty → cat` | no pty/login scripts, exits on EOF when the link drops |

## Console and Files

Every host in the list has one way in: the *Console* button on its row,
there while the pointer is on the row (⌘T for the selected host), a
window of shellder's own per host; the host's menu bar submenu offers
the same, with *Shell* (`ssh host` in the application that handles
`ssh://` links; that ssh is not shellder's and asks you itself) and
*Files* beside it. The host's page on the right is about the connection:
its state, the master's controls, the credentials, and a *Tools* card
with the Console and the Files window side by side. The + on the card's
title adds a forward (the same the Console's *Ports* tab shows, see
there) or a direct connection: a port of the host opened at its own
address (the `HostName` of its ssh config) with no tunnel, in the VNC
viewer for a port from 5900 to 5999 and in the browser otherwise. Both
are listed under the buttons and in the Console's *Ports* tab, kept on
this machine, so that a port is a click away without the Console. The button in the page's top left corner
folds the host list away and brings it back (⌘⇧S); the choice is kept
on this machine.

The Console is a column of tabs on the left and terminals in tabs on the
right. The window's top left corner, whichever way the column is, is
the host's name: a menu of the hosts, each with its colour from the
list, and the window switches to the one picked, where it is and as big
as it is (its tabs close with the switch, a tmux or screen session lives
on and a shell ends, and the column reads the new host; a host that
already has a Console window is brought up instead). The column's tabs are a strip of icons down its left edge, each with its
count; a click on the open one folds the column away to the strip, as
an editor's side bar does, and the terminals take the window (so does
⌘⇧S, or dragging the column's edge well under its minimum width); a
click on a tab, or a drag back out, brings it back, and the choice is
kept on this machine. The host's name is then first in the first
terminal tab row. The column:

* *Status*: uptime and load, CPU, memory and swap, the GPUs (`nvidia-smi`,
  where there is one), the disks, the network, the busiest processes and
  who is logged in, read again every few seconds (the interval and the
  time of the last reading are on the Usage card, ⌘R reads now). The eye
  on the Usage card chooses which cards are shown; a card switched off is
  not read either, so the host is spared its commands (`top`'s one-second
  sample on macOS, `nvidia-smi`, a `df` that waits on a dead network
  mount, `ps` and its sort), which matters on a host that is slow to
  answer or busy with something else. The choice is kept per host, on
  this machine. The + on the Swap row (Linux) adds a swap file, made with
  `fallocate` (`dd` where the file system will not swap on such a file),
  `mkswap` and `swapon`, and gone at the next boot: nothing is written to
  fstab. Its name carries "shellder", which is how the Status tab tells
  it from the host's own swap and lists it under the gauge with a button
  that switches it off and removes it. The host wants root for either:
  `sudo` without a password where the account has it, otherwise the
  action opens in a terminal tab, where `sudo` asks. A double click on a
  disk shows it in the *Files* tab. A volume on a removable
  drive (USB, a card, a disc; `lsblk` says which, `diskutil` on macOS)
  has two buttons: unmount it, and eject the drive, which unmounts every
  volume on it and powers it off so that it can be pulled. The drive's
  volumes that are not mounted are listed under the disks with a mount
  button. On Linux this goes through udisks (`udisksctl`), then plain
  `mount`/`umount` (an fstab entry with `user`), then `sudo -n` where the
  account has it without a password; what the host answers is shown.
  udisks lets a user at the host's own console mount and power off, but
  an ssh session is not that, so over ssh it asks polkit, which wants an
  administrator's password (unmounting what you mounted yourself needs
  none). When the host would do it for a password, polkit or `sudo`, the
  action opens in a terminal tab instead, where the host asks and you
  answer; the tab closes by itself once it is done. A polkit rule on the
  host spares the question, letting the account's group through, here
  `plugdev`:

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

  Without one, an fstab entry with `user` for the volume or passwordless
  `sudo` for `mount`, `umount` and `eject` does the same.
  The *Network* card lists the interfaces that have an address (IPv4 and
  global IPv6; not the loopback, not link-local ones), each with what
  came in and went out per second since the reading before, from the
  host's counters (`/proc/net/dev`, `netstat -ibn` on macOS and the BSDs;
  `ip -o addr` or `ifconfig` for the addresses). The *Processes* table
  has the twenty busiest by CPU and the twenty busiest by memory; a click
  on a heading sorts by that column, a second one the other way (kept on
  this machine). A click on a process offers a signal: SIGTERM, SIGKILL,
  SIGINT, SIGHUP, SIGSTOP or SIGCONT, sent with `kill` as this account,
  or through `sudo` for another user's process, in a terminal tab where
  the host asks for the password.
* *Files*: the host's directory tree, the same view as the Files window's
  panes (a folder menu, new folders and files, delete, the copies and
  their progress underneath). A folder opens and closes in the tree by
  the chevron at its left, a double click enters it. Every directory shown keeps the
  shape of its tree while the pane shows another: entered again, by a
  double click, *Back* or *Up*, its folders are open as they were left,
  the pane scrolled as it was, and after *Back* or *Up* the folder
  just left is selected. The button at the
  right of a folder's row, under the pointer, shuts every folder inside
  it. A click on a file shows it in a
  tab beside the terminals, the way an editor does: one temporary tab,
  in italics, which the next file clicked takes over; a double click
  (Return, *Preview* in its menu, or a double click on the tab itself)
  keeps the tab. What the tab shows: text as
  it is, with a button that wraps long lines; Markdown rendered (links
  open in the browser, images from the web show, relative ones as their
  alt text); a web page in a frame that runs no script and reaches
  nothing outside itself (its own styles apply, images from the web
  load); a PDF page by page, as wide as the tab; an image; a video or a
  recording in the system's player, in the formats it can decode. A
  button on a Markdown or HTML tab shows the file as written instead.
  A picture, a video or a PDF is fitted to the tab's width (⌘0) or to its
  height, which puts a PDF's pages side by side, or scaled with ⌘+ and
  ⌘−, ⌘ and the wheel, or the buttons; a text's letters scale the same
  way; the percentage on the header goes back to the file's own size.
  The bytes come over through the master or the session like a listing,
  up to 16 MB for a text, 32 MB for a PDF or an image, 64 MB for a video
  or a recording: a cut-off text says so, anything else that big is not
  shown. The right-click menus have what the toolbar has. On rows:
  *Open* (a folder) or *Preview*, *Download…* (a folder on this machine
  for what is selected), on a folder *Upload here…* (files picked on
  this machine) and *Shell here* (a terminal tab whose shell starts in
  that directory), *New folder…* and *New file…* (in the folder, or
  beside the file), *Delete…* and *Refresh*. On the blank, for the
  folder shown: *Upload here…*, *Shell here*, the new folder and file,
  *Go to folder…*, *Back*, *Up one directory*, the hidden files shown
  or not, *Refresh* and *Open in the Files window*. The Files window's
  panes have the same menus, with *Copy to* the other pane instead of
  the downloads and uploads, and *Open* for a file of this machine.
  Files dragged in from the desktop are copied too. Rows dragged onto a terminal tab are typed
  into it as their paths, quoted where needed, a space after each. The last button of the toolbar opens the Files
  window: two trees side by side, this host, this machine or another host
  in either one, where a drag copies.
* *Sessions*: the tmux sessions (with their windows) and GNU screen
  sessions. A click attaches one in a terminal tab (`tmux attach`,
  `screen -x`), or brings up the tab it is already in. Like a file of
  the Files tab, a single click uses one temporary tab, in italics,
  which the next session clicked takes over (the client in it detaches,
  the session lives on); a double click on the row, or on the tab
  itself, keeps the tab. Each row shows the
  session's current directory (tmux: the active window's; screen: the
  shell's, read from `/proc`, so Linux only), with two buttons: the
  folder shows that directory in the *Files* tab, the terminal opens a
  shell of the host in it, in a tab named after it. The pencil gives it
  another name (`tmux rename-session`, `screen -X sessionname`) and the
  cross ends it, with everything in it (`tmux kill-session`,
  `screen -X quit`).
* *Jobs*, where the host has a Slurm or PBS client: your jobs. A running
  Slurm job opens a shell on its nodes (`srun --jobid … --overlap --pty
  bash`), a running PBS job a shell on its first node (`ssh` from the
  login node). Each job's row shows its working directory (Slurm's
  `WorkDir`, PBS's `PBS_O_WORKDIR`), queued ones too, with the same
  folder and terminal buttons as a session.
* *Containers*, where the host has Docker or Podman: a click opens a shell
  inside one (`docker exec -it`). A container's bind mounts are listed
  by their host side, with the folder and terminal buttons: the Files
  tab, or a shell on the host, there (a menu when there are several).
* *Ports*: the forwards you set up, and the host's own ports. A forward
  (the + on the Forwards label) goes either way: `-L` makes a port here
  reach a port on the host, `-R` makes a port on the host reach one here.
  Both ports are yours to pick or left to shellder (a free one here, one
  sshd allocates on the host). The forward is made on the master
  (`ssh -O forward`, so the host has to be connected with a socket) and
  lives as long as the master; its row says whether it is up, starts it
  again after a reconnect, stops it (`ssh -O cancel`) or forgets it. A
  click on a `-L` forward opens its near end here (made first when it is
  not up): in the VNC viewer (Screen Sharing on macOS) when a VNC server
  is at the far end, in the browser otherwise. Under the forwards, the
  direct connections (the + on their label, or the main window's Tools
  card, which lists the same): a port opened at the host's own address
  with no tunnel, a click away; × forgets one. Under them, the
  host's own ports: the VNC servers (Xvnc, Xtigervnc, x11vnc…, and on a
  Mac its Screen Sharing or Remote Management, whose port 5900 launchd
  holds as root, so that only `launchctl` can tell it is on) until the +
  on the label adds another: one of the listeners this account sees
  (`ss`, or `lsof` on macOS; with its program where the host names it,
  which `ss` does only for the user's own, and `lsof` lists only those at
  all), or any port by hand, for what it cannot see. A port from 5900 to
  5999 that nothing names otherwise counts as VNC; × takes a port off the
  list. A click on one opens it here through a `-L` forward to it, made
  on the spot when there is none and kept with the others; the globe
  opens it at the host's own address instead (the `HostName` of its ssh
  config), with no tunnel, for a host reachable that way. What you set up
  is kept on this machine, per host.

The window opens with a shell on the host in its first terminal tab; the
+ beside the tabs (or ⌘T for a shell) offers another shell, a new tmux or
screen session, and an interactive job (`srun --pty bash`, `qsub -I`)
where the host has a scheduler; × closes a tab. The tabs are in groups,
as an editor's are: the split button at the right of a tab row, a tab's
right-click menu, ⌘\ (right) and ⌘⌥\ (down) put a second group beside
or under it, showing what the tab shows (a terminal's command run once
more: another shell in the same directory, the session attached a second
time; a preview's file); the dividers between groups drag. A tab dragged
onto another group's tab row moves there; dragged to an edge of a group
it splits the group there, to the middle it moves in. ⌘1 to ⌘9 give a
group the keyboard, where new tabs open; a group whose last tab closes
goes. The window comes back as it was closed, and a host switched away
from comes back as it was left: the groups and their tabs are kept per
host on this machine, the terminals connect again and run their commands
(a session made here is attached again, or made again when it is gone),
the previews show their files. Tabs of one-off actions (a mount asking
for a password, an interactive job) are not kept. Closing a tab hangs up on
it (a tmux or screen client detaches); the session itself, and anything
in it, lives on, as it does when the window closes or the connection
drops. The button at the right of the tabs gives the terminals a dark or
a light background, whatever the system's look (kept per host, on this
machine).
A tab whose ssh ended cleanly (an `exit`, a detach) closes itself;
one that ended any other way says so, and Return in it tries again. In the
terminals ⌘C copies a selection (and is the interrupt otherwise), ⌘V
pastes, ⌘K clears.

The readings come from one shell script: `/proc`, `df -P`, `ps`, `who`,
`ss`, `ip`, `tmux list-sessions`, `screen -ls`, `docker ps`, `squeue` and
`qstat -f` on Linux; a macOS host answers through `sysctl`, `vm_stat`,
`top`, `netstat`, `ifconfig` and `lsof`. Only the parts of the Status
tab that are shown are in it.

Neither the Files view nor the Console needs the master: through it when
it is up, and then without a login; on a login of its own otherwise. That
login is the same as a master's: ssh asks on a controlling terminal that
only shellder reads, the answer comes from the keychain or the prompt
window, and what is typed into the Console's terminals never reaches a
password prompt.

### Without a socket

A host whose master owns no socket, because its config has no `ControlPath`
or because the platform's ssh has no ControlMaster, is still switched on,
locked, reconnected and shown like any other: shellder runs the same ssh
without `-M`, which keeps the connection and the port forwards its config
asks for (`LocalForward`, `RemoteForward`, `DynamicForward`) alive, and
takes "Authenticated to host" from ssh's own output as the sign that it is
up, since there is no socket to check. The dot says "Connected (no socket)".
What such a master cannot do is serve other ssh commands, Console or Files
windows: those log in on their own.

A Files window on a host without a master up lists through a *session*: one
`ssh host sh` logged in when the window first asks for a listing and kept
open, which runs the listings, creates and deletes; one login per host,
however many listings, closed when shellder quits. The copies are the same
sftp (scp between two hosts) as through a master, only each one logs in on
its own, its questions answered the same way, so a copy costs a login (on a
host with a one-time code, the next code: a code just used is not sent
again). A copy that stopped goes on from what landed, as through a master.

`shellder status` can only ask a master with a socket whether it is up; one
without is marked `?`, kept alive by the app alone.
`SHELLDER_NO_CONTROLMASTER=1` shows the app the way a platform without
ControlMaster sees it, on one that has it.

## Jump hosts

A host whose `ProxyJump` names another `Host` entry from the same config is
connected through shellder's own master for that entry. Switching the inner
host on switches the jump host on as well, brings it up first, then connects
the inner host, whose hop reuses the jump host's socket instead of opening a
throw-away connection of its own. The jump host's list entry says which hosts
go through it. Switching the inner host off leaves the jump host on, it is a
connected host like any other now. Switching the jump host off (or
*Disconnect*, *Close socket* on it) takes the hosts behind it down with it,
and a jump host that fails to connect fails the hosts waiting for it with the
same reason. The lock follows the same rules: locking the inner host locks
its jump hosts, unlocking a jump host unlocks the hosts behind it. Chains
(`ProxyJump` through a host that itself has one) work the same way. A hop
that is not a `Host` entry, or one without a `ControlPath`, is left to ssh.

## How prompts are answered

ssh never reads a password from stdin: it opens its controlling terminal.
shellder gives every master a pseudo-terminal as its controlling terminal
and nothing else (stdin, stdout and stderr stay on pipes, so ssh's
diagnostics go to the log and never mix with the questions). Text ssh
leaves on that terminal without a newline, ending in `:` or `?`, is a
question; the app:

1. works out which host the prompt belongs to (ProxyJump hops print
   `user@host`, which is matched against the resolved config of every host),
2. answers from the keychain (TOTP codes are generated and never reused),
3. otherwise opens a dialog: a password or passphrase (with "save in
   keychain"), a verification code, a host key confirmation with the
   fingerprint, or free text.

Cancelling a dialog turns that host's switch off, so a wrong guess never
hammers the server.

A ProxyJump hop asks on the same terminal, before the host behind it, and
its prompt carries its own `user@host`, which is how it is told apart.

## App icon

`build.sh` takes `scripts/icon-source.png` (or `.jpg`) and has the Tauri CLI
crop and scale it to every size the bundle wants. The menu bar icon is a
monochrome template made from the same picture (light pixels opaque, dark
ones transparent), checked in as `src-tauri/icons/tray.png`, so macOS tints
it like its own status icons. After changing the source picture, make it
again with

```bash
cargo run --release --manifest-path scripts/menubar-template/Cargo.toml -- \
    scripts/icon-source.png src-tauri/icons/tray.png 36
```

## Build & run

```bash
./build.sh            # -> build/Shellder.app (macOS), build/*.deb and *.AppImage (Linux)
./build.sh --install  # also copy to ~/Applications and launch (macOS)
```

`build.sh` installs what is missing: Rust (with rustup), the Tauri CLI and,
on Linux, the development packages of what every Tauri app links against
(webkit2gtk 4.1, libayatana-appindicator, librsvg, dbus and the rest, through
apt, dnf, pacman or zypper). `./build.sh --deps` does only that.

The app has a main window (host list with the Connect switches and locks,
per-host details, credentials, log panel), a Settings window (start at
login, silent start, background behaviour, menu bar icon) and a menu bar
item. By default closing the last window quits the app and its masters. With
"Run in background" switched on, closing the window instead leaves the Dock
and keeps the connections alive in the background, reachable from the menu
bar icon (a click opens the window, another one while it is in front puts it
away, a right click shows its menu) or by launching it again. A Console or
Files window minimized to the Dock still counts as open: closing the main
window beside it leaves the app in the Dock with that window.

The icon is faded while no master is up and plain as soon as one is; errors
and prompts do not change it. On Linux the panel shows the picture as drawn,
dark; "Invert menu bar icon" (shown there only, since macOS tints the picture
itself) turns it light for a dark panel.

"Start silently" skips the window when the login item starts the app; a
double click or `open` always opens it. It can only be switched on together
with "Start at login" and "Run in background" (without the latter there
would be no window and no icon) and goes off with "Run in background".
`shellder --background` forces a start without a window for a single launch.
Both "Start at login" and "Start silently" are off until you switch them on:
nothing is written to `~/Library/LaunchAgents` before you ask for it.

The interface language is chosen in Settings: English unless you pick
another, whatever the system language, and a change shows after a relaunch.
English and Simplified Chinese are built in, the log stays English. To add a
language, copy `ui/lang/zh-Hans.json` to `ui/lang/<lang>.json` (a BCP 47 name
such as `ja` or `zh-Hant`), translate the right-hand sides (the keys are the
English text, `{name}` is a placeholder) and add the language to the list at
the top of `ui/settings.js`. Keys a translation lacks fall back to English.

Settings ends with the version and a "Software update" row. With "Check for
updates automatically" on (the default) the app reads the latest release's
`latest.json` from GitHub a few seconds after launch and once a day, and says
so in Settings, in the main window's bottom bar and in the menu bar item's
menu. Nothing is installed until you click "Install and relaunch": that
downloads the release's updater artifact for this platform, checks its
signature against the key built into the app, swaps it for the running
bundle in place and relaunches. "Check now" asks right away. A release only
counts when its version is newer than the running one, so a local build with
the same number stays put. Release builds are signed with the maintainer's
Apple Development certificate, so the first launch after updating from a
build you signed yourself may show one keychain dialog, see "Keychain"
below.

The same binary is also a CLI:

```bash
shellder hosts                    # Host entries and their ControlPath
shellder lock socjump             # keep this host connected
shellder status                   # UP/down per locked host
shellder add-secret socjump password
shellder install | uninstall      # login item
```

## Notes

### Keychain

The login keychain checks two things before handing a secret to an app: the
item's access list (who may read it) and the app's *partition* (`teamid:…`
for Apple-signed apps, otherwise the build's cdhash). A partition mismatch is
the dialog that insists on the keychain password. To make rebuilds silent,
sign with an Apple-issued certificate that carries a Team ID: sign into Xcode
with any Apple ID (Settings → Accounts → Manage Certificates → + → Apple
Development, where a free account's "Personal Team" is enough), and
`build.sh` picks it up automatically (or set `SHELLDER_SIGN_IDENTITY`). The
first read under a new identity asks for the keychain password once: answer
"Always Allow", which puts that identity on the vault for good ("Allow" is
for this one read and the next one asks again). shellder never deletes or
re-creates the vault, so two copies with different signatures (say a release
in `/Applications` started at login and a build of your own started by hand)
share it after one dialog each. Without such a certificate `build.sh` stops
instead of signing ad hoc, since an ad hoc build is a new identity on every
rebuild that asks again; `SHELLDER_SIGN_IDENTITY=-` forces ad hoc anyway when
you must. A dialog dismissed at launch is not asked again for five minutes,
the Keychain section of Settings asks right away. These dialogs never offer
Touch ID.

### The keychain on Linux

The vault is one item in the Secret Service's default keyring, so a keyring
daemon (GNOME Keyring, KWallet) must be running and unlocked; on a desktop
session it is. A session that has no default keyring yet (autologin, a login
without a password) reads as an empty vault, and the first secret saved
creates one: the daemon asks for a password for it then. The menu bar icon
needs a StatusNotifier host (KDE has one, GNOME needs the AppIndicator
extension).

### A wrong password

`NumberOfPasswordPrompts=1` means a bad secret produces exactly one failed
login, and the host is switched off rather than retried. Some servers ban
the client IP for a while after a handful of failures.

### Isolated testing

Used for the automated end-to-end test against a local sshd container:
`SHELLDER_SSH_CONFIG=<file>` makes shellder read that ssh config (passed to
ssh as `-F`), `SHELLDER_STATE_DIR` relocates the prefs, the lock and the log,
`SHELLDER_NO_CONTROLMASTER=1` takes ControlMaster away. Run the binary
directly (`build/Shellder.app/Contents/MacOS/shellder`) so the variables are
inherited.
