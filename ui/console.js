// The Console window of a host: what it is doing (uptime and load, CPU,
// memory, swap, GPUs, disks, the busiest processes, who is logged in),
// read every few seconds through its master or its session, and its
// sessions: tmux and screen, one of which the window opens in a terminal
// of its own (xterm.js on the screen pty of an `ssh -t host tmux attach`),
// and VNC servers, which open in the viewer through a port forward on the
// master. The questions ssh asks never show
// here: they go down the controlling terminal, answered from the keychain
// or the prompt window.
import { invoke, listen, loadLanguage, t, el, clear, append, icon, fmtBytes, fmtDuration, fmtTime, fmtFileDate, connectState, dialog, toast, commonKeys } from './shellder.js';

const host = new URLSearchParams(location.search).get('host');
const statsBox = document.getElementById('stats');
const footBox = document.getElementById('foot');
const sessionsBox = document.getElementById('sessions');
const termHead = document.getElementById('term-head');
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
  render();
}

function start() { stop(); poll(); timer = setInterval(poll, interval * 1000); }
function stop() { clearInterval(timer); timer = null; }

// MARK: the stats column

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
    el('div.row', el('span.caption', d.filesystem), el('span.caption.v', ofTotal(d))))));
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

function renderStats() {
  clear(statsBox);
  if (error) {
    statsBox.append(el('div', el('div.card-title', t('Could not read {host}', { host })), el('div.card', el('div.crow',
      el('div.red.selectable', error),
      el('div', { style: 'margin-top:8px' }, el('button', { onclick: () => { error = null; render(); start(); } }, t('Try again')))))));
  } else if (!snap) {
    statsBox.append(el('div.caption', { style: 'padding: 12px' }, t('Reading {host}…', { host })));
  } else {
    append(statsBox, [headCard(), usageCard(), gpuCard(), diskCard(), processCard(), loginCard()]);
  }
  clear(footBox);
  const select = el('select', { onchange: e => {
    interval = Number(e.target.value);
    try { localStorage.setItem('console-interval', String(interval)); } catch {}
    if (!error) start();
  } }, INTERVALS.map(n => el('option', { value: n, selected: n === interval }, t('every {n}s', { n }))));
  footBox.append(
    el('span', updatedAt ? t('Updated {time}', { time: fmtTime(updatedAt) }) : ''),
    el('div.spacer'),
    select,
    el('button', { title: t('Read it again now (⌘R)'), onclick: () => { if (error) { error = null; start(); } else poll(); } }, icon('refresh')));
}

// MARK: sessions

function shellWord(s) { return `'${s.replace(/'/g, "'\\''")}'`; }

const isMine = (kind, name) => attachedTo !== null && attachedTo.kind === kind && attachedTo.name === name;
const tabKey = `console-tab-${host}`;
let tab = (() => { try { return localStorage.getItem(tabKey); } catch { return null; } })();

/// A row that opens something in the terminal when clicked.
function row(kind, name, command, title, caption, extra) {
  const mine = isMine(kind, name);
  return el('div.session', { class: mine ? 'current' : '', title: mine ? t('Open in this window') : title, onclick: () => mine ? term.focus() : open(command, { kind, name }) },
    icon(mine ? 'play' : 'terminal', mine ? 'accent' : ''),
    el('div.text', el('div.row', el('span.name', name), el('span.caption', caption)), extra));
}

/// A row that opens something on the host here, through a port forward.
function portRow(iconName, name, caption, port, url, title) {
  return el('div.session', { title, onclick: () => openPort(port, url) },
    icon(iconName),
    el('div.text', el('div.row', el('span.name', name), el('span.caption', caption))),
    el('button', { title, onclick: e => { e.stopPropagation(); openPort(port, url); } }, icon('link'), t('Open')));
}

function tmuxRow(s) {
  const facts = [s.windows.length === 1 ? t('1 window') : t('{n} windows', { n: s.windows.length }), s.attached ? t('{n} attached', { n: s.attached }) : null,
    s.activity ? t('active {when}', { when: fmtFileDate(s.activity) }) : null].filter(Boolean).join(' · ');
  const windows = el('div.windows', s.windows.map(w => el('span.win', { class: w.active ? 'active' : '', title: w.panes > 1 ? t('{n} panes', { n: w.panes }) : '' }, `${w.index}:${w.name}${w.command && w.command !== w.name ? ' (' + w.command + ')' : ''}`)));
  return row('tmux', s.name, `tmux attach-session -t ${shellWord('=' + s.name)}`, t('Open this session in the window'), facts, windows);
}

function screenRow(s) {
  return row('screen', s.id, `screen -x ${shellWord(s.id)}`, t('Open this session in the window'), [t('pid {pid}', { pid: s.pid }), s.attached ? t('attached') : t('detached')].join(' · '));
}

function containerRow(c) {
  const tool = snap.docker.tool;
  const shell = shellWord('command -v bash >/dev/null 2>&1 && exec bash || exec sh');
  return row(tool, c.name, `${tool} exec -it ${shellWord(c.name)} sh -c ${shell}`, t('Open a shell in this container ({tool} exec)', { tool }), [c.image, c.status, c.ports].filter(Boolean).join(' · '));
}

function jobRow(j) {
  const facts = [j.state, j.time !== '0:00' ? j.time : null, j.nodes ? (j.nodes === 1 ? t('1 node') : t('{n} nodes', { n: j.nodes })) : null, j.reason, j.partition].filter(Boolean).join(' · ');
  if (j.state !== 'RUNNING') {
    return el('div.session.inert', { title: t('Only a running job can be entered') }, icon('clock'), el('div.text', el('div.row', el('span.name', `${j.id} ${j.name}`), el('span.caption', facts))));
  }
  return row('slurm', j.id, `srun --jobid=${shellWord(j.id)} --overlap --pty bash`, t('Open a shell on the nodes of this job (srun --jobid --overlap --pty)'), facts, el('div.caption', j.name));
}

function pbsRow(j) {
  const facts = [j.state, j.time, j.nodes ? (j.nodes === 1 ? t('1 node') : t('{n} nodes', { n: j.nodes })) : null, j.node, j.queue].filter(Boolean).join(' · ');
  if (j.state !== 'R' || !j.node) {
    return el('div.session.inert', { title: t('Only a running job can be entered') }, icon('clock'), el('div.text', el('div.row', el('span.name', `${j.id} ${j.name}`), el('span.caption', facts))));
  }
  return row('pbs', j.id, `ssh -t ${shellWord(j.node)}`, t('Open a shell on the first node of this job (ssh from the login node)'), facts, el('div.caption', j.name));
}

function vncRow(v) {
  const name = v.display === null ? v.program : `:${v.display}`;
  const facts = [v.display === null ? null : v.program, v.user, t('port {port}', { port: v.port })].filter(Boolean).join(' · ');
  return portRow('laptop', name, facts, v.port, 'vnc://127.0.0.1:{port}', t('Open {name} in your VNC viewer, through a port forward on the master', { name }));
}

/// The kinds of session the host has something for: one tab each.
function tabs() {
  const out = [];
  if (snap.tmux !== null) out.push({ key: 'tmux', title: 'tmux', items: snap.tmux, add: 'tmux', rows: tmuxRow, empty: t('No tmux session is running. A new one opens here and survives this window.') });
  if (snap.screen !== null) out.push({ key: 'screen', title: 'screen', items: snap.screen, add: 'screen', rows: screenRow, empty: t('No screen session is running. A new one opens here and survives this window.') });
  if (snap.docker !== null) out.push({ key: 'docker', title: snap.docker.tool === 'podman' ? 'Podman' : 'Docker', items: snap.docker.list, rows: containerRow, empty: t('No container is running, or {tool} is not usable by this account.', { tool: snap.docker.tool }) });
  if (snap.slurm !== null) out.push({ key: 'slurm', title: 'Slurm', items: snap.slurm, rows: jobRow, empty: t('No job of yours is queued or running.') });
  if (snap.pbs !== null) out.push({ key: 'pbs', title: 'PBS', items: snap.pbs, rows: pbsRow, empty: t('No job of yours is queued or running.') });
  if (snap.vnc.length) out.push({ key: 'vnc', title: 'VNC', items: snap.vnc, rows: vncRow });
  return out;
}

function renderSessions() {
  clear(sessionsBox);
  const bar = el('div.tabs');
  sessionsBox.append(bar);
  if (!snap) { bar.append(el('span.caption', t('Sessions'))); return; }
  const all = tabs();
  if (!all.length) {
    bar.append(el('span.caption', t('Sessions')));
    sessionsBox.append(el('div.note', t('Nothing to open on {host}: it has no tmux or screen, no container tool, no Slurm or PBS client, and no VNC server is running.', { host })));
    return;
  }
  const current = all.find(x => x.key === tab) || all.find(x => x.items.length) || all[0];
  for (const x of all) {
    bar.append(el('button.tab', { class: x === current ? 'active' : '', onclick: () => { tab = x.key; try { localStorage.setItem(tabKey, tab); } catch {} renderSessions(); } },
      x.title, x.items.length ? el('span.count', String(x.items.length)) : null));
  }
  bar.append(el('div.spacer'));
  if (current.add) bar.append(el('button', { title: t('Start a new {kind} session on {host} and open it here', { kind: current.add, host }), onclick: () => newSession(current.add) }, icon('plus'), t('New session')));
  const list = el('div.list');
  sessionsBox.append(list);
  if (!current.items.length) list.append(el('div.note', current.empty || ''));
  else append(list, current.items.map(current.rows));
}

async function newSession(kind) {
  const r = await dialog({ title: t('New {kind} session', { kind }), message: t('A name for it; {kind} numbers it when you leave this empty.', { kind }), input: true, action: t('Open') });
  if (!r.ok) return;
  const name = r.value;
  const command = kind === 'tmux' ? (name ? `tmux new-session -s ${shellWord(name)}` : 'tmux new-session')
    : (name ? `screen -S ${shellWord(name)}` : 'screen');
  await open(command, { kind, name: name || t('new session') });
  setTimeout(poll, 1500);
}

async function openPort(port, url) {
  try { await invoke('console_open_port', { host, port, url }); } catch (e) { toast(String(e), true); }
}

// MARK: the terminal

function theme() {
  return dark.matches
    ? { background: '#1e1e1e', foreground: '#f2f2f2', cursor: '#f2f2f2', selectionBackground: 'rgba(64,156,255,0.35)' }
    : { background: '#ffffff', foreground: '#1d1d1f', cursor: '#1d1d1f', selectionBackground: 'rgba(10,132,255,0.25)' };
}

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

let attachedTo = null;  // the session in the terminal, {kind, name}, while one is
let current = 0;        // the id of the terminal that is ours; output of an earlier one is ignored
let ended = true;

/// The bytes of a base64 string, as ssh wrote them.
function decode(b64) {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

function renderTerm() {
  clear(termHead);
  if (attachedTo === null) {
    termHead.append(el('span', t('Nothing open')));
    termBox.classList.add('idle');
    term.element?.classList.add('hidden');
    let empty = termBox.querySelector('.empty');
    if (!empty) termBox.append(empty = el('div.empty'));
    clear(empty).append(icon('terminal'), el('div', t('Pick a session on the right to open it here.')));
    return;
  }
  termBox.classList.remove('idle');
  termBox.querySelector('.empty')?.remove();
  term.element?.classList.remove('hidden');
  const label = `${attachedTo.kind}: ${attachedTo.name}`;
  termHead.append(icon('terminal'), el('span', ended ? t('{name} — closed', { name: label }) : label),
    el('div.spacer'),
    el('button', { title: ended ? t('Close') : t('Detach: leave the session running and close the terminal'), onclick: detach }, icon('close')));
}

/// Run `command` on the host in the terminal, hanging up on whatever was
/// there (a tmux or screen client detaches, the session lives on).
async function open(command, session) {
  if (!ended) await invoke('term_close').catch(() => {});
  attachedTo = session;
  ended = false;
  term.reset();
  renderTerm();
  renderSessions();
  fit.fit();
  term.write(`\x1b[2m${t('Connecting to {host}…', { host })}\x1b[0m\r\n`);
  try {
    current = await invoke('term_open', { host, command, cols: term.cols, rows: term.rows });
    term.focus();
  } catch (e) {
    ended = true;
    term.write(`\r\n\x1b[31m${String(e)}\x1b[0m\r\n`);
    renderTerm();
  }
}

async function detach() {
  if (!ended) await invoke('term_close').catch(() => {});
  ended = true;
  attachedTo = null;
  renderTerm();
  renderSessions();
  setTimeout(poll, 500);
}

function render() {
  renderStats();
  renderSessions();
  renderTerm();
}

async function init() {
  const s = await connectState();
  await loadLanguage(s.settings.language);
  document.title = t('Console — {host}', { host });
  document.getElementById('title').textContent = document.title;
  term.open(termBox);
  dark.addEventListener('change', () => { term.options.theme = theme(); });
  await listen('term-data', ({ id, data }) => { if (id === current) term.write(decode(data)); });
  await listen('term-exit', ({ id, code }) => {
    if (id !== current) return;
    ended = true;
    term.write(`\r\n\x1b[2m[${t('{name} is closed (exit status {code}).', { name: `${attachedTo.kind}: ${attachedTo.name}`, code })}]\x1b[0m\r\n`);
    renderTerm();
    setTimeout(poll, 500);
  });
  term.onData(data => { if (!ended) invoke('term_write', { data }).catch(() => {}); });
  // Binary input (a paste that is not text): bytes as a latin1 string.
  term.onBinary(data => { if (!ended) invoke('term_write', { data }).catch(() => {}); });
  term.onResize(({ cols, rows }) => { if (!ended) invoke('term_resize', { cols, rows }).catch(() => {}); });
  new ResizeObserver(() => { if (!termBox.classList.contains('idle')) fit.fit(); }).observe(termBox);
  // ⌘C copies a selection (otherwise it is the interrupt it always was),
  // ⌘V pastes, ⌘K clears, ⌘A selects all; any other ⌘ key is the
  // window's (⌘W, ⌘R, ⌘,, ⌘Q) and goes past the terminal to the page.
  term.attachCustomKeyEventHandler(e => {
    if (e.type !== 'keydown' || !e.metaKey) return true;
    if (e.key === 'c' && term.hasSelection()) { navigator.clipboard.writeText(term.getSelection()); return false; }
    if (e.key === 'v') { navigator.clipboard.readText().then(text => { if (text) term.paste(text); }); return false; }
    if (e.key === 'k') { term.clear(); return false; }
    if (e.key === 'a') { term.selectAll(); return false; }
    return false;
  });
  commonKeys();
  document.addEventListener('keydown', e => {
    if ((e.metaKey || e.ctrlKey) && e.key === 'r') { e.preventDefault(); if (error) { error = null; start(); } else poll(); }
  });
  document.addEventListener('visibilitychange', () => { if (document.hidden) stop(); else if (!error) start(); });
  render();
  start();
}

init();
