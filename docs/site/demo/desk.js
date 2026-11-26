// The front page's desk: the demo's windows, each a page of ui/ in a
// frame (app/, with shim.js in front) at the size the app gives it,
// laid over one another as on a screen. A window comes forward when it
// is clicked or opened again, from the buttons over the desk or from
// another window as in the app (the main window's Console button). The
// desk is drawn 1200 points wide and scaled down where the page is
// narrower.
(() => {
  'use strict';
  const B = window.__shellderBackend, stage = document.getElementById('stage');
  if (!B || !stage) return;
  const MAC = /^Mac/.test(navigator.platform);
  const W = 1200, H = 760, BAR = MAC ? 0 : 30;
  const KINDS = {
    main: { page: 'index.html', w: 900, h: 620, x: 150, y: 70, title: 'Shellder' },
    console: { page: 'console.html', w: 1160, h: 720, x: 20, y: 20, title: 'Console' },
    files: { page: 'files.html', w: 1000, h: 620, x: 110, y: 96, title: 'Files' },
    settings: { page: 'settings.html', w: 560, h: 600, x: 320, y: 84, title: 'Settings' },
    termprefs: { page: 'consoleprefs.html', w: 780, h: 660, x: 210, y: 50, title: 'Console Settings' },
  };
  const lang = () => document.documentElement.dataset.lang === 'zh' ? 'zh' : 'en';
  const open = new Map();   // label -> {label, kind, host, el, frame}
  let top = 10;
  const src = w => `app/${KINDS[w.kind].page}?label=${encodeURIComponent(w.label)}&lang=${lang()}${w.host ? '&host=' + encodeURIComponent(w.host) : ''}`;
  const el = (tag, cls, ...kids) => { const e = document.createElement(tag); if (cls) e.className = cls; e.append(...kids); return e; };

  function raise(w, keyboard) {
    for (const o of open.values()) o.el.classList.toggle('front', o === w);
    w.el.style.zIndex = ++top;
    if (keyboard) try { w.frame.contentWindow.focus(); } catch { /* not loaded yet */ }
  }
  function close(label) {
    const w = open.get(label);
    if (!w) return;
    open.delete(label);
    w.el.remove();
    const next = [...open.values()].sort((a, b) => b.el.style.zIndex - a.el.style.zIndex)[0];
    if (next) raise(next, false);
  }
  function openWindow(kind, host, keyboard = true) {
    const k = KINDS[kind];
    if (!k) return;
    if (kind === 'console' || kind === 'files') host = host || 'gpu-box'; else host = null;
    const label = host ? `${kind}-${host.replace(/[^\w-]/g, '_')}` : kind;
    let w = open.get(label);
    if (w) return raise(w, keyboard);
    w = { label, kind, host };
    w.frame = el('iframe');
    w.frame.src = src(w);
    w.frame.title = k.title;
    const shut = el('button', 'shut');
    shut.setAttribute('aria-label', 'Close');
    shut.onclick = () => close(label);
    w.name = el('span', 'name', k.title);
    const cover = el('div', 'cover');
    cover.onmousedown = e => { e.preventDefault(); raise(w, true); };
    w.el = el('div', 'win' + (MAC ? ' mac' : ''), MAC ? el('div', 'lights', shut, el('i'), el('i')) : el('div', 'titlebar', w.name, shut), w.frame, cover);
    // The cascade of a second window of a kind, as a screen gives one.
    const n = [...open.values()].filter(o => o.kind === kind).length;
    w.el.style.cssText = `left:${k.x + n * 24}px;top:${k.y + n * 24}px;width:${k.w}px;height:${k.h + BAR}px`;
    stage.append(w.el);
    open.set(label, w);
    raise(w, false);
    if (keyboard) w.frame.addEventListener('load', () => raise(w, true), { once: true });
  }

  B.lang = lang();
  B.desk = {
    open: (which, host) => openWindow(which, host),
    close,
    title: (label, text) => { const w = open.get(label); if (w && text) w.name.textContent = text; },
    // What the system's notification centre would show.
    notify: (title, body) => {
      const n = el('div', 'notice', el('b', '', title || 'Shellder'), el('span', '', body || ''));
      stage.append(n);
      setTimeout(() => n.remove(), 6000);
    },
  };

  const fit = () => {
    const s = Math.min(1, stage.parentElement.clientWidth / W);
    stage.style.transform = `scale(${s})`;
    stage.parentElement.style.height = Math.round(H * s) + 'px';
  };
  new ResizeObserver(fit).observe(stage.parentElement);
  fit();

  for (const b of document.querySelectorAll('[data-open]')) b.addEventListener('click', () => openWindow(b.dataset.open, 'gpu-box'));

  // "Show me": a step a visitor may not find, done in the Console the
  // way a hand would do it, by its buttons and keys.
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  // A panel shown: its button is pressed unless the panel is up already,
  // since a press on the panel that is up folds the side bar away.
  const panel = async (d, i) => {
    const tabs = [...d.querySelectorAll('#tabs .tab')], tab = tabs[i < 0 ? tabs.length + i : i];
    if (!tab.classList.contains('active') || d.querySelector('.side.collapsed')) { tab.click(); await sleep(700); }
  };
  const clickOn = (d, e) => { for (const kind of ['mousedown', 'mouseup', 'click']) e.dispatchEvent(new d.defaultView.MouseEvent(kind, { bubbles: true, cancelable: true, button: 0, detail: 1 })); };
  const steps = {
    files: async d => { await panel(d, 1); const row = d.querySelector('.filesbox tr[data-path$="README.md"]'); if (row) clickOn(d, row); },
    sessions: async d => { await panel(d, 2); const s = d.querySelector('#side-body .session'); if (s) s.click(); },
    split: async d => { d.querySelector('.splitbtn').click(); await sleep(250); const item = d.querySelector('.menu .item'); if (item) item.click(); },
    wheel: async d => { d.defaultView.dispatchEvent(new d.defaultView.KeyboardEvent('keydown', { key: 'Meta', metaKey: true, shiftKey: true, bubbles: true })); },
  };
  for (const b of document.querySelectorAll('[data-do]')) b.addEventListener('click', async () => {
    openWindow('console', 'gpu-box', true);
    const w = [...open.values()].find(o => o.kind === 'console' && o.host === 'gpu-box');
    stage.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
    for (let i = 0; i < 40 && !(w.frame.contentDocument && w.frame.contentDocument.querySelector('#tabs .tab')); i++) await sleep(100);
    const d = w.frame.contentDocument;
    try { await steps[b.dataset.do](d); } catch { /* the page is not where the step expects it */ }
  });

  // The other language: the windows are the app's, which reads its
  // language when a page is built, so they are built again.
  document.addEventListener('shellder-lang', () => { B.lang = lang(); for (const w of open.values()) w.frame.src = src(w); });

  openWindow('console', 'gpu-box', false);
})();
