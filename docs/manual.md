# shellder manual

shellder keeps `ssh -M -N` master connections open in the background so that every
later `ssh host`, `scp`, `rsync`, VS Code Remote, … reuses the socket and never
asks for a password again. When a master has to be (re)established it answers
the password / key passphrase / TOTP prompt itself from your login keychain, and
falls back to asking you in a dialog for anything it cannot answer (unknown
host keys included).

## Principles

* **Your `~/.ssh/config` is the only source of truth.** shellder lists every
  `Host` block (following `Include`), resolves it with `ssh -G`, and never
  writes to that file. ControlMaster/ControlPath/ControlPersist come from your
  config. If a host has no `ControlPath`, the app shows the snippet to add.
* The master runs as `ssh -M -N <alias>` plus a few process-only options
  (`ControlPersist=no` so shellder owns the process, `NumberOfPasswordPrompts=1`,
  `LogLevel=ERROR`, and `ServerAliveInterval` only if your config leaves it at 0).
* Secrets live in the login keychain in a single generic-password item, the
  "shellder vault" (service `shellder:vault`, a JSON object keyed by host). One item
  means the keychain asks for permission at most once.
* State outside the keychain: `~/.local/state/shellder/` (askpass socket, lock,
  last TOTP code), `~/Library/Logs/shellder.log`, app preferences
  (`local.shellder.prefs`), and optionally `~/Library/LaunchAgents/local.shellder.plist`.

## Switch semantics

The switch next to a host means *connect*:

1. **On** = one connection attempt. If it fails (wrong password, cancelled
   prompt, declined host key, unreachable, no ControlPath) the switch flips
   back off and nothing is retried. The reason is shown in the host list.
2. Once the master is up it is kept alive: a dropped link is reconnected with
   back-off (5 s → 10 min). A link that keeps dying right after connecting
   gives up after three tries and turns the switch off.
3. **Off** = close the master. *Close socket* additionally runs `ssh -O exit`,
   which also works for a master started by an interactive `ssh`.

Some servers close a session-less `ssh -N` connection within seconds. When
shellder sees an authenticated master die that quickly it escalates the
*keep-alive mode* and remembers it per host:

| mode | what the server sees | notes |
|---|---|---|
| `-N` (default) | a connection with no session | cleanest, but some servers reject it |
| idle shell | `sshd: you@pts/N → -bash`, an idle login in `w` | indistinguishable from an open terminal, SIGHUP on disconnect |
| cat | `sshd: you@notty → cat` | no pty/login scripts, exits on EOF when the link drops |

The mode can also be picked by hand in the host's Status section.

## Jump hosts

A host whose `ProxyJump` names another `Host` entry from the same config is
connected through shellder's own master for that entry. Switching the inner
host on brings the jump host up first, then the inner host, and its hop
reuses the jump host's socket instead of opening a throw-away connection of
its own. The jump host is kept alive for as long as any host needs it, its own
switch being on or off, and its list entry says what it is up for. When the
last host going through it is switched off the jump host is closed again,
unless its own switch is on. *Disconnect* or *Close socket* on a jump host
takes the hosts behind it down with it. A jump host that fails to connect
fails the hosts waiting for it with the same reason. Chains
(`ProxyJump` through a host that itself has one) work the same way. A hop
that is not a `Host` entry, or one without a `ControlPath`, is left to ssh.

## How prompts are answered

ssh is started with `SSH_ASKPASS` pointing at the shellder binary and
`SSH_ASKPASS_REQUIRE=force`. The helper forwards each prompt over a unix
socket to the running app, which:

1. works out which host the prompt belongs to (ProxyJump hops print
   `user@host`, which is matched against the resolved config of every host),
2. answers from the keychain (TOTP codes are generated and never reused),
3. otherwise opens a dialog — password/passphrase (with "save in keychain"),
   verification code, host-key confirmation with the fingerprint, or free text.

Cancelling a dialog pauses that host until you press Connect again, so a wrong
guess never hammers the server.

Password and passphrase fields switch the keyboard to the ASCII layout (ABC)
while they have focus and switch back afterwards. Secure fields refuse
input-method composition, and Apple's Pinyin treats lowercase `u` as a mode
prefix, so with Pinyin active that key was swallowed with a beep.

## App icon

`build.sh` takes `Assets/icon-source.png` (or `.jpg`), crops it to a centred
square and packs every icon size into the bundle. The menu bar icon is a
monochrome template made from the same picture (light pixels opaque, dark
ones transparent, see `Tools/menubar-template.swift`), so macOS tints it like its
own status icons. The file is required.

## Build & run

```bash
./build.sh            # -> build/shellder.app
./build.sh --install  # also copy to ~/Applications and launch
```

Requires Xcode command line tools (Swift 5.9+, macOS 13+). The app has a main
window (host list with "keep connected" switches, per-host details, credentials,
log panel), a Settings window (start at login, silent start, Dock / menu bar
icons) and a menu bar item. Closing the window does not quit: the app leaves
the Dock and keeps its connections alive in the background. Reopen it from the
menu bar icon or by launching it again. "Start silently" skips the window on
every launch, at login included, and `shellder --background` forces that for a
single launch.

The same binary is also a CLI:

```bash
shellder hosts                    # Host entries and their ControlPath
shellder enable socjump           # keep this host connected
shellder status                   # UP/down per kept host
shellder add-secret socjump password
shellder test socjump             # one-shot login with the stored secrets
shellder install | uninstall      # LaunchAgent
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
  automatically (or set `SHELLDER_SIGN_IDENTITY`). The first read after switching
  identities asks for the keychain password one last time. After that the
  `teamid:` partition is stable. Without such a certificate the app is signed
  ad hoc and every rebuild asks again. These dialogs never offer Touch ID. shellder uses
  Touch ID itself before showing a secret in plain text.
* **Wrong password = switch off, not hammering.** `NumberOfPasswordPrompts=1`
  means a bad secret produces exactly one failed login and the host is switched
  off. Some servers ban the client IP for a while after a handful of failures.
* **Building without an accepted Xcode licence.** `build.sh` falls back to the
  Command Line Tools (borrowing Xcode's SwiftUI macro plugin) when
  `xcodebuild` refuses to run. `sudo xcodebuild -license accept` fixes it properly.
* **Isolated testing** (used for the automated end-to-end test against a local
  sshd container): `SHELLDER_SSH_CONFIG=<file>` makes shellder read that ssh config
  (passed to ssh as `-F`), `SHELLDER_STATE_DIR` relocates the socket/lock,
  `SHELLDER_PREFS_SUITE` uses a separate preferences domain. Run the binary
  directly (`build/shellder.app/Contents/MacOS/shellder`) so the variables are inherited.
