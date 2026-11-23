# The main window

The main window is where you see every host of your ssh config and how its connection is doing. The host list is on the left, the host page of the selected host on the right, and the log panel can be opened underneath. Connecting, locking, the credentials and the forwards of a host are all set here; the host's shell, files and readings are in its [Console](console.md).

{{#include figures/main-window.html}}

## The host list

The host list has one host row for every `Host` entry of `~/.ssh/config`, files brought in with `Include` among them. A click on a row selects the host and shows its host page. The list's right edge drags to make it wider or narrower.

A host row has, from left to right:

- the **status dot**, which says how the connection is (see the table below); the pointer resting on it names the state;
- the host's name, and under it the address ssh resolves it to (`user@hostname`). While `ssh -G` has not answered yet this line says *resolving…*, and *config error: …* when it failed;
- the **Console button**, there only while the pointer is on the row. It opens the host's [Console](console.md) without selecting the host;
- the **Connect switch**. Switching it on is one attempt to connect, switching it off closes the master. Using it also selects the host, so the host page shows what the attempt does. See [The switch and the lock](switch-and-lock.md).

The status dot and the Connect switch wear the same colour:

| Colour | Meaning |
|---|---|
| grey | off |
| yellow | an attempt or a retry is under way, or the host waits for its jump host |
| green | the master is up |
| blue | the master is up and the host is locked |
| cyan | a master is up that is not Shellder's own (one left by an interactive `ssh` with `ControlMaster auto`). The lock does not keep that one; *Take over* replaces it |
| red | the last attempt failed. The dot stays red until the switch goes on again |

The switch itself is plain grey whenever it is off, whatever the last attempt did.

When the ssh config has no `Host` entry, the host page says *No Host entries in ~/.ssh/config*.

## The list toolbar

The list toolbar is the row of buttons under the host list.

| Button | Key | What it does |
|---|---|---|
| reload | ⌘R | Reads `~/.ssh/config` again |
| edit | ⌘E | Opens `~/.ssh/config` in your editor. Shellder never writes to that file |
| Settings | ⌘, | Opens the [Settings window](settings.md) |
| update | | Shown only when a newer version is available; opens the Settings window, where it is installed |
| log | ⌘L | Opens and closes the log panel |

The host list can be folded away so that the host page takes the whole window: the button in the host page's top left corner does it and brings the list back (⌘⇧S). The choice is kept on this machine.

## The host page

The host page is the right side of the window: everything about the selected host, in cards. From top to bottom: the connection card, a notice card when there is something to say about the host's configuration, the Tools card, the Credentials card and the Configuration card.

### The connection card

The connection card is about the connection itself.

- **The host's name**, and under it the **status line**: the status dot again and the state in words, with how long it has lasted. The states are *Disconnected*, *Connecting…*, *Connected*, *Connected (no socket)*, *Connected (external master)*, *Dropped — retrying in 5s*, *Retrying…*, *Waiting for jump host …* and *Failed (details in log)*. On a jump host the line adds *jump host for …* with the hosts that go through it.
- Under the status line, the address the host resolves to (`user@hostname`, with the port when it is not 22, and *via …* for a `ProxyJump`), and the other names of the same `Host` entry (*also: …*).
- **The connection buttons**, at the right. Which ones are there depends on the state:

| Button | Shown when | What it does |
|---|---|---|
| *Lock* / *Unlock* | always | Sets and clears the lock. Locking switches the host on if it is off |
| *Connect* | the host is off | The same as the Connect switch: one attempt |
| *Disconnect* | the master is up or connecting | The same as switching off: closes the master, clears the lock, and switches off the hosts that jump through this one |
| *Reconnect* | the master is up or connecting | Closes the master and connects again (⌘K). The switch and the lock stay as they are |
| *Retry now* | the host is on and waiting for its next try | Tries at once instead of waiting |
| *Take over* | the master up is not Shellder's | Closes that master and starts one Shellder owns. Switches the host on |
| *Close socket* | a master is up | Runs `ssh -O exit`, which shuts the master down and removes its socket; this also works on a master started by an interactive `ssh`. The switch goes off and the lock with it, and the hosts that jump through this one go down as well |

- **The keep-alive mode** (the *Keep-alive* row): what the master runs on the host so that the server keeps the connection. The choices are *no session (-N)*, *idle shell (pty)* and *cat on stdin*, remembered per host. See [keep-alive modes](switch-and-lock.md#keep-alive-modes).
- **The reconnect rule** (the *Custom reconnect rule* row): a switch. Off, the host follows the default rule from Settings, which the row spells out beside the switch (*5s → 10 min, doubling*). On, the host gets a copy of the default rule as a rule of its own, and its fields appear underneath. See [the reconnect rule](switch-and-lock.md#the-reconnect-rule).
- A *Last error* row, on a host that is still on (retrying, say) after a try went wrong, with what ssh said.

A host whose attempt failed and that is now off shows *Failed (details in log)* on the status line; the reason itself is in the log panel.

### Notice cards

Under the connection card a notice card appears in three cases:

- **No ControlPath.** The card is titled *No ControlPath for this host*. The connection is kept alive for its port forwards, but without a socket no other ssh command, Console or Files window can reuse it. Shellder never edits `~/.ssh/config`, so the card shows a snippet to add yourself (`ControlMaster auto`, a `ControlPath`, `ControlPersist`), with *Copy* and *Edit config…* beside it. A `Host *` block covers every host.
- **No ControlMaster.** On a platform whose ssh has no ControlMaster the card says so: the connection is kept alive for its port forwards, and every Console and Files window logs in on its own, with Shellder answering the questions.
- **A configuration that does not resolve.** When `ssh -G` fails for the host, a card titled *ssh -G host failed* shows what ssh said.

The first two cases are described in [Without a socket](without-a-socket.md).

### The Tools card

The Tools card has two large buttons and, under them, the host's forwards and direct connections.

- **Console** opens the host's [Console](console.md) (⌘T for the selected host): a shell on the host, and beside it its readings, sessions, files and ports.
- **Files** opens the host's [Files window](files-window.md): two panes side by side, where a drag copies.

Both work with or without a master. Through a master with a socket there is no login; otherwise the window logs in on its own, and Shellder answers the questions from the keychain or passes them on in the [prompt window](prompt-window.md).

The + on the card's title adds one of two things:

- *New forward…*: a forward, the same ones the Console's [Ports panel](console-ports.md) shows.
- *Direct connection…*: a port of the host opened at the host's own address (the `HostName` of its ssh config) with no forward, in the VNC viewer for a port from 5900 to 5999 and in the browser otherwise.

Both are listed under the buttons, forwards first, and in the Ports panel. They are kept on this machine, so a port is a click away without opening the Console.

| Row | A click | Its buttons |
|---|---|---|
| forward row | A `-L` forward opens its near end here, made first when it is not up. A `-R` forward is made when it is not up | start or stop, and forget. The row says *up* or *not up* |
| direct connection | Opens the port at the host's address | forget |

### The Credentials card

The Credentials card lists the credentials stored for the host in the vault: a password, a key passphrase, a TOTP secret, one credential row each.

- The + on the card's title adds a credential of a kind not stored yet. A dialog asks for the value; a TOTP secret is entered as a base32 secret or an `otpauth://totp/…` URI.
- The bin beside it removes every credential of the host, after asking.
- A credential row has three buttons: show (the eye) reveals the stored value, with *Copy* beside it and, for a password or passphrase, its number of characters; change (the pencil) asks for a new value; remove (the bin) deletes the credential from the keychain.
- The TOTP row also has *Show code*: the current code and the seconds it still has, to compare with your authenticator app.

Values shown do not stay shown: they are hidden again when the window goes away.

When the keychain will not open the vault, the card says why. What is stored is still in it; allow Shellder when the keychain asks, then press *Try Again*. See [Keychain](keychain.md).

### The Configuration card

The Configuration card, titled *Effective configuration*, shows what `ssh -G` says the host's configuration comes to: `HostName`, `User`, `Port`, `ProxyJump`, `ProxyCommand`, `IdentityFile`, `ControlMaster`, `ControlPath`, `ControlPersist` and `ServerAliveInterval`, and the file the entry is *Defined in*.

- A `ProxyJump` that names a host from the list is marked as such: its master is brought up first and kept alive with this one (see [Jump hosts](jump-hosts.md)).
- A `ServerAliveInterval` of 0 is marked too: Shellder adds 15 s for its own master.
- *Edit ~/.ssh/config…* on the card's title opens the file in your editor.

## The log panel

The log panel shows the end of Shellder's log under the two columns. ⌘L or the list toolbar's log button opens and closes it, and its top edge drags to resize it; the height is remembered.

- The filter field keeps only the lines that contain what you type: a host name, `ERROR`, and so on.
- *Follow* keeps the panel at the newest line.
- *Open file* opens `shellder.log` itself.

The log is where a failed attempt's reason is written, every line ssh printed (`SSH host: …`), and for every question what was asked and how it was answered (`PROMPT`). The log is always in English, and never contains a credential.

## Keys

| Key | What it does |
|---|---|
| ⌘T | the selected host's Console |
| ⌘K | reconnect the selected host |
| ⌘R | reload `~/.ssh/config` |
| ⌘E | edit `~/.ssh/config` in your editor |
| ⌘L | the log panel |
| ⌘⇧S | fold the host list away, and bring it back |
| ⌘, | the Settings window |
| ⌘W | close the window |
| ⌘Q | quit Shellder |

On Linux, Ctrl takes the place of ⌘. The keys of the other windows are in [Keys](keys.md).

## The menu bar icon

With *Run in background* and *Show in menu bar* switched on in [Settings](settings.md), Shellder has an icon in the menu bar.

- A click opens the main window; another click while the window is in front puts it away.
- A right click shows the menu bar menu.
- The icon is faded while no master is up and plain as soon as one is. Errors and questions do not change it.
- The pointer resting on it says how many masters are up.

On Linux the panel shows the picture as drawn, dark; *Invert menu bar icon* in Settings turns it light for a dark panel. The icon needs a StatusNotifier host there (KDE has one, GNOME needs the AppIndicator extension).

### The menu bar menu

| Item | What it does |
|---|---|
| *Open Shellder* | Shows the main window |
| one host submenu per host | See below. ● marks a host whose master is up, ◐ a master that is not Shellder's, ○ any other |
| *Other hosts (n)* | The rest of the hosts, when there are more than fit: hosts that are on or connected come first, six hosts are listed, and the others are folded in here |
| *Connect All*, *Disconnect All* | Switches every host on, or off |
| *Update to …* | Shown when a newer version is available; opens the Settings window |
| *Settings* | Opens the Settings window |
| *Quit Shellder* | Quits the app and closes its masters |

### A host submenu

A host submenu has the same connection buttons as the host page, for the same state: *Lock* or *Unlock*, then *Connect*, *Disconnect*, *Reconnect*, *Retry now*, *Take over* and *Close socket*, whichever apply. Under them:

- *Shell* runs `ssh host` in the application that handles `ssh://` links. That ssh is not Shellder's: without a master to reuse, it asks you itself.
- *Files* opens the host's Files window.
- *Console* opens the host's Console.
- *Show in Shellder* opens the main window with the host selected.
