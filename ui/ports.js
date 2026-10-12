// The ports of a host: the forwards the user set up (kept on this
// machine, per host; a port left null is picked when the forward is
// made, `got` is what it came to) and the host's own ports worth a row
// (null: those with a VNC server). The Console's Ports tab and the main
// window's Tools card both draw them from here; the Console passes what
// its readings say listens on the host, the main window has no reading
// and shows what was set up.
import { invoke, t, el, icon, toast, menu, currentState } from './shellder.js';

export function createPorts(host, { listeners = () => [], vncServers = () => [], onChange = () => {} } = {}) {
  const fwdKey = `console-forwards-${host}`;
  const showKey = `console-show-ports-${host}`;
  function savedForwards() { try { return JSON.parse(localStorage.getItem(fwdKey) || '[]'); } catch { return []; } }
  function saveForwards(list) { try { localStorage.setItem(fwdKey, JSON.stringify(list)); } catch {} }
  function shownPorts() { try { const v = localStorage.getItem(showKey); return v === null ? null : JSON.parse(v); } catch { return null; } }
  function saveShownPorts(list) { try { localStorage.setItem(showKey, JSON.stringify(list)); } catch {} }
  // The direct connections: ports of the host opened at its own address,
  // no tunnel, made in the main window's Tools card or the Console's
  // Ports tab and listed in both. A list of their own, apart from the
  // host's ports the Console lists, whose default comes from its readings.
  const directKey = `console-direct-${host}`;
  function directPorts() { try { return JSON.parse(localStorage.getItem(directKey) || '[]'); } catch { return []; } }
  function saveDirectPorts(list) { try { localStorage.setItem(directKey, JSON.stringify(list)); } catch {} }

  let activeForwards = [];  // what the master has up, as last asked

  // The lists are shared by every window (the main window's Tools card,
  // the host's Console): one changed in another window is drawn here too.
  window.addEventListener('storage', e => { if (e.key === fwdKey || e.key === showKey || e.key === directKey) onChange(); });

  /// Ask the app what is up; `onChange` when that differs from the last
  /// answer. Returns whether it did.
  async function refresh() {
    let now;
    try { now = await invoke('console_forwards', { host }); } catch { now = []; }
    const changed = JSON.stringify(now) !== JSON.stringify(activeForwards);
    activeForwards = now;
    if (changed) onChange();
    return changed;
  }

  /// The live forward a saved one corresponds to, if it is up.
  function liveOf(def) {
    const l = def.local ?? (def.got && def.got.local);
    const r = def.remote ?? (def.got && def.got.remote);
    return activeForwards.find(f => f.direction === def.direction && f.local === l && f.remote === r) || null;
  }

  const liveCount = () => savedForwards().filter(d => liveOf(d)).length;

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
    if (!await refresh()) onChange();
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

  /// The host's own address: the HostName of its ssh config.
  function address() {
    const s = currentState();
    const r = s && s.resolved && s.resolved[host];
    return (r && r.hostname) || host;
  }

  /// Open the port at the host's own address, with no tunnel: for a host
  /// reachable directly.
  function openDirect(port) {
    const url = isVncPort(port) ? `vnc://${address()}:${port}` : `http://${address()}:${port}/`;
    invoke('open_url', { url }).catch(e => toast(String(e), true));
  }

  async function stopForward(def) {
    const f = liveOf(def);
    if (f) { try { await invoke('console_cancel_forward', { host, forward: f }); } catch (e) { toast(String(e), true); } }
    if (!await refresh()) onChange();
  }

  function removeForward(def) {
    stopForward(def);
    saveForwards(savedForwards().filter(x => !(x.direction === def.direction && x.local === def.local && x.remote === def.remote)));
    onChange();
  }

  /// A port the VNC viewer opens: one a VNC server the host named has, or,
  /// for a port nothing names (one added by hand, say), the range VNC
  /// servers use, 5900 plus the display.
  const isVncPort = port => vncServers().some(v => v.port === port)
    || (port >= 5900 && port <= 5999 && !listeners().some(l => l.port === port && l.process && !/vnc/i.test(l.process)));

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
    const v = vncServers().find(x => x.port === l.port);
    const what = v ? [v.display == null ? v.program : `${v.program} :${v.display}`, v.user]
      : [l.process ? `${l.process}${l.pid ? ' (' + l.pid + ')' : ''}` : (l.added ? t('added by hand') : null)];
    return el('div.session', { title: t('Open it here through a forward on the master, made now if there is none'), onclick: () => forwardAndOpen(l.port) },
      icon(isVncPort(l.port) ? 'laptop' : 'network'),
      el('div.text', el('div.row', el('span.name', `:${l.port}`)), el('div.caption', [...what, l.address].filter(Boolean).join(' · '))),
      el('button.act', { title: t('Open {address}:{port} directly, with no tunnel', { address: address(), port: l.port }), onclick: e => { e.stopPropagation(); openDirect(l.port); } }, icon('globe')),
      el('button.act', { title: t('Take this port off the list'), onclick: e => { e.stopPropagation(); hidePort(l.port); } }, icon('close')));
  }

  /// The host's own listeners worth a row.
  function hostListeners() { return listeners().filter(l => l.port !== 22 && l.process !== 'sshd'); }
  /// What is listed while nothing was chosen: the VNC servers' ports.
  function defaultPorts() { return [...new Set(vncServers().map(v => v.port))]; }
  function listedPorts() { return shownPorts() ?? defaultPorts(); }
  /// A row per listed port: the listener on it when this account sees
  /// one, the VNC server there otherwise, or just the number for a port
  /// added by hand.
  function shownListeners() {
    const all = hostListeners();
    return listedPorts().map(port => {
      const l = all.find(x => x.port === port);
      if (l) return l;
      const v = vncServers().find(x => x.port === port);
      return v ? { port, address: '', process: v.program, pid: v.pid } : { port, address: '', process: '', pid: null, added: true };
    });
  }
  function addPort(port) { const list = listedPorts(); if (!list.includes(port)) saveShownPorts([...list, port]); onChange(); }
  function hidePort(port) { saveShownPorts(listedPorts().filter(p => p !== port)); onChange(); }

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
  function portDialog() { return askPort().then(n => { if (n) addPort(n); }); }

  /// Ask for a port of the host: the number, or null.
  function askPort(title = t('List a port of {host}', { host })) {
    return new Promise(resolve => {
      const field = el('input.text', { type: 'text', placeholder: t('required'), spellcheck: false, inputmode: 'numeric' });
      const done = v => { back.remove(); resolve(v); };
      const ok = el('button.primary', { onclick: () => {
        const n = Number(field.value.trim());
        if (!Number.isInteger(n) || n < 1 || n > 65535) { toast(t('A port is a number from 1 to 65535.'), true); return; }
        done(n);
      } }, t('Add'));
      const box = el('div.dialog', el('h3', title),
        el('div.row', el('span', { style: 'width:120px' }, t('Port on {host}', { host })), field),
        el('div.buttons', el('button', { onclick: () => done(null) }, t('Cancel')), ok));
      const back = el('div.backdrop', { onkeydown: e => { if (e.key === 'Escape') done(null); if (e.key === 'Enter' && e.target.tagName === 'INPUT') ok.click(); } }, box);
      document.body.append(back);
      field.focus();
    });
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
      onChange();
      startForward(def);
    });
  }

  /// A direct connection's row: a click opens the port at the host's own
  /// address, × forgets it.
  function directRow(port) {
    return el('div.session', { title: t('Open {address}:{port} directly, with no tunnel', { address: address(), port }), onclick: () => openDirect(port) },
      icon(isVncPort(port) ? 'laptop' : 'globe'),
      el('div.text', el('div.row', el('span.name', `${address()}:${port}`)), el('div.caption', t('direct, no tunnel'))),
      el('button.act', { title: t('Forget this direct connection'), onclick: e => { e.stopPropagation(); saveDirectPorts(directPorts().filter(p => p !== port)); onChange(); } }, icon('close')));
  }

  /// Ask for a port and keep it as a direct connection, opened now.
  function directDialog() {
    return askPort(t('Direct connection to {host}', { host })).then(n => {
      if (!n) return;
      if (!directPorts().includes(n)) saveDirectPorts([...directPorts(), n]);
      onChange();
      openDirect(n);
    });
  }

  /// What the main window's Tools card lists under its buttons, no
  /// labels: the forwards, then the direct connections.
  function cardRows() { return [...savedForwards().map(forwardRow), ...directPorts().map(directRow)]; }

  /// The + of the Tools card: a forward, or a direct connection.
  function newMenu(anchor) {
    menu(anchor, [
      { icon: 'arrowright', title: t('New forward…'), run: () => forwardDialog() },
      { icon: 'globe', title: t('Direct connection…'), run: () => directDialog() }]);
  }

  /// The rows: a label and the forwards, a label and the direct
  /// connections, a label and the host's ports, each label with its +.
  function rows() {
    const out = [];
    const defs = savedForwards();
    out.push(el('div.sect', t('Forwards'), el('div.spacer'), el('button.act', { title: t('A forward in either direction, the ports yours to pick or leave to shellder'), onclick: () => forwardDialog() }, icon('plus'))));
    if (!defs.length) out.push(el('div.note', t('None yet')));
    for (const d of defs) out.push(forwardRow(d));
    const direct = directPorts();
    out.push(el('div.sect', t('Direct connections'), el('div.spacer'), el('button.act', { title: t('A port of the host opened at its own address, with no tunnel'), onclick: () => directDialog() }, icon('plus'))));
    if (!direct.length) out.push(el('div.note', t('None yet')));
    for (const port of direct) out.push(directRow(port));
    out.push(el('div.sect', t('On the host'), el('div.spacer'), el('button.act', { title: t('List a port of the host: one this account sees listening, or any port by hand'), onclick: e => addMenu(e.currentTarget) }, icon('plus'))));
    const shown = shownListeners();
    if (!shown.length) out.push(el('div.note', t('None shown')));
    for (const l of shown) out.push(hostPortRow(l));
    return out;
  }

  return { refresh, rows, cardRows, newMenu, liveCount };
}
