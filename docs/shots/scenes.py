"""The documentation's pictures: what each shows and which parts it names.

A scene: `name` (the picture's and the figure's file name), `page` of
ui/ with its `query` and the window `label` it is drawn as, `size` in
points, `before` (a script run before the page's own, for what it reads
as it starts), `setup` (a script run in the page once it is drawn, with
the tools of stub.js: `$`, `$$`, `withText`, `titled`, `click`, `press`,
`twice`, `key`, `sleep`, `emit`, `state`), `state` (fields of the app's
state the scene has otherwise), `terminals` (what its terminals say,
by a word of their command), `crop` (an expression for the part of the
page the picture is of; the whole page without one), `alt` (English,
Chinese) and `parts`: (the term of docs/terms.py, an expression for its
element or elements, where its pin sits on its box: t/m/b and l/c/r,
"tl" when not said). A part with no term of its own has a fourth
member, its name and line in both languages, and any key in place of
the term.
"""
import time

ESC = "\x1b"
NOW = time.time()


def status(host, kind="up", locked=False, needed_by=(), since=3600, **more):
    state = {"kind": kind, **more}
    if kind == "up":
        state["pid"] = 4242
    return {
        "host": host, "enabled": kind != "off", "locked": locked, "state": state,
        "since": int(NOW) - since if kind == "up" else None, "lastError": None,
        "quickFailures": 0, "idleMode": "none", "neededBy": list(needed_by), "socket": True,
    }


def copy(n, name, state, progress=None, total=None, upload=False, resume=False, ended=True):
    host, here = {"kind": "host", "host": "gpu-box"}, {"kind": "local"}
    return {
        "id": n, "name": name, "isUpload": upload, "canResume": resume,
        "job": {
            "from": here if upload else host, "to": host if upload else here,
            "source": ("/Users/alice/Documents/" if upload else "/home/alice/project/data/") + name,
            "destination": "/home/alice/project" if upload else "/Users/alice/Documents", "resume": False,
        },
        "state": state, "progress": progress, "total": total,
        "queuedAt": NOW - 600 + n * 60, "endedAt": NOW - 500 + n * 60 if ended else None,
    }


PROMPT = f"{ESC}[1;32malice@gpu-box{ESC}[0m:{ESC}[1;34m~/project{ESC}[0m$ "


def dress(doc):
    """What the test's data lacks to look like an app in use: masters
    that are up, stored credentials, a log, copies, what a terminal
    says."""
    s = doc["state"]
    s["statuses"] = {
        "bastion": status("bastion", needed_by=["web-1"], since=93600),
        "web-1": status("web-1", locked=True, since=93500),
        "gpu-box": status("gpu-box", locked=True, since=18200),
        "db-staging": status("db-staging", "waiting", locked=True, retryIn=20),
        "backup": status("backup", "off"),
    }
    s["enabled"] = ["bastion", "web-1", "gpu-box", "db-staging"]
    s["locked"] = ["web-1", "gpu-box", "db-staging"]
    s["connected"] = ["bastion", "web-1", "gpu-box"]
    s["secrets"]["gpu-box"] = ["password", "totp"]
    s["secrets"]["web-1"] = ["passphrase"]
    s["settings"]["autostartOtherCopy"] = None
    s["settings"]["logPanelHeight"] = 170
    s["update"] = {"state": "upToDate", "version": None, "page": None, "notes": None, "progress": None, "message": None}
    doc["log"] = [
        "2026-11-23 09:12:04 INFO [731] daemon starting",
        "2026-11-23 09:12:05 INFO [731] bastion: master established",
        "2026-11-23 09:12:07 INFO [731] web-1: master established",
        "2026-11-23 09:12:09 PROMPT [731] gpu-box: password asked, answered from the keychain",
        "2026-11-23 09:12:10 PROMPT [731] gpu-box: verification code asked, answered with a TOTP code",
        "2026-11-23 09:12:11 INFO [731] gpu-box: master established",
        "2026-11-23 14:02:40 WARNING [731] db-staging: master exited: Connection closed by 10.0.3.40 port 22",
        "2026-11-23 14:02:40 INFO [731] db-staging: retrying in 20s",
    ]
    doc["shell"] = (
        f"{ESC}]0;alice@gpu-box: ~/project{ESC}\\"
        "Welcome to Ubuntu 24.04.1 LTS (GNU/Linux 6.8.0-45-generic x86_64)\r\n\r\n"
        f"{PROMPT}python train.py --epochs 3\r\n"
        "epoch 1/3  loss 0.412  acc 0.871  00:41\r\n"
        "epoch 2/3  loss 0.287  acc 0.913  00:40\r\n"
        "epoch 3/3  loss 0.214  acc 0.936  00:41\r\n"
        f"{ESC}[32msaved{ESC}[0m checkpoints/run-42.pt\r\n"
        f"{PROMPT}"
    )
    doc["preview"] = (
        "# Run 42\n\nThe model was trained for three epochs on the full set.\n\n"
        "| epoch | loss | accuracy |\n|---|---|---|\n| 1 | 0.412 | 0.871 |\n| 2 | 0.287 | 0.913 |\n| 3 | 0.214 | 0.936 |\n\n"
        "## To do\n\n- compare with run 41\n- write the report\n\n```python\nmodel = Model.load('checkpoints/run-42.pt')\n```\n"
    )
    doc["tree"] = None
    doc["forwards"] = [{"direction": "local", "local": 8888, "remote": 8888}]
    doc["reading"]["cpuPercent"] = 37.0
    # The Files views: the host's tree on the left and in the Console's
    # panel, this machine's on the right, and a few copies.
    f = doc["files"]
    f["hosts"] = ["bastion", "web-1", "gpu-box"]
    for pane in ("left", "tab"):
        f["panes"][pane]["source"] = {"kind": "host", "host": "gpu-box"}
        f["panes"][pane]["title"] = "gpu-box"
        f["panes"][pane]["shortcuts"] = [{"title": "alice", "path": "/home/alice"}, {"title": "/", "path": "/"}]
        f["panes"][pane]["home"] = "/home/alice"
    right = f["panes"]["right"]
    right["root"] = "/Users/alice/Documents"
    right["rows"] = [
        dict(r, path=r["path"].replace("/home/alice", "/Users/alice/Documents"), expanded=False)
        for r in right["rows"] if r["depth"] == 0
    ]
    f["canCopy"] = {"left": True, "right": False, "tab": False}
    running = copy(3, "results.csv", {"kind": "running"}, progress=0.62, total=652000, ended=False)
    f["transfers"] = [
        copy(1, "report.pdf", {"kind": "done"}, total=1840000),
        copy(2, "run-42.log", {"kind": "failed", "why": "Connection reset by peer"}, total=96000, resume=True),
        running,
        copy(4, "figure.png", {"kind": "waiting"}, upload=True, ended=False),
    ]
    f["active"] = running
    f["queued"] = 1
    f["canRetry"] = [2]
    f["showHistory"] = True


def own(en, en_line, zh, zh_line):
    return {"en": (en, en_line), "zh": (zh, zh_line)}


CONSOLE_PAGE = {"page": "console.html", "query": "host=gpu-box", "label": "console-gpu-box"}
SIDE = "[$('#tabs'), $('.side')]"
SPLIT = """
    press($('.splitbtn'));
    await sleep(300);
    press($('.menu .item'));
    await sleep(900);
"""

MAIN = {
    "name": "main-window",
    "page": "index.html",
    "size": (900, 820),
    "alt": ("The main window", "主窗口"),
    "setup": """
        click($('.host-row[data-alias="gpu-box"]'));
        await sleep(300);
        press($$('#sidebar-bar button').pop());
        await sleep(600);
    """,
    "parts": [
        ("host list", "$('.sidebar')", "mr"),
        ("host row", "$('.host-row[data-alias=\"web-1\"]')", "ml"),
        ("status dot", "$('.host-row[data-alias=\"bastion\"] .dot')", "tl"),
        ("Connect switch", "$('.host-row[data-alias=\"bastion\"] .switch')", "tr"),
        ("list toolbar", "$('#sidebar-bar')", "tl"),
        ("host page", "$('.detail')", "tr"),
        ("connection card", "$('#detail .card')", "tl"),
        ("status line", "$('#detail .status-text')", "mr"),
        ("connection buttons", "$('#detail .controls')", "bl"),
        ("Tools card", "titled('#detail', 0)", "tl"),
        ("Credentials card", "titled('#detail', 1)", "tl"),
        ("Configuration card", "titled('#detail', 2)", "tl"),
        ("log panel", "$('#log-area')", "tl"),
    ],
}

SETTINGS = {
    "name": "settings",
    "page": "settings.html",
    "label": "settings",
    "size": (560, 1180),
    "alt": ("The Settings window", "设置窗口"),
    "parts": [
        ("startup", "titled('#form', 0)", "tl", own("Startup", "Start at login, and silently.", "启动", "登录时启动，以及是否静默启动。")),
        ("background", "titled('#form', 1)", "tl", own("Background", "Keep running with no window, and the menu bar icon.", "后台", "没有窗口时继续运行，以及菜单栏图标。")),
        ("appearance", "titled('#form', 2)", "tl", own("Appearance", "The interface's language.", "外观", "界面语言。")),
        ("keychain", "titled('#form', 3)", "tl", own("Keychain", "Whether the vault can be read.", "钥匙串", "保险库是否可以读取。")),
        ("reconnect", "titled('#form', 4)", "tl", own("Reconnect", "The default reconnect rule, and the hosts with a rule of their own.", "重连", "默认重连规则，以及有自定义规则的主机。")),
        ("files", "titled('#form', 5)", "tl", own("Files", "Where the ssh config, the log and the state directory are.", "文件", "ssh 配置、日志和状态目录的位置。")),
        ("update", "titled('#form', 6)", "tl", own("Software update", "The version, and the updates.", "软件更新", "当前版本与更新。")),
    ],
}

PROMPT_WINDOW = {
    "name": "prompt-window",
    "page": "prompt.html",
    "label": "prompt",
    "size": (460, 290),
    "alt": ("The prompt window", "提示窗口"),
    "state": {
        "prompt": {"id": 1, "host": "gpu-box", "kind": "password", "prompt": "alice@gpu-box.example.org's password:", "allowSave": True, "about": None},
        "pendingPrompts": 2,
    },
    "parts": [
        ("prompt head", "$('.prompt .head')", "tl"),
        ("queue", "$('.prompt .more')", "mr", own("queue counter", "How many more questions wait behind this one.", "排队计数", "这个提问后面还有几个在等。")),
        ("question text", "$('.prompt .text')", "tl"),
        ("answer field", "$('.prompt input[type=password], .prompt input[type=text]')", "tl"),
        ("save", "$('.prompt label.row')", "tl", own("save checkbox", "Keep the answer as the host's credential.", "保存选项", "把回答保存为这台主机的凭据。")),
        ("buttons", "$('.prompt .buttons')", "tr", own("buttons", "Cancel, which ends the attempt, or answer.", "按钮", "取消（结束这次尝试）或回答。")),
    ],
}

CONSOLE = {
    "name": "console",
    **CONSOLE_PAGE,
    "size": (1240, 760),
    "alt": ("A host's Console window", "一台主机的控制台窗口"),
    "setup": "await sleep(600);" + SPLIT,
    "parts": [
        ("top bar", "$('#topbar')", "tc"),
        ("host menu", "$('.hostpick')", "br"),
        ("settings button", "$('.keysbtn')", "bl"),
        ("panel switcher", "$('#tabs')", "ml"),
        ("panel button", "$$('#tabs .tab')[1]", "mr"),
        ("side bar", "$('.side')", "bl"),
        ("Status panel", "$('#side-body')", "br"),
        ("card", "$$('#side-body .card')[1]", "tl"),
        ("tab area", "$('#term')", "bc"),
        ("group", "$$('.group')[1]", "mc"),
        ("tab bar", "$$('.termtabs')[1]", "bc"),
        ("tab", "$('.ttab')", "bc"),
        ("divider", "$('.divider')", "mc"),
    ],
}

STATUS = {
    "name": "console-status",
    **CONSOLE_PAGE,
    "size": (1240, 2500),
    "crop": SIDE,
    "alt": ("The Status panel", "状态面板"),
    "setup": "await sleep(800);",
    "parts": [
        ("summary card", "$('#side-body .card')", "tl"),
        ("Usage card", "titled('#side-body', 0)", "tl"),
        ("interval picker", "$('#side-body .card-title select')", "bl"),
        ("card picker", "$$('#side-body .card-title')[0].querySelectorAll('button')[0]", "bc"),
        ("gauge", "$$('#side-body .gauge')[1]", "ml"),
        ("gpus", "titled('#side-body', 1)", "tl", own("GPUs card", "Each GPU's load and memory.", "GPU 卡片", "每块 GPU 的负载和显存。")),
        ("disks", "titled('#side-body', 2)", "tl", own("Disks card", "The file systems, and a removable drive's mount, unmount and eject buttons.", "磁盘卡片", "各个文件系统，以及可移动驱动器的挂载、卸载和弹出按钮。")),
        ("network", "titled('#side-body', 3)", "tl", own("Network card", "The interfaces with their addresses and traffic.", "网络卡片", "各个网络接口及其地址和流量。")),
        ("processes", "titled('#side-body', 4)", "tl", own("Processes card", "The busiest table, or the process tree.", "进程卡片", "最忙进程表，或进程树。")),
        ("busiest table", "$('#side-body table.procs')", "tr"),
        ("logins", "titled('#side-body', 5)", "tl", own("Logged in card", "Who is logged in, and from where.", "已登录卡片", "谁登录着，从哪里登录。")),
    ],
}

SESSIONS = {
    "name": "console-sessions",
    **CONSOLE_PAGE,
    "size": (1240, 620),
    "crop": SIDE,
    "alt": ("The Sessions panel", "会话面板"),
    "setup": "await sleep(500); press($$('#tabs .tab')[2]); await sleep(700);",
    "parts": [
        ("panel button", "$('#tabs .tab.active')", "mr"),
        ("section", "$('#side-body .sect')", "tl"),
        ("new", "$('#side-body .sect button')", "bl", own("new session", "The + of a section: a new tmux or screen session.", "新建会话", "分区标题上的 +：新建一个 tmux 或 screen 会话。")),
        ("session row", "$('#side-body .session')", "tl"),
        ("window chip", "$('#side-body .windows .win')", "bl"),
        ("row buttons", "$$('#side-body .session')[0].querySelectorAll('button.act')", "bl", own("row buttons", "Show the session's directory in the Files panel, open a shell there, rename the session, end it.", "行按钮", "在文件面板里显示会话的目录、在那里开一个 shell、重命名会话、结束会话。")),
    ],
}

PORTS = {
    "name": "console-ports",
    **CONSOLE_PAGE,
    "size": (1240, 620),
    "crop": SIDE,
    "alt": ("The Ports panel", "端口面板"),
    "before": """
        localStorage.setItem('console-forwards-gpu-box', JSON.stringify([{ direction: 'local', local: 8888, remote: 8888 }, { direction: 'remote', local: 3000, remote: 9000 }]));
        localStorage.setItem('console-direct-gpu-box', JSON.stringify([8080]));
    """,
    "setup": "await sleep(500); press($$('#tabs .tab').pop()); await sleep(900);",
    "parts": [
        ("section", "$('#side-body .sect')", "tl"),
        ("forward row", "$('#side-body .session')", "tl"),
        ("direct connection", "$$('#side-body .sect')[1].nextElementSibling", "tl"),
        ("host port", "$$('#side-body .sect')[2].nextElementSibling", "tl"),
    ],
}

FILES_PANEL = {
    "name": "console-files",
    **CONSOLE_PAGE,
    "size": (1240, 700),
    "alt": ("The Files panel, and a file in a preview tab", "文件面板，以及预览标签页里的一个文件"),    "setup": """
        await sleep(500);
        press($$('#tabs .tab')[1]);
        await sleep(900);
        click($('.filesbox tr[data-path$="README.md"]'));
        await sleep(1500);
    """,
    "parts": [
        ("Files panel", "$('#side-body')", "tr"),
        ("file toolbar", "$('.filesbox .top')", "bl"),
        ("column headings", "$('.filesbox thead')", "tr"),
        ("file row", "$('.filesbox tr[data-path$=\"train.py\"]')", "ml"),
        ("disclosure arrow", "$('.filesbox .disclosure')", "ml"),
        ("path bar", "$('.filesbox .pathbar')", "tl"),
        ("copies strip", "$('.filesbox .strip')", "tl"),
        ("copies list", "$('.filesbox .history')", "tr"),
        ("preview tab", "$('.ttab.temporary') || $$('.ttab').pop()", "bl"),
        ("temporary tab", "$('.ttab.temporary .label')", "br"),
        ("preview header", "$('.termbox.preview .top')", "bc"),
    ],
}

PREVIEW = {
    "name": "preview",
    **CONSOLE_PAGE,
    "size": (1240, 620),
    "crop": "$$('.group')[0]",
    "alt": ("A Markdown file in a preview tab", "预览标签页里的一个 Markdown 文件"),
    "setup": FILES_PANEL["setup"] + """
        press($('.termbox.preview .top button[title*="⌘F"]'));
        await sleep(500);
    """,
    "parts": [
        ("preview tab", "$('.ttab.temporary') || $$('.ttab').pop()", "bl"),
        ("preview header", "$('.termbox.preview .top')", "bl"),
        ("path", "$('.termbox.preview .top .path')", "bc", own("path", "The file's path, and beside it its size or page.", "路径", "文件的路径，旁边是大小或页码。")),
        ("buttons", "$$('.termbox.preview .top button')", "bc", own("preview buttons", "Find, the source view, the size, read again, download; which ones depends on the kind of file.", "预览按钮", "查找、原文视图、缩放、重新读取、下载；具体有哪些取决于文件类型。")),
        ("find strip", "$('.termbox.preview .search')", "bl"),
    ],
}

TABS = {
    "name": "console-tabs",
    **CONSOLE_PAGE,
    "size": (1720, 520),
    "crop": "$('#term')",
    "alt": ("The tab area: two groups", "标签区：两个分组"),
    "terminals": [{"match": "tmux", "text": f"{PROMPT}htop\r\n"}],
    "setup": """
        await sleep(500);
        press($$('#tabs .tab')[2]);
        await sleep(700);
        twice($$('#side-body .session')[0]);
        await sleep(600);
        click($$('#side-body .session')[1]);
        await sleep(600);
        press($('.termtabs .add'));
        await sleep(600);
    """ + SPLIT + """
        emit('term-data', { id: 1, data: bytes('more output\\r\\n') });
        await sleep(400);
    """,
    "parts": [
        ("group", "$$('.group')[0]", "bc"),
        ("tab bar", "$$('.termtabs')[0]", "bl"),
        ("tab", "$$('.ttab')[0]", "tl"),
        ("tab mark", "$('.ttab .mark')", "bc"),
        ("kept tab", "$$('.ttab')[1]", "bl"),
        ("temporary tab", "$('.ttab.temporary')", "bl"),
        ("new-tab button", "$('.termtabs .add')", "bc"),
        ("group buttons", "$('.termtabs .ends')", "bl"),
        ("background button", "$('.theme-toggle')", "bc"),
        ("split", "$('.splitbtn')", "br"),
        ("ready bell", "$$('.bellbtn').find(b => b.getBoundingClientRect().width)", "bl"),
        ("divider", "$('.divider')", "mc"),
    ],
}

TERMINALS = {
    "name": "console-terminals",
    **CONSOLE_PAGE,
    "size": (1240, 460),
    "crop": "$('#term')",
    "alt": ("Two terminals: a password offer and the find strip", "两个终端：密码代填和查找条"),
    "setup": "await sleep(700);" + SPLIT + """
        emit('term-data', { id: 1, data: bytes('sudo systemctl restart nginx\\r\\n[sudo] password for alice: ') });
        await sleep(2500);
        key('f', { metaKey: true }, $$('.group')[1].querySelector('.xterm-helper-textarea'));
        await sleep(600);
    """,
    "settle": 800,
    "parts": [
        ("terminal tab", "$('.ttab')", "bl"),
        ("password offer", "$('.offer')", "bl"),
        ("find strip", "$('.termbox .search')", "bl"),
        ("group ready bell", "$('.group-bell')", "bc", own("the group's ready bell", "Switches the ready bell of every tab of the group.", "分组的就绪提醒", "一起开关这个分组所有标签页的就绪提醒。")),
    ],
}

WHEEL = {
    "name": "host-wheel",
    **CONSOLE_PAGE,
    "size": (1240, 700),
    "alt": ("The host wheel over the tab area", "标签区上的主机轮盘"),
    "setup": """
        await sleep(700);
        key('Meta', { metaKey: true, shiftKey: true });
        await sleep(1500);
        key('ArrowRight', { metaKey: true, shiftKey: true });
        await sleep(500);
    """,
    "parts": [
        ("host wheel", "$('.wheel')", "tl"),
        ("chosen host", "$('.wheel .slice.marked') || $('.wheel .marked')", "mc", own("chosen host", "The host the window switches to when ⌘ is let go.", "选中的主机", "松开 ⌘ 时窗口切换过去的主机。")),
        ("centre", "$('.wheel .centre')", "bc", own("centre", "The chosen host's name and state.", "中心", "选中主机的名字和状态。")),
    ],
}

FILES_WINDOW = {
    "name": "files-window",
    "page": "files.html",
    "query": "host=gpu-box",
    "label": "files-gpu-box",
    "size": (1100, 640),
    "alt": ("The Files window", "文件窗口"),
    "setup": "await sleep(900);",
    "parts": [
        ("pane", "$$('.pane')[1]", "mc"),
        ("host picker", "$('.pane button.source')", "bl"),
        ("file toolbar", "$('.pane .top')", "bc"),
        ("column headings", "$('.pane thead')", "tr"),
        ("file tree", "$('.pane .tree')", "mc"),
        ("file row", "$('.pane tr[data-path$=\"train.py\"]')", "ml"),
        ("path bar", "$('.pane .pathbar')", "tl"),
        ("copies strip", "$('.transfers .strip')", "tl"),
        ("copies list", "$('.transfers .history')", "tr"),
    ],
}

CONSOLE_SETTINGS = {
    "name": "console-settings",
    "page": "consoleprefs.html",
    "label": "termprefs",
    "size": (780, 660),
    "alt": ("The Console settings window, its Theme page", "控制台设置窗口的主题页"),
    "setup": "await sleep(500); press($$('#pages button')[3]); await sleep(900);",
    "parts": [
        ("pages", "$('#pages')", "bl", own("page bar", "Picks one of the five pages.", "页栏", "在五个页之间切换。")),
        ("page", "$('#pages button.active')", "br"),
        ("side", "$('.tl .seg')", "bl", own("light or dark", "Which terminals the palette is being chosen for.", "浅色或深色", "正在为哪一种终端选择配色。")),
        ("palette", "$('.tl-item.active')", "ml"),
        ("swatch", "$('.tl-item .sw')", "bc"),
        ("sample terminal", "$('.td-sample')", "tl"),
        ("colour well", "$('.td-colours input[type=color]')", "ml"),
    ],
}

SCENES = [MAIN, SETTINGS, PROMPT_WINDOW, FILES_WINDOW, CONSOLE, STATUS, SESSIONS, PORTS, FILES_PANEL, PREVIEW, TABS, TERMINALS, WHEEL, CONSOLE_SETTINGS]
