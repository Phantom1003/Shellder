# The keychain

Every credential Shellder stores (a password, a key passphrase, a TOTP secret) is in the vault, one item of the system's keychain: the login keychain on macOS, the Secret Service on Linux. This page explains when macOS asks for the keychain password and how to sign a build so that it asks once only, what the vault needs on Linux, and how to run a copy of the app apart from your own for testing.

## The login keychain on macOS

The login keychain checks two things before handing a credential to an app:

- the item's access list: who may read it;
- the app's *partition*: `teamid:…` for an app signed with an Apple-issued certificate, otherwise the build's cdhash.

A partition mismatch is what makes macOS insist on the keychain password.

### Signing, so that rebuilds are silent

Sign with an Apple-issued certificate that carries a Team ID:

1. Sign into Xcode with any Apple ID: Settings → Accounts → Manage Certificates → + → Apple Development. A free account's "Personal Team" is enough.
2. Build. `build.sh` picks the certificate up automatically, or uses the one set in `SHELLDER_SIGN_IDENTITY`.

Without such a certificate `build.sh` stops instead of signing ad hoc, since an ad hoc build is a new identity on every rebuild, which asks again each time. `SHELLDER_SIGN_IDENTITY=-` forces ad hoc anyway when you must.

### The first read under a new identity

The first read under a new identity asks for the keychain password once.

- Answer "Always Allow": that puts the identity on the vault for good.
- "Allow" is for this one read, and the next one asks again.

Shellder never deletes or re-creates the vault. So two copies with different signatures (say a release in `/Applications` started at login and a build of your own started by hand) share it, after one such request each.

### A request dismissed

A request dismissed at launch is not made again for five minutes. The *Keychain* section of the [Settings window](settings.md) asks right away.

These requests never offer Touch ID.

## The keychain on Linux

The vault is one item in the Secret Service's default keyring, so a keyring daemon (GNOME Keyring, KWallet) must be running and unlocked. On a desktop session it is.

A session that has no default keyring yet (autologin, a login without a password) reads as an empty vault, and the first credential saved creates one: the daemon asks for a password for it then.

The menu bar icon needs a StatusNotifier host: KDE has one, GNOME needs the AppIndicator extension.

## Isolated testing

A copy of the app can be run apart from your own, with its own ssh config and state. This is what the automated end-to-end test against a local sshd container uses.

| Variable | Effect |
| --- | --- |
| `SHELLDER_SSH_CONFIG=<file>` | Shellder reads that ssh config, and passes it to ssh as `-F` |
| `SHELLDER_STATE_DIR=<dir>` | relocates the prefs, the lock and the log |
| `SHELLDER_NO_CONTROLMASTER=1` | takes ControlMaster away: the app behaves as on a platform whose ssh has none, see [Without a socket](without-a-socket.md) |
| `SHELLDER_NO_KEYCHAIN=1` | the vault reads as empty and is never written, so a test copy makes the keychain ask for nothing |

Run the binary directly (`build/Shellder.app/Contents/MacOS/shellder`), so that the variables are inherited.

The vault itself is not isolated by the first three: a test copy without `SHELLDER_NO_KEYCHAIN` reads and writes your real vault.

Building and the command line are in [Build and run](build.md).
