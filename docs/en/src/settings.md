# Settings

The Settings window holds the app's own settings: how it starts, whether it stays in the background, its language, access to the keychain, the default reconnect rule and updates. It opens with ⌘, from any window, from the list toolbar of the [main window](main-window.md) and from the menu bar menu. The terminals' settings are not here: they are in the Console's own [settings window](console-settings.md).

{{#include figures/settings.html}}

## Startup

| Setting | What it does |
|---|---|
| *Start at login* | Adds Shellder to the login items: `~/Library/LaunchAgents/local.shellder.plist` on macOS, `~/.config/autostart/local.shellder.desktop` on Linux |
| *Start silently* | Skips the main window when the login item starts the app |

Both are off until you switch them on: nothing is written to `~/Library/LaunchAgents` before you ask for it.

*Start silently* can only be switched on together with *Start at login* and *Run in background* (without the latter there would be no window and no icon), and it goes off with *Run in background*. It concerns the login item only: a double click or `open` always opens the window. `shellder --background` forces a start without a window for a single launch.

## Background

| Setting | What it does |
|---|---|
| *Run in background* | Closing the last window leaves the Dock and keeps the connections alive, instead of quitting |
| *Show in menu bar* | Shows the [menu bar icon](main-window.md#the-menu-bar-icon). Available with *Run in background* |
| *Invert menu bar icon* | Linux only: turns the icon light for a dark panel. macOS tints the picture itself, so the setting is not shown there |

By default, closing the last window quits the app and its masters. With *Run in background* on, the app stays reachable from the menu bar icon (a click opens the main window, another one while it is in front puts it away, a right click shows its menu) or by launching it again.

A Console or Files window minimised to the Dock still counts as open: closing the main window beside it leaves the app in the Dock with that window.

Quitting takes every ssh the app started with it, those of the terminals, the sessions and the copies included. None of them is allowed to persist as a background master: `ControlPersist` is off for them, whatever the config says, because macOS 26 reports an app whose processes stay behind as still running in the background.

The menu bar icon is faded while no master is up and plain as soon as one is; errors and questions do not change it. On Linux the panel shows the picture as drawn, dark, which is what *Invert menu bar icon* is for.

## Appearance

*Language* chooses the interface language. It is English unless you pick another, whatever the system language, and a change shows after a relaunch: the *Relaunch to apply* button appears under the menu once you have changed it.

English and Simplified Chinese are built in. The log stays English.

To add a language, copy `ui/lang/zh-Hans.json` to `ui/lang/<lang>.json` (a BCP 47 name such as `ja` or `zh-Hant`), translate the right-hand sides (the keys are the English text, `{name}` is a placeholder) and add the language to the list at the top of `ui/settings.js`. Keys a translation lacks fall back to English. See [Building](build.md).

## Keychain

The *Vault* row says whether Shellder can read the vault: *readable*, or the reason the keychain gave for refusing.

*Ask the keychain for access* asks again right away. All credentials live in one keychain item, so if the keychain was not allowed to open it, nothing is lost. A keychain question dismissed at launch is otherwise not asked again for five minutes. See [Keychain](keychain.md).

## Reconnect

This section, titled *Reconnect policy*, holds the default rule: how a locked host comes back after a drop. Every host follows it unless its host page gives it a rule of its own.

| Row | Meaning |
|---|---|
| *Wait between tries* | *Doubling* or *Fixed* |
| *First wait* (*Wait* when fixed) | the wait after the first failed try, in seconds |
| *Longest wait* | where the doubling stops, in seconds |
| *Give up* | *Never*, or after how long down |
| *Own rules* | the hosts that have a rule of their own, each with its rule in a line. Shown only when there are any |
| *Health check* | how often a master that is up is checked. Shown, not changed here |
| *Connect timeout* | how long a connection attempt may take. Shown, not changed here |

Out of the box the waits double from 5 s up to 10 min and nothing gives up. What the fields mean, and how the waits are counted, is in [the reconnect rule](switch-and-lock.md#the-reconnect-rule).

## Files

The paths Shellder works with:

| Row | What it is |
|---|---|
| *ssh config* | the ssh config Shellder reads; the button opens it in your editor |
| *Included* | the files it brings in with `Include`, when there are any |
| *Log* | `shellder.log`; the button opens it |
| *State directory* | where the preferences, the lock and the log are kept |

A warning appears here when the login item points at another copy of the app than the one running.

## Software update

The last section shows the *Version* and the state of updates.

- With *Check for updates automatically* on (the default), the app reads the latest release's `latest.json` from GitHub a few seconds after launch and once a day. When a newer version is there, it says so here, in the main window's list toolbar and in the menu bar menu.
- *Check now* asks right away.
- Nothing is installed until you click *Install and relaunch*. That downloads the release's updater artifact for this platform, checks its signature against the key built into the app, swaps it for the running bundle in place and relaunches. *Release notes* opens the release's page.
- A release only counts when its version is newer than the running one, so a local build with the same number stays put.

Release builds are signed with the maintainer's Apple Development certificate, so the first launch after updating from a build you signed yourself may show one keychain question. See [Keychain](keychain.md).
