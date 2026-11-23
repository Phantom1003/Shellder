# Console settings

The terminals have a settings window of their own, the Console settings window, apart from the app's [Settings window](settings.md). The settings button (the gear at the top bar's right) opens it, and so does ⌘, in a Console. Its settings are the app's: they are the same in every Console window, and a change reaches the terminals open now. The page bar at the window's top picks one of five pages: *Terminal*, *Manners*, *Quick terminal*, *Theme* and *Keys*. The window opens on the page shown last.

{{#include figures/console-settings.html}}

## Terminal

How the terminals look.

| Row | What it sets |
| --- | --- |
| *Font* | The family of the terminals' letters, picked from the fonts installed on this machine. The system's monospace font (SF Mono on a Mac) is at the top as the default, then the monospaced families, then the rest. The size is not here: it is ⌘+ and ⌘− in the Console |
| *Line height* | The height of a line, from 1 to 2 |
| *Cursor* | The cursor's shape: *Block*, *Bar* or *Underline* |
| *Blinking cursor* | Whether the cursor blinks |
| *Scrollback* | How many lines a terminal keeps |
| *Colours, light* | The palette of the light terminals |
| *Colours, dark* | The palette of the dark terminals |
| *Opacity* | How see-through the window is, a slider |
| *Background blur* | How much what is behind the window is blurred (macOS only) |
| *Padding* | The room round the terminal, at its sides, in pixels |

**Palettes.** Each of the two palette rows offers the window's own two colour sets (*Default*), the built-in palettes (Solarized, One, GitHub, Catppuccin, Gruvbox, Tokyo Night, Nord, Dracula) and your theme files. A group's [background button](console-tabs.md) picks between the two palettes chosen here. The *Theme* page chooses the same two palettes with a view of their colours.

**Opacity.** The Console windows follow the slider as it moves: they are transparent windows, so the opacity reaches them all at once. The top bar, the side bar and the terminals take it alike, as Ghostty's `background-opacity` does.

**Background blur.** Off and five steps, a tick at each, sharp at the left end and blurred at the right. It is the window server's own blur of the backdrop, as Ghostty and iTerm2 have it, and it applies to a window that is see-through. It is off unless asked for, since a blur is redrawn with every frame and can make typing feel slow.

**Padding.** Above and below, the grid has only what an integer number of rows leaves over, split between the two. What the columns leave over is split likewise, on top of the padding. A window in [immersive mode](console-window.md) has no padding at all.

## Manners

How the terminals behave.

| Row | What it sets | Default |
| --- | --- | --- |
| *Copy on select* | A selection is copied as it is made | off |
| *Ask before pasting several lines* | The paste question | on |
| *Ask before closing a tab with a command running* | The question before a tab, or the window, is closed with a command still running (needs the shell integration) | on |
| *Bell* | What the terminal bell does in a terminal on screen: *Flash*, *Sound*, *Flash and sound* or *Neither* | *Flash* |
| *Sound* (under *Bell*) | Which sound the terminal bell plays: the system's alert, another of the system's sounds, or a file of your own; ▶ plays it. Shown when the bell makes a sound | the system's alert |
| *Notifications* | Whether what happens in a tab nobody is watching is told as a notification | on |
| *Sound* (under *Notifications*) | The sound a notification comes with, picked the same way | *None* |
| *Shell integration* | Whether a shell started from the Console gets the shell integration | on |
| *Draw with WebGL* | The terminals drawn with WebGL instead of the DOM renderer | off |

The paste question, the terminal bell, the notifications and the shell integration are described in [The terminals](console-terminals.md).

**WebGL.** The DOM renderer lets WebKit draw the letters itself, which looks better than WebGL's glyph atlas on a Mac. WebGL is faster with a screenful of output, so try it when a `cat` or a `htop` feels slow.

## Quick terminal

The key and the host of the [quick terminal](console-window.md).

| Row | What it sets |
| --- | --- |
| *Key* | The key that drops the quick terminal from the top of the screen, in every application. Click the field, then press the keys, a modifier among them. ⌫, or the × beside the field, takes the key off |
| *Host* | The host the quick terminal's shell is on, or this machine (*Local*) |

A key the app could not have (the system or another application holds it) is said under the *Key* row, with the reason.

## Theme

The palettes side by side: the list on the left, the chosen palette on the right.

### The list

- A switch at the top, *Light* or *Dark*, says which side is being chosen: the palette of the light terminals or of the dark ones.
- Under it the palettes: the window's own colours (*Default*), the built-in palettes and your theme files, each with a swatch. The palette that side has is marked.
- A click chooses the palette clicked for that side, and the terminals open follow.
- A right click on a palette offers *Use for light terminals*, *Use for dark terminals*, *Duplicate…* and, for a theme file, *Rename…* and *Delete the theme file*.
- *Open the themes folder*, under the list, opens the folder of the theme files.

### The chosen palette

- A sample terminal drawn in the palette's colours.
- Every colour the palette sets: the background, the foreground, the cursor, the text under the cursor, the selection and the selected text, then the sixteen colours a program names (palette 0 to 15).
- Each colour shows its value, or what it falls back on when the palette leaves it out: the cursor on the foreground, the selection on the accent, an unset colour on xterm's own.
- Each colour has a colour well to change it.

What a colour well does depends on the palette:

| Palette | A change in a colour well |
| --- | --- |
| a theme file | is taken on the spot: the line is replaced in the file and the rest of the file kept. The × beside a colour of the file's own takes the line out again |
| a built-in palette, or *Default* | is not made. *Duplicate* makes a duplicate as a theme file, named as you say and chosen for the side, and the colour wells work on that |

The pencil and the bin beside *Duplicate* rename and delete a theme file. A built-in palette has neither.

### Theme files

The theme files live in a folder, `themes` in the state directory, one file per palette, in Ghostty's format:

```
# a comment
background = 1d1f21
foreground = c5c8c6
palette = 0=#1d1f21
```

- `palette` lines go from 0 up to 15.
- `cursor-color`, `cursor-text`, `selection-background` and `selection-foreground` are read when the file has them.
- A line starting with `#` is a comment.

Ghostty's own themes and the Ghostty exports of iTerm2-Color-Schemes go in as they are, named after their file. A file put there or changed in an editor shows up in the list when the window comes back to the front, and in the terminals when it is chosen.

## Keys

The Console's keys, in sections, each key a key chip. The defaults are listed in [Keys](keys.md).

| To | Do |
| --- | --- |
| change a key | click its key chip, then press the keys to use instead |
| stop a change | press Esc |
| take a key off | click the × on its key chip |
| give an action another key | click the + on its row |
| put a row's default back | click the arrow at the row's end |
| put every default back | click *Reset all* |

A key two actions share is marked, and the action higher in the list gets it. The keys are kept on this machine, for every Console window.
