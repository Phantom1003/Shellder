# 钥匙串

Shellder 保存的每一项凭据（密码、密钥口令、TOTP 密钥）都在保险库里，它是系统钥匙串中的一个条目：macOS 上是登录钥匙串，Linux 上是 Secret Service。本页说明 macOS 什么时候会要钥匙串密码、怎样为构建签名才能只被问一次、保险库在 Linux 上需要什么，以及测试时怎样让一份应用和你自己在用的那份互不干扰。

## macOS 上的登录钥匙串

登录钥匙串把凭据交给应用之前会检查两样东西：

- 条目的访问列表：谁可以读它；
- 应用的“分区”（partition）：用 Apple 签发的证书签名的应用是 `teamid:…`，否则是这次构建的 cdhash。

分区对不上，macOS 就会坚持要你输入钥匙串密码。

### 签名，让重新构建不再询问

用 Apple 签发的、带 Team ID 的证书签名：

1. 用任意 Apple ID 登录 Xcode：Settings → Accounts → Manage Certificates → + → Apple Development。免费账户的“Personal Team”就够了。
2. 构建。`build.sh` 会自动找到这个证书，或者使用 `SHELLDER_SIGN_IDENTITY` 里指定的那个。

没有这样的证书时，`build.sh` 会停下来，而不是做 ad hoc 签名，因为 ad hoc 构建每次重新构建都是一个新的身份，每次都会再问。确实需要时，`SHELLDER_SIGN_IDENTITY=-` 可以强制 ad hoc 签名。

### 新身份下的第一次读取

在新的身份下第一次读取时，会要一次钥匙串密码。

- 选“始终允许”（Always Allow）：这个身份就永久记在保险库上了。
- “允许”（Allow）只管这一次读取，下一次还会再问。

Shellder 从不删除或重建保险库。所以签名不同的两份应用（比如 `/Applications` 里随登录启动的发布版，和你手动启动的自己的构建）可以共用它，各被问一次即可。

### 请求被关掉之后

启动时被关掉的请求，五分钟之内不会再出现。[设置窗口](settings.md)的“钥匙串”分区可以立刻再请求一次。

这些请求不提供 Touch ID。

## Linux 上的钥匙串

保险库是 Secret Service 默认密钥环里的一个条目，所以必须有一个密钥环守护进程（GNOME Keyring、KWallet）在运行并且已解锁。桌面会话里通常就是这样。

还没有默认密钥环的会话（自动登录、不用密码的登录）读到的是一个空的保险库，第一次保存凭据时会创建密钥环：守护进程这时会让你为它设一个密码。

菜单栏图标需要 StatusNotifier 宿主：KDE 自带，GNOME 需要 AppIndicator 扩展。

## 隔离测试

可以让一份应用带着自己的 ssh 配置和状态运行，和你自己在用的那份分开。针对本地 sshd 容器的自动化端到端测试用的就是这个办法。

| 变量 | 效果 |
| --- | --- |
| `SHELLDER_SSH_CONFIG=<file>` | Shellder 读取这个 ssh 配置，并用 `-F` 传给 ssh |
| `SHELLDER_STATE_DIR=<dir>` | 把偏好设置、锁文件和日志换到这个目录 |
| `SHELLDER_NO_CONTROLMASTER=1` | 去掉 ControlMaster：应用的表现和在 ssh 不支持它的平台上一样，见[没有套接字时](without-a-socket.md) |
| `SHELLDER_NO_KEYCHAIN=1` | 保险库读出来是空的，也不会被写入，所以测试用的那份应用不会让钥匙串弹出任何请求 |

直接运行可执行文件（`build/Shellder.app/Contents/MacOS/shellder`），这些变量才会被继承。

前三个变量并不隔离保险库本身：没有设 `SHELLDER_NO_KEYCHAIN` 的测试应用读写的是你真正的保险库。

构建和命令行见[构建与运行](build.md)。
