# Notes for agents working on Shellder

Shellder is a macOS menu bar app plus CLI (one binary, `Sources/shellder`) that
keeps `ssh -M` ControlMaster connections alive and answers ssh prompts through
`SSH_ASKPASS` over a unix socket. Secrets live in one login-keychain item, the
"shellder vault". The user manual is `docs/manual.md`.

There is no unit test target. `Tests/` holds shell scripts that drive the
built binary against a throwaway sshd in Docker (`Tests/drop-unlocked.sh`:
an unlocked host whose master the server closes right after login must
switch off with the error, not retry).
Everything below is what has worked when testing by hand. Prefer adding a
script under `Tests/` over repeating these steps in a conversation.

## Build

```bash
./build.sh              # SwiftPM release build, wraps it as build/Shellder.app
./build.sh --install    # also copies to ~/Applications and relaunches (kills the running app!)
```

* `build.sh` signs with the first Apple Development / Developer ID identity in
  the keychain, or the one in `SHELLDER_SIGN_IDENTITY`, and falls back to ad hoc.
  On CI there is no identity, so CI builds are ad hoc.
* Never run `--install` while testing: it does `pkill -x shellder`, which also
  kills any test copy.
* If `xcodebuild` refuses to run (licence not accepted after an Xcode update),
  `build.sh` falls back to the Command Line Tools automatically.

## CI (`.github/workflows/build.yml`)

* Runs on every push to `main`, every PR, every `v*` tag and manual dispatch.
* Matrix: `macos-26` (GA, blocking) and `xcode-27` (preview, `continue-on-error`).
* The "Verify bundle" step only checks the bundle's structure: codesign
  `--verify --strict`, `plutil -lint`, presence of the icns and menubar PNGs.
  No functional test runs on CI.
* A `v*` tag publishes a GitHub Release with the `macos-26` zip, ad hoc signed,
  not notarised.

## One binary, three modes

`Sources/shellder/main.swift` picks the mode from the environment and argv:

1. `SHELLDER_ASKPASS=1`: askpass helper, ssh spawned it with the prompt in
   argv[1]. Talks to the running app over `SHELLDER_SOCK`, falls back to the
   keychain when the app is not running.
2. Any other argument: CLI subcommand (`shellder help` lists them).
3. No arguments: GUI app (`--background` starts without a window; `--login`
   is what the LaunchAgent passes, the only launch that honours "start silently").

The CLI works without the app running. `shellder test HOST` does a one-shot
login with the stored secrets and is the quickest end-to-end check.

## Isolation hooks

Set these on the binary you launch. Run
`build/Shellder.app/Contents/MacOS/shellder` directly so the variables are
inherited (`open` would not pass them).

| Variable | Effect | Default |
| --- | --- | --- |
| `SHELLDER_SSH_CONFIG` | ssh config to use, passed to ssh as `-F` | `~/.ssh/config` |
| `SHELLDER_STATE_DIR` | askpass socket, `app.lock`, TOTP replay markers, `shellder.log` | `~/Library/Application Support/shellder` |
| `SHELLDER_PREFS_SUITE` | NSUserDefaults suite (locked hosts, settings) | `local.shellder.prefs` |
| `SHELLDER_UPDATE_API` | URL of the "latest release" JSON the updater reads | GitHub's `releases/latest` for the repository |

Things that are **not** isolated:

* **The keychain vault.** `shellder add-secret` writes to the user's real
  vault. Use throwaway host aliases (for example `shjt-*`) and run
  `shellder del-secret HOST KIND` when done. Never delete the vault item
  itself (`shellder:vault`), see below.
* **The LaunchAgent.** `shellder install` writes `~/Library/LaunchAgents/local.shellder.plist`.
  Do not run it from a test copy.

`app.lock` is an `flock` in the state dir, so a test GUI with its own
`SHELLDER_STATE_DIR` runs beside the user's app and writes its own
`shellder.log` there. A GUI instance truncates its log at start (previous
content moves to `.log.old`), CLI runs append. `SHELLDER_PREFS_SUITE` must
differ from the bundle id (`local.shellder`), NSUserDefaults refuses that.

## End-to-end test against a local sshd (Docker)

Docker is available on the dev machine (OrbStack). The recipe that has worked:

1. `docker run -d --name shjt -p 127.0.0.1:2222:22 alpine sh -c 'apk add openssh && ssh-keygen -A && adduser -D t && echo t:pw | chpasswd && /usr/sbin/sshd -D -e'`
   (or an image with `google-authenticator` PAM for TOTP tests).
2. Write a throwaway ssh config with a `Host` entry: `HostName 127.0.0.1`,
   `Port 2222`, `User t`, `ControlMaster auto`, `ControlPath /tmp/shjt/cm-%C`,
   `StrictHostKeyChecking no`, `UserKnownHostsFile /dev/null`.
3. Store the password: `printf 'pw\n' | SHELLDER_SSH_CONFIG=... shellder add-secret HOST password`
   (stdin is read when it is not a TTY).
4. `SHELLDER_SSH_CONFIG=... SHELLDER_STATE_DIR=/tmp/shjt shellder test HOST`
   prints `shellder: login OK` on success. Add `-v` for ssh's own trace.
5. For the daemon path: lock the host (`shellder lock HOST` with the same env),
   start the GUI binary with the same env plus `--background`, then check
   `ssh -F <config> -O check HOST` and the log.
6. Clean up: `del-secret`, `docker rm -f shjt`, `rm -rf /tmp/shjt`.

Gotchas:

* ControlPath and the askpass socket must be short (unix socket paths are
  limited to about 104 bytes). Use `/tmp/shjt`, not a deep scratch directory.
* ProxyJump: two sshd containers on one docker network, only the jump host
  published to 127.0.0.1. Alpine's sshd ships `AllowTcpForwarding no`; sed it
  to `yes` and `kill -HUP 1` in the container, or the hop fails with
  "stdio forwarding failed".
* `pkill -f` inside `docker exec sh -c '...'` matches its own shell, anchor
  the pattern with `^`.
* Wrong secrets: `NumberOfPasswordPrompts=1` means one failed login per
  attempt, and after three quick failures the daemon backs off to ten
  minutes. Real servers may ban the client IP after a few failures, so never
  point tests at a real host with a guessed password.
* Host key confirmations are never auto-accepted without the app; the
  askpass fallback answers "no". Use `StrictHostKeyChecking no` in the test
  config.

## Keychain

* One item: service `shellder:vault`, account `shellder`, a JSON object
  `{host: {password|passphrase|totp: secret}}`.
* Updates go through `SecItemUpdate`. `SecItemDelete` on the existing item
  has failed with `errSecInvalidOwnerEdit (-25244)` from the app, so do not
  "fix" the vault by deleting it.
* An ad hoc signed build gets a different partition than an Apple-signed one
  and triggers a keychain password dialog on every rebuild. To poke at the
  vault without dialogs, sign the test binary with the same Apple Development
  identity **and** `--identifier local.shellder`.
* A keychain dialog blocks the app until answered. From a script `kill -9`
  the test copy to dismiss it.

## Driving a test GUI with the desktop automation tools

When a change needs the real window or the menu bar item:

1. Copy `build/Shellder.app` to `/tmp/shjt/ShellderTest.app`.
2. `plutil -replace CFBundleIdentifier -string local.shellder.test .../Info.plist`
   so window lookup does not resolve to the user's own running Shellder.
3. Re-sign with the Apple Development identity and `--identifier local.shellder`
   (keeps the vault readable, see above).
4. Launch the binary with the `SHELLDER_*` variables and request desktop
   access for `local.shellder.test`.
5. Quit it through the app menu ("Quit Shellder"); the Bash sandbox cannot
   signal it.

Known quirks:

* Toggles draw grey while the window is inactive. Activate the app (System
  Events `set frontmost`) before a screenshot that has to show colour.
* The status item is not a window; only display-scope screenshots and clicks
  reach it, and clicks are refused unless the test app is frontmost.
* A screenshot dismisses an open status menu. Confirm menu paths with a
  temporary log line instead of a picture.
* The automation tool activates the desktop app right before a click, so
  `NSApp.isActive` is false inside Shellder at click time. Judge
  activation-dependent behaviour from a log line, not from the tool run.
* The tool's double click sends two separate clicks; the app's double-click
  handling has to count a second click while the first is pending.
* Posting CGEvents from a scratch binary is dropped (no accessibility grant).
* A GUI started from the Bash tool's sandbox is invisible to System Events
  and to the automation tools. Start it with `open --env VAR=value ...
  ShellderTest.app` instead, `open` passes the hooks that way.
* The test copy is unreachable while a relaunch (language, update) is in
  flight and again once an update replaced it, since the release bundle
  carries the real bundle id: put `local.shellder.test` back with plutil and
  re-sign before driving it further.

## Testing the updater

`Sources/shellder/App/Updater.swift` reads `SHELLDER_UPDATE_API` (a GitHub
"latest release" JSON: `tag_name`, `html_url`, `assets[].name` and
`browser_download_url`) and installs the first `.zip` asset. To try it
without a release: copy `build/Shellder.app`, bump
`CFBundleShortVersionString` in the copy, sign it (`--identifier
local.shellder`, the Apple identity keeps the keychain quiet), `ditto -c -k
--keepParent` it into a directory served by `python3 -m http.server`, and
write a `latest.json` next to it whose asset URL points at that zip. Start the
test copy with `SHELLDER_UPDATE_API=http://127.0.0.1:PORT/latest.json` and
the other hooks. The log shows `update: version X is available` a few seconds
in, "Install and relaunch" in Settings replaces the test copy's bundle and the
relaunched process logs a fresh `daemon starting`. The install lines land in
`shellder.log.old` because the relaunch truncates the log.

## Conventions

* Commit messages: `[component] description`, English. Do not commit or push
  unless asked.
* Do not add migrations for prefs or the vault format.
* Never log secret values or their lengths.
* No personal data (real host names, accounts, Team IDs) in the repository.
