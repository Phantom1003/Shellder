# The host wheel, immersive mode and the quick terminal

Three features of the Console window are about the window as a whole and not about one tab. The host wheel switches the window to another host from the keyboard. Immersive mode leaves the terminals alone in the window, which is then moved with keys of its own. The quick terminal is a shell that a key drops from the top of the screen in any application. This page also covers full screen and a second Console window. The keys named here are the macOS defaults; all of them can be changed, see [Keys](keys.md).

{{#include figures/host-wheel.html}}

## The host wheel

The host wheel is a ring of hosts drawn over the tab area: this machine (*Local*) at the top, then the hosts of the host list round a centre, each with its colour from the list, the way a game offers its weapons. The centre names the chosen host and its state. When the wheel opens, the chosen host is the one the window shows.

### Opening it

- Hold ⌘⇧ for a moment. The modifiers alone draw the wheel; a quick ⌘⇧T does not flash it.
- ⌘⇧↑ or ⌘⇧↓ draws it at once, with the chosen host moved that way.

The wheel does not open while a dialog is up in the window.

### Choosing a host

- An arrow moves the choice to the next host along the circle, the way the arrow points on screen. At the top → is clockwise and ← goes back; on the right side ↓ is clockwise. At the far side that way the choice stays where it is, so the circle is traced with →, ↓, ←, ↑ in turn.
- The pointer over a host chooses it too.

### Letting go

Letting ⌘ go switches the window to the chosen host, as the [host menu](console.md) does, and gives the keyboard focus to a shell there:

- one of the window's own shells on that host (the one shown first), or
- a new shell in the group with the keyboard focus, when the window has none there.

So going back and forth opens no more than one shell per host. ↩, or a click on a host, switches the same way. Choosing the host the window already shows changes nothing.

### Putting it away

- Esc, or a click outside the wheel, puts it away with nothing changed.
- Any other key pressed with the modifiers still held (⌘⇧T, say) closes the wheel and does what it always does.

| Key | Does |
| --- | --- |
| ⌘⇧ held | draws the host wheel after a moment |
| ⌘⇧↑, ⌘⇧↓ | draws it at once, the chosen host moved that way |
| ← → ↑ ↓ | the choice to the next host along the circle that way |
| letting ⌘ go, ↩ | switches the window to the chosen host |
| Esc | puts the wheel away |

## Immersive mode

⌘⇧I takes the window into immersive mode: the terminals alone, without the top bar, the side bar, the tab bars or, on macOS, the title bar. ⌘⇧I again brings them back.

- The terminals run to the window's edges, without the padding set in the [Console settings](console-settings.md).
- On macOS the window's corners are drawn small, so the cells sit well beside them.
- Immersive mode is not kept: a Console opens with its bars, whatever it was closed in.

### The side bar visit

The side bar is still there for a side bar visit. ⌘⌥← from the leftmost group, ⌘⇧L or ⌘⇧S bring it out, with the keyboard focus in it. It goes back when the keyboard focus leaves: ⌘⌥→, Esc, ⌘⇧L again, or a tab opened from it.

### Moving the window with the keys

An immersive window has nothing left to drag it by, so keys move it. They work in any Console window.

| Key | Does |
| --- | --- |
| ⌃⌥←, ⌃⌥→ | the window in the left or right half of the screen, keeping its height (a top half becomes the top left or top right quarter) |
| ⌃⌥↑, ⌃⌥↓ | the window a step up or down: from the bottom half to the full height, then to the top half, keeping its width (so a quarter goes back to its half, then to the other quarter) |
| ⌃⌥↩ | the window over the whole screen and, pressed there again, back where it was |
| ⌃⌥⇧←, ⌃⌥⇧→ | the window to the screen before or after this one, where it is now: the same half or quarter, or its size and its distance from the corner, held within the screen |

The screens are taken in order from left to right, those side by side from top to bottom, and round at the ends. These keys lift the window's minimum size, so a quarter fits a small screen.

## Full screen

⌘↩ or ⌃⌘F takes the window full screen, and back. With a [password offer](console-terminals.md) on show in the tab, ⌘↩ types the password instead.

## Another Console window

⌘N opens another Console window on the host.

## The quick terminal

The quick terminal is a shell that a key of its own drops from the top edge of the screen, in every application, as Ghostty has it. It has no top bar, no side bar and no tab bar.

### The key and the host

Both are on the *Quick terminal* page of the [Console settings](console-settings.md).

- **The key.** Click the field and press the keys, a modifier among them. ⌫ takes the key off. A key the system or another application holds cannot be had; the page says so.
- **The host.** The quick terminal's shell is on the host chosen there, or on this machine.

### Where it appears and how big

- It drops from the top edge of the screen the pointer is on, over every other window, a full-screen one included.
- It is as wide as the screen and a third of its height until you drag an edge. After that its size is kept, as shares of the screen.
- It stays centred along the top: both sides move at once when one is dragged.

### Its background

The quick terminal's background is the one last chosen with a Console group's background button (the system's look until one is). A Console's terminal has its ready bell in its top right corner; the quick terminal has no ready bell and has a background button there instead. That button, or the terminal menu, switches between the dark and the light background.

### Putting it away

Any of these takes it away:

- the key again;
- Esc at its prompt (the [shell integration](console-terminals.md) says when no command is running; in a program Esc is the program's);
- the keyboard focus going to another window.

The shell lives on, hidden, for the next time. Closing its last tab, or `exit`, puts a fresh shell in it.

Called up over another application, the quick terminal gives that application the keyboard focus back as it goes, so no window of Shellder's comes forward.
