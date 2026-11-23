# Build and run

Shellder is one binary built with Tauri: started with no arguments it is the app, started with an argument it is a command-line tool. This page covers building it with `build.sh`, the app icon, how the app runs in the background and starts at login, the interface language, updates, and the command line. Signing and the keychain are in [The keychain](keychain.md).

## Building

```bash
./build.sh            # -> build/Shellder.app (macOS), build/*.deb and *.AppImage (Linux)
./build.sh --install  # also copy to ~/Applications and launch (macOS)
./build.sh --debug    # a debug build, unsigned, no updater artifacts
./build.sh --deps     # only install what a build needs
```

`build.sh` installs what is missing:

- Rust, with rustup;
- the Tauri CLI;
- on Linux, the development packages of what every Tauri app links against (webkit2gtk 4.1, libayatana-appindicator, librsvg, dbus and the rest), through apt, dnf, pacman or zypper.

`./build.sh --deps` does only that.

On macOS `build.sh` signs the build with an Apple Development certificate it finds in the keychain, or with the one named in `SHELLDER_SIGN_IDENTITY`. Why that matters, and what happens without one, is in [The keychain](keychain.md).

## The app icon

`build.sh` takes `scripts/icon-source.png` (or `.jpg`) and has the Tauri CLI crop and scale it to every size the bundle wants.

The menu bar icon is a monochrome template made from the same picture (light pixels opaque, dark ones transparent), checked in as `src-tauri/icons/tray.png`, so macOS tints it like its own status icons. After changing the source picture, make it again with

```bash
cargo run --release --manifest-path scripts/menubar-template/Cargo.toml -- \
    scripts/icon-source.png src-tauri/icons/tray.png 36
```

## What the app is made of

- The [main window](main-window.md): the host list with the Connect switches and locks, the host page with its credentials, and the log panel.
- The [Settings window](settings.md): start at login, silent start, background behaviour, the menu bar icon, the default reconnect rule. The terminals' own settings are in the [Console settings window](console-settings.md).
- The menu bar icon.

## Closing, the background and quitting

By default closing the last window quits the app and its masters.

With *Run in background* switched on, closing the window instead takes the app out of the Dock and keeps the connections alive in the background. The app is then reached from the menu bar icon, or by launching it again:

- a click on the menu bar icon opens the main window;
- another click while the window is in front puts it away;
- a right click shows the menu bar menu.

A Console or Files window minimized to the Dock still counts as open: closing the main window beside it leaves the app in the Dock with that window.

Quitting takes every ssh the app started with it, the terminals', the sessions' and the copies' included. None of them is allowed to persist as a background master: `ControlPersist` is off for them, whatever the ssh config says. macOS 26 reports an app whose processes stay behind as still running in the background.

## The menu bar icon

The menu bar icon is faded while no master is up and plain as soon as one is. Errors and questions do not change it.

On Linux the panel shows the picture as drawn, dark. *Invert menu bar icon* turns it light for a dark panel; the switch is shown on Linux only, since macOS tints the picture itself.

## Starting at login

*Start at login* registers the app as a login item. *Start silently* skips the window when the login item starts the app; a double click or `open` always opens it.

- *Start silently* can only be switched on together with *Start at login* and *Run in background* (without the latter there would be no window and no icon), and it goes off with *Run in background*.
- `shellder --background` forces a start without a window for a single launch.
- Both switches are off until you switch them on: nothing is written to `~/Library/LaunchAgents` before you ask for it.

## Language

The interface language is chosen in the Settings window: English unless you pick another, whatever the system language. A change shows after a relaunch. English and Simplified Chinese are built in; the log stays English.

To add a language:

1. Copy `ui/lang/zh-Hans.json` to `ui/lang/<lang>.json`, with a BCP 47 name such as `ja` or `zh-Hant`.
2. Translate the right-hand sides. The keys are the English text; `{name}` is a placeholder.
3. Add the language to the list at the top of `ui/settings.js`.

Keys a translation lacks fall back to English.

## Updates

The Settings window ends with the version and a *Software update* row.

- With *Check for updates automatically* on (the default) the app reads the latest release's `latest.json` from GitHub a few seconds after launch and once a day. An update found is said in the Settings window, in the main window's list toolbar and in the menu bar menu.
- *Check now* asks right away.
- Nothing is installed until you click *Install and relaunch*. That downloads the release's updater artifact for this platform, checks its signature against the key built into the app, swaps it for the running bundle in place and relaunches.
- A release only counts when its version is newer than the running one, so a local build with the same number stays put.

Release builds are signed with the maintainer's Apple Development certificate. The first launch after updating from a build you signed yourself may therefore make the keychain ask for its password once, see [The keychain](keychain.md).

## The command line

The same binary is also a command-line tool. It works without the app running. No command prints a credential or logs in with one.

```bash
shellder hosts                    # Host entries and their ControlPath
shellder lock socjump             # keep this host connected
shellder unlock socjump           # no longer keep it connected
shellder status                   # UP/down per locked host
shellder add-secret socjump password
shellder del-secret socjump password
shellder install | uninstall      # login item
shellder help
```

| Command | Does |
| --- | --- |
| `hosts` | lists the `Host` entries of the ssh config and their `ControlPath` |
| `status` | says whether the master of every locked host is up |
| `lock HOST`, `unlock HOST` | locks a host (it is reconnected after a drop and connected at launch) or unlocks it |
| `add-secret HOST KIND` | stores a credential in the vault; `KIND` is `password`, `passphrase` or `totp` |
| `del-secret HOST KIND` | removes one stored credential |
| `install` | registers the app as a login item and starts it |
| `uninstall` | stops it and removes the login item |
| `help` | lists the commands |

Started with no command, the binary is the app. `--background` starts it without a window for this launch; `--login` is what the login item passes, the only launch that honours *Start silently*.

The switch and the lock are described in [The switch and the lock](switch-and-lock.md).
