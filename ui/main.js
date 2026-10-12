import { invoke, listen, loadLanguage, t, el, append, clear, icon, fmtDuration, abbreviateHome, connectState, watchState, currentState,
         stateLabel, statusSummary, hostColor, isUp, isRunning, failed, dialog, toast, commonKeys } from './shellder.js';

let selection = null;
let showLog = false;
let logFilter = '';
let follow = true;
let revealed = {};          // "host|kind" -> value
let totpShown = {};         // host -> true
let logTimer = null;
let logLines = [];
let tick = null;
let shownAlias = null;      // the host the detail pane was last built for
let sidebarHidden = (() => { try { return localStorage.getItem('sidebar-hidden') === '1'; } catch { return false; } })();

const hostsEl = document.getElementById('hosts');
const detailEl = document.getElementById('detail');
const logArea = document.getElementById('log-area');
const bar = document.getElementById('sidebar-bar');
const sidebarEl = hostsEl.parentElement;
const detailPane = detailEl.parentElement;

// MARK: sidebar

function renderSidebar(s) {
  clear(hostsEl);
  if (!s.hosts.some(h => h.alias === selection)) selection = s.hosts[0] ? s.hosts[0].alias : null;
  for (const h of s.hosts) {
    const st = s.statuses[h.alias];
    const r = s.resolved[h.alias];
    const sub = r ? (r.user ? `${r.user}@${r.hostname}` : r.hostname)
      : s.resolveErrors[h.alias] ? t('config error: {e}', { e: s.resolveErrors[h.alias] }) : t('resolving…');
    const row = el('div.host-row', { class: h.alias === selection ? 'selected' : '', 'data-alias': h.alias, onclick: () => { selection = h.alias; render(s); } },
      el('span.dot', { class: hostColor(st, s.locked.includes(h.alias)), title: failed(st) ? t('Failed') : stateLabel(st && st.state, st && st.socket) }),
      el('div.text', el('div.name', h.alias), el('div.sub', { title: sub }, sub)),
      consoleChip(h.alias),
      connectSwitch(s, h.alias));
    hostsEl.append(row);
  }
}

// The way into a host: its Console window. One per row, so every host shows
// it can be opened; dim on the rows that are not selected (app.css), it is
// a column of hints rather than a toolbar. It does not select the host: the
// Console is its own window, and the detail pane stays on what it showed.
function consoleChip(alias) {
  return el('button.chip.console', { title: t('Console: a shell on {alias}, its sessions and what it is doing, in a window of its own (⌘T); the Files window and your terminal application are reached from there.', { alias }),
    'aria-label': t('Console'), onclick: e => { e.stopPropagation(); invoke('open_window', { which: 'console', host: alias }); } }, icon('dashboard'));
}

function connectSwitch(s, alias) {
  const on = s.enabled.includes(alias);
  // The switch wears the host's colour while it is on (grey, as in "nothing
  // yet", counts as an attempt under way); off it is plain grey, whatever
  // the last attempt did.
  const color = hostColor(s.statuses[alias], s.locked.includes(alias));
  // Flipping the switch also selects the host, so the detail pane shows what
  // the attempt does. The sidebar is not re-rendered here: that would draw
  // the knob from the old state until the new one arrives.
  const input = el('input', { type: 'checkbox', checked: on, onchange: e => {
    invoke('set_enabled', { host: alias, on: e.target.checked });
    if (selection !== alias) {
      selection = alias;
      for (const row of hostsEl.querySelectorAll('.host-row')) row.classList.toggle('selected', row.dataset.alias === alias);
      renderDetail(s);
    }
  } });
  return el('label.switch', { title: t('Connect: one attempt. If it succeeds the master stays up until it drops or you switch it off. If it fails the switch goes back off. Jump hosts from this list are switched on with it, and switching a jump host off takes the hosts behind it down.'), onclick: e => e.stopPropagation() },
    input, el('span.knob', { class: on ? (color === 'off' ? 'yellow' : color) : '' }));
}

function renderBar(s) {
  clear(bar);
  append(bar, [
    el('button', { title: t('Reload ~/.ssh/config (⌘R)'), disabled: s.reloading, onclick: () => invoke('reload_config') }, icon('refresh')),
    el('button', { title: t('Edit ~/.ssh/config in your editor (⌘E)'), onclick: () => invoke('open_path', { path: s.paths.sshConfig, editor: true }) }, icon('doc')),
    el('button', { title: t('Settings (⌘,)'), onclick: () => invoke('open_window', { which: 'settings' }) }, icon('gear')),
    s.update.state === 'available' ? el('button', { title: t('Version {v} is available', { v: s.update.version }), style: 'color: var(--accent)', onclick: () => invoke('open_window', { which: 'settings' }) }, icon('download')) : null,
    el('div.spacer'),
    el('button', { title: t('Toggle the log panel (⌘L)'), class: showLog ? 'active' : '', onclick: toggleLog }, icon('log'))]);
}

// The host list folds away (⌘⇧S) and the detail pane takes the window;
// the choice is kept on this machine. The button lives in the detail pane,
// so it is there to bring the list back.
function toggleSidebar() {
  sidebarHidden = !sidebarHidden;
  try { localStorage.setItem('sidebar-hidden', sidebarHidden ? '1' : '0'); } catch {}
  renderSidebarToggle();
}

function renderSidebarToggle() {
  sidebarEl.classList.toggle('hidden', sidebarHidden);
  let b = detailPane.querySelector('.sidebar-toggle');
  if (!b) detailPane.append(b = el('button.sidebar-toggle', { onclick: toggleSidebar }));
  b.title = sidebarHidden ? t('Show the host list (⌘⇧S)') : t('Hide the host list (⌘⇧S)');
  b.setAttribute('aria-label', b.title);
  clear(b).append(icon('panel'));
}

// MARK: detail

function chip(name, iconName, help, onclick, { active = false, disabled = false } = {}) {
  return el('button.chip', { class: active ? 'active' : '', title: help || name, 'aria-label': name, disabled, onclick }, icon(iconName));
}

function masterControls(s, alias) {
  const st = s.statuses[alias];
  const state = st ? st.state : { kind: 'off' };
  const locked = s.locked.includes(alias);
  const enabled = s.enabled.includes(alias);
  const act = a => () => invoke('host_action', { host: alias, action: a });
  const out = [chip(locked ? t('Unlock') : t('Lock'), locked ? 'lock' : 'unlock',
    locked ? t('Locked: shellder reconnects this host after a drop and switches it on again when it starts. Switching it off, or a failed attempt, unlocks it.')
           : t('Lock: switch the host on if it is off, then keep it connected. A drop is reconnected with back-off and the host comes back at the next launch. The lock goes with the switch: off, or a failed attempt, unlocks it.'),
    () => invoke('set_locked', { host: alias, on: !locked }), { active: locked })];
  if (isRunning(state)) {
    const behind = st ? st.neededBy : [];
    out.push(chip(t('Disconnect'), 'stop', behind.length ? t('Disconnect: same as switching off. Closes the master, unlocks it, and switches off the hosts that jump through it ({hosts})', { hosts: behind.join(', ') }) : t('Disconnect: same as switching off. Closes the master and unlocks it.'), act('disconnect')));
    out.push(chip(t('Reconnect'), 'refresh', t('Reconnect: close the master and connect again. The switch and the lock stay as they are.'), act('reconnect')));
  } else if (state.kind === 'foreign') {
    out.push(chip(t('Take over'), 'hand', t('Take over: close the external master and start one owned by shellder. Switches the host on.'), act('takeover')));
  } else if (enabled) {
    out.push(chip(t('Retry now'), 'refresh', t('Retry now: try again immediately instead of waiting for the back-off'), act('retry')));
  } else {
    out.push(chip(t('Connect'), 'play', t('Connect: same as the switch, one attempt, kept up if it succeeds'), act('connect')));
  }
  if (isUp(state)) {
    const socket = s.resolved[alias] && s.resolved[alias].controlPath ? abbreviateHome(s.resolved[alias].controlPath, s.paths.home) : t('the socket');
    out.push(chip(t('Close socket'), 'xcircle', t('Close socket: ssh -O exit, shuts the master down and removes {socket}. The switch goes off and the lock with it, and hosts jumping through this one go down as well.', { socket }), act('close')));
  }
  return el('div.controls', out);
}

function card(title, rows, headerExtra) {
  const c = el('div.card', rows.filter(Boolean).map(r => el('div.crow', r)));
  if (!title) return c;
  return el('div', el('div.card-title', el('span', title), el('div.spacer'), headerExtra || null), c);
}

function kv(k, v, mono) {
  return el('div.kv', el('div.k', k), el('div.v.selectable', { class: mono ? 'mono' : '' }, v));
}

function renderDetail(s) {
  clear(detailEl);
  const entry = s.hosts.find(h => h.alias === selection);
  // The pane is rebuilt in place, so its scroll offset would carry over to
  // the next host. Back to the top when the host changes; a re-render of
  // the same host (status ticks, a revealed secret) keeps the reader's place.
  const alias0 = entry ? entry.alias : null;
  if (alias0 !== shownAlias) { shownAlias = alias0; detailEl.scrollTop = 0; }
  if (!entry) {
    detailEl.append(el('div.empty', el('img', { src: 'emblem.png' }), el('div', s.hosts.length ? t('Select a host') : t('No Host entries in ~/.ssh/config'))));
    return;
  }
  const alias = entry.alias;
  const st = s.statuses[alias];
  const r = s.resolved[alias];
  const err = s.resolveErrors[alias];
  const inner = el('div.inner');

  // header card: what the connection is, how it is doing, and the master's controls
  const statusLine = el('div.row.secondary', { style: 'font-size:12px' }, el('span.dot', { class: hostColor(st, s.locked.includes(alias)) }), el('span.status-text'));
  updateStatusLine(statusLine, s, alias);
  const header = el('div.header',
    el('div.text', el('h2', alias), statusLine,
      r ? el('div.secondary.selectable', { style: 'font-size:12px' }, targetLine(r)) : null,
      entry.aliases.length ? el('div.caption', t('also: {names}', { names: entry.aliases.join(', ') })) : null),
    masterControls(s, alias));
  const keepAlive = el('div.kv', el('div.k', t('Keep-alive')), el('div.spacer'),
    el('select', { title: t('Keep-alive mode: -N is cleanest. If the server closes a session-less connection right after login, pick an idle login shell or cat here. The choice is remembered per host.'),
      onchange: e => invoke('set_idle_mode', { host: alias, mode: e.target.value }) },
      s.idleModes.map(m => el('option', { value: m, selected: (st ? st.idleMode : 'none') === m }, idleTitle(m)))));
  const rows = [header, keepAlive];
  if (st && !failed(st) && st.lastError) rows.push(kv(t('Last error'), st.lastError));
  inner.append(card(null, rows));

  // A master without a socket keeps the connection (and its port forwards)
  // alive, but other ssh commands, the shells and the Files windows cannot
  // go through it. A ControlPath gives it one, where the platform has
  // ControlMaster at all.
  if (!s.platform.controlMaster) {
    inner.append(card(null, [el('div.secondary', icon('info'), ' ', t('This platform\'s ssh has no ControlMaster: the connection is kept alive for its port forwards, and every shell and Files window logs in on its own, with shellder answering the prompts.'))]));
  } else if (r && !r.controlPath) {
    const snippet = `Host ${alias}\n    ControlMaster auto\n    ControlPath ~/.ssh/cm-%C\n    ControlPersist 5m`;
    inner.append(card(null, [el('div', { style: 'display:flex;flex-direction:column;gap:8px' },
      el('div.secondary', { style: 'font-weight:600' }, icon('info'), ' ', t('No ControlPath for this host')),
      el('div.secondary', t('The connection is kept alive for its port forwards, but without a socket no other ssh command, shell or Files window can reuse it. shellder never edits ~/.ssh/config, so add something like this yourself (a `Host *` block covers every host):')),
      el('div.row', { style: 'align-items:flex-start' }, el('div.snippet.selectable', snippet),
        el('button', { onclick: () => navigator.clipboard.writeText(snippet) }, t('Copy')),
        el('button', { onclick: () => invoke('open_path', { path: s.paths.sshConfig, editor: true }) }, t('Edit config…'))))]));
  }
  if (err) inner.append(card(t('ssh -G {alias} failed', { alias }), [el('div.red', icon('warning'), ' ', err)]));

  inner.append(credentials(s, alias));
  inner.append(configuration(s, entry, r, err));
  detailEl.append(inner);
}

function idleTitle(m) {
  return { none: t('no session (-N)'), shell: t('idle shell (pty)'), cat: t('cat on stdin') }[m] || m;
}

function targetLine(r) {
  let s = r.user ? `${r.user}@${r.hostname}` : r.hostname;
  if (r.port !== '22') s += `:${r.port}`;
  if (r.proxyJump) s = t('{target}  via {jump}', { target: s, jump: r.proxyJump });
  return s;
}

function updateStatusLine(line, s, alias) {
  const st = s.statuses[alias];
  const text = line.querySelector('.status-text');
  if (!text) return;
  if (st && st.since) text.textContent = `${statusSummary(st)} · ${fmtDuration(Date.now() / 1000 - st.since)}`;
  else if (failed(st)) text.textContent = t('{summary} (details in log)', { summary: statusSummary(st) });
  else text.textContent = statusSummary(st);
}

const KINDS = ['password', 'passphrase', 'totp'];
const kindTitle = k => ({ password: t('Password'), passphrase: t('Key passphrase'), totp: t('TOTP secret') })[k];
const kindNoun = k => ({ password: t('password'), passphrase: t('key passphrase'), totp: t('TOTP secret') })[k];

function credentials(s, alias) {
  const stored = (s.secrets[alias] || []);
  const missing = KINDS.filter(k => !stored.includes(k));
  const rows = [];
  if (s.keychainRefusal) {
    rows.push(el('div.row', { style: 'align-items:flex-start' }, icon('warning', 'orange'), el('div', { style: 'display:flex;flex-direction:column;gap:3px' },
      el('div', t('The keychain will not open the shellder vault: {why}', { why: s.keychainRefusal })),
      el('div.caption', t('Whatever is stored is still in it. Allow shellder when the keychain asks, then try again.')),
      el('button.link', { onclick: () => invoke('request_keychain_access') }, t('Try Again')))));
  } else if (!stored.length) {
    rows.push(el('div.secondary', t('No credentials stored. Press + to add a password, key passphrase or TOTP secret.')));
  }
  for (const k of KINDS.filter(k => stored.includes(k))) rows.push(credentialRow(s, alias, k));
  const header = el('div.row',
    el('button.chip', { title: missing.length ? t('Add a credential') : t('Every kind of credential is stored'), 'aria-label': t('Add credential'), disabled: !missing.length,
      onclick: e => menu(e.currentTarget, missing.map(k => ({ title: kindTitle(k), run: () => editSecret(alias, k) }))) }, icon('plus')),
    el('button.chip', { title: t('Remove every stored credential of this host from the keychain'), 'aria-label': t('Remove all credentials'), disabled: !stored.length,
      onclick: async () => { const r = await dialog({ title: t('Remove every stored credential for {alias}?', { alias }), action: t('Remove'), destructive: true }); if (r.ok) invoke('remove_all_secrets', { host: alias }); } }, icon('trash')));
  return card(t('Credentials'), rows, header);
}

function credentialRow(s, alias, kind) {
  const key = `${alias}|${kind}`;
  const shown = revealed[key];
  const row = el('div', { style: 'display:flex;flex-direction:column;gap:6px' });
  const line = el('div.row', el('span', kindTitle(kind)), el('div.spacer'));
  if (kind === 'totp') line.append(totpCode(alias));
  line.append(icon('circlecheck', 'green-icon'));
  line.lastChild.style.color = 'var(--green)';
  line.append(
    chip(shown === undefined ? t('Show') : t('Hide'), shown === undefined ? 'eye' : 'eyeoff', shown === undefined ? t('Show the stored {kind}', { kind: kindNoun(kind) }) : t('Hide'),
      async () => { if (shown !== undefined) { delete revealed[key]; } else { revealed[key] = await invoke('reveal_secret', { host: alias, kind }); } render(currentState()); }, { active: shown !== undefined }),
    chip(t('Change {kind}', { kind: kindNoun(kind) }), 'pencil', t('Change the stored {kind}', { kind: kindNoun(kind) }), () => editSecret(alias, kind)),
    chip(t('Remove {kind}', { kind: kindNoun(kind) }), 'trash', t('Remove the stored {kind} from the keychain', { kind: kindNoun(kind) }), () => invoke('remove_secret', { host: alias, kind })));
  row.append(line);
  if (shown !== undefined && shown !== null) {
    row.append(el('div.row', el('span.secret.selectable', shown), el('button', { onclick: () => navigator.clipboard.writeText(shown) }, t('Copy')),
      kind === 'totp' ? null : el('span.caption', t('{n} characters', { n: [...shown].length }))));
  }
  return row;
}

function totpCode(alias) {
  if (!totpShown[alias]) return el('button', { title: t('Compare with your authenticator app'), onclick: () => { totpShown[alias] = true; render(currentState()); } }, t('Show code'));
  const box = el('span.row', { style: 'gap:6px' }, el('span.mono.selectable', { style: 'font-weight:600' }, '······'), el('span.caption', { style: 'width:28px' }, ''));
  const update = async () => {
    const c = await invoke('totp_code', { host: alias });
    if (!box.isConnected) return;
    if (c) { box.firstChild.textContent = c.code; box.lastChild.textContent = `${c.remaining}s`; }
  };
  update();
  box.dataset.totp = alias;
  return box;
}

async function editSecret(alias, kind) {
  const r = await dialog({ title: `${kindTitle(kind)} — ${alias}`, input: true, secret: true, preset: '',
    message: kind === 'totp' ? t('base32 secret or otpauth://totp/… URI') : null, action: t('Save') });
  if (!r.ok || !r.value) return;
  try { await invoke('set_secret', { host: alias, kind, value: r.value }); delete revealed[`${alias}|${kind}`]; }
  catch (e) { toast(String(e), true); }
}

function configuration(s, entry, r, err) {
  const home = s.paths.home;
  const rows = [];
  if (r) {
    rows.push(kv('HostName', r.hostname), kv('User', r.user), kv('Port', r.port));
    if (r.proxyJump) rows.push(kv('ProxyJump', r.proxyJump + (r.jumpAlias ? t('  (a host from this list: its master is brought up first and kept alive with this one)') : '')));
    if (r.proxyCommand) rows.push(kv('ProxyCommand', r.proxyCommand, true));
    rows.push(kv('IdentityFile', r.identityFiles.map(f => abbreviateHome(f, home)).join('\n'), true));
    rows.push(kv('ControlMaster', r.controlMaster));
    rows.push(kv('ControlPath', r.controlPath ? abbreviateHome(r.controlPath, home) : t('none'), true));
    rows.push(kv('ControlPersist', r.controlPersist));
    rows.push(kv('ServerAliveInterval', r.serverAliveInterval === 0 ? t('0 (shellder adds 15s for its master)') : String(r.serverAliveInterval)));
  } else if (!err) rows.push(el('div.secondary', t('resolving…')));
  rows.push(kv(t('Defined in'), abbreviateHome(entry.source, home), true));
  return card(t('Effective configuration'), rows, el('button.link', { style: 'font-size:11px', onclick: () => invoke('open_path', { path: s.paths.sshConfig, editor: true }) }, t('Edit ~/.ssh/config…')));
}

// a small pop-up menu under a button
function menu(anchor, items) {
  const m = el('div.menu');
  for (const it of items) m.append(el('div.item', { onclick: () => { close(); it.run(); } }, it.title));
  const rect = anchor.getBoundingClientRect();
  m.style.left = `${rect.left}px`; m.style.top = `${rect.bottom + 4}px`;
  const close = () => { m.remove(); document.removeEventListener('mousedown', outside); };
  const outside = e => { if (!m.contains(e.target)) close(); };
  document.body.append(m);
  // Keep it inside the window: a button near the right edge (the
  // credentials' +) would otherwise push the menu's right half off screen,
  // where nothing draws it.
  const over = rect.left + m.offsetWidth - (window.innerWidth - 8);
  if (over > 0) m.style.left = `${Math.max(8, rect.left - over)}px`;
  const below = rect.bottom + 4 + m.offsetHeight - (window.innerHeight - 8);
  if (below > 0) m.style.top = `${Math.max(8, rect.top - 4 - m.offsetHeight)}px`;
  setTimeout(() => document.addEventListener('mousedown', outside), 0);
}

// MARK: log panel

function toggleLog() { showLog = !showLog; renderLog(); renderBar(currentState()); }

function renderLog() {
  clear(logArea);
  if (!showLog) { logArea.classList.add('hidden'); if (logTimer) { clearInterval(logTimer); logTimer = null; } return; }
  logArea.classList.remove('hidden');
  const s = currentState();
  const height = (s.settings && s.logPanelHeight) || 200;
  const lines = el('div.lines');
  const panel = el('div.log', { style: `height:${height}px` },
    el('div.bar', el('span', { style: 'font-weight:600' }, t('Log')),
      el('input.text', { placeholder: t('filter (host name, ERROR, …)'), value: logFilter, oninput: e => { logFilter = e.target.value; fillLog(lines); } }),
      el('label.row', el('input', { type: 'checkbox', checked: follow, onchange: e => { follow = e.target.checked; if (follow) lines.scrollTop = lines.scrollHeight; } }), t('Follow')),
      el('div.spacer'),
      el('button', { onclick: () => invoke('open_path', { path: s.paths.log, editor: false }) }, t('Open file')),
      el('button.borderless', { onclick: toggleLog }, icon('close'))),
    lines);
  const handle = el('div.drag-handle', { title: t('Drag to resize the log panel') });
  let startY = 0, startH = 0;
  handle.onmousedown = e => {
    startY = e.clientY; startH = panel.offsetHeight;
    const move = ev => { panel.style.height = `${Math.min(700, Math.max(80, startH - (ev.clientY - startY)))}px`; };
    const up = () => { document.removeEventListener('mousemove', move); document.removeEventListener('mouseup', up); invoke('set_setting', { key: 'logPanelHeight', value: panel.offsetHeight }); };
    document.addEventListener('mousemove', move); document.addEventListener('mouseup', up);
  };
  logArea.append(handle, panel);
  const refresh = async () => { logLines = await invoke('log_tail'); fillLog(lines); };
  refresh();
  logTimer = setInterval(refresh, 1000);
}

function fillLog(lines) {
  const f = logFilter.trim().toLowerCase();
  const shown = f ? logLines.filter(l => l.toLowerCase().includes(f)) : logLines;
  if (lines.dataset.sig === shown.length + '|' + (shown[shown.length - 1] || '')) return;
  lines.dataset.sig = shown.length + '|' + (shown[shown.length - 1] || '');
  clear(lines);
  for (const l of shown) {
    const level = (l.match(/ (ERROR|WARNING|PROMPT|SSH|INFO) /) || [])[1] || '';
    lines.append(el('div', { class: level }, l));
  }
  if (follow) lines.scrollTop = lines.scrollHeight;
}

// MARK: render

function render(s) {
  renderSidebar(s);
  renderSidebarToggle();
  renderBar(s);
  renderDetail(s);
}

async function init() {
  const s = await connectState();
  await loadLanguage(s.settings.language);
  watchState(render);
  render(s);
  if (s.logPanelHeight) {}
  listen('select-host', alias => { selection = alias; render(currentState()); });
  // The status line's uptime and the TOTP code tick by themselves.
  tick = setInterval(() => {
    const st = currentState();
    const line = detailEl.querySelector('.status-text');
    if (st && line && selection) updateStatusLine(line.parentElement, st, selection);
    for (const box of detailEl.querySelectorAll('[data-totp]')) {
      invoke('totp_code', { host: box.dataset.totp }).then(c => { if (c && box.isConnected) { box.firstChild.textContent = c.code; box.lastChild.textContent = `${c.remaining}s`; } });
    }
  }, 1000);
  commonKeys();
  document.addEventListener('keydown', e => {
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.key === 'l') { e.preventDefault(); toggleLog(); }
    if (mod && e.shiftKey && e.key.toLowerCase() === 's') { e.preventDefault(); toggleSidebar(); }
    if (mod && e.key === 'r') { e.preventDefault(); invoke('reload_config'); }
    if (mod && e.key === 'e') { e.preventDefault(); invoke('open_path', { path: currentState().paths.sshConfig, editor: true }); }
    if (mod && e.key === 'k' && selection) { e.preventDefault(); invoke('host_action', { host: selection, action: 'reconnect' }); }
    if (mod && e.key === 't' && selection) { e.preventDefault(); invoke('open_window', { which: 'console', host: selection }); }
  });
  // Revealed secrets do not outlive the page: hide them when the window goes away.
  document.addEventListener('visibilitychange', () => { if (document.hidden) { revealed = {}; totpShown = {}; render(currentState()); } });
}

init();
