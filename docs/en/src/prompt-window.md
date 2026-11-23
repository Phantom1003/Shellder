# The prompt window

The prompt window is where Shellder passes a question on to you. ssh, sudo and polkit ask their questions (a password, a code, a host key confirmation) on a terminal; Shellder reads them there and answers what it can from the keychain. A question it cannot answer comes up in this small window, which floats over the others and, on macOS, over a full-screen window too: a Console in full screen stays where it is while a tab's login asks.

{{#include figures/prompt-window.html}}

## The parts of the window

- **The prompt head** says what is asked for (*Password needed*, *Key passphrase needed*, *Verification code needed*, *Confirm host key* or *ssh is asking*) and by whom: `ssh → host` for a login, or the action and its host for a command of the Console that wants a password (`mount /dev/sdb1`, `kill -TERM 1234`).
- **The queue counter**, at the head's right, says how many more questions wait (*+2 more*). One question is shown at a time; the next comes when this one is answered.
- **The question text** is the asker's own words, shown as written. For a host key it includes the fingerprint.
- **The answer field** is where you type. A password or passphrase is masked; the eye beside the field shows what you typed.
- **The save checkbox**, *Save in Keychain so shellder can answer this itself next time*, is offered for a password and a key passphrase. Ticked, the answer is stored as the host's credential. A password typed for a Console action can be saved as the host's too.
- **The buttons**: *Cancel* and *Continue*. Return is *Continue* once something is typed, Esc is *Cancel*.

## The kinds of question

| Kind | What you answer | Can be saved |
|---|---|---|
| password | the account's password on the host | yes |
| key passphrase | the passphrase of a private key | yes |
| verification code | the code from your authenticator. Store the TOTP secret on the host page's Credentials card and Shellder generates these codes itself | no |
| host key confirmation | no field: *Yes, connect* or *Don't connect* (Return and Esc). Compare the fingerprint with what the server's administrator gave you; answering yes adds the key to `~/.ssh/known_hosts` | no |
| anything else | free text | no |

## How questions are answered

ssh never reads a password from stdin: it opens its controlling terminal. Shellder gives every master a pseudo-terminal as its controlling terminal and nothing else. stdin, stdout and stderr stay on pipes, so ssh's diagnostics go to the log and never mix with the questions.

Text that ssh leaves on that terminal without a newline, ending in `:` or `?`, is a question. Shellder then:

1. works out which host the question belongs to. A `ProxyJump` hop prints `user@host`, which is matched against the resolved configuration of every host;
2. answers from the keychain when the credential is stored. TOTP codes are generated, and a code is never used twice;
3. otherwise shows the question in the prompt window.

A `ProxyJump` hop asks on the same terminal, before the host behind it, and its question carries its own `user@host`, which is how it is told apart.

A Console tab's login, a Files window's login and a copy's login are answered the same way, and so is what sudo or polkit asks when a Console action needs a password. See [Console](console.md).

## Cancelling

Cancelling a question of a host's master turns that host's switch off, so a wrong guess never hammers the server. Declining a host key does the same.

Cancelling the question of a Console action ends the action. A tab's, a Files window's or a copy's login that is cancelled fails by itself; the host's switch is not touched.

## A wrong password

Shellder runs ssh with `NumberOfPasswordPrompts=1`, so a bad credential produces exactly one failed login, and the host is switched off instead of retried. When ssh asks again for a password or passphrase that was just sent from the keychain, the server has rejected it: Shellder does not send it a second time, and the attempt ends. Change the credential on the host page's Credentials card, then switch the host on again.

Some servers ban the client's IP address for a while after a handful of failures.

A Console action is different: a sudo password need not be the login's, so when the host does not take the stored password, the question comes to the prompt window.
