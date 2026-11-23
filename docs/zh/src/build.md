# 构建与运行

Shellder 是用 Tauri 构建的单个可执行文件：不带参数启动是应用，带参数启动是命令行工具。本页包括用 `build.sh` 构建、应用图标、应用如何在后台运行和随登录启动、界面语言、更新，以及命令行。签名和钥匙串见[钥匙串](keychain.md)。

## 构建

```bash
./build.sh            # -> build/Shellder.app (macOS), build/*.deb and *.AppImage (Linux)
./build.sh --install  # also copy to ~/Applications and launch (macOS)
./build.sh --debug    # a debug build, unsigned, no updater artifacts
./build.sh --deps     # only install what a build needs
```

`build.sh` 会安装缺少的东西：

- Rust，通过 rustup；
- Tauri CLI；
- 在 Linux 上，每个 Tauri 应用都要链接的那些库的开发包（webkit2gtk 4.1、libayatana-appindicator、librsvg、dbus 等），通过 apt、dnf、pacman 或 zypper 安装。

`./build.sh --deps` 只做这一步。

在 macOS 上，`build.sh` 用它在钥匙串里找到的 Apple Development 证书为构建签名，或者用 `SHELLDER_SIGN_IDENTITY` 指定的证书。为什么要这样，以及没有证书时会怎样，见[钥匙串](keychain.md)。

## 应用图标

`build.sh` 读取 `scripts/icon-source.png`（或 `.jpg`），让 Tauri CLI 把它裁剪、缩放成应用包需要的各种尺寸。

菜单栏图标是用同一张图做的单色模板图（浅色像素不透明，深色像素透明），作为 `src-tauri/icons/tray.png` 提交在仓库里，所以 macOS 会像对待自己的状态图标一样给它着色。换了源图之后，用下面的命令重新生成：

```bash
cargo run --release --manifest-path scripts/menubar-template/Cargo.toml -- \
    scripts/icon-source.png src-tauri/icons/tray.png 36
```

## 应用的组成

- [主窗口](main-window.md)：主机列表（带连接开关和锁）、主机页（带凭据）和日志面板。
- [设置窗口](settings.md)：登录时启动、静默启动、后台行为、菜单栏图标、默认重连规则。终端自己的设置在[控制台设置窗口](console-settings.md)里。
- 菜单栏图标。

## 关闭窗口、后台运行与退出

默认情况下，关闭最后一个窗口就会退出应用，主连接也随之关闭。

打开“后台执行”之后，关闭窗口只是让应用离开 Dock，连接在后台继续保持。这时通过菜单栏图标，或再次启动应用，就能回到它：

- 点击菜单栏图标打开主窗口；
- 主窗口在前面时再点一次，把它收起；
- 右键点击显示菜单栏菜单。

最小化到 Dock 的控制台或文件窗口仍然算打开着：这时关闭主窗口，应用会带着那个窗口留在 Dock 里。

退出时，应用启动的所有 ssh 都会一起结束，包括终端的、会话的和复制用的。它们都不允许作为后台主连接留下来：不管 ssh 配置怎么写，这些 ssh 的 `ControlPersist` 都是关闭的。macOS 26 会把仍有进程残留的应用报告为“仍在后台运行”。

## 菜单栏图标

没有任何主连接建立时，菜单栏图标是淡的；只要有一个建立了，就变成正常的样子。错误和提问不会改变它。

在 Linux 上，面板按原样显示这张图，是深色的。“菜单栏图标反色”把它变成浅色，适合深色面板；这个开关只在 Linux 上显示，因为 macOS 会自己给图标着色。

## 随登录启动

“登录时启动”把应用注册为登录项。“静默启动”让登录项启动应用时不打开窗口；双击或 `open` 总是会打开窗口。

- “静默启动”只能和“登录时启动”“后台执行”一起打开（没有后者的话，就既没有窗口也没有图标了），并且会随“后台执行”一起关闭。
- `shellder --background` 强制单次启动不打开窗口。
- 这两个开关在你打开之前都是关闭的：在你要求之前，不会往 `~/Library/LaunchAgents` 里写任何东西。

## 语言

界面语言在设置窗口里选择：不管系统语言是什么，除非你另选，否则是英文。改动在重新启动后生效。内置英文和简体中文；日志始终是英文。

添加一种语言：

1. 把 `ui/lang/zh-Hans.json` 复制为 `ui/lang/<lang>.json`，用 BCP 47 名称，比如 `ja` 或 `zh-Hant`。
2. 翻译右边的值。键是英文原文；`{name}` 是占位符。
3. 把这种语言加到 `ui/settings.js` 顶部的列表里。

译文里缺少的键回退到英文。

## 更新

设置窗口的末尾是版本号和“软件更新”一行。

- “自动检查更新”打开时（默认），应用在启动几秒之后以及此后每天一次，从 GitHub 读取最新发布版的 `latest.json`。发现更新时，会在设置窗口、主窗口的列表工具栏和菜单栏菜单里提示。
- “立即检查”马上检查一次。
- 在你点击“安装并重新启动”之前，不会安装任何东西。点击之后，应用下载该发布版中本平台的更新包，用内置在应用里的公钥校验签名，就地替换正在运行的应用包，然后重新启动。
- 只有版本号比当前运行的更新，发布版才算数，所以版本号相同的本地构建不会被替换。

发布版用维护者的 Apple Development 证书签名。所以从你自己签名的构建更新过来之后，第一次启动时钥匙串可能会要一次密码，见[钥匙串](keychain.md)。

## 命令行

同一个可执行文件也是命令行工具。应用没有运行时它也能用。没有哪个命令会打印凭据，或用凭据去登录。

```bash
shellder hosts                    # Host entries and their ControlPath
shellder lock socjump             # keep this host connected
shellder unlock socjump           # no longer keep it connected
shellder status                   # UP/down per locked host
shellder add-secret socjump password
shellder del-secret socjump password
shellder install | uninstall      # login item
shellder help
```

| 命令 | 作用 |
| --- | --- |
| `hosts` | 列出 ssh 配置里的 `Host` 条目和它们的 `ControlPath` |
| `status` | 报告每台锁定的主机的主连接是否已建立 |
| `lock HOST`、`unlock HOST` | 锁定一台主机（断线后重连，启动时连接），或解除锁定 |
| `add-secret HOST KIND` | 把一项凭据存入保险库；`KIND` 是 `password`、`passphrase` 或 `totp` |
| `del-secret HOST KIND` | 删除一项已保存的凭据 |
| `install` | 把应用注册为登录项并启动它 |
| `uninstall` | 停止应用并移除登录项 |
| `help` | 列出这些命令 |

不带命令启动时，这个可执行文件就是应用。`--background` 让这一次启动不打开窗口；`--login` 是登录项传入的参数，只有这种启动才会遵守“静默启动”。

开关和锁的说明见[开关与锁](switch-and-lock.md)。
