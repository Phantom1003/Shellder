# Jump hosts

A host whose `ProxyJump` names another `Host` entry of the same config is connected through Shellder's own master for that entry. The hop reuses the jump host's socket instead of opening a throw-away connection of its own, so the jump host is logged in to once, however many hosts go through it.

## Switching on and off

- **Switching the inner host on** switches the jump host on as well. The jump host is brought up first, then the inner host is connected through it. While it waits, the inner host's status line says *Waiting for jump host …*.
- **Switching the inner host off** leaves the jump host on: it is a connected host like any other now.
- **Switching the jump host off** takes the hosts behind it down with it. *Disconnect* and *Close socket* on the jump host do the same.
- **A jump host that fails to connect** fails the hosts waiting for it with the same reason.

## The lock

The lock follows the same rules:

- locking the inner host locks its jump hosts;
- unlocking a jump host unlocks the hosts behind it.

## Where it shows

- The jump host's status line on its host page says which hosts go through it (*jump host for …*).
- The inner host's connection card shows *via …* after its address, and its Configuration card marks the `ProxyJump` as a host from the list.

## Chains and other hops

Chains, a `ProxyJump` through a host that itself has one, work the same way.

A hop that is not a `Host` entry, or one without a `ControlPath`, is left to ssh. Its questions are still answered by Shellder: the hop asks on the same terminal, before the host behind it, see [The prompt window](prompt-window.md#how-questions-are-answered).

Hosts whose jump hosts go round in a loop are switched off with that error.
