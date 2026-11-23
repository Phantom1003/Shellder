# The terminals

A terminal tab is xterm.js on the screen pty of an `ssh -t host …`, or of your login shell when the tab is on this machine. The width of emoji and East Asian characters is the one Unicode 11 gives them. The letters are drawn by the web view itself unless *Draw with WebGL* is switched on in the [Console settings](console-settings.md), in which case the terminal is drawn with WebGL where the view gives a context. This page covers what a terminal does beyond showing a shell: the clipboard, links, the find strip, the two bells, notifications, the password offer and the shell integration. Tabs, groups and splits are in [Tabs and groups](console-tabs.md).

{{#include figures/console-terminals.html}}

## Clipboard, keys and letter size

What a program on the host writes to the clipboard (OSC 52: tmux with `set-clipboard on`, a vim with its clipboard) lands on this machine's clipboard. A program that asks to read the clipboard gets nothing, since anything printed on a host could ask.

| Key | Does |
| --- | --- |
| ⌘C | copies the selection; without one it is the interrupt, as the key goes to the shell |
| ⌘V | pastes |
| ⌘A | selects all |
| ⌘K | clears the terminal |
| ⌘+, ⌘−, ⌘ and the wheel | the letters bigger or smaller |
| ⌘0 | the letters back to 13 points |

The letter size applies to every terminal of the window and to the ones opened after it, and it is kept for the next window. With *Copy on select* switched on in the Console settings, a selection is copied as it is made.

A held key repeats in the terminals: the app switches macOS's accent popup (press and hold) off for itself, as terminal applications do.

All the keys, with the ones that go to the shell (⌘←, ⌥→, ⇧↩ and so on), are in [Keys](keys.md).

## Links

### Web links

Web addresses in the text, and links a program marks (OSC 8), are links. A ⌘-click (Ctrl-click on Linux) opens one in the browser. A plain click is a click and opens nothing.

### Links under tmux

tmux passes a program's links on only to a terminal it knows takes them.

- A tmux the Console starts (a session of the [Sessions panel](console-sessions.md), or a new one) is told so with `tmux -T hyperlinks`. The option exists from tmux 3.2 on; the links come with tmux 3.4.
- A tmux you start yourself in a shell tab needs this line in its `tmux.conf`:

  ```
  set -as terminal-features ",xterm-256color:hyperlinks"
  ```

  Without it a link such as Claude Code's session name shows as bare text.

### Path links

Paths are links too: absolute ones, and ones under the shell's directory when the shell says where it is (see [Shell integration](#shell-integration)).

- A ⌘-click shows the file in the [Files panel](console-files.md): its directory is entered and the file selected. A path ending in a slash enters that directory.
- A ⌥⌘-click shows the file in a preview tab of the group instead.
- On a terminal of another host than the one the window shows, the window switches to that host first, as the host menu does. When another window already shows that host, that window comes up and the file is shown in this one's Files window.

## The find strip

⌘F opens the find strip in the terminal's top right corner.

- Every match is marked, the one reached brighter, and the strip shows the count.
- ↩ goes to the next match and ⇧↩ to the previous one; the strip's arrows do the same.
- A capital in the text makes the search mind the case.
- Esc puts the strip away and gives the keyboard focus back to the terminal.

The same strip finds in a preview tab.

## The terminal menu

The right button over a terminal opens the terminal menu:

| Item | Does |
| --- | --- |
| *Copy* | copies the selection |
| *Paste* | pastes |
| *Select all* | selects everything in the terminal |
| *Select last* | selects the last command's output (needs the shell integration) |
| *Find…* | opens the find strip |
| *Clear* | clears the terminal |
| *Open in Files* | shows the directory the shell is in in the Files panel; the directory it started in when the shell integration is off. A folded side bar comes out for it |

In the quick terminal, which has no side bar, the last item is the one that switches the background instead (see [the quick terminal](console-window.md)).

## The terminal bell

The terminal bell is the bell a program rings (BEL). In a terminal on screen it does what *Bell* in the Console settings says:

| *Bell* | Does |
| --- | --- |
| *Flash* (the default) | the terminal's edge flashes orange for a moment |
| *Sound* | a sound plays |
| *Flash and sound* | both |
| *Neither* | nothing |

A tab out of sight gets its tab mark either way.

The sound is picked in the row below *Bell*: the system's alert sound (the one chosen in the system's Sound settings), another of the system's sounds (on macOS those of the Sounds folders, `~/Library/Sounds` included; on Linux those of the sound themes), or a file of your own (AIFF, WAV, MP3, M4A, Ogg, FLAC). The ▶ beside it plays it.

## The ready bell

The ready bell is the bell button in a terminal's top right corner, the same corner as the find strip. It has nothing to do with the terminal bell. Switched on by a click, it tells every time the terminal is ready for input again (its command done, which the shell integration reports, or a question waited on), until it is switched off or the terminal ends.

How it tells depends on where you are looking:

| Where the terminal is | What you get |
| --- | --- |
| The window is behind another | a notification, whatever the *Notifications* switch says |
| The window is in front, but the tab is not shown or the keyboard focus is in another group | the lit group: an orange glow round the tab's group, until the terminal gets the keyboard focus |
| The terminal was being watched | a toast |

The group's ready bell, among the group buttons beside the background button, switches the ready bell of every tab of the group on, those opened in the group later too, and off again. A tab's own ready bell can still be switched either way afterwards.

The ready bells are kept with the layout. The quick terminal has none.

## Tab marks

A tab that is not shown gets a tab mark, a dot, when something happens in it:

| Tab mark | Means |
| --- | --- |
| plain dot | output arrived |
| orange | the terminal bell or the ready bell rang |
| green | a command ended well (shell integration) |
| red | a command ended badly (shell integration) |

The tab mark goes when the tab is brought up.

## Notifications and toasts

What happens in a tab nobody is watching (the tab is not shown, or its window is not in front) is told as a notification, with the Dock asking for attention. The same event in the tab in front is a toast.

| Event | Notes |
| --- | --- |
| A command failed | needs the shell integration |
| A command took more than ten seconds | told when it ends; needs the shell integration |
| A command waits on a question | its output rested a few seconds on a line such as `[y/N]`, `Password:` or one ending in `?` |
| The terminal bell | told as a notification when the window is not in front |
| A program's own notification | OSC 9 (iTerm2's), OSC 777 (Ghostty's and rxvt's), OSC 99 (kitty's) |

Of kitty's OSC 99 the title and the body are taken, in chunks too, with `o=unfocused` and `o=invisible`; a program's question about what the terminal takes is answered.

OSC 1337 `RequestAttention=yes` (iTerm2's) has the Dock ask for attention until the app is active, `once` once, and marks the tab as the terminal bell does.

How notifications behave:

- They are the *Notifications* switch in the Console settings. The ready bell's notification comes even with the switch off.
- A notification shows as a banner also while Shellder is in front (its tab is out of sight then).
- A click on a notification brings its window up and the tab in it.
- The notifications of a tab leave the notification centre once the tab has been looked at.
- No more than a dozen of the Console's notifications are left standing.
- A program that sends its own notifications by the hundred is held to a few in ten seconds per tab; the rest are dropped.
- macOS shows none for a binary outside an app bundle (a `cargo tauri dev` run): the log says `notification not shown`, once, and why.

### Notification sounds

A notification can come with a sound, picked below *Notifications* the same way the terminal bell's is (none by default).

- On macOS one of the system's sounds is the notification centre's to play: it keeps quiet with the notification in a Focus and follows the app's sound switch in the system's Notifications settings.
- A file of your own the notification centre cannot play, so Shellder plays it itself, Focus or not. A copy put in `~/Library/Sounds` is listed among the system's sounds.
- A terminal bell told as a notification with a sound does not also play the bell's sound.

## Tab progress

A program's progress (OSC 9;4, ConEmu's, which systemd, winget and others send) is shown as the tab progress, a thin bar along the tab's foot. In the quick terminal, which has no tab bar, it runs along the top of the terminal.

- Red is an error, orange is paused, and a piece sliding back and forth means the program gives no percent.
- It goes at the next prompt, when the terminal ends, or a quarter of a minute after the program's last word on it.

## Programs inside tmux

A program in a tmux pane is heard only through tmux.

- The terminal bell gets through by itself, unless the `tmux.conf` has `bell-action none` or `visual-bell on`.
- tmux keeps every notification to itself unless the program wraps it in tmux's passthrough (as programs do when they find themselves under tmux) and `allow-passthrough` is on.
- A tmux the Console starts switches `allow-passthrough` on for its server when it is off; an `all` of yours is left as it is.
- A tmux you start yourself in a shell tab needs `set -g allow-passthrough on` in its `tmux.conf`.

## The paste question

Several lines pasted into a shell that is not reading a bracketed paste would run as they land, every line but the last. So the paste question, a strip over the terminal, asks first: ⌘↩ or *Paste* pastes, Esc does not.

A shell of today, or an editor, reads a bracketed paste and is not asked for. The question is the *Ask before pasting several lines* switch in the Console settings.

## The password offer

When the shell in a tab asks for a password itself (`sudo`, `su`) and a password is stored for the host, the password offer appears in the terminal's top right corner. It names the account whose password it is (`alice@work`).

**When it appears.** The cursor's line ends in a password prompt, the cursor waits after it, and the output has been still for a moment. A password stored while the prompt waits (in the main window's Credentials card, or with `shellder add-secret`) is offered then.

**What it does.** A click on *Type*, or ⌘↩, types the stored password with a Return. This happens on the app's side: the page never sees the password. Nothing is typed without the click or the key.

**How it goes away.** Typing anything else, the ×, or a minute without an answer puts it away, and the same prompt is not offered twice. An offer on show is taken back when the password is removed, and the same prompt is offered again when one is stored.

**Which host's password.** The host is the one the tab was opened on.

- A shell that went on to another host with an `ssh` typed into it is still offered the password of the tab's host.
- A tab on this machine, which has no password stored, is offered none.
- A prompt carrying a `user@host` (a login from the shell to another host) or asking for a passphrase gets no offer: the stored password is this host's.

Credentials are stored on the host page of the [main window](main-window.md); the questions ssh itself asks are answered in the [prompt window](prompt-window.md), not here.

## Shell integration

A shell started from the Console (a new tab, *Shell here*) gets a few lines of Shellder's in its startup, for bash, zsh and fish. A tmux or screen session is not covered: its shell was started before. Any other shell starts as it is. The shell integration is the *Shell integration* switch in the Console settings.

### What it marks

- The prompt, the command and its end with its exit status (OSC 133). This is what iTerm2, VS Code and Ghostty speak too, so a shell that already has one of their integrations is understood as well.
- The shell's directory (OSC 7).

### What the Console does with it

- Names the tab after the directory, unless the shell sets a title itself.
- Opens a new shell (⌘T, the new-tab button) in the directory of the shell with the keyboard focus.
- Scrolls to the previous or the next prompt mark with ⌘↑ and ⌘↓. The command reached, from its prompt to the end of its output, is framed for a moment.
- Selects the last command's output with ⌘⇧A (*Select last* in the terminal menu).
- Tells of a command's end and of one waiting for input (the tab marks, the notifications and the ready bell above).
- Asks before closing a tab, or the window, with a command still running in it (the *Ask before closing a tab with a command running* switch).
- Follows relative path links and *Open in Files*.

### How a shell gets it

On a host the lines go over in the ssh command and land in a directory made for the tab, which the shell deletes once it has read them. Nothing is written into your dotfiles.

| Shell | How |
| --- | --- |
| bash | as the rcfile of an interactive shell that first reads what a login shell would (`/etc/profile`, then `~/.bash_profile`, `~/.bash_login` or `~/.profile`). Marking a command's start needs bash 4.4 (`PS0`); an older bash marks prompts and directories only |
| zsh | through a `ZDOTDIR` of Shellder's whose `.zshenv` puts yours back, so your files are read as always. The hooks go in at the first prompt, after `.zshrc` |
| fish | through a `vendor_conf.d` on `XDG_DATA_DIRS` |

### Prompt themes and tmux

A prompt theme that draws the prompt anew (Powerlevel10k) may drop the mark for the prompt's end. The command's name is then read from the line as best it can be. Powerlevel10k's own `POWERLEVEL9K_TERM_SHELL_INTEGRATION=true` puts the marks back.

Inside tmux the prompt marks of the inner shell do not reach the Console.
