// The Console window of a host. The column on the left has a tab per
// kind of thing: Status (what the host is doing — uptime and load, CPU,
// memory, swap, GPUs, disks, the busiest processes, who is logged in —
// read every few seconds through its master or its session), Sessions
// (tmux and screen), Jobs (Slurm and PBS), Containers, Ports (what
// listens on the host, VNC servers among it, and ports you add: each
// opens here through a port forward on the master) and Files (the host's
// tree, uploads and downloads; the two-pane window is a click away). The
// rest of the window is terminals in tabs: a shell on the host when the
// window opens, more from the + menu (a shell, a new tmux or screen
// session, an interactive job), and one for every session, container or
// job clicked in the list (xterm.js on the screen pty of an `ssh -t host
// …`). The questions ssh asks never show here: they go down the
// controlling terminal, answered from the keychain or the prompt window.
import { invoke, listen, loadLanguage, t, el, clear, append, icon, fmtBytes, fmtDuration, fmtTime, fmtFileDate, connectState, dialog, toast, menu, commonKeys } from './shellder.js';
import { createFilesView } from './filesview.js';

const host = new URLSearchParams(location.search).get('host');
const tabsBox = document.getElementById('tabs');
const sideBody = document.getElementById('side-body');
const sideFoot = document.getElementById('side-foot');
const termTabs = document.getElementById('term-tabs');
const termBox = document.getElementById('term');
const dark = window.matchMedia('(prefers-color-scheme: dark)');
const INTERVALS = [2, 5, 10, 30];

let snap = null;        // the last reading
let prevTicks = null;   // the CPU ticks of the one before: the difference is the percentage
let cpuPercent = null;
let updatedAt = null;
let error = null;       // why the last reading failed; polling stops until "Try again"
let polling = false;
let timer = null;
let interval = (() => { try { return Number(localStorage.getItem('console-interval')) || 5; } catch { return 5; } })();

// MARK: readings

async function poll() {
  if (polling || document.hidden) return;
  polling = true;
  try {
    const s = await invoke('console_snapshot', { host });
    if (s.cpuTicks && prevTicks) {
      const dt = s.cpuTicks.total - prevTicks.total;
      if (dt > 0) cpuPercent = (s.cpuTicks.busy - prevTicks.busy) / dt * 100;
    } else if (s.cpuPercent !== null) cpuPercent = s.cpuPercent;
    if (s.cpuTicks) prevTicks = s.cpuTicks;
    snap = s;
    error = null;
    updatedAt = Date.now() / 1000;
  } catch (e) {
    error = String(e);
    stop();
  }
  polling = false;
  renderSide();
}

function start() { stop(); poll(); timer = setInterval(poll, interval * 1000); }
function stop() { clearInterval(timer); timer = null; }

// MARK: the Status tab

const pct = frac => Math.max(0, Math.min(100, frac * 100));
const level = frac => frac >= 0.9 ? 'red' : frac >= 0.7 ? 'orange' : '';
const bar = frac => el('div.bar', el('div', { class: level(frac), style: `width:${pct(frac).toFixed(1)}%` }));
const gauge = (label, frac, text) => el('div.gauge', el('div.row', el('span.k', label), el('span.v', text)), bar(frac));
const ofTotal = g => t('{used} of {total}', { used: fmtBytes(g.used), total: fmtBytes(g.total) });
const card = (title, ...rows) => el('div', el('div.card-title', title), el('div.card', rows.map(r => el('div.crow', r))));

function headCard() {
  const running = [];
  if (snap.uptime !== null) running.push(t('up {d}', { d: fmtDuration(snap.uptime) }));
  if (snap.load) running.push(t('load {a} {b} {c}', { a: snap.load[0].toFixed(2), b: snap.load[1].toFixed(2), c: snap.load[2].toFixed(2) }));
  const who = [];
  if (snap.cpus !== null) who.push(t('{n} CPUs', { n: snap.cpus }));
  if (snap.users !== null) who.push(t('{n} logged in', { n: snap.users }));
  return el('div.card', el('div.crow.head',
    el('div.name', snap.hostname || host),
    el('div.caption', [snap.os, snap.kernel !== snap.os ? snap.kernel : null].filter(Boolean).join(' · ')),
    running.length ? el('div.caption', running.join(' · ')) : null,
    who.length ? el('div.caption', who.join(' · ')) : null));
}

function usageCard() {
  const rows = [];
  rows.push(gauge(t('CPU'), cpuPercent === null ? 0 : cpuPercent / 100, cpuPercent === null ? '—' : `${cpuPercent.toFixed(0)}%`));
  if (snap.memory) rows.push(gauge(t('Memory'), snap.memory.used / snap.memory.total, ofTotal(snap.memory)));
  if (snap.swap) rows.push(gauge(t('Swap'), snap.swap.used / snap.swap.total, ofTotal(snap.swap)));
  return card(t('Usage'), ...rows);
}

function gpuCard() {
  if (!snap.gpus.length) return null;
  return card(t('GPUs'), ...snap.gpus.map(g => el('div.gauges',
    el('div.caption', `${g.index} · ${g.name}`),
    gauge(t('Load'), (g.utilization ?? 0) / 100, g.utilization === null ? '—' : `${g.utilization.toFixed(0)}%`),
    g.memory ? gauge(t('Memory'), g.memory.used / g.memory.total, ofTotal(g.memory)) : null)));
}

function diskCard() {
  if (!snap.disks.length) return null;
  return card(t('Disks'), ...snap.disks.map(d => el('div.gauge',
    el('div.row', el('span.k', d.mount), el('span.v', t('{free} free', { free: fmtBytes(d.total - d.used) }))),
    bar(d.used / d.total),
    el('div.row', el('span.caption.k', d.filesystem), el('span.caption.v', ofTotal(d))))));
}

function processCard() {
  if (!snap.processes.length) return null;
  const rows = snap.processes.map(p => el('tr',
    el('td.num', String(p.pid)), el('td', p.user), el('td.num', p.cpu.toFixed(1)), el('td.num', p.memory.toFixed(1)), el('td.cmd', { title: p.command }, p.command)));
  const head = el('tr', el('th', t('PID')), el('th', t('User')), el('th.num', t('CPU %')), el('th.num', t('Mem %')), el('th', t('Command')));
  return el('div', el('div.card-title', t('Busiest processes')), el('div.card', el('div.crow', el('table.procs', head, rows))));
}

function loginCard() {
  if (!snap.logins.length) return null;
  return card(t('Logged in'), ...snap.logins.map(l => el('div.row',
    el('span', l.user), el('span.caption', [l.tty, l.since, l.from].filter(Boolean).join(' · ')))));
}

function renderStatus() {
  const body = el('div.stats');
  if (error) {
    body.append(el('div', el('div.card-title', t('Could not read {host}', { host })), el('div.card', el('div.crow',
      el('div.red.selectable', error),
      el('div', { style: 'margin-top:8px' }, el('button', { onclick: () => { error = null; renderSide(); start(); } }, t('Try again')))))));
  } else if (!snap) {
    body.append(el('div.caption', t('Reading {host}…', { host })));
  } else {
    append(body, [headCard(), usageCard(), gpuCard(), diskCard(), processCard(), loginCard()]);
  }
  sideBody.append(body);
  const select = el('select', { onchange: e => {
    interval = Number(e.target.value);
    try { localStorage.setItem('console-interval', String(interval)); } catch {}
    if (!error) start();
  } }, INTERVALS.map(n => el('option', { value: n, selected: n === interval }, t('every {n}s', { n }))));
  sideFoot.append(
    el('span', updatedAt ? t('Updated {time}', { time: fmtTime(updatedAt) }) : ''),
    el('div.spacer'),
    select,
    el('button', { title: t('Read it again now (⌘R)'), onclick: () => { if (error) { error = null; start(); } else poll(); } }, icon('refresh')));
}

// MARK: the session rows

function shellWord(s) { return `'${s.replace(/'/g, "'\\''")}'`; }

/// The terminal tab this is open in, if any.
const tabOf = (kind, name) => terms.find(x => x.kind === kind && x.name === name && !x.ended);

/// A row that opens something in a terminal tab when clicked (or brings
/// up the tab it is already open in). `shown` is the name on the row when
/// it is not the id; `rename` a function that asks for another name;
/// `path` the directory it is in.
function row(kind, id, command, title, caption, { shown, rename, end, path, extra } = {}) {
  const open = tabOf(kind, id);
  return el('div.session', { class: open ? 'current' : '', title: open ? t('Show its tab') : title, onclick: () => open ? select(open) : openTab(kind, id, command) },
    icon(open ? 'play' : 'terminal', open ? 'accent' : ''),
    el('div.text',
      el('div.row', el('span.name', shown ?? id), el('div.spacer'),
        rename ? el('button.rename', { title: t('Rename this session'), onclick: e => { e.stopPropagation(); rename(); } }, icon('pencil')) : null,
        end ? el('button.rename', { title: t('End this session and everything in it'), onclick: e => { e.stopPropagation(); end(); } }, icon('xcircle')) : null),
      caption ? el('div.caption', caption) : null,
      path ? el('div.path', { title: path }, path) : null,
      extra));
}

/// A row for something that cannot be entered (a job still queued).
function inertRow(name, caption, title) {
  return el('div.session.inert', { title }, icon('clock'), el('div.text', el('div.row', el('span.name', name)), el('div.caption', caption)));
}

async function rename(kind, id, current, after) {
  const r = await dialog({ title: t('Rename session'), preset: current, input: true, action: t('Rename') });
  if (!r.ok || r.value === current) return;
  try {
    await invoke('console_rename', { host, kind, id, name: r.value });
    const open = tabOf(kind, id);
    if (open) { open.name = after(r.value); renderTerms(); }
    poll();
  } catch (e) { toast(String(e), true); }
}

async function endSession(kind, id, name) {
  const r = await dialog({ title: t('End the {kind} session “{name}”?', { kind, name }), message: t('Everything running in it ends with it.'), action: t('End'), destructive: true });
  if (!r.ok) return;
  try { await invoke('console_kill', { host, kind, id }); poll(); } catch (e) { toast(String(e), true); }
}

function tmuxRow(s) {
  const facts = ['tmux', s.windows.length === 1 ? t('1 window') : t('{n} windows', { n: s.windows.length }), s.attached ? t('{n} attached', { n: s.attached }) : null,
    s.activity ? t('active {when}', { when: fmtFileDate(s.activity) }) : null].filter(Boolean).join(' · ');
  const windows = el('div.windows', s.windows.map(w => el('span.win', { class: w.active ? 'active' : '', title: [w.path, w.panes > 1 ? t('{n} panes', { n: w.panes }) : null].filter(Boolean).join(' · ') }, `${w.index}:${w.name}${w.command && w.command !== w.name ? ' (' + w.command + ')' : ''}`)));
  return row('tmux', s.name, `tmux attach-session -t ${shellWord('=' + s.name)}`, t('Open this session in a tab'), facts,
    { path: s.path, extra: windows, rename: () => rename('tmux', s.name, s.name, v => v), end: () => endSession('tmux', s.name, s.name) });
}

function screenRow(s) {
  return row('screen', s.id, `screen -x ${shellWord(s.id)}`, t('Open this session in a tab'), ['screen', t('pid {pid}', { pid: s.pid }), s.attached ? t('attached') : t('detached')].join(' · '),
    { shown: s.name, path: s.path, rename: () => rename('screen', s.id, s.name, v => `${s.pid}.${v}`), end: () => endSession('screen', s.id, s.name) });
}

function containerRow(c) {
  const tool = snap.docker.tool;
  const shell = shellWord('command -v bash >/dev/null 2>&1 && exec bash || exec sh');
  return row(tool, c.name, `${tool} exec -it ${shellWord(c.name)} sh -c ${shell}`, t('Open a shell in this container ({tool} exec)', { tool }), [c.image, c.status, c.ports].filter(Boolean).join(' · '));
}

const nodesText = n => n ? (n === 1 ? t('1 node') : t('{n} nodes', { n })) : null;

function slurmRow(j) {
  const facts = ['Slurm', j.state, j.time !== '0:00' ? j.time : null, nodesText(j.nodes), j.reason, j.partition].filter(Boolean).join(' · ');
  if (j.state !== 'RUNNING') return inertRow(`${j.id} ${j.name}`, facts, t('Only a running job can be entered'));
  return row('slurm', j.id, `srun --jobid=${shellWord(j.id)} --overlap --pty bash`, t('Open a shell on the nodes of this job (srun --jobid --overlap --pty)'), facts, { shown: `${j.id} ${j.name}` });
}

function pbsRow(j) {
  const facts = ['PBS', j.state, j.time, nodesText(j.nodes), j.node, j.queue].filter(Boolean).join(' · ');
  if (j.state !== 'R' || !j.node) return inertRow(`${j.id} ${j.name}`, facts, t('Only a running job can be entered'));
  return row('pbs', j.id, `ssh -t ${shellWord(j.node)}`, t('Open a shell on the first node of this job (ssh from the login node)'), facts, { shown: `${j.id} ${j.name}` });
}

// MARK: the Ports tab

const portsKey = `console-ports-${host}`;
function customPorts() { try { return JSON.parse(localStorage.getItem(portsKey) || '[]'); } catch { return []; } }
function saveCustomPorts(list) { try { localStorage.setItem(portsKey, JSON.stringify(list)); } catch {} }

async function openPort(port, url) {
  try { await invoke('console_open_port', { host, port, url }); } catch (e) { toast(String(e), true); }
}

/// A row that opens what listens on `port` here, through a port forward:
/// in the VNC viewer when a VNC server is on it, in the browser otherwise.
function portRow(port, caption, vnc, onRemove) {
  const url = vnc ? 'vnc://127.0.0.1:{port}' : 'http://127.0.0.1:{port}/';
  const title = vnc ? t('Open :{port} in your VNC viewer, through a port forward on the master', { port })
    : t('Open :{port} in your browser, through a port forward on the master', { port });
  return el('div.session', { title, onclick: () => openPort(port, url) },
    icon(vnc ? 'laptop' : 'network'),
    el('div.text', el('div.row', el('span.name', `:${port}`)), el('div.caption', caption)),
    onRemove ? el('button.borderless', { title: t('Forget this port'), onclick: e => { e.stopPropagation(); onRemove(); } }, icon('close')) : null,
    el('button', { title, onclick: e => { e.stopPropagation(); openPort(port, url); } }, icon('link'), t('Open')));
}

/// The rows: the listeners with a known program, the VNC servers, and
/// the ports added by hand. A listener nobody can name is not shown,
/// but offered by "Add port".
function portRows() {
  const vnc = new Map(snap.vnc.map(v => [v.port, v]));
  const custom = customPorts();
  const seen = new Set();
  const rows = [];
  for (const l of snap.listeners) {
    if (l.port === 22 || l.process === 'sshd') continue;
    const v = vnc.get(l.port);
    if (!v && !l.process && !custom.includes(l.port)) continue;
    seen.add(l.port);
    const what = v ? [`${v.program} :${v.display ?? ''}`.trim(), v.user] : [l.process ? `${l.process}${l.pid ? ' (' + l.pid + ')' : ''}` : null];
    rows.push(portRow(l.port, [...what, l.address].filter(Boolean).join(' · '), !!v, custom.includes(l.port) ? () => { saveCustomPorts(custom.filter(x => x !== l.port)); renderSide(); } : null));
  }
  for (const v of snap.vnc) if (!seen.has(v.port)) { seen.add(v.port); rows.push(portRow(v.port, [`${v.program} :${v.display ?? ''}`.trim(), v.user].join(' · '), true)); }
  for (const port of custom) if (!seen.has(port)) {
    rows.push(portRow(port, t('added by you'), vnc.has(port), () => { saveCustomPorts(custom.filter(x => x !== port)); renderSide(); }));
  }
  return rows;
}

function addPortNumber(port) {
  const list = customPorts();
  if (!list.includes(port)) saveCustomPorts([...list, port].sort((a, b) => a - b));
  renderSide();
}

/// The Add menu: the listeners not in the list (nobody named them), then
/// any port typed in.
function addPortMenu(anchor) {
  const custom = customPorts();
  const unnamed = snap.listeners.filter(l => l.port !== 22 && l.process !== 'sshd' && !l.process && !custom.includes(l.port) && !snap.vnc.some(v => v.port === l.port));
  const items = unnamed.map(l => ({ title: `:${l.port} · ${l.address}`, icon: 'network', run: () => addPortNumber(l.port) }));
  if (items.length) items.push({ sep: true });
  items.push({ title: t('Any port…'), icon: 'plus', run: addPort });
  menu(anchor, items);
}

async function addPort() {
  const r = await dialog({ title: t('Forward a port'), message: t('A port on {host} to reach from here. It opens in your browser, or in the VNC viewer when a VNC server listens on it.', { host }), input: true, action: t('Add') });
  if (!r.ok) return;
  const port = Number(r.value);
  if (!Number.isInteger(port) || port < 1 || port > 65535) { toast(t('A port is a number from 1 to 65535.'), true); return; }
  addPortNumber(port);
}

// MARK: the Files tab

let filesView = null;
let filesBox = null;
let filesOpened = false;

function filesTab() {
  if (!filesBox) {
    filesBox = el('div.filesbox');
    filesView = createFilesView(filesBox, { host, single: true, transferHeight: 120, onUpload: upload, onDownload: download });
    listen(`files:files-${host.replace(/[^A-Za-z0-9_-]/g, '_')}`, s => filesView.render(s));
  }
  sideBody.classList.add('fill');
  sideBody.append(filesBox);
  if (!filesOpened) {
    filesOpened = true;
    invoke('files_open', { host }).then(s => { filesView.render(s); filesView.nativeDrops(); }).catch(e => toast(String(e), true));
  }
  sideFoot.append(
    el('span.caption', t('Right-click: upload here, download what is selected')),
    el('div.spacer'),
    el('button', { title: t('The Files window: two trees side by side — {alias}, this machine, or another host in either one, a drag copies', { alias: host }), onclick: () => invoke('open_window', { which: 'files', host }) }, icon('link'), t('Window')));
}

const picker = () => window.__TAURI__.dialog;

/// Files picked on this machine go into `dir` on the host.
async function upload(dir) {
  const k = filesView && filesView.hostPane();
  if (!k) return;
  const paths = await picker().open({ multiple: true, title: t('Upload to {host}:{dir}', { host, dir }) });
  if (!paths || !paths.length) return;
  await invoke('files_drop_local', { host, pane: k, paths, onto: dir }).catch(e => toast(String(e), true));
}

/// `paths` on the host go to a folder picked on this machine.
async function download(paths) {
  const s = filesView && filesView.snapshot();
  const k = filesView && filesView.hostPane();
  if (!s || !k || !paths.length) return;
  const folder = await picker().open({ directory: true, title: t('Download to') });
  if (!folder) return;
  const other = k === 'left' ? 'right' : 'left';
  try {
    if (s.panes[other].source.kind !== 'local') {
      await invoke('files_set_source', { host, pane: other, source: { kind: 'local' } });
      // The pane lists its home on a thread of its own; the folder goes in after.
      for (let i = 0; i < 30; i++) {
        const now = await invoke('files_state', { host });
        if (now.panes[other].root) break;
        await new Promise(r => setTimeout(r, 100));
      }
    }
    await invoke('files_set_root', { host, pane: other, path: folder });
    await invoke('files_select', { host, pane: k, paths });
    await invoke('files_copy_selection', { host, pane: k });
  } catch (e) { toast(String(e), true); }
}

// MARK: the column

const sideKey = `console-tab-${host}`;
let side = (() => { try { return localStorage.getItem(sideKey) || 'status'; } catch { return 'status'; } })();

/// The tabs of the column: Status, Sessions, then Jobs and Containers
/// where the host has them, Ports, Files.
function sideTabs() {
  const out = [{ key: 'status', title: t('Status'), icon: 'gauge' }];
  if (!snap) return out;
  out.push({ key: 'files', title: t('Files'), icon: 'folder' });
  const sessions = [...(snap.tmux || []).map(x => ({ kind: 'tmux', x })), ...(snap.screen || []).map(x => ({ kind: 'screen', x }))];
  out.push({ key: 'sessions', title: t('Sessions'), icon: 'terminal', count: sessions.length, rows: () => sessions.map(s => s.kind === 'tmux' ? tmuxRow(s.x) : screenRow(s.x)),
    empty: snap.tmux === null && snap.screen === null ? t('Neither tmux nor screen is installed on {host}.', { host }) : t('No tmux or screen session is running. The + above the terminals starts one; it survives this window.') });
  if (snap.slurm !== null || snap.pbs !== null) {
    const jobs = [...(snap.slurm || []).map(x => ({ kind: 'slurm', x })), ...(snap.pbs || []).map(x => ({ kind: 'pbs', x }))];
    out.push({ key: 'jobs', title: t('Jobs'), icon: 'clock', count: jobs.length, rows: () => jobs.map(j => j.kind === 'slurm' ? slurmRow(j.x) : pbsRow(j.x)), empty: t('No job of yours is queued or running. The + above the terminals starts an interactive one.') });
  }
  if (snap.docker !== null) {
    out.push({ key: 'containers', title: t('Containers'), icon: 'box', count: snap.docker.list.length, rows: () => snap.docker.list.map(containerRow), empty: t('No container is running, or {tool} is not usable by this account.', { tool: snap.docker.tool }) });
  }
  const ports = portRows();
  out.push({ key: 'ports', title: t('Ports'), icon: 'network', count: ports.length, rows: () => ports, empty: t('Nothing listens on {host} that this account can see. Add a port to forward it anyway.', { host }), foot: () => [el('div.spacer'), el('button', { title: t('Forward a port of {host} and open it here', { host }), onclick: e => addPortMenu(e.currentTarget) }, icon('plus'), t('Add port…'))] });
  return out;
}

function renderSide() {
  clear(tabsBox); clear(sideBody); clear(sideFoot);
  sideBody.classList.remove('fill');
  const all = sideTabs();
  const current = all.find(x => x.key === side) || all[0];
  for (const x of all) {
    // One row: an icon and a count per tab, the name only on the one open.
    tabsBox.append(el('button.tab', { class: x === current ? 'active' : '', title: x.title, 'aria-label': x.title, onclick: () => { side = x.key; try { localStorage.setItem(sideKey, side); } catch {} renderSide(); } },
      icon(x.icon), x === current ? el('span.label', x.title) : null, x.count ? el('span.count', String(x.count)) : null));
  }
  if (current.key === 'status') { renderStatus(); return; }
  if (current.key === 'files') { filesTab(); return; }
  const rows = current.rows();
  if (!rows.length) sideBody.append(el('div.note', current.empty || ''));
  else append(sideBody, rows);
  if (current.foot) append(sideFoot, current.foot());
}

async function newSession(kind) {
  const r = await dialog({ title: t('New {kind} session', { kind }), message: t('A name for it; {kind} numbers it when you leave this empty.', { kind }), input: true, action: t('Open') });
  if (!r.ok) return;
  const name = r.value;
  const command = kind === 'tmux' ? (name ? `tmux new-session -s ${shellWord(name)}` : 'tmux new-session')
    : (name ? `screen -S ${shellWord(name)}` : 'screen');
  openTab(kind, name || t('new session'), command);
  setTimeout(poll, 1500);
}

/// What + offers: a shell, and whatever else the host can start.
function plusMenu(anchor) {
  const items = [{ title: t('New shell'), icon: 'terminal', run: () => openTab('shell', '', null) }];
  if (snap && snap.tmux !== null) items.push({ title: t('New tmux session…'), icon: 'plus', run: () => newSession('tmux') });
  if (snap && snap.screen !== null) items.push({ title: t('New screen session…'), icon: 'plus', run: () => newSession('screen') });
  if (snap && snap.slurm !== null) items.push({ title: t('Interactive Slurm job (srun --pty bash)'), icon: 'clock', run: () => openTab('slurm', t('interactive'), 'srun --pty bash -l') });
  if (snap && snap.pbs !== null) items.push({ title: t('Interactive PBS job (qsub -I)'), icon: 'clock', run: () => openTab('pbs', t('interactive'), 'qsub -I') });
  menu(anchor, items);
}

// MARK: the terminal tabs

function theme() {
  return dark.matches
    ? { background: '#1e1e1e', foreground: '#f2f2f2', cursor: '#f2f2f2', selectionBackground: 'rgba(64,156,255,0.35)' }
    : { background: '#ffffff', foreground: '#1d1d1f', cursor: '#1d1d1f', selectionBackground: 'rgba(10,132,255,0.25)' };
}

/// The bytes of a base64 string, as ssh wrote them.
function decode(b64) {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

const terms = [];   // {kind, name, command, id, term, fit, box, ended}
let active = null;

const titleOf = x => x.kind === 'shell' ? t('Shell') : `${x.kind}: ${x.name}`;

/// A new tab running `command` on the host (the login shell without one).
function openTab(kind, name, command) {
  const term = new Terminal({
    fontFamily: 'ui-monospace, "SF Mono", Menlo, Consolas, monospace',
    fontSize: 13,
    lineHeight: 1.15,
    cursorBlink: true,
    scrollback: 10000,
    macOptionIsMeta: true,
    theme: theme(),
  });
  const fit = new FitAddon.FitAddon();
  term.loadAddon(fit);
  const box = el('div.termbox');
  termBox.append(box);
  term.open(box);
  const x = { kind, name, command, id: null, term, fit, box, ended: true };
  terms.push(x);
  term.onData(data => { if (!x.ended) invoke('term_write', { id: x.id, data }).catch(() => {}); else if (data === '\r') connect(x); });
  // Binary input (a paste that is not text): bytes as a latin1 string.
  term.onBinary(data => { if (!x.ended) invoke('term_write', { id: x.id, data }).catch(() => {}); });
  term.onResize(({ cols, rows }) => { if (!x.ended) invoke('term_resize', { id: x.id, cols, rows }).catch(() => {}); });
  // ⌘C copies a selection (otherwise it is the interrupt it always was),
  // ⌘V pastes, ⌘K clears, ⌘A selects all; any other ⌘ key is the
  // window's (⌘W, ⌘T, ⌘R, ⌘,, ⌘Q) and goes past the terminal to the page.
  term.attachCustomKeyEventHandler(e => {
    if (e.type !== 'keydown' || !e.metaKey) return true;
    if (e.key === 'c' && term.hasSelection()) { navigator.clipboard.writeText(term.getSelection()); return false; }
    if (e.key === 'v') { navigator.clipboard.readText().then(text => { if (text) term.paste(text); }); return false; }
    if (e.key === 'k') { term.clear(); return false; }
    if (e.key === 'a') { term.selectAll(); return false; }
    return false;
  });
  select(x);
  connect(x);
  return x;
}

async function connect(x) {
  x.ended = false;
  x.term.reset();
  x.fit.fit();
  x.term.write(`\x1b[2m${t('Connecting to {host}…', { host })}\x1b[0m\r\n`);
  renderTerms();
  try {
    x.id = await invoke('term_open', { host, command: x.command, cols: x.term.cols, rows: x.term.rows });
    if (x === active) x.term.focus();
  } catch (e) {
    x.ended = true;
    x.term.write(`\r\n\x1b[31m${String(e)}\x1b[0m\r\n`);
    renderTerms();
  }
  renderSide();
}

function select(x) {
  active = x;
  for (const y of terms) y.box.classList.toggle('hidden', y !== x);
  renderTerms();
  requestAnimationFrame(() => { x.fit.fit(); x.term.focus(); });
  renderSide();
}

/// Close the tab: hang up (a tmux client detaches, the session lives on).
function closeTab(x) {
  if (!x.ended && x.id !== null) invoke('term_close', { id: x.id }).catch(() => {});
  const i = terms.indexOf(x);
  terms.splice(i, 1);
  x.term.dispose();
  x.box.remove();
  if (active === x) {
    active = null;
    if (terms.length) select(terms[Math.min(i, terms.length - 1)]);
    else { renderTerms(); renderSide(); }
  } else renderTerms();
  setTimeout(poll, 500);
}

function renderTerms() {
  clear(termTabs);
  for (const x of terms) {
    termTabs.append(el('div.ttab', { class: (x === active ? 'active' : '') + (x.ended ? ' ended' : ''), title: x.ended ? t('{name} is closed. Return in it connects again.', { name: titleOf(x) }) : x.command || `ssh ${host}`, onclick: () => select(x) },
      icon(x.kind === 'shell' ? 'terminal' : 'play'), el('span.label', titleOf(x)),
      el('button.close', { title: x.ended ? t('Close') : t('Close this tab: hang up on it (a session lives on)'), onclick: e => { e.stopPropagation(); closeTab(x); } }, icon('close'))));
  }
  termTabs.append(el('button.add', { title: t('A new shell on {host} (⌘T), a new tmux or screen session, an interactive job', { host }), onclick: e => plusMenu(e.currentTarget) }, icon('plus')));
  let empty = termBox.querySelector('.empty');
  if (!terms.length) {
    if (!empty) termBox.append(empty = el('div.empty'));
    clear(empty).append(icon('terminal'), el('div', t('No tab is open: press + for a shell, or pick a session in the list.')));
  } else if (empty) empty.remove();
}

async function init() {
  const s = await connectState();
  await loadLanguage(s.settings.language);
  document.title = t('Console — {host}', { host });
  document.getElementById('title').textContent = document.title;
  dark.addEventListener('change', () => { for (const x of terms) x.term.options.theme = theme(); });
  await listen('term-data', ({ id, data }) => { const x = terms.find(y => y.id === id); if (x) x.term.write(decode(data)); });
  await listen('term-exit', ({ id, code }) => {
    const x = terms.find(y => y.id === id);
    if (!x) return;
    x.ended = true;
    // A clean end (exit, a tmux or screen detach) closes the tab; any
    // other leaves its last words on screen, Return tries again.
    if (code === 0) { closeTab(x); return; }
    x.term.write(`\r\n\x1b[2m[${t('{name} is closed (exit status {code}). Return connects again.', { name: titleOf(x), code })}]\x1b[0m\r\n`);
    renderTerms();
    renderSide();
    setTimeout(poll, 500);
  });
  new ResizeObserver(() => { if (active) active.fit.fit(); }).observe(termBox);
  commonKeys();
  document.addEventListener('keydown', e => {
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.key === 'r') { e.preventDefault(); if (error) { error = null; start(); } else poll(); }
    if (mod && e.key === 't') { e.preventDefault(); openTab('shell', '', null); }
  });
  document.addEventListener('visibilitychange', () => { if (document.hidden) stop(); else if (!error) start(); });
  renderSide();
  renderTerms();
  start();
  openTab('shell', '', null);
}

init();
