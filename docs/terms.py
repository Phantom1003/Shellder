#!/usr/bin/env python3
"""The names of the app's parts, in English and Chinese, in one list:
`python3 docs/terms.py` writes the two books' Terms pages from it
(docs/en/src/terms.md, docs/zh/src/terms.md), so that they cannot name a
thing differently, and docs/shots takes the labels of the annotated
pictures from the same list (`TERMS`).

An entry: (English term, Chinese term, English line, Chinese line, what
it was called before); `S` starts a section: (S, English title, Chinese
title, English line, Chinese line).
"""
import os

S = object()
DATA = [
    (S, "Everywhere", "通用",
     "Words that mean the same in every window.",
     "在每个窗口里含义都相同的词。"),
    ("this machine", "本机", "The computer Shellder runs on. The interface's own label for it is *Local*.", "运行 Shellder 的这台电脑。界面上的标签是“本机”（英文界面为 *Local*）。", "Local, this machine, here"),
    ("host", "主机", "A `Host` entry of the ssh config. Where a window chooses what it shows (the host menu, the host wheel, the host picker), this machine counts as a host too, listed first as *Local*.", "ssh 配置里的一个 `Host` 条目。在窗口选择显示对象的地方（主机菜单、主机轮盘、主机选择器），本机也算一台主机，排在最前面，标签是“本机”。", ""),
    ("master", "主连接", "The `ssh -M` connection Shellder keeps open for a host.", "Shellder 为一台主机保持的 `ssh -M` 连接。", "master (left in English in the Chinese interface)"),
    ("socket", "套接字", "The master's control socket (`ControlPath`), which other ssh commands reuse.", "主连接的控制套接字（`ControlPath`），其他 ssh 命令复用它。", "socket (left in English)"),
    ("credential", "凭据", "A password, key passphrase or TOTP secret stored for a host. Prose says *credential*, not *secret*.", "为主机保存的密码、密钥口令或 TOTP 密钥。正文统一说“凭据”。", "secret, credential; 密钥 for secret, key and host key alike"),
    ("vault", "保险库", "The one keychain item that holds every credential.", "保存全部凭据的那一个钥匙串条目。", "shellder vault"),
    ("host key", "主机密钥", "The server's key, confirmed by its fingerprint.", "服务器的密钥，凭指纹确认。", ""),
    ("question", "提问", "What ssh, sudo or polkit asks on its terminal: a password, a code, a host key confirmation.", "ssh、sudo 或 polkit 在终端上问的问题：密码、验证码、主机密钥确认。", "prompt, question"),
    ("prompt window", "提示窗口", "The small window in which Shellder passes a question on to you. Never called a dialog.", "Shellder 把提问转给你回答的小窗口。不叫“对话框”。", "a dialog, the prompt window, a small window of its own"),
    ("dialog", "对话框", "A box inside a window that asks for a value or a confirmation and holds the window until answered.", "窗口内部弹出的框，要一个值或一次确认，回答之前窗口不能操作。", "dialog (also used for the prompt window and the keychain's)"),
    ("toast", "浮动提示", "A short message at the window's corner that goes away by itself.", "窗口角上自动消失的短消息。", "toast"),
    ("notification", "系统通知", "A message shown by the system's notification centre, not by the window.", "由系统通知中心显示的消息，不是窗口自己画的。", "a notification of the system's, a banner"),
    ("menu", "菜单", "A list of commands opened from a button; a *context menu* (右键菜单) is the one the right button opens.", "从按钮打开的命令列表；右键打开的叫“右键菜单”。", "popup, menu, right-click menu"),
    ("card", "卡片", "A rounded box with a title that groups rows about one thing.", "带标题的圆角框，放关于同一件事的几行内容。", "card"),
    ("row", "行", "One line of a card, a list or a table.", "卡片、列表或表格里的一行。", ""),
    ("section", "分区", "A titled part of a settings page or of a panel, its title the *section title*.", "设置页或面板里带标题的一部分，标题叫“分区标题”。", "section, part, head, label"),
    ("panel", "面板", "A region of a window that shows one thing and can be put away: the main window's log panel, the Console side bar's panels.", "窗口里显示一种内容、可以收起的区域：主窗口的日志面板、控制台侧栏的各个面板。", "tab, panel; 面板 also for tmux panes"),
    ("pane", "窗格", "One of several equal views side by side: the Files window's two panes, a tmux pane.", "并排的几个对等视图之一：文件窗口的两个窗格、tmux 的窗格。", "pane; 栏, 面板"),
    ("copy", "复制", "A file or folder carried from one machine to another, and the act. Also what ⌘C does. One word in Chinese: 复制.", "把文件或文件夹从一台机器送到另一台，以及这个动作；⌘C 也是它。中文统一用“复制”。", "copy, transfer; 拷贝, 拷, 复制, 传输"),
    ("duplicate", "创建副本", "A second copy of a thing beside the first (a palette).", "在原物旁边再做一份（配色）。", "Duplicate; 复制"),
    ("forward", "端口转发", "A port here that reaches a port on the host (`-L`), or the other way (`-R`), made on the master.", "本机端口通向主机端口（`-L`），或反过来（`-R`），建在主连接上。", "forward; 隧道, 端口转发"),
    ("direct connection", "直连", "A port of the host opened at the host's own address, with no forward.", "按主机自己的地址直接打开它的一个端口，不经转发。", "direct connection; “no tunnel”"),
    ("forget", "移除", "Take a remembered row (a forward, a direct connection, a listed port) off its list. *Delete* (删除) is for what is then gone: a file, a credential, a theme file.", "把记住的一行（转发、直连、列出的端口）从列表里拿掉。“删除”用于真正消失的东西：文件、凭据、主题文件。", "Forget, Take off the list, Remove; 删除, 移除"),
    ("keyboard focus", "键盘焦点", "Where typing goes. English prose may say *the keyboard*.", "键盘输入去往的地方。", "the keyboard; 焦点, 键盘, 键盘焦点"),
    ("key", "快捷键", "A key combination that runs an action.", "执行某个动作的按键组合。", "key, keys; 快捷键, 按键, 键, 组合键"),

    (S, "The main window", "主窗口",
     "The host list on the left, the selected host's page on the right, the log panel underneath.",
     "左边是主机列表，右边是所选主机的主机页，下面是日志面板。"),
    ("main window", "主窗口", "The window with the host list.", "带主机列表的那个窗口。", ""),
    ("host list", "主机列表", "The left column: every host of the ssh config, one row each.", "左边一列：ssh 配置里的每台主机各占一行。", "host list, the list; `sidebar` in the code"),
    ("host row", "主机行", "A host in the host list: its status dot, its name, its Connect switch, and the Console button while the pointer is on it.", "主机列表里的一台主机：状态点、名字、连接开关，指针移上去时还有控制台按钮。", "the host's row, list entry"),
    ("status dot", "状态点", "The coloured dot that says how the host's connection is: grey off, yellow trying, green up, blue up and locked, cyan a master that is not Shellder's, red failed.", "表示连接状态的彩色圆点：灰色关闭、黄色连接中、绿色已连接、蓝色已连接并锁定、青色是别人的主连接、红色失败。", "the dot"),
    ("Connect switch", "连接开关", "A host's switch: on is one attempt to connect, off closes the master.", "主机的开关：打开是尝试连接一次，关闭则关掉主连接。", "the switch, Connect switch"),
    ("lock", "锁", "What keeps a host connected: a locked host is reconnected after a drop. The *Lock button* (锁定按钮) on the host page sets and clears it.", "让主机保持连接的状态：锁定的主机断线后会重连。主机页上的“锁定按钮”负责上锁和解锁。", "the lock, lock button"),
    ("list toolbar", "列表工具栏", "The buttons under the host list: reload and edit the ssh config, Settings, the update, the log panel.", "主机列表下方的一排按钮：重新加载和编辑 ssh 配置、设置、更新、日志面板。", "the main window's bottom bar; `sidebar-bar`"),
    ("host page", "主机页", "The right side of the main window: everything about the selected host, in cards.", "主窗口右侧：所选主机的全部信息，分成几张卡片。", "the host's page, host page, per-host details; `detail` in the code"),
    ("connection card", "连接卡片", "The host page's first card: the host's name, its status line, the connection buttons, the keep-alive mode and the reconnect rule.", "主机页的第一张卡片：主机名、状态行、连接按钮、保活方式和重连规则。", "header card, its state"),
    ("status line", "状态行", "The line under the host's name that spells the state out (*Connected*, *Dropped — retrying in 5s*).", "主机名下面用文字写出状态的那一行（“已连接”“已断开，5 秒后重试”）。", ""),
    ("connection buttons", "连接按钮", "Lock, Connect, Disconnect, Reconnect, Retry now, Take over, Close socket, whichever apply.", "锁定、连接、断开、重新连接、立即重试、接管、关闭套接字，按当前状态显示其中几个。", "the master's controls"),
    ("keep-alive mode", "保活方式", "What the master runs on the host so that the server keeps it: nothing (`-N`), an idle shell, or `cat`.", "主连接在主机上运行什么来让服务器不断开它：什么都不运行（`-N`）、空闲 shell 或 `cat`。", "Keep-alive, keep-alive mode, the Keep-alive tool"),
    ("reconnect rule", "重连规则", "How a locked host is brought back: the waits between tries and when to give up. The *default rule* (默认规则) is in Settings; a host may have *a rule of its own* (自定义规则).", "锁定的主机如何恢复连接：两次尝试之间等多久、何时放弃。“默认规则”在设置里；主机可以有“自定义规则”。", "Reconnect policy, Custom reconnect rule, Own rules; 重连策略, 重连规则"),
    ("Tools card", "工具卡片", "The Console and Files buttons, and under them the host's forwards and direct connections.", "控制台和文件两个按钮，下面是这台主机的端口转发和直连。", "Tools card"),
    ("Credentials card", "凭据卡片", "The credentials stored for the host, a *credential row* each.", "为这台主机保存的凭据，每项一行。", "the credentials"),
    ("Configuration card", "配置卡片", "What `ssh -G` says the host's configuration comes to.", "`ssh -G` 给出的这台主机的最终配置。", "Effective configuration (not named in the manual)"),
    ("log panel", "日志面板", "The log under the main window's two columns, opened and closed with ⌘L.", "主窗口两栏下方的日志，用 ⌘L 打开和收起。", "log panel"),
    ("menu bar icon", "菜单栏图标", "Shellder's icon in the menu bar; its *menu bar menu* (菜单栏菜单) has a *host submenu* (主机子菜单) per host.", "Shellder 在菜单栏上的图标；它的“菜单栏菜单”里每台主机有一个“主机子菜单”。", "menu bar item, menu bar icon, tray"),

    (S, "Settings and the prompt window", "设置与提示窗口", "", ""),
    ("Settings window", "设置窗口", "The app's settings. The Console has a window of its own for the terminals', the *Console settings window*.", "应用的设置。终端相关的设置在控制台自己的“控制台设置窗口”里。", "Settings (used for both)"),
    ("prompt head", "提示标题", "The top of the prompt window: what is asked for and by which host or action.", "提示窗口的上部：要的是什么，以及是哪台主机或哪个操作在问。", ""),
    ("question text", "提问原文", "The asker's own words, shown as written.", "提问方的原话，原样显示。", "the host's own words"),
    ("answer field", "回答框", "Where the answer is typed.", "输入回答的地方。", ""),

    (S, "The Files window and the file tree", "文件窗口与文件树",
     "The file tree is drawn in two places: in each pane of the Files window and in the Console's Files panel.",
     "文件树出现在两处：文件窗口的每个窗格里，以及控制台的文件面板里。"),
    ("Files window", "文件窗口", "A window per host with two panes side by side, where a drag copies.", "每台主机一个，两个窗格并排，拖动即复制。", "Files, the Files window"),
    ("pane", "窗格", "One half of the Files window, showing one host's file tree.", "文件窗口的一半，显示一台主机的文件树。", "pane, tree; 栏"),
    ("host picker", "主机选择器", "The button at a pane's top left that chooses which host, or this machine, the pane shows.", "窗格左上角的按钮，选择这个窗格显示哪台主机或本机。", "(no name), “another machine”; `source`"),
    ("file toolbar", "文件工具栏", "The buttons above a file tree: go to a folder, new folder, new file, delete, hidden files, back, up, refresh.", "文件树上方的按钮：前往文件夹、新建文件夹、新建文件、删除、隐藏文件、返回、上一级、刷新。", "the toolbar, the bar above"),
    ("file tree", "文件树", "The list of a folder's entries, in which a folder opens in place.", "一个文件夹里各项的列表，文件夹可以就地展开。", "tree, the pane, the list"),
    ("column headings", "列标题", "Name, Date Modified and Size above the file tree; a click sorts by one, the *sort arrow* (排序箭头) says which way.", "文件树上方的“名称”“修改日期”“大小”；点击按该列排序，“排序箭头”表示方向。", "header (in the code)"),
    ("file row", "文件行", "An entry of the file tree.", "文件树里的一项。", "row"),
    ("disclosure arrow", "展开箭头", "The arrow at a folder's left that opens and closes it in the tree.", "文件夹左边的箭头，在树里展开或收起它。", "the chevron"),
    ("path bar", "路径栏", "The folder's path along the foot of a file tree, each part a button.", "文件树底部的当前路径，每一段都是按钮。", "(not in the manual)"),
    ("folder picker", "文件夹选择器", "The small tree of folders that *Go to folder…* and a new session's folder button open.", "“前往文件夹…”和新建会话的文件夹按钮打开的小型文件夹树。", "a folder menu, a tree of the host's folders"),
    ("copies strip", "复制状态条", "The one line under the file tree: the copy under way with its progress, how many wait, *Clear*.", "文件树下方的一行：正在进行的复制及其进度、等待的个数、“清空”。", "Transfers; the strip (in the code)"),
    ("copies list", "复制列表", "Every copy of this host, opened from the copies strip.", "这台主机的所有复制，从复制状态条展开。", "the list underneath, history"),
    ("name question", "重名提问", "The dialog asked when a copy lands on a name that is taken: *Replace*, *Skip*, *Apply to all*.", "复制落到已有同名项时问的对话框：“替换”“跳过”“应用于全部”。", "their questions"),
    ("continue", "继续", "Go on with a copy that stopped, from what it wrote (▷).", "从已写入的部分接着做一次中断的复制（▷）。", "continue, go on, resume; 从断点继续"),

    (S, "The Console window", "控制台窗口",
     "A top bar, a side bar on the left that shows one panel at a time, and the tab area on the right. *Tab* is a word for the right side only.",
     "顶栏，左边一次显示一个面板的侧栏，右边是标签区。“标签页”这个词只用于右边。"),
    ("Console", "控制台", "A host's window of readings and terminals.", "一台主机的窗口：各项读数和终端。", "Console; 监控台, Console"),
    ("top bar", "顶栏", "The bar across the top of the window; its blank drags the window.", "窗口顶部的横条；空白处可以拖动窗口。", "the bar, the top bar"),
    ("host menu", "主机菜单", "The host's name in the top bar: a menu that switches the window to another host or to this machine.", "顶栏里的主机名：一个菜单，把窗口切换到另一台主机或本机。", "the host's name, a menu; `hostpick`"),
    ("settings button", "设置按钮", "The gear at the top bar's right, which opens the Console settings window.", "顶栏右端的齿轮，打开控制台设置窗口。", "the gear, the keyboard button; `keysbtn`"),
    ("side bar", "侧栏", "The left region, which shows one panel and can be folded away.", "左侧区域，显示一个面板，可以折叠。", "the column; 左栏, 侧栏, 左侧栏"),
    ("panel switcher", "面板切换条", "The strip of icons down the window's left edge.", "窗口左边缘的一竖条图标。", "a strip of icons, the column's tabs; `tabstrip`"),
    ("panel button", "面板按钮", "An icon of the panel switcher: it shows its panel, and the shown panel's button folds the side bar.", "面板切换条上的一个图标：显示对应的面板；再点当前面板的按钮则折叠侧栏。", "tab; `.tabstrip .tab`"),
    ("Status panel", "状态面板", "The host's readings, in cards.", "主机的各项读数，分成卡片。", "the Status tab; 状态页, “状态”标签页"),
    ("Files panel", "文件面板", "The host's file tree, with its copies strip.", "主机的文件树，带复制状态条。", "the Files tab; “文件”标签页"),
    ("Sessions panel", "会话面板", "The host's tmux and screen sessions.", "主机上的 tmux 和 screen 会话。", "the Sessions tab"),
    ("Jobs panel", "作业面板", "Your Slurm and PBS jobs, on a host that has a scheduler.", "你的 Slurm 和 PBS 作业（主机上有调度器时）。", "the Jobs tab"),
    ("Containers panel", "容器面板", "The Docker or Podman containers.", "Docker 或 Podman 容器。", "the Containers tab"),
    ("Ports panel", "端口面板", "The forwards, the direct connections and the host's own ports.", "端口转发、直连和主机自己的端口。", "the Ports tab"),
    ("side bar visit", "侧栏访问", "The keyboard focus taken into the side bar and back (⌘⇧L); a folded side bar comes out for it.", "把键盘焦点移进侧栏再移回来（⌘⇧L）；折叠的侧栏会为此临时展开。", "a visit, the keyboard to the column"),

    (S, "The Status panel", "状态面板", "", ""),
    ("summary card", "概况卡片", "The first card: the host's name, system, uptime, load, CPUs and logins.", "第一张卡片：主机名、系统、运行时间、负载、CPU 数和登录数。", "(no name)"),
    ("Usage card", "用量卡片", "CPU, memory and swap, each a gauge.", "CPU、内存和交换区，各一个用量条。", "Usage; 占用"),
    ("gauge", "用量条", "A label, a value and a bar that fills with it.", "一个名称、一个数值和一根随之填充的条。", "gauge, row"),
    ("interval picker", "刷新间隔", "How often the readings are taken.", "多久读取一次。", "the interval"),
    ("card picker", "卡片选择", "The eye on the Usage card: which cards are shown, and read.", "用量卡片上的眼睛：显示并读取哪些卡片。", "the eye"),
    ("reading", "读数", "What the Status panel reads from the host at each interval.", "状态面板每个间隔从主机读到的内容。", "reading"),
    ("busiest table", "最忙进程表", "The Processes card as a table of the busiest processes.", "进程卡片的表格形式：最忙的进程。", "the Processes table, the busiest"),
    ("process tree", "进程树", "The Processes card as the tree under the active tab.", "进程卡片的树形式：当前标签页下面的进程。", "the process tree"),
    ("signal menu", "信号菜单", "The context menu of a process: the signals to send it.", "进程的右键菜单：可以发给它的信号。", ""),

    (S, "Sessions, jobs, containers and ports", "会话、作业、容器与端口", "", ""),
    ("session row", "会话行", "A tmux or screen session in the Sessions panel, with its folder, terminal, rename and end buttons.", "会话面板里的一个 tmux 或 screen 会话，带文件夹、终端、重命名和结束按钮。", "row"),
    ("window chip", "窗口标记", "A tmux window of a session, listed on its row.", "会话行上列出的 tmux 窗口。", "the windows"),
    ("new-session dialog", "新建会话对话框", "Asks for a new session's name, directory and command.", "询问新会话的名称、目录和命令。", ""),
    ("job row", "作业行", "A job of the Jobs panel.", "作业面板里的一个作业。", ""),
    ("container row", "容器行", "A container of the Containers panel.", "容器面板里的一个容器。", ""),
    ("forward row", "转发行", "A forward, with whether it is up and its start, stop and forget buttons.", "一条端口转发：是否已建立，以及建立、停止、移除按钮。", ""),
    ("host port", "主机端口", "A port the host itself listens on, listed in the Ports panel.", "主机自己监听的端口，列在端口面板里。", "the host's own ports"),

    (S, "The tab area", "标签区", "", ""),
    ("tab area", "标签区", "The right region of the Console: one group, or several split apart.", "控制台右侧区域：一个分组，或分栏出来的几个。", "the terminals, terminals in tabs"),
    ("group", "分组", "A set of tabs with its own tab bar, showing one of them.", "一组标签页，有自己的标签栏，显示其中一个。", "group; 分组, 组"),
    ("tab bar", "标签栏", "The row of a group's tabs with the group buttons at its right.", "分组顶部那一行：各个标签页，右端是分组按钮。", "tab row; 标签行"),
    ("tab", "标签页", "One thing open in a group: a terminal or a preview.", "分组里打开的一项：终端或预览。", "tab; 标签, 标签页"),
    ("new-tab button", "新建按钮", "The + after the last tab: a new shell on the host.", "最后一个标签页后面的 +：在主机上新开一个 shell。", "the +"),
    ("group buttons", "分组按钮", "The buttons at a tab bar's right: the group's ready bell, the background button and the split button.", "标签栏右端的按钮：分组的就绪提醒、背景按钮和分栏按钮。", "the ends"),
    ("background button", "背景按钮", "Gives the group's terminals, or its previews, a dark or a light background.", "让分组的终端（或预览）用深色或浅色背景。", "the sun and moon, the light/dark button, the group's button"),
    ("split", "分栏", "A second group beside or under one, and the act of making it; the *split button* (分栏按钮) offers both ways.", "在一个分组旁边或下面再开一个分组，以及这个动作；“分栏按钮”提供两个方向。", "split"),
    ("divider", "分隔线", "The line between two groups, which drags.", "两个分组之间可以拖动的线。", "divider"),
    ("drop zone", "放置区", "Where a dragged tab would land, shown while it is dragged: a tab bar, a group's edge (a split) or its middle.", "拖动标签页时显示的落点：标签栏、分组的边缘（分栏）或中间。", ""),
    ("solo", "单独显示", "One group shown alone, as large as the tab area (⌘⇧↩), and back.", "一个分组单独占满标签区（⌘⇧↩），再按恢复。", "the group alone, shown alone; `zoom` in the code"),
    ("layout", "布局", "The groups, their splits and their tabs, kept per host and brought back with the window.", "各个分组、分栏和标签页的排布，按主机保存，窗口重开时恢复。", "the layout"),

    (S, "Tabs", "标签页", "", ""),
    ("terminal tab", "终端标签页", "A tab that is a terminal. By what runs in it: a *shell tab* (shell 标签页), a *session tab* (会话标签页), a *job tab* (作业标签页), a *container tab* (容器标签页).", "内容是终端的标签页。按运行的东西分：shell 标签页、会话标签页、作业标签页、容器标签页。", "terminal tab, shell tab"),
    ("preview tab", "预览标签页", "A tab that shows a file of the Files panel.", "显示文件面板里某个文件的标签页。", "a preview tab, the previews"),
    ("temporary tab", "临时标签页", "A tab opened by a single click, its name in italics: the next single click takes it over.", "单击打开的标签页，名字是斜体：下一次单击会顶替它。", "temporary tab"),
    ("kept tab", "固定标签页", "A tab that stays: opened by a double click, or a temporary tab double-clicked. To *keep* a tab is 固定.", "留下来的标签页：双击打开，或双击临时标签页。", "keeps the tab; 保留, 固定"),
    ("ended tab", "已结束的标签页", "A terminal tab whose command ended badly: it says so, and Return in it connects again.", "命令非正常结束的终端标签页：会显示原因，在里面按回车重新连接。", "(no name)"),
    ("tab mark", "标签标记", "The dot on a tab that is not shown: new output, the terminal bell or ready bell (orange), a command done (green) or failed (red).", "未显示的标签页上的圆点：有新输出、响过铃或就绪提醒（橙色）、命令完成（绿色）或失败（红色）。", "a dot, the mark"),
    ("host label", "主机标注", "The host's name at the start of a tab that runs on another host than the window shows.", "标签页开头的主机名，出现在运行于别的主机（不是窗口当前显示的主机）的标签页上。", ""),
    ("tab progress", "标签进度条", "The thin bar along a tab's foot while its program reports progress.", "程序报告进度时，标签页底部的细条。", "a thin bar along its tab's foot"),

    (S, "Inside a terminal", "终端内部", "", ""),
    ("terminal bell", "终端响铃", "The bell a program rings (BEL): a flash of the terminal's edge, a sound, or both.", "程序发出的响铃（BEL）：终端边缘闪一下、发声，或两者都有。", "the bell (also used for the ready bell)"),
    ("ready bell", "就绪提醒", "The bell button in a terminal's top right corner: switched on, it tells every time the terminal is ready for input again. A group's ready bell switches it for every tab of the group.", "终端右上角的铃铛按钮：打开后，每当终端再次可以输入时提醒你。分组的就绪提醒对该分组的所有标签页一起开关。", "the bell, bell button; 响铃"),
    ("lit group", "分组高亮", "The orange glow round a group in which a ready bell rang unseen.", "就绪提醒响过但没被看到时，分组周围的橙色光晕。", "the group lit"),
    ("password offer", "密码代填", "The small box in a terminal's top right corner that offers to type the host's stored password at a password prompt.", "终端右上角的小框：在密码提示处提出替你输入这台主机保存的密码。", "a small box, the offer"),
    ("paste question", "粘贴确认", "The strip that asks before several lines are pasted into a shell that would run them.", "粘贴多行到会直接执行它们的 shell 之前，先询问的那一条。", "a strip"),
    ("find strip", "查找条", "The search field with its count and arrows, in a terminal (⌘F) and in a preview alike.", "带计数和上下箭头的搜索框，终端（⌘F）和预览里都是它。", "a strip, the search strip"),
    ("terminal menu", "终端菜单", "The context menu of a terminal.", "终端的右键菜单。", ""),
    ("path link", "路径链接", "A path in a terminal's text: ⌘-click shows the file in the Files panel, ⌥⌘-click in a preview tab.", "终端文字里的路径：⌘ 点击在文件面板里显示该文件，⌥⌘ 点击在预览标签页里打开。", "Paths are links too"),
    ("shell integration", "Shell 集成", "The lines a shell started by the Console is given, which mark its prompts and report its directory.", "控制台启动的 shell 得到的几行脚本，用来标记提示符并报告当前目录。", "shell integration"),
    ("prompt mark", "提示符标记", "Where the shell integration says a prompt, a command or its output begins; what ⌘↑ and ⌘↓ jump between.", "Shell 集成报告的提示符、命令或输出的起点；⌘↑ 和 ⌘↓ 在它们之间跳转。", "the marks"),

    (S, "Features of the Console window", "控制台的窗口功能", "", ""),
    ("host wheel", "主机轮盘", "The ring of hosts drawn over the tab area while ⌘⇧ is held; letting go switches the window to the *chosen host* (选中的主机).", "按住 ⌘⇧ 时画在标签区上的一圈主机；松开后窗口切换到“选中的主机”。", "the host wheel, the mark; 主机转盘"),
    ("immersive mode", "沉浸模式", "The terminals alone: no top bar, side bar or tab bars (⌘⇧I).", "只留终端：没有顶栏、侧栏和标签栏（⌘⇧I）。", "immersive mode"),
    ("quick terminal", "快捷终端", "The terminal a key of its own drops from the top of the screen, in any application.", "在任何应用里按专用快捷键，从屏幕顶部滑下来的终端。", "quick terminal"),
    ("reopen", "恢复关闭的标签页", "Bring the tab closed last back (⌘⇧T).", "把最近关闭的标签页开回来（⌘⇧T）。", ""),

    (S, "The preview tab", "预览标签页", "", ""),
    ("preview header", "预览标题栏", "The line at a preview's top: the file's path, its size or page, and the preview's buttons.", "预览顶部的一行：文件路径、大小或页码，以及预览的按钮。", "the header"),
    ("source view", "原文视图", "A Markdown or HTML file shown as written instead of as the page it makes.", "把 Markdown 或 HTML 文件按原文显示，而不是渲染后的页面。", "as written"),
    ("fit", "适配", "A picture, video or PDF as wide as the tab (⌘0), or as tall.", "让图片、视频或 PDF 与标签页同宽（⌘0）或同高。", "fitted to"),
    ("two-page view", "双页视图", "A PDF's pages two by two.", "PDF 两页并排显示。", "the book button, two by two"),
    ("follow", "跟随", "A preview on screen reads its file again when the file changes.", "显示中的预览在文件变化时重新读取。", "followed"),

    (S, "The Console settings window", "控制台设置窗口", "", ""),
    ("page", "页", "One of the window's five parts, picked from the *page bar* (页栏) at its top: Terminal, Manners, Quick terminal, Theme, Keys.", "窗口的五个部分之一，从顶部的“页栏”选择：终端、行为、快捷终端、主题、快捷键。", "pages"),
    ("palette", "配色", "A set of terminal colours: a *built-in palette* (内置配色) or a *theme file* (主题文件). *Theme* names the page only.", "一套终端颜色：“内置配色”或“主题文件”。“主题”只是页的名字。", "theme, palette, colours"),
    ("swatch", "色样", "The small picture of a palette's colours beside its name.", "配色名字旁边的小色块图。", "swatch"),
    ("sample terminal", "示例终端", "The terminal on the Theme page drawn in the chosen palette.", "主题页上用所选配色画出的终端。", "a sample terminal"),
    ("colour well", "取色框", "The box that changes one colour of a theme file.", "修改主题文件里某个颜色的小框。", "a well"),
    ("key chip", "快捷键标签", "A key on the Keys page: a click, then the keys to use instead.", "快捷键页上的一个键：点击后按下要改用的按键。", "a chip"),
]


def page(lang):
    en = lang == "en"
    out = []
    if en:
        out += ["# Terms", "",
                "> **A proposal, not yet settled.** These are the names the documentation is to use for every part of the app. The interface, its Chinese translation and the manual still use the older names in the last column; they are to follow once the list is agreed.", "",
                "One thing has one name, and one name means one thing. Where a word had two uses, one of them got another word: *tab* is now only what the Console's right side holds, and what the left side shows is a *panel*.", ""]
    else:
        out += ["# 术语表", "",
                "> **这是提案，还没有定稿。** 文档里对应用各个部分的叫法以此为准。界面文字、中文翻译和手册目前还在用最后一列的旧叫法，术语表定下来之后再统一修改。", "",
                "一样东西只有一个名字，一个名字只指一样东西。原来一词两用的地方，其中一种用法换了词：“标签页”现在只指控制台右侧的东西，左侧显示的叫“面板”。", ""]
    for e in DATA:
        if e[0] == S:
            _, ten, tzh, den, dzh = e
            out += ["", f"## {ten if en else tzh}", ""]
            d = den if en else dzh
            if d:
                out += [d, ""]
            out += (["| Term | Chinese | What it is | Called before |", "|---|---|---|---|"] if en
                    else ["| 术语 | 英文 | 说明 | 以前的叫法 |", "|---|---|---|---|"])
            continue
        ten, tzh, den, dzh, old = e
        old = old.replace("|", "\\|")
        if en:
            out.append(f"| **{ten}** | {tzh} | {den} | {old} |")
        else:
            out.append(f"| **{tzh}** | {ten} | {dzh} | {old} |")
    return "\n".join(out) + "\n"


#: English term -> Chinese term, for whoever labels a picture.
TERMS = {e[0]: e[1] for e in DATA if e[0] is not S}

if __name__ == "__main__":
    here = os.path.dirname(os.path.abspath(__file__))
    for lang in ("en", "zh"):
        with open(os.path.join(here, lang, "src", "terms.md"), "w") as f:
            f.write(page(lang))
    print(len(TERMS), "terms")
