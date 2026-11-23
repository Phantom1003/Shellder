// A stand-in for the app, for the pictures of the documentation
// (shoot.py): put in front of a page of ui/, it answers the page's calls
// with the data shoot.py prepared (`window.__DOC`, data.js), so that the
// page draws itself as in the app, in a web view that is no window of
// the app's. Once the page says it is drawn (`window_ready`) the scene's
// own steps run (`setup`, a script: clicks, tabs opened), then the places
// of the parts the picture names are read and left in `window.__SHOT`
// for shoot.swift, with whatever went wrong.
(() => {
  'use strict';
  const Q = new URLSearchParams(location.search);
  const D = window.__DOC;
  const scene = D.scenes[Q.get('scene')] || { setup: '', parts: {} };
  window.__SHELLDER_LANG = Q.get('lang') || 'en';
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const errors = [], unknown = new Set();
  window.addEventListener('error', e => errors.push(String(e.message || e.error)));
  window.addEventListener('unhandledrejection', e => errors.push('rejected: ' + String(e.reason && e.reason.stack || e.reason)));

  // The window is on no screen and never has the keyboard, and a Console
  // takes a click in a window without it for the one that brings the
  // window forward, which it swallows.
  document.hasFocus = () => true;

  const listeners = {};
  const emit = (name, payload) => { for (const f of listeners[name] || []) f({ payload }); };
  const bytes = text => { const u = new TextEncoder().encode(text); let s = ''; for (const b of u) s += String.fromCharCode(b); return btoa(s); };
  const state = D.state;
  if (scene.state) Object.assign(state, scene.state);
  const files = D.files;
  let terms = 0;

  const answers = {
    get_state: () => state,
    window_ready: () => { setTimeout(run, 400); },
    set_setting: ({ key, value }) => {
      const at = key.split('.'); let o = state.settings;
      while (at.length > 1) o = o[at.shift()];
      o[at[0]] = value;
      emit('state', state);
    },
    log_tail: ({ after }) => ({ offset: 1, reset: after == null, lines: after == null ? D.log : [] }),
    console_snapshot: () => D.reading,
    console_tree: () => D.tree,
    console_forwards: () => D.forwards,
    terminal_themes: () => [],
    fonts: () => ['Menlo', 'Monaco', 'SF Mono', 'Courier New'],
    sounds: () => [],
    files_open: () => files,
    files_state: () => files,
    files_home: () => '/home/alice',
    files_exists: () => true,
    files_subdirectories: () => [],
    files_stamp: () => ({ inode: 1, size: D.preview.length, modified: 1 }),
    files_read: () => Array.from(new TextEncoder().encode(D.preview)),
    term_open: ({ command }) => {
      const id = ++terms;
      const list = scene.terminals || [];
      const said = list.find(x => x.match && command && command.includes(x.match)) || list.find(x => !x.match) || { text: D.shell };
      setTimeout(() => emit('term-data', { id, data: bytes(said.text) }), 60);
      return id;
    },
    'plugin:clipboard-manager|read_text': () => '',
  };
  const invoke = async (cmd, args) => {
    if (cmd in answers) return answers[cmd](args || {});
    unknown.add(cmd);
    return null;
  };
  const win = {
    label: Q.get('label') || 'main',
    setTitle: async () => {}, close: async () => {}, destroy: async () => {}, setFocus: async () => {},
    onCloseRequested: async () => () => {}, onFocusChanged: async () => () => {}, onResized: async () => () => {},
    requestUserAttention: async () => {}, isFullscreen: async () => false, startDragging: async () => {},
  };
  window.__TAURI__ = {
    core: { invoke },
    event: { listen: async (name, f) => { (listeners[name] = listeners[name] || []).push(f); return () => {}; }, emit: async () => {} },
    window: { getCurrentWindow: () => win },
    webview: { getCurrentWebview: () => ({ label: win.label, onDragDropEvent: async () => () => {} }) },
    dialog: { open: async () => null, save: async () => null },
  };

  /// Where an element is, or several together, in the page's pixels.
  const rect = v => {
    if (!v) return null;
    const list = v instanceof Element ? [v] : Array.isArray(v) || v instanceof NodeList ? [...v] : null;
    if (!list) return [v.x, v.y, v.width, v.height];
    if (!list.length) return null;
    const rs = list.map(e => e.getBoundingClientRect());
    const x0 = Math.min(...rs.map(r => r.left)), y0 = Math.min(...rs.map(r => r.top));
    return [x0, y0, Math.max(...rs.map(r => r.right)) - x0, Math.max(...rs.map(r => r.bottom)) - y0];
  };
  const $ = (s, root = document) => root.querySelector(s);
  const $$ = (s, root = document) => [...root.querySelectorAll(s)];
  /// The first element matching `s` whose text has `text` in it.
  const withText = (s, text) => $$(s).find(e => e.textContent.includes(text));
  const click = e => {
    if (!e) throw new Error('nothing to click');
    // A whole click as the mouse makes it: the pages tell a first press
    // from the second of a double click by its count.
    for (const kind of ['mousedown', 'mouseup', 'click']) e.dispatchEvent(new MouseEvent(kind, { bubbles: true, cancelable: true, button: 0, detail: 1 }));
  };
  /// The `i`th titled card under `root`: its title and the card under it.
  const titled = (root, i) => { const title = $$(root + ' .card-title')[i]; return title ? [title, title.nextElementSibling] : null; };
  /// A button pressed, a row double-clicked, a key pressed with its
  /// modifiers (on the window, or where `target` says).
  const press = e => { if (!e) throw new Error('nothing to press'); e.click(); };
  const twice = e => { if (!e) throw new Error('nothing to double-click'); e.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true, button: 0 })); };
  const key = (k, mods = {}, target = window) => target.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true, ...mods }));
  const tools = { $, $$, withText, titled, click, press, twice, key, sleep, emit, bytes, state, files, D, rect };
  // What a scene wants in place before the page's own script runs.
  if (scene.before) new Function(...Object.keys(tools), scene.before)(...Object.values(tools));
  const AsyncFunction = Object.getPrototypeOf(async () => {}).constructor;

  let ran = false;
  async function run() {
    if (ran) return;
    ran = true;
    const shot = { rects: {}, missing: [], errors, unknown: [] };
    try {
      await new AsyncFunction(...Object.keys(tools), scene.setup || '')(...Object.values(tools));
      await sleep(scene.settle || 500);
      const part = how => rect(new Function(...Object.keys(tools), `return ${how};`)(...Object.values(tools)));
      const view = scene.crop ? part(scene.crop) : [0, 0, innerWidth, innerHeight];
      shot.crop = view;
      for (const [name, how] of Object.entries(scene.parts || {})) {
        let r = null;
        try { r = part(how); } catch (e) { errors.push(`${name}: ${e.message}`); }
        if (!r || !r[2] || !r[3]) { shot.missing.push(name); continue; }
        // As shares of the picture, whatever it is scaled to.
        shot.rects[name] = [(r[0] - view[0]) / view[2], (r[1] - view[1]) / view[3], r[2] / view[2], r[3] / view[3]];
      }
    } catch (e) {
      errors.push('setup: ' + String(e && e.stack || e));
    }
    // For whoever writes a scene: what an expression of its says now.
    if (scene.debug) { try { shot.debug = JSON.stringify(new Function(...Object.keys(tools), `return ${scene.debug};`)(...Object.values(tools))); } catch (e) { shot.debug = String(e); } }
    shot.unknown = [...unknown];
    window.__SHOT = JSON.stringify(shot);
  }
  // A page that never says it is drawn is taken as it stands.
  setTimeout(run, 8000);
})();
