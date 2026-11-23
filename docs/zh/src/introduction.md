# Shellder

**Shellder 让你的 SSH 连接随时可用。** 它是一个 macOS 和 Linux 上的菜单栏小应用，
在后台保持 `ssh -M -N` 主连接，之后的每一次 `ssh`、`scp`、`rsync` 或 VS Code
Remote 都复用同一个套接字，立刻连上，不用再输入密码、密钥口令或一次性验证码。

主连接需要重新建立时，Shellder 会从钥匙串里取出答案自己回答提问，TOTP 验证码也
包括在内；只有它回答不了的才会来问你，比如未知的主机密钥。

![主窗口：主机列表，每台主机一个开关和一把锁，右侧是所选主机的主机页](../images/zh/main-window.png)

## 从这里开始

* [原则](principles.md)：Shellder 会碰什么，不会碰什么。
* [术语表](terms.md)：各个窗口每一部分的名字。其他页面只用这些名字。
* [主窗口](main-window.md)和[开关与锁](switch-and-lock.md)：主机和它们的连接。
* [控制台](console.md)：一台主机的读数、文件、会话和终端都在一个窗口里。
* [文件窗口](files-window.md)：两棵文件树并排。
* [构建与运行](build.md)和[钥匙串](keychain.md)。
* [源码参考](reference.md)：给阅读或修改代码的人。

每个窗口的页面都以它的一张图开头，每个部分上有一个带编号的标记；图下面列出的名字
就是[术语表](terms.md)里的名字。

本文档也有[英文版](../en/index.html)。
