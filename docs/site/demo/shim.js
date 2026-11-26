// What a page of ui/ gets in place of the app on the documentation site
// (docs/build.sh puts this script in front of each page it copies to
// app/): `window.__TAURI__` as the pages use it, every call handed to
// backend.js. The backend is the front page's, one for all its windows,
// so that a switch flipped in one window shows in the others; a page
// opened on its own loads a backend for itself. What goes across is
// copied, as over the app's IPC: the two sides are different pages.
(() => {
  'use strict';
  const Q = new URLSearchParams(location.search);
  window.__SHELLDER_LANG = Q.get('lang') === 'zh' ? 'zh-Hans' : 'en';
  // As the app tells a page on macOS: its top is clear of the traffic
  // lights, which desk.js draws there.
  if (/^Mac/.test(navigator.platform)) document.documentElement.classList.add('mac');
  // A Console takes a click in a window without the keyboard for the one
  // that brings the window forward, and swallows it: a frame has the
  // keyboard only once clicked.
  document.hasFocus = () => true;

  const label = Q.get('label') || 'main';
  const here = document.currentScript.src.replace(/[^/]*$/, '');
  let theirs = null;
  try { theirs = parent !== window && parent.__shellderBackend || null; } catch { /* in a frame of another site */ }
  if (!theirs) document.write(`<script src="${here}data.js"></script><script src="${here}backend.js"></script>`);

  const copy = v => v === undefined || v === null ? null : JSON.parse(JSON.stringify(v));
  const listeners = {};
  const emit = (name, payload) => { for (const f of listeners[name] || []) f({ payload: copy(payload) }); };
  let link = null;
  const linked = () => {
    if (!link) {
      const backend = theirs || window.__shellderBackend;
      if (!theirs) backend.lang = Q.get('lang') === 'zh' ? 'zh' : 'en';
      link = backend.attach(label, emit);
    }
    return link;
  };
  addEventListener('pagehide', () => { if (link) link.detach(); });

  const none = async () => {};
  const never = async () => () => {};
  const win = {
    label,
    setTitle: async text => linked().title(text), close: async () => linked().close(), destroy: async () => linked().close(),
    setFocus: none, requestUserAttention: none, startDragging: none, setFullscreen: none, isFullscreen: async () => false,
    onCloseRequested: never, onFocusChanged: never, onResized: never,
  };
  window.__TAURI__ = {
    core: { invoke: async (cmd, args) => copy(await linked().invoke(cmd, copy(args) || {})) },
    event: { listen: async (name, f) => { (listeners[name] = listeners[name] || []).push(f); return () => { listeners[name] = listeners[name].filter(x => x !== f); }; }, emit: none },
    window: { getCurrentWindow: () => win },
    webview: { getCurrentWebview: () => ({ label, onDragDropEvent: never }) },
    dialog: { open: async () => null, save: async () => null },
  };
})();
