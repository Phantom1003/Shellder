# Shellder

**Shellder keeps your SSH connections warm.** It is a small menu bar app
for macOS and Linux that holds `ssh -M -N` master connections open in the
background, so every later `ssh`, `scp`, `rsync` or VS Code Remote session
reuses the socket and connects at once, with no password, passphrase or
one-time code to type.

When a master has to be established again, Shellder answers the questions
itself from your keychain, TOTP codes included, and asks you only what it
cannot answer, such as an unknown host key.

![The main window: the host list with a switch and a lock per host, and the selected host's page](../images/en/main-window.png)

## Where to go from here

* [Principles](principles.md): what Shellder does and does not touch.
* [Terms](terms.md): the name of every part of the windows. The other
  pages use these names and no others.
* [The main window](main-window.md), with [the switch and the
  lock](switch-and-lock.md): the hosts and their connections.
* [The Console](console.md): a host's readings, files, sessions and
  terminals in one window.
* [The Files window](files-window.md): two file trees side by side.
* [Build and run](build.md) and [the keychain](keychain.md).
* [Source reference](reference.md), for whoever reads or changes the code.

Every window's page starts with a picture of it, a numbered pin on each
part; the names under the picture are the ones of the [terms](terms.md).

This documentation is also there [in Chinese](../zh/index.html).
