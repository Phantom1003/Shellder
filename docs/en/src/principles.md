# Principles

Shellder keeps `ssh -M -N` master connections open in the background, so that every later `ssh host`, `scp`, `rsync` or VS Code Remote session reuses the socket and never asks for a password again. When a master has to be established, or established again, Shellder answers the password, key passphrase and TOTP questions itself from your keychain, and passes on to you in the [prompt window](prompt-window.md) anything it cannot answer, unknown host keys included. It runs on macOS and Linux.

A host without a `ControlPath`, or any host on a platform whose OpenSSH has no ControlMaster (Windows), is kept connected all the same, for the port forwards its config asks for: the same `ssh -N`, only without the socket, so other ssh commands cannot reuse it. The Console and Files windows then log in on their own, with Shellder answering the questions. See [Without a socket](without-a-socket.md).

## Your ssh config is the only configuration

Shellder lists every `Host` block of `~/.ssh/config`, following `Include`, resolves each with `ssh -G`, and never writes to that file. `ControlMaster`, `ControlPath` and `ControlPersist` come from your config.

If a host has no `ControlPath`, its connection is kept alive without a socket, and the host page shows the snippet that would give it one.

## What the master is

The master runs as `ssh -M -N <alias>` plus a few options that apply to that process only:

| Option | Why |
|---|---|
| `ControlPersist=no` | so that Shellder owns the process |
| `NumberOfPasswordPrompts=1` | so that a wrong credential is one failed login |
| `LogLevel=ERROR` | |
| `ServerAliveInterval` | only if your config leaves it at 0 |

## One keychain item

Credentials live in the keychain in a single item, the vault (service `shellder:vault`, a JSON object keyed by host): a generic password in the login keychain on macOS, a Secret Service item on Linux. One item means the keychain asks for permission at most once. See [Keychain](keychain.md).

## Credentials go to ssh and nowhere else

Credentials only ever leave the keychain towards an ssh the app itself started.

- Each master gets a pseudo-terminal of its own as its controlling terminal, which is where ssh asks its questions (`/dev/tty`). The app reads them off that terminal and writes the answer back.
- There is no helper program and no socket, so nothing outside the app can ask it for a credential.
- The command line has no command that prints a credential or logs in with one.

## State outside the keychain

| Where | What |
|---|---|
| `~/Library/Application Support/shellder/` on macOS, `~/.local/state/shellder/` on Linux | `prefs.json`, the lock, the last TOTP code, `shellder.log` |
| `~/Library/LaunchAgents/local.shellder.plist` on macOS, `~/.config/autostart/local.shellder.desktop` on Linux | the login item, optional (see [Settings](settings.md)) |
