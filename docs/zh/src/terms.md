# 术语表

> **这是提案，还没有定稿。** 文档里对应用各个部分的叫法以此为准。界面文字、中文翻译和手册目前还在用最后一列的旧叫法，术语表定下来之后再统一修改。

一样东西只有一个名字，一个名字只指一样东西。原来一词两用的地方，其中一种用法换了词：“标签页”现在只指控制台右侧的东西，左侧显示的叫“面板”。


## 通用

在每个窗口里含义都相同的词。

| 术语 | 英文 | 说明 | 以前的叫法 |
|---|---|---|---|
| **本机** | this machine | 运行 Shellder 的这台电脑。界面上的标签是“本机”（英文界面为 *Local*）。 | Local, this machine, here |
| **主机** | host | ssh 配置里的一个 `Host` 条目。在窗口选择显示对象的地方（主机菜单、主机轮盘、主机选择器），本机也算一台主机，排在最前面，标签是“本机”。 |  |
| **主连接** | master | Shellder 为一台主机保持的 `ssh -M` 连接。 | master (left in English in the Chinese interface) |
| **套接字** | socket | 主连接的控制套接字（`ControlPath`），其他 ssh 命令复用它。 | socket (left in English) |
| **凭据** | credential | 为主机保存的密码、密钥口令或 TOTP 密钥。正文统一说“凭据”。 | secret, credential; 密钥 for secret, key and host key alike |
| **保险库** | vault | 保存全部凭据的那一个钥匙串条目。 | shellder vault |
| **主机密钥** | host key | 服务器的密钥，凭指纹确认。 |  |
| **提问** | question | ssh、sudo 或 polkit 在终端上问的问题：密码、验证码、主机密钥确认。 | prompt, question |
| **提示窗口** | prompt window | Shellder 把提问转给你回答的小窗口。不叫“对话框”。 | a dialog, the prompt window, a small window of its own |
| **对话框** | dialog | 窗口内部弹出的框，要一个值或一次确认，回答之前窗口不能操作。 | dialog (also used for the prompt window and the keychain's) |
| **浮动提示** | toast | 窗口角上自动消失的短消息。 | toast |
| **系统通知** | notification | 由系统通知中心显示的消息，不是窗口自己画的。 | a notification of the system's, a banner |
| **菜单** | menu | 从按钮打开的命令列表；右键打开的叫“右键菜单”。 | popup, menu, right-click menu |
| **卡片** | card | 带标题的圆角框，放关于同一件事的几行内容。 | card |
| **行** | row | 卡片、列表或表格里的一行。 |  |
| **分区** | section | 设置页或面板里带标题的一部分，标题叫“分区标题”。 | section, part, head, label |
| **面板** | panel | 窗口里显示一种内容、可以收起的区域：主窗口的日志面板、控制台侧栏的各个面板。 | tab, panel; 面板 also for tmux panes |
| **窗格** | pane | 并排的几个对等视图之一：文件窗口的两个窗格、tmux 的窗格。 | pane; 栏, 面板 |
| **复制** | copy | 把文件或文件夹从一台机器送到另一台，以及这个动作；⌘C 也是它。中文统一用“复制”。 | copy, transfer; 拷贝, 拷, 复制, 传输 |
| **创建副本** | duplicate | 在原物旁边再做一份（配色）。 | Duplicate; 复制 |
| **端口转发** | forward | 本机端口通向主机端口（`-L`），或反过来（`-R`），建在主连接上。 | forward; 隧道, 端口转发 |
| **直连** | direct connection | 按主机自己的地址直接打开它的一个端口，不经转发。 | direct connection; “no tunnel” |
| **移除** | forget | 把记住的一行（转发、直连、列出的端口）从列表里拿掉。“删除”用于真正消失的东西：文件、凭据、主题文件。 | Forget, Take off the list, Remove; 删除, 移除 |
| **键盘焦点** | keyboard focus | 键盘输入去往的地方。 | the keyboard; 焦点, 键盘, 键盘焦点 |
| **快捷键** | key | 执行某个动作的按键组合。 | key, keys; 快捷键, 按键, 键, 组合键 |

## 主窗口

左边是主机列表，右边是所选主机的主机页，下面是日志面板。

| 术语 | 英文 | 说明 | 以前的叫法 |
|---|---|---|---|
| **主窗口** | main window | 带主机列表的那个窗口。 |  |
| **主机列表** | host list | 左边一列：ssh 配置里的每台主机各占一行。 | host list, the list; `sidebar` in the code |
| **主机行** | host row | 主机列表里的一台主机：状态点、名字、连接开关，指针移上去时还有控制台按钮。 | the host's row, list entry |
| **状态点** | status dot | 表示连接状态的彩色圆点：灰色关闭、黄色连接中、绿色已连接、蓝色已连接并锁定、青色是别人的主连接、红色失败。 | the dot |
| **连接开关** | Connect switch | 主机的开关：打开是尝试连接一次，关闭则关掉主连接。 | the switch, Connect switch |
| **锁** | lock | 让主机保持连接的状态：锁定的主机断线后会重连。主机页上的“锁定按钮”负责上锁和解锁。 | the lock, lock button |
| **列表工具栏** | list toolbar | 主机列表下方的一排按钮：重新加载和编辑 ssh 配置、设置、更新、日志面板。 | the main window's bottom bar; `sidebar-bar` |
| **主机页** | host page | 主窗口右侧：所选主机的全部信息，分成几张卡片。 | the host's page, host page, per-host details; `detail` in the code |
| **连接卡片** | connection card | 主机页的第一张卡片：主机名、状态行、连接按钮、保活方式和重连规则。 | header card, its state |
| **状态行** | status line | 主机名下面用文字写出状态的那一行（“已连接”“已断开，5 秒后重试”）。 |  |
| **连接按钮** | connection buttons | 锁定、连接、断开、重新连接、立即重试、接管、关闭套接字，按当前状态显示其中几个。 | the master's controls |
| **保活方式** | keep-alive mode | 主连接在主机上运行什么来让服务器不断开它：什么都不运行（`-N`）、空闲 shell 或 `cat`。 | Keep-alive, keep-alive mode, the Keep-alive tool |
| **重连规则** | reconnect rule | 锁定的主机如何恢复连接：两次尝试之间等多久、何时放弃。“默认规则”在设置里；主机可以有“自定义规则”。 | Reconnect policy, Custom reconnect rule, Own rules; 重连策略, 重连规则 |
| **工具卡片** | Tools card | 控制台和文件两个按钮，下面是这台主机的端口转发和直连。 | Tools card |
| **凭据卡片** | Credentials card | 为这台主机保存的凭据，每项一行。 | the credentials |
| **配置卡片** | Configuration card | `ssh -G` 给出的这台主机的最终配置。 | Effective configuration (not named in the manual) |
| **日志面板** | log panel | 主窗口两栏下方的日志，用 ⌘L 打开和收起。 | log panel |
| **菜单栏图标** | menu bar icon | Shellder 在菜单栏上的图标；它的“菜单栏菜单”里每台主机有一个“主机子菜单”。 | menu bar item, menu bar icon, tray |

## 设置与提示窗口

| 术语 | 英文 | 说明 | 以前的叫法 |
|---|---|---|---|
| **设置窗口** | Settings window | 应用的设置。终端相关的设置在控制台自己的“控制台设置窗口”里。 | Settings (used for both) |
| **提示标题** | prompt head | 提示窗口的上部：要的是什么，以及是哪台主机或哪个操作在问。 |  |
| **提问原文** | question text | 提问方的原话，原样显示。 | the host's own words |
| **回答框** | answer field | 输入回答的地方。 |  |

## 文件窗口与文件树

文件树出现在两处：文件窗口的每个窗格里，以及控制台的文件面板里。

| 术语 | 英文 | 说明 | 以前的叫法 |
|---|---|---|---|
| **文件窗口** | Files window | 每台主机一个，两个窗格并排，拖动即复制。 | Files, the Files window |
| **窗格** | pane | 文件窗口的一半，显示一台主机的文件树。 | pane, tree; 栏 |
| **主机选择器** | host picker | 窗格左上角的按钮，选择这个窗格显示哪台主机或本机。 | (no name), “another machine”; `source` |
| **文件工具栏** | file toolbar | 文件树上方的按钮：前往文件夹、新建文件夹、新建文件、删除、隐藏文件、返回、上一级、刷新。 | the toolbar, the bar above |
| **文件树** | file tree | 一个文件夹里各项的列表，文件夹可以就地展开。 | tree, the pane, the list |
| **列标题** | column headings | 文件树上方的“名称”“修改日期”“大小”；点击按该列排序，“排序箭头”表示方向。 | header (in the code) |
| **文件行** | file row | 文件树里的一项。 | row |
| **展开箭头** | disclosure arrow | 文件夹左边的箭头，在树里展开或收起它。 | the chevron |
| **路径栏** | path bar | 文件树底部的当前路径，每一段都是按钮。 | (not in the manual) |
| **文件夹选择器** | folder picker | “前往文件夹…”和新建会话的文件夹按钮打开的小型文件夹树。 | a folder menu, a tree of the host's folders |
| **复制状态条** | copies strip | 文件树下方的一行：正在进行的复制及其进度、等待的个数、“清空”。 | Transfers; the strip (in the code) |
| **复制列表** | copies list | 这台主机的所有复制，从复制状态条展开。 | the list underneath, history |
| **重名提问** | name question | 复制落到已有同名项时问的对话框：“替换”“跳过”“应用于全部”。 | their questions |
| **继续** | continue | 从已写入的部分接着做一次中断的复制（▷）。 | continue, go on, resume; 从断点继续 |

## 控制台窗口

顶栏，左边一次显示一个面板的侧栏，右边是标签区。“标签页”这个词只用于右边。

| 术语 | 英文 | 说明 | 以前的叫法 |
|---|---|---|---|
| **控制台** | Console | 一台主机的窗口：各项读数和终端。 | Console; 监控台, Console |
| **顶栏** | top bar | 窗口顶部的横条；空白处可以拖动窗口。 | the bar, the top bar |
| **主机菜单** | host menu | 顶栏里的主机名：一个菜单，把窗口切换到另一台主机或本机。 | the host's name, a menu; `hostpick` |
| **设置按钮** | settings button | 顶栏右端的齿轮，打开控制台设置窗口。 | the gear, the keyboard button; `keysbtn` |
| **侧栏** | side bar | 左侧区域，显示一个面板，可以折叠。 | the column; 左栏, 侧栏, 左侧栏 |
| **面板切换条** | panel switcher | 窗口左边缘的一竖条图标。 | a strip of icons, the column's tabs; `tabstrip` |
| **面板按钮** | panel button | 面板切换条上的一个图标：显示对应的面板；再点当前面板的按钮则折叠侧栏。 | tab; `.tabstrip .tab` |
| **状态面板** | Status panel | 主机的各项读数，分成卡片。 | the Status tab; 状态页, “状态”标签页 |
| **文件面板** | Files panel | 主机的文件树，带复制状态条。 | the Files tab; “文件”标签页 |
| **会话面板** | Sessions panel | 主机上的 tmux 和 screen 会话。 | the Sessions tab |
| **作业面板** | Jobs panel | 你的 Slurm 和 PBS 作业（主机上有调度器时）。 | the Jobs tab |
| **容器面板** | Containers panel | Docker 或 Podman 容器。 | the Containers tab |
| **端口面板** | Ports panel | 端口转发、直连和主机自己的端口。 | the Ports tab |
| **侧栏访问** | side bar visit | 把键盘焦点移进侧栏再移回来（⌘⇧L）；折叠的侧栏会为此临时展开。 | a visit, the keyboard to the column |

## 状态面板

| 术语 | 英文 | 说明 | 以前的叫法 |
|---|---|---|---|
| **概况卡片** | summary card | 第一张卡片：主机名、系统、运行时间、负载、CPU 数和登录数。 | (no name) |
| **用量卡片** | Usage card | CPU、内存和交换区，各一个用量条。 | Usage; 占用 |
| **用量条** | gauge | 一个名称、一个数值和一根随之填充的条。 | gauge, row |
| **刷新间隔** | interval picker | 多久读取一次。 | the interval |
| **卡片选择** | card picker | 用量卡片上的眼睛：显示并读取哪些卡片。 | the eye |
| **读数** | reading | 状态面板每个间隔从主机读到的内容。 | reading |
| **最忙进程表** | busiest table | 进程卡片的表格形式：最忙的进程。 | the Processes table, the busiest |
| **进程树** | process tree | 进程卡片的树形式：当前标签页下面的进程。 | the process tree |
| **信号菜单** | signal menu | 进程的右键菜单：可以发给它的信号。 |  |

## 会话、作业、容器与端口

| 术语 | 英文 | 说明 | 以前的叫法 |
|---|---|---|---|
| **会话行** | session row | 会话面板里的一个 tmux 或 screen 会话，带文件夹、终端、重命名和结束按钮。 | row |
| **窗口标记** | window chip | 会话行上列出的 tmux 窗口。 | the windows |
| **新建会话对话框** | new-session dialog | 询问新会话的名称、目录和命令。 |  |
| **作业行** | job row | 作业面板里的一个作业。 |  |
| **容器行** | container row | 容器面板里的一个容器。 |  |
| **转发行** | forward row | 一条端口转发：是否已建立，以及建立、停止、移除按钮。 |  |
| **主机端口** | host port | 主机自己监听的端口，列在端口面板里。 | the host's own ports |

## 标签区

| 术语 | 英文 | 说明 | 以前的叫法 |
|---|---|---|---|
| **标签区** | tab area | 控制台右侧区域：一个分组，或分栏出来的几个。 | the terminals, terminals in tabs |
| **分组** | group | 一组标签页，有自己的标签栏，显示其中一个。 | group; 分组, 组 |
| **标签栏** | tab bar | 分组顶部那一行：各个标签页，右端是分组按钮。 | tab row; 标签行 |
| **标签页** | tab | 分组里打开的一项：终端或预览。 | tab; 标签, 标签页 |
| **新建按钮** | new-tab button | 最后一个标签页后面的 +：在主机上新开一个 shell。 | the + |
| **分组按钮** | group buttons | 标签栏右端的按钮：分组的就绪提醒、背景按钮和分栏按钮。 | the ends |
| **背景按钮** | background button | 让分组的终端（或预览）用深色或浅色背景。 | the sun and moon, the light/dark button, the group's button |
| **分栏** | split | 在一个分组旁边或下面再开一个分组，以及这个动作；“分栏按钮”提供两个方向。 | split |
| **分隔线** | divider | 两个分组之间可以拖动的线。 | divider |
| **放置区** | drop zone | 拖动标签页时显示的落点：标签栏、分组的边缘（分栏）或中间。 |  |
| **单独显示** | solo | 一个分组单独占满标签区（⌘⇧↩），再按恢复。 | the group alone, shown alone; `zoom` in the code |
| **布局** | layout | 各个分组、分栏和标签页的排布，按主机保存，窗口重开时恢复。 | the layout |

## 标签页

| 术语 | 英文 | 说明 | 以前的叫法 |
|---|---|---|---|
| **终端标签页** | terminal tab | 内容是终端的标签页。按运行的东西分：shell 标签页、会话标签页、作业标签页、容器标签页。 | terminal tab, shell tab |
| **预览标签页** | preview tab | 显示文件面板里某个文件的标签页。 | a preview tab, the previews |
| **临时标签页** | temporary tab | 单击打开的标签页，名字是斜体：下一次单击会顶替它。 | temporary tab |
| **固定标签页** | kept tab | 留下来的标签页：双击打开，或双击临时标签页。 | keeps the tab; 保留, 固定 |
| **已结束的标签页** | ended tab | 命令非正常结束的终端标签页：会显示原因，在里面按回车重新连接。 | (no name) |
| **标签标记** | tab mark | 未显示的标签页上的圆点：有新输出、响过铃或就绪提醒（橙色）、命令完成（绿色）或失败（红色）。 | a dot, the mark |
| **主机标注** | host label | 标签页开头的主机名，出现在运行于别的主机（不是窗口当前显示的主机）的标签页上。 |  |
| **标签进度条** | tab progress | 程序报告进度时，标签页底部的细条。 | a thin bar along its tab's foot |

## 终端内部

| 术语 | 英文 | 说明 | 以前的叫法 |
|---|---|---|---|
| **终端响铃** | terminal bell | 程序发出的响铃（BEL）：终端边缘闪一下、发声，或两者都有。 | the bell (also used for the ready bell) |
| **就绪提醒** | ready bell | 终端右上角的铃铛按钮：打开后，每当终端再次可以输入时提醒你。分组的就绪提醒对该分组的所有标签页一起开关。 | the bell, bell button; 响铃 |
| **分组高亮** | lit group | 就绪提醒响过但没被看到时，分组周围的橙色光晕。 | the group lit |
| **密码代填** | password offer | 终端右上角的小框：在密码提示处提出替你输入这台主机保存的密码。 | a small box, the offer |
| **粘贴确认** | paste question | 粘贴多行到会直接执行它们的 shell 之前，先询问的那一条。 | a strip |
| **查找条** | find strip | 带计数和上下箭头的搜索框，终端（⌘F）和预览里都是它。 | a strip, the search strip |
| **终端菜单** | terminal menu | 终端的右键菜单。 |  |
| **路径链接** | path link | 终端文字里的路径：⌘ 点击在文件面板里显示该文件，⌥⌘ 点击在预览标签页里打开。 | Paths are links too |
| **Shell 集成** | shell integration | 控制台启动的 shell 得到的几行脚本，用来标记提示符并报告当前目录。 | shell integration |
| **提示符标记** | prompt mark | Shell 集成报告的提示符、命令或输出的起点；⌘↑ 和 ⌘↓ 在它们之间跳转。 | the marks |

## 控制台的窗口功能

| 术语 | 英文 | 说明 | 以前的叫法 |
|---|---|---|---|
| **主机轮盘** | host wheel | 按住 ⌘⇧ 或同时按住鼠标左右两键时画在标签区上的一圈主机；松开后窗口切换到“选中的主机”。 | the host wheel, the mark; 主机转盘 |
| **沉浸模式** | immersive mode | 只留终端：没有顶栏、侧栏和标签栏（⌘⇧I）。 | immersive mode |
| **快捷终端** | quick terminal | 在任何应用里按专用快捷键，从屏幕顶部滑下来的终端。 | quick terminal |
| **恢复关闭的标签页** | reopen | 把最近关闭的标签页开回来（⌘⇧T）。 |  |

## 预览标签页

| 术语 | 英文 | 说明 | 以前的叫法 |
|---|---|---|---|
| **预览标题栏** | preview header | 预览顶部的一行：文件路径、大小或页码，以及预览的按钮。 | the header |
| **原文视图** | source view | 把 Markdown 或 HTML 文件按原文显示，而不是渲染后的页面。 | as written |
| **适配** | fit | 让图片、视频或 PDF 与标签页同宽（⌘0）或同高。 | fitted to |
| **双页视图** | two-page view | PDF 两页并排显示。 | the book button, two by two |
| **跟随** | follow | 显示中的预览在文件变化时重新读取。 | followed |

## 控制台设置窗口

| 术语 | 英文 | 说明 | 以前的叫法 |
|---|---|---|---|
| **页** | page | 窗口的五个部分之一，从顶部的“页栏”选择：终端、行为、快捷终端、主题、快捷键。 | pages |
| **配色** | palette | 一套终端颜色：“内置配色”或“主题文件”。“主题”只是页的名字。 | theme, palette, colours |
| **色样** | swatch | 配色名字旁边的小色块图。 | swatch |
| **示例终端** | sample terminal | 主题页上用所选配色画出的终端。 | a sample terminal |
| **取色框** | colour well | 修改主题文件里某个颜色的小框。 | a well |
| **快捷键标签** | key chip | 快捷键页上的一个键：点击后按下要改用的按键。 | a chip |
