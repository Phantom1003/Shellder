# Notes for agents working on Shellder

Shellder is a desktop app plus CLI (one binary, built with Tauri 2 from
`src-tauri/`, the pages in `ui/`) that keeps `ssh -M` ControlMaster
connections alive and answers ssh's prompts itself: every master gets a
pseudo-terminal as its controlling terminal, and the questions ssh writes to
`/dev/tty` are answered on that pty. Secrets live in one keychain item, the
"shellder vault" (the login keychain on macOS, the Secret Service on Linux).
The user manual is `docs/manual.md`. macOS and Linux are the targets; Windows
is not (its OpenSSH has no ControlMaster).

## Layout

* `src-tauri/src/core/`: everything below the windows, no Tauri in it.
  `config` (paths, tunables), `log`, `prefs` (one JSON file), `catalog`
  (ssh config parsing, `ssh -G`), `ssh` (running ssh/scp, the master with
  its pty), `pty` (the controlling terminal and the reader that answers
  questions), `prompt` (what a question is), `daemon` (one thread, the
  switch/lock semantics), `keychain`, `totp`, `autostart` (LaunchAgent /
  XDG autostart), `remotefs` (`ls` through the master, this machine's
  files), `transfer` (one scp with progress).
* `src-tauri/src/app/`: `model` (the state every window draws from, and the
  `PromptHandler` that answers ssh), `files` (the Files window's model),
  `commands` (what the pages may call), `windows`, `tray`, `updater`.
* `src-tauri/src/cli.rs`: the subcommands. `src/main.rs` picks the mode.
* `ui/`: plain HTML/JS/CSS, no bundler, no npm. `shellder.js` is the glue
  (`invoke`, events, `t()` translations, DOM helpers, icons); one page per
  window (`index`, `settings`, `prompt`, `files`). Translations are
  `ui/lang/<lang>.json`, keyed by the English text, `{name}` placeholders.
* Tests: `cargo test` in `src-tauri` runs the unit tests (prompt
  classification, `ls` parsing, TOTP vectors, the keychain's three
  decisions, the catalog parser) and two integration tests that need no
  Docker: `tests/pty_prompt.rs` (a shell script asking on `/dev/tty` is
  answered through the pty) and `tests/ssh_master.rs` (a real master against
  a throwaway `sshd` on a high port, run as this user: the host key
  confirmation and the key passphrase both come down the pty, the socket
  comes up). `Tests/*.sh` drive Docker: `Tests/drop-unlocked.sh` (an
  unlocked host whose master the server closes right after login must
  switch off with the error, not retry), `Tests/files-window.sh` (brings two
  sshd's up, then runs the ignored `tests/files_window.rs`, which drives
  `FilesModel` the way the panes do). Prefer adding a test over repeating
  steps in a conversation.

## Build

```bash
./build.sh              # cargo tauri build, wraps it as build/Shellder.app (macOS) or build/*.deb, *.AppImage (Linux)
./build.sh --debug      # unsigned debug build, no updater artifacts
./build.sh --install    # also copies to ~/Applications and relaunches (kills the running app!)
```

* Needs Rust and the Tauri CLI (`cargo install tauri-cli --version '^2' --locked`).
  On Linux also webkit2gtk 4.1, libayatana-appindicator, librsvg, patchelf.
* `build.sh` signs with the first Apple Development / Developer ID identity
  in the keychain, or the one in `SHELLDER_SIGN_IDENTITY`. There is no ad hoc
  fallback for a release build; `SHELLDER_SIGN_IDENTITY=-` asks for it,
  `--debug` builds are left unsigned. Keep the bundle id `local.shellder`:
  the vault item trusts it.
* The updater artifacts (`Shellder.app.tar.gz` + `.sig`, `.AppImage` +
  `.sig`) are only made when `TAURI_SIGNING_PRIVATE_KEY` is set (the
  minisign key whose public half is in `tauri.conf.json`). Without it the
  build skips them.
* Icons are generated from `scripts/icon-source.png` by `cargo tauri icon`
  into `src-tauri/icons/` (ignored by git), except the menu bar pictures
  `tray.png` / `tray-badge.png`, which are checked in and come from
  `scripts/menubar-template.swift` (macOS only).
* Never run `--install` while testing: it does `pkill -x shellder`.

## CI (`.github/workflows/build.yml`)

* Runs on every push to `main`, every PR, every `v*` tag and manual dispatch.
* Matrix: `macos-26` (GA, blocking), `xcode-27` (preview, `continue-on-error`),
  `ubuntu-24.04` (builds the release), `ubuntu-26.04`.
* The "Icons" step runs `cargo tauri icon` before the unit tests: the
  icons are not checked in, and `tauri::generate_context!()` reads them at
  compile time, so without that step `cargo test` fails to compile.
* macOS imports the maintainer's Apple Development certificate from
  `SHELLDER_SIGN_P12` / `SHELLDER_SIGN_P12_PASSWORD` into a throwaway
  keychain; the job fails without them, so pull requests from forks do not
  build. `TAURI_SIGNING_PRIVATE_KEY` (+ `_PASSWORD`) signs the updater
  artifacts on both platforms.
* A `v*` tag publishes a GitHub Release with the macOS zip, the Linux deb
  and AppImage, the signed updater artifacts and `latest.json`
  (`scripts/latest-json.sh`), which is what the app's updater reads.

## One binary, two modes

`src/main.rs` picks the mode from argv:

1. Any argument: CLI subcommand (`shellder help` lists them). Works without
   the app running, but there is no command that prints a secret or logs
   in with one.
2. No arguments: GUI (`--background` starts without a window; `--login` is
   what the login item passes, the only launch that honours "start
   silently").

There is no askpass helper any more: nothing outside the app ever handles a
prompt, so there is no socket and no peer verification.

## Isolation hooks

Set these on the binary you launch. Run
`build/Shellder.app/Contents/MacOS/shellder` directly so the variables are
inherited (`open` would not pass them).

| Variable | Effect | Default |
| --- | --- | --- |
| `SHELLDER_SSH_CONFIG` | ssh config to use, passed to ssh as `-F` | `~/.ssh/config` |
| `SHELLDER_STATE_DIR` | `prefs.json`, `app.lock`, `vault.stamp`, TOTP replay markers, `shellder.log` | `~/Library/Application Support/shellder` (macOS), `$XDG_STATE_HOME/shellder` (Linux) |
| `SHELLDER_UPDATE_API` | URL of the updater's `latest.json` | the latest GitHub release's `latest.json` |

Things that are **not** isolated:

* **The keychain vault.** `shellder add-secret` writes to the user's real
  vault. Use throwaway host aliases (for example `shjt-*`) and run
  `shellder del-secret HOST KIND` when done. Never delete the vault item
  itself (`shellder:vault`), see below.
* **The login item.** `shellder install` writes
  `~/Library/LaunchAgents/local.shellder.plist` (macOS) or
  `~/.config/autostart/local.shellder.desktop` (Linux). Do not run it from a
  test copy.
* **The window positions** (tauri-plugin-window-state) are keyed by the
  bundle id.

`app.lock` is an `flock` in the state dir, so a test GUI with its own
`SHELLDER_STATE_DIR` runs beside the user's app and writes its own
`shellder.log` there. A GUI instance truncates its log at start (previous
content moves to `.log.old`), CLI runs append.

## End-to-end test against a local sshd (Docker)

The recipe that has worked:

1. `docker run -d --name shjt -p 127.0.0.1:2222:22 alpine sh -c 'apk add openssh && ssh-keygen -A && adduser -D t && echo t:pw | chpasswd && /usr/sbin/sshd -D -e'`
   (or an image with `google-authenticator` PAM for TOTP tests).
2. Write a throwaway ssh config with a `Host` entry: `HostName 127.0.0.1`,
   `Port 2222`, `User t`, `ControlMaster auto`, `ControlPath /tmp/shjt/cm-%C`,
   `StrictHostKeyChecking no`, `UserKnownHostsFile /dev/null`.
3. Store the password: `printf 'pw\n' | SHELLDER_SSH_CONFIG=... shellder add-secret HOST password`
   (stdin is read when it is not a TTY).
4. Lock the host (`shellder lock HOST` with the same env), start the GUI
   binary with the same env plus `--background`, then check
   `ssh -F <config> -O check HOST` and the log for `master established`.
   There is no CLI login: only an ssh the app started gets an answer.
5. Clean up: `del-secret`, `docker rm -f shjt`, `rm -rf /tmp/shjt`.

Without Docker, `cargo test --test ssh_master` covers the prompt path with a
key passphrase and a host key confirmation against a throwaway sshd started
as the current user (password authentication needs root, so it is not
covered that way).

Gotchas:

* ControlPath must be short (unix socket paths are limited to about 104
  bytes). Use `/tmp/shjt`, not a deep scratch directory.
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
* Host key confirmations need the app (it shows the fingerprint). Use
  `StrictHostKeyChecking no` in the test config.
* **How a question is recognised.** The pty reader logs every complete line
  as `SSH host: line` and treats text left without a newline for 200 ms,
  ending in `:` or `?`, as a question; the whole text since the last answer
  is the prompt (so a host key's fingerprint lines are part of it). A
  `PROMPT` line in the log says what was asked and how it was answered.
  ssh's stderr stays on a pipe and never reaches the pty.
* The pty's slave side is held by `MasterProc` for the process's lifetime.
  Closing it is what ends the reader thread (on macOS a pty whose slave
  nobody holds reads as hung up at once), so a master that is gone from the
  daemon takes its reader with it.

## Keychain

* One item: service `shellder:vault`, account `shellder`, a JSON object
  `{host: {password|passphrase|totp: secret}}`. On Linux the same item in
  the Secret Service (`keyring` crate), which needs a running keyring
  daemon (gnome-keyring, KWallet).
* On macOS updates go through `SecItemUpdate`. The app never deletes or
  re-creates the item: a `SecItemDelete` of an item another signature made
  is a second password dialog, and has also failed with
  `errSecInvalidOwnerEdit (-25244)`, so do not "fix" the vault by deleting
  it. The first read under a new signature is the one dialog ("wants to use
  your confidential information"); "Always Allow" with the password puts
  that signature on the item for good.
* The app caches the vault for the whole run. Every write touches
  `vault.stamp` in the state dir with the writer's pid, and the app drops
  its cache when another process wrote it, so `shellder add-secret` still
  shows up within a few seconds.
* A refused read is remembered for `keychain::REFUSAL_TTL` (five minutes)
  and repeated without another dialog. Writes (`set`, `delete`) always ask
  the keychain afresh and fail when it refuses, they never write over a
  vault they could not read.
* The keychain partition is `teamid:` plus the Team ID of the signature, so a
  build signed by another team (or unsigned, as `--debug` builds are)
  triggers a keychain password dialog, or a refusal where no dialog is
  possible (an ssh session: "User interaction is not allowed"). To poke at
  the vault without dialogs, sign the test binary with the same Apple
  Development identity (`build.sh` does, keeping the identifier
  `local.shellder`).
* A keychain dialog blocks the app until answered. From a script `kill -9`
  the test copy to dismiss it.

## Driving a test GUI by hand

1. Copy `build/Shellder.app` to `/tmp/shjt/ShellderTest.app` and change
   `CFBundleIdentifier` in its `Info.plist` to `local.shellder.test` so
   window lookup does not resolve to the user's own running Shellder; re-sign
   with the Apple Development identity and `--identifier local.shellder`
   (keeps the vault readable).
2. Launch it with the `SHELLDER_*` variables (`open --env VAR=value ...` when
   it must be visible to System Events; a GUI started from a sandboxed shell
   is invisible to the desktop automation tools).
3. The pages are ordinary web pages: a change in `ui/` only needs the window
   reopened, not a rebuild, when running `cargo tauri dev` from `src-tauri`.
4. Quit it through the tray menu or ⌘Q; `kill` (SIGTERM) also stops the
   masters cleanly.

## Testing the updater

`src-tauri/src/app/updater.rs` is tauri-plugin-updater: it reads
`SHELLDER_UPDATE_API` (a `latest.json` with `version`, `notes`, `pub_date`
and `platforms.<target>.{url, signature}`) and installs the artifact for
this platform after checking its minisign signature against the public key
in `tauri.conf.json`. To try it without a release: build with
`TAURI_SIGNING_PRIVATE_KEY` set to a key of your own, bump `version` in
`tauri.conf.json` and `Cargo.toml` for the second build, serve its
`target/release/bundle/macos/Shellder.app.tar.gz` and `.sig` with
`python3 -m http.server`, and write a `latest.json` next to them
(`scripts/latest-json.sh`). Start the first build with
`SHELLDER_UPDATE_API=http://127.0.0.1:PORT/latest.json`; the log shows
`update: version X is available` a few seconds in, "Install and relaunch" in
Settings replaces the bundle and the relaunched process logs a fresh
`daemon starting`.

## Conventions

* Commit messages: `[component] description`, English. Do not commit or push
  unless asked.
* Do not add migrations for prefs or the vault format.
* Never log secret values or their lengths.
* No personal data (real host names, accounts, Team IDs) in the repository.
* `cargo fmt` and `cargo clippy --all-targets` before handing over.
