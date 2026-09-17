# Shellder

[![Build](https://github.com/Phantom1003/Shellder/actions/workflows/build.yml/badge.svg)](https://github.com/Phantom1003/Shellder/actions/workflows/build.yml)

**Shellder keeps your SSH connections warm.** It is a small macOS menu bar app
that holds `ssh -M -N` master connections open in the background, so every later
`ssh`, `scp`, `rsync` or VS Code Remote session reuses the socket and connects
instantly, with no password, passphrase or one-time code to type.

When a master has to be (re)established, Shellder answers the prompts itself
from your login keychain, including TOTP codes, and only shows a dialog for
things it cannot answer, such as an unknown host key.

## Why

Servers behind a jump host, a VPN that drops every few hours, a login that
needs a password *and* a 2FA code: every hop costs a prompt, and ControlMaster
only helps while a master is alive. Shellder makes the master a thing the OS
keeps alive for you, reconnects it with back-off when the link drops, and puts
a switch and a lock per host in the menu bar.

## Features

- **Your `~/.ssh/config` is the only configuration.** Shellder lists every
  `Host` block (following `Include`), resolves it with `ssh -G` and never
  writes to the file.
- **One switch per host.** On means "connect": the master stays up as long as
  the connection lasts. Off closes it. A failed login flips the switch back
  instead of hammering the server.
- **A lock per host.** A locked host is reconnected with back-off when the
  link drops and comes back when Shellder starts. Whatever turns the switch
  off clears the lock.
- **A per-host keep-alive mode** for servers that reject session-less
  connections.
- **Secrets in the login keychain**, in a single item, so the keychain asks
  for permission at most once. Touch ID guards showing a secret in plain text.
- **Prompts answered for you.** Passwords, key passphrases and TOTP codes are
  filled in through `SSH_ASKPASS`. Unknown host keys are shown with their
  fingerprint for you to accept.
- **Starts at login, runs in the background.** Closing the window keeps the
  connections alive, and the menu bar icon brings it back.
- **Also a CLI.** The same binary can list hosts, lock them, show status,
  store secrets and test a login from a terminal.

## Quick start

Requires macOS 13 or later and the Xcode command line tools (Swift 5.9+).

```bash
git clone https://github.com/Phantom1003/Shellder.git
cd Shellder
./build.sh --install   # builds build/Shellder.app, copies it to ~/Applications and launches it
```

Prebuilt bundles for every commit are attached to the
[CI runs](https://github.com/Phantom1003/Shellder/actions/workflows/build.yml),
and tagged versions are published under
[Releases](https://github.com/Phantom1003/Shellder/releases).

Then flip the switch next to a host. The first time, Shellder asks for the
password and offers to save it in the keychain. Lock the host and it
reconnects on its own from then on.

```bash
shellder hosts               # Host entries and their ControlPath
shellder lock myserver       # keep this host connected
shellder status              # UP/down per locked host
shellder add-secret myserver password
shellder test myserver       # one-shot login with the stored secrets
```

## Documentation

The full manual, covering the switch and the lock, keep-alive modes, how prompts
are matched to hosts, code signing and keychain behaviour, and isolated
testing, is in [docs/manual.md](docs/manual.md).
