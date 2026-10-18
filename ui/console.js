// The Console window of a host. The column on the left has a tab per
// kind of thing: Status (what the host is doing — uptime and load, CPU,
// memory, swap (a swap file added until the next boot), GPUs, disks (a
// removable drive's volumes mount, unmount and eject from here; a double
// click shows one in the Files tab), the network, the busiest processes
// (a click copies a cell, the right button sends one a signal), who is logged in — read every few
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
import { invoke, listen, startPage, pageReady, t, el, clear, append, icon, fmtBytes, fmtDuration, fmtFileDate, currentState, watchState, hostColor, statusSummary, dialog, toast, menu, menuAt, popup, closeWindow, toggleFullscreen, resizableWidth, LOCAL, hostName, REAL_DPR } from './shellder.js';
import * as keys from './keys.js';
import { xtermTheme, themeIsDark, setThemeFiles, withAlpha, paletteBackground } from './themes.js';
import { createFilesView, folderPicker, DRAG, dropRows } from './filesview.js';
import { createPreview } from './preview.js';
import { createPorts } from './ports.js';

let host = new URLSearchParams(location.search).get('host');   // the host shown: the column's, new tabs'
// The quick terminal (windows.rs, `quick=1`): the same page with nothing
// but one group of terminals, no bar, no column, no tab row; its layout
// is not kept, Esc at the prompt and the keyboard going elsewhere hide
// the window, and closing its last tab hides it with a fresh shell.
const quick = new URLSearchParams(location.search).get('quick') === '1';
// On a display with one pixel per point, the WebGL renderer draws its
// glyph atlas at that one pixel: grainy letters, none of the smoothing
// the DOM's text gets. Told the ratio is two, xterm draws its canvases
// and glyphs at twice the size and the browser scales them down, which
// comes out close to the DOM's letters. The real ratio is `REAL_DPR`
// (shellder.js), for what scales a native event's position by it.
if (REAL_DPR < 2) { try { Object.defineProperty(window, 'devicePixelRatio', { get: () => 2, configurable: true }); } catch {} }
// The host shown may be this machine (`LOCAL`): the column reads it, the
// Files tab is its own tree, the shells run here. Its name in the bar
// and the title, and a file's source on it for a preview.
const hostTitle = () => host === LOCAL ? t('Local') : host;
const sourceOf = h => h === LOCAL ? { kind: 'local' } : { kind: 'host', host: h };
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
  if (polling || document.hidden || quick) return;
  polling = true;
  const h = host;
  try {
    const parts = shownParts();
    if (procTree) parts.delete('processes');   // the tree is read instead of the busiest
    const s = await invoke('console_snapshot', { host, parts: [...parts] });
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
    if (procTree && shows('processes')) tree = await readTree();
  } catch (e) {
    if (h !== host) { polling = false; poll(); return; }
    error = String(e);
    stop();
  }
  polling = false;
  procPending = false;
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
// Memory, swap and a GPU's memory are in binary units (GiB), disks and
// traffic in decimal ones (GB), each as its own tools count it.
const ofTotal = (g, binary = false) => t('{used} of {total}', { used: fmtBytes(g.used, binary), total: fmtBytes(g.total, binary) });
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
    if (snap.memory) rows.push(gauge(t('Memory'), snap.memory.used / snap.memory.total, ofTotal(snap.memory, true)));
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
  if (snap.swap) rows.push(gauge(t('Swap'), snap.swap.used / snap.swap.total, ofTotal(snap.swap, true), add));
  else if (add) rows.push(el('div.gauge', el('div.row', el('span.k', t('Swap')), el('span.v', t('none')), add)));
  for (const f of (files || []).filter(f => f.temporary)) {
    rows.push(el('div.gauge', el('div.row', el('span.caption.k', { title: f.path }, f.path), el('span.caption.v', t('{size}, until the next boot', { size: fmtBytes(f.size, true) })),
      act(t('Switch this swap file off and remove it'), 'trash', () => removeSwap(f)))));
  }
  return rows;
}

/// Remove a swap file the Console added, after asking: what is in it
/// goes back into memory first.
async function removeSwap(f) {
  const r = await dialog({ title: t('Remove the swap file {path}?', { path: f.path }),
    message: t('What is in it ({size}) goes back into memory, then the file is deleted.', { size: fmtBytes(f.size, true) }),
    action: t('Remove'), destructive: true });
  if (r.ok) swapAction('remove', f.path, 0);
}

/// Ask for a swap file: its size and where it goes. The name offered is
/// the first of `/shellder.swap`, `/shellder.swap2`, … that is
/// not on as swap already (the host numbers it again if the file is
/// there all the same).
function swapDialog() {
  return new Promise(resolve => {
    const size = el('input.text', { type: 'text', value: '4', spellcheck: false, inputmode: 'decimal', style: 'width:80px' });
    const taken = new Set((snap.swapFiles || []).map(f => f.path));
    let free = `/${SWAP_MARK}.swap`;
    for (let n = 2; taken.has(free); n++) free = `/${SWAP_MARK}.swap${n}`;
    const path = el('input.text', { type: 'text', value: free, spellcheck: false });
    const done = v => { back.remove(); resolve(v); };
    const ok = el('button.primary', { onclick: () => {
      const gb = Number(size.value.trim().replace(',', '.'));
      if (!(gb > 0) || gb > 1024) { toast(t('The size is a number of GiB, up to 1024.'), true); return; }
      const p = path.value.trim();
      const name = p.split('/').pop();
      if (!p.startsWith('/') || !/^[A-Za-z0-9\/_.+-]+$/.test(p) || p.includes('..') || !name.includes(SWAP_MARK)) {
        toast(t('An absolute path with plain characters, its name carrying “{mark}”.', { mark: SWAP_MARK }), true);
        return;
      }
      done({ path: p, size: Math.round(gb * 1024) });
    } }, t('Add'));
    const box = el('div.dialog', el('h3', t('Add swap on {host}', { host: hostName(host) })),
      el('p.secondary', t('A swap file, made and switched on now, gone at the next boot (nothing is written to fstab). Its name carries “{mark}”, which is how the Status tab tells it from the host\'s own swap and offers to remove it. The host wants root for it: through sudo, or a password it asks you for.', { mark: SWAP_MARK })),
      el('div.row', el('span', { style: 'width:120px' }, t('Size')), size, el('span.caption', 'GiB')),
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
/// at once where the account may, after a password where the host wants
/// one (`diskAction`).
async function swapAction(action, path, size) {
  try {
    const r = await invoke('console_swap', { host, action, path, size });
    if (r.cancelled) return;
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
    el('button.act', { title: t('Read it again now ({key})', { key: keyName('refresh') }), onclick: () => { if (error) { error = null; start(); } else poll(); } }, icon('refresh'))];
}

function gpuCard() {
  if (!shows('gpu') || !snap.gpus.length) return null;
  return card(t('GPUs'), ...snap.gpus.map(g => el('div.gauges',
    el('div.caption', `${g.index} · ${g.name}`),
    gauge(t('Load'), (g.utilization ?? 0) / 100, g.utilization === null ? '—' : `${g.utilization.toFixed(0)}%`),
    g.memory ? gauge(t('Memory'), g.memory.used / g.memory.total, ofTotal(g.memory, true)) : null)));
}

/// Mount or unmount a volume, or eject a drive, on the host; the host's
/// own words (where udisks put it) or `said` in a toast, and a reading
/// right after. A host that wants a password for it (udisks asks polkit,
/// which over ssh has nobody to ask; sudo) is run again on a terminal of
/// the app's, where its question is answered from the vault or in the
/// prompt window, like ssh's own; a question cancelled there ends it,
/// with nothing to say.
async function diskAction(action, device, said) {
  try {
    const r = await invoke('console_disk', { host, action, device });
    if (r.cancelled) return;
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
/// sudo for another user's, the password answered as for a disk action
/// (`diskAction`).
async function signalProcess(p, sig) {
  const r = await dialog({ title: t('Send {signal} to {command} ({pid})?', { signal: 'SIG' + sig.name, command: p.command, pid: p.pid }), message: sig.what(), action: t('Send'), destructive: sig.destructive });
  if (!r.ok) return;
  try {
    const out = await invoke('console_signal', { host, pid: p.pid, signal: sig.name });
    if (out.cancelled) return;
    toast(t('{signal} sent to {command} ({pid})', { signal: 'SIG' + sig.name, command: p.command, pid: p.pid }));
  } catch (e) { toast(String(e), true); }
  setTimeout(poll, 500);
}

function signalMenu(e, p) {
  e.preventDefault();
  menuAt(e.clientX, e.clientY, SIGNALS.map(s => ({ title: s.title(), run: () => signalProcess(p, s) })));
}

/// A click on a cell of the process table copies what the cell holds,
/// whole: the command line that the column cuts short, not the row.
/// (`data-copy` is the cell's value; the right button is the signals'.)
async function copyCell(e) {
  const td = e.target.closest('td');
  if (!td || td.dataset.copy === undefined) return;
  const text = td.dataset.copy;
  try {
    await navigator.clipboard.writeText(text);
    toast(t('Copied: {text}', { text: text.length > 60 ? text.slice(0, 60) + '…' : text }));
  } catch (err) { toast(String(err), true); }
}
const PROC_TIP = () => t('Click: copy the cell. Right click: send it a signal');

// The card shows the busiest processes, or (the button in its title)
// the process tree of the window's active tab: the panes of its tmux
// session, its screen session, its container, or for a plain shell (and
// anything else) the ssh logins of this account. The choice is kept on
// this machine; the tree is read with every reading instead of the
// busiest, and again when another tab is selected.
let procTree = (() => { try { return localStorage.getItem('console-proc-tree') === '1'; } catch { return false; } })();
let tree = null;          // the last tree read: {host, kind, name, rows, error}
let procPending = false;  // the view was just switched: the reading that shows it is on its way
const TREE_KINDS = ['tmux', 'screen', 'docker', 'podman'];

/// What the tree is rooted at: the active tab's session where it has
/// one (on the tab's host, which may not be the column's), the ssh
/// logins of this account otherwise.
function treeWanted() {
  const x = activeTab();
  const kind = x && x.term && TREE_KINDS.includes(x.kind) ? x.kind : 'logins';
  return { host: x ? x.host : host, kind, name: kind === 'logins' ? '' : x.name };
}

async function readTree() {
  const w = treeWanted();
  try { return { ...w, rows: (await invoke('console_tree', w)).rows, error: null }; } catch (e) { return { ...w, rows: [], error: String(e) }; }
}

function toggleProcTree() {
  procTree = !procTree;
  try { localStorage.setItem('console-proc-tree', procTree ? '1' : '0'); } catch {}
  tree = null;
  procPending = true;
  renderSide();
  poll();
}

function processCard() {
  if (!shows('processes')) return null;
  const toggle = el('button.act', { title: procTree ? t('Show the busiest processes') : t('Show the process tree of the active tab: its tmux or screen session, its container, or the ssh logins of this account'), onclick: toggleProcTree },
    icon(procTree ? 'list' : 'tree'));
  // The headings sort the busiest; a tree keeps its order.
  const th = (key, label, cls) => procTree ? el('th', { class: cls || '' }, label)
    : el('th', { class: (cls || '') + (procSort.key === key ? ' sorted' : ''), title: t('Sort by this column'), onclick: () => sortProcesses(key) },
      label, procSort.key === key ? el('span.dir', procSort.up ? ' ▲' : ' ▼') : null);
  const head = el('tr', th('pid', t('PID')), th('user', t('User')), th('cpu', t('CPU %'), 'num'), th('memory', t('Mem %'), 'num'), th('command', t('Cmd')));
  const table = rows => el('div.crow.procsrow', el('table.procs', head, rows));
  // A cell shows `text` (or `shown`, the text with its decorations) and copies `text`.
  const cell = (spec, text, attrs = {}, shown = null) => el(spec, { dataset: { copy: text }, ...attrs }, shown ?? text);
  const reading = el('div.crow', el('div.caption', t('Reading…')));
  if (!procTree) {
    if (!snap.processes.length && !procPending) return null;
    const rows = sortedProcesses().map(p => el('tr.proc', { title: PROC_TIP(), onclick: copyCell, oncontextmenu: e => signalMenu(e, p) },
      cell('td.num', String(p.pid)), cell('td', p.user), cell('td.num', p.cpu.toFixed(1)), cell('td.num', p.memory.toFixed(1)), cell('td.cmd', p.command, { title: p.command })));
    return el('div', el('div.card-title', t('Processes'), el('div.spacer'), toggle), el('div.card', snap.processes.length ? table(rows) : reading));
  }
  const w = tree || treeWanted();
  const what = (w.kind === 'logins' ? t('the ssh logins of this account') : `${w.kind}: ${w.name}`) + (w.host !== host ? ` · ${w.host}` : '');
  const num = v => v === null ? '—' : v.toFixed(1);
  let body;
  if (!tree) body = reading;
  else if (tree.error) body = el('div.crow', el('div.red.selectable', tree.error));
  else if (!tree.rows.length) body = el('div.crow', el('div.caption', t('Nothing found: the session is gone, or its processes are not in the process list of this host.')));
  else body = table(tree.rows.map(p => el('tr.proc', { title: [p.args, p.tty ? t('on {tty}', { tty: p.tty }) : null, PROC_TIP()].filter(Boolean).join('\n'), onclick: copyCell, oncontextmenu: e => signalMenu(e, p) },
    cell('td.num', String(p.pid)), cell('td', p.user), cell('td.num', num(p.cpu)), cell('td.num', num(p.memory)),
    cell('td.cmd', p.args, {}, [el('span.indent', { style: `margin-left:${p.depth * 10}px` }, p.depth ? '└ ' : ''), p.args]))));
  return el('div', el('div.card-title', t('Process tree'), el('div.spacer'), toggle),
    el('div.card', el('div.crow', el('div.caption', { title: what }, what)), body));
}

function loginCard() {
  if (!shows('logins') || !snap.logins.length) return null;
  return card(t('Logged in'), ...snap.logins.map(l => el('div.row',
    el('span', l.user), el('span.caption', [l.tty, l.since, l.from].filter(Boolean).join(' · ')))));
}

function renderStatus() {
  const body = el('div.stats');
  if (error) {
    body.append(el('div', el('div.card-title', t('Could not read {host}', { host: hostName(host) })), el('div.card', el('div.crow',
      el('div.red.selectable', error),
      el('div', { style: 'margin-top:8px' }, el('button', { onclick: () => { error = null; renderSide(); start(); } }, t('Try again')))))));
  } else if (!snap) {
    body.append(el('div.caption', t('Reading {host}…', { host: hostName(host) })));
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
  openTab('shell', path, null);
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
    // On this machine the tree is its own: nothing to upload to it or
    // download from it, and no Files window of its own.
    const local = h === LOCAL;
    const view = createFilesView(box, { host: h, single: true, transferHeight: 120, onUpload: local ? null : upload, onDownload: local ? null : download, onOpen: openPreview, onShell: shellAt, dropOutside,
      onWindow: local ? null : () => invoke('open_window', { which: 'files', host: h }) });
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
  const paths = await picker().open({ multiple: true, title: t('Upload to {host}:{dir}', { host: hostName(host), dir }) });
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

/// This machine (Local, as the Files window calls it), then the hosts of the list, the one shown checked, each
/// host with its colour: a click switches this window to it. A host that
/// already has a Console window is brought up instead (the app knows
/// which).
function hostMenu(anchor) {
  const s = currentState();
  const m = el('div.menu.hosts');
  let close = null;
  const item = (alias, mark, label) => {
    const current = alias === host;
    return el('div.item', { class: current ? 'current' : '', onclick: () => { close(); if (!current) invoke('console_switch', { host: alias }).catch(e => toast(String(e), true)); } },
      mark, el('span.label', label), current ? icon('check') : null);
  };
  m.append(item(LOCAL, icon('laptop'), t('Local')), el('div.sep'));
  for (const h of (s ? s.hosts : [])) m.append(item(h.alias, el('span.dot', { class: hostColor(s.statuses[h.alias], s.locked.includes(h.alias)) }), h.alias));
  close = popup(anchor, m);
}

const hostLabel = el('span.label', hostTitle());
const hostButton = el('button.hostpick', { title: t('Switch this window to another host, or to this machine'), onclick: e => hostMenu(e.currentTarget) }, hostLabel, icon('chevrondown'));

/// The window shows `h` from now on (the app's `console-host`, after a
/// pick in the menu). The tabs stay as they are, each on the host it
/// runs on; the column reads the new host, what it keeps per host (the
/// tab open, the cards, the terminals' background) is the new host's,
/// and new tabs open there. The layout is kept under the host shown.
function switchHost(h) {
  if (h === host) return;
  // The quick terminal: its shells are hung up and start again on the
  // new host; there is no column to switch.
  if (quick) {
    host = h;
    document.title = t('Quick terminal');
    for (const x of terms) {
      if (!x.term) continue;
      if (!x.ended && x.id !== null) invoke('term_close', { id: x.id }).catch(() => {});
      x.id = null;
      x.host = h;
      x.name = '';
      x.command = null;
      x.again = null;
      connect(x);
    }
    renderTerms();
    return;
  }
  stop();
  keepFilesScroll();
  host = h;
  document.title = hostTitle();
  hostLabel.textContent = hostTitle();
  snap = null; prevTicks = null; cpuPercent = null; prevNet = {}; netRates = {}; error = null;
  files = filesTabs.get(host) || null;
  ports.dispose();
  ports = makePorts();
  side = savedSide();
  renderSide();
  renderTerms();
  start();
}

// MARK: the host wheel

/// The hosts as the wheel shows them: this machine, then the list.
function wheelHosts() {
  const s = currentState();
  return [LOCAL, ...(s ? s.hosts.map(h => h.alias) : [])];
}

let wheel = null;        // the wheel on screen: {move(dx, dy)}, or null
let chordTimer = null;   // the hold of the wheel's modifiers under way

const MOD_FLAGS = { Ctrl: 'ctrlKey', Alt: 'altKey', Shift: 'shiftKey', Meta: 'metaKey' };
const MODIFIER_KEYS = new Set(['Shift', 'Control', 'Alt', 'Meta']);
const ARROWS = { ArrowUp: [0, -1], ArrowDown: [0, 1], ArrowLeft: [-1, 0], ArrowRight: [1, 0] };
const WHEEL_HOLD = 160;   // ms the modifiers are held before the wheel shows: a quick ⌘⇧T does not flash it

/// The modifiers of the wheel's keys (⌘⇧ by default: host-prev's first
/// key, or host-next's), as the event's flags; none for a key without.
function wheelChord() {
  const k = keys.keysOf('host-prev')[0] || keys.keysOf('host-next')[0] || '';
  return k.split('+').slice(0, -1).map(m => MOD_FLAGS[m]).filter(Boolean);
}

/// Whether the event holds these modifiers and no other.
const holdsExactly = (e, mods) => Object.values(MOD_FLAGS).every(f => !!e[f] === mods.includes(f));

/// The host wheel, a game's weapon wheel for the hosts. Holding the
/// modifiers of its keys (⌘⇧) for a moment draws a ring of this machine
/// and the hosts over the terminals, this machine at the top, the host
/// shown marked; an arrow moves the mark to the next host along the
/// circle the way the arrow points on screen (at the top → walks
/// clockwise and ← back, on the right side ↓ walks clockwise and ↑
/// back), one host a press, staying at the far side that way; the
/// pointer over a slice marks it too.
/// Letting the modifiers go switches the window to the host marked (as
/// the host menu does); Esc or a click outside puts the wheel away
/// unchanged; any other key with the modifiers held (⌘⇧T) closes it and
/// does what it always did. ⌘⇧↑ and ⌘⇧↓ (host-prev, host-next) open it
/// at once, the mark moved that way.
function watchWheelChord() {
  const stop = () => { clearTimeout(chordTimer); chordTimer = null; };
  window.addEventListener('keydown', e => {
    if (wheel) return;
    const mods = wheelChord();
    if (mods.length && MODIFIER_KEYS.has(e.key) && holdsExactly(e, mods)) {
      if (!chordTimer) chordTimer = setTimeout(() => { chordTimer = null; if (!wheel) openWheel(mods); }, WHEEL_HOLD);
    } else stop();
  }, true);
  window.addEventListener('keyup', e => { if (!holdsExactly(e, wheelChord())) stop(); }, true);
  window.addEventListener('blur', stop);
}

/// host-prev, host-next: the wheel, opened if need be, its mark walked
/// up or down the circle.
function wheelMove(dx, dy, e) {
  clearTimeout(chordTimer);
  chordTimer = null;
  if (!wheel) openWheel(Object.values(MOD_FLAGS).filter(f => e && e[f]));
  wheel.move(dx, dy);
}

function openWheel(mods) {
  const hosts = wheelHosts();
  const s = currentState();
  const n = hosts.length;
  let index = Math.max(0, hosts.indexOf(host));
  // Big enough for the names, within the window.
  const R = Math.min(Math.max(190, 70 + n * 15), Math.min(innerWidth, innerHeight) / 2 - 28);
  const lr = R * 0.72, size = 2 * R + 16;
  const NS = 'http://www.w3.org/2000/svg';
  const sv = (tag, attrs) => { const x = document.createElementNS(NS, tag); for (const [k, v] of Object.entries(attrs)) x.setAttribute(k, v); return x; };
  const svg = sv('svg', { class: 'ring', viewBox: `${-R - 8} ${-R - 8} ${size} ${size}`, width: size, height: size });
  // The mark: a beam of the accent from the centre, fading out towards
  // the rim, over the slice marked.
  const grad = sv('radialGradient', { id: 'wheelglow', cx: 0, cy: 0, r: R, gradientUnits: 'userSpaceOnUse' });
  for (const [at, alpha, light] of [['0%', 0, 70], ['16%', 0.85, 55], ['45%', 0.55, 20], ['100%', 0, 0]]) grad.append(sv('stop', { offset: at, style: `stop-color: color-mix(in srgb, var(--accent), white ${light}%); stop-opacity: ${alpha}` }));
  // The halo under everything: the middle a little darker, fading out past the rim.
  const halo = sv('radialGradient', { id: 'wheelhalo', cx: 0, cy: 0, r: R * 1.5, gradientUnits: 'userSpaceOnUse' });
  for (const [at, alpha] of [['0%', 0.4], ['35%', 0.3], ['100%', 0]]) halo.append(sv('stop', { offset: at, style: `stop-color: black; stop-opacity: ${alpha}` }));
  // The beam's edges softened.
  const soft = sv('filter', { id: 'wheelsoft', x: '-20%', y: '-20%', width: '140%', height: '140%' });
  soft.append(sv('feGaussianBlur', { stdDeviation: Math.max(4, R / 30) }));
  const defs = sv('defs', {});
  defs.append(grad, halo, soft);
  svg.append(defs, sv('circle', { class: 'halo', cx: 0, cy: 0, r: R * 1.5 }));
  const angle = i => i / n * 2 * Math.PI - Math.PI / 2;
  const P = (rad, a) => `${(rad * Math.cos(a)).toFixed(2)} ${(rad * Math.sin(a)).toFixed(2)}`;
  const nameOf = h => h === LOCAL ? t('Local') : h;
  const statusOf = h => h === LOCAL ? t('This machine') : statusSummary(s && s.statuses[h]);
  const colourOf = h => h === LOCAL ? 'local' : hostColor(s && s.statuses[h], s && s.locked.includes(h));
  const box = el('div.wheel', { style: `width:${size}px;height:${size}px` });
  const slices = [], labels = [];
  hosts.forEach((h, i) => {
    const a0 = angle(i - 0.5), a1 = angle(i + 0.5), am = angle(i);
    // A wedge from the centre to the rim (one host: the whole disc).
    const d = n === 1
      ? `M ${-R} 0 A ${R} ${R} 0 1 1 ${R} 0 A ${R} ${R} 0 1 1 ${-R} 0 Z`
      : `M 0 0 L ${P(R, a0)} A ${R} ${R} 0 0 1 ${P(R, a1)} Z`;
    const g = sv('g', { class: `slice ${colourOf(h)}` });
    g.append(sv('path', { d }));
    g.addEventListener('mouseenter', () => mark(i));
    g.addEventListener('click', () => { mark(i); pick(); });
    svg.append(g);
    slices.push(g);
    // The label over the slice's middle: a dot in the host's colour (a
    // laptop for this machine) and the name, no wider than the slice.
    const x = size / 2 + lr * Math.cos(am), y = size / 2 + lr * Math.sin(am);
    const wide = Math.max(60, 2 * Math.PI * lr / n - 8);
    const label = el('div.wlabel', { style: `max-width:${wide.toFixed(0)}px` },
      h === LOCAL ? icon('laptop') : el('span.dot', { class: colourOf(h) }), el('span.name', nameOf(h)));
    label.at = [x, y];
    labels.push(label);
  });
  const centre = el('div.centre', { style: `max-width:${Math.round(R * 1.1)}px` }, el('div.name'), el('div.status'));
  box.append(svg, ...labels, centre);
  const back = el('div.wheelback', { onmousedown: ev => { if (ev.target === back) close(); } }, box);
  const mark = i => {
    index = (i + n) % n;
    // The beam starts again from the centre for the slice newly marked.
    slices.forEach((g, j) => { if (j === index && !g.classList.contains('marked')) { g.classList.remove('marked'); void g.getBoundingClientRect(); } g.classList.toggle('marked', j === index); });
    labels.forEach((l, j) => l.classList.toggle('marked', j === index));
    centre.firstChild.textContent = nameOf(hosts[index]);
    centre.lastChild.textContent = statusOf(hosts[index]);
  };
  // The mark to the neighbour that lies the way the arrow points: of
  // the two hosts beside it on the circle, the one the step to which
  // goes the arrow's way (at the top, → is the clockwise one; on the
  // right side, ↓ is; from a host at the upper right, ↑ is the one at
  // the top). At the far side that way (↑ at the top, → at the
  // rightmost host) neither does, and the mark stays: the circle is
  // traced with →, ↓, ←, ↑ in turn, the way it is drawn.
  const move = (dx, dy) => {
    if (n < 2) return;
    const a = angle(index), px = Math.cos(a), py = Math.sin(a);
    const cosTo = i => { const b = angle(i), vx = Math.cos(b) - px, vy = Math.sin(b) - py; return (vx * dx + vy * dy) / Math.hypot(vx, vy); };
    const cw = cosTo(index + 1), ccw = cosTo(index - 1);
    if (Math.max(cw, ccw) < 0.1) return;
    mark(cw >= ccw ? index + 1 : index - 1);
  };
  const close = () => {
    back.remove();
    window.removeEventListener('keydown', onKey, true);
    window.removeEventListener('keyup', onUp, true);
    window.removeEventListener('blur', close);
    wheel = null;
  };
  const pick = () => {
    const h = hosts[index];
    close();
    if (h === host) return;
    if (quick) switchHost(h); else invoke('console_switch', { host: h }).catch(err => toast(String(err), true));
  };
  // The keys are the wheel's while it is up: the arrows, ↩ and Esc. A
  // key that is none of those, with the modifiers still held, is a
  // shortcut of the window's (⌘⇧T): the wheel goes and the key goes on
  // to what it always did. The modifiers let go pick on their own
  // key-up, or on the first key pressed or let go without them, for a
  // web view that does not report a modifier's own up.
  const onKey = ev => {
    const dir = ARROWS[ev.key];
    if (!dir && ev.key !== 'Enter' && ev.key !== 'Escape' && !MODIFIER_KEYS.has(ev.key) && mods.every(k => ev[k])) { close(); return; }
    ev.preventDefault();
    ev.stopPropagation();
    if (mods.some(k => !ev[k])) { pick(); return; }
    if (dir) move(dir[0], dir[1]);
    else if (ev.key === 'Enter') pick();
    else if (ev.key === 'Escape') close();
  };
  const onUp = ev => { if (mods.some(k => !ev[k])) pick(); };
  window.addEventListener('keydown', onKey, true);
  window.addEventListener('keyup', onUp, true);
  window.addEventListener('blur', close);
  document.body.append(back);
  // The labels centred on their spots at whole pixels (a half pixel
  // either way, and the text blurs).
  for (const l of labels) { l.style.left = `${Math.round(l.at[0] - l.offsetWidth / 2)}px`; l.style.top = `${Math.round(l.at[1] - l.offsetHeight / 2)}px`; }
  mark(index);
  wheel = { move };
}

// MARK: immersive mode

let immersive = false;   // the terminals alone (⌘⇧I): no bar, column or tab rows

/// The quick terminal's look for a Console window: the top bar, the
/// column and the groups' tab rows go, and on macOS the title bar with
/// its buttons, so the terminals have the whole window; the same key
/// brings everything back. Kept with the layout, so the window comes
/// back as it was closed. The window is then moved by the keys that
/// put it in a half or a quarter of the screen (snap-*), there being
/// no bar to drag it by.
function setImmersive(on) {
  if (quick) return;
  immersive = on;
  document.body.classList.toggle('immersive', on);
  invoke('console_immersive', { on }).catch(() => {});
  // The title bar's coming and going takes the keyboard from the web
  // view (the window's first responder changes with its style, and a
  // focus the page already has does not ask for it back): the terminal
  // lets go and takes it again once that has settled.
  const back = () => { const x = activeTab(); if (x && x.term) { x.term.blur(); x.term.focus(); } };
  setTimeout(back, 120);
  setTimeout(back, 500);
  saveLayout();
}

/// The window in a half of the screen (`h` 'left' or 'right', `v` 'top'
/// or 'bottom'), a quarter when both, the whole work area with 'full';
/// an axis not given keeps the half the window is in now.
function snapWindow(h, v) {
  if (quick) return;
  invoke('window_snap', { h: h || null, v: v || null }).catch(err => toast(String(err), true));
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
  out.push({ key: 'sessions', title: t('Sessions'), icon: 'terminal', count: sessions.length, rows: sessionRows,
    empty: !snap ? (error ? t('Could not read {host}: {error}', { host: hostName(host), error }) : t('Reading {host}…', { host: hostName(host) }))
      : t('Neither tmux nor screen is installed on {host}.', { host: hostName(host) }) });
  if (snap && (snap.slurm !== null || snap.pbs !== null)) {
    const jobs = [...(snap.slurm || []).map(x => ({ kind: 'slurm', x })), ...(snap.pbs || []).map(x => ({ kind: 'pbs', x }))];
    out.push({ key: 'jobs', title: t('Jobs'), icon: 'clock', count: jobs.length, head: jobsHead, rows: () => jobs.map(j => j.kind === 'slurm' ? slurmRow(j.x) : pbsRow(j.x)), empty: t('No job of yours is queued or running. The + above starts an interactive one.') });
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
function setSideHidden(hidden, remember = true) {
  sideHidden = hidden;
  if (remember) { try { localStorage.setItem('console-side-hidden', sideHidden ? '1' : '0'); } catch {} }
  renderSide();
  renderTerms();
}
function toggleSide() { setSideHidden(!sideHidden); }

// MARK: the keyboard in the column

// The column is one more place the keyboard goes, left of the leftmost
// group: ⌘⌥← from there (or ⌘⇧L from anywhere) takes it in, ⌘⌥→, Esc or
// ⌘⇧L again bring it back to the terminal. A column folded away is
// unfolded for the keyboard's visit and folds again when it leaves,
// whether by those keys or because what it did opened a tab (a session
// attached, a file previewed, a shell in a directory), whose terminal
// takes the keyboard: the column is browsed and gets out of the way.
// Inside, Tab and ⇧Tab go from tab to tab; the Files tree has its own
// keys (arrows, ↩); the lists of sessions, jobs, containers and ports
// are walked with ↑ ↓ and ↩ does what a click does (⇧↩ a double click:
// a session's tab kept).
let sideKbd = false;    // the keyboard is in the column
let sidePeek = false;   // the column was unfolded for it, and folds when it leaves
const kbdRows = {};     // the row marked in each tab, by the tab's key

const sideRows = () => sideKbd && side !== 'files' && side !== 'status' ? [...sideBody.querySelectorAll('.session:not(.inert)')] : [];

/// The marked row drawn (after every render of the column).
function applyKbdRow() {
  const rows = sideRows();
  for (const r of rows) r.classList.remove('kbd');
  if (!rows.length) return;
  const i = Math.max(0, Math.min(rows.length - 1, kbdRows[side] || 0));
  kbdRows[side] = i;
  rows[i].classList.add('kbd');
  rows[i].scrollIntoView({ block: 'nearest' });
}

/// Where the keyboard goes in the column: the Files tree, or the body
/// (its lists walked by the keys below, the Status tab scrolled).
function focusSideContent() {
  const tree = side === 'files' && files ? files.box.querySelector('.pane .tree') : null;
  // The tree is drawn once the listing arrives: the keyboard follows.
  if (side === 'files' && files && !tree) files.ready.then(() => { if (sideKbd && side === 'files') focusSideContent(); });
  (tree || sideBody).focus({ preventScroll: true });
  applyKbdRow();
}

function enterSide() {
  if (quick || sideKbd) return;
  if (sideHidden) { sidePeek = true; setSideHidden(false, false); }
  sideKbd = true;
  sideEl.classList.add('kbd');
  if (side === 'ports') ports.refresh();
  focusSideContent();
}

/// Back to the terminal (`focus`), or merely noting that the keyboard
/// has gone elsewhere; a column unfolded for the visit folds again.
function leaveSide(focus = true) {
  if (!sideKbd) return;
  sideKbd = false;
  sideEl.classList.remove('kbd');
  for (const r of sideBody.querySelectorAll('.session.kbd')) r.classList.remove('kbd');
  if (sidePeek) { sidePeek = false; setSideHidden(true, false); }
  if (focus) { const x = activeTab(); if (x && x.term) x.term.focus(); else if (x && x.preview) x.preview.focus(); }
}

/// The next or the previous tab of the column (Tab, ⇧Tab).
function stepSideTab(step) {
  const all = sideTabs();
  const i = Math.max(0, all.findIndex(x => x.key === side));
  side = all[(i + step + all.length) % all.length].key;
  try { localStorage.setItem(sideKey(), side); } catch {}
  renderSide();
  if (side === 'ports') ports.refresh();
  focusSideContent();
}

/// The column's keys while the keyboard is in it.
function sideKeys(e) {
  if (!sideKbd) return;
  if (e.key === 'Tab' && !e.metaKey && !e.ctrlKey && !e.altKey) { e.preventDefault(); stepSideTab(e.shiftKey ? -1 : 1); return; }
  if (e.key === 'Escape') { e.preventDefault(); leaveSide(); return; }
  const rows = sideRows();
  if (!rows.length) return;
  const i = kbdRows[side] || 0;
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); kbdRows[side] = i + (e.key === 'ArrowDown' ? 1 : -1); applyKbdRow(); }
  else if (e.key === 'Home' || e.key === 'End') { e.preventDefault(); kbdRows[side] = e.key === 'Home' ? 0 : rows.length - 1; applyKbdRow(); }
  else if (e.key === 'Enter') {
    e.preventDefault();
    const r = rows[Math.min(i, rows.length - 1)];
    if (e.shiftKey && r.ondblclick) r.dispatchEvent(new MouseEvent('dblclick', { bubbles: true })); else r.click();
  }
}

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
    if (current.head) sideBody.append(current.head());
    const rows = current.rows();
    if (!rows.length) sideBody.append(el('div.note', current.empty || ''));
    else append(sideBody, rows);
  }
  if (top) sideBody.scrollTop = top;
  applyKbdRow();
}

/// A path as a word of the host's shell: quoted, a leading `~` left
/// for the shell to expand.
function shellPath(p) { return p === '~' ? '~' : p.startsWith('~/') ? '~/' + shellWord(p.slice(2)) : shellWord(p); }

/// The dialog a new tmux or screen session is made with: its name (or
/// none, and the tool numbers it), the directory it starts in (its home
/// when empty; the folder button picks one from a tree of the host's
/// folders, or this machine's own dialog for Local) and a command to
/// run in it instead of the shell, which the session ends with.
async function newSession(kind) {
  const r = await new Promise(resolve => {
    const field = placeholder => el('input.text', { placeholder, spellcheck: false, autocomplete: 'off', autocapitalize: 'off' });
    const name = field(t('Optional'));
    const dir = field(t('Home'));
    const cmd = field(t('Shell'));
    const choose = el('button', { title: t('Choose a folder on {host}', { host: hostName(host) }), onclick: e => pickDirectory(e.currentTarget, dir) }, icon('folder'));
    const done = v => { back.remove(); resolve(v); };
    const ok = el('button.primary', { onclick: () => done({ ok: true, name: name.value.trim(), dir: dir.value.trim(), cmd: cmd.value.trim() }) }, t('Open'));
    const label = text => el('span', { style: 'width:90px; flex:none' }, text);
    const box = el('div.dialog', el('h3', t('New {kind} session', { kind })),
      el('div.row', label(t('Name')), name),
      el('div.row', label(t('Directory')), dir, choose),
      el('div.row', label(t('Command')), cmd),
      el('div.buttons', el('button', { onclick: () => done({ ok: false }) }, t('Cancel')), ok));
    const back = el('div.backdrop', { onkeydown: e => { if (e.key === 'Escape') done({ ok: false }); if (e.key === 'Enter' && e.target.tagName === 'INPUT') ok.click(); } }, box);
    document.body.append(back);
    name.focus();
  });
  if (!r.ok) return;
  const { name, dir, cmd } = r;
  // tmux takes the directory and the command as options of its own;
  // screen starts where the shell is and runs the command in place of
  // it, so the directory is a `cd` before it and the command goes
  // through `sh -c`, one word to screen.
  const make = (flags, tool) => tool === 'tmux'
    ? ['tmux new-session', flags, dir ? `-c ${shellPath(dir)}` : '', cmd ? shellWord(cmd) : ''].filter(Boolean).join(' ')
    : [dir ? `cd ${shellPath(dir)} &&` : '', 'screen', flags, cmd ? `sh -c ${shellWord(cmd)}` : ''].filter(Boolean).join(' ');
  const command = make(name ? `-${kind === 'tmux' ? 's' : 'S'} ${shellWord(name)}` : '', kind);
  // Brought back, the session is attached, or made again when it is gone;
  // one that named itself cannot be found again.
  const again = !name ? null : make(kind === 'tmux' ? `-A -s ${shellWord(name)}` : `-R ${shellWord(name)}`, kind);
  openTab(kind, name || t('new session'), command, false, { restore: !!name, again });
  setTimeout(poll, 1500);
}

/// The folder button of the session dialog: on this machine, its own
/// folder dialog; on a host, a tree of its folders (the Files model's
/// listings, started here when the Files tab has not yet), at the
/// directory typed so far or the home. The folder picked goes into
/// `field`.
async function pickDirectory(anchor, field) {
  if (host === LOCAL) {
    const folder = await window.__TAURI__.dialog.open({ directory: true, title: t('Choose a folder on {host}', { host: hostName(host) }) });
    if (folder) field.value = folder;
    return;
  }
  let s;
  try { s = await invoke('files_open', { host }); } catch (e) { toast(String(e), true); return; }
  const k = Object.keys(s.panes).find(k => s.panes[k].source.kind === 'host' && s.panes[k].source.host === host) || 'left';
  const p = s.panes[k];
  folderPicker(anchor, { where: hostName(host), root: field.value.trim() || p.home, home: p.home, above: true,
    list: path => invoke('files_subdirectories', { host, pane: k, path }), pick: path => { field.value = path; field.focus(); } });
}

/// The head of the Sessions tab: a + for a new tmux or screen session
/// (whichever the host has; a menu when both).
/// The Sessions tab: a part per tool the host has, tmux then screen,
/// each with its head (a + for a new session) and its sessions, or a
/// word when there is none.
function sessionRows() {
  if (!snap) return [];
  const out = [];
  if (snap.tmux !== null) {
    out.push(head('tmux', t('A new tmux session (it survives this window)'), [{ run: () => newSession('tmux') }]));
    if (snap.tmux.length) out.push(...snap.tmux.map(tmuxRow)); else out.push(el('div.note', t('No tmux session is running.')));
  }
  if (snap.screen !== null) {
    out.push(head('screen', t('A new screen session (it survives this window)'), [{ run: () => newSession('screen') }]));
    if (snap.screen.length) out.push(...snap.screen.map(screenRow)); else out.push(el('div.note', t('No screen session is running.')));
  }
  return out;
}

/// The head of the Jobs tab: a + for an interactive job.
function jobsHead() {
  const items = [];
  if (snap && snap.slurm !== null) items.push({ title: t('Interactive Slurm job (srun --pty bash)'), run: () => openTab('slurm', t('interactive'), 'srun --pty bash -l', false, { restore: false }) });
  if (snap && snap.pbs !== null) items.push({ title: t('Interactive PBS job (qsub -I)'), run: () => openTab('pbs', t('interactive'), 'qsub -I', false, { restore: false }) });
  return head(t('Jobs'), t('An interactive job, in a tab'), items);
}

/// A column tab's head: its name and, with anything to start, a +.
function head(title, tip, items) {
  return el('div.sect.tabhead', title, el('div.spacer'),
    items.length ? act(tip, 'plus', anchor => items.length === 1 ? items[0].run() : menu(anchor, items)) : null);
}

// MARK: the terminal tabs

// A group's terminals' background, and its previews' apart from it:
// dark or light, chosen with the button in its tab row beside the split
// button (`g.theme` for the terminals, `g.previews` for the previews,
// 'dark' or 'light'; null until chosen: the system's look, since a
// prompt drawn for a dark terminal is hard to read on white). The button
// acts on the kind of tab shown: with a terminal up it turns the
// terminals, with a preview up the previews, so a dark shell can sit
// beside a light file. A group split off another starts with its
// choices, and they are kept with the layout. The tabs keep the
// window's colours.
const groupDark = g => g.theme ? g.theme === 'dark' : dark.matches;
const previewDark = g => g.previews ? g.previews === 'dark' : dark.matches;
const chosen = v => v === 'dark' || v === 'light' ? v : null;

// The terminals' settings (Settings; `TerminalView` in app/model.rs
// spells out every default): the letters, the cursor, the scrollback,
// the colours for light and for dark terminals (themes.js), how
// see-through they are, and their manners. Read from the state and
// applied to every terminal as they change (`applyTerminalSettings`).
const TSET_DEFAULT = { font: '', lineHeight: 1.15, cursor: 'block', cursorBlink: true, scrollback: 10000, themeLight: 'default', themeDark: 'default', opacity: 1, blur: false, padding: 6, copyOnSelect: false, pasteProtection: true, confirmClose: true, notifications: true, shellIntegration: true, webgl: false };
const tset = () => (currentState() && currentState().settings && currentState().settings.terminal) || TSET_DEFAULT;
const FONT_STACK = 'ui-monospace, "SF Mono", Menlo, Consolas, monospace';
const fontFamily = () => { const f = (tset().font || '').trim(); return f ? `${f}, ${FONT_STACK}` : FONT_STACK; };
// The window is built transparent (windows.rs): the page paints the
// bars and the column, and the terminals' background with the opacity
// Settings give it, so the setting reaches every window at once.
const alpha = () => tset().opacity;
/// The palette of a light or a dark terminal, as Settings have it.
const paletteId = isDark => isDark ? tset().themeDark : tset().themeLight;

function theme(isDark) {
  return xtermTheme(paletteId(isDark), isDark, alpha());
}

/// What the group's body paints under the terminals: the palette's
/// background at the terminals' opacity.
const bodyTint = isDark => withAlpha(paletteBackground(paletteId(isDark), isDark), alpha());

/// The group's body and its terminals in the terminals' colours (the
/// palette's background on the body too, so terminal and room are one;
/// the palette says whether what is drawn is dark, whatever side it
/// was chosen for), its previews in theirs, and its button for the tab
/// shown.
function applyGroupTheme(g) {
  const isDark = groupDark(g);
  const th = theme(isDark);
  const drawnDark = themeIsDark(paletteId(isDark), isDark);
  g.el.classList.toggle('dark', drawnDark);
  g.el.classList.toggle('light', !drawnDark);
  g.el.classList.toggle('preview-dark', previewDark(g));
  g.el.classList.toggle('preview-light', !previewDark(g));
  // The body paints the background, see-through or not; a see-through
  // terminal's own is wholly transparent (themes.js), whichever renderer.
  g.body.style.background = bodyTint(isDark);
  for (const x of g.tabs) if (x.term) { x.term.options.theme = th; onlyBodyPaints(x); }
  applyToggle(g);
}

/// xterm paints its theme's background on its scroll element, inline,
/// with every change of colours: with an alpha it would stack on the
/// group body's and the terminal would look opaque where the room round
/// it is see-through, so only the body paints (app.css says so too; this
/// holds when a web view still has an older app.css).
function onlyBodyPaints(x) {
  const e = x.box.querySelector('.xterm-scrollable-element');
  if (e) e.style.setProperty('background-color', 'transparent', 'important');
}

let appliedTset = null;
/// The settings reached the terminals: every option that changed, on
/// every terminal, and the room round them.
function applyTerminalSettings(s) {
  const ts = s && s.settings && s.settings.terminal;
  if (!ts) return;
  const key = JSON.stringify(ts);
  if (key === appliedTset) return;
  appliedTset = key;
  document.documentElement.style.setProperty('--term-pad', `${ts.padding}px`);
  document.body.classList.toggle('translucent', ts.opacity < 1);
  document.documentElement.style.setProperty('--alpha', String(ts.opacity));
  // The user's theme files, read again with every change of the
  // settings (a new file is chosen in the settings window, which read
  // it; here it is read for the colours).
  invoke('terminal_themes').then(files => { setThemeFiles(files); for (const g of groups()) applyGroupTheme(g); }).catch(() => {});
  for (const x of terms) {
    if (!x.term) continue;
    const o = x.term.options;
    o.fontFamily = fontFamily();
    o.lineHeight = ts.lineHeight;
    o.cursorStyle = ts.cursor;
    o.cursorBlink = ts.cursorBlink;
    o.scrollback = ts.scrollback;
    // See-through or not is an option the WebGL renderer bakes into its
    // colours (opaque backgrounds without it, for crisp letters): a
    // change of it gets a fresh renderer, which the DOM one needs not.
    const wantT = ts.opacity < 1;
    const crossed = o.allowTransparency !== wantT;
    o.allowTransparency = wantT;
    if (!ts.webgl && x.gl) { x.gl.dispose(); x.gl = null; }
    else if (ts.webgl && x.gl && crossed) { x.gl.dispose(); x.gl = null; tryWebgl(x); }
    else if (ts.webgl && !x.gl) tryWebgl(x);
    if (crossed) x.term.refresh(0, x.term.rows - 1);
  }
  for (const g of groups()) applyGroupTheme(g);
  for (const x of terms) if (x.fit && !x.box.classList.contains('hidden')) x.fit.fit();
}

/// The button says what it would do to the kind of tab shown: a preview's
/// background with a preview up, the terminals' otherwise.
function applyToggle(g) {
  const preview = g.active && !g.active.term;
  const isDark = preview ? previewDark(g) : groupDark(g);
  g.toggle.title = preview
    ? (isDark ? t('A light background for the previews') : t('A dark background for the previews'))
    : (isDark ? t('A light background for the terminals') : t('A dark background for the terminals'));
  g.toggle.replaceChildren(icon(isDark ? 'sun' : 'moon'));
}

function toggleGroupTheme(g) {
  if (g.active && !g.active.term) g.previews = previewDark(g) ? 'light' : 'dark';
  else g.theme = groupDark(g) ? 'light' : 'dark';
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
// For a probe script (AGENTS.md, SHELLDER_PROBE): the tabs and their
// terminals, which the page otherwise keeps to itself.
window.__shellder = { terms };
let root = null;         // the layout: a group or a split
let focused = null;      // the group with the keyboard
let zoomed = null;       // the group shown alone (⌘⇧↩), or null
const closed = [];       // terminal tabs closed, last first reopened (⌘⇧T, ⌘Z)
let restoring = false;   // the saved layout is being put back: not saved over meanwhile
const activeTab = () => focused ? focused.active : null;
let nextUid = 1;

// A shell's tab is named by the shell (OSC 0 or 2, when it sets a
// title), else after its directory: the one it is in, when it says
// (OSC 7, the shell integration), else the one it was started in.
const titleOf = x => {
  if (x.kind === 'preview') return x.title;
  if (x.kind !== 'shell') return `${x.kind}: ${x.name}`;
  if (x.osc) return x.osc;
  const dir = x.cwd || x.name;
  return dir ? `${t('Shell')}: ${dir.split('/').filter(Boolean).pop() || '/'}` : t('Shell');
};

/// The groups, left to right and top to bottom.
function groupsOf(node) { return node.tabs ? [node] : node.children.flatMap(groupsOf); }
const groups = () => root ? groupsOf(root) : [];

/// A group, with `theme` as its terminals' background and `previews` as
/// its previews' (null: the system's).
function newGroup(theme = null, previews = null) {
  const g = { tabs: [], active: null, parent: null, theme, previews };
  // The row: the tabs, which scroll sideways when more than fit, the
  // blank after them (the window's handle, like the top bar's) and at
  // the right the buttons that stay in sight whatever the tabs do. Only
  // the blank moves the window: a tab or the gap beside it never does,
  // so a drag that starts on a tab is the tab's.
  g.tabsEl = el('div.tabs');
  g.ends = el('div.ends');
  g.row = el('div.termtabs', g.tabsEl, el('div.spacer', { 'data-tauri-drag-region': true }), g.ends);
  // A row with more tabs than fit scrolls sideways: a mouse's wheel (up
  // and down) as well as a trackpad's sideways swipe, which the browser
  // does by itself.
  g.tabsEl.addEventListener('wheel', e => {
    if (e.ctrlKey || e.metaKey || g.tabsEl.scrollWidth <= g.tabsEl.clientWidth) return;
    const d = Math.abs(e.deltaX) >= Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
    if (!d) return;
    e.preventDefault();
    g.tabsEl.scrollLeft += e.deltaMode === 1 ? d * 16 : d;
  }, { passive: false });
  // The + follows the last tab while they fit, and goes to the right
  // beside the other buttons when they do not (`addOut`), so that it is
  // never scrolled out of reach; decided again whenever the row's room
  // changes.
  g.addOut = false;
  new ResizeObserver(() => placeAdd(g)).observe(g.tabsEl);
  g.toggle = el('button.theme-toggle', { onclick: () => toggleGroupTheme(g) });
  g.body = el('div.body');
  // A mousedown anywhere in the group gives it the keyboard. The rows are
  // not drawn again here: the click that follows wants the element it
  // started on.
  g.el = el('div.group', { onmousedown: () => setFocused(g) }, g.row, g.body);
  applyGroupTheme(g);
  takesTabs(g);
  // Not while the group is hidden behind a zoomed one: fitted to no
  // room, the terminal would tell the host it is two columns wide.
  new ResizeObserver(() => { if (g.active && g.active.fit && g.body.offsetHeight) g.active.fit.fit(); }).observe(g.body);
  return g;
}

/// The + of a group's row: after the tabs when they fit the row, first
/// of the buttons at its right when they overflow. With the + out the
/// tabs have exactly its room less, so the test reads the same either
/// way and the + does not hop back and forth.
function placeAdd(g) {
  if (!g.add || !g.tabsEl.clientWidth) return;
  const out = g.tabsEl.scrollWidth > g.tabsEl.clientWidth;
  if (out === g.addOut) return;
  g.addOut = out;
  g.row.classList.toggle('scrolling', out);
  if (out) g.ends.prepend(g.add); else g.tabsEl.append(g.add);
}

function setFocused(g) {
  focused = g;
  if (zoomed && zoomed !== g) setZoom(null);
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
  if (zoomed) setZoom(null);
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
    const ra = node.children[i].el.getBoundingClientRect();
    const start = horizontal ? e.clientX : e.clientY;
    const startA = horizontal ? ra.width : ra.height;
    d.classList.add('dragging');
    document.body.style.cursor = horizontal ? 'col-resize' : 'row-resize';
    const move = ev => setShares(node, i, startA + (horizontal ? ev.clientX : ev.clientY) - start);
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

/// Child `i` of the split `px` wide (or high) and the child after it the
/// rest of what the two have: the divider between them moved there,
/// neither side under 80 pixels. Their shares and flex follow.
function setShares(node, i, px) {
  const a = node.children[i].el, b = node.children[i + 1].el;
  const ra = a.getBoundingClientRect(), rb = b.getBoundingClientRect();
  const total = node.dir === 'row' ? ra.width + rb.width : ra.height + rb.height;
  const sum = node.sizes[i] + node.sizes[i + 1];
  px = Math.min(total - 80, Math.max(80, px));
  node.sizes[i] = sum * px / total;
  node.sizes[i + 1] = sum - node.sizes[i];
  a.style.flex = `${node.sizes[i]} 1 0`;
  b.style.flex = `${node.sizes[i + 1]} 1 0`;
}

/// The divider of the group with the keyboard moved `px` that way (⌘⌃
/// and an arrow): the one on that side of it, in the nearest row or
/// column around it that runs that way, or the one on its other side
/// when it is the last there.
function resizeFocused(dx, dy, px = 10) {
  if (zoomed) return;
  const dir = dx ? 'row' : 'col';
  let n = focused, p = focused.parent;
  while (p && p.dir !== dir) { n = p; p = p.parent; }
  if (!p) return;
  const forward = dx > 0 || dy > 0;
  const i = p.children.indexOf(n);
  const d = Math.min(p.children.length - 2, Math.max(0, forward ? i : i - 1));
  const r = p.children[d].el.getBoundingClientRect();
  setShares(p, d, (dir === 'row' ? r.width : r.height) + (forward ? px : -px));
  saveLayout();
}

/// Every split's children the same size (⌘⌃=).
function equalizeSplits() {
  const walk = node => { if (node.tabs) return; node.sizes = node.children.map(() => 1); node.children.forEach(walk); };
  walk(root);
  renderLayout();
}

/// A new group beside `g` (`dir` 'row': to its right, or its left with
/// `before`) or under it ('col'; above with `before`): a sibling in a
/// split of that direction, which is made when `g` is not in one. It
/// starts with `g`'s background.
function splitGroup(g, dir, before = false) {
  const g2 = newGroup(g.theme, g.previews);
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
    const scale = REAL_DPR;
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
    x.preview = createPreview(x.box, { source: sourceOf(h), path: r.path, name: r.name, size: r.size });
  } else if (!x) {
    const box = el('div.termbox.preview');
    x = { host: h, kind: 'preview', name: r.path, title: r.name, size: r.size, command: null, id: null, term: null, fit: null, box, ended: false, temporary: !keep, restore: true, uid: nextUid++ };
    x.preview = createPreview(box, { source: sourceOf(h), path: r.path, name: r.name, size: r.size });
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

// MARK: the terminals' letters

/// The size of the terminals' letters, one for the window's terminals
/// (the ones opened later included) and kept for the next window: ⌘ and
/// the wheel, ⌘+ and ⌘− scale them a point at a time, ⌘0 puts them back.
const FONT_DEFAULT = 13;
const FONT_MIN = 6;
const FONT_MAX = 48;
let fontSize = (() => { try { return Number(localStorage.getItem('console-term-font')) || FONT_DEFAULT; } catch { return FONT_DEFAULT; } })();
fontSize = Math.min(FONT_MAX, Math.max(FONT_MIN, Math.round(fontSize)));

function setFontSize(n) {
  n = Math.min(FONT_MAX, Math.max(FONT_MIN, Math.round(n)));
  if (n === fontSize) return;
  fontSize = n;
  try { localStorage.setItem('console-term-font', String(n)); } catch {}
  for (const x of terms) {
    if (!x.term) continue;
    x.term.options.fontSize = n;
    // A hidden tab has no size to fit to; `select` fits it when it shows.
    if (!x.box.classList.contains('hidden')) x.fit.fit();
  }
}

/// ⌘ and the wheel over a terminal: a trackpad's turn comes as many small
/// events, a mouse's notch as one larger one; they add up, and every so
/// much of the way is a point. Taken before xterm's own wheel handler on
/// the viewport (the capture phase), which would scroll the buffer too.
const WHEEL_POINT = 6;   // pixels of wheel per point of letter: about a mouse's notch
function zoomsOnWheel(box) {
  let acc = 0;
  box.addEventListener('wheel', e => {
    if (!(e.metaKey || e.ctrlKey)) { acc = 0; return; }
    e.preventDefault();
    e.stopPropagation();
    acc += e.deltaMode === WheelEvent.DOM_DELTA_PIXEL ? e.deltaY : e.deltaY * WHEEL_POINT;
    const steps = Math.trunc(acc / WHEEL_POINT);
    if (!steps) return;
    acc -= steps * WHEEL_POINT;
    setFontSize(fontSize - steps);
  }, { capture: true, passive: false });
}

function openTabIn(g, kind, name, command, temporary = false, { restore = true, again = null, select: sel = true, host: h = host } = {}) {
  // A shell tab saved by an earlier version carried its directory as a
  // `cd` command: the directory is its name, and the command goes, so
  // that the shell gets the integration.
  if (kind === 'shell' && command && /^cd /.test(command)) command = null;
  const ts = tset();
  const term = new Terminal({
    fontFamily: fontFamily(),
    fontSize,
    lineHeight: ts.lineHeight,
    cursorStyle: ts.cursor,
    cursorBlink: ts.cursorBlink,
    scrollback: ts.scrollback,
    macOptionIsMeta: true,
    // Only while the terminals are see-through: with it on, the WebGL
    // renderer's glyph atlas is a canvas with alpha, which macOS draws
    // with grey antialiasing only, and the letters come out thin.
    allowTransparency: ts.opacity < 1,
    allowProposedApi: true,
    theme: theme(groupDark(g)),
    // A link the program marked (OSC 8) opens like one found in the text.
    linkHandler: { activate: (e, uri) => openLink(e, uri), allowNonHttpProtocols: false },
    // The scrollbar's width (xterm sizes it by its overview ruler).
    overviewRuler: { width: 9, showTopBorder: false, showBottomBorder: false },
  });
  const fit = new FitAddon.FitAddon();
  term.loadAddon(fit);
  // The width of emoji and East Asian characters as Unicode 11 has them.
  term.loadAddon(new Unicode11Addon.Unicode11Addon());
  term.unicode.activeVersion = '11';
  // OSC 52: a program on the host (tmux with `set-clipboard on`, a vim
  // with its clipboard) writes this machine's clipboard. Never reads it:
  // anything printed on the host could ask, and get what was last copied.
  term.loadAddon(new ClipboardAddon.ClipboardAddon(undefined, {
    readText: () => Promise.resolve(''),
    writeText: (selection, text) => selection === 'c' ? navigator.clipboard.writeText(text).catch(() => {}) : Promise.resolve(),
  }));
  // Web addresses in the text are links: a ⌘-click opens one (`openLink`).
  term.loadAddon(new WebLinksAddon.WebLinksAddon((e, uri) => openLink(e, uri)));
  const search = new SearchAddon.SearchAddon();
  term.loadAddon(search);
  const box = el('div.termbox');
  const x = { host: h, kind, name, command, again, id: null, term, fit, search, box, ended: true, temporary, restore, uid: nextUid++, marks: [], running: null, cwd: null, osc: null };
  terms.push(x);
  addTab(g, x);
  // Opened and measured while the box is on screen (it is, until the
  // group's active tab hides it): a terminal opened out of sight has no
  // size to go by.
  term.open(box);
  fit.fit();
  // Drawn with WebGL where the view gives a context, which keeps a
  // screenful of `cat` or a `htop` smooth; the DOM renderer when it is
  // lost, or when Settings say so. Loaded after the first fit, and fitted
  // again once the new renderer has measured its cells and the page has
  // had a frame: a fit on cells not yet measured makes a two-column
  // terminal.
  if (ts.webgl) tryWebgl(x);
  fit.fit();
  requestAnimationFrame(() => { if (!box.classList.contains('hidden')) fit.fit(); });
  onlyBodyPaints(x);
  dropsPaths(box, x);
  zoomsOnWheel(box);
  linksPaths(x);
  shellMarks(x);
  pasteGuard(x);
  // The shell names the tab (OSC 0, 2); a bell marks it and may tell;
  // a selection goes to the clipboard when Settings say so; the right
  // button has a menu.
  term.onTitleChange(title => { const o = (title || '').trim() || null; if (o !== x.osc) { x.osc = o; renderTabsSoon(); } });
  // The scrollbar shows while the terminal scrolls (app.css, `scrolling`).
  term.onScroll(() => {
    const sc = box.querySelector('.xterm-scrollable-element');
    if (!sc) return;
    sc.classList.add('scrolling');
    clearTimeout(x.scrollTimer);
    x.scrollTimer = setTimeout(() => sc.classList.remove('scrolling'), 900);
  });
  term.onBell(() => rang(x));
  term.onSelectionChange(() => { if (tset().copyOnSelect && term.hasSelection()) navigator.clipboard.writeText(term.getSelection()).catch(() => {}); });
  box.oncontextmenu = e => termMenu(e, x);
  term.onData(data => { if (x.offer) hideOffer(x); if (!x.ended) invoke('term_write', { id: x.id, data }).catch(() => {}); else if (data === '\r') connect(x); });
  // Binary input (a paste that is not text): bytes as a latin1 string.
  term.onBinary(data => { if (!x.ended) invoke('term_write', { id: x.id, data }).catch(() => {}); });
  term.onResize(({ cols, rows }) => { if (!x.ended) invoke('term_resize', { id: x.id, cols, rows }).catch(() => {}); });
  // The keys are `keys.js`'s (the defaults, or the user's own): what the
  // terminal does itself (a copy of its selection, otherwise ⌘C is the
  // interrupt it always was; clearing, selecting all, the letters'
  // size, scrolling; ^A ^E ^U, ESC b and ESC f to the shell, as
  // Terminal.app sends them for ⌘← ⌘→ ⌘⌫ ⌥← ⌥→) is done
  // here and goes no further; the window's go past the terminal to
  // the page, xterm leaving them alone (⌃Tab would type a tab). A ⌘ key
  // bound to nothing does nothing. ⌘V is not taken here: the WebView
  // pastes on its own (xterm takes the paste event) and reading the
  // clipboard as well pasted everything twice. ⌘↩ with a password
  // offered types it, whatever ⌘↩ is otherwise.
  term.attachCustomKeyEventHandler(e => {
    if (e.type !== 'keydown') return true;
    if (x.offer && e.key === 'Enter' && (e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey) { typePassword(x); return false; }
    if (x.pasteAsk && e.key === 'Enter' && (e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey) { x.pasteAsk.paste(); return false; }
    if (x.pasteAsk && e.key === 'Escape') { hidePasteAsk(x); return false; }
    // The quick terminal goes away on Esc at the prompt (the shell
    // integration says when no command is running; in a program, Esc is
    // the program's).
    if (quick && e.key === 'Escape' && !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey && x.marks.length && !x.running) { invoke('quick_hide'); return false; }
    const a = keys.actionFor(e);
    if (!a) return !e.metaKey;
    const send = seq => { if (!x.ended) term.input(seq); };
    switch (a) {
      case 'copy': if (!term.hasSelection()) return true; navigator.clipboard.writeText(term.getSelection()); break;
      case 'select-all': term.selectAll(); break;
      case 'clear': term.clear(); break;
      case 'font-bigger': setFontSize(fontSize + 1); break;
      case 'font-smaller': setFontSize(fontSize - 1); break;
      case 'font-reset': setFontSize(FONT_DEFAULT); break;
      case 'scroll-top': term.scrollToTop(); break;
      case 'scroll-bottom': term.scrollToBottom(); break;
      case 'page-up': term.scrollPages(-1); break;
      case 'page-down': term.scrollPages(1); break;
      case 'line-start': send('\x01'); break;
      case 'line-end': send('\x05'); break;
      case 'line-delete': send('\x15'); break;
      case 'word-back': send('\x1bb'); break;
      case 'word-forward': send('\x1bf'); break;
      case 'search': openSearch(x); break;
      case 'prompt-prev': jumpPrompt(x, -1); break;
      case 'prompt-next': jumpPrompt(x, 1); break;
      case 'select-output': selectOutput(x); break;
      default: return false;
    }
    e.preventDefault();
    e.stopPropagation();
    return false;
  });
  if (sel) select(x); else { if (!g.active) g.active = x; showActive(g); }
  connect(x);
  return x;
}

// MARK: the shell asking for a password

/// A password prompt of the shell's own (sudo's, su's), read off the
/// screen: the cursor's line, once the output has been still for a
/// moment, ends in one and the cursor sits after it. Not ssh's own
/// prompts, which carry a user@host (a login from the shell to another
/// host: not this host's password), nor a key's passphrase.
const PASSWORD_LINE = /(password|passwd|密码)[^@]*:\s*$/i;
const OFFER_MS = 60000;   // how long the offer stays, the bar under it running down
function passwordAsked(x) {
  const b = x.term.buffer.active;
  const row = b.baseY + b.cursorY;
  let line = b.getLine(row);
  if (!line) return null;
  const last = line.translateToString(true).trimEnd();
  if (b.cursorX < last.length) return null;
  // The prompt's line as the shell wrote it: a narrow terminal wraps it
  // over several rows, which xterm marks.
  let text = last;
  for (let r = row; line.isWrapped && r > 0; ) { line = b.getLine(--r); text = line.translateToString(true) + text; }
  if (!PASSWORD_LINE.test(text) || text.includes('@') || /passphrase/i.test(text)) return null;
  return `${row}|${text}`;
}

/// Output arrived: once it has been still for a moment, see whether the
/// shell is waiting for a password that is stored for the host, and
/// offer to type it, over the terminal's top right corner. The offer
/// goes when the user types instead, after a minute, or when the line
/// moves on; a prompt once offered (or declined) is not offered again
/// until a new one shows.
function watchPrompt(x) {
  clearTimeout(x.watch);
  x.watch = setTimeout(() => {
    if (x.ended) return hideOffer(x);
    clearTimeout(x.waitTimer);
    x.waitTimer = setTimeout(() => { if (!x.ended) inputWanted(x); }, INPUT_QUIET_MS - 500);
    const key = passwordAsked(x);
    if (!key) { hideOffer(x); x.offered = null; return; }
    if (key === x.offered) return;
    const stored = (currentState().secrets || {})[x.host] || [];
    if (!stored.includes('password')) return;
    showOffer(x, key);
  }, 500);
}

function showOffer(x, key) {
  hideOffer(x);
  x.offered = key;
  // Whose password it is (the account the host is logged into as), then
  // the one thing to do about it; the keyboard stays with the terminal.
  const r = (currentState().resolved || {})[x.host];
  const who = r && r.user ? `${r.user}@${x.host}` : hostName(x.host);
  x.offer = el('div.offer', { onmousedown: e => e.preventDefault() },
    icon('key'), el('span.what', t("{who}'s password", { who })),
    el('button.act', { onclick: () => typePassword(x) }, t('Type'), el('kbd', '⌘↩')),
    el('button.borderless', { title: t('Not now'), onclick: () => hideOffer(x) }, icon('close')),
    el('div.countdown', { style: `animation-duration: ${OFFER_MS}ms` }));
  x.box.append(x.offer);
  x.offerTimer = setTimeout(() => hideOffer(x), OFFER_MS);
}

function hideOffer(x) {
  clearTimeout(x.offerTimer);
  if (x.offer) x.offer.remove();
  x.offer = null;
}

/// The stored password goes in on the app's side, Return included; the
/// page never has it.
function typePassword(x) {
  hideOffer(x);
  if (x.ended) return;
  invoke('term_type_password', { id: x.id }).catch(e => toast(String(e), true));
  x.term.focus();
}

// MARK: links, search, the shell's marks, pasting, notifications

/// A ⌘-click (Ctrl elsewhere) on a web address, found in the text or
/// marked by the program (OSC 8), opens it in the browser; a plain
/// click is a click, so that nothing opens while the pointer is put
/// somewhere.
function openLink(e, uri) {
  if (!(e.metaKey || e.ctrlKey)) return;
  invoke('open_url', { url: uri }).catch(err => toast(String(err), true));
}

/// Paths in the text are links too: absolute ones, and ones under the
/// shell's directory when it says where it is (the integration). A
/// ⌘-click shows the file in the column's Files tab, selected in its
/// directory (a directory is entered); a tab on another host than the
/// one shown switches the window to that host first, as the host menu
/// does (when another window already shows it, that window comes up
/// and the file is shown in this one's Files window instead); ⌥⌘
/// shows the file in a preview tab of the group.
const PATH_RE = /(?:^|[\s"'`(\[<=:,])((?:\.{1,2})?\/[\w.@%+=,~#$!-]+(?:\/[\w.@%+=,~#$!-]*)*)/g;
function linksPaths(x) {
  x.term.registerLinkProvider({
    provideLinks(row, cb) {
      const line = x.term.buffer.active.getLine(row - 1);
      if (!line) return cb(undefined);
      // The text, and for each of its characters the column it is in
      // (a wide character takes two cells).
      let text = '';
      const col = [];
      for (let c = 0; c < line.length; c++) {
        const cell = line.getCell(c);
        if (!cell) break;
        const chars = cell.getChars();
        if (cell.getWidth() === 0) continue;
        for (let k = 0; k < (chars || ' ').length; k++) col.push(c);
        text += chars || ' ';
      }
      const links = [];
      for (const m of text.matchAll(PATH_RE)) {
        const start = m.index + m[0].length - m[1].length;
        const p = m[1].replace(/[.,:;'"]+$/, '');
        if (p.length < 2 || (p.startsWith('.') && !x.cwd)) continue;
        const from = col[start], to = col[start + p.length - 1];
        if (from === undefined || to === undefined) continue;
        links.push({ text: p, range: { start: { x: from + 1, y: row }, end: { x: to + 1, y: row } }, activate: e => openPathLink(e, x, p) });
      }
      cb(links.length ? links : undefined);
    },
  });
}

function openPathLink(e, x, p) {
  if (!(e.metaKey || e.ctrlKey)) return;
  let path = p.startsWith('.') ? joinPath(x.cwd, p) : p;
  try { path = decodeURIComponent(path); } catch {}
  if (e.altKey && !p.endsWith('/')) { openPreviewIn(x.group, { path, name: path.split('/').pop(), size: null }, true, true, x.host); return; }
  revealOnHost(x.host, path, p.endsWith('/'));
}

/// `path` on `h` in the column's Files tab: the window switched to `h`
/// when it shows another host, and the Files window of `h` when another
/// window has it.
async function revealOnHost(h, path, isDir) {
  // Not on the host: ssh's own words name this machine's files (its
  // ControlSocket, its config), so the path is looked for here next.
  const there = async on => { try { return await invoke('files_exists', { source: sourceOf(on), path }); } catch { return false; } };
  if (!(await there(h))) {
    if (h !== LOCAL && await there(LOCAL)) { toast(t('Not on {host}: {path} is on this machine', { host: hostName(h), path })); h = LOCAL; }
    else { toast(t('{path} is not on {host}', { path, host: hostName(h) }), true); return; }
  }
  if (h !== host) {
    try { await invoke('console_switch', { host: h }); } catch (err) { toast(String(err), true); return; }
    // The switch comes back as `console-host`, a moment later; another
    // window already showing the host keeps this one as it is.
    for (let i = 0; i < 20 && host !== h; i++) await new Promise(r => setTimeout(r, 50));
    if (host !== h) { invoke('open_window', { which: 'files', host: h }).catch(err => toast(String(err), true)); return; }
  }
  if (isDir) { showInFiles(path.replace(/\/+$/, '') || '/'); return; }
  revealInFiles(path);
}

/// The Files tab showing the file's directory, with the file selected.
async function revealInFiles(path) {
  const dir = path.replace(/\/[^/]*$/, '') || '/';
  await showInFiles(dir);
  try {
    const pane = files && files.view.hostPane();
    if (pane) await invoke('files_select', { host, pane, paths: [path] });
  } catch (e) { toast(String(e), true); }
}

/// `rel` (`./x`, `../y/z`) under `dir`, the dots resolved; no URL
/// parsing, so a `#` or a `?` in a directory's name is a character.
function joinPath(dir, rel) {
  const parts = (dir.replace(/\/+$/, '') + '/' + rel).split('/');
  const out = [];
  for (const seg of parts) {
    if (seg === '' || seg === '.') continue;
    if (seg === '..') out.pop(); else out.push(seg);
  }
  return '/' + out.join('/') + (rel.endsWith('/') ? '/' : '');
}

// xterm's WebGL renderer blends with SRC_ALPHA / ONE_MINUS_SRC_ALPHA for
// the alpha channel too, so a glyph's antialiased edge leaves the canvas
// pixel short of opaque (0.5 over an opaque cell gives 0.75), and what
// is behind the canvas, the page's light background, shows through round
// every letter as a pale fringe on a coloured cell. The contexts asked
// for while the addon loads get a blendFunc that keeps the alpha whole
// (ONE / ONE_MINUS_SRC_ALPHA for alpha).
let loadingWebgl = false;
(() => {
  const orig = HTMLCanvasElement.prototype.getContext;
  HTMLCanvasElement.prototype.getContext = function (kind, attrs) {
    const gl = orig.call(this, kind, attrs);
    if (loadingWebgl && gl && (kind === 'webgl2' || kind === 'webgl') && !gl.__shellderBlend) {
      gl.__shellderBlend = true;
      const sep = gl.blendFuncSeparate.bind(gl), plain = gl.blendFunc.bind(gl);
      gl.blendFunc = (src, dst) => src === gl.SRC_ALPHA && dst === gl.ONE_MINUS_SRC_ALPHA ? sep(src, dst, gl.ONE, gl.ONE_MINUS_SRC_ALPHA) : plain(src, dst);
    }
    return gl;
  };
})();

/// WebGL for the terminal, where the view gives a context; back to the
/// DOM renderer when it is lost.
function tryWebgl(x) {
  if (x.gl) return;
  try {
    loadingWebgl = true;
    const gl = new WebglAddon.WebglAddon();
    gl.onContextLoss(() => { gl.dispose(); if (x.gl === gl) x.gl = null; });
    x.term.loadAddon(gl);
    loadingWebgl = false;
    x.gl = gl;
    // Measured again once the fonts are in: a glyph atlas built on a
    // fallback font would draw the wrong cells until the next resize.
    if (document.fonts && document.fonts.status !== 'loaded') document.fonts.ready.then(() => { if (x.term && !x.box.classList.contains('hidden')) { x.fit.fit(); x.term.refresh(0, x.term.rows - 1); } });
  } catch (e) { loadingWebgl = false; console.warn('WebGL renderer not available', e); }
}

/// The search strip over the terminal's top right (⌘F): the text, the
/// count, the previous and the next match (⇧↩ and ↩), Esc closes it and
/// the keyboard goes back to the terminal. Every match is marked, the
/// one reached brighter; a capital in the text makes the search mind
/// the case.
function openSearch(x) {
  if (!x || !x.term) return;
  if (x.searchEl) { const i = x.searchEl.querySelector('input'); i.focus(); i.select(); return; }
  const input = el('input.text', { type: 'text', placeholder: t('Find'), spellcheck: false, autocomplete: 'off' });
  const count = el('span.count');
  const decorations = { matchBackground: '#ffcc0066', matchOverviewRuler: '#ffcc00', activeMatchBackground: '#ff9f0acc', activeMatchColorOverviewRuler: '#ff9f0a' };
  const find = (dir = 1, incremental = false) => {
    const q = input.value;
    if (!q) { x.search.clearDecorations(); count.textContent = ''; return; }
    const o = { caseSensitive: /[A-Z]/.test(q), incremental, decorations };
    if (dir < 0) x.search.findPrevious(q, o); else x.search.findNext(q, o);
  };
  input.oninput = () => find(1, true);
  input.onkeydown = e => {
    if (e.key === 'Enter') { e.preventDefault(); e.stopPropagation(); find(e.shiftKey ? -1 : 1); }
    else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); closeSearch(x); }
  };
  const strip = el('div.search', { onmousedown: e => { if (e.target !== input) e.preventDefault(); } },
    icon('search'), input, count,
    el('button.borderless', { title: t('The previous match (⇧↩)'), onclick: () => find(-1) }, icon('chevronup')),
    el('button.borderless', { title: t('The next match (↩)'), onclick: () => find(1) }, icon('chevrondown')),
    el('button.borderless', { title: t('Close (Esc)'), onclick: () => closeSearch(x) }, icon('close')));
  x.searchEl = strip;
  x.searchResults = x.search.onDidChangeResults(r => {
    count.textContent = r.resultCount > 0 ? `${r.resultIndex + 1}/${r.resultCount}` : r.resultCount < 0 ? `${r.resultIndex + 1}/1000+` : input.value ? t('No match') : '';
  });
  x.box.append(strip);
  input.focus();
}

function closeSearch(x) {
  if (!x.searchEl) return;
  x.search.clearDecorations();
  x.searchResults.dispose();
  x.searchEl.remove();
  x.searchEl = null;
  x.term.focus();
}

/// What the shell says about itself when it runs with the shell
/// integration (core/shellint.rs; or one of its own, iTerm2's, VS Code's,
/// Ghostty's, which speak the same OSC 133): A before the prompt, B
/// after it, C as the command starts, D with its exit status as it ends;
/// OSC 7 its directory. Every A is a marker on its line (`x.marks`,
/// moved by xterm as the buffer scrolls, gone with the line), which ⌘↑
/// and ⌘↓ jump between; the command is read off the screen between B
/// and C, for the question before closing the tab and the word when it
/// is done. OSC 9 and OSC 777 are a program's own notification.
function shellMarks(x) {
  const term = x.term;
  const current = () => { const m = x.marks[x.marks.length - 1]; return m && !m.marker.isDisposed ? m : null; };
  term.parser.registerOscHandler(133, data => {
    const [kind, ...rest] = data.split(';');
    switch (kind) {
      case 'A': {
        const marker = term.registerMarker(0);
        if (!marker) break;
        const last = current();
        // Two A's on one line (a theme's integration beside ours) are one.
        if (last && last.marker.line === marker.line) { marker.dispose(); break; }
        const m = { marker, b: null, c: null, cmd: null, start: null, exit: null };
        marker.onDispose(() => { const i = x.marks.indexOf(m); if (i >= 0) x.marks.splice(i, 1); });
        x.marks.push(m);
        while (x.marks.length > 1000) dropMark(x.marks[0]);
        break;
      }
      case 'B': { const m = current(); if (m && !m.b) { m.b = term.registerMarker(0); m.bCol = term.buffer.active.cursorX; } break; }
      case 'C': { const m = current(); if (m) { m.cmd = commandText(x, m); if (m.b) { m.b.dispose(); m.b = null; } m.c = term.registerMarker(0); m.start = Date.now(); x.running = m; } break; }
      case 'D': {
        const m = x.running;
        x.running = null;
        if (!m) break;
        m.exit = rest[0] === undefined || rest[0] === '' || Number.isNaN(Number(rest[0])) ? null : Number(rest[0]);
        commandDone(x, m);
        break;
      }
    }
    return true;
  });
  term.parser.registerOscHandler(7, url => {
    try { const u = new URL(url); if (u.protocol === 'file:') x.cwd = decodeURIComponent(u.pathname) || '/'; } catch {}
    if (x.kind === 'shell' && !x.osc) renderTabsSoon();
    return true;
  });
  term.parser.registerOscHandler(9, text => { if (!/^4;/.test(text)) notify(x, hostName(x.host), text); return true; });
  term.parser.registerOscHandler(777, data => { const p = data.split(';'); if (p[0] === 'notify') notify(x, p[1] || hostName(x.host), p.slice(2).join(';')); return true; });
}

/// A prompt's markers let go (the A's disposal takes it out of `x.marks`).
function dropMark(m) {
  if (m.b) m.b.dispose();
  if (m.c) m.c.dispose();
  m.marker.dispose();
}

/// The command typed at the prompt `m`, read off the screen as it
/// starts: from the prompt's end (B) to the line before the cursor, the
/// wrapped lines joined; without a B (a theme that draws its prompt
/// anew), the prompt's line after what looks like the prompt.
function commandText(x, m) {
  const b = x.term.buffer.active;
  let last = b.baseY + b.cursorY;
  if (b.cursorX === 0) last--;
  const first = m.b && !m.b.isDisposed ? m.b.line : m.marker.line;
  if (first < 0) return null;
  let text = '';
  for (let r = first; r <= Math.max(first, last); r++) {
    const line = b.getLine(r);
    if (!line) continue;
    const str = line.translateToString(true);
    text += r === first && m.b ? str.slice(m.bCol) : str;
  }
  if (!m.b) text = text.replace(/^.*[$%#>❯» ] ?(?=\S)/, '');
  text = text.trim();
  return text || null;
}

const LONG_COMMAND = 10;   // seconds after which a command's end is told

/// A command is done: the tab marked when it is not up, and a word when
/// it took a while and nobody was watching (the window behind another,
/// or the tab behind another).
function commandDone(x, m) {
  const secs = m.start ? (Date.now() - m.start) / 1000 : 0;
  const failed = m.exit !== null && m.exit !== 0;
  const up = x.group.active === x;
  const watched = up && document.hasFocus() && !document.hidden;
  if (watched) return;
  if (!up) { x.done = failed ? 'fail' : 'ok'; renderTerms(); }
  // A failure is always told, a success when it took a while.
  if (!failed && secs < LONG_COMMAND) return;
  const cmd = m.cmd ? (m.cmd.length > 60 ? m.cmd.slice(0, 60) + '…' : m.cmd) : t('A command');
  const time = fmtDuration(Math.round(secs));
  notify(x, hostName(x.host), failed ? t('{cmd} failed (exit status {code}) after {time}', { cmd, code: m.exit, time }) : t('{cmd} finished in {time}', { cmd, time }));
}

/// A command that stopped writing with a question on its last line, the
/// cursor after it (a `[y/N]`, a `Password:`, a `name?`): it waits for
/// input, which is told when nobody is watching, once per such line.
const WAITING_LINE = /(?:[?？]|\[[^\]]*[/|][^\]]*\]|\((?:y(?:es)?\/no?|[yn]\/[yn]|ynq?)\)|\b(?:password|passphrase|passcode|pin|otp|token|verification code|username|login|user name|continue|proceed|confirm|answer|choice|select(?:ion)?|input|enter)\b[^:：]{0,40}[:：])\s*$/i;
const INPUT_QUIET_MS = 3000;   // how long the output must have rested
function inputWanted(x) {
  if (!x.running) return;
  const b = x.term.buffer.active;
  const row = b.baseY + b.cursorY;
  const line = b.getLine(row);
  if (!line) return;
  const text = line.translateToString(true).trimEnd();
  if (!text || b.cursorX < text.length || !WAITING_LINE.test(text)) return;
  const key = `${row}|${text}`;
  if (x.askedInput === key) return;
  x.askedInput = key;
  const watched = x.group.active === x && document.hasFocus() && !document.hidden;
  if (watched) return;
  if (x.group.active !== x && !x.done) { x.activity = true; renderTerms(); }
  const cmd = x.running.cmd ? (x.running.cmd.length > 40 ? x.running.cmd.slice(0, 40) + '…' : x.running.cmd) : t('A command');
  notify(x, hostName(x.host), t('{cmd} waits for input: {line}', { cmd, line: text.length > 60 ? text.slice(0, 60) + '…' : text }));
}

/// The bell: the tab marked when it is not up, a word when the window is
/// not in front; not more than once a second per tab.
function rang(x) {
  const now = Date.now();
  if (x.rangAt && now - x.rangAt < 1000) return;
  x.rangAt = now;
  if (x.group.active !== x) { x.bell = true; renderTerms(); }
  if (!document.hasFocus() || document.hidden) notify(x, hostName(x.host), t('The bell rang in {name}', { name: titleOf(x) }));
}

/// A word to the user about the tab: a toast when the tab is up in the
/// window in front, a notification of the system's (and the Dock's
/// attention) otherwise; nothing when Settings say so.
const NOTIFY_BURST = 3, NOTIFY_WINDOW_MS = 10000;
function notify(x, title, body) {
  if (!tset().notifications) return;
  if (document.hasFocus() && !document.hidden && x.group.active === x) { toast(`${title}: ${body}`); return; }
  // A program's own (OSC 9, OSC 777) could come by the hundred: a few in
  // ten seconds per tab, the rest dropped.
  const now = Date.now();
  x.notified = (x.notified || []).filter(at => now - at < NOTIFY_WINDOW_MS);
  if (x.notified.length >= NOTIFY_BURST) return;
  x.notified.push(now);
  invoke('notify', { title, body: body || '' }).catch(() => {});
  try { window.__TAURI__.window.getCurrentWindow().requestUserAttention(2).catch(() => {}); } catch {}
}

/// The lines of the prompts, for the jumps and the output.
const markLines = x => x.marks.map(m => m.marker.line).filter(l => l >= 0).sort((a, b) => a - b);

/// ⌘↑, ⌘↓: the terminal scrolled so that the previous (next) prompt is
/// at its top; past the last, to the bottom; with no prompt marked, to
/// the top or the bottom.
function jumpPrompt(x, dir) {
  const top = x.term.buffer.active.viewportY;
  const lines = markLines(x);
  let target = null;
  if (dir < 0) {
    const before = lines.filter(l => l < top);
    if (before.length) target = before[before.length - 1]; else x.term.scrollToTop();
  } else {
    const after = lines.filter(l => l > top);
    if (after.length) target = after[0]; else x.term.scrollToBottom();
  }
  if (target === null) return;
  x.term.scrollToLine(target);
  frameCommand(x, target);
}

/// The command reached by a jump, framed for a moment: its prompt line
/// down to the line before the next prompt (or the cursor), as a
/// decoration of xterm's that fades.
function frameCommand(x, line) {
  const i = x.marks.findIndex(m => m.marker.line === line);
  if (i < 0) return;
  const b = x.term.buffer.active;
  const next = x.marks[i + 1];
  const end = next && next.marker.line >= 0 ? next.marker.line - 1 : b.baseY + b.cursorY;
  const height = Math.max(1, Math.min(end - line + 1, x.term.rows));
  if (x.frame) x.frame.dispose();
  const d = x.term.registerDecoration({ marker: x.marks[i].marker, width: x.term.cols, height, layer: 'top' });
  if (!d) return;
  d.onRender(e => e.classList.add('jumpframe'));
  x.frame = d;
  setTimeout(() => { if (x.frame === d) { d.dispose(); x.frame = null; } }, 1800);
}

/// The last command whose output is on screen: its first line of output
/// (C) and the line before the prompt after it.
function lastOutput(x) {
  for (let i = x.marks.length - 1; i >= 0; i--) {
    const m = x.marks[i];
    if (!m.c || m.c.isDisposed || m.c.line < 0) continue;
    const next = x.marks[i + 1];
    const b = x.term.buffer.active;
    const to = (next && next.marker.line >= 0 ? next.marker.line : b.baseY + b.cursorY) - 1;
    if (to < m.c.line) return null;
    return { from: m.c.line, to };
  }
  return null;
}

/// The last command's output selected (⌘⇧A, the menu), ready to copy.
function selectOutput(x) {
  const o = lastOutput(x);
  if (!o) { toast(t('No command output is marked: the shell integration tells where a command\'s output starts and ends.')); return; }
  x.term.selectLines(o.from, o.to);
  x.term.scrollToLine(o.from);
}

/// The right button over a terminal.
function termMenu(e, x) {
  e.preventDefault();
  const term = x.term;
  const sel = term.hasSelection();
  menuAt(e.clientX, e.clientY, [
    { title: t('Copy'), icon: 'copy', disabled: !sel, run: () => { if (sel) navigator.clipboard.writeText(term.getSelection()).catch(() => {}); } },
    { title: t('Paste'), icon: 'clipboard', run: () => pasteFromClipboard(x) },
    { title: t('Select all'), run: () => term.selectAll() },
    { title: t("Select the last command's output"), disabled: !lastOutput(x), run: () => { if (lastOutput(x)) selectOutput(x); } },
    { sep: true },
    { title: t('Find…'), icon: 'search', run: () => openSearch(x) },
    { title: t('Clear'), run: () => term.clear() },
  ]);
}

async function pasteFromClipboard(x) {
  if (x.ended) return;
  try {
    const text = await navigator.clipboard.readText();
    if (!text) return;
    if (tset().pasteProtection && /[\r\n]/.test(text) && !x.term.modes.bracketedPasteMode) askPaste(x, text); else x.term.paste(text);
  } catch (err) { toast(String(err), true); }
  x.term.focus();
}

/// Several lines pasted into a shell that is not reading a bracketed
/// paste would run, every line but the last, as they land: the strip
/// asks first (Settings). A program reading a bracketed paste (a shell
/// of today, an editor) gets the lines as one paste and runs nothing,
/// so it is not asked for.
function pasteGuard(x) {
  x.box.addEventListener('paste', e => {
    if (!tset().pasteProtection || x.ended) return;
    const text = e.clipboardData && e.clipboardData.getData('text/plain');
    if (!text || !/[\r\n]/.test(text) || x.term.modes.bracketedPasteMode) return;
    e.preventDefault();
    e.stopPropagation();
    askPaste(x, text);
  }, true);
}

function askPaste(x, text) {
  hidePasteAsk(x);
  const lines = text.split(/\r\n|\r|\n/).filter((l, i, a) => i < a.length - 1 || l !== '');
  const first = lines.find(l => l.trim()) || '';
  const paste = () => { hidePasteAsk(x); if (!x.ended) x.term.paste(text); x.term.focus(); };
  const strip = el('div.offer.paste', { onmousedown: e => e.preventDefault() },
    icon('clipboard'),
    el('span.what', { title: text.length > 2000 ? text.slice(0, 2000) + '…' : text }, lines.length > 1 ? t('Paste {n} lines?', { n: lines.length }) : t('Paste a line ending in a line break? It would run.'), ' ', el('span.line', first.length > 50 ? first.slice(0, 50) + '…' : first)),
    el('button.act', { onclick: paste }, t('Paste'), el('kbd', keys.MAC ? '⌘↩' : 'Ctrl+↩')),
    el('button.borderless', { title: t('Not now (Esc)'), onclick: () => hidePasteAsk(x) }, icon('close')));
  strip.paste = paste;
  x.pasteAsk = strip;
  x.box.append(strip);
}

function hidePasteAsk(x) {
  if (x.pasteAsk) x.pasteAsk.remove();
  x.pasteAsk = null;
}

/// The same thing again in another group: what a split shows there. A
/// terminal's command run once more (a shell in the same directory, the
/// session attached a second time), a preview's file; a plain shell for
/// a tab that is not brought back, or when there is none.
function duplicateInto(x, g) {
  if (!x || (x.term && !x.restore)) return openTabIn(g, 'shell', '', null);
  if (x.term) return openTabIn(g, x.kind, x.kind === 'shell' && x.cwd ? x.cwd : x.name, x.again || x.command, false, { again: x.again, host: x.host });
  return openPreviewIn(g, { path: x.name, name: x.title, size: x.size }, true, true, x.host);
}

/// A second group beside (`dir` 'row') or under ('col') the group with
/// the keyboard, showing what its tab shows (⌘D, ⌘⇧D).
function splitFocused(dir) {
  const x = activeTab();
  const g2 = splitGroup(focused, dir);
  duplicateInto(x, g2);
}

/// Give the group the keyboard, its tab brought up.
function focusGroup(g) { if (g.active) select(g.active); else setFocused(g); }

/// The group after (`step` 1) or before (-1) the one with the keyboard,
/// left to right and top to bottom, round the ends (⌘] and ⌘[).
function focusNeighbourGroup(step) {
  if (zoomed) setZoom(null);
  const all = groups();
  if (all.length < 2) return;
  focusGroup(all[(all.indexOf(focused) + step + all.length) % all.length]);
}

/// The group in that direction from the one with the keyboard (⌘⌥ and
/// an arrow): the nearest of those facing it across that edge, the one
/// sharing the most of the edge when several are as near.
function focusTowards(dx, dy) {
  if (sideKbd) { if (dx > 0) leaveSide(); return; }
  if (zoomed) setZoom(null);
  const r = focused.el.getBoundingClientRect();
  let best = null, bestGap = 0, bestOverlap = 0;
  for (const g of groups()) {
    if (g === focused) continue;
    const s = g.el.getBoundingClientRect();
    const gap = dx > 0 ? s.left - r.right : dx < 0 ? r.left - s.right : dy > 0 ? s.top - r.bottom : r.top - s.bottom;
    const overlap = dx ? Math.min(r.bottom, s.bottom) - Math.max(r.top, s.top) : Math.min(r.right, s.right) - Math.max(r.left, s.left);
    if (gap < -1 || overlap <= 0) continue;
    if (!best || gap < bestGap - 1 || (gap <= bestGap + 1 && overlap > bestOverlap)) { best = g; bestGap = gap; bestOverlap = overlap; }
  }
  if (best) focusGroup(best);
  else if (dx < 0) enterSide();
}

/// The group shown alone in the window, the others hidden (⌘⇧↩), or
/// null: every group again. The keyboard moving to another group, and
/// any change of the layout, bring them back.
function setZoom(g) {
  zoomed = g;
  termBox.classList.toggle('zoomed', !!g);
  // The group and the splits down to it are shown, the rest hidden.
  for (const e of termBox.querySelectorAll('.zoom')) e.classList.remove('zoom');
  for (let n = g; n; n = n.parent) n.el.classList.add('zoom');
  renderTerms();
}

function toggleZoom() { setZoom(zoomed || groups().length < 2 ? null : focused); }

async function connect(x) {
  x.ended = false;
  x.term.reset();
  for (const m of [...x.marks]) dropMark(m);
  x.marks = [];
  x.running = null;
  x.cwd = null;
  x.osc = null;
  if (!x.box.classList.contains('hidden')) x.fit.fit();
  x.term.write(`\x1b[2m${x.host === LOCAL ? t('Starting a shell on this machine…') : t('Connecting to {host}…', { host: x.host })}\x1b[0m\r\n`);
  renderTerms();
  // The temporary tab may be given to another session while this login
  // is under way: the ssh that comes back then is not the tab's any more
  // and is hung up on.
  const gen = x.gen = (x.gen || 0) + 1;
  try {
    const id = await invoke('term_open', { host: x.host, command: x.command, dir: x.kind === 'shell' && !x.command ? x.name || null : null, cols: x.term.cols, rows: x.term.rows });
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
  x.activity = x.bell = x.done = null;
  focused = g;
  if (zoomed && zoomed !== g) setZoom(null);
  showActive(g);
  renderTerms();
  if (focus) requestAnimationFrame(() => { if (x.term) { x.fit.fit(); x.term.focus(); } else x.preview.focus(); });
  else if (x.fit) requestAnimationFrame(() => x.fit.fit());
  renderSide();
  // The process tree follows the active tab.
  if (procTree && shows('processes') && !polling) poll();
}

/// Close the tab: hang up (a tmux client detaches, the session lives on);
/// a preview lets its file go. A group left without a tab goes, unless
/// it is the only one.
async function closeTab(x, force = false) {
  if (!force && x.term && !x.ended && x.running && tset().confirmClose) {
    const r = await dialog({ title: t('Close this tab?'), message: t('{cmd} is still running in it.', { cmd: x.running.cmd || t('A command') }), action: t('Close'), destructive: true });
    if (!r.ok) return;
    if (x.ended || !terms.includes(x)) return;
  }
  if (!x.ended && x.id !== null) invoke('term_close', { id: x.id }).catch(() => {});
  // The quick terminal's last tab closing: a fresh shell for the next
  // time, and the window away.
  if (quick && !terms.some(y => y !== x && y.term)) {
    // The old process's exit is nobody's: its id is dropped first.
    x.id = null;
    x.ended = true;
    connect(x);
    invoke('quick_hide');
    return;
  }
  if (x.term && x.restore) closed.push({ group: x.group, kind: x.kind, name: x.name, command: x.again || x.command, again: x.again, host: x.host });
  const g = detachTab(x);
  terms.splice(terms.indexOf(x), 1);
  if (x.term) x.term.dispose(); else x.preview.dispose();
  if (!g.tabs.length && g.parent) removeGroup(g);
  else if (g.active && focused === g) select(g.active);
  else { showActive(g); renderTerms(); renderSide(); }
  if (x.term) setTimeout(poll, 500);
}

/// ⌘W: the tab with the keyboard goes; with no tab open, the window.
function closeFocusedTab() {
  const x = activeTab();
  if (x) closeTab(x); else if (quick) invoke('quick_hide'); else closeWindow();
}

/// ⌘⌥W: the group with the keyboard goes, every tab of it (the last
/// group stays, empty).
async function closeFocusedGroup() {
  for (const y of [...focused.tabs]) await closeTab(y);
}

/// ⌘⇧T, ⌘Z: the terminal tab closed last comes back, its command run
/// again (a session attached again), in its group when that is still
/// there, else in the group with the keyboard.
function reopenClosed() {
  const c = closed.pop();
  if (!c) return;
  const g = groups().includes(c.group) ? c.group : focused;
  openTabIn(g, c.kind, c.name, c.command, false, { again: c.again, host: c.host });
}

/// The tab after (`step` 1) or before (-1) the one with the keyboard, in
/// its group's row, round the ends.
function selectNeighbour(step) {
  const g = focused;
  if (!g || g.tabs.length < 2) return;
  const n = g.tabs.length;
  select(g.tabs[(g.tabs.indexOf(g.active) + step + n) % n]);
}

/// What a right click on a tab offers.
function tabMenu(e, x) {
  e.preventDefault();
  const items = [
    { title: t('Split right'), icon: 'splitright', run: () => duplicateInto(x, splitGroup(x.group, 'row')) },
    { title: t('Split down'), icon: 'splitdown', run: () => duplicateInto(x, splitGroup(x.group, 'col')) }];
  if (x.temporary) items.push({ sep: true }, { title: t('Keep this tab'), run: () => keepTab(x) });
  items.push({ sep: true }, { title: t('Close'), run: () => closeTab(x) });
  if (x.group.tabs.length > 1) items.push({ title: t('Close the others in this group'), run: async () => { for (const y of [...x.group.tabs]) if (y !== x) await closeTab(y); } });
  menuAt(e.clientX, e.clientY, items);
}

/// The first key of the action as it is written, for a tooltip.
const keyName = id => keys.keysOf(id).map(keys.describe)[0] || '';

function splitMenu(anchor, g) {
  menu(anchor, [
    { title: `${t('Split right')} (${keyName('split-right')})`, icon: 'splitright', run: () => duplicateInto(g.active, splitGroup(g, 'row')) },
    { title: `${t('Split down')} (${keyName('split-down')})`, icon: 'splitdown', run: () => duplicateInto(g.active, splitGroup(g, 'col')) }]);
}


function tabEl(x, g) {
  const tip = x.kind === 'preview' ? (x.temporary ? t('{path} — shown for a look: the next file clicked takes this tab. A double click keeps it.', { path: x.name }) : x.name)
    : x.ended ? t('{name} is closed. Return in it connects again.', { name: titleOf(x) })
    : x.temporary ? t('{name} — attached for a look: the next session clicked takes this tab (this one detaches). A double click keeps it.', { name: titleOf(x) }) : x.command || (x.host === LOCAL ? t('A shell on this machine') : `ssh ${x.host}`);
  // A tab of another host than the one shown (the window switched since
  // it opened) says which on the tab.
  const where = x.host === host ? null : el('span.where', { title: t('On {host}, not the host this window shows', { host: hostName(x.host) }) }, x.host === LOCAL ? t('Local') : x.host);
  // What happened in the tab while it was not up: the bell, a command
  // done (green) or failed (red), new output; gone when it is selected.
  const mark = x.bell ? el('span.mark.bell', { title: t('The bell rang here') })
    : x.done ? el('span.mark', { class: x.done, title: x.done === 'ok' ? t('A command finished here') : t('A command failed here') })
    : x.activity ? el('span.mark.activity', { title: t('New output here') }) : null;
  const tab = el('div.ttab', { class: (x === g.active ? 'active' : '') + (x.ended ? ' ended' : '') + (x.temporary ? ' temporary' : ''), title: tip, draggable: true,
    onclick: () => select(x), ondblclick: () => keepTab(x), oncontextmenu: e => tabMenu(e, x) },
    icon(x.kind === 'shell' ? 'terminal' : x.kind === 'preview' ? 'doc' : 'play'), where, el('span.label', titleOf(x)), mark,
    el('button.close', { title: x.ended || x.kind === 'preview' ? `${t('Close')} (${keyName('close-tab')})` : t('Close this tab ({key}): hang up on it (a session lives on)', { key: keyName('close-tab') }), onclick: e => { e.stopPropagation(); closeTab(x); } }, icon('close')));
  tab.ondragstart = e => { clearTimeout(dragForgets); dragged = x; e.dataTransfer.setData(TABDRAG, String(x.uid)); e.dataTransfer.effectAllowed = 'move'; };
  // The native drop can come after the source's dragend: the tab is
  // forgotten a moment later.
  tab.ondragend = () => { clearDropMarks(); clearTimeout(dragForgets); dragForgets = setTimeout(() => { dragged = null; }, 1000); };
  return tab;
}

/// The tabs' words changed (a title, a directory; htop and ninja set
/// theirs many times a second): the rows drawn once a frame, and the
/// layout not saved, since it holds neither.
let tabsFrame = 0;
function renderTabsSoon() {
  if (tabsFrame) return;
  tabsFrame = requestAnimationFrame(() => { tabsFrame = 0; renderTerms(false); });
}

/// Every group's tab row: the tabs, +, and at the right the terminals'
/// light/dark switch and the split button. The layout is saved with
/// every change, since every change comes here (`save` false: the
/// change was the tabs' words only).
function renderTerms(save = true) {
  for (const g of groups()) {
    clear(g.tabsEl);
    clear(g.ends);
    g.el.classList.toggle('focused', g === focused);
    for (const x of g.tabs) g.tabsEl.append(tabEl(x, g));
    g.add = el('button.add', { title: t('A new shell on {host} ({key})', { host: hostName(host), key: keyName('new-tab') }), onclick: () => { focused = g; openTab('shell', cwdForNew(), null); } }, icon('plus'));
    (g.addOut ? g.ends : g.tabsEl).append(g.add);
    g.ends.append(g.toggle,
      zoomed === g ? el('button.splitbtn', { title: t('Shown alone: bring the other groups back ({key})', { key: keyName('zoom') }), onclick: () => setZoom(null) }, icon('unzoom'))
        : el('button.splitbtn', { title: t('A second group of tabs beside this one ({right}) or under it ({down}), showing what this tab shows', { right: keyName('split-right'), down: keyName('split-down') }), onclick: e => splitMenu(e.currentTarget, g) }, icon('splitright')));
    placeAdd(g);
    showActive(g);
    applyToggle(g);
    // The tab shown stays in sight when the row scrolls.
    const active = g.tabsEl.querySelector('.ttab.active');
    if (active && g.tabsEl.scrollWidth > g.tabsEl.clientWidth) active.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  }
  if (save) saveLayout();
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
      active: kept.indexOf(node.active), theme: node.theme, previews: node.previews };
  }
  return { dir: node.dir, sizes: node.sizes, children: node.children.map(serialize) };
}

/// Saved with every change. The group with the keyboard is its place
/// among the groups that come back: one with no tab to keep does not.
function saveLayout() {
  if (restoring || !root || quick) return;
  const kept = groups().filter(g => g.tabs.some(x => x.restore));
  try { localStorage.setItem(layoutKey(), JSON.stringify({ layout: serialize(root), focused: kept.indexOf(focused), immersive })); } catch {}
}

/// The saved layout, put back; false when there is none worth it (the
/// window then opens with a shell).
function restoreLayout() {
  if (quick) return false;
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
      const g = newGroup(chosen(node.theme), chosen(node.previews));
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
  if (saved.immersive) setImmersive(true);
  restoring = false;
  if (focused.active) select(focused.active); else renderTerms();
  return true;
}

/// Where a new shell starts: the directory of the shell with the
/// keyboard, when it is on the host shown and says where it is (the
/// shell integration); home otherwise.
const cwdForNew = () => { const x = activeTab(); return x && x.term && x.kind === 'shell' && x.host === host && x.cwd ? x.cwd : ''; };

/// The window's actions by name (`keys.js` says which key is which);
/// false for one that is not the window's.
function runAction(a, e) {
  const tab = n => focused.tabs[n];
  switch (a) {
    case 'new-tab': openTab('shell', cwdForNew(), null); break;
    case 'search': openSearch(activeTab() && activeTab().term ? activeTab() : null); break;
    case 'close-tab': closeFocusedTab(); break;
    case 'close-group': closeFocusedGroup(); break;
    case 'close-window': if (quick) invoke('quick_hide'); else closeWindow(); break;
    case 'reopen': reopenClosed(); break;
    case 'new-window': invoke('open_window', { which: 'console', host, fresh: true }); break;
    case 'next-tab': selectNeighbour(1); break;
    case 'prev-tab': selectNeighbour(-1); break;
    case 'tab-last': if (tab(focused.tabs.length - 1)) select(tab(focused.tabs.length - 1)); break;
    case 'split-right': splitFocused('row'); break;
    case 'split-down': splitFocused('col'); break;
    case 'next-group': focusNeighbourGroup(1); break;
    case 'prev-group': focusNeighbourGroup(-1); break;
    case 'group-up': focusTowards(0, -1); break;
    case 'group-down': focusTowards(0, 1); break;
    case 'group-left': focusTowards(-1, 0); break;
    case 'group-right': focusTowards(1, 0); break;
    case 'resize-up': resizeFocused(0, -1); break;
    case 'resize-down': resizeFocused(0, 1); break;
    case 'resize-left': resizeFocused(-1, 0); break;
    case 'resize-right': resizeFocused(1, 0); break;
    case 'equalize': equalizeSplits(); break;
    case 'zoom': toggleZoom(); break;
    case 'fullscreen': toggleFullscreen(); break;
    case 'host-prev': wheelMove(0, -1, e); break;
    case 'host-next': wheelMove(0, 1, e); break;
    case 'immersive': setImmersive(!immersive); break;
    case 'snap-left': snapWindow('left', null); break;
    case 'snap-right': snapWindow('right', null); break;
    case 'snap-up': snapWindow(null, 'top'); break;
    case 'snap-down': snapWindow(null, 'bottom'); break;
    case 'snap-fill': snapWindow('full', 'full'); break;
    case 'refresh': if (error) { error = null; start(); } else poll(); break;
    case 'toggle-side': toggleSide(); break;
    case 'side-focus': if (sideKbd) leaveSide(); else enterSide(); break;
    case 'settings': invoke('open_window', { which: 'termprefs' }); break;
    case 'quit': invoke('quit'); break;
    case 'font-bigger': setFontSize(fontSize + 1); break;
    case 'font-smaller': setFontSize(fontSize - 1); break;
    case 'font-reset': setFontSize(FONT_DEFAULT); break;
    default: {
      const m = /^tab-(\d)$/.exec(a);
      if (!m) return false;
      if (tab(Number(m[1]) - 1)) select(tab(Number(m[1]) - 1));
    }
  }
  return true;
}

async function init() {
  await startPage();
  document.title = hostTitle();
  topbar.append(hostButton, el('div.spacer', { 'data-tauri-drag-region': true }),
    el('button.keysbtn', { title: t('Console settings: the terminals, the quick terminal, the themes, the keys'), onclick: () => invoke('open_window', { which: 'termprefs' }) }, icon('gear')));
  if (quick) { document.body.classList.add('quick'); document.title = t('Quick terminal'); }
  // The system's look changed: the groups that follow it do too.
  dark.addEventListener('change', () => { for (const g of groups()) if (!g.theme || !g.previews) applyGroupTheme(g); });
  await listen('console-host', ({ host: h }) => switchHost(h));
  await listen('term-data', ({ id, data }) => {
    const x = terms.find(y => y.id === id);
    if (!x) return;
    x.term.write(decode(data), () => watchPrompt(x));
    // Output in a tab that is not up marks it, once.
    if (!x.activity && !x.done && !x.bell && x.group.active !== x) { x.activity = true; renderTerms(); }
  });
  await listen('term-exit', ({ id, code }) => {
    const x = terms.find(y => y.id === id);
    if (!x) return;
    x.ended = true;
    x.running = null;
    hideOffer(x);
    hidePasteAsk(x);
    // A clean end (exit, a tmux or screen detach) closes the tab; any
    // other leaves its last words on screen, Return tries again.
    if (code === 0) { closeTab(x); return; }
    x.term.write(`\r\n\x1b[2m[${t('{name} is closed (exit status {code}). Return connects again.', { name: titleOf(x), code })}]\x1b[0m\r\n`);
    renderTerms();
    renderSide();
    setTimeout(poll, 500);
  });
  // No narrower than the Usage card's title (the interval and its
  // buttons) on one line, the Files tab's buttons, and the processes'
  // columns with their last one's head.
  resizableWidth(sideEl, 'console-side-width', 280, 640, { collapse: setSideHidden, collapsed: () => sideHidden });
  // The window's keys (`keys.js`): what the terminal did not take.
  // The letters' size is the window's too, so it works from the column.
  document.addEventListener('keydown', e => {
    const a = keys.actionFor(e);
    if (!a || runAction(a, e) === false) return;
    e.preventDefault();
  });
  document.addEventListener('visibilitychange', () => { if (document.hidden) stop(); else if (!error) start(); });
  nativeTabDrops();
  watchWheelChord();
  // The keyboard in the column: its keys, and its leaving noticed (a
  // tab opened from it takes the keyboard to its terminal).
  sideBody.tabIndex = -1;
  sideEl.addEventListener('keydown', sideKeys);
  sideEl.addEventListener('focusout', e => { if (sideKbd && !(e.relatedTarget && sideEl.contains(e.relatedTarget)) && e.relatedTarget) leaveSide(false); });
  sideEl.addEventListener('mousedown', () => { if (sideKbd) { sideKbd = false; sideEl.classList.remove('kbd'); for (const r of sideBody.querySelectorAll('.session.kbd')) r.classList.remove('kbd'); sidePeek = false; } });
  watchState(applyTerminalSettings);
  // Closing the window with a command running in a tab asks first, the
  // way closing the tab does (Settings).
  let closing = false;
  window.__TAURI__.window.getCurrentWindow().onCloseRequested(async e => {
    if (closing || !tset().confirmClose) return;
    const busy = terms.filter(x => x.term && !x.ended && x.running);
    if (!busy.length) return;
    e.preventDefault();
    const r = await dialog({ title: t('Close this window?'),
      message: busy.length === 1 ? t('{cmd} is still running in a tab.', { cmd: busy[0].running.cmd || t('A command') }) : t('{n} commands are still running in its tabs.', { n: busy.length }),
      action: t('Close'), destructive: true });
    if (r.ok) { closing = true; closeWindow(); }
  });
  // The page going away under the window (a reload: `cargo tauri dev`
  // does one on every change under ui/) is not the window closing, which
  // is what hangs its terminals up: they are hung up here, and in case
  // this never ran, by the next page's term_reset below.
  window.addEventListener('pagehide', () => {
    for (const x of terms) if (x.term && !x.ended && x.id !== null) invoke('term_close', { id: x.id }).catch(() => {});
  });
  restoring = true;
  root = focused = newGroup();
  renderLayout();
  renderSide();
  pageReady();
  if (!quick) start();
  // Whatever a page before this one left in the window goes first, and
  // before a tab of this page opens: a reset after that would take it.
  await invoke('term_reset').catch(() => {});
  // As the window was closed, or a shell.
  if (!restoreLayout()) { restoring = false; openTab('shell', '', null); }
}

init();
