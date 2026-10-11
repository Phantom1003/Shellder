# shellder manual

shellder keeps `ssh -M -N` master connections open in the background so that every
later `ssh host`, `scp`, `rsync`, VS Code Remote, … reuses the socket and never
asks for a password again. When a master has to be (re)established it answers
the password / key passphrase / TOTP prompt itself from your keychain, and
falls back to asking you in a dialog for anything it cannot answer (unknown
host keys included). It runs on macOS and Linux (Windows' OpenSSH has no
ControlMaster, so there is nothing for it to keep alive there).

## Principles

* **Your `~/.ssh/config` is the only source of truth.** shellder lists every
  `Host` block (following `Include`), resolves it with `ssh -G`, and never
  writes to that file. ControlMaster/ControlPath/ControlPersist come from your
  config. If a host has no `ControlPath`, the app shows the snippet to add.
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
  that terminal and writes the answer back. No helper program, no socket:
  nothing outside the app can ask it for a secret. The CLI has no command
  that prints a secret or logs in with one.
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
is up and locked. A failed attempt turns the dot red until the switch goes
on again; the switch itself is plain grey whenever it is off.

**The lock keeps a connection connected.** Locking a host switches it on if
it is off, and from then on shellder brings the connection back by itself: a
dropped link is reconnected with back-off (5 s → 10 min), and a locked host
is switched on again when shellder starts. Only a link that keeps dying right
after connecting gives up after three tries.

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
3. otherwise opens a dialog — password/passphrase (with "save in keychain"),
   verification code, host-key confirmation with the fingerprint, or free text.

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
apt, dnf, pacman or zypper). `./build.sh --deps` does only that. The app has a main
window (host list with the Connect switches and locks, per-host details,
credentials, log panel), a Settings window (start at login, silent start,
background behaviour, menu bar icon) and a menu bar item. By default closing
the last window quits the app and its masters. With "Run in background"
switched on, closing the window instead leaves the Dock and keeps the
connections alive in the background, reachable from the menu bar icon (a
click opens the window, another one while it is in front puts it away, a
right click shows its menu) or by launching it again. The icon is faded
while no master is up and plain as soon as one is; errors and prompts do not
change it. "Start silently"
skips the window when the login item starts the app; a double click or
`open` always opens it. It can only be switched on together with "Start at
login" and "Run in background" (without the latter there would be no window
and no icon) and goes off with "Run in background". `shellder --background`
forces a start without a window for a single launch. Both "Start at login"
and "Start silently" are off until you switch them on: nothing is written to
`~/Library/LaunchAgents` before you ask for it.

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
build you signed yourself may show one keychain dialog, see the Keychain
section.

The same binary is also a CLI:

```bash
shellder hosts                    # Host entries and their ControlPath
shellder lock socjump             # keep this host connected
shellder status                   # UP/down per locked host
shellder add-secret socjump password
shellder install | uninstall      # login item
```

## Notes

* **Keychain prompts and rebuilds.** The login keychain checks two things
  before handing a secret to an app: the item's access list (who may read it)
  and the app's *partition* (`teamid:…` for Apple-signed apps, otherwise the
  build's cdhash). A partition mismatch is the dialog that insists on the
  keychain password. To make rebuilds silent, sign with an Apple-issued
  certificate that carries a Team ID: sign into Xcode with any Apple ID
  (Settings → Accounts → Manage Certificates → + → Apple Development, where a free
  account's "Personal Team" is enough), and `build.sh` picks it up
  automatically (or set `SHELLDER_SIGN_IDENTITY`). The first read under a new
  identity asks for the keychain password once: answer **Always Allow**, which
  puts that identity on the vault for good ("Allow" is for this one read and
  the next one asks again). Shellder never deletes or re-creates the vault, so
  two copies with different signatures (say a release in `/Applications`
  started at login and a build of your own started by hand) share it after
  one dialog each. Without such a certificate `build.sh` stops instead of
  signing ad hoc, since an ad hoc build is a new identity on every rebuild
  that asks again; `SHELLDER_SIGN_IDENTITY=-` forces ad hoc anyway when you
  must. A dialog dismissed at
  launch is not asked again for five minutes, the Keychain section of
  Settings asks right away. These dialogs never offer Touch ID.
* **Linux keychain.** The vault is one item in the Secret Service's default
  keyring, so a keyring daemon (GNOME Keyring, KWallet) must be running and
  unlocked; on a desktop session it is. A session that has no default keyring
  yet (autologin, a login without a password) reads as an empty vault, and
  the first secret saved creates one: the daemon asks for a password for it
  then. The menu bar icon needs a StatusNotifier host
  (KDE has one, GNOME needs the AppIndicator extension).
* **Wrong password = switch off, not hammering.** `NumberOfPasswordPrompts=1`
  means a bad secret produces exactly one failed login and the host is switched
  off. Some servers ban the client IP for a while after a handful of failures.
* **Isolated testing** (used for the automated end-to-end test against a local
  sshd container): `SHELLDER_SSH_CONFIG=<file>` makes shellder read that ssh config
  (passed to ssh as `-F`), `SHELLDER_STATE_DIR` relocates the prefs, the lock
  and the log. Run the binary directly
  (`build/Shellder.app/Contents/MacOS/shellder`) so the variables are inherited.
