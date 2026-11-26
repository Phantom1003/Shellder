// The app behind the front page's windows. The pages of ui/ run there
// as they are, in frames, and what they would ask the Rust side is
// answered here instead (shim.js hands the calls over): made-up hosts
// with a switch that connects, readings that move, a file tree, copies,
// and a shell that knows a handful of commands. It starts from data.js,
// which docs/shots/shoot.py --demo writes from what the app's own code
// says of those hosts. Nothing leaves the page: there is no host.
(() => {
  'use strict';
  if (window.__shellderBackend) return;
  const D = window.__SHELLDER_DEMO;
  const LOCAL = 'this machine';   // ui/shellder.js's LOCAL
  const USER = 'alice';
  const now = () => Date.now() / 1000;
  const clone = v => JSON.parse(JSON.stringify(v));
  const rnd = (a, b) => a + Math.random() * (b - a);
  const clamp = (v, a, b) => Math.min(b, Math.max(a, v));
  const wander = (v, step, a, b) => clamp(v + rnd(-step, step), a, b);
  const pad = (n, w = 2) => String(n).padStart(w, '0');
  const bytesOf = text => new TextEncoder().encode(text);
  const b64 = text => { let s = ''; for (const b of bytesOf(text)) s += String.fromCharCode(b); return btoa(s); };

  // MARK: the windows

  const wins = new Map();   // label -> what shim.js gave: emit(name, payload)
  // An event reaches a page later, as one of the app's does: never
  // inside the call that caused it (a page that is told of its own
  // click while it handles the mouse coming down redraws under it).
  // What it carries is as it was when it was sent.
  const send = (to, name, payload) => { const text = JSON.stringify(payload === undefined ? null : payload); setTimeout(() => { for (const w of to()) w.emit(name, JSON.parse(text)); }); };
  const emitAll = (name, payload) => send(() => [...wins.values()], name, payload);
  const emitTo = (label, name, payload) => send(() => wins.has(label) ? [wins.get(label)] : [], name, payload);

  // MARK: the hosts and their masters

  const state = D.state;
  const moved = now() - D.now;
  for (const s of Object.values(state.statuses)) if (s.since) s.since += moved;
  const push = () => emitAll('state', state);
  const setIn = (list, host, on) => { const i = list.indexOf(host); if (on && i < 0) list.push(host); if (!on && i >= 0) list.splice(i, 1); };

  const stamp = d => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
  const lineAt = l => Date.parse(l.slice(0, 19).replace(' ', 'T'));
  // The log's lines end five minutes ago, as far apart as they were.
  const logMoved = Date.now() - 5 * 60e3 - lineAt(D.log[D.log.length - 1]);
  const log = D.log.map(l => stamp(new Date(lineAt(l) + logMoved)) + l.slice(19));
  const say = (level, text) => { log.push(`${stamp(new Date())} ${level} [731] ${text}`); };

  // db-staging is the host that never comes up: its server closes the
  // connection, and a locked host is tried again after a wait that doubles.
  const FAILS = { 'db-staging': 'Connection closed by 10.0.3.40 port 22' };
  const timers = {};
  const later = (host, ms, f) => { clearTimeout(timers[host]); timers[host] = setTimeout(f, ms); };

  function connect(host) {
    const st = state.statuses[host];
    if (!st) return;
    st.enabled = true; st.state = { kind: 'connecting' }; st.lastError = null;
    setIn(state.enabled, host, true);
    say('INFO', `${host}: switched on, connecting`);
    push();
    later(host, rnd(900, 1500), () => attempt(host, 20));
  }
  function attempt(host, wait) {
    const st = state.statuses[host];
    const kinds = state.secrets[host] || [];
    if (kinds.includes('password')) say('PROMPT', `${host}: password asked, answered from the keychain`);
    if (kinds.includes('totp')) say('PROMPT', `${host}: verification code asked, answered with a TOTP code`);
    if (FAILS[host]) {
      say('WARNING', `${host}: master exited: ${FAILS[host]}`);
      if (st.locked) {
        st.state = { kind: 'waiting', retryIn: wait };
        say('INFO', `${host}: retrying in ${wait}s`);
        later(host, wait * 1000, () => { st.state = { kind: 'connecting' }; push(); later(host, 1200, () => attempt(host, Math.min(wait * 2, 600))); });
      } else {
        st.state = { kind: 'off' }; st.enabled = false; st.lastError = FAILS[host];
        setIn(state.enabled, host, false);
      }
      return push();
    }
    st.state = { kind: 'up', pid: Math.floor(rnd(3000, 9000)) }; st.since = Math.floor(now());
    setIn(state.connected, host, true);
    say('INFO', `${host}: master established`);
    push();
  }
  function disconnect(host) {
    const st = state.statuses[host];
    if (!st) return;
    clearTimeout(timers[host]);
    if (st.state.kind === 'up') say('INFO', `${host}: stopping master pid ${st.state.pid}`);
    st.state = { kind: 'off' }; st.enabled = false; st.locked = false; st.since = null;
    for (const list of [state.enabled, state.locked, state.connected]) setIn(list, host, false);
    push();
  }
  // The host that was waiting when the page was made goes on trying.
  for (const [host, st] of Object.entries(state.statuses)) {
    if (st.state.kind === 'waiting') later(host, st.state.retryIn * 1000, () => { st.state = { kind: 'connecting' }; push(); later(host, 1200, () => attempt(host, st.state.retryIn ? st.state.retryIn * 2 : 40)); });
  }

  // MARK: the readings

  const live = {};
  function reading(host, parts) {
    let r = live[host];
    if (!r) {
      r = live[host] = clone(D.reading);
      r.at = now();
      r.cpuPercent = 37;
      if (host === LOCAL) {
        Object.assign(r, { hostname: 'macbook', os: 'macOS 26.1', kernel: 'Darwin 25.1.0', cpus: 12, gpus: [], slurm: null, pbs: null, docker: null, vnc: [], tmux: [], screen: null, volumes: [], users: 1 });
        r.logins = [{ user: USER, tty: 'console', since: '2026-11-20 08:30', from: null }];
        r.disks = [{ filesystem: '/dev/disk3s1', mount: '/', total: 994662584320, used: 402000000000, removable: false, drive: null }];
        r.network = [{ name: 'en0', addresses: ['192.168.1.20/24'], received: 5e9, sent: 1e9 }];
        r.memory = { total: 34359738368, used: 19200000000 }; r.swap = { total: 0, used: 0 }; r.swapFiles = [];
        r.processes = [{ pid: 612, user: USER, cpu: 14.0, memory: 3.1, command: 'WindowServer' }, { pid: 2210, user: USER, cpu: 6.5, memory: 2.4, command: 'shellder' }, { pid: 1, user: 'root', cpu: 0.1, memory: 0.1, command: 'launchd' }];
        r.cpuPercent = 12;
      } else if (host !== 'gpu-box') {
        Object.assign(r, { hostname: host, cpus: 8, gpus: [], slurm: null, pbs: null, vnc: [], users: 1, cpuPercent: 9 });
        r.logins = r.logins.slice(0, 1);
        r.tmux = r.tmux.slice(0, 1);
        r.screen = [];
        r.disks = r.disks.slice(0, 1);
        r.volumes = [];
        r.memory = { total: 16777216000, used: 5200000000 };
        r.processes = [{ pid: 811, user: 'www-data', cpu: 3.2, memory: 1.8, command: 'nginx' }, { pid: 1, user: 'root', cpu: 0.0, memory: 0.1, command: 'systemd' }];
        if (host !== 'web-1') r.docker = null;
      }
    }
    const dt = Math.max(0, now() - r.at);
    r.at = now();
    r.uptime += dt;
    const busy = host === 'gpu-box';
    r.cpuPercent = wander(r.cpuPercent, 5, busy ? 20 : 2, busy ? 75 : 30);
    r.cpuTicks = null;
    r.load = [wander(r.load[0], 0.08, 0.05, r.cpus), r.load[1], r.load[2]].map(v => Math.round(v * 100) / 100);
    r.memory.used = Math.round(wander(r.memory.used, r.memory.total * 0.004, r.memory.total * 0.2, r.memory.total * 0.9));
    if (r.gpus[0]) {
      r.gpus[0].utilization = Math.round(wander(r.gpus[0].utilization, 4, 72, 99));
      r.gpus[0].memory.used = Math.round(wander(r.gpus[0].memory.used, 3e8, 2.6e10, 3.6e10));
    }
    for (const i of r.network) if (i.received !== null && i.addresses.length && i.name !== 'docker0') { i.received += Math.round(dt * rnd(2e5, 3e6)); i.sent += Math.round(dt * rnd(5e4, 6e5)); }
    if (r.processes[0]) r.processes[0].cpu = Math.round(wander(r.processes[0].cpu, busy ? 20 : 2, busy ? 150 : 1, busy ? 320 : 30) * 10) / 10;
    const out = clone(r);
    delete out.at;
    out.readParts = parts || [];
    return out;
  }

  // MARK: the files

  const T0 = now() - 2.2 * 3600;
  const dir = kids => ({ dir: kids, modified: T0 });
  const file = (text, modified = T0) => ({ text, size: bytesOf(text).length, modified });
  const README = '# Run 42\n\nThe model was trained for three epochs on the full set.\n\n| epoch | loss | accuracy |\n|---|---|---|\n| 1 | 0.412 | 0.871 |\n| 2 | 0.287 | 0.913 |\n| 3 | 0.214 | 0.936 |\n\n## To do\n\n- compare with run 41\n- write the report\n\n```python\nmodel = Model.load(\'checkpoints/run-42.pt\')\n```\n\nThe curve is in `~/figure.png`; the numbers are in `data/results.csv`.\n';
  const TRAIN = '#!/usr/bin/env python3\n"""Trains the model and saves a checkpoint."""\nimport argparse\nimport time\n\nfrom src.data import batches\nfrom src.model import Model\n\n\ndef main():\n    parser = argparse.ArgumentParser()\n    parser.add_argument("--epochs", type=int, default=3)\n    args = parser.parse_args()\n    model = Model()\n    for epoch in range(1, args.epochs + 1):\n        started = time.time()\n        loss, acc = model.fit(batches("data/train"))\n        print(f"epoch {epoch}/{args.epochs}  loss {loss:.3f}  acc {acc:.3f}  {time.time() - started:.0f}s")\n    model.save("checkpoints/run-42.pt")\n\n\nif __name__ == "__main__":\n    main()\n';
  const MODEL = 'import torch\nfrom torch import nn\n\n\nclass Model(nn.Module):\n    def __init__(self, width=512, depth=6):\n        super().__init__()\n        self.layers = nn.Sequential(*[nn.Linear(width, width) for _ in range(depth)])\n\n    def forward(self, x):\n        return self.layers(x)\n\n    @classmethod\n    def load(cls, path):\n        model = cls()\n        model.load_state_dict(torch.load(path))\n        return model\n';
  const DATA = 'from pathlib import Path\n\n\ndef batches(root, size=64):\n    """The files under `root`, `size` at a time."""\n    files = sorted(Path(root).glob("*.pt"))\n    for i in range(0, len(files), size):\n        yield files[i:i + size]\n';
  const MAKEFILE = '.PHONY: train test clean\n\ntrain:\n\tpython train.py --epochs 3\n\ntest:\n\tpytest -q\n\nclean:\n\trm -rf checkpoints/*.pt\n';
  const RESULTS = 'epoch,loss,accuracy,seconds\n1,0.412,0.871,41\n2,0.287,0.913,40\n3,0.214,0.936,41\n';
  const RUNLOG = '2026-11-26 11:02:14 loading data/train (1840 files)\n2026-11-26 11:02:19 model: 6 layers, 512 wide\n2026-11-26 11:03:00 epoch 1/3  loss 0.412  acc 0.871\n2026-11-26 11:03:40 epoch 2/3  loss 0.287  acc 0.913\n2026-11-26 11:04:21 epoch 3/3  loss 0.214  acc 0.936\n2026-11-26 11:04:21 saved checkpoints/run-42.pt\n';
  const TODO = '# Notes\n\n- [x] move the data to /mnt/data disk\n- [ ] ask bob about the second GPU\n- [ ] nightly eval: `sbatch eval.sh`\n';
  const project = () => dir({
    data: dir({ 'results.csv': file(RESULTS), 'run-42.log': file(RUNLOG) }),
    docs: dir({ 'report.md': file('# Report\n\nSee [the run](../README.md).\n') }),
    src: dir({ 'data.py': file(DATA), 'model.py': file(MODEL) }),
    checkpoints: dir({ 'run-42.pt': { size: 48234496, modified: T0 } }),
    Makefile: file(MAKEFILE), 'README.md': file(README), 'train.py': file(TRAIN),
  });
  const fs = {
    host: dir({
      home: dir({ [USER]: dir({ notes: dir({ 'todo.md': file(TODO) }), project: project(), 'figure.png': { size: 0, modified: T0, picture: true }, '.bashrc': file('export EDITOR=vim\nalias ll="ls -l"\n') }) }),
      etc: dir({ hostname: file('gpu-box\n'), 'os-release': file('PRETTY_NAME="Ubuntu 24.04.1 LTS"\nNAME="Ubuntu"\nVERSION_ID="24.04"\n') }),
      srv: dir({ web: dir({ html: dir({ 'index.html': file('<!doctype html>\n<title>web</title>\n<h1>It works</h1>\n') }), conf: dir({}) }) }),
      tmp: dir({}),
    }),
    local: dir({
      Users: dir({ [USER]: dir({ Desktop: dir({}), Documents: dir({ notes: dir({}), project: dir({ 'README.md': file(README) }), 'figure.png': { size: 0, modified: T0, picture: true } }), Downloads: dir({}) }) }),
    }),
  };
  const fsOf = source => source && source.kind === 'local' ? fs.local : fs.host;
  const homeOf = source => source && source.kind === 'local' ? `/Users/${USER}` : `/home/${USER}`;
  const at = (source, path) => { let n = fsOf(source); for (const part of path.split('/').filter(Boolean)) { if (!n || !n.dir) return null; n = n.dir[part]; } return n || null; };
  const join = (d, name) => d === '/' ? '/' + name : d + '/' + name;
  const parentOf = p => { const i = p.replace(/\/+$/, '').lastIndexOf('/'); return i <= 0 ? '/' : p.slice(0, i); };
  const baseOf = p => p.replace(/\/+$/, '').split('/').pop();

  // The picture a preview shows for figure.png: the run's loss, drawn
  // here, since no file of the demo is a real one.
  let picture = null;
  function pictureBytes() {
    if (picture) return picture;
    const c = document.createElement('canvas');
    c.width = 720; c.height = 440;
    const g = c.getContext('2d');
    g.fillStyle = '#fff'; g.fillRect(0, 0, c.width, c.height);
    g.strokeStyle = '#d0d0d6'; g.lineWidth = 1; g.font = '15px -apple-system, sans-serif'; g.fillStyle = '#555';
    const x = e => 80 + (e - 1) * 280, y = v => 380 - v * 320;
    for (const v of [0, 0.25, 0.5, 0.75, 1]) { g.beginPath(); g.moveTo(70, y(v)); g.lineTo(680, y(v)); g.stroke(); g.fillText(v.toFixed(2), 24, y(v) + 5); }
    for (const e of [1, 2, 3]) g.fillText('epoch ' + e, x(e) - 26, 412);
    const curve = (points, colour, name) => {
      g.strokeStyle = colour; g.fillStyle = colour; g.lineWidth = 3;
      g.beginPath(); points.forEach((v, i) => i ? g.lineTo(x(i + 1), y(v)) : g.moveTo(x(1), y(v))); g.stroke();
      points.forEach((v, i) => { g.beginPath(); g.arc(x(i + 1), y(v), 5, 0, 7); g.fill(); });
      g.fillText(name, x(3) - 70, y(points[2]) - 14);
    };
    curve([0.412, 0.287, 0.214], '#0a84ff', 'loss');
    curve([0.871, 0.913, 0.936], '#ff9500', 'accuracy');
    const raw = atob(c.toDataURL('image/png').split(',')[1]);
    picture = Array.from(raw, ch => ch.charCodeAt(0));
    for (const tree of [fs.host, fs.local]) (function size(n) { if (n.picture) n.size = picture.length; if (n.dir) Object.values(n.dir).forEach(size); })(tree);
    return picture;
  }
  pictureBytes();

  const hostKey = h => 'files-' + [...bytesOf(h)].map(b => (b >= 48 && b <= 57) || (b >= 65 && b <= 90) || (b >= 97 && b <= 122) || b === 45 ? String.fromCharCode(b) : '_' + b.toString(16).padStart(2, '0')).join('');
  const models = new Map();
  const pane = (source, root, open = []) => ({ source, root, open: new Set(open), selected: [], sort: { key: 'name', ascending: true }, showHidden: false, back: [], version: 1 });
  function model(host) {
    let m = models.get(host);
    if (!m) {
      const src = host === LOCAL ? { kind: 'local' } : { kind: 'host', host };
      const home = homeOf(src);
      m = { host, seq: 0, transfers: [], active: null, lastId: 0, panes: {
        left: pane(src, home), right: pane({ kind: 'local' }, `/Users/${USER}/Documents`),
        tab: pane(src, home, host === LOCAL ? [] : [home + '/project']) } };
      models.set(host, m);
    }
    return m;
  }
  function rowsOf(p) {
    const out = [];
    const sign = p.sort.ascending ? 1 : -1;
    const walk = (path, depth) => {
      const n = at(p.source, path);
      if (!n || !n.dir) return;
      const names = Object.keys(n.dir).filter(x => p.showHidden || !x.startsWith('.'));
      names.sort((a, b) => {
        const x = n.dir[a], y = n.dir[b];
        if (!x.dir !== !y.dir) return x.dir ? -1 : 1;
        const c = p.sort.key === 'size' ? (x.size || 0) - (y.size || 0) : p.sort.key === 'modified' ? x.modified - y.modified : 0;
        return sign * (c || a.localeCompare(b));
      });
      for (const name of names) {
        const k = n.dir[name], full = join(path, name), expanded = !!k.dir && p.open.has(full);
        out.push({ name, path: full, isDir: !!k.dir, size: k.dir ? 0 : k.size || 0, modified: k.modified, depth, expanded, loading: false });
        if (expanded) walk(full, depth + 1);
      }
    };
    walk(p.root, 0);
    return out;
  }
  function snapshot(m) {
    const one = p => {
      const local = p.source.kind === 'local', home = homeOf(p.source);
      return {
        source: p.source, title: local ? 'Local' : p.source.host, root: p.root, error: at(p.source, p.root) ? null : 'No such file or directory',
        rows: rowsOf(p), showHidden: p.showHidden, sort: p.sort, canGoBack: p.back.length > 0, selected: p.selected,
        shortcuts: local ? [{ title: USER, path: home }, { title: 'Desktop', path: home + '/Desktop' }, { title: 'Documents', path: home + '/Documents' }, { title: 'Downloads', path: home + '/Downloads' }]
          : [{ title: USER, path: home }, { title: '/', path: '/' }],
        home, loadingRoot: false, version: ++p.version,
      };
    };
    const P = m.panes;
    const differ = JSON.stringify(P.left.source) !== JSON.stringify(P.right.source) || P.left.root !== P.right.root;
    return {
      host: m.host, hosts: state.hosts.map(h => h.alias).filter(h => state.connected.includes(h)),
      panes: { left: one(P.left), right: one(P.right), tab: one(P.tab) },
      transfers: m.transfers, active: m.active, queued: m.transfers.filter(t => t.state.kind === 'waiting').length,
      lastFinished: null, notice: '', noticeIsError: false, showHistory: m.history !== false, question: null,
      canCopy: { left: differ && P.left.selected.length > 0, right: differ && P.right.selected.length > 0, tab: false },
      canRetry: [], seq: ++m.seq,
    };
  }
  const changed = m => emitAll('files:' + hostKey(m.host), snapshot(m));
  const everyModel = () => { for (const m of models.values()) changed(m); };

  // A copy: nothing is copied anywhere, but the tree gets the entry
  // once the bar is full.
  function copy(m, from, paths, to, folder) {
    for (const path of paths) {
      const n = at(from, path);
      if (!n || !at(to, folder)) continue;
      m.transfers.push({
        id: ++m.lastId, name: baseOf(path), isUpload: from.kind === 'local', canResume: false,
        job: { from, to, source: path, destination: folder, resume: false },
        state: { kind: 'waiting' }, progress: null, total: n.dir ? 4096 : n.size, queuedAt: now(), endedAt: null, node: n,
      });
    }
    runCopies(m);
    changed(m);
  }
  function runCopies(m) {
    if (m.active) return;
    const rec = m.transfers.find(t => t.state.kind === 'waiting');
    if (!rec) return;
    m.active = rec; rec.state = { kind: 'running' }; rec.progress = 0;
    const step = () => {
      if (m.active !== rec) return;
      rec.progress = Math.min(1, rec.progress + rnd(0.08, 0.2));
      emitAll('files-progress:' + hostKey(m.host), { id: rec.id, progress: rec.progress });
      if (rec.progress < 1) return void setTimeout(step, 220);
      const folder = at(rec.job.to, rec.job.destination);
      if (folder && folder.dir) folder.dir[rec.name] = clone(rec.node);
      rec.state = { kind: 'done' }; rec.endedAt = now(); m.active = null;
      delete rec.node;
      runCopies(m);
      everyModel();
    };
    setTimeout(step, 220);
  }

  // MARK: the terminals

  const terms = new Map();
  let lastTerm = 0;
  const WIDE = /[\u1100-\u115f\u2e80-\ua4cf\uac00-\ud7a3\uf900-\ufaff\ufe30-\ufe4f\uff00-\uff60\uffe0-\uffe6]|[\ud800-\udbff][\udc00-\udfff]/;
  const widthOf = text => Array.from(text).reduce((w, ch) => w + (WIDE.test(ch) ? 2 : 1), 0);
  const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
  const DAYS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
  const human = n => { const u = ['B', 'K', 'M', 'G', 'T']; let i = 0; while (n >= 1024 && i < 4) { n /= 1024; i++; } return (i && n < 10 ? n.toFixed(1) : Math.round(n)) + u[i]; };
  const unescape = s => s.replace(/\\(e|033|x1b|a|007|n|t|\\)/gi, (_, c) => ({ e: '\x1b', '033': '\x1b', x1b: '\x1b', a: '\x07', '007': '\x07', n: '\n', t: '\t', '\\': '\\' })[c.toLowerCase()]);

  function openShell(id, label, { host, command, dir: startDir, cols, rows }) {
    const local = host === LOCAL;
    const source = local ? { kind: 'local' } : { kind: 'host', host };
    const home = homeOf(source);
    const cmd = command || '';
    const session = /^tmux /.test(cmd) ? (cmd.match(/=([\w.-]+)/) || [])[1] || 'main' : null;
    const container = (cmd.match(/^(?:docker|podman) exec -it '?([\w.-]+)/) || [])[1];
    const job = (cmd.match(/^srun --jobid='?(\w+)/) || [])[1];
    const sh = {
      id, label, cols: cols || 80, rows: rows || 24, line: '', history: [], at: 0, status: 0, running: null, asking: null, sudoUntil: 0,
      user: container ? 'root' : USER, name: container || (job ? 'gpu01' : local ? 'macbook' : host),
      cwd: container ? '/' : startDir && at(source, startDir) ? startDir : host === 'gpu-box' && !session && !job ? home + '/project' : home,
      // Shells the Console starts get its shell integration; a session's
      // shell and a container's were started by others and have none.
      marks: !session && !container && !/^screen /.test(cmd),
    };
    const out = text => emitTo(label, 'term-data', { id, data: b64(text) });
    const print = text => out(text.replace(/\r?\n/g, '\r\n'));
    const tilde = p => p === home ? '~' : p.startsWith(home + '/') ? '~' + p.slice(home.length) : p;
    const resolve = p => {
      const full = p === '~' ? home : p.startsWith('~/') ? home + p.slice(1) : p.startsWith('/') ? p : join(sh.cwd, p);
      const parts = [];
      for (const part of full.split('/')) { if (part === '..') parts.pop(); else if (part && part !== '.') parts.push(part); }
      return '/' + parts.join('/');
    };
    const bar = () => {
      if (!session) return;
      const s = (live[host] || D.reading).tmux.find(x => x.name === session);
      const windows = s ? s.windows.map(w => `${w.index}:${w.name}${w.active ? '*' : ' '}`).join(' ') : '0:bash*';
      const d = new Date();
      const right = `"${sh.name}" ${pad(d.getHours())}:${pad(d.getMinutes())} ${pad(d.getDate())}-${MONTHS[d.getMonth()]}-${String(d.getFullYear()).slice(2)}`;
      const left = `[${session}] ${windows}`;
      const text = (left + ' '.repeat(Math.max(1, sh.cols - left.length - right.length)) + right).slice(0, sh.cols);
      out(`\x1b7\x1b[1;${sh.rows - 1}r\x1b[${sh.rows};1H\x1b[30;42m${text}\x1b[0m\x1b8`);
    };
    const prompt = () => {
      sh.line = ''; sh.running = null;
      const ps = local ? `\x1b[1;32m${sh.user}@${sh.name}\x1b[0m \x1b[1;34m${tilde(sh.cwd) === '~' ? '~' : baseOf(sh.cwd) || '/'}\x1b[0m % `
        : container ? `${sh.user}@${sh.name}:${sh.cwd}# ` : `\x1b[1;32m${sh.user}@${sh.name}\x1b[0m:\x1b[1;34m${tilde(sh.cwd)}\x1b[0m$ `;
      const marks = sh.marks ? `\x1b]133;D;${sh.status}\x07\x1b]7;file://${sh.name}${encodeURI(sh.cwd)}\x07\x1b]133;A\x07\x1b]0;${sh.user}@${sh.name}: ${tilde(sh.cwd)}\x07` : '';
      out(marks + ps + (sh.marks ? '\x1b]133;B\x07' : ''));
      bar();
    };
    const done = code => { sh.status = code || 0; prompt(); };
    const end = () => { terms.delete(id); emitTo(label, 'term-exit', { id, code: 0 }); };
    const lsLine = (name, n) => {
      const d = new Date(n.modified * 1000);
      return `${n.dir ? 'drwxr-xr-x' : '-rw-r--r--'} ${n.dir ? 2 : 1} ${sh.user} ${sh.user} ${String(n.dir ? 4096 : n.size || 0).padStart(9)} ${MONTHS[d.getMonth()]} ${String(d.getDate()).padStart(2)} ${pad(d.getHours())}:${pad(d.getMinutes())} ${n.dir ? `\x1b[1;34m${name}\x1b[0m` : name}`;
    };
    const table = (head, rows) => {
      const all = [head, ...rows], w = head.map((_, i) => Math.max(...all.map(r => String(r[i]).length)));
      print(all.map(r => r.map((c, i) => i === r.length - 1 ? c : String(c).padEnd(w[i] + 2)).join('')).join('\n') + '\n');
    };
    const read = () => reading(host, null);

    const commands = {
      help() {
        print('This shell belongs to the demo and runs in your browser. It knows\n  ls cd pwd cat head wc echo printf clear mkdir touch rm\n  whoami hostname id uname date uptime nproc free df\n  python train.py [--epochs N]   sleep N   sudo CMD   tput bel\n  nvidia-smi   tmux ls   docker ps   squeue   git status   exit\nTab completes a name, the arrows go through what was typed.\n');
      },
      ls(args) {
        const flags = args.filter(a => a.startsWith('-')).join(''), targets = args.filter(a => !a.startsWith('-'));
        let code = 0;
        for (const t of targets.length ? targets : ['.']) {
          const path = resolve(t), n = at(source, path);
          if (!n) { print(`ls: cannot access '${t}': No such file or directory\n`); code = 2; continue; }
          if (targets.length > 1 && n.dir) print(`${t}:\n`);
          const names = n.dir ? Object.keys(n.dir).filter(x => flags.includes('a') || !x.startsWith('.')).sort() : [baseOf(path)];
          const node = name => n.dir ? n.dir[name] : n;
          if (flags.includes('l')) print(names.map(name => lsLine(name, node(name))).join('\n') + (names.length ? '\n' : ''));
          else if (names.length) {
            const w = Math.max(...names.map(widthOf)) + 2, per = Math.max(1, Math.floor(sh.cols / w));
            let s = '';
            names.forEach((name, i) => { const last = (i + 1) % per === 0 || i === names.length - 1; s += (node(name).dir ? `\x1b[1;34m${name}\x1b[0m` : name) + (last ? '\n' : ' '.repeat(w - widthOf(name))); });
            print(s);
          }
        }
        return code;
      },
      cd(args) {
        const path = resolve(args[0] || '~'), n = at(source, path);
        if (!n) { print(`bash: cd: ${args[0]}: No such file or directory\n`); return 1; }
        if (!n.dir) { print(`bash: cd: ${args[0]}: Not a directory\n`); return 1; }
        sh.cwd = path;
      },
      pwd() { print(sh.cwd + '\n'); },
      cat(args, _, lines) {
        let code = 0;
        for (const a of args.filter(x => !x.startsWith('-'))) {
          const n = at(source, resolve(a));
          if (!n) { print(`cat: ${a}: No such file or directory\n`); code = 1; }
          else if (n.dir) { print(`cat: ${a}: Is a directory\n`); code = 1; }
          else if (n.text === undefined) { print(`cat: ${a}: ${human(n.size)} of binary data, not shown in this demo\n`); code = 1; }
          else print(lines ? n.text.split('\n').slice(0, lines).join('\n') + '\n' : n.text);
        }
        return code;
      },
      head(args) { const i = args.indexOf('-n'); const n = i >= 0 ? Number(args[i + 1]) || 10 : 10; return commands.cat(args.filter((a, j) => i < 0 || (j !== i && j !== i + 1)), '', n); },
      wc(args) {
        for (const a of args.filter(x => !x.startsWith('-'))) {
          const n = at(source, resolve(a));
          if (!n || n.dir || n.text === undefined) { print(`wc: ${a}: No such file or directory\n`); return 1; }
          print(`${n.text.split('\n').length - 1} ${a}\n`);
        }
      },
      echo(args) { const e = args[0] === '-e' || args[0] === '-ne', bare = args[0] === '-n' || args[0] === '-ne'; const text = args.slice(e || bare ? 1 : 0).join(' '); print((e ? unescape(text) : text) + (bare ? '' : '\n')); },
      printf(args) { let i = 1; print(unescape(args[0] || '').replace(/%s/g, () => args[i++] || '')); },
      clear() { out('\x1b[H\x1b[2J\x1b[3J'); },
      mkdir(args) { for (const a of args.filter(x => !x.startsWith('-'))) { const p = resolve(a), up = at(source, parentOf(p)); if (!up || !up.dir) { print(`mkdir: cannot create directory '${a}': No such file or directory\n`); return 1; } if (!up.dir[baseOf(p)]) up.dir[baseOf(p)] = { dir: {}, modified: now() }; } everyModel(); },
      touch(args) { for (const a of args) { const p = resolve(a), up = at(source, parentOf(p)); if (!up || !up.dir) { print(`touch: cannot touch '${a}': No such file or directory\n`); return 1; } if (up.dir[baseOf(p)]) up.dir[baseOf(p)].modified = now(); else up.dir[baseOf(p)] = file('', now()); } everyModel(); },
      rm(args) {
        const r = args.some(a => /^-\w*r/i.test(a));
        for (const a of args.filter(x => !x.startsWith('-'))) {
          const p = resolve(a), up = at(source, parentOf(p)), n = at(source, p);
          if (!n) { print(`rm: cannot remove '${a}': No such file or directory\n`); return 1; }
          if (n.dir && !r) { print(`rm: cannot remove '${a}': Is a directory\n`); return 1; }
          delete up.dir[baseOf(p)];
        }
        everyModel();
      },
      whoami() { print(sh.user + '\n'); },
      hostname() { print(sh.name + '\n'); },
      id() { print(`uid=1000(${sh.user}) gid=1000(${sh.user}) groups=1000(${sh.user}),27(sudo),999(docker)\n`); },
      uname(args) { const r = read(); print((args.includes('-a') ? `${local ? 'Darwin' : 'Linux'} ${sh.name} ${r.kernel.split(' ')[1]} ${local ? 'arm64' : 'x86_64 GNU/Linux'}` : args.includes('-r') ? r.kernel.split(' ')[1] : local ? 'Darwin' : 'Linux') + '\n'); },
      date() { const d = new Date(); print(`${DAYS[d.getDay()]} ${MONTHS[d.getMonth()]} ${String(d.getDate()).padStart(2)} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())} ${d.getFullYear()}\n`); },
      uptime() { const r = read(), d = new Date(); print(` ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())} up ${Math.floor(r.uptime / 86400)} days, ${Math.floor(r.uptime % 86400 / 3600)}:${pad(Math.floor(r.uptime % 3600 / 60))},  ${r.users} users,  load average: ${r.load.map(v => v.toFixed(2)).join(', ')}\n`); },
      nproc() { print(read().cpus + '\n'); },
      free() { const r = read(); table(['', 'total', 'used', 'free'], [['Mem:', human(r.memory.total), human(r.memory.used), human(r.memory.total - r.memory.used)], ['Swap:', human(r.swap.total), human(r.swap.used), human(r.swap.total - r.swap.used)]]); },
      df() { table(['Filesystem', 'Size', 'Used', 'Avail', 'Use%', 'Mounted on'], read().disks.map(d => [d.filesystem, human(d.total), human(d.used), human(d.total - d.used), Math.round(d.used / d.total * 100) + '%', d.mount])); },
      'nvidia-smi'() {
        const g = read().gpus;
        if (!g.length) return missing('nvidia-smi');
        const line = '+-----------------------------------------------------------------------------+\n';
        let s = line + '| NVIDIA-SMI 550.90.07    Driver Version: 550.90.07    CUDA Version: 12.4     |\n' + line;
        s += '| GPU  Name                       Memory-Usage              GPU-Util          |\n' + line;
        for (const x of g) s += '| ' + `${String(x.index).padStart(3)}  ${x.name.padEnd(26)} ${(Math.round(x.memory.used / 1048576) + 'MiB / ' + Math.round(x.memory.total / 1048576) + 'MiB').padEnd(25)} ${String(Math.round(x.utilization)).padStart(3)}%`.padEnd(76) + '|\n';
        print(s + line);
      },
      tmux(args) {
        if (args[0] !== 'ls' && args[0] !== 'list-sessions') { print('tmux: a session opens from the Sessions panel in this demo, in a tab of its own\n'); return 1; }
        const list = read().tmux || [];
        if (!list.length) { print('no server running on /tmp/tmux-1000/default\n'); return 1; }
        print(list.map(s => `${s.name}: ${s.windows.length} windows${s.attached ? ' (attached)' : ''}`).join('\n') + '\n');
      },
      docker(args) {
        const d = read().docker;
        if (!d) return missing('docker');
        if (args[0] !== 'ps') { print('docker: only ps in this demo; a container opens from the Containers panel\n'); return 1; }
        table(['CONTAINER ID', 'IMAGE', 'STATUS', 'PORTS', 'NAMES'], d.list.map(c => [c.id, c.image, c.status, c.ports || '', c.name]));
      },
      squeue() {
        const jobs = read().slurm;
        if (!jobs) return missing('squeue');
        table(['JOBID', 'PARTITION', 'NAME', 'USER', 'ST', 'TIME', 'NODES', 'NODELIST(REASON)'], jobs.map(j => [j.id, j.partition, j.name, sh.user, j.state === 'RUNNING' ? 'R' : 'PD', j.time, j.nodes, j.reason]));
      },
      git(args) {
        if (args[0] !== 'status') { print(`git: only status in this demo\n`); return 1; }
        if (!at(source, sh.cwd + '/README.md')) { print('fatal: not a git repository (or any of the parent directories): .git\n'); return 128; }
        print("On branch main\nYour branch is up to date with 'origin/main'.\n\nnothing to commit, working tree clean\n");
      },
      true() {}, false() { return 1; },
      tput(args) { if (args[0] === 'bel') out('\x07'); },
      systemctl(args) { if (args[0] === 'status') print(`● ${args[1] || 'nginx'}.service\n     Active: active (running)\n`); },
      exit() { print(session ? '[exited]\n' : 'logout\n'); end(); return 'gone'; },
      logout() { return commands.exit(); },
      sleep(args) {
        const s = clamp(Number(args[0]) || 0, 0, 600);
        const timer = setTimeout(() => done(0), s * 1000);
        return { stop: () => clearTimeout(timer) };
      },
      python(args) {
        if (args[0] === '-V' || args[0] === '--version') { print('Python 3.12.3\n'); return 0; }
        if (!args[0]) { print('python: only a script runs in this demo, try: python train.py\n'); return 1; }
        const path = resolve(args[0]);
        if (!at(source, path)) { print(`python: can't open file '${path}': [Errno 2] No such file or directory\n`); return 2; }
        if (baseOf(path) !== 'train.py') { print(`python: only train.py runs in this demo\n`); return 1; }
        const i = args.indexOf('--epochs'), n = clamp(Math.floor(Number(i >= 0 ? args[i + 1] : 3)) || 3, 1, 30);
        let epoch = 0, loss = 0.62, acc = 0.79, timer = null;
        const tick = () => {
          epoch++;
          loss = Math.max(0.05, loss * rnd(0.66, 0.78)); acc = Math.min(0.995, acc + (1 - acc) * rnd(0.3, 0.42));
          print(`epoch ${epoch}/${n}  loss ${loss.toFixed(3)}  acc ${acc.toFixed(3)}  00:${pad(Math.round(rnd(39, 42)))}\n`);
          out(`\x1b]9;4;1;${Math.round(epoch / n * 100)}\x07`);
          if (epoch < n) return void (timer = setTimeout(tick, 1300));
          const ck = at(source, parentOf(path) + '/checkpoints');
          const name = `run-${43 + (ck ? Object.keys(ck.dir).length - 1 : 0)}.pt`;
          if (ck && ck.dir) { ck.dir[name] = { size: 48234496, modified: now() }; everyModel(); }
          out('\x1b]9;4;0\x07');
          print(`\x1b[32msaved\x1b[0m checkpoints/${name}\n`);
          done(0);
        };
        out('\x1b]9;4;1;0\x07');
        timer = setTimeout(tick, 1300);
        return { stop: () => { clearTimeout(timer); out('\x1b]9;4;0\x07'); print('Traceback (most recent call last):\n  File "train.py", line 17, in main\nKeyboardInterrupt\n'); } };
      },
      python3(args) { return commands.python(args); },
      sudo(args) {
        if (!args.length) { print('usage: sudo command\n'); return 1; }
        const go = () => run(args.join(' '), true);
        if (sh.user === 'root' || now() < sh.sudoUntil) return go();
        sh.asking = { then: () => { sh.sudoUntil = now() + 300; const r = go(); if (typeof r !== 'object' && r !== 'gone') done(r); }, cancel: () => done(1) };
        out(`[sudo] password for ${sh.user}: `);
        return 'asking';
      },
    };
    for (const name of ['vim', 'vi', 'nano', 'less', 'man', 'htop', 'top', 'ssh']) commands[name] = () => { print(`${name}: not in this demo, which has no host to run it on\n`); return 1; };
    const missing = name => { print(`bash: ${name}: command not found\n`); return 127; };
    const words = text => { const out = []; text.replace(/"([^"]*)"|'([^']*)'|(\S+)/g, (_, a, b, c) => { out.push(a !== undefined ? a : b !== undefined ? b : c); }); return out; };
    /// One command line: what its command returns, an exit status, a
    /// job under way ({stop}), 'asking' (sudo's question) or 'gone'.
    const run = (text, inner) => {
      const argv = words(text.trim());
      if (!argv.length) return 0;
      const f = Object.prototype.hasOwnProperty.call(commands, argv[0]) ? commands[argv[0]] : null;
      const r = f ? f(argv.slice(1), text) : missing(argv[0]);
      if (r && typeof r === 'object') sh.running = r;
      return r === undefined ? 0 : r;
    };
    const enter = () => {
      const text = sh.line.trim();
      out('\r\n');
      if (!text) return prompt();
      if (sh.history[sh.history.length - 1] !== text) sh.history.push(text);
      sh.at = sh.history.length;
      if (sh.marks) out('\x1b]133;C\x07');
      const r = run(text);
      if (typeof r === 'number') done(r);
    };
    const setLine = text => { const w = widthOf(sh.line); out((w ? `\x1b[${w}D` : '') + '\x1b[K' + text); sh.line = text; };
    const complete = () => {
      const parts = sh.line.split(' '), word = parts[parts.length - 1];
      let names;
      if (parts.length === 1) names = Object.keys(commands).filter(c => c.startsWith(word)).map(c => c + ' ');
      else {
        const cut = word.lastIndexOf('/'), base = cut >= 0 ? word.slice(0, cut + 1) : '', stem = word.slice(cut + 1);
        const n = at(source, resolve(base || '.'));
        names = n && n.dir ? Object.keys(n.dir).filter(x => x.startsWith(stem) && (stem.startsWith('.') || !x.startsWith('.'))).map(x => base + x + (n.dir[x].dir ? '/' : ' ')) : [];
      }
      if (!names.length) return out('\x07');
      let common = names[0];
      for (const n of names) while (!n.startsWith(common)) common = common.slice(0, -1);
      if (names.length > 1) common = common.trimEnd();
      if (common.length <= word.length) return out('\x07');
      out(common.slice(word.length)); sh.line += common.slice(word.length);
    };
    sh.resize = (c, r) => { if (session && r !== sh.rows) out(`\x1b7\x1b[${sh.rows};1H\x1b[2K\x1b8`); sh.cols = c; sh.rows = r; bar(); };
    sh.input = data => {
      const chars = Array.from(data);
      for (let i = 0; i < chars.length; i++) {
        const ch = chars[i];
        if (ch === '\x1b') {
          // An escape sequence: the arrows go through what was typed, the rest is dropped.
          if (chars[i + 1] === '[' || chars[i + 1] === 'O') {
            let j = i + 2;
            while (j < chars.length && !/[@-~]/.test(chars[j])) j++;
            const last = chars[j];
            i = j;
            if (sh.running || sh.asking) continue;
            if (last === 'A' && sh.at > 0) setLine(sh.history[--sh.at]);
            if (last === 'B' && sh.at < sh.history.length) setLine(sh.history[++sh.at] || '');
          }
          continue;
        }
        if (sh.running) {
          if (ch === '\x03') { const job = sh.running; sh.running = null; out('^C\r\n'); job.stop(); done(130); }
          continue;
        }
        if (sh.asking) {
          const q = sh.asking;
          if (ch === '\r') { sh.asking = null; out('\r\n'); q.then(); }
          else if (ch === '\x03') { sh.asking = null; out('\r\n'); q.cancel(); }
          continue;
        }
        if (ch === '\r') enter();
        else if (ch === '\x7f' || ch === '\b') { const last = Array.from(sh.line).pop(); if (last) { sh.line = sh.line.slice(0, -last.length); const w = widthOf(last); out(`\x1b[${w}D\x1b[K`); } }
        else if (ch === '\x03') { out('^C\r\n'); done(130); }
        else if (ch === '\x04') { if (!sh.line) { out('\r\n'); commands.exit(); return; } }
        else if (ch === '\x0c') { const line = sh.line; out('\x1b[H\x1b[2J'); prompt(); out(line); sh.line = line; }
        else if (ch === '\x15') setLine('');
        else if (ch === '\t') complete();
        else if (ch >= ' ') { sh.line += ch; out(ch); }
      }
    };
    sh.password = () => { if (sh.asking) sh.input('\r'); };
    terms.set(id, sh);
    setTimeout(() => {
      if (!terms.has(id)) return;
      if (session) out(`\x1b[H\x1b[2J`);
      else if (job) print(`srun: job ${job} has been allocated resources\n`);
      else if (!container) {
        const d = new Date(Date.now() - 6.4e6);
        print(local ? `Last login: ${DAYS[d.getDay()]} ${MONTHS[d.getMonth()]} ${d.getDate()} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())} on ttys003\n`
          : `Welcome to Ubuntu 24.04.1 LTS (GNU/Linux 6.8.0-45-generic x86_64)\n\n\x1b[2m${api.lang === 'zh' ? '这台主机属于演示，只存在于你的浏览器里。输入 help 列出这个 shell 认得的命令。' : 'This host belongs to the demo and exists in your browser alone. Type help to list the commands of this shell.'}\x1b[0m\n\n`);
      }
      prompt();
    }, 260);
  }

  // MARK: what the pages ask

  const forwards = { 'gpu-box': clone(D.forwards) };
  const sameForward = (a, b) => a.direction === b.direction && a.local === b.local && a.remote === b.remote;
  const paneOf = a => model(a.host).panes[a.pane];
  const andChanged = f => a => { const r = f(a); changed(model(a.host)); return r === undefined ? null : r; };

  const answers = {
    get_state: () => state,
    window_ready: () => null,
    set_setting: ({ key, value }) => { const path = key.split('.'); let o = state.settings; while (path.length > 1) o = o[path.shift()]; o[path[0]] = value; push(); },
    reload_config: () => push(),
    open_window: ({ which, host }) => { if (api.desk) api.desk.open(which, host); },
    quit: () => null,
    log_tail: ({ after }) => ({ offset: log.length, reset: after == null, lines: after == null ? log.slice() : log.slice(after) }),
    set_enabled: ({ host, on }) => on ? connect(host) : disconnect(host),
    set_locked: ({ host, on }) => {
      const st = state.statuses[host];
      st.locked = on; setIn(state.locked, host, on);
      if (on && st.state.kind === 'off') connect(host); else push();
    },
    host_action: ({ host, action }) => {
      if (action === 'disconnect') return disconnect(host);
      if (action === 'reconnect') { const st = state.statuses[host]; if (st.state.kind === 'up') say('INFO', `${host}: reconnect requested`); setIn(state.connected, host, false); }
      connect(host);
    },
    set_idle_mode: ({ host, mode }) => { state.statuses[host].idleMode = mode; state.idleModes[host] = mode; push(); },
    set_secret: ({ host, kind }) => { const list = state.secrets[host] = state.secrets[host] || []; setIn(list, kind, true); push(); },
    remove_secret: ({ host, kind }) => { setIn(state.secrets[host] || [], kind, false); push(); },
    remove_all_secrets: ({ host }) => { state.secrets[host] = []; push(); },
    // Nothing is stored, so there is nothing to show: what a visitor
    // typed into a field of the demo is not kept either.
    reveal_secret: ({ kind }) => kind === 'totp' ? 'JBSWY3DPEHPK3PXP' : 'not-a-real-password',
    totp_code: () => { const step = Math.floor(now() / 30); return { code: pad((step * 7919 + 104729) % 1000000, 6), remaining: 30 - Math.floor(now() % 30) }; },

    console_snapshot: ({ host, parts }) => reading(host, parts),
    console_tree: () => ({ rows: [] }),
    console_switch: ({ host }, label) => { setTimeout(() => emitTo(label, 'console-host', { host }), 30); },
    console_forwards: ({ host }) => forwards[host] || [],
    console_forward: ({ host, direction, local, remote }) => {
      const f = { direction, local: local ?? Math.floor(rnd(49152, 65000)), remote: remote ?? Math.floor(rnd(32768, 60000)) };
      (forwards[host] = forwards[host] || []).push(f);
      return f;
    },
    console_cancel_forward: ({ host, forward }) => { forwards[host] = (forwards[host] || []).filter(f => !sameForward(f, forward)); },
    notify: ({ title, body }) => { if (api.desk) api.desk.notify(title, body); },
    terminal_themes: () => [],
    fonts: () => ['Menlo', 'Monaco', 'SF Mono', 'Courier New'],
    system_fonts: () => ['Menlo', 'Monaco', 'SF Mono', 'Courier New'],
    sounds: () => [], system_sounds: () => [],

    files_open: ({ host }) => snapshot(model(host)),
    files_state: ({ host }) => snapshot(model(host)),
    files_home: ({ source }) => homeOf(source),
    files_exists: ({ source, path }) => !!at(source, path),
    files_target: a => paneOf(a).root,
    files_subdirectories: a => {
      const source = a.source || paneOf(a).source, n = at(source, a.path);
      if (!n || !n.dir) throw 'No such file or directory';
      return Object.keys(n.dir).filter(k => n.dir[k].dir && !k.startsWith('.')).sort().map(k => ({ title: k, path: join(a.path, k) }));
    },
    files_stamp: ({ source, path }) => { const n = at(source, path); if (!n) throw 'No such file or directory'; return { inode: 1, size: n.size, modified: n.modified }; },
    files_read: ({ source, path }) => {
      const n = at(source, path);
      if (!n || n.dir) throw 'No such file or directory';
      return n.picture ? pictureBytes() : n.text !== undefined ? Array.from(bytesOf(n.text)) : Array.from({ length: 256 }, (_, i) => i);
    },
    files_nav: andChanged(a => {
      const p = paneOf(a), under = x => x === a.path || x.startsWith(a.path + '/');
      switch (a.action) {
        case 'toggle': if (p.open.has(a.path)) p.open.delete(a.path); else p.open.add(a.path); break;
        case 'expand': p.open.add(a.path); break;
        case 'collapse': p.open.delete(a.path); break;
        case 'collapse-all': for (const x of [...p.open]) if (under(x)) p.open.delete(x); break;
        case 'back': if (p.back.length) { p.root = p.back.pop(); p.selected = []; } break;
        case 'up': if (p.root !== '/') { p.back.push(p.root); p.root = parentOf(p.root); p.selected = []; } break;
        case 'toggle-hidden': p.showHidden = !p.showHidden; break;
      }
    }),
    files_set_root: andChanged(a => { const p = paneOf(a), n = at(p.source, a.path); if (!n || !n.dir) throw `${a.path}: No such file or directory`; if (a.path !== p.root) { p.back.push(p.root); p.root = a.path; p.selected = []; } }),
    files_select: andChanged(a => { paneOf(a).selected = a.paths; }),
    files_set_sort: andChanged(a => { paneOf(a).sort = a.sort; }),
    files_set_source: andChanged(a => { const p = paneOf(a); p.source = a.source; p.root = homeOf(a.source); p.back = []; p.open = new Set(); p.selected = []; }),
    files_show_in_window: andChanged(a => { const p = model(a.host).panes.left; if (at(p.source, a.path)) p.root = a.path; }),
    files_create: andChanged(a => { const p = paneOf(a), n = at(p.source, p.root); if (n.dir[a.name]) throw `${a.name}: the name is taken`; n.dir[a.name] = a.directory ? { dir: {}, modified: now() } : file('', now()); }),
    files_rename: andChanged(a => { const p = paneOf(a), up = at(p.source, parentOf(a.path)); if (up.dir[a.name]) throw `${a.name}: the name is taken`; up.dir[a.name] = up.dir[baseOf(a.path)]; delete up.dir[baseOf(a.path)]; p.selected = [join(parentOf(a.path), a.name)]; }),
    files_delete: andChanged(a => { const p = paneOf(a); for (const path of a.paths) { const up = at(p.source, parentOf(path)); if (up && up.dir) delete up.dir[baseOf(path)]; } p.selected = []; }),
    files_copy_selection: a => { const m = model(a.host), from = m.panes[a.pane], to = m.panes[a.pane === 'left' ? 'right' : 'left']; copy(m, from.source, from.selected, to.source, to.root); },
    files_drop: a => {
      const m = model(a.host), to = m.panes[a.pane], onto = a.onto && at(to.source, a.onto);
      const folder = onto ? (onto.dir ? a.onto : parentOf(a.onto)) : to.root;
      if (!a.dragged.length) return;
      const from = a.dragged[0].source, same = JSON.stringify(from) === JSON.stringify(to.source);
      // Dropped where it already is: nothing to copy.
      copy(m, from, a.dragged.map(r => r.path).filter(p => !same || parentOf(p) !== folder), to.source, folder);
    },
    files_download: a => { copy(model(a.host), { kind: 'host', host: a.host }, a.paths, { kind: 'local' }, a.folder || `/Users/${USER}/Downloads`); },
    files_transfer: andChanged(a => {
      const m = model(a.host);
      if (a.action === 'history') m.history = a.on;
      if (a.action === 'clear') m.transfers = m.transfers.filter(t => t.state.kind === 'running' || t.state.kind === 'waiting');
      if (a.action === 'cancel-all-waiting') m.transfers = m.transfers.filter(t => t.state.kind !== 'waiting');
      if (a.action === 'cancel-waiting') m.transfers = m.transfers.filter(t => t.id !== a.id);
      if (a.action === 'cancel' && m.active) { m.active.state = { kind: 'cancelled' }; m.active.endedAt = now(); m.active = null; runCopies(m); }
    }),

    term_open: (a, label) => { const id = ++lastTerm; openShell(id, label, a); return id; },
    term_write: ({ id, data }) => { const sh = terms.get(id); if (sh) sh.input(data); },
    term_resize: ({ id, cols, rows }) => { const sh = terms.get(id); if (sh) sh.resize(cols, rows); },
    term_close: ({ id }) => { terms.delete(id); },
    term_reset: (_, label) => { for (const [id, sh] of terms) if (sh.label === label) terms.delete(id); },
    term_type_password: ({ id }) => { const sh = terms.get(id); if (sh) sh.password(); },
    'plugin:clipboard-manager|read_text': () => '',
    'plugin:clipboard-manager|write_text': ({ text }) => { try { navigator.clipboard.writeText(text).catch(() => {}); } catch { /* not allowed here */ } },
  };

  const api = window.__shellderBackend = {
    lang: 'en',
    desk: null,   // desk.js: open(which, host), close(label), title(label, text), notify(title, body)
    attach(label, emit) {
      const w = { emit };
      wins.set(label, w);
      return {
        // What the page asks; a call the demo has no answer for gets none, as from an app that ignores it.
        invoke: async (cmd, args) => { const f = answers[cmd]; const r = f ? f(args || {}, label) : null; return r === undefined ? null : r; },
        title: text => { if (api.desk) api.desk.title(label, text); },
        close: () => { if (api.desk) api.desk.close(label); },
        detach: () => { if (wins.get(label) === w) wins.delete(label); for (const [id, sh] of terms) if (sh.label === label) terms.delete(id); },
      };
    },
  };
})();
