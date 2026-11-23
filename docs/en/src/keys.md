# Keys

The Console's keys are the ones terminals with tabs and splits usually have; the window's own are ⌘R and ⌘⇧S. Every key can be changed on the *Keys* page of the Console settings window, which the settings button (the gear at the top bar's right) or ⌘, opens: see [Console settings](console-settings.md). The keys are kept on this machine, for every Console window.

The tables below are the defaults on macOS. On Linux the defaults are the Ctrl+Shift ones (Ctrl+Shift+T for a new shell, and so on); the *Keys* page shows the ones in force.

## Tabs

| Key | Does |
| --- | --- |
| ⌘T | a new shell on the host, in the group with the keyboard focus |
| ⌘W | closes the tab with the keyboard focus; with no tab open, the window |
| ⌘⇧T, ⌘Z | the terminal tab closed last comes back, its command run again |
| ⌘1 … ⌘8, ⌘9 | that tab of the group; ⌘9 its last |
| ⌘⇧], ⌘⇧[, ⌃Tab, ⌃⇧Tab | the next, the previous tab of the group |

## Groups

| Key | Does |
| --- | --- |
| ⌘D, ⌘⇧D | a split: a second group to the right, below, showing what the tab shows |
| ⌘⌥W | closes the group, every tab of it |
| ⌘], ⌘[ | the keyboard focus to the next, the previous group (left to right, top to bottom) |
| ⌘⌥↑ ↓ ← → | the keyboard focus to the group that way |
| ⌘⌃↑ ↓ ← → | the group's divider moved that way, ten pixels at a time |
| ⌘⌃= | every split's groups the same size |
| ⌘⇧↩ | solo: the group alone, as large as the tab area, and back |

Tabs, groups and splits are described in [Tabs and groups](console-tabs.md).

## Window

| Key | Does |
| --- | --- |
| ⌘N | another Console window on the host |
| ⌘⇧W | closes the window |
| ⌘⇧ held, ⌘⇧↑, ⌘⇧↓ | the host wheel: this machine and the hosts round a centre over the tab area. Held a moment, the modifiers alone draw it; with an arrow, at once. An arrow moves the choice to the next host along the circle the way it points (at the top → is clockwise, on the right side ↓ is; at the far side that way it stays), and the pointer chooses too. Letting ⌘ go switches the window to the chosen host, as the host menu does, and gives the keyboard focus to a shell on it, a new one when the window has none there. Esc puts the wheel away; any other key closes it and runs |
| ⌘↩, ⌃⌘F | the window full screen, and back (with a password offer on show in the tab, ⌘↩ types the password instead) |
| ⌘⇧I | immersive mode: the terminals alone, without the top bar, the side bar, the tab bars or (on macOS) the title bar; again brings them back. The terminals run to the window's edges there, without the padding from the Console settings, and the window's corners are drawn small (on macOS), so the cells sit well beside them. Not kept: a Console opens with its bars. ⌘⌥← from the leftmost group, ⌘⇧L or ⌘⇧S bring the side bar out for a side bar visit, and it goes back with the keyboard focus (⌘⌥→, Esc, ⌘⇧L again, a tab opened from it) |
| ⌃⌥←, ⌃⌥→ | the window in the left or right half of the screen, keeping its height (a top half becomes the top left quarter). How a window in immersive mode, with nothing to drag it by, is moved; the keys lift the window's minimum size, so a quarter fits a small screen |
| ⌃⌥↑, ⌃⌥↓ | the window a step up or down: from the bottom half to the full height, then to the top half, keeping its width (so a quarter goes back to its half, then to the other quarter) |
| ⌃⌥↩ | the window over the whole screen and, pressed there again, back where it was |
| ⌃⌥⇧←, ⌃⌥⇧→ | the window to the screen before or after this one (the screens in order from left to right, side by side from top to bottom, round at the ends), where it is now: the same half or quarter, or its size and its distance from the corner, held within the screen |
| ⌘R | the Status panel read again now |
| ⌘⇧S | the side bar folded away, and back |
| ⌘⇧L, ⌘⌥← from the leftmost group | a side bar visit: the keyboard focus to the side bar, and back (⌘⇧L again, ⌘⌥→, Esc). A side bar folded away is unfolded for the visit and folds again when the keyboard focus leaves, which it does by itself once a row opened a tab. Inside, Tab and ⇧Tab go from panel to panel, the file tree has its arrows and ↩, the lists of sessions, jobs, containers and ports are walked with ↑ ↓, ↩ does what a click does and ⇧↩ what a double click does |
| ⌘, | the Console settings window (the settings button) |
| ⌘Q | quits |

The host wheel, immersive mode and the keys that move the window are described in [The host wheel, immersive mode and the quick terminal](console-window.md).

## Terminal

| Key | Does |
| --- | --- |
| ⌘C, ⌘V, ⌘A, ⌘K | copies a selection (the interrupt without one), pastes, selects all, clears the terminal |
| ⌘F | the find strip: finds in the terminal, or in the file a preview tab shows (a text, a Markdown page, a PDF). ↩ and ⇧↩ go to the next and the previous match, Esc back to the terminal or the file |
| ⌘↑, ⌘↓ | the terminal scrolled to the previous, the next prompt mark (the shell integration sets them) |
| ⌘⇧A | selects the last command's output (the shell integration tells where it is) |
| ⌘+, ⌘−, ⌘0, ⌘ and the wheel | the letters bigger, smaller, back to 13 points |
| ⌘Home, ⌘End, ⌘PageUp, ⌘PageDown | the terminal scrolled to its top, its bottom, a page up, a page down (fn with ⌘ and an arrow on a laptop) |
| ⌘←, ⌘→, ⌘⌫ | ^A, ^E, ^U to the shell: the line's start, its end, the line gone |
| ⌥←, ⌥→ | ESC b, ESC f to the shell: a word back, forward |
| ⇧↩ | ESC ↩ to the program, as ⌥↩: a new line without sending the line (an agent's prompt, zsh's line editor), tmux passing it on |

What these do in a terminal is described in [The terminals](console-terminals.md).

## Changing a key

On the *Keys* page each key is a key chip:

- a click on a key chip, then the keys to use instead, changes it; Esc stops a change;
- the × on a key chip takes the key off;
- the + on a row gives the action another key;
- the arrow at a row's end puts its default back, and *Reset all* every row's.

A key two actions share is marked, and the action higher in the list gets it.
