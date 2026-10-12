// The Console window of a host. The column on the left has a tab per
// kind of thing: Status (what the host is doing — uptime and load, CPU,
// memory, swap (a swap file added until the next boot), GPUs, disks (a
// removable drive's volumes mount, unmount and eject from here; a double
// click shows one in the Files tab), the network, the busiest processes
// (a click sends one a signal), who is logged in — read every few
// seconds through its master or its session, each card only while it is
// shown, so that a card switched off costs the host nothing), Sessions
// (tmux and screen), Jobs (Slurm and PBS), Containers, Ports (what
// listens on the host, VNC servers among it, and ports you add: each
// opens here through a port forward on the master) and Files (the host's
// tree, uploads and downloads; the two-pane window is a click away). The
// rest of the window is tabs in groups that split like an editor's
// (and come back as they were when the window opens again): terminals,
// a shell on the host when the window opens, more from the + menu (a shell, a new tmux or screen
// session, an interactive job), and one for every session, container or
// job clicked in the list (xterm.js on the screen pty of an `ssh -t host
// …`); and a preview for every file opened from the Files tab (text,
// Markdown, a PDF, an image: preview.js). The questions ssh asks never
// show here: they go down the controlling terminal, answered from the
// keychain or the prompt window. A bar across the top of the window
// holds the host's name, a menu of the hosts: the window switches to
// the one picked, where it is and as big as it is (`console_switch`,
// answered with `console-host`: the column reads that host and new tabs
// open there, while the tabs already open stay, each on the host it
// runs on, named on the tab when it is not the one shown). The window
// has no title bar of its own: that bar is where it is dragged by.
import { invoke, listen, startPage, pageReady, t, el, clear, append, icon, fmtBytes, fmtDuration, fmtFileDate, currentState, hostColor, dialog, toast, menu, menuAt, popup, commonKeys, resizableWidth } from './shellder.js';
import { createFilesView, DRAG, dropRows } from './filesview.js';
import { createPreview } from './preview.js';
import { createPorts } from './ports.js';

let host = new URLSearchParams(location.search).get('host');   // the host shown: the column's, new tabs'
const tabsBox = document.getElementById('tabs');
const topbar = document.getElementById('topbar');
const sideBody = document.getElementById('side-body');
const termBox = document.getElementById('term');
const sideEl = document.querySelector('.side');
const dark = window.matchMedia('(prefers-color-scheme: dark)');
const INTERVALS = [2, 5, 10, 30];

let snap = null;        // the last reading
let prevTicks = null;   // the CPU ticks of the one before: the difference is the percentage
let cpuPercent = null;
let prevNet = {};       // an interface's counters at the reading before, by name
let netRates = {};      // bytes per second since then, by name
let error = null;       // why the last reading failed; polling stops until "Try again"
let polling = false;
let timer = null;
let interval = (() => { try { return Number(localStorage.getItem('console-interval')) || 5; } catch { return 5; } })();
let sideHidden = (() => { try { return localStorage.getItem('console-side-hidden') === '1'; } catch { return false; } })();

// MARK: readings

async function poll() {
  if (polling || document.hidden) return;
  polling = true;
  const h = host;
  try {
    const s = await invoke('console_snapshot', { host, parts: [...shownParts()] });
    // The window switched hosts while this was read: the reading is
    // nobody's, the new host's is taken now.
    if (h !== host) { polling = false; poll(); return; }
    const at = Date.now() / 1000;
    if (s.cpuTicks && prevTicks) {
      const dt = s.cpuTicks.total - prevTicks.total;
      if (dt > 0) cpuPercent = (s.cpuTicks.busy - prevTicks.busy) / dt * 100;
    } else if (s.cpuPercent !== null) cpuPercent = s.cpuPercent;
    if (s.cpuTicks) prevTicks = s.cpuTicks;
    // An interface's rate: the bytes since the reading before, over the
    // time between the two.
    const rates = {};
    for (const i of s.network) {
      if (i.received === null) continue;
      const p = prevNet[i.name];
      if (p && at > p.at) rates[i.name] = { down: Math.max(0, (i.received - p.received) / (at - p.at)), up: Math.max(0, (i.sent - p.sent) / (at - p.at)) };
      prevNet[i.name] = { received: i.received, sent: i.sent, at };
    }
    netRates = rates;
    snap = s;
    error = null;
  } catch (e) {
    if (h !== host) { polling = false; poll(); return; }
    error = String(e);
    stop();
  }
  polling = false;
  if (side === 'ports') { if (!await ports.refresh()) renderSide(); return; }
  renderSide();
}

function start() { stop(); poll(); timer = setInterval(poll, interval * 1000); }
function stop() { clearInterval(timer); timer = null; }

// MARK: the Status tab

const pct = frac => Math.max(0, Math.min(100, frac * 100));
const level = frac => frac >= 0.9 ? 'red' : frac >= 0.7 ? 'orange' : '';
const bar = frac => el('div.bar', el('div', { class: level(frac), style: `width:${pct(frac).toFixed(1)}%` }));
const gauge = (label, frac, text, ...acts) => el('div.gauge', el('div.row', el('span.k', label), el('span.v', text), acts), bar(frac));
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

// Which cards the tab shows, kept per host on this machine: a card
// switched off is not read either, which spares the host its commands
// (`top`'s second on macOS, `nvidia-smi`, a `df` that waits on a dead
// network mount, `ps` and its sort).
const PARTS = [
  { key: 'cpu', title: () => t('CPU') },
  { key: 'memory', title: () => t('Memory and swap') },
  { key: 'gpu', title: () => t('GPUs') },
  { key: 'disks', title: () => t('Disks') },
  { key: 'network', title: () => t('Network') },
  { key: 'processes', title: () => t('Processes') },
  { key: 'logins', title: () => t('Logged in') }];
const partsKey = () => `console-parts-${host}`;
function shownParts() {
  try { const v = localStorage.getItem(partsKey()); if (v !== null) return new Set(JSON.parse(v)); } catch {}
  return new Set(PARTS.map(p => p.key));
}
const shows = key => shownParts().has(key);
function togglePart(key) {
  const set = shownParts();
  if (set.has(key)) set.delete(key); else set.add(key);
  try { localStorage.setItem(partsKey(), JSON.stringify([...set])); } catch {}
  if (!set.has('cpu')) { prevTicks = null; cpuPercent = null; }
  if (!set.has('network')) { prevNet = {}; netRates = {}; }
  if (!error) poll();
}

/// The menu of cards: a check on each that is read; a click toggles it
/// and the menu stays, for the next.
function partsMenu(anchor) {
  const m = el('div.menu.checks');
  for (const p of PARTS) {
    const item = el('div.item', { onclick: () => { togglePart(p.key); item.firstChild.replaceChildren(...(shows(p.key) ? [icon('check')] : [])); renderSide(); } },
      el('span.check', shows(p.key) ? icon('check') : null), p.title());
    m.append(item);
  }
  popup(anchor, m);
}

function usageCard() {
  const rows = [];
  if (shows('cpu')) rows.push(gauge(t('CPU'), cpuPercent === null ? 0 : cpuPercent / 100, cpuPercent === null ? '—' : `${cpuPercent.toFixed(0)}%`));
  if (shows('memory')) {
    if (snap.memory) rows.push(gauge(t('Memory'), snap.memory.used / snap.memory.total, ofTotal(snap.memory)));
    rows.push(...swapRows());
  }
  const [every, refresh] = refreshControls();
  const title = el('div.card-title', t('Usage'), el('div.spacer'), every,
    el('button.act', { title: t('Which cards are shown, and read'), onclick: e => partsMenu(e.currentTarget) }, icon('eye')), refresh);
  return el('div', title, rows.length ? el('div.card', rows.map(r => el('div.crow', r))) : null);
}

/// The Swap gauge, with a button that adds a swap file where the host
/// takes one (Linux: `/proc/swaps` is there), and under it the files the
/// Console added, each with a button that removes it.
function swapRows() {
  const rows = [];
  const files = snap.swapFiles;
  const add = files !== null ? act(t('Add a swap file until the next boot…'), 'plus', () => swapDialog()) : null;
  if (snap.swap) rows.push(gauge(t('Swap'), snap.swap.used / snap.swap.total, ofTotal(snap.swap), add));
  else if (add) rows.push(el('div.gauge', el('div.row', el('span.k', t('Swap')), el('span.v', t('none')), add)));
  for (const f of (files || []).filter(f => f.temporary)) {
    rows.push(el('div.gauge', el('div.row', el('span.caption.k', { title: f.path }, f.path), el('span.caption.v', t('{size}, until the next boot', { size: fmtBytes(f.size) })),
      act(t('Switch this swap file off and remove it'), 'trash', () => swapAction('remove', f.path, 0)))));
  }
  return rows;
}

/// Ask for a swap file: its size and where it goes.
function swapDialog() {
  return new Promise(resolve => {
    const size = el('input.text', { type: 'text', value: '4', spellcheck: false, inputmode: 'decimal', style: 'width:80px' });
    const path = el('input.text', { type: 'text', value: `/swapfile.${SWAP_MARK}`, spellcheck: false });
    const done = v => { back.remove(); resolve(v); };
    const ok = el('button.primary', { onclick: () => {
      const gb = Number(size.value.trim().replace(',', '.'));
      if (!(gb > 0) || gb > 1024) { toast(t('The size is a number of GB, up to 1024.'), true); return; }
      const p = path.value.trim();
      const name = p.split('/').pop();
      if (!p.startsWith('/') || !/^[A-Za-z0-9\/_.+-]+$/.test(p) || p.includes('..') || !name.includes(SWAP_MARK)) {
        toast(t('An absolute path with plain characters, its name carrying “{mark}”.', { mark: SWAP_MARK }), true);
        return;
      }
      done({ path: p, size: Math.round(gb * 1024) });
    } }, t('Add'));
    const box = el('div.dialog', el('h3', t('Add swap on {host}', { host })),
      el('p.secondary', t('A swap file, made and switched on now, gone at the next boot (nothing is written to fstab). Its name carries “{mark}”, which is how the Status tab tells it from the host\'s own swap and offers to remove it. The host wants root for it: through sudo, or a password asked in a terminal tab.', { mark: SWAP_MARK })),
      el('div.row', el('span', { style: 'width:120px' }, t('Size')), size, el('span.caption', 'GB')),
      el('div.row', el('span', { style: 'width:120px' }, t('File')), path),
      el('div.buttons', el('button', { onclick: () => done(null) }, t('Cancel')), ok));
    const back = el('div.backdrop', { onkeydown: e => { if (e.key === 'Escape') done(null); if (e.key === 'Enter' && e.target.tagName === 'INPUT') ok.click(); } }, box);
    document.body.append(back);
    size.focus();
    size.select();
  }).then(r => { if (r) swapAction('add', r.path, r.size); });
}

const SWAP_MARK = 'shellder';

/// Add or remove a swap file on the host, the way a disk action goes:
/// at once where the account may, in a terminal tab where the host wants
/// a password.
async function swapAction(action, path, size) {
  try {
    const r = await invoke('console_swap', { host, action, path, size });
    if (r.ask) { toast(t('{host} wants a password for this: it asks in the tab.', { host })); openTab(action === 'add' ? t('add swap') : t('remove swap'), path, r.ask, false, { restore: false }); return; }
    toast(r.said.split('\n')[0] || (action === 'add' ? t('{path} added as swap', { path }) : t('{path} removed', { path })));
  } catch (e) { toast(String(e), true); }
  poll();
}

/// How often the readings are taken, and a button for one now.
function refreshControls() {
  const select = el('select', { title: t('How often the readings are taken'), onchange: e => {
    interval = Number(e.target.value);
    try { localStorage.setItem('console-interval', String(interval)); } catch {}
    if (!error) start();
  } }, INTERVALS.map(n => el('option', { value: n, selected: n === interval }, t('every {n}s', { n }))));
  return [
    select,
    el('button.act', { title: t('Read it again now (⌘R)'), onclick: () => { if (error) { error = null; start(); } else poll(); } }, icon('refresh'))];
}

function gpuCard() {
  if (!shows('gpu') || !snap.gpus.length) return null;
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
    if (r.ask) { toast(t('{host} wants a password for this: it asks in the tab.', { host })); openTab(action, device, r.ask, false, { restore: false }); return; }
    toast(r.said.split('\n')[0] || said);
  } catch (e) { toast(String(e), true); }
  poll();
}

/// The buttons of a volume on a removable drive: mount or unmount it,
/// and eject the drive it is on.
function diskButtons(device, drive, mounted) {
  const out = [mounted
    ? act(t('Unmount {device}', { device }), 'unplug', () => diskAction('unmount', device, t('{device} unmounted', { device })))
    : act(t('Mount {device}', { device }), 'plug', () => diskAction('mount', device, t('{device} mounted', { device })))];
  if (drive) out.push(act(t('Eject {drive}: unmount everything on it and power it off', { drive }), 'eject', () => diskAction('eject', drive, t('{drive} ejected', { drive }))));
  return out;
}

function diskCard() {
  if (!shows('disks') || (!snap.disks.length && !snap.volumes.length)) return null;
  // A double click on a mounted disk shows it in the Files tab.
  const rows = snap.disks.map(d => el('div.gauge', { title: t('Double click: show {mount} in the Files tab', { mount: d.mount }), ondblclick: () => showInFiles(d.mount) },
    el('div.row', el('span.k', d.mount), el('span.v', t('{free} free', { free: fmtBytes(d.total - d.used) })), d.removable ? diskButtons(d.filesystem, d.drive, true) : null),
    bar(d.used / d.total),
    el('div.row', el('span.caption.k', d.filesystem), el('span.caption.v', ofTotal(d)))));
  // Volumes on removable drives that are not mounted: a mount button, no bar.
  for (const v of snap.volumes) rows.push(el('div.gauge',
    el('div.row', el('span.k', v.label || v.device), el('span.v', t('not mounted')), diskButtons(v.device, v.drive, false)),
    el('div.row', el('span.caption.k', [v.device, v.fstype].filter(Boolean).join(' · ')), el('span.caption.v', fmtBytes(v.size)))));
  return card(t('Disks'), ...rows);
}

/// The interfaces with an address: what comes in and goes out per
/// second since the reading before (nothing to say at the first), and
/// the addresses under the name.
function networkCard() {
  if (!shows('network') || !snap.network.length) return null;
  const rate = r => fmtBytes(r) + '/s';
  return card(t('Network'), ...snap.network.map(i => {
    const r = netRates[i.name];
    const text = i.received === null ? t('no counters') : r ? `↓ ${rate(r.down)} · ↑ ${rate(r.up)}` : '—';
    return el('div.gauge', { title: i.received === null ? '' : t('{down} received, {up} sent since boot', { down: fmtBytes(i.received), up: fmtBytes(i.sent) }) },
      el('div.row', el('span.k', i.name), el('span.v', text)),
      i.addresses.length ? el('div.row', el('span.caption.k.selectable', i.addresses.join(' · '))) : null);
  }));
}

// What a click on a process offers.
const SIGNALS = [
  { name: 'TERM', title: () => t('Terminate (SIGTERM)'), what: () => t('Asks it to end, with the chance to clean up.'), destructive: true },
  { name: 'KILL', title: () => t('Kill (SIGKILL)'), what: () => t('Ends it at once, with no chance to save anything.'), destructive: true },
  { name: 'INT', title: () => t('Interrupt (SIGINT)'), what: () => t('What ⌃C does in its terminal.') },
  { name: 'HUP', title: () => t('Hang up (SIGHUP)'), what: () => t('What closing its terminal does; a daemon reads its configuration again.') },
  { name: 'STOP', title: () => t('Stop (SIGSTOP)'), what: () => t('Pauses it until SIGCONT; it cannot refuse.') },
  { name: 'CONT', title: () => t('Continue (SIGCONT)'), what: () => t('Goes on after a SIGSTOP.') }];
// The process table's order: a column and a direction, kept on this
// machine; a click on a heading sorts by it, a second one the other way.
let procSort = (() => { try { return JSON.parse(localStorage.getItem('console-proc-sort')) || { key: 'cpu', up: false }; } catch { return { key: 'cpu', up: false }; } })();

function sortProcesses(key) {
  procSort = procSort.key === key ? { key, up: !procSort.up } : { key, up: key === 'user' || key === 'command' || key === 'pid' };
  try { localStorage.setItem('console-proc-sort', JSON.stringify(procSort)); } catch {}
  renderSide();
}

function sortedProcesses() {
  const { key, up } = procSort;
  const cmp = (a, b) => typeof a[key] === 'number' ? a[key] - b[key] : String(a[key]).localeCompare(String(b[key]));
  return [...snap.processes].sort((a, b) => (up ? 1 : -1) * cmp(a, b) || a.pid - b.pid);
}

/// Send a signal to a process, after asking: as this account, or through
/// sudo for another user's, where the host asks for the password in a
/// terminal tab.
async function signalProcess(p, sig) {
  const r = await dialog({ title: t('Send {signal} to {command} ({pid})?', { signal: 'SIG' + sig.name, command: p.command, pid: p.pid }), message: sig.what(), action: t('Send'), destructive: sig.destructive });
  if (!r.ok) return;
  try {
    const out = await invoke('console_signal', { host, pid: p.pid, signal: sig.name });
    if (out.ask) { toast(t('{host} wants a password for this: it asks in the tab.', { host })); openTab('kill', `${p.command} (${p.pid})`, out.ask, false, { restore: false }); return; }
    toast(t('{signal} sent to {command} ({pid})', { signal: 'SIG' + sig.name, command: p.command, pid: p.pid }));
  } catch (e) { toast(String(e), true); }
  setTimeout(poll, 500);
}

function signalMenu(e, p) {
  e.preventDefault();
  menuAt(e.clientX, e.clientY, SIGNALS.map(s => ({ title: s.title(), run: () => signalProcess(p, s) })));
}

function processCard() {
  if (!shows('processes') || !snap.processes.length) return null;
  const rows = sortedProcesses().map(p => el('tr.proc', { title: t('Click: send it a signal'), onclick: e => signalMenu(e, p), oncontextmenu: e => signalMenu(e, p) },
    el('td.num', String(p.pid)), el('td', p.user), el('td.num', p.cpu.toFixed(1)), el('td.num', p.memory.toFixed(1)), el('td.cmd', { title: p.command }, p.command)));
  const th = (key, label, cls) => el('th', { class: (cls || '') + (procSort.key === key ? ' sorted' : ''), title: t('Sort by this column'), onclick: () => sortProcesses(key) },
    label, procSort.key === key ? el('span.dir', procSort.up ? ' ▲' : ' ▼') : null);
  const head = el('tr', th('pid', t('PID')), th('user', t('User')), th('cpu', t('CPU %'), 'num'), th('memory', t('Mem %'), 'num'), th('command', t('Cmd')));
  return el('div', el('div.card-title', t('Processes')), el('div.card', el('div.crow.procsrow', el('table.procs', head, rows))));
}

function loginCard() {
  if (!shows('logins') || !snap.logins.length) return null;
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
    append(body, [headCard(), usageCard(), gpuCard(), diskCard(), networkCard(), processCard(), loginCard()]);
  }
  sideBody.append(body);
}

// MARK: the session rows

function shellWord(s) { return `'${s.replace(/'/g, "'\\''")}'`; }

/// The terminal tab this is open in, if any.
const tabOf = (kind, name, h = host) => terms.find(x => x.kind === kind && x.name === name && x.host === h && !x.ended);

/// A shell on the host in `path`, in a tab of its own: a row's terminal
/// button, *Shell here* in the Files tab.
function shellAt(path) {
  openTab('shell', path, 'cd ' + shellWord(path) + ' && exec "${SHELL:-sh}" -l');
}

/// The Files tab, showing `path`.
async function showInFiles(path) {
  side = 'files';
  renderSide();
  try {
    await files.ready;
    const pane = files.view.hostPane();
    if (pane) await invoke('files_set_root', { host, pane, path });
  } catch (e) { toast(String(e), true); }
}

/// A row's small button. A click on it is not the row's (which would
/// open the session), nor is a double click.
const act = (title, name, run) => el('button.act', { title, onclick: e => { e.stopPropagation(); run(e.currentTarget); }, ondblclick: e => e.stopPropagation() }, icon(name));

/// The buttons of a row that has directories (a session's, a job's, the
/// host side of a container's bind mounts): show one in the Files tab,
/// open a shell in it. Several make a menu of them.
function dirActs(paths) {
  if (!paths.length) return [];
  const pick = (anchor, run) => paths.length === 1 ? run(paths[0]) : menu(anchor, paths.map(p => ({ title: p, run: () => run(p) })));
  return [act(t('Show this directory in the Files tab'), 'folder', anchor => pick(anchor, showInFiles)),
    act(t('A shell in this directory'), 'terminal', anchor => pick(anchor, shellAt))];
}

/// A row that opens something in a terminal tab when clicked (or brings
/// up the tab it is already open in). `shown` is the name on the row when
/// it is not the id; `rename` a function that asks for another name;
/// `paths` the directories it is in (`dirActs`). With `look` (a tmux or screen session,
/// which a hang-up only detaches) a click attaches it in the temporary
/// tab and a double click keeps the tab, the way the Files tab shows a
/// file (`openSession`).
function row(kind, id, command, title, caption, { shown, rename, end, paths = [], extra, look } = {}) {
  const open = tabOf(kind, id);
  return el('div.session', { class: open ? 'current' : '', title: open ? t('Show its tab') : title,
    onclick: () => look ? openSession(kind, id, command, false) : open ? select(open) : openTab(kind, id, command),
    ondblclick: look ? () => openSession(kind, id, command, true) : null },
    icon(open ? 'play' : 'terminal', open ? 'accent' : ''),
    el('div.text',
      el('div.row', el('span.name', shown ?? id), el('div.spacer'),
        dirActs(paths),
        rename ? act(t('Rename this session'), 'pencil', rename) : null,
        end ? act(t('End this session and everything in it'), 'xcircle', end) : null),
      caption ? el('div.caption', caption) : null,
      paths.map(p => el('div.path', { title: p }, p)),
      extra));
}

/// A row for something that cannot be entered (a job still queued); its
/// directories still are.
function inertRow(name, caption, title, paths = []) {
  return el('div.session.inert', { title }, icon('clock'),
    el('div.text', el('div.row', el('span.name', name), el('div.spacer'), dirActs(paths)), el('div.caption', caption), paths.map(p => el('div.path', { title: p }, p))));
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
  return row('tmux', s.name, `tmux attach-session -t ${shellWord('=' + s.name)}`, t('Open this session in a tab: a click for a look, a double click keeps the tab'), facts,
    { paths: s.path ? [s.path] : [], extra: windows, look: true, rename: () => rename('tmux', s.name, s.name, v => v), end: () => endSession('tmux', s.name, s.name) });
}

function screenRow(s) {
  return row('screen', s.id, `screen -x ${shellWord(s.id)}`, t('Open this session in a tab: a click for a look, a double click keeps the tab'), ['screen', t('pid {pid}', { pid: s.pid }), s.attached ? t('attached') : t('detached')].join(' · '),
    { shown: s.name, paths: s.path ? [s.path] : [], look: true, rename: () => rename('screen', s.id, s.name, v => `${s.pid}.${v}`), end: () => endSession('screen', s.id, s.name) });
}

function containerRow(c) {
  const tool = snap.docker.tool;
  const shell = shellWord('command -v bash >/dev/null 2>&1 && exec bash || exec sh');
  return row(tool, c.name, `${tool} exec -it ${shellWord(c.name)} sh -c ${shell}`, t('Open a shell in this container ({tool} exec)', { tool }), [c.image, c.status, c.ports].filter(Boolean).join(' · '), { paths: c.mounts });
}

const nodesText = n => n ? (n === 1 ? t('1 node') : t('{n} nodes', { n })) : null;

function slurmRow(j) {
  const facts = ['Slurm', j.state, j.time !== '0:00' ? j.time : null, nodesText(j.nodes), j.reason, j.partition].filter(Boolean).join(' · ');
  const paths = j.path ? [j.path] : [];
  if (j.state !== 'RUNNING') return inertRow(`${j.id} ${j.name}`, facts, t('Only a running job can be entered'), paths);
  return row('slurm', j.id, `srun --jobid=${shellWord(j.id)} --overlap --pty bash`, t('Open a shell on the nodes of this job (srun --jobid --overlap --pty)'), facts, { shown: `${j.id} ${j.name}`, paths });
}

function pbsRow(j) {
  const facts = ['PBS', j.state, j.time, nodesText(j.nodes), j.node, j.queue].filter(Boolean).join(' · ');
  const paths = j.path ? [j.path] : [];
  if (j.state !== 'R' || !j.node) return inertRow(`${j.id} ${j.name}`, facts, t('Only a running job can be entered'), paths);
  return row('pbs', j.id, `ssh -t ${shellWord(j.node)}`, t('Open a shell on the first node of this job (ssh from the login node)'), facts, { shown: `${j.id} ${j.name}`, paths });
}

// MARK: the Ports tab

// The forwards and the host's ports (ports.js), with what the readings
// say listens on the host.
const makePorts = () => createPorts(host, { listeners: () => snap ? snap.listeners : [], vncServers: () => snap ? snap.vnc : [], onChange: () => { if (side === 'ports') renderSide(); } });
let ports = makePorts();

// MARK: the Files tab

// The host shown's Files tab, once opened: {box, view, ready (the first
// listing, once asked for), scroll (where the trees were scrolled to
// when another tab took the column)}; and every host's the window has
// shown, by host, so that the trees are as they were when the window
// comes back to it.
let files = null;
const filesTabs = new Map();

/// The Files tab's box stays in the column across the column's redraws
/// (every reading): a tree taken out of the page and put back would
/// start at the top again. Coming back from another tab, the trees are
/// scrolled to where they were.
function filesTab() {
  if (!files) {
    const h = host;
    const box = el('div.filesbox');
    const view = createFilesView(box, { host: h, single: true, transferHeight: 120, onUpload: upload, onDownload: download, onOpen: openPreview, onShell: shellAt, dropOutside,
      onWindow: () => invoke('open_window', { which: 'files', host: h }) });
    listen(`files:files-${h.replace(/[^A-Za-z0-9_-]/g, '_')}`, s => view.render(s));
    const ready = invoke('files_open', { host: h }).then(s => { view.render(s); view.nativeDrops(); });
    ready.catch(e => toast(String(e), true));
    files = { box, view, ready, scroll: {} };
    filesTabs.set(h, files);
  }
  if (files.box.parentElement !== sideBody) {
    clear(sideBody);
    sideBody.classList.add('fill');
    sideBody.append(files.box);
    for (const tree of files.box.querySelectorAll('.pane .tree')) { const k = tree.closest('.pane').dataset.pane; if (files.scroll[k]) tree.scrollTop = files.scroll[k]; }
  }
}

/// The Files tab's trees leave the column: where they were scrolled to
/// is kept for when they come back.
function keepFilesScroll() {
  if (!files || files.box.parentElement !== sideBody) return;
  files.scroll = {};
  for (const tree of files.box.querySelectorAll('.pane .tree')) files.scroll[tree.closest('.pane').dataset.pane] = tree.scrollTop;
}

const picker = () => window.__TAURI__.dialog;

/// Files picked on this machine go into `dir` on the host.
async function upload(dir) {
  const k = files && files.view.hostPane();
  if (!k) return;
  const paths = await picker().open({ multiple: true, title: t('Upload to {host}:{dir}', { host, dir }) });
  if (!paths || !paths.length) return;
  await invoke('files_drop_local', { host, pane: k, paths, onto: dir }).catch(e => toast(String(e), true));
}

/// `paths` on the host go to a folder picked on this machine.
async function download(paths) {
  const s = files && files.view.snapshot();
  const k = files && files.view.hostPane();
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

// MARK: the host

/// The hosts of the list, the one shown checked, each with its colour: a
/// click switches this window to it. A host that already has a Console
/// window is brought up instead (the app knows which).
function hostMenu(anchor) {
  const s = currentState();
  const m = el('div.menu.hosts');
  let close = null;
  for (const h of (s ? s.hosts : [])) {
    const current = h.alias === host;
    m.append(el('div.item', { class: current ? 'current' : '', onclick: () => { close(); if (!current) invoke('console_switch', { host: h.alias }).catch(e => toast(String(e), true)); } },
      el('span.dot', { class: hostColor(s.statuses[h.alias], s.locked.includes(h.alias)) }), el('span.label', h.alias), current ? icon('check') : null));
  }
  close = popup(anchor, m);
}

const hostLabel = el('span.label', host);
const hostButton = el('button.hostpick', { title: t('Switch this window to another host'), onclick: e => hostMenu(e.currentTarget) }, hostLabel, icon('chevrondown'));

/// The window shows `h` from now on (the app's `console-host`, after a
/// pick in the menu). The tabs stay as they are, each on the host it
/// runs on; the column reads the new host, what it keeps per host (the
/// tab open, the cards, the terminals' background) is the new host's,
/// and new tabs open there. The layout is kept under the host shown.
function switchHost(h) {
  if (h === host) return;
  stop();
  keepFilesScroll();
  host = h;
  document.title = host;
  hostLabel.textContent = host;
  snap = null; prevTicks = null; cpuPercent = null; prevNet = {}; netRates = {}; error = null;
  files = filesTabs.get(host) || null;
  ports.dispose();
  ports = makePorts();
  side = savedSide();
  renderSide();
  renderTerms();
  start();
}

// MARK: the column

// The tab open, kept per host on this machine with the tabs of the terminals.
const sideKey = () => `console-side-${host}`;
const savedSide = () => { try { return localStorage.getItem(sideKey()) || 'status'; } catch { return 'status'; } };
let side = savedSide();

/// The tabs of the column: Status, Files, Sessions, then Jobs and
/// Containers where the host has them, Ports. Files and Ports need no
/// reading, and Sessions is there before the first one and when a
/// reading fails: the Status tab's trouble is its own.
function sideTabs() {
  const out = [{ key: 'status', title: t('Status'), icon: 'gauge' }];
  out.push({ key: 'files', title: t('Files'), icon: 'folder' });
  const sessions = snap ? [...(snap.tmux || []).map(x => ({ kind: 'tmux', x })), ...(snap.screen || []).map(x => ({ kind: 'screen', x }))] : [];
  out.push({ key: 'sessions', title: t('Sessions'), icon: 'terminal', count: sessions.length, rows: () => sessions.map(s => s.kind === 'tmux' ? tmuxRow(s.x) : screenRow(s.x)),
    empty: !snap ? (error ? t('Could not read {host}: {error}', { host, error }) : t('Reading {host}…', { host }))
      : snap.tmux === null && snap.screen === null ? t('Neither tmux nor screen is installed on {host}.', { host }) : t('No tmux or screen session is running. The + above the terminals starts one; it survives this window.') });
  if (snap && (snap.slurm !== null || snap.pbs !== null)) {
    const jobs = [...(snap.slurm || []).map(x => ({ kind: 'slurm', x })), ...(snap.pbs || []).map(x => ({ kind: 'pbs', x }))];
    out.push({ key: 'jobs', title: t('Jobs'), icon: 'clock', count: jobs.length, rows: () => jobs.map(j => j.kind === 'slurm' ? slurmRow(j.x) : pbsRow(j.x)), empty: t('No job of yours is queued or running. The + above the terminals starts an interactive one.') });
  }
  if (snap && snap.docker !== null) {
    out.push({ key: 'containers', title: t('Containers'), icon: 'box', count: snap.docker.list.length, rows: () => snap.docker.list.map(containerRow), empty: t('No container is running, or {tool} is not usable by this account.', { tool: snap.docker.tool }) });
  }
  out.push({ key: 'ports', title: t('Ports'), icon: 'network', count: ports.liveCount(), rows: ports.rows });
  return out;
}

// The column folds away to its strip of tabs and the terminals take the
// window, the way an editor's side bar does: a click on the tab that is
// open, a drag of its edge well under its minimum width, or ⌘⇧S; a
// click on a tab, or a drag back out, brings it back. The choice is
// kept on this machine.
function setSideHidden(hidden) {
  sideHidden = hidden;
  try { localStorage.setItem('console-side-hidden', sideHidden ? '1' : '0'); } catch {}
  renderSide();
  renderTerms();
}
function toggleSide() { setSideHidden(!sideHidden); }

function renderSide() {
  sideEl.classList.toggle('collapsed', sideHidden);
  clear(tabsBox);
  const all = sideTabs();
  const current = all.find(x => x.key === side) || all[0];
  for (const x of all) {
    // A strip down the left: an icon per tab, its name in the tooltip.
    // The open tab's icon folds the column away.
    const open = x === current && !sideHidden;
    tabsBox.append(el('button.tab', { class: open ? 'active' : '', title: x.title, 'aria-label': x.title, onclick: () => {
      if (open) { setSideHidden(true); return; }
      side = x.key;
      try { localStorage.setItem(sideKey(), side); } catch {}
      if (sideHidden) setSideHidden(false); else renderSide();
      if (x.key === 'ports') ports.refresh();
    } }, icon(x.icon)));
  }
  if (sideHidden) return;
  if (current.key === 'files') { filesTab(); return; }
  keepFilesScroll();
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
  // Brought back, the session is attached, or made again when it is gone;
  // one that named itself cannot be found again.
  const again = !name ? null : kind === 'tmux' ? `tmux new-session -A -s ${shellWord(name)}` : `screen -R ${shellWord(name)}`;
  openTab(kind, name || t('new session'), command, false, { restore: !!name, again });
  setTimeout(poll, 1500);
}

/// What + offers: a shell, and whatever else the host can start.
function plusMenu(anchor) {
  const items = [{ title: t('New shell'), icon: 'terminal', run: () => openTab('shell', '', null) }];
  if (snap && snap.tmux !== null) items.push({ title: t('New tmux session…'), icon: 'plus', run: () => newSession('tmux') });
  if (snap && snap.screen !== null) items.push({ title: t('New screen session…'), icon: 'plus', run: () => newSession('screen') });
  if (snap && snap.slurm !== null) items.push({ title: t('Interactive Slurm job (srun --pty bash)'), icon: 'clock', run: () => openTab('slurm', t('interactive'), 'srun --pty bash -l', false, { restore: false }) });
  if (snap && snap.pbs !== null) items.push({ title: t('Interactive PBS job (qsub -I)'), icon: 'clock', run: () => openTab('pbs', t('interactive'), 'qsub -I', false, { restore: false }) });
  menu(anchor, items);
}

// MARK: the terminal tabs

// A group's terminals' background: dark or light, chosen with the
// button in its tab row beside the split button (`g.theme`, 'dark' or
// 'light'; null until chosen: the system's look, since a prompt drawn
// for a dark terminal is hard to read on white). A group split off
// another starts with its choice, and the choice is kept with the
// layout. The tabs keep the window's colours.
const groupDark = g => g.theme ? g.theme === 'dark' : dark.matches;

function theme(isDark) {
  return isDark
    ? { background: '#1e1e1e', foreground: '#f2f2f2', cursor: '#f2f2f2', selectionBackground: 'rgba(64,156,255,0.35)' }
    : { background: '#ffffff', foreground: '#1d1d1f', cursor: '#1d1d1f', selectionBackground: 'rgba(10,132,255,0.25)' };
}

/// The group's body, its terminals and its button in its colours.
function applyGroupTheme(g) {
  const isDark = groupDark(g);
  g.el.classList.toggle('dark', isDark);
  g.el.classList.toggle('light', !isDark);
  for (const x of g.tabs) if (x.term) x.term.options.theme = theme(isDark);
  g.toggle.title = isDark ? t('A light background for the terminals') : t('A dark background for the terminals');
  g.toggle.replaceChildren(icon(isDark ? 'sun' : 'moon'));
}

function toggleGroupTheme(g) {
  g.theme = groupDark(g) ? 'light' : 'dark';
  applyGroupTheme(g);
  saveLayout();
}

/// The bytes of a base64 string, as ssh wrote them.
function decode(b64) {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

// The tabs are in groups, laid out the way an editor lays out its
// editors: a group is a row of tabs over the one shown; a split puts a
// second group beside it or under it, and so on, a tree of rows and
// columns whose dividers drag. A tab dragged to another group's tab row
// moves there; dragged to an edge of a group's body it splits the group
// there. New tabs open in the group with the keyboard (`focused`).
//
// A tab: a terminal is {host, kind, name, command, again, id, term, fit, box,
// ended, temporary, restore, group}; a preview {kind: 'preview', name:
// the path, title: the file's name, size, preview, box, temporary,
// restore, group} with no term. A group: {tabs, active, parent, el, row,
// body, toggle}. A split: {dir: 'row' | 'col', children, sizes, parent,
// el}; `sizes` are the children's shares of it.
const terms = [];        // every tab, in the order opened
let root = null;         // the layout: a group or a split
let focused = null;      // the group with the keyboard
let restoring = false;   // the saved layout is being put back: not saved over meanwhile
const activeTab = () => focused ? focused.active : null;
let nextUid = 1;

// A shell started in a directory is named after it.
const titleOf = x => x.kind === 'shell' ? (x.name ? `${t('Shell')}: ${x.name.split('/').filter(Boolean).pop() || '/'}` : t('Shell')) : x.kind === 'preview' ? x.title : `${x.kind}: ${x.name}`;

/// The groups, left to right and top to bottom.
function groupsOf(node) { return node.tabs ? [node] : node.children.flatMap(groupsOf); }
const groups = () => root ? groupsOf(root) : [];

/// A group, with `theme` as its terminals' background (null: the system's).
function newGroup(theme = null) {
  const g = { tabs: [], active: null, parent: null, theme };
  g.row = el('div.termtabs', { 'data-tauri-drag-region': true });
  g.toggle = el('button.theme-toggle', { onclick: () => toggleGroupTheme(g) });
  g.body = el('div.body');
  // A mousedown anywhere in the group gives it the keyboard. The rows are
  // not drawn again here: the click that follows wants the element it
  // started on.
  g.el = el('div.group', { onmousedown: () => setFocused(g) }, g.row, g.body);
  applyGroupTheme(g);
  takesTabs(g);
  new ResizeObserver(() => { if (g.active && g.active.fit) g.active.fit.fit(); }).observe(g.body);
  return g;
}

function setFocused(g) {
  focused = g;
  for (const h of groups()) h.el.classList.toggle('focused', h === g);
}

/// A group's tabs and body on screen: the active tab shown, the others
/// hidden, a word where there is no tab.
function showActive(g) {
  for (const y of g.tabs) y.box.classList.toggle('hidden', y !== g.active);
  let empty = g.body.querySelector('.empty');
  if (!g.tabs.length) {
    if (!empty) g.body.append(empty = el('div.empty'));
    clear(empty).append(icon('terminal'), el('div', t('No tab is open: press + for a shell, or pick a session in the list.')));
  } else if (empty) empty.remove();
}

/// The layout's elements, built again from the tree (the groups' own
/// elements are kept, moved where they go now: a terminal survives that).
function layoutEl(node) {
  if (node.tabs) return node.el;
  node.el = el('div.split', { class: 'dir-' + node.dir });
  node.children.forEach((c, i) => {
    if (i) node.el.append(divider(node, i - 1));
    const ce = layoutEl(c);
    ce.style.flex = `${node.sizes[i]} 1 0`;
    node.el.append(ce);
  });
  return node.el;
}

function renderLayout() {
  clear(termBox);
  const e = layoutEl(root);
  e.style.flex = '1 1 0';
  termBox.append(e);
  renderTerms();
  for (const g of groups()) if (g.active && g.active.fit) requestAnimationFrame(() => g.active.fit.fit());
}

/// The line between two children of a split: dragged, it moves their
/// shares; the terminals follow their bodies' sizes by themselves.
function divider(node, i) {
  const d = el('div.divider', { class: 'dir-' + node.dir });
  d.onmousedown = e => {
    if (e.button !== 0) return;
    e.preventDefault();
    const horizontal = node.dir === 'row';
    const a = node.children[i].el, b = node.children[i + 1].el;
    const ra = a.getBoundingClientRect(), rb = b.getBoundingClientRect();
    const total = horizontal ? ra.width + rb.width : ra.height + rb.height;
    const sum = node.sizes[i] + node.sizes[i + 1];
    const start = horizontal ? e.clientX : e.clientY;
    const startA = horizontal ? ra.width : ra.height;
    d.classList.add('dragging');
    document.body.style.cursor = horizontal ? 'col-resize' : 'row-resize';
    const move = ev => {
      const px = Math.min(total - 80, Math.max(80, startA + (horizontal ? ev.clientX : ev.clientY) - start));
      node.sizes[i] = sum * px / total;
      node.sizes[i + 1] = sum - node.sizes[i];
      a.style.flex = `${node.sizes[i]} 1 0`;
      b.style.flex = `${node.sizes[i + 1]} 1 0`;
    };
    const up = () => {
      document.removeEventListener('mousemove', move); document.removeEventListener('mouseup', up);
      d.classList.remove('dragging');
      document.body.style.cursor = '';
      saveLayout();
    };
    document.addEventListener('mousemove', move); document.addEventListener('mouseup', up);
  };
  return d;
}

/// A new group beside `g` (`dir` 'row': to its right, or its left with
/// `before`) or under it ('col'; above with `before`): a sibling in a
/// split of that direction, which is made when `g` is not in one. It
/// starts with `g`'s background.
function splitGroup(g, dir, before = false) {
  const g2 = newGroup(g.theme);
  const p = g.parent;
  if (p && p.dir === dir) {
    const i = p.children.indexOf(g);
    const half = p.sizes[i] / 2;
    p.sizes.splice(i, 1, half, half);
    p.children.splice(before ? i : i + 1, 0, g2);
    g2.parent = p;
  } else {
    const node = { dir, children: before ? [g2, g] : [g, g2], sizes: [1, 1], parent: p };
    if (p) p.children[p.children.indexOf(g)] = node; else root = node;
    g.parent = node;
    g2.parent = node;
  }
  renderLayout();
  return g2;
}

/// A group with no tab left goes, its share to its neighbours; a split
/// with one child left is that child, folded into a split of the same
/// direction above it. The only group stays, empty.
function removeGroup(g) {
  const p = g.parent;
  if (!p) return;
  const i = p.children.indexOf(g);
  p.children.splice(i, 1);
  p.sizes.splice(i, 1);
  g.el.remove();
  const neighbour = p.children[Math.min(i, p.children.length - 1)];
  if (p.children.length === 1) {
    const only = p.children[0];
    const gp = p.parent;
    only.parent = gp;
    if (!gp) root = only;
    else {
      const j = gp.children.indexOf(p);
      if (!only.tabs && only.dir === gp.dir) {
        const share = gp.sizes[j];
        const total = only.sizes.reduce((a, b) => a + b, 0);
        gp.children.splice(j, 1, ...only.children);
        gp.sizes.splice(j, 1, ...only.sizes.map(s => s / total * share));
        for (const c of only.children) c.parent = gp;
      } else gp.children[j] = only;
    }
  }
  if (focused === g) focused = groupsOf(neighbour)[0];
  renderLayout();
}

/// A tab into the group; a terminal takes the group's colours.
function addTab(g, x, at = g.tabs.length) {
  x.group = g;
  g.tabs.splice(at, 0, x);
  g.body.append(x.box);
  if (x.term) x.term.options.theme = theme(groupDark(g));
}

/// Take a tab out of its group; the group's active tab becomes the one
/// beside it. Returns the group.
function detachTab(x) {
  const g = x.group;
  const i = g.tabs.indexOf(x);
  g.tabs.splice(i, 1);
  x.box.remove();
  if (g.active === x) g.active = g.tabs[Math.min(i, g.tabs.length - 1)] || null;
  return g;
}

/// A tab into another group, at `at` among its tabs (the end without).
function moveTab(x, g, at) {
  const from = detachTab(x);
  if (from === g && at !== undefined && at > g.tabs.length) at = g.tabs.length;
  addTab(g, x, at);
  showActive(from);
  if (!from.tabs.length && from !== g) removeGroup(from);
  select(x);
}

/// A tab dragged (its own kind of drag, not the Files tab's rows): a
/// group's tab row takes it in, between the tabs where it is dropped;
/// the middle of a group's body takes it in at the end, an edge of the
/// body splits the group there and puts the tab in the new one. The
/// page gets the drop as an HTML event where the webview gives one
/// (Linux), as the webview's own drag-drop event with a position on
/// macOS (see filesview.js): both come to `markDrop` and `dropTab`.
const TABDRAG = 'application/x-shellder-tab';
let dragged = null;   // the tab under way, this page's
let dragForgets = null;

/// The tab under way, once: whichever of the two drops comes first.
function takeDragged() { const x = dragged; dragged = null; return x; }

function clearDropMarks() {
  for (const d of termBox.querySelectorAll('.dropzone')) d.remove();
  for (const r of termBox.querySelectorAll('.termtabs.droptarget')) r.classList.remove('droptarget');
}

/// Where a drop at (px, py) in `g` would go: {at} among the row's tabs,
/// or {zone} of the body.
function dropPlace(g, node, px, py) {
  if (g.row.contains(node)) {
    const tabs = [...g.row.querySelectorAll('.ttab')];
    let at = tabs.length;
    for (let i = 0; i < tabs.length; i++) { const r = tabs[i].getBoundingClientRect(); if (px < r.left + r.width / 2) { at = i; break; } }
    return { at };
  }
  const r = g.body.getBoundingClientRect();
  const fx = (px - r.left) / r.width, fy = (py - r.top) / r.height;
  return { zone: fx < 0.25 ? 'left' : fx > 0.75 ? 'right' : fy < 0.25 ? 'top' : fy > 0.75 ? 'bottom' : 'middle' };
}

function markDrop(g, node, px, py) {
  clearDropMarks();
  const place = dropPlace(g, node, px, py);
  if (place.at !== undefined) { g.row.classList.add('droptarget'); return; }
  g.body.append(el('div.dropzone', { class: place.zone }));
}

function dropTab(x, g, node, px, py) {
  clearDropMarks();
  const place = dropPlace(g, node, px, py);
  if (place.at !== undefined) {
    let at = place.at;
    if (x.group === g && g.tabs.indexOf(x) < at) at--;
    moveTab(x, g, at);
    return;
  }
  const z = place.zone;
  if (z === 'middle') { if (x.group !== g) moveTab(x, g); return; }
  if (x.group === g && g.tabs.length === 1) return;
  const g2 = splitGroup(g, z === 'left' || z === 'right' ? 'row' : 'col', z === 'left' || z === 'top');
  moveTab(x, g2);
}

/// The HTML drag events of a group, where the page gets them.
function takesTabs(g) {
  const over = e => { if (!dragged) return; e.preventDefault(); e.dataTransfer.dropEffect = 'move'; markDrop(g, e.target, e.clientX, e.clientY); };
  const drop = e => { if (!dragged) { clearDropMarks(); return; } e.preventDefault(); const x = takeDragged(); if (x) dropTab(x, g, e.target, e.clientX, e.clientY); };
  g.row.addEventListener('dragover', over);
  g.row.addEventListener('drop', drop);
  g.body.addEventListener('dragover', over);
  g.body.addEventListener('drop', drop);
  g.el.addEventListener('dragleave', e => { if (!g.el.contains(e.relatedTarget)) clearDropMarks(); });
}

/// The webview's own drag-drop events: a tab's drag ends on the group
/// under the pointer (macOS, where the page gets no HTML drop of its
/// own drag). Files dragged in from the desktop are not ours here.
function nativeTabDrops() {
  const w = window.__TAURI__.webview.getCurrentWebview();
  w.onDragDropEvent(e => {
    const p = e.payload;
    if (!dragged) return;
    if (p.type !== 'over' && p.type !== 'drop') { clearDropMarks(); return; }
    if (p.type === 'drop' && p.paths && p.paths.length) return;
    const scale = window.devicePixelRatio || 1;
    const px = p.position.x / scale, py = p.position.y / scale;
    const node = document.elementFromPoint(px, py);
    const g = node && groups().find(h => h.el.contains(node));
    if (!g) { clearDropMarks(); return; }
    if (p.type === 'over') markDrop(g, node, px, py);
    else { const x = takeDragged(); if (x) dropTab(x, g, node, px, py); }
  });
}

/// A file of the Files tab in a tab of the group with the keyboard, the
/// way an editor does it: one click shows it in that group's temporary
/// tab (italics), which the next click's file takes over, and the tree
/// keeps the keyboard; `keep` (a double click, Return, the menu) gives
/// it a tab of its own, or keeps the one it is in. A file already in a
/// tab of the group is brought up; one shown in another group gets a
/// tab here as well (so a split can show it twice).
function openPreview(r, keep = true) {
  openPreviewIn(focused, r, keep, true);
}

function openPreviewIn(g, r, keep, sel, h = host) {
  let x = g.tabs.find(y => y.kind === 'preview' && y.name === r.path && y.host === h);
  if (!x && !keep) x = g.tabs.find(y => y.kind === 'preview' && y.temporary);
  if (x && (x.name !== r.path || x.host !== h)) {
    x.preview.dispose();
    clear(x.box);
    x.host = h;
    x.name = r.path;
    x.title = r.name;
    x.size = r.size;
    x.preview = createPreview(x.box, { source: { kind: 'host', host: h }, path: r.path, name: r.name, size: r.size });
  } else if (!x) {
    const box = el('div.termbox.preview');
    x = { host: h, kind: 'preview', name: r.path, title: r.name, size: r.size, command: null, id: null, term: null, fit: null, box, ended: false, temporary: !keep, restore: true, uid: nextUid++ };
    x.preview = createPreview(box, { source: { kind: 'host', host: h }, path: r.path, name: r.name, size: r.size });
    terms.push(x);
    addTab(g, x);
  }
  if (keep) x.temporary = false;
  if (sel) select(x, keep); else { if (!g.active) g.active = x; showActive(g); renderTerms(); }
  return x;
}

/// A session of the Sessions tab in a terminal tab, the same way: one
/// click attaches it in the temporary tab (italics) of the group with
/// the keyboard, which the next session clicked takes over (that client
/// detaches, the session lives on); `keep` (a double click) gives it a
/// tab of its own, or keeps the one it is in. A session already in a
/// tab is brought up wherever it is.
function openSession(kind, id, command, keep) {
  let x = tabOf(kind, id);
  if (!x && !keep) x = focused.tabs.find(y => y.temporary && y.term);
  if (x && (x.kind !== kind || x.name !== id || x.host !== host)) {
    // The temporary tab, attached to another session (on any host): hang
    // up on that one and attach this one in the same tab. Its id is
    // dropped first, so the old ssh's last bytes and its exit are nobody's.
    if (!x.ended && x.id !== null) invoke('term_close', { id: x.id }).catch(() => {});
    x.id = null;
    x.host = host;
    x.kind = kind;
    x.name = id;
    x.command = command;
    x.again = null;
    x.restore = true;
    connect(x);
    setTimeout(poll, 500);
  } else if (!x) x = openTab(kind, id, command, !keep);
  if (keep) x.temporary = false;
  select(x);
}

/// The temporary tab is kept: a double click on it.
function keepTab(x) {
  if (!x.temporary) return;
  x.temporary = false;
  renderTerms();
}

/// A path as typed on a command line: as it is when every character is
/// plain, in single quotes otherwise.
const typed = path => /^[\w\/.,+=:@%~-]+$/.test(path) ? path : shellWord(path);

/// Rows of the Files tab dropped on a terminal: their paths, quoted and
/// a space after each, are typed into it, as Terminal does. Only rows of
/// the terminal's own host mean anything there.
function typePaths(x, rows) {
  const paths = rows.filter(r => r.source.kind === 'host' && r.source.host === x.host).map(r => r.path);
  if (!paths.length || x.ended) return;
  invoke('term_write', { id: x.id, data: paths.map(p => typed(p) + ' ').join('') }).catch(() => {});
  x.term.focus();
}

/// A drag of the Files tab's rows may end on a terminal (the webview's
/// native drop event is how it gets there on macOS, see `nativeDrops`).
function dropOutside(node) {
  if (dragged) return null;   // a tab's drag, not rows
  const box = node.closest('.termbox');
  const x = box && terms.find(y => y.box === box);
  if (!x || !x.term || x.ended) return null;
  return { node: box, drop: rows => typePaths(x, rows) };
}

/// The same drop as an HTML event, where the page gets one.
function dropsPaths(box, x) {
  box.ondragover = e => {
    if (!e.dataTransfer.types.includes(DRAG) || x.ended) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = 'copy';
    box.classList.add('droptarget');
  };
  box.ondragleave = () => box.classList.remove('droptarget');
  box.ondrop = e => {
    box.classList.remove('droptarget');
    const rows = dropRows(e);
    if (!rows) return;
    e.preventDefault();
    typePaths(x, rows);
  };
}

/// A new tab running `command` on the host (the login shell without one)
/// in the group with the keyboard; `temporary` makes it the tab the next
/// session clicked takes over. `restore` false: a tab the window does not
/// bring back when it opens again (a one-off action, a job submitted);
/// `again` is the command that brings a tab back when it is not the one
/// that opened it (a session made here is attached, not made again);
/// `host` the host it runs on, the one shown unless said otherwise (a
/// tab brought back, a split's copy of a tab from before a switch).
function openTab(kind, name, command, temporary = false, opts = {}) {
  return openTabIn(focused, kind, name, command, temporary, opts);
}

function openTabIn(g, kind, name, command, temporary = false, { restore = true, again = null, select: sel = true, host: h = host } = {}) {
  const term = new Terminal({
    fontFamily: 'ui-monospace, "SF Mono", Menlo, Consolas, monospace',
    fontSize: 13,
    lineHeight: 1.15,
    cursorBlink: true,
    scrollback: 10000,
    macOptionIsMeta: true,
    theme: theme(groupDark(g)),
  });
  const fit = new FitAddon.FitAddon();
  term.loadAddon(fit);
  const box = el('div.termbox');
  const x = { host: h, kind, name, command, again, id: null, term, fit, box, ended: true, temporary, restore, uid: nextUid++ };
  terms.push(x);
  addTab(g, x);
  // Opened and measured while the box is on screen (it is, until the
  // group's active tab hides it): a terminal opened out of sight has no
  // size to go by.
  term.open(box);
  fit.fit();
  dropsPaths(box, x);
  term.onData(data => { if (!x.ended) invoke('term_write', { id: x.id, data }).catch(() => {}); else if (data === '\r') connect(x); });
  // Binary input (a paste that is not text): bytes as a latin1 string.
  term.onBinary(data => { if (!x.ended) invoke('term_write', { id: x.id, data }).catch(() => {}); });
  term.onResize(({ cols, rows }) => { if (!x.ended) invoke('term_resize', { id: x.id, cols, rows }).catch(() => {}); });
  // ⌘C copies a selection (otherwise it is the interrupt it always was),
  // ⌘K clears, ⌘A selects all; any other ⌘ key is the window's (⌘W, ⌘T,
  // ⌘R, ⌘\, ⌘,, ⌘Q) and goes past the terminal to the page. ⌘V is not
  // taken here: the WebView pastes on its own (xterm takes the paste
  // event) and reading the clipboard as well pasted everything twice.
  term.attachCustomKeyEventHandler(e => {
    if (e.type !== 'keydown' || !e.metaKey) return true;
    if (e.key === 'c' && term.hasSelection()) { navigator.clipboard.writeText(term.getSelection()); return false; }
    if (e.key === 'k') { term.clear(); return false; }
    if (e.key === 'a') { term.selectAll(); return false; }
    return false;
  });
  if (sel) select(x); else { if (!g.active) g.active = x; showActive(g); }
  connect(x);
  return x;
}

/// The same thing again in another group: what a split shows there. A
/// terminal's command run once more (a shell in the same directory, the
/// session attached a second time), a preview's file; a plain shell for
/// a tab that is not brought back, or when there is none.
function duplicateInto(x, g) {
  if (!x || (x.term && !x.restore)) return openTabIn(g, 'shell', '', null);
  if (x.term) return openTabIn(g, x.kind, x.name, x.again || x.command, false, { again: x.again, host: x.host });
  return openPreviewIn(g, { path: x.name, name: x.title, size: x.size }, true, true, x.host);
}

/// A second group beside (`dir` 'row') or under ('col') the group with
/// the keyboard, showing what its tab shows (⌘\, ⌘⌥\).
function splitFocused(dir) {
  const x = activeTab();
  const g2 = splitGroup(focused, dir);
  duplicateInto(x, g2);
}

async function connect(x) {
  x.ended = false;
  x.term.reset();
  if (!x.box.classList.contains('hidden')) x.fit.fit();
  x.term.write(`\x1b[2m${t('Connecting to {host}…', { host: x.host })}\x1b[0m\r\n`);
  renderTerms();
  // The temporary tab may be given to another session while this login
  // is under way: the ssh that comes back then is not the tab's any more
  // and is hung up on.
  const gen = x.gen = (x.gen || 0) + 1;
  try {
    const id = await invoke('term_open', { host: x.host, command: x.command, cols: x.term.cols, rows: x.term.rows });
    if (x.gen !== gen) { invoke('term_close', { id }).catch(() => {}); return; }
    x.id = id;
    if (x === activeTab()) x.term.focus();
  } catch (e) {
    if (x.gen !== gen) return;
    x.ended = true;
    x.term.write(`\r\n\x1b[31m${String(e)}\x1b[0m\r\n`);
    renderTerms();
  }
  renderSide();
}

/// Bring the tab up, its group taking the keyboard; `focus` false leaves
/// the keyboard where it is.
function select(x, focus = true) {
  const g = x.group;
  g.active = x;
  focused = g;
  showActive(g);
  renderTerms();
  if (focus) requestAnimationFrame(() => { if (x.term) { x.fit.fit(); x.term.focus(); } else x.preview.focus(); });
  else if (x.fit) requestAnimationFrame(() => x.fit.fit());
  renderSide();
}

/// Close the tab: hang up (a tmux client detaches, the session lives on);
/// a preview lets its file go. A group left without a tab goes, unless
/// it is the only one.
function closeTab(x) {
  if (!x.ended && x.id !== null) invoke('term_close', { id: x.id }).catch(() => {});
  const g = detachTab(x);
  terms.splice(terms.indexOf(x), 1);
  if (x.term) x.term.dispose(); else x.preview.dispose();
  if (!g.tabs.length && g.parent) removeGroup(g);
  else if (g.active && focused === g) select(g.active);
  else { showActive(g); renderTerms(); renderSide(); }
  if (x.term) setTimeout(poll, 500);
}

/// What a right click on a tab offers.
function tabMenu(e, x) {
  e.preventDefault();
  const items = [
    { title: t('Split right'), icon: 'splitright', run: () => duplicateInto(x, splitGroup(x.group, 'row')) },
    { title: t('Split down'), icon: 'splitdown', run: () => duplicateInto(x, splitGroup(x.group, 'col')) }];
  if (x.temporary) items.push({ sep: true }, { title: t('Keep this tab'), run: () => keepTab(x) });
  items.push({ sep: true }, { title: t('Close'), run: () => closeTab(x) });
  if (x.group.tabs.length > 1) items.push({ title: t('Close the others in this group'), run: () => { for (const y of [...x.group.tabs]) if (y !== x) closeTab(y); } });
  menuAt(e.clientX, e.clientY, items);
}

function splitMenu(anchor, g) {
  menu(anchor, [
    { title: t('Split right (⌘\\)'), icon: 'splitright', run: () => duplicateInto(g.active, splitGroup(g, 'row')) },
    { title: t('Split down (⌘⌥\\)'), icon: 'splitdown', run: () => duplicateInto(g.active, splitGroup(g, 'col')) }]);
}


function tabEl(x, g) {
  const tip = x.kind === 'preview' ? (x.temporary ? t('{path} — shown for a look: the next file clicked takes this tab. A double click keeps it.', { path: x.name }) : x.name)
    : x.ended ? t('{name} is closed. Return in it connects again.', { name: titleOf(x) })
    : x.temporary ? t('{name} — attached for a look: the next session clicked takes this tab (this one detaches). A double click keeps it.', { name: titleOf(x) }) : x.command || `ssh ${x.host}`;
  // A tab of another host than the one shown (the window switched since
  // it opened) says which on the tab.
  const where = x.host === host ? null : el('span.where', { title: t('On {host}, not the host this window shows', { host: x.host }) }, x.host);
  const tab = el('div.ttab', { class: (x === g.active ? 'active' : '') + (x.ended ? ' ended' : '') + (x.temporary ? ' temporary' : ''), title: tip, draggable: true,
    onclick: () => select(x), ondblclick: () => keepTab(x), oncontextmenu: e => tabMenu(e, x) },
    icon(x.kind === 'shell' ? 'terminal' : x.kind === 'preview' ? 'doc' : 'play'), where, el('span.label', titleOf(x)),
    el('button.close', { title: x.ended || x.kind === 'preview' ? t('Close') : t('Close this tab: hang up on it (a session lives on)'), onclick: e => { e.stopPropagation(); closeTab(x); } }, icon('close')));
  tab.ondragstart = e => { clearTimeout(dragForgets); dragged = x; e.dataTransfer.setData(TABDRAG, String(x.uid)); e.dataTransfer.effectAllowed = 'move'; };
  // The native drop can come after the source's dragend: the tab is
  // forgotten a moment later.
  tab.ondragend = () => { clearDropMarks(); clearTimeout(dragForgets); dragForgets = setTimeout(() => { dragged = null; }, 1000); };
  return tab;
}

/// Every group's tab row: the tabs, +, and at the right the terminals'
/// light/dark switch and the split button. The layout is saved with
/// every change, since every change comes here.
function renderTerms() {
  for (const g of groups()) {
    clear(g.row);
    g.el.classList.toggle('focused', g === focused);
    for (const x of g.tabs) g.row.append(tabEl(x, g));
    g.row.append(el('button.add', { title: t('A new shell on {host} (⌘T), a new tmux or screen session, an interactive job', { host }), onclick: e => { focused = g; plusMenu(e.currentTarget); } }, icon('plus')),
      el('div.spacer'),
      g.toggle,
      el('button.splitbtn', { title: t('A second group of tabs beside this one (⌘\\) or under it (⌘⌥\\), showing what this tab shows'), onclick: e => splitMenu(e.currentTarget, g) }, icon('splitright')));
    showActive(g);
  }
  saveLayout();
}

// MARK: what the window brings back

// The groups and their tabs, kept on this machine with every change
// under the host the window shows, so that the host's window opens as
// it was closed: the terminals are connected again, each on its host,
// running its command (a shell in its directory, a session attached),
// the previews show their files. Tabs of one-off actions are not kept
// (`restore`). A window switched to another host keeps its tabs and
// saves them under that host from then on.
const layoutKey = () => `console-layout-${host}`;

function serialize(node) {
  if (node.tabs) {
    const kept = node.tabs.filter(x => x.restore);
    return { tabs: kept.map(x => x.term
      ? { host: x.host, kind: x.kind, name: x.name, command: x.again || x.command, temporary: x.temporary }
      : { host: x.host, kind: 'preview', path: x.name, name: x.title, size: x.size, temporary: x.temporary }),
      active: kept.indexOf(node.active), theme: node.theme };
  }
  return { dir: node.dir, sizes: node.sizes, children: node.children.map(serialize) };
}

/// Saved with every change. The group with the keyboard is its place
/// among the groups that come back: one with no tab to keep does not.
function saveLayout() {
  if (restoring || !root) return;
  const kept = groups().filter(g => g.tabs.some(x => x.restore));
  try { localStorage.setItem(layoutKey(), JSON.stringify({ layout: serialize(root), focused: kept.indexOf(focused) })); } catch {}
}

/// The saved layout, put back; false when there is none worth it (the
/// window then opens with a shell).
function restoreLayout() {
  let saved = null;
  try { saved = JSON.parse(localStorage.getItem(layoutKey())); } catch {}
  if (!saved || !saved.layout) return false;
  const pending = [];
  // The tree first, with what each group is to hold. A group with no
  // tab to bring back (its tabs were all one-off actions) is not made,
  // the way a group whose last tab closes goes; a split with one child
  // left is that child, one with none is dropped.
  const build = node => {
    if (node.tabs) {
      if (!node.tabs.length) return null;
      const g = newGroup(node.theme === 'dark' || node.theme === 'light' ? node.theme : null);
      pending.push([g, node]);
      return g;
    }
    const made = (node.children || []).map((c, i) => [build(c), Math.max(0.05, Number(node.sizes && node.sizes[i]) || 1)]).filter(([c]) => c);
    if (!made.length) return null;
    if (made.length === 1) return made[0][0];
    const children = made.map(([c]) => c);
    const n = { dir: node.dir === 'col' ? 'col' : 'row', children, sizes: made.map(([, size]) => size) };
    for (const c of children) c.parent = n;
    return n;
  };
  let tree;
  try { tree = build(saved.layout); } catch { return false; }
  if (!tree) return false;
  restoring = true;
  root = tree;
  const all = groups();
  focused = all[saved.focused] || all[0];
  renderLayout();
  for (const [g, node] of pending) {
    node.tabs.forEach((tab, i) => {
      const sel = false;
      const h = tab.host || host;   // a layout saved before tabs had hosts: the window's
      let x;
      if (tab.kind === 'preview') x = openPreviewIn(g, { path: tab.path, name: tab.name, size: tab.size }, !tab.temporary, sel, h);
      else x = openTabIn(g, tab.kind, tab.name, tab.command, !!tab.temporary, { select: sel, host: h });
      if (i === node.active) g.active = x;
    });
    if (!g.active) g.active = g.tabs[0] || null;
    showActive(g);
  }
  restoring = false;
  if (focused.active) select(focused.active); else renderTerms();
  return true;
}

async function init() {
  await startPage();
  document.title = host;
  topbar.append(hostButton);
  // The system's look changed: the groups that follow it do too.
  dark.addEventListener('change', () => { for (const g of groups()) if (!g.theme) applyGroupTheme(g); });
  await listen('console-host', ({ host: h }) => switchHost(h));
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
  commonKeys();
  // No narrower than the Usage card's title (the interval and its
  // buttons) on one line, the Files tab's buttons, and the processes'
  // columns with their last one's head.
  resizableWidth(sideEl, 'console-side-width', 280, 640, { collapse: setSideHidden, collapsed: () => sideHidden });
  document.addEventListener('keydown', e => {
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.key === 'r') { e.preventDefault(); if (error) { error = null; start(); } else poll(); }
    if (mod && e.key === 't') { e.preventDefault(); openTab('shell', '', null); }
    if (mod && e.shiftKey && e.key.toLowerCase() === 's') { e.preventDefault(); toggleSide(); }
    // ⌘\ splits the group right, ⌘⌥\ down; ⌘1 to ⌘9 give a group the keyboard.
    if (mod && e.code === 'Backslash') { e.preventDefault(); splitFocused(e.altKey ? 'col' : 'row'); }
    if (mod && !e.shiftKey && !e.altKey && e.key >= '1' && e.key <= '9') {
      const g = groups()[Number(e.key) - 1];
      if (g) { e.preventDefault(); if (g.active) select(g.active); else setFocused(g); }
    }
  });
  document.addEventListener('visibilitychange', () => { if (document.hidden) stop(); else if (!error) start(); });
  nativeTabDrops();
  restoring = true;
  root = focused = newGroup();
  renderLayout();
  renderSide();
  pageReady();
  start();
  // As the window was closed, or a shell.
  if (!restoreLayout()) { restoring = false; openTab('shell', '', null); }
}

init();
