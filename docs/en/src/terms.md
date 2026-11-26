# Terms

> **A proposal, not yet settled.** These are the names the documentation is to use for every part of the app. The interface, its Chinese translation and the manual still use the older names in the last column; they are to follow once the list is agreed.

One thing has one name, and one name means one thing. Where a word had two uses, one of them got another word: *tab* is now only what the Console's right side holds, and what the left side shows is a *panel*.


## Everywhere

Words that mean the same in every window.

| Term | Chinese | What it is | Called before |
|---|---|---|---|
| **this machine** | 本机 | The computer Shellder runs on. The interface's own label for it is *Local*. | Local, this machine, here |
| **host** | 主机 | A `Host` entry of the ssh config. Where a window chooses what it shows (the host menu, the host wheel, the host picker), this machine counts as a host too, listed first as *Local*. |  |
| **master** | 主连接 | The `ssh -M` connection Shellder keeps open for a host. | master (left in English in the Chinese interface) |
| **socket** | 套接字 | The master's control socket (`ControlPath`), which other ssh commands reuse. | socket (left in English) |
| **credential** | 凭据 | A password, key passphrase or TOTP secret stored for a host. Prose says *credential*, not *secret*. | secret, credential; 密钥 for secret, key and host key alike |
| **vault** | 保险库 | The one keychain item that holds every credential. | shellder vault |
| **host key** | 主机密钥 | The server's key, confirmed by its fingerprint. |  |
| **question** | 提问 | What ssh, sudo or polkit asks on its terminal: a password, a code, a host key confirmation. | prompt, question |
| **prompt window** | 提示窗口 | The small window in which Shellder passes a question on to you. Never called a dialog. | a dialog, the prompt window, a small window of its own |
| **dialog** | 对话框 | A box inside a window that asks for a value or a confirmation and holds the window until answered. | dialog (also used for the prompt window and the keychain's) |
| **toast** | 浮动提示 | A short message at the window's corner that goes away by itself. | toast |
| **notification** | 系统通知 | A message shown by the system's notification centre, not by the window. | a notification of the system's, a banner |
| **menu** | 菜单 | A list of commands opened from a button; a *context menu* (右键菜单) is the one the right button opens. | popup, menu, right-click menu |
| **card** | 卡片 | A rounded box with a title that groups rows about one thing. | card |
| **row** | 行 | One line of a card, a list or a table. |  |
| **section** | 分区 | A titled part of a settings page or of a panel, its title the *section title*. | section, part, head, label |
| **panel** | 面板 | A region of a window that shows one thing and can be put away: the main window's log panel, the Console side bar's panels. | tab, panel; 面板 also for tmux panes |
| **pane** | 窗格 | One of several equal views side by side: the Files window's two panes, a tmux pane. | pane; 栏, 面板 |
| **copy** | 复制 | A file or folder carried from one machine to another, and the act. Also what ⌘C does. One word in Chinese: 复制. | copy, transfer; 拷贝, 拷, 复制, 传输 |
| **duplicate** | 创建副本 | A second copy of a thing beside the first (a palette). | Duplicate; 复制 |
| **forward** | 端口转发 | A port here that reaches a port on the host (`-L`), or the other way (`-R`), made on the master. | forward; 隧道, 端口转发 |
| **direct connection** | 直连 | A port of the host opened at the host's own address, with no forward. | direct connection; “no tunnel” |
| **forget** | 移除 | Take a remembered row (a forward, a direct connection, a listed port) off its list. *Delete* (删除) is for what is then gone: a file, a credential, a theme file. | Forget, Take off the list, Remove; 删除, 移除 |
| **keyboard focus** | 键盘焦点 | Where typing goes. English prose may say *the keyboard*. | the keyboard; 焦点, 键盘, 键盘焦点 |
| **key** | 快捷键 | A key combination that runs an action. | key, keys; 快捷键, 按键, 键, 组合键 |

## The main window

The host list on the left, the selected host's page on the right, the log panel underneath.

| Term | Chinese | What it is | Called before |
|---|---|---|---|
| **main window** | 主窗口 | The window with the host list. |  |
| **host list** | 主机列表 | The left column: every host of the ssh config, one row each. | host list, the list; `sidebar` in the code |
| **host row** | 主机行 | A host in the host list: its status dot, its name, its Connect switch, and the Console button while the pointer is on it. | the host's row, list entry |
| **status dot** | 状态点 | The coloured dot that says how the host's connection is: grey off, yellow trying, green up, blue up and locked, cyan a master that is not Shellder's, red failed. | the dot |
| **Connect switch** | 连接开关 | A host's switch: on is one attempt to connect, off closes the master. | the switch, Connect switch |
| **lock** | 锁 | What keeps a host connected: a locked host is reconnected after a drop. The *Lock button* (锁定按钮) on the host page sets and clears it. | the lock, lock button |
| **list toolbar** | 列表工具栏 | The buttons under the host list: reload and edit the ssh config, Settings, the update, the log panel. | the main window's bottom bar; `sidebar-bar` |
| **host page** | 主机页 | The right side of the main window: everything about the selected host, in cards. | the host's page, host page, per-host details; `detail` in the code |
| **connection card** | 连接卡片 | The host page's first card: the host's name, its status line, the connection buttons, the keep-alive mode and the reconnect rule. | header card, its state |
| **status line** | 状态行 | The line under the host's name that spells the state out (*Connected*, *Dropped — retrying in 5s*). |  |
| **connection buttons** | 连接按钮 | Lock, Connect, Disconnect, Reconnect, Retry now, Take over, Close socket, whichever apply. | the master's controls |
| **keep-alive mode** | 保活方式 | What the master runs on the host so that the server keeps it: nothing (`-N`), an idle shell, or `cat`. | Keep-alive, keep-alive mode, the Keep-alive tool |
| **reconnect rule** | 重连规则 | How a locked host is brought back: the waits between tries and when to give up. The *default rule* (默认规则) is in Settings; a host may have *a rule of its own* (自定义规则). | Reconnect policy, Custom reconnect rule, Own rules; 重连策略, 重连规则 |
| **Tools card** | 工具卡片 | The Console and Files buttons, and under them the host's forwards and direct connections. | Tools card |
| **Credentials card** | 凭据卡片 | The credentials stored for the host, a *credential row* each. | the credentials |
| **Configuration card** | 配置卡片 | What `ssh -G` says the host's configuration comes to. | Effective configuration (not named in the manual) |
| **log panel** | 日志面板 | The log under the main window's two columns, opened and closed with ⌘L. | log panel |
| **menu bar icon** | 菜单栏图标 | Shellder's icon in the menu bar; its *menu bar menu* (菜单栏菜单) has a *host submenu* (主机子菜单) per host. | menu bar item, menu bar icon, tray |

## Settings and the prompt window

| Term | Chinese | What it is | Called before |
|---|---|---|---|
| **Settings window** | 设置窗口 | The app's settings. The Console has a window of its own for the terminals', the *Console settings window*. | Settings (used for both) |
| **prompt head** | 提示标题 | The top of the prompt window: what is asked for and by which host or action. |  |
| **question text** | 提问原文 | The asker's own words, shown as written. | the host's own words |
| **answer field** | 回答框 | Where the answer is typed. |  |

## The Files window and the file tree

The file tree is drawn in two places: in each pane of the Files window and in the Console's Files panel.

| Term | Chinese | What it is | Called before |
|---|---|---|---|
| **Files window** | 文件窗口 | A window per host with two panes side by side, where a drag copies. | Files, the Files window |
| **pane** | 窗格 | One half of the Files window, showing one host's file tree. | pane, tree; 栏 |
| **host picker** | 主机选择器 | The button at a pane's top left that chooses which host, or this machine, the pane shows. | (no name), “another machine”; `source` |
| **file toolbar** | 文件工具栏 | The buttons above a file tree: go to a folder, new folder, new file, delete, hidden files, back, up, refresh. | the toolbar, the bar above |
| **file tree** | 文件树 | The list of a folder's entries, in which a folder opens in place. | tree, the pane, the list |
| **column headings** | 列标题 | Name, Date Modified and Size above the file tree; a click sorts by one, the *sort arrow* (排序箭头) says which way. | header (in the code) |
| **file row** | 文件行 | An entry of the file tree. | row |
| **disclosure arrow** | 展开箭头 | The arrow at a folder's left that opens and closes it in the tree. | the chevron |
| **path bar** | 路径栏 | The folder's path along the foot of a file tree, each part a button. | (not in the manual) |
| **folder picker** | 文件夹选择器 | The small tree of folders that *Go to folder…* and a new session's folder button open. | a folder menu, a tree of the host's folders |
| **copies strip** | 复制状态条 | The one line under the file tree: the copy under way with its progress, how many wait, *Clear*. | Transfers; the strip (in the code) |
| **copies list** | 复制列表 | Every copy of this host, opened from the copies strip. | the list underneath, history |
| **name question** | 重名提问 | The dialog asked when a copy lands on a name that is taken: *Replace*, *Skip*, *Apply to all*. | their questions |
| **continue** | 继续 | Go on with a copy that stopped, from what it wrote (▷). | continue, go on, resume; 从断点继续 |

## The Console window

A top bar, a side bar on the left that shows one panel at a time, and the tab area on the right. *Tab* is a word for the right side only.

| Term | Chinese | What it is | Called before |
|---|---|---|---|
| **Console** | 控制台 | A host's window of readings and terminals. | Console; 监控台, Console |
| **top bar** | 顶栏 | The bar across the top of the window; its blank drags the window. | the bar, the top bar |
| **host menu** | 主机菜单 | The host's name in the top bar: a menu that switches the window to another host or to this machine. | the host's name, a menu; `hostpick` |
| **settings button** | 设置按钮 | The gear at the top bar's right, which opens the Console settings window. | the gear, the keyboard button; `keysbtn` |
| **side bar** | 侧栏 | The left region, which shows one panel and can be folded away. | the column; 左栏, 侧栏, 左侧栏 |
| **panel switcher** | 面板切换条 | The strip of icons down the window's left edge. | a strip of icons, the column's tabs; `tabstrip` |
| **panel button** | 面板按钮 | An icon of the panel switcher: it shows its panel, and the shown panel's button folds the side bar. | tab; `.tabstrip .tab` |
| **Status panel** | 状态面板 | The host's readings, in cards. | the Status tab; 状态页, “状态”标签页 |
| **Files panel** | 文件面板 | The host's file tree, with its copies strip. | the Files tab; “文件”标签页 |
| **Sessions panel** | 会话面板 | The host's tmux and screen sessions. | the Sessions tab |
| **Jobs panel** | 作业面板 | Your Slurm and PBS jobs, on a host that has a scheduler. | the Jobs tab |
| **Containers panel** | 容器面板 | The Docker or Podman containers. | the Containers tab |
| **Ports panel** | 端口面板 | The forwards, the direct connections and the host's own ports. | the Ports tab |
| **side bar visit** | 侧栏访问 | The keyboard focus taken into the side bar and back (⌘⇧L); a folded side bar comes out for it. | a visit, the keyboard to the column |

## The Status panel

| Term | Chinese | What it is | Called before |
|---|---|---|---|
| **summary card** | 概况卡片 | The first card: the host's name, system, uptime, load, CPUs and logins. | (no name) |
| **Usage card** | 用量卡片 | CPU, memory and swap, each a gauge. | Usage; 占用 |
| **gauge** | 用量条 | A label, a value and a bar that fills with it. | gauge, row |
| **interval picker** | 刷新间隔 | How often the readings are taken. | the interval |
| **card picker** | 卡片选择 | The eye on the Usage card: which cards are shown, and read. | the eye |
| **reading** | 读数 | What the Status panel reads from the host at each interval. | reading |
| **busiest table** | 最忙进程表 | The Processes card as a table of the busiest processes. | the Processes table, the busiest |
| **process tree** | 进程树 | The Processes card as the tree under the active tab. | the process tree |
| **signal menu** | 信号菜单 | The context menu of a process: the signals to send it. |  |

## Sessions, jobs, containers and ports

| Term | Chinese | What it is | Called before |
|---|---|---|---|
| **session row** | 会话行 | A tmux or screen session in the Sessions panel, with its folder, terminal, rename and end buttons. | row |
| **window chip** | 窗口标记 | A tmux window of a session, listed on its row. | the windows |
| **new-session dialog** | 新建会话对话框 | Asks for a new session's name, directory and command. |  |
| **job row** | 作业行 | A job of the Jobs panel. |  |
| **container row** | 容器行 | A container of the Containers panel. |  |
| **forward row** | 转发行 | A forward, with whether it is up and its start, stop and forget buttons. |  |
| **host port** | 主机端口 | A port the host itself listens on, listed in the Ports panel. | the host's own ports |

## The tab area

| Term | Chinese | What it is | Called before |
|---|---|---|---|
| **tab area** | 标签区 | The right region of the Console: one group, or several split apart. | the terminals, terminals in tabs |
| **group** | 分组 | A set of tabs with its own tab bar, showing one of them. | group; 分组, 组 |
| **tab bar** | 标签栏 | The row of a group's tabs with the group buttons at its right. | tab row; 标签行 |
| **tab** | 标签页 | One thing open in a group: a terminal or a preview. | tab; 标签, 标签页 |
| **new-tab button** | 新建按钮 | The + after the last tab: a new shell on the host. | the + |
| **group buttons** | 分组按钮 | The buttons at a tab bar's right: the group's ready bell, the background button and the split button. | the ends |
| **background button** | 背景按钮 | Gives the group's terminals, or its previews, a dark or a light background. | the sun and moon, the light/dark button, the group's button |
| **split** | 分栏 | A second group beside or under one, and the act of making it; the *split button* (分栏按钮) offers both ways. | split |
| **divider** | 分隔线 | The line between two groups, which drags. | divider |
| **drop zone** | 放置区 | Where a dragged tab would land, shown while it is dragged: a tab bar, a group's edge (a split) or its middle. |  |
| **solo** | 单独显示 | One group shown alone, as large as the tab area (⌘⇧↩), and back. | the group alone, shown alone; `zoom` in the code |
| **layout** | 布局 | The groups, their splits and their tabs, kept per host and brought back with the window. | the layout |

## Tabs

| Term | Chinese | What it is | Called before |
|---|---|---|---|
| **terminal tab** | 终端标签页 | A tab that is a terminal. By what runs in it: a *shell tab* (shell 标签页), a *session tab* (会话标签页), a *job tab* (作业标签页), a *container tab* (容器标签页). | terminal tab, shell tab |
| **preview tab** | 预览标签页 | A tab that shows a file of the Files panel. | a preview tab, the previews |
| **temporary tab** | 临时标签页 | A tab opened by a single click, its name in italics: the next single click takes it over. | temporary tab |
| **kept tab** | 固定标签页 | A tab that stays: opened by a double click, or a temporary tab double-clicked. To *keep* a tab is 固定. | keeps the tab; 保留, 固定 |
| **ended tab** | 已结束的标签页 | A terminal tab whose command ended badly: it says so, and Return in it connects again. | (no name) |
| **tab mark** | 标签标记 | The dot on a tab that is not shown: new output, the terminal bell or ready bell (orange), a command done (green) or failed (red). | a dot, the mark |
| **host label** | 主机标注 | The host's name at the start of a tab that runs on another host than the window shows. |  |
| **tab progress** | 标签进度条 | The thin bar along a tab's foot while its program reports progress. | a thin bar along its tab's foot |

## Inside a terminal

| Term | Chinese | What it is | Called before |
|---|---|---|---|
| **terminal bell** | 终端响铃 | The bell a program rings (BEL): a flash of the terminal's edge, a sound, or both. | the bell (also used for the ready bell) |
| **ready bell** | 就绪提醒 | The bell button in a terminal's top right corner: switched on, it tells every time the terminal is ready for input again. A group's ready bell switches it for every tab of the group. | the bell, bell button; 响铃 |
| **lit group** | 分组高亮 | The orange glow round a group in which a ready bell rang unseen. | the group lit |
| **password offer** | 密码代填 | The small box in a terminal's top right corner that offers to type the host's stored password at a password prompt. | a small box, the offer |
| **paste question** | 粘贴确认 | The strip that asks before several lines are pasted into a shell that would run them. | a strip |
| **find strip** | 查找条 | The search field with its count and arrows, in a terminal (⌘F) and in a preview alike. | a strip, the search strip |
| **terminal menu** | 终端菜单 | The context menu of a terminal. |  |
| **path link** | 路径链接 | A path in a terminal's text: ⌘-click shows the file in the Files panel, ⌥⌘-click in a preview tab. | Paths are links too |
| **shell integration** | Shell 集成 | The lines a shell started by the Console is given, which mark its prompts and report its directory. | shell integration |
| **prompt mark** | 提示符标记 | Where the shell integration says a prompt, a command or its output begins; what ⌘↑ and ⌘↓ jump between. | the marks |

## Features of the Console window

| Term | Chinese | What it is | Called before |
|---|---|---|---|
| **host wheel** | 主机轮盘 | The ring of hosts drawn over the tab area while ⌘⇧ is held, or the mouse's left and right buttons together; letting go switches the window to the *chosen host* (选中的主机). | the host wheel, the mark; 主机转盘 |
| **immersive mode** | 沉浸模式 | The terminals alone: no top bar, side bar or tab bars (⌘⇧I). | immersive mode |
| **quick terminal** | 快捷终端 | The terminal a key of its own drops from the top of the screen, in any application. | quick terminal |
| **reopen** | 恢复关闭的标签页 | Bring the tab closed last back (⌘⇧T). |  |

## The preview tab

| Term | Chinese | What it is | Called before |
|---|---|---|---|
| **preview header** | 预览标题栏 | The line at a preview's top: the file's path, its size or page, and the preview's buttons. | the header |
| **source view** | 原文视图 | A Markdown or HTML file shown as written instead of as the page it makes. | as written |
| **fit** | 适配 | A picture, video or PDF as wide as the tab (⌘0), or as tall. | fitted to |
| **two-page view** | 双页视图 | A PDF's pages two by two. | the book button, two by two |
| **follow** | 跟随 | A preview on screen reads its file again when the file changes. | followed |

## The Console settings window

| Term | Chinese | What it is | Called before |
|---|---|---|---|
| **page** | 页 | One of the window's five parts, picked from the *page bar* (页栏) at its top: Terminal, Manners, Quick terminal, Theme, Keys. | pages |
| **palette** | 配色 | A set of terminal colours: a *built-in palette* (内置配色) or a *theme file* (主题文件). *Theme* names the page only. | theme, palette, colours |
| **swatch** | 色样 | The small picture of a palette's colours beside its name. | swatch |
| **sample terminal** | 示例终端 | The terminal on the Theme page drawn in the chosen palette. | a sample terminal |
| **colour well** | 取色框 | The box that changes one colour of a theme file. | a well |
| **key chip** | 快捷键标签 | A key on the Keys page: a click, then the keys to use instead. | a chip |
