// The Console window of a host. The column on the left has a tab per
// kind of thing: Status (what the host is doing — uptime and load, CPU,
// memory, swap, GPUs, disks (a removable drive's volumes mount, unmount
// and eject from here), the busiest processes, who is logged in —
// read every few seconds through its master or its session), Sessions
// (tmux and screen), Jobs (Slurm and PBS), Containers, Ports (what
// listens on the host, VNC servers among it, and ports you add: each
// opens here through a port forward on the master) and Files (the host's
// tree, uploads and downloads; the two-pane window is a click away). The
// rest of the window is tabs: terminals, a shell on the host when the
// window opens, more from the + menu (a shell, a new tmux or screen
// session, an interactive job), and one for every session, container or
// job clicked in the list (xterm.js on the screen pty of an `ssh -t host
// …`); and a preview for every file opened from the Files tab (text,
// Markdown, a PDF, an image: preview.js). The questions ssh asks never
// show here: they go down the controlling terminal, answered from the
// keychain or the prompt window.
import { invoke, listen, loadLanguage, t, el, clear, append, icon, fmtBytes, fmtDuration, fmtTime, fmtFileDate, connectState, currentState, dialog, toast, menu, commonKeys } from './shellder.js';
import { createFilesView } from './filesview.js';
import { createPreview } from './preview.js';

const host = new URLSearchParams(location.search).get('host');
const tabsBox = document.getElementById('tabs');
const sideBody = document.getElementById('side-body');
const termTabs = document.getElementById('term-tabs');
const termBox = document.getElementById('term');
const termWrap = termBox.parentElement;
const sideEl = tabsBox.parentElement;
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
let sideHidden = (() => { try { return localStorage.getItem('console-side-hidden') === '1'; } catch { return false; } })();

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
  if (side === 'ports') { await refreshForwards(); return; }
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
  return el('div', el('div.card-title', t('Usage'), el('div.spacer'), ...refreshControls()), el('div.card', rows.map(r => el('div.crow', r))));
}

/// When the readings were taken, how often they are, and a button for now.
function refreshControls() {
  const select = el('select', { title: t('How often the readings are taken'), onchange: e => {
    interval = Number(e.target.value);
    try { localStorage.setItem('console-interval', String(interval)); } catch {}
    if (!error) start();
  } }, INTERVALS.map(n => el('option', { value: n, selected: n === interval }, t('every {n}s', { n }))));
  return [
    el('span.caption', updatedAt ? fmtTime(updatedAt) : ''),
    select,
    el('button.act', { title: t('Read it again now (⌘R)'), onclick: () => { if (error) { error = null; start(); } else poll(); } }, icon('refresh'))];
}

function gpuCard() {
  if (!snap.gpus.length) return null;
  return card(t('GPUs'), ...snap.gpus.map(g => el('div.gauges',
    el('div.caption', `${g.index} · ${g.name}`),
    gauge(t('Load'), (g.utilization ?? 0) / 100, g.utilization === null ? '—' : `${g.utilization.toFixed(0)}%`),
    g.memory ? gauge(t('Memory'), g.memory.used / g.memory.total, ofTotal(g.memory)) : null)));
}

/// Mount or unmount a volume, or eject a drive, on the host; the host's
/// own words (where udisks put it) or `said` in a toast, and a reading
/// right after. A host that wants a password for it (udisks asks polkit,
/// which over ssh has nobody to ask; sudo) gets a terminal tab, where it
/// asks; the tab closes by itself when it is done.
async function diskAction(action, device, said) {
  try {
    const r = await invoke('console_disk', { host, action, device });
    if (r.ask) { toast(t('{host} wants a password for this: it asks in the tab.', { host })); openTab(action, device, r.ask); return; }
    toast(r.said.split('\n')[0] || said);
  } catch (e) { toast(String(e), true); }
  poll();
}

/// The buttons of a volume on a removable drive: mount or unmount it,
/// and eject the drive it is on.
function diskButtons(device, drive, mounted) {
  const out = [mounted
    ? el('button.act', { title: t('Unmount {device}', { device }), onclick: () => diskAction('unmount', device, t('{device} unmounted', { device })) }, icon('unplug'))
    : el('button.act', { title: t('Mount {device}', { device }), onclick: () => diskAction('mount', device, t('{device} mounted', { device })) }, icon('plug'))];
  if (drive) out.push(el('button.act', { title: t('Eject {drive}: unmount everything on it and power it off', { drive }), onclick: () => diskAction('eject', drive, t('{drive} ejected', { drive })) }, icon('eject')));
  return out;
}

function diskCard() {
  if (!snap.disks.length && !snap.volumes.length) return null;
  const rows = snap.disks.map(d => el('div.gauge',
    el('div.row', el('span.k', d.mount), el('span.v', t('{free} free', { free: fmtBytes(d.total - d.used) })), d.removable ? diskButtons(d.filesystem, d.drive, true) : null),
    bar(d.used / d.total),
    el('div.row', el('span.caption.k', d.filesystem), el('span.caption.v', ofTotal(d)))));
  // Volumes on removable drives that are not mounted: a mount button, no bar.
  for (const v of snap.volumes) rows.push(el('div.gauge',
    el('div.row', el('span.k', v.label || v.device), el('span.v', t('not mounted')), diskButtons(v.device, v.drive, false)),
    el('div.row', el('span.caption.k', [v.device, v.fstype].filter(Boolean).join(' · ')), el('span.caption.v', fmtBytes(v.size)))));
  return card(t('Disks'), ...rows);
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
        rename ? el('button.act', { title: t('Rename this session'), onclick: e => { e.stopPropagation(); rename(); } }, icon('pencil')) : null,
        end ? el('button.act', { title: t('End this session and everything in it'), onclick: e => { e.stopPropagation(); end(); } }, icon('xcircle')) : null),
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

// What the user set up, kept on this machine: the forwards (a port left
// null is picked when the forward is made; `got` is what it came to),
// and which of the host's own ports to list (null: those with a VNC
// server).
const fwdKey = `console-forwards-${host}`;
const showKey = `console-show-ports-${host}`;
function savedForwards() { try { return JSON.parse(localStorage.getItem(fwdKey) || '[]'); } catch { return []; } }
function saveForwards(list) { try { localStorage.setItem(fwdKey, JSON.stringify(list)); } catch {} }
function shownPorts() { try { const v = localStorage.getItem(showKey); return v === null ? null : JSON.parse(v); } catch { return null; } }
function saveShownPorts(list) { try { localStorage.setItem(showKey, JSON.stringify(list)); } catch {} }

let activeForwards = [];  // what the master has up, as last asked

async function refreshForwards() {
  try { activeForwards = await invoke('console_forwards', { host }); } catch { activeForwards = []; }
  if (side === 'ports') renderSide();
}

/// The live forward a saved one corresponds to, if it is up.
function liveOf(def) {
  const l = def.local ?? (def.got && def.got.local);
  const r = def.remote ?? (def.got && def.got.remote);
  return activeForwards.find(f => f.direction === def.direction && f.local === l && f.remote === r) || null;
}

/// Make the forward on the master; `open` opens its near end here once
/// it is up. Returns the live forward, or null when it could not be made.
async function startForward(def, open = false) {
  let f = null;
  try {
    f = await invoke('console_forward', { host, direction: def.direction, local: def.local, remote: def.remote });
    const list = savedForwards();
    const i = list.findIndex(x => x.direction === def.direction && x.local === def.local && x.remote === def.remote);
    if (i >= 0) { list[i].got = { local: f.local, remote: f.remote }; saveForwards(list); }
    if (open) openLocal(f);
  } catch (e) { toast(String(e), true); }
  refreshForwards();
  return f;
}

/// A click on one of the host's ports: a forward to it (made now if there
/// is none, kept with the others) and its near end opened here.
async function forwardAndOpen(port) {
  const list = savedForwards();
  let def = list.find(x => x.direction === 'local' && x.remote === port);
  if (!def) { def = { direction: 'local', local: null, remote: port }; saveForwards([...list, def]); }
  const live = liveOf(def);
  if (live) openLocal(live); else await startForward(def, true);
}

/// Open the port at the host's own address (the HostName of its ssh
/// config), with no tunnel: for a host reachable directly.
function openDirect(port) {
  const s = currentState();
  const r = s && s.resolved && s.resolved[host];
  const address = (r && r.hostname) || host;
  const url = isVncPort(port) ? `vnc://${address}:${port}` : `http://${address}:${port}/`;
  invoke('open_url', { url }).catch(e => toast(String(e), true));
}

async function stopForward(def) {
  const f = liveOf(def);
  if (f) { try { await invoke('console_cancel_forward', { host, forward: f }); } catch (e) { toast(String(e), true); } }
  refreshForwards();
}

function removeForward(def) {
  stopForward(def);
  saveForwards(savedForwards().filter(x => !(x.direction === def.direction && x.local === def.local && x.remote === def.remote)));
  renderSide();
}

/// A port the VNC viewer opens: one a VNC server the host named has, or,
/// for a port nothing names (one added by hand, say), the range VNC
/// servers use, 5900 plus the display.
const isVncPort = port => snap.vnc.some(v => v.port === port)
  || (port >= 5900 && port <= 5999 && !snap.listeners.some(l => l.port === port && l.process && !/vnc/i.test(l.process)));

/// Open a local forward's near end here: the VNC viewer when a VNC server
/// is at the far end, the browser otherwise.
function openLocal(f) {
  const url = isVncPort(f.remote) ? `vnc://127.0.0.1:${f.local}` : `http://127.0.0.1:${f.local}/`;
  invoke('open_url', { url }).catch(e => toast(String(e), true));
}

function forwardRow(def) {
  const live = liveOf(def);
  const l = def.local ?? (live && live.local) ?? (def.got && def.got.local);
  const r = def.remote ?? (live && live.remote) ?? (def.got && def.got.remote);
  const here = l ? `127.0.0.1:${l}` : t('a port here');
  const there = r ? `${host}:${r}` : t('a port on {host}', { host });
  const name = def.direction === 'local' ? `${here} → ${there}` : `${there} → ${here}`;
  const caption = live ? t('up') : t('not up');
  // A click opens the near end here (a -L forward, made first when it is
  // not up); a -R forward has nothing to open here, a click makes it.
  const title = def.direction === 'local'
    ? t('A port here reaches a port on the host (-L). Click: open it here, made first when it is not up.')
    : t('A port on the host reaches a port here (-R). Click: make it when it is not up.');
  const act = () => { if (def.direction === 'local') { if (live) openLocal(live); else startForward(def, true); } else if (!live) startForward(def); };
  return el('div.session', { class: live ? '' : 'inert', title, onclick: act },
    icon(def.direction === 'local' ? 'arrowright' : 'arrowleft'),
    el('div.text', el('div.row', el('span.name', name)), el('div.caption', caption)),
    live ? el('button.act', { title: t('Stop: ssh -O cancel'), onclick: e => { e.stopPropagation(); stopForward(def); } }, icon('stop'))
      : el('button.act', { title: t('Start: ssh -O forward on the master'), onclick: e => { e.stopPropagation(); startForward(def); } }, icon('play')),
    el('button.act', { title: t('Forget this forward'), onclick: e => { e.stopPropagation(); removeForward(def); } }, icon('close')));
}

function hostPortRow(l) {
  const v = snap.vnc.find(x => x.port === l.port);
  const what = v ? [v.display == null ? v.program : `${v.program} :${v.display}`, v.user]
    : [l.process ? `${l.process}${l.pid ? ' (' + l.pid + ')' : ''}` : (l.added ? t('added by hand') : null)];
  const s = currentState();
  const address = (s && s.resolved && s.resolved[host] && s.resolved[host].hostname) || host;
  return el('div.session', { title: t('Open it here through a forward on the master, made now if there is none'), onclick: () => forwardAndOpen(l.port) },
    icon(isVncPort(l.port) ? 'laptop' : 'network'),
    el('div.text', el('div.row', el('span.name', `:${l.port}`)), el('div.caption', [...what, l.address].filter(Boolean).join(' · '))),
    el('button.act', { title: t('Open {address}:{port} directly, with no tunnel', { address, port: l.port }), onclick: e => { e.stopPropagation(); openDirect(l.port); } }, icon('globe')),
    el('button.act', { title: t('Take this port off the list'), onclick: e => { e.stopPropagation(); hidePort(l.port); } }, icon('close')));
}

/// The host's own listeners worth a row.
function hostListeners() { return snap.listeners.filter(l => l.port !== 22 && l.process !== 'sshd'); }
/// What is listed while nothing was chosen: the VNC servers' ports.
function defaultPorts() { return [...new Set(snap.vnc.map(v => v.port))]; }
function listedPorts() { return shownPorts() ?? defaultPorts(); }
/// A row per listed port: the listener on it when this account sees
/// one, the VNC server there otherwise, or just the number for a port
/// added by hand.
function shownListeners() {
  const all = hostListeners();
  return listedPorts().map(port => {
    const l = all.find(x => x.port === port);
    if (l) return l;
    const v = snap.vnc.find(x => x.port === port);
    return v ? { port, address: '', process: v.program, pid: v.pid } : { port, address: '', process: '', pid: null, added: true };
  });
}
function addPort(port) { const list = listedPorts(); if (!list.includes(port)) saveShownPorts([...list, port]); renderSide(); }
function hidePort(port) { saveShownPorts(listedPorts().filter(p => p !== port)); renderSide(); }

/// The + on the label: the listeners this account sees and does not list
/// yet, and any port by hand, for what it cannot see (a service of
/// root's, say: lsof on macOS names only your own processes' ports).
function addMenu(anchor) {
  const listed = listedPorts();
  const items = hostListeners().filter(l => !listed.includes(l.port))
    .map(l => ({ title: `:${l.port}${l.process ? ' · ' + l.process : ''}${l.address ? ' · ' + l.address : ''}`, run: () => addPort(l.port) }));
  if (!items.length) items.push({ title: t('Nothing else listens that this account can see'), run: () => {} });
  items.push({ sep: true }, { title: t('Another port…'), run: () => portDialog() });
  menu(anchor, items);
}

/// Ask for a port of the host to list.
function portDialog() {
  return new Promise(resolve => {
    const field = el('input.text', { type: 'text', placeholder: t('required'), spellcheck: false, inputmode: 'numeric' });
    const done = v => { back.remove(); resolve(v); };
    const ok = el('button.primary', { onclick: () => {
      const n = Number(field.value.trim());
      if (!Number.isInteger(n) || n < 1 || n > 65535) { toast(t('A port is a number from 1 to 65535.'), true); return; }
      done(n);
    } }, t('Add'));
    const box = el('div.dialog', el('h3', t('List a port of {host}', { host })),
      el('p.secondary', t('For a port this account cannot see listening: a service of root\'s, say. A port from 5900 to 5999 opens in the VNC viewer.')),
      el('div.row', el('span', { style: 'width:120px' }, t('Port on {host}', { host })), field),
      el('div.buttons', el('button', { onclick: () => done(null) }, t('Cancel')), ok));
    const back = el('div.backdrop', { onkeydown: e => { if (e.key === 'Escape') done(null); if (e.key === 'Enter' && e.target.tagName === 'INPUT') ok.click(); } }, box);
    document.body.append(back);
    field.focus();
  }).then(n => { if (n) addPort(n); });
}

function portsRows() {
  const rows = [];
  const defs = savedForwards();
  rows.push(el('div.sect', t('Forwards'), el('div.spacer'), el('button.act', { title: t('A forward in either direction, the ports yours to pick or leave to shellder'), onclick: () => forwardDialog() }, icon('plus'))));
  if (!defs.length) rows.push(el('div.note', t('None yet')));
  for (const d of defs) rows.push(forwardRow(d));
  rows.push(el('div.sect', t('On the host'), el('div.spacer'), el('button.act', { title: t('List a port of the host: one this account sees listening, or any port by hand'), onclick: e => addMenu(e.currentTarget) }, icon('plus'))));
  const shown = shownListeners();
  if (!shown.length) rows.push(el('div.note', t('None shown')));
  for (const l of shown) rows.push(hostPortRow(l));
  return rows;
}

/// Ask for a forward: the direction and the two ports, either of which
/// may be left to shellder.
function forwardDialog(preset = {}) {
  return new Promise(resolve => {
    const direction = el('select', el('option', { value: 'local', selected: preset.direction !== 'remote' }, t('Local → host (-L): a port here reaches a port on the host')),
      el('option', { value: 'remote', selected: preset.direction === 'remote' }, t('Host → local (-R): a port on the host reaches a port here')));
    const local = el('input.text', { type: 'text', value: preset.local ?? '', placeholder: t('shellder picks a free one'), spellcheck: false, inputmode: 'numeric' });
    const remote = el('input.text', { type: 'text', value: preset.remote ?? '', placeholder: t('sshd allocates one'), spellcheck: false, inputmode: 'numeric' });
    const hint = () => {
      if (direction.value === 'local') { local.placeholder = t('shellder picks a free one'); remote.placeholder = t('required'); }
      else { local.placeholder = t('required'); remote.placeholder = t('sshd allocates one'); }
    };
    direction.onchange = hint; hint();
    const done = v => { back.remove(); resolve(v); };
    const port = field => { const v = field.value.trim(); if (!v) return null; const n = Number(v); return Number.isInteger(n) && n >= 1 && n <= 65535 ? n : NaN; };
    const ok = el('button.primary', { onclick: () => {
      const l = port(local), r = port(remote);
      if (Number.isNaN(l) || Number.isNaN(r)) { toast(t('A port is a number from 1 to 65535.'), true); return; }
      if (direction.value === 'local' && r === null) { toast(t('A local forward needs the port on the host.'), true); return; }
      if (direction.value === 'remote' && l === null) { toast(t('A remote forward needs the port on this machine.'), true); return; }
      done({ direction: direction.value, local: l, remote: r });
    } }, t('Add'));
    const box = el('div.dialog', el('h3', t('New forward on {host}', { host })), direction,
      el('div.row', el('span', { style: 'width:120px' }, t('Port here')), local),
      el('div.row', el('span', { style: 'width:120px' }, t('Port on {host}', { host })), remote),
      el('div.buttons', el('button', { onclick: () => done(null) }, t('Cancel')), ok));
    const back = el('div.backdrop', { onkeydown: e => { if (e.key === 'Escape') done(null); if (e.key === 'Enter' && e.target.tagName === 'INPUT') ok.click(); } }, box);
    document.body.append(back);
    (preset.direction === 'remote' ? local : (preset.remote ? local : remote)).focus();
  }).then(def => {
    if (!def) return;
    const list = savedForwards();
    if (!list.some(x => x.direction === def.direction && x.local === def.local && x.remote === def.remote)) saveForwards([...list, def]);
    renderSide();
    startForward(def);
  });
}

// MARK: the Files tab

let filesView = null;
let filesBox = null;
let filesOpened = false;

let filesScroll = {};  // where the trees were scrolled to when another tab took the column

/// The Files tab's box stays in the column across the column's redraws
/// (every reading): a tree taken out of the page and put back would
/// start at the top again. Coming back from another tab, the trees are
/// scrolled to where they were.
function filesTab() {
  if (!filesBox) {
    filesBox = el('div.filesbox');
    filesView = createFilesView(filesBox, { host, single: true, transferHeight: 120, onUpload: upload, onDownload: download, onOpen: openPreview,
      onWindow: () => invoke('open_window', { which: 'files', host }) });
    listen(`files:files-${host.replace(/[^A-Za-z0-9_-]/g, '_')}`, s => filesView.render(s));
  }
  if (filesBox.parentElement !== sideBody) {
    clear(sideBody);
    sideBody.classList.add('fill');
    sideBody.append(filesBox);
    for (const tree of filesBox.querySelectorAll('.pane .tree')) { const k = tree.closest('.pane').dataset.pane; if (filesScroll[k]) tree.scrollTop = filesScroll[k]; }
  }
  if (!filesOpened) {
    filesOpened = true;
    invoke('files_open', { host }).then(s => { filesView.render(s); filesView.nativeDrops(); }).catch(e => toast(String(e), true));
  }
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

let side = 'status';

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
  const live = savedForwards().filter(d => liveOf(d)).length;
  out.push({ key: 'ports', title: t('Ports'), icon: 'network', count: live, rows: portsRows });
  return out;
}

// The column folds away (⌘⇧S) and the terminals take the window; the
// choice is kept on this machine. The button is first in the terminal
// tabs' row, where it stays to bring the column back.
function toggleSide() {
  sideHidden = !sideHidden;
  try { localStorage.setItem('console-side-hidden', sideHidden ? '1' : '0'); } catch {}
  sideEl.classList.toggle('hidden', sideHidden);
  renderTerms();
}

function renderSide() {
  sideEl.classList.toggle('hidden', sideHidden);
  clear(tabsBox);
  const all = sideTabs();
  const current = all.find(x => x.key === side) || all[0];
  for (const x of all) {
    // One row: an icon and a count per tab, the name only on the one open.
    tabsBox.append(el('button.tab', { class: x === current ? 'active' : '', title: x.title, 'aria-label': x.title, onclick: () => { side = x.key; renderSide(); if (x.key === 'ports') refreshForwards(); } },
      icon(x.icon), x === current ? el('span.label', x.title) : null, x.count ? el('span.count', String(x.count)) : null));
  }
  if (current.key === 'files') { filesTab(); return; }
  if (filesBox && filesBox.parentElement === sideBody) {
    filesScroll = {};
    for (const tree of filesBox.querySelectorAll('.pane .tree')) filesScroll[tree.closest('.pane').dataset.pane] = tree.scrollTop;
  }
  // The column is drawn again with every reading: the same scroll
  // position afterwards, not the top (an emptied column is scrolled to
  // the top by itself).
  const top = sideBody.scrollTop;
  clear(sideBody);
  sideBody.classList.remove('fill');
  if (current.key === 'status') renderStatus();
  else {
    const rows = current.rows();
    if (!rows.length) sideBody.append(el('div.note', current.empty || ''));
    else append(sideBody, rows);
  }
  if (top) sideBody.scrollTop = top;
}

async function newSession(kind) {
  const r = await dialog({ title: t('New {kind} session', { kind }), placeholder: t('A name, or none and {kind} numbers it', { kind }), input: true, action: t('Open') });
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

// The terminals' background: dark or light, chosen with the button at the
// right of the tabs and kept per host on this machine; the system's look
// until then (a prompt drawn for a dark terminal is hard to read on
// white).
const termThemeKey = `console-term-dark-${host}`;
let termChoice = (() => { try { return localStorage.getItem(termThemeKey); } catch { return null; } })();
let termDark = termChoice ? termChoice === 'dark' : dark.matches;

function theme() {
  return termDark
    ? { background: '#1e1e1e', foreground: '#f2f2f2', cursor: '#f2f2f2', selectionBackground: 'rgba(64,156,255,0.35)' }
    : { background: '#ffffff', foreground: '#1d1d1f', cursor: '#1d1d1f', selectionBackground: 'rgba(10,132,255,0.25)' };
}

function applyTermTheme() {
  termWrap.classList.toggle('dark', termDark);
  termWrap.classList.toggle('light', !termDark);
  for (const x of terms) if (x.term) x.term.options.theme = theme();
}

function toggleTermTheme() {
  termDark = !termDark;
  termChoice = termDark ? 'dark' : 'light';
  try { localStorage.setItem(termThemeKey, termChoice); } catch {}
  applyTermTheme();
  renderTerms();
}

/// The bytes of a base64 string, as ssh wrote them.
function decode(b64) {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

// The tabs: a terminal is {kind, name, command, id, term, fit, box,
// ended}; a preview {kind: 'preview', name: the path, title: the file's
// name, preview, box} with no term.
const terms = [];
let active = null;

const titleOf = x => x.kind === 'shell' ? t('Shell') : x.kind === 'preview' ? x.title : `${x.kind}: ${x.name}`;

/// A file of the Files tab in a tab of its own, or the tab it is in already.
function openPreview(r) {
  const open = tabOf('preview', r.path);
  if (open) { select(open); return; }
  const box = el('div.termbox.preview');
  termBox.append(box);
  const x = { kind: 'preview', name: r.path, title: r.name, command: null, id: null, term: null, fit: null, box, ended: false };
  x.preview = createPreview(box, { source: { kind: 'host', host }, path: r.path, name: r.name, size: r.size });
  terms.push(x);
  select(x);
}

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
  requestAnimationFrame(() => { if (x.term) { x.fit.fit(); x.term.focus(); } else x.preview.focus(); });
  renderSide();
}

/// Close the tab: hang up (a tmux client detaches, the session lives on);
/// a preview lets its file go.
function closeTab(x) {
  if (!x.ended && x.id !== null) invoke('term_close', { id: x.id }).catch(() => {});
  const i = terms.indexOf(x);
  terms.splice(i, 1);
  if (x.term) x.term.dispose(); else x.preview.dispose();
  x.box.remove();
  if (active === x) {
    active = null;
    if (terms.length) select(terms[Math.min(i, terms.length - 1)]);
    else { renderTerms(); renderSide(); }
  } else renderTerms();
  if (x.term) setTimeout(poll, 500);
}

function renderTerms() {
  clear(termTabs);
  termTabs.append(el('button.side-toggle', { title: sideHidden ? t('Show the column (⌘⇧S)') : t('Hide the column (⌘⇧S)'), onclick: toggleSide }, icon('panel')));
  for (const x of terms) {
    const tip = x.kind === 'preview' ? x.name : x.ended ? t('{name} is closed. Return in it connects again.', { name: titleOf(x) }) : x.command || `ssh ${host}`;
    termTabs.append(el('div.ttab', { class: (x === active ? 'active' : '') + (x.ended ? ' ended' : ''), title: tip, onclick: () => select(x) },
      icon(x.kind === 'shell' ? 'terminal' : x.kind === 'preview' ? 'doc' : 'play'), el('span.label', titleOf(x)),
      el('button.close', { title: x.ended || x.kind === 'preview' ? t('Close') : t('Close this tab: hang up on it (a session lives on)'), onclick: e => { e.stopPropagation(); closeTab(x); } }, icon('close'))));
  }
  termTabs.append(el('button.add', { title: t('A new shell on {host} (⌘T), a new tmux or screen session, an interactive job', { host }), onclick: e => plusMenu(e.currentTarget) }, icon('plus')));
  termTabs.append(el('div.spacer'), el('button.add', { title: termDark ? t('A light background for the terminals') : t('A dark background for the terminals'), onclick: toggleTermTheme }, icon(termDark ? 'sun' : 'moon')));
  let empty = termBox.querySelector('.empty');
  if (!terms.length) {
    if (!empty) termBox.append(empty = el('div.empty'));
    clear(empty).append(icon('terminal'), el('div', t('No tab is open: press + for a shell, or pick a session in the list.')));
  } else if (empty) empty.remove();
}

async function init() {
  const s = await connectState();
  await loadLanguage(s.settings.language);
  document.title = host;
  document.getElementById('title').textContent = document.title;
  dark.addEventListener('change', () => { if (!termChoice) { termDark = dark.matches; applyTermTheme(); renderTerms(); } });
  applyTermTheme();
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
  new ResizeObserver(() => { if (active && active.fit) active.fit.fit(); }).observe(termBox);
  commonKeys();
  document.addEventListener('keydown', e => {
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.key === 'r') { e.preventDefault(); if (error) { error = null; start(); } else poll(); }
    if (mod && e.key === 't') { e.preventDefault(); openTab('shell', '', null); }
    if (mod && e.shiftKey && e.key.toLowerCase() === 's') { e.preventDefault(); toggleSide(); }
  });
  document.addEventListener('visibilitychange', () => { if (document.hidden) stop(); else if (!error) start(); });
  renderSide();
  renderTerms();
  start();
  openTab('shell', '', null);
}

init();
