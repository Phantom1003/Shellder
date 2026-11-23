# Tabs and groups

The tab area is the right side of the Console. It holds tabs, each a terminal or a preview of a file, and the tabs are in groups, as an editor's are: a group has a tab bar and shows one of its tabs, and a split puts a second group beside or under it. This page is about the tabs and the groups themselves; what happens inside a terminal is in [Inside a terminal](console-terminals.md), and what a preview shows in [The Files panel and previews](console-files.md).

{{#include figures/console-tabs.html}}

## The tab area

The tab area starts as one group. Each group has:

- a **tab bar**: the group's tabs, the new-tab button after the last of them, and the group buttons at the right. The blank between them drags the window. When there are more tabs than fit, the tab bar scrolls sideways (the wheel does it too) and the new-tab button moves to the right, beside the group buttons, so it is never out of reach;
- under it, the tab shown.

The keys on this page are the defaults on macOS. All of them can be changed, see [Keys](keys.md).

## Kinds of tab

| Kind | What it is | How it opens |
|---|---|---|
| Shell tab | A terminal tab with a shell on the host. | The new-tab button, ⌘T, *Shell here* in the Files panel, the terminal button of a session, job or container row. |
| Session tab | A terminal tab attached to a tmux or screen session. | A click on a session row in [the Sessions panel](console-sessions.md). |
| Job tab | A terminal tab with a shell in a Slurm or PBS job. | A running job's row, or a new interactive job, in the Jobs panel. |
| Container tab | A terminal tab with a shell inside a container. | A click on a container row in the Containers panel. |
| Preview tab | A file of the Files panel. | A click on a file row in [the Files panel](console-files.md). |

A shell tab is named by its shell when the shell sets a title, otherwise after the directory it is in. The other terminal tabs are named after what they run, and a preview tab after its file.

## Opening a tab

The window opens with a shell on the host in its first terminal tab. The new-tab button (the + after the last tab) or ⌘T opens another shell tab, in the group with the keyboard focus. The new shell starts in the directory of the shell with the keyboard focus, when that shell says where it is (the shell integration, see [Inside a terminal](console-terminals.md)), and in the home directory otherwise.

A new tmux or screen session, and an interactive job, are opened from their panels, see [Sessions, jobs and containers](console-sessions.md).

## Switching tabs

A click on a tab shows it. From the keyboard:

| Key | Does |
|---|---|
| ⌘1 … ⌘8 | Shows that tab of the group. |
| ⌘9 | Shows the group's last tab. |
| ⌘⇧], ⌃Tab | Shows the next tab of the group. |
| ⌘⇧[, ⌃⇧Tab | Shows the previous tab of the group. |

## Temporary and kept tabs

A single click on a file row or a session row opens it in a temporary tab: a tab for a look, its name in italics, which the next single click takes over. Each group has one temporary preview tab and one temporary terminal tab. When the next session clicked takes over a temporary terminal tab, the client in it detaches and the session lives on.

A kept tab stays until it is closed. A tab is kept when:

- its row is opened with a double click (Return in the file tree, *Preview* in a file's menu);
- the temporary tab itself is double-clicked;
- *Keep this tab* is chosen in the tab's context menu.

A file or a session that is already open in a tab is brought up instead of opened again.

## Closing a tab

| Way | Closes |
|---|---|
| The × on a tab | That tab. |
| ⌘W | The tab with the keyboard focus. With no tab open, the window. |
| ⌘⇧W | The window, at any time. |
| *Close* in the tab's context menu | That tab. |
| *Close the others in this group* | Every other tab of the group. |

Closing a terminal tab hangs up on it. A tmux or screen client detaches; the session itself, and anything in it, lives on, as it does when the window closes or the connection drops. Closing a preview tab lets its file go.

When a command is still running in the tab, Shellder asks first: *Close this tab?*, with the command's name. The shell integration is what tells that a command is running; the question can be switched off in the Console settings window (*Ask before closing a tab with a command running*, see [The Console settings](console-settings.md)).

## Reopening a tab

⌘⇧T or ⌘Z brings the terminal tab closed last back, its command run again: a shell in the same directory, a session attached again. It comes back in the group it was in when that group is still there, and in the group with the keyboard focus otherwise. Pressed again, it brings back the one closed before that. Preview tabs and the tabs of one-off actions are not brought back this way.

## The ended tab

A terminal tab whose command ended cleanly (an `exit`, a detach) closes itself. One that ended any other way stays as an ended tab: its name is in italics, its last output stays on screen with a line that says it is closed and with which exit status, and Return in it connects again.

## Tab marks, the host label and tab progress

A **tab mark** is a dot on a tab that is not the one shown. It says what happened there meanwhile, and goes when the tab is selected.

| Dot | Meaning |
|---|---|
| Orange | The terminal bell rang there, or its ready bell did. |
| Green | A command finished there. |
| Red | A command failed there. |
| Grey | There is new output. |

The green and the red dot need the shell integration, which tells when a command ends and how. The bells are described in [Inside a terminal](console-terminals.md).

The **host label** is the host's name at the start of a tab that runs on another host than the window shows (*Local* for this machine). It appears after the window was switched to another host with the host menu or the host wheel: the tabs already open stay as they are, each on the host it runs on (see [The Console](console.md)).

**Tab progress** is a thin bar along a tab's foot, drawn while the program in it reports its progress (see [Inside a terminal](console-terminals.md)).

## The tab context menu

| Item | What it does |
|---|---|
| *Split right* | A second group to the right, showing what this tab shows. |
| *Split down* | A second group below, showing what this tab shows. |
| *Keep this tab* | Makes a temporary tab a kept tab. Only on a temporary tab. |
| *Close* | Closes the tab. |
| *Close the others in this group* | Closes every other tab of the group. Only when the group has more than one. |

## Groups

### Splitting

A split puts a second group beside a group or under it. Three ways to make one:

- the split button, the last of the group buttons: its menu offers *Split right* and *Split down*;
- *Split right* and *Split down* in a tab's context menu;
- ⌘D (right) and ⌘⇧D (down), for the group with the keyboard focus.

The new group shows what the tab shows. A terminal's command is run once more: another shell in the same directory, the session attached a second time. A preview's file is shown again. The tab of a one-off action gives a plain shell. The new group starts with the background choices of the group it was split off.

### Dragging a tab

A tab can be dragged to another place. While it is dragged, the drop zone under the pointer is shown:

| Drop zone | What the drop does |
|---|---|
| A tab bar | The tab moves there, between the tabs where it is dropped. Within its own tab bar, this reorders the tabs. |
| An edge of a group (left, right, top or bottom) | The group is split there, and the tab goes into the new group. |
| The middle of a group | The tab moves into that group, after its last tab. |

A group whose last tab is dragged away goes.

### Dividers

The divider between two groups drags. From the keyboard, ⌘⌃ and an arrow move the divider of the group with the keyboard focus that way, ten pixels at a time, and ⌘⌃= gives every split's groups the same size.

### Moving the keyboard focus

A click in a group gives it the keyboard focus, and new tabs open in the group that has it.

| Key | Moves the keyboard focus to |
|---|---|
| ⌘] | The next group (left to right, top to bottom). |
| ⌘[ | The previous group. |
| ⌘⌥ and an arrow | The group that way. |

⌘⌥← from the leftmost group goes on into the side bar, see [The Console](console.md).

### Solo

⌘⇧↩ shows the group with the keyboard focus solo: alone, as large as the tab area. The others come back with:

- ⌘⇧↩ again;
- the button that took the split button's place in the tab bar;
- the keyboard focus moving to another group;
- any change of the layout.

### Closing a group

A group whose last tab closes goes, and its room goes to its neighbours. ⌘⌥W closes the group with the keyboard focus, every tab of it; a tab with a command still running asks as it does when closed alone.

The last group stays, empty: it says that no tab is open, and that + opens a shell. With no tab open, ⌘W closes the window.

## The group buttons

The buttons at the right of a tab bar, which stay in sight however many tabs there are:

| Button | What it does |
|---|---|
| The group's ready bell (a bell) | Switches the ready bell on or off for every tab of the group, new ones included. See [Inside a terminal](console-terminals.md). |
| The background button (a sun or a moon) | Gives the group's terminals a dark or a light background, whatever the system's look. With a preview tab shown, it does the same for the files the group's previews show, a choice of their own: a dark shell can sit beside a light file. |
| The split button | Offers *Split right* and *Split down*. While the group is shown solo, a button that brings the other groups back takes its place. |

The background choices are kept with the layout. The colours themselves are the palettes chosen in [the Console settings](console-settings.md).

## The layout comes back with the window

The layout is kept on this machine, under the host the window shows, and saved with every change. A Console window opened again comes back as it was closed:

- the groups, their splits and the sizes of the splits;
- each group's tabs, the tab it showed, and whether a tab was temporary;
- each group's background choices and ready bells;
- the group that had the keyboard focus.

The terminal tabs connect again, each on its host, and run their commands: a session made in the Console is attached again, or made again when it is gone. The preview tabs show their files.

Two things do not come back:

- **Of a group's plain shells only one comes back**: the one that was shown (the first, when the tab shown was not a shell), in its directory. The groups are laid out as they were, with a shell each, rather than a login for every shell whose screen is gone anyway.
- **Tabs of one-off actions**, such as an interactive job, are not kept, and a group that held nothing else does not come back.

A window switched to another host keeps its tabs, and its layout is saved under that host from then on. Immersive mode and the quick terminal's tabs are not kept, see [Features of the Console window](console-window.md).
