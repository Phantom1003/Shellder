# 原则

Shellder 在后台保持 `ssh -M -N` 主连接，这样之后的每一次 `ssh host`、`scp`、`rsync` 或 VS Code Remote 会话都复用它的套接字，不会再问密码。需要建立或重新建立主连接时，Shellder 自己从钥匙串回答密码、密钥口令和 TOTP 的提问；回答不了的（包括未知的主机密钥）在[提示窗口](prompt-window.md)里转给你。它运行在 macOS 和 Linux 上。

没有 `ControlPath` 的主机，以及 OpenSSH 不支持 ControlMaster 的平台（Windows）上的所有主机，同样会保持连接，为的是配置里要求的端口转发：还是那个 `ssh -N`，只是没有套接字，所以其他 ssh 命令无法复用它。这时控制台和文件窗口各自登录，提问由 Shellder 回答。见[没有套接字时](without-a-socket.md)。

## ssh 配置是唯一的配置

Shellder 列出 `~/.ssh/config` 里的每个 `Host` 块（会跟随 `Include`），用 `ssh -G` 解析每一个，并且从不写这个文件。`ControlMaster`、`ControlPath` 和 `ControlPersist` 都来自你的配置。

主机没有 `ControlPath` 时，它的连接在没有套接字的情况下保持，主机页会给出能让它得到套接字的那段配置。

## 主连接是什么

主连接以 `ssh -M -N <alias>` 运行，外加几个只对这个进程生效的选项：

| 选项 | 原因 |
|---|---|
| `ControlPersist=no` | 让这个进程归 Shellder 所有 |
| `NumberOfPasswordPrompts=1` | 让一个错误的凭据只造成一次登录失败 |
| `LogLevel=ERROR` | |
| `ServerAliveInterval` | 只在你的配置把它留为 0 时才加 |

## 只有一个钥匙串条目

凭据保存在钥匙串的同一个条目里，也就是保险库（服务名 `shellder:vault`，一个以主机为键的 JSON 对象）：在 macOS 上是登录钥匙串里的一个通用密码，在 Linux 上是 Secret Service 里的一个条目。只有一个条目，钥匙串最多只需要征求一次许可。详见[钥匙串](keychain.md)。

## 凭据只交给 ssh

凭据离开钥匙串，只会去往应用自己启动的 ssh。

- 每个主连接都有一个自己的伪终端作为控制终端，ssh 就在那里提问（`/dev/tty`）。应用从这个终端上读到提问，再把回答写回去。
- 没有辅助程序，也没有套接字，所以应用之外的任何东西都无法向它索要凭据。
- 命令行里没有任何命令会打印凭据，或者拿凭据去登录。

## 钥匙串之外的状态

| 位置 | 内容 |
|---|---|
| macOS 上的 `~/Library/Application Support/shellder/`，Linux 上的 `~/.local/state/shellder/` | `prefs.json`、锁、最近一次的 TOTP 验证码、`shellder.log` |
| macOS 上的 `~/Library/LaunchAgents/local.shellder.plist`，Linux 上的 `~/.config/autostart/local.shellder.desktop` | 登录项，可选（见[设置](settings.md)） |
