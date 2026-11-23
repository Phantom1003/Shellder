# The Ports panel

The Ports panel is where a port of the host is brought within reach of this machine. It has three sections: the forwards you set up, the direct connections, and the host's own ports. A click on a row opens the port here, in the VNC viewer or the browser.

{{#include figures/console-ports.html}}

## Forwards

A forward goes either way:

| Direction | What it does |
|---|---|
| `-L` | A port here reaches a port on the host. |
| `-R` | A port on the host reaches a port here. |

The + on the *Forwards* section title adds one. Both ports are yours to pick or left to Shellder: it picks a free one here, and sshd allocates one on the host. A `-L` forward needs the port on the host, a `-R` forward the port here.

The forward is made on the master (`ssh -O forward`), so the host has to be connected with a socket (see [Without a socket](without-a-socket.md)). It lives as long as the master.

### Forward rows

Each forward has a forward row, which says whether the forward is up.

| Control | What it does |
|---|---|
| Click on a `-L` forward | Opens its near end here, making the forward first when it is not up. |
| Click on a `-R` forward | Makes it when it is not up. There is nothing to open here. |
| Start button | Makes the forward; this is how it is started again after a reconnect. |
| Stop button | Stops the forward (`ssh -O cancel`). The row stays. |
| × | Forgets the forward. |

The near end of a `-L` forward opens in the VNC viewer (Screen Sharing on macOS) when a VNC server is at the far end, and in the browser otherwise.

## Direct connections

A direct connection is a port of the host opened at the host's own address (the `HostName` of its ssh config), with no forward. It is for a host that is reachable that way, and needs no master.

The + on the *Direct connections* section title adds one and opens it. From then on it is a click away: a click on its row opens the port, in the VNC viewer for a port from 5900 to 5999 and in the browser otherwise. × forgets it.

## The host's own ports

Under *On the host* are the host ports: ports the host itself listens on.

To begin with, the section lists the VNC servers:

- Xvnc, Xtigervnc, x11vnc and the like;
- on a Mac, its Screen Sharing or Remote Management. Their port 5900 is held by launchd as root, so only `launchctl` can tell that it is on.

The + on the section title adds another port:

- one of the listeners this account sees (`ss`, or `lsof` on macOS), with its program where the host names it. `ss` names the program only for the user's own listeners, and `lsof` lists only those at all;
- or any port by hand (*Another port…*), for what the account cannot see.

A port from 5900 to 5999 that nothing names otherwise counts as VNC.

| Control | What it does |
|---|---|
| Click on the row | Opens the port here through a `-L` forward to it. The forward is made on the spot when there is none, and kept with the others under *Forwards*. |
| Globe | Opens the port at the host's own address instead (the `HostName` of its ssh config), with no forward, for a host reachable that way. |
| × | Takes the port off the list. |

In the [Console of this machine](console.md) the panel has one section, *On this machine*: its listening ports, opened at `127.0.0.1` with no forward.

## Where the rows are kept

What you set up is kept on this machine, per host.

The forwards and the direct connections are also listed in the Tools card of the [main window](main-window.md), under the Console and Files buttons: the same rows, so that a port is a click away without the Console. The + on the Tools card's title adds a forward (*New forward…*) or a direct connection (*Direct connection…*).
