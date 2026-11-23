# The switch and the lock

Every host has a Connect switch and a lock. The switch means *connect*: one attempt, and a connection that lasts as long as it lasts. The lock means *stay connected*: Shellder brings the connection back whenever it drops. The Connect switch is on the host row in the [main window](main-window.md), the Lock button on the host page's connection card.

## The switch

Switching a host on is one connection attempt.

- **If the attempt succeeds**, the master stays up and the switch turns green. The connection lasts as long as it lasts: when the link drops, the switch goes off again.
- **If the attempt fails** (a wrong password, a question cancelled, a host key declined, the host unreachable, a configuration that does not resolve), the switch goes back off and nothing is retried. The status dot turns red, the host page's status line says *Failed (details in log)*, and the reason is in the log panel.
- **Switching it off** closes the master. *Disconnect* does the same. *Close socket* additionally runs `ssh -O exit`, which also works on a master started by an interactive `ssh`.

The switch is not remembered: after a restart every host starts off.

### The colours

The status dot of a host and its Connect switch wear the same colour:

| Colour | Meaning |
|---|---|
| yellow | an attempt or a retry is under way |
| green | the master is up |
| blue | the master is up and the host is locked |
| cyan | the master up is not Shellder's own: one left by an interactive `ssh` with `ControlMaster auto`. The lock does not keep that one, and *Take over* replaces it |
| red | an attempt failed. The dot stays red until the switch goes on again |
| grey | off. The switch itself is plain grey whenever it is off |

## The lock

The lock keeps a connection connected. Locking a host switches it on if it is off, and from then on Shellder brings the connection back by itself:

- a dropped link is reconnected by the reconnect rule;
- a locked host is switched on again when Shellder starts.

Only a link that keeps dying right after connecting gives up after three tries.

The lock goes with the switch. Whatever turns the switch off, whether you or a failed attempt, clears the lock. Unlocking by hand leaves the connection as it is; it only stops being brought back.

Locked hosts are the only thing remembered across launches, and what the `shellder lock`, `unlock` and `status` commands work on.

## The reconnect rule

The reconnect rule says how long Shellder waits before each new try and how long it keeps trying.

- **The default rule** is in the [Settings window](settings.md) and is the one every host follows: waits that double from 5 s up to 10 min, and no giving up.
- **A rule of its own** can be given to a host on its host page: the *Custom reconnect rule* row of the connection card, under the keep-alive mode. It is remembered per host. It is a whole rule, so a later change of the default rule does not reach it.

A rule has these fields:

| Field | Meaning |
|---|---|
| *Wait between tries* | *Doubling*: each wait is twice the one before, up to the longest. *Fixed*: the same wait for every try |
| *First wait* (*Wait* when fixed) | the wait after the first failed try, in seconds (1 s to 1 h) |
| *Longest wait* | where the doubling stops, in seconds (up to a day). Not shown for a fixed wait |
| *Give up* | how long the connection may stay down before the switch goes off (and the lock with it), or *Never* |

How the waits are counted:

- A try that fails right away (the server unreachable, the login refused) counts the waits up. A connection that held for a while starts them over.
- With doubling waits, the third quick failure in a row goes straight to the longest wait, so a wrong password does not hammer the server.
- With a fixed wait it is your wait every time, so keep it a reasonable one on hosts that ban clients for failed logins.
- A change to a rule reaches a host that is waiting for its next try at once: the wait is counted again under the new rule.

## Keep-alive modes

Some servers close a session-less `ssh -N` connection within seconds. The master then dies like any other drop: an unlocked host switches off with the error, a locked one retries and gives up after a few quick failures. If that is what you see, pick another keep-alive mode in the *Keep-alive* row of the host page's connection card. The choice is remembered per host.

| Mode | What the server sees | Notes |
|---|---|---|
| *no session (-N)*, the default | a connection with no session | cleanest, but some servers reject it |
| *idle shell (pty)* | `sshd: you@pts/N → -bash`, an idle login in `w` | indistinguishable from an open terminal; SIGHUP on disconnect |
| *cat on stdin* | `sshd: you@notty → cat` | no pty and no login scripts; exits on EOF when the link drops |
