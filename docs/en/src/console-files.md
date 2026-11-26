# The Files panel and previews

The Files panel is the panel of the Console's side bar that shows the host's files: a file toolbar, the file tree with its column headings, a path bar along the foot, and under it the copies strip. A click on a file shows it in a preview tab in the tab area, beside the terminals. The same file tree is drawn in each pane of [the Files window](files-window.md); this page describes it in full, and that page only says what differs.

{{#include figures/console-files.html}}

## What the panel shows

The panel shows one folder of the host, the *folder shown*, and everything opened under it. It opens at the home folder of the account, or at `~/Shellder` when the host has such a folder.

Other parts of the Console can send the panel to a folder:

- a double click on a disk in [the Status panel](console-status.md);
- the folder button of a session row, a job row or a container row ([Sessions, jobs and containers](console-sessions.md));
- a ⌘-click on a path link in a terminal ([Inside a terminal](console-terminals.md)).

The Console keeps a Files panel for every host the window has shown, so a window switched to another host and back finds the tree as it was. When the window shows this machine, the panel is this machine's own file tree: there is nothing to upload or download, and it has no Files window button, but a drag from it still copies to a host's Console or Files window.

## The file toolbar

The buttons above the file tree, from left to right. Each is an icon; the name given here is its tooltip.

| Button | What it does |
|---|---|
| *Go to a folder on …* | Opens the [folder picker](#the-folder-picker). |
| *New Folder* | Asks for a name and makes a folder (see [New folders and files](#new-folders-and-files)). |
| *New File* | Asks for a name and makes an empty file. |
| *Delete* | Deletes what is selected, after asking. Greyed while nothing is selected. On this machine it reads *Move to Trash*. |
| *Show the entries whose name starts with a dot* | Shows or hides the hidden files. |
| *Back to the directory before this one* | Returns to the folder shown before. Greyed when there is none. |
| *Up one directory* | Shows the folder above the one shown. |
| *Refresh* | Reads the folder shown, and every folder open under it, again. |
| The last button (a link icon) | Opens the Files window on the folder the panel shows (see [Open in the Files window](#open-in-the-files-window)). Not there for this machine. |

## The file tree

### Column headings and sorting

The column headings are *Name*, *Date Modified* and *Size*. A click on one sorts the tree by that column, and a second click turns the order round; the sort arrow (↑ or ↓) beside the heading says which way. Sorted by name, folders come first. Sorted by date or by size, folders sit among the files, so what changed last, or the biggest thing, is at the top whatever it is. The order is kept on this machine.

The side bar is narrow, so the Files panel shows a date without its time; the full date is the tooltip.

### File rows and folders

Every entry of a folder is a file row: its icon, its name, its date and its size.

- The **disclosure arrow** at a folder's left opens and closes the folder in the tree, in place.
- A **double click** on a folder enters it: it becomes the folder shown.
- The button at the right of a folder's row, there while the pointer is on the row (*Shut every folder inside*), closes every folder open inside it.

Every folder shown keeps the shape of its tree while the panel shows another. Entered again, by a double click, *Back* or *Up one directory*, its folders are open as they were left and the tree is scrolled as it was; after *Back* or *Up one directory* the folder just left is selected.

### Selecting and clicking

| Action | What it does |
|---|---|
| Click on a file | Selects it and shows it in the temporary preview tab of the group with the keyboard focus (see [The preview tab](#the-preview-tab)). The keyboard focus stays in the tree. |
| Double click on a file | Shows it in a kept preview tab. |
| Click on a folder | Selects it. Only its disclosure arrow opens it. |
| ⇧-click | Selects every row from the one clicked last to this one. |
| ⌘-click | Adds the row to the selection, or takes it out. |
| Click on one of several selected rows | Leaves that row alone selected. |
| Click on the blank under the rows | Selects nothing. |

With the keyboard focus in the tree: ↑ and ↓ move the selection, → opens the selected folder and ← closes it, ↩ enters a folder or shows a file in a kept preview tab, and ⌘⌫ deletes what is selected, after asking. ⌘⇧L brings the keyboard focus into the side bar and back (see [The Console](console.md)).

## The path bar

The path bar along the foot of the tree is the path of the folder shown, each part a button: the first is the root (`/`, labelled *Local* on this machine), the last the folder shown. A click on a part shows that folder; a click on the last one reads the folder again. A name too long for the bar is cut short; the tooltip has the whole path.

## The folder picker

*Go to a folder on …* in the file toolbar and *Go to folder…* in the blank's context menu open the folder picker:

- At the top is the path, in a field. Edit it and press Return to go there; Esc closes the picker. A path typed into the field shows that path's tree underneath.
- Beside the field, *Up one directory* and *Home* move the picker's tree.
- Under it is the tree of that path's folders. A folder opens by its disclosure arrow, and a click on its name shows it in the panel. A folder with none inside says *No folders inside*.

## Context menus

The context menus have what the file toolbar has, and more. A right click on a row that is not selected selects it first; on a selected row the menu is for everything selected.

### On a file row

| Item | Shown for | What it does |
|---|---|---|
| *Open* | one folder | Enters the folder. |
| *Preview* | one file | Shows the file in a kept preview tab. |
| *Upload here…* | one folder | Copies files picked on this machine into the folder. |
| *Shell here* | one folder | Opens a shell tab whose shell starts in that folder. |
| *Download…* (*Download n items…*) | any selection | Copies what is selected to a folder picked on this machine. |
| *New folder…*, *New file…* | any | Into the folder clicked, or beside the file clicked. |
| *Rename…* | one row | Gives the entry another name where it is. |
| *Delete…* | any selection | Deletes what is selected, after asking. |
| *Refresh* | any | As the toolbar's. |

### On the blank

For the folder shown:

| Item | What it does |
|---|---|
| *Upload here…* | Copies files picked on this machine into the folder shown. |
| *Shell here* | Opens a shell tab whose shell starts in the folder shown. |
| *New folder…*, *New file…* | Into the folder shown. |
| *Go to folder…* | Opens the folder picker. |
| *Back*, *Up one directory* | As the toolbar's. |
| *Show hidden files* / *Hide hidden files* | As the toolbar's eye. |
| *Refresh* | As the toolbar's. |
| *Open in the Files window* | As the toolbar's last button. |

On this machine the menus have no *Upload here…*, *Download…* or *Open in the Files window*.

## New folders and files

*New Folder* and *New File* ask for a name in a dialog, which says where the new entry goes (*It goes into …*): into the selected folder, into the folder around the selected file, or into the folder shown when nothing is selected. From a context menu that is the folder clicked, the folder of the file clicked, or, on the blank, the folder shown.

## Renaming

*Rename…* is there for one row alone. The dialog starts with the present name; a file's name is selected up to its extension, the part a rename mostly keeps.

- A name that is taken is refused: nothing is written over. The copies strip says why.
- A renamed folder stays open as it was, and the row stays selected.
- A preview tab of the file, or of a file in the renamed folder, follows it to the new name.

## Deleting

*Delete* asks first (*Delete “name” on host?*) and says that it cannot be undone. On this machine the item is *Move to Trash* and the entry goes to the Trash (see [The Files window](files-window.md)).

## Uploading and downloading

- *Upload here…* opens the system's file dialog: the files picked on this machine are copied into the folder the item was asked on.
- *Download…* opens the system's folder dialog: what is selected is copied into the folder picked on this machine.

Both name the two ends themselves. A download goes to the folder picked and an upload into the folder it was asked on: no pane of the Files window is pointed anywhere for either. The copies run in the queue of this host and show in the [copies strip](#the-copies-strip).

## Shell here, and rows dragged onto a terminal

*Shell here*, in the menu of a folder and of the blank, opens a shell tab whose shell starts in that folder, in the group with the keyboard focus.

File rows dragged onto a terminal tab are typed into it as their paths, quoted where needed, with a space after each. Only rows of the terminal's own host are typed.

## Files dragged in

Files dragged in from the desktop are copied to the host: into the folder under the pointer, into the folder of the file under it, or, dropped on the blank, into the folder shown. File rows dragged in from another machine's tree (a pane of a Files window, another Console's Files panel) are copied the same way.

## Open in the Files window

The last button of the file toolbar, and *Open in the Files window* in the blank's menu, open [the Files window](files-window.md) of this host on the folder the panel shows: the window's pane that shows this host goes to that folder, and *Back* there returns to where the pane was. A window whose panes both show other machines is left as it is.

## What is the panel's own, and what is shared

The file tree is the panel's own. The folder it shows, the folders open in it, what is selected and how it is ordered are not the Files window's, and nothing done in one moves the other.

What the two share is the host's:

- the copies, listed under both, with their name questions;
- a folder that changed (a new file, a rename, a copy that landed), which is read again wherever it shows.

## The copies strip

The copies strip is the one line under the file tree (its label on screen is *Transfers*). Copies of one host run one at a time; the rest wait in the queue. From left to right the strip holds:

- an arrow that opens and closes the [copies list](#the-copies-list);
- the copy under way: an icon for its direction, its name and where it goes (`name → folder`), a progress bar with its percentage (a bar without one while the size is not known), and a button that stops it (*Stop this copy*). What a stopped copy has written stays, and the next one waiting starts;
- with no copy under way, what happened last: the last copy finished, or a message (an entry deleted, a rename, an error);
- how many copies wait (*+n waiting*), with a × that takes every waiting copy off the queue;
- *Clear*, which takes the copies that are over off the copies list.

A narrow strip drops the parts least missed first: its label, the word beside the count, the name (the strip's tooltip has it), then the percentage.

## The copies list

The copies list opens under the copies strip and holds every copy of this host, the newest first, up to two hundred. Its top edge drags to make it taller or lower. A row shows the copy's state, its name, where it came from and where it went (`machine:path → machine:path`), and one of:

| Status | Meaning |
|---|---|
| *Waiting* | In the queue. The × on the row takes this copy off the queue. |
| a progress bar | Under way. |
| a size and a time | Done. |
| *Cancelled* | Stopped, or taken off the queue. |
| *Skipped, it was already there* | Answered *Skip* at the name question. |
| the error | Failed; the text is what sftp or scp said. |

A copy that was cancelled, skipped or failed has a button to ask for it again: ▷ (*Continue where it stopped*) or ↻ (*Try Again*), see [Continuing a copy](#continuing-a-copy). An empty list says *Nothing has been copied yet.*

The copies belong to the host, not to the window: they go on when the Console or the Files window closes, and end when Shellder quits.

## The name question

A copy writes over what is there, so before each copy starts Shellder looks whether its name is taken where it lands, and asks first when it is: *Replace “name”?*

- *Replace* writes over the entry that is there.
- *Skip* leaves it alone; the copy is listed as skipped.
- *Apply to all* gives the same answer to the rest of the same drop. It is offered when more copies of that drop wait.

The queue waits for the answer. The name question is asked in the Files panel and in the Files window of the host alike; answered in one, it goes away in the other.

## Continuing a copy

A copy that stopped, because it was stopped or because it failed, can be continued from what it wrote itself (▷): it is not asked about the name again, since something being there is the point. These start over instead:

- a copy that was replacing a file;
- an upload of a folder, or of a file that had not begun;
- a copy between two hosts;
- a copy taken off the queue before it started, and one that was skipped. These get their name question again.

## The preview tab

{{#include figures/preview.html}}

A preview tab shows a file of the Files panel in the tab area, the way an editor does. It has a preview header along its top and the file under it. For tabs in general see [Tabs and groups](console-tabs.md).

### Temporary and kept

A single click on a file shows it in the temporary tab of the group with the keyboard focus. Each group has one temporary preview tab, its name in italics, and the next file clicked takes it over. A double click on the file (Return, *Preview* in its menu, or a double click on the tab itself) makes it a kept tab.

A file already open in a tab of that group is brought up instead. A file open in another group gets a tab in this one as well, so a split can show it twice.

### What each kind of file shows

| Kind of file | What the preview shows |
|---|---|
| Text | The text as it is, its lines numbered (the numbers are not part of a selection). It is coloured by its syntax when its name, or the first line of a script without a suffix, says the language, up to 512 KB. Buttons wrap long lines and turn the colours off. |
| Markdown (`.md`, `.markdown`) | The page rendered. Links open in the browser; images from the web show, relative ones as their alt text; code blocks are coloured as their fence says. |
| Web page (`.html`, `.htm`, `.xhtml`) | The page in a sandboxed frame that runs no script and reaches nothing outside itself. Its own styles apply and images from the web load. |
| PDF | The pages one under the other, as wide as the tab. The text is selectable and copied as plain text (a scanned page has none). A copy gives a paragraph's lines as one line, a word cut at a line's end put together; a word with a hyphen of its own ("counter-based") keeps it when the pages copied from have it so inside a line. A list's items, a table's rows and a program's lines stay lines. Links are followed, see below. |
| Image | The picture, fitted to the tab's width. |
| Video, audio | The system's player, in the formats it can decode. |
| Anything else | A file that is not text has no preview, and the tab says so. |

In a PDF, a link into the document scrolls to the place it names, and the arrow on the preview header (⌘←) goes back; a link to the web opens in the browser. The link under the pointer is marked and says where it goes. The preview header names the page at the top of the tab.

### The preview header

The preview header holds the file's path, then its size (with the page for a PDF, the pixels of a picture or a video, the length of a recording), then the buttons that apply to what is shown. Each is an icon; the name given here is its tooltip.

| Button | Shown for | What it does |
|---|---|---|
| *Back to where the link was (⌘←)* | a PDF, after a link into it | Returns to where the link was followed. |
| *Find in the file (⌘F)* | text, Markdown, PDF | Opens the [find strip](#the-find-strip). |
| *Wrap long lines* | text | Wraps lines longer than the tab is wide. |
| *Colour the syntax* | text whose language is known | Turns the syntax colours off and on. The choice is kept. |
| *The file as written, instead of the page it makes* | Markdown, web page | Switches to the source view and back. |
| *As wide as the tab (⌘0)*, *As tall as the tab* | image, video, PDF | Fit, see below. |
| *Two pages side by side* | PDF | The two-page view. |
| *Smaller*, the percentage, *Bigger* | everything but a web page and audio | Zoom, see below. |
| *Read it again* | all | Reads the file again now. |
| *Download to a folder on this machine…* | a file on a host | See [Downloading the file](#downloading-the-file). |

### The source view

The source view shows a Markdown file or a web page as written, instead of as the page it makes: as text, with numbered lines and syntax colours, and everything a text preview has.

### Fit and zoom

A picture, a video or a PDF is fitted to the tab's width (⌘0, the first fit button) or to its height (the second). Anything but a web page is scaled with:

- ⌘+ and ⌘−;
- ⌘ and the wheel;
- a pinch on the trackpad (macOS);
- the *Smaller* and *Bigger* buttons.

A text's letters, and a Markdown page, scale the same way. For the wheel and the pinch, a PDF's pages grow and shrink about the point under the pointer. The percentage on the preview header says the size now; a click on it goes back to the file's own size.

A PDF's pages are one under the other, or, in the two-page view, two by two side by side, each half the tab's width when fitted to it. The choice is one every preview keeps.

### The find strip

⌘F, or the magnifier on the preview header, opens the find strip in a text, a Markdown page or a PDF (all its pages):

- every match is marked, the one reached brighter, and the strip shows the count;
- ↩ goes to the next match and ⇧↩ to the previous one, as the strip's arrows do;
- a capital in the text makes the search mind the case;
- Esc closes the strip.

A web page is searched in its source view. A picture, a video and a recording have no find strip. The find strip of a terminal works the same way, see [Inside a terminal](console-terminals.md).

### Size limits

The bytes come over through the host's master or its session, like a listing (see [The Files window](files-window.md)), up to a limit per kind:

| Kind | Limit | Past the limit |
|---|---|---|
| Text, Markdown, web page | 16 MB | The first 16 MB are shown, and a note says so. |
| PDF, image | 32 MB | Not shown. |
| Video, audio | 64 MB | Not shown. |

### Follow

While the preview tab is on screen, the file is followed: when it changes, it is read again and drawn where the reader was, the old content staying on screen until then. A web page's own scroll is not kept.

A change is read once the file is the same for two looks in a row, a second apart on this machine and two and a half on a host, so a PDF still being written is not read half done.

### Downloading the file

The last button on the preview header of a host's file downloads it to a folder picked on this machine: the whole file, whatever the tab could show, as *Download…* in the tree does. The copy is listed in the copies list of that host's Files panel. The button is not there for a file of this machine, nor in the [quick terminal](console-window.md).
