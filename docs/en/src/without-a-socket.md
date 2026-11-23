# Without a socket

A master normally owns a socket (its `ControlPath`), which every other ssh command reuses. A host can be without one for two reasons: its config has no `ControlPath`, or the platform's ssh has no ControlMaster at all. Such a host is still switched on, locked, reconnected and shown like any other; what changes is what can go through its connection.

## What the master still does

Shellder runs the same ssh without `-M`. That keeps the connection alive, and with it the port forwards the host's config asks for (`LocalForward`, `RemoteForward`, `DynamicForward`).

There is no socket to check, so Shellder takes "Authenticated to host" from ssh's own output as the sign that the connection is up. The status dot and the status line say *Connected (no socket)*.

## What it cannot do

Such a master cannot serve other ssh commands, Console windows or Files windows: those log in on their own, with Shellder answering the questions. Forwards added in the Tools card or the Console's Ports panel are made on the master's socket, so they need a host connected with one.

The host page says which case applies in a [notice card](main-window.md#notice-cards). For a host without a `ControlPath` the card shows the lines to add to your ssh config to give it a socket.

## The Files window

A Files window on a host without a master up lists through a *session*: one `ssh host sh`, logged in when the window first asks for a listing and kept open. It runs the listings and creates and deletes. There is one login per host, however many listings, and the session is closed when Shellder quits.

The copies are the same sftp (scp between two hosts) as through a master, only each one logs in on its own, its questions answered the same way. So a copy costs a login; on a host with a one-time code it costs the next code, since a code just used is not sent again. A copy that stopped is continued from what landed, as through a master.

## The command line

`shellder status` can only ask a master with a socket whether it is up. One without is marked `?`: it is kept alive by the app alone.

`SHELLDER_NO_CONTROLMASTER=1` shows the app the way a platform without ControlMaster sees it, on one that has it.
