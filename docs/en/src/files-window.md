# The Files window

The Files window is a window per host with two panes side by side. Each pane shows the file tree of one machine: this host, this machine, or another host. A drag from one pane to the other copies. The file tree, the copies strip and the copies list are the ones the Console's Files panel has; they are described in full in [The Files panel and previews](console-files.md), and this page says what the window adds and what differs.

{{#include figures/files-window.html}}

## Opening the window

- the *Files* button on the Tools card of the host page ([The main window](main-window.md));
- *Files* in the host's submenu of the menu bar menu;
- the last button of the file toolbar in the Console's Files panel, or *Open in the Files window* in its context menu, which also send the window's pane on the host to the folder the panel shows ([The Files panel and previews](console-files.md)).

A window opened for the first time shows the host in the left pane and this machine in the right one. The host's pane opens at the home folder of the account, or at `~/Shellder` when the host has such a folder; this machine's at your home folder.

## The two panes

A pane has, from top to bottom: the host picker and the file toolbar on one line, the column headings, the file tree, and the path bar. All but the host picker work as in the Files panel: sorting, the disclosure arrows, selecting, the folder picker, new folders and files, renaming and deleting are described there. A pane is wider than the side bar, so its dates carry their time.

Each pane keeps its own folder, its own open folders, its own selection and its own order. They are not the Files panel's either: nothing done in the window moves the panel's tree, and nothing done there moves the window's.

## The host picker

The host picker is the button at a pane's top left, labelled with the machine the pane shows. Its menu lists *Local* (this machine, which counts as a host here) and then every host of the ssh config. Picking one points the pane at that machine, at its home folder.

A host with a master up is reached through the master. Any other host is reached on a login of its own, see [Without a master](#without-a-master).

Both panes can show any machine: the host and this machine, two hosts, or two folders of the same machine. Nothing is copied between two panes that show the same machine.

## Copying

There are three ways to copy from one pane to the other:

| Way | Where the copy lands |
|---|---|
| Drag file rows from one pane onto the other | In the folder under the pointer, in the folder of the file under it, or, dropped on the blank, in the folder the pane shows. |
| The copy-across button, the arrow at the end of the file toolbar that points at the other pane | In the folder the other pane shows. It copies what is selected. |
| *Copy to …* in the context menu of a row | The same as the copy-across button. |

The copy-across button and *Copy to …* are greyed while nothing is selected or both panes show the same machine. A folder is copied with everything in it.

A copy between this machine and a host is an sftp; a copy between two hosts is an scp.

Files dragged in from the desktop onto a pane that shows a host are copied too, into the folder under the pointer. So are file rows dragged in from another window: another Files window, or the Files panel of a Console.

## What differs from the Files panel

- A click on a file only selects it: the window has no preview. A double click on a file of this machine opens it in its application, and its context menu has *Open* for the same.
- The context menus have *Copy to …* (the other pane) instead of *Upload here…* and *Download…*, and no *Shell here*, *Preview* or *Open in the Files window*.
- The file toolbar starts with the host picker and ends with the copy-across button, where the Files panel has the button for this window.
- On a pane that shows this machine, *Delete* is *Move to Trash*.

## Copies, the name question and continuing

The copies strip runs under both panes, and the copies list opens from it. They are the host's, shared with the Files panel of the host's Console: a copy started in either place is listed in both, and the name question (*Replace*, *Skip*, *Apply to all*) is asked in both and answered once. A copy that stopped can be continued from what it wrote (▷). The copies go on when the window closes.

All of this is described in [The Files panel and previews](console-files.md): the copies strip, the copies list, the name question and continuing a copy.

## Deleting on this machine

On a host, *Delete* removes the entry for good, and the question says that it cannot be undone. On this machine nothing is removed: the button and the menu item read *Move to Trash*, the question asks *Move “name” to the Trash?*, and the entry goes to the system's Trash, from where it can be put back.

## Without a master

A Files window does not need the host's master. On a host without a master up, or whose master has no socket (see [Without a socket](without-a-socket.md)), the window logs in on its own:

- **Listings go through a session**: one `ssh host sh`, logged in when the window first asks for a listing and kept open. It runs the listings, creates and deletes. There is one login per host, however many listings, and the session is closed when Shellder quits.
- **Copies log in on their own**: they are the same sftp (scp between two hosts) as through a master, only each copy logs in for itself, so a copy costs a login. On a host with a one-time code, each copy takes the next code: a code just used is not sent again.
- **A copy that stopped** is continued from what landed, as through a master.

The questions of these logins (a password, a code, a host key) are answered the way a master's are: from the stored credentials, or in [the prompt window](prompt-window.md).

The Console's Files panel and its preview tabs read a host the same way: through the master when one is up, through the session otherwise.
