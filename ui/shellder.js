// Glue every page shares: the bridge to the app, translations, and a few
// helpers for building the DOM.

import { ICONS } from './icons.js';

const tauri = window.__TAURI__;
export const invoke = (cmd, args) => tauri.core.invoke(cmd, args);
export const listen = (name, fn) => tauri.event.listen(name, e => fn(e.payload));

// MARK: scroll bars

// app.css draws a scroll bar's thumb with the opacity in --thumb-alpha on
// its area: full while the area scrolls, then, a moment after the last
// step, faded out frame by frame (WebKit does not animate scroll bars).
const thumbFades = new WeakMap();
document.addEventListener('scroll', e => {
  const area = e.target === document ? document.documentElement : e.target;
  if (!(area instanceof Element)) return;
  let fade = thumbFades.get(area);
  if (!fade) thumbFades.set(area, fade = {});
  clearTimeout(fade.timer);
  cancelAnimationFrame(fade.frame);
  area.style.setProperty('--thumb-alpha', '1');
  fade.timer = setTimeout(() => {
    const start = performance.now();
    const step = now => {
      const k = Math.min(1, (now - start) / 400);
      if (k < 1) {
        area.style.setProperty('--thumb-alpha', String((1 - k) * (1 - k)));
        fade.frame = requestAnimationFrame(step);
      } else area.style.removeProperty('--thumb-alpha');
    };
    fade.frame = requestAnimationFrame(step);
  }, 800);
}, true);

/// Wheel scrolling of `area` by hand: each turn of the wheel moves the
/// area `factor` of what the system would, and the move is eased over a
/// few frames rather than jumped, so a mouse wheel's notches do not throw
/// a long list about. A trackpad's gestures come as many small deltas,
/// which the easing follows closely. ⌘ and the wheel is left alone (a
/// zoom), as is a mostly sideways turn.
export function smoothWheel(area, factor = 0.6) {
  let target = null;
  let frame = 0;
  const step = () => {
    const diff = target - area.scrollTop;
    if (Math.abs(diff) < 0.5) { area.scrollTop = target; target = null; frame = 0; return; }
    area.scrollTop += diff * 0.22;
    frame = requestAnimationFrame(step);
  };
  area.addEventListener('wheel', e => {
    if (e.ctrlKey || e.metaKey || Math.abs(e.deltaX) > Math.abs(e.deltaY)) return;
    const dy = e.deltaMode === 1 ? e.deltaY * 16 : e.deltaMode === 2 ? e.deltaY * area.clientHeight : e.deltaY;
    if (!dy) return;
    e.preventDefault();
    if (target === null) target = area.scrollTop;
    target = Math.max(0, Math.min(area.scrollHeight - area.clientHeight, target + dy * factor));
    if (!frame) frame = requestAnimationFrame(step);
  }, { passive: false });
}

/// A column whose width the user sets by dragging its right edge, kept on
/// this machine under `key`; `min` and `max` are the widths allowed.
/// `collapse(folded)`, when given, is told when the drag goes well under
/// `min` (the column folds away, as an editor's side bar does) and when
/// it comes back out; `collapsed()` says whether it is folded now.
export function resizableWidth(column, key, min, max, { collapse = null, collapsed = () => false } = {}) {
  try { const saved = Number(localStorage.getItem(key)); if (saved) column.style.width = `${Math.min(max, Math.max(min, saved))}px`; } catch {}
  const handle = el('div.col-handle', { title: t('Drag to set the width') });
  handle.onmousedown = e => {
    if (e.button !== 0) return;
    e.preventDefault();
    const startX = e.clientX;
    let folded = collapsed();
    const startW = folded ? min : column.getBoundingClientRect().width;
    let width = startW;
    handle.classList.add('dragging');
    document.body.style.cursor = 'col-resize';
    const move = ev => {
      const w = startW + ev.clientX - startX;
      if (collapse) {
        if (w < min - 60) { if (!folded) { folded = true; collapse(true); } return; }
        if (folded) { folded = false; collapse(false); }
      }
      width = Math.min(max, Math.max(min, w));
      column.style.width = `${width}px`;
    };
    const up = () => {
      document.removeEventListener('mousemove', move); document.removeEventListener('mouseup', up);
      handle.classList.remove('dragging');
      document.body.style.cursor = '';
      if (!folded) { try { localStorage.setItem(key, String(Math.round(width))); } catch {} }
    };
    document.addEventListener('mousemove', move); document.addEventListener('mouseup', up);
  };
  column.append(handle);
}

// MARK: translations

let strings = {};
let language = 'en';

/// Load the language chosen in Settings. The keys are the English text;
/// what a translation lacks falls back to it.
export async function loadLanguage(lang) {
  language = lang || 'en';
  document.documentElement.lang = language;
  if (language === 'en') { strings = {}; return; }
  try {
    const r = await fetch(`lang/${language}.json`);
    strings = r.ok ? await r.json() : {};
  } catch { strings = {}; }
}

export function currentLanguage() { return language; }

/// t("Waiting for jump host {j}", {j: "gw"})
export function t(key, vars) {
  let s = strings[key] ?? key;
  if (vars) for (const [k, v] of Object.entries(vars)) s = s.split(`{${k}}`).join(String(v));
  return s;
}

/// Translate every element carrying data-i18n (its text) or data-i18n-title.
export function translateDocument() {
  for (const e of document.querySelectorAll('[data-i18n]')) e.textContent = t(e.dataset.i18n);
  for (const e of document.querySelectorAll('[data-i18n-title]')) e.title = t(e.dataset.i18nTitle);
  for (const e of document.querySelectorAll('[data-i18n-placeholder]')) e.placeholder = t(e.dataset.i18nPlaceholder);
}

// MARK: DOM

/// el('div.card', {title: 'x', onclick: f}, child, 'text', ...)
export function el(spec, attrs, ...children) {
  if (attrs && (attrs instanceof Node || typeof attrs !== 'object' || Array.isArray(attrs))) { children.unshift(attrs); attrs = {}; }
  const [tag, ...classes] = spec.split('.');
  const e = document.createElement(tag || 'div');
  if (classes.length) e.className = classes.join(' ');
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v === null || v === undefined || v === false) continue;
    if (k.startsWith('on')) e.addEventListener(k.slice(2), v);
    else if (k === 'class') e.className += ' ' + v;
    else if (k === 'dataset') Object.assign(e.dataset, v);
    else if (k === 'style') e.style.cssText = v;
    else if (k in e && k !== 'list') e[k] = v;
    else e.setAttribute(k, v === true ? '' : v);
  }
  append(e, children);
  return e;
}

export function append(parent, children) {
  for (const c of children.flat(Infinity)) {
    if (c === null || c === undefined || c === false) continue;
    parent.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return parent;
}

export function clear(e) { while (e.firstChild) e.removeChild(e.firstChild); return e; }

// MARK: icons (Lucide, see scripts/icons.py; drawn in the text colour)

export function icon(name, cls) {
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('viewBox', '0 0 24 24');
  svg.setAttribute('class', 'icon' + (cls ? ' ' + cls : ''));
  svg.setAttribute('aria-hidden', 'true');
  svg.innerHTML = ICONS[name] || ICONS.question;
  return svg;
}

// MARK: formatting

/// Bytes for people: decimal units (1 GB = 10⁹ bytes, what files, disks
/// and traffic are counted in, as Finder and the vendors do), or with
/// `binary` the 1024-based ones, labelled as such (1 GiB = 2³⁰ bytes:
/// memory, swap and a GPU's memory, as the kernel and the system's own
/// tools count them, so that 4 GiB of swap reads as 4.0 GiB, not 4.3 GB).
export function fmtBytes(n, binary = false) {
  if (n === null || n === undefined) return '';
  const units = binary ? ['B', 'KiB', 'MiB', 'GiB', 'TiB'] : ['B', 'KB', 'MB', 'GB', 'TB'];
  const step = binary ? 1024 : 1000;
  let i = 0, v = Number(n);
  while (v >= step && i < units.length - 1) { v /= step; i++; }
  return (i === 0 ? v.toFixed(0) : v < 10 ? v.toFixed(1) : v.toFixed(0)) + ' ' + units[i];
}

export function fmtDuration(seconds) {
  const s = Math.max(0, Math.floor(seconds));
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m ${s % 60}s`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ${Math.floor((s % 3600) / 60)}m`;
  return `${Math.floor(s / 86400)}d ${Math.floor((s % 86400) / 3600)}h`;
}

// Times are on the 24-hour clock whatever the locale (hourCycle h23).
// The locale dates, times and numbers are written in: the language chosen,
// not the system's, so an English page does not carry the system's
// dates. English keeps the system's region (en-GB, en-DE) for the order
// of day, month and year; a system that is not English gets en-REGION.
const locale = () => {
  if (language === 'zh-Hans') return 'zh-CN';
  if (language !== 'en') return language;
  const region = (navigator.language || '').split('-')[1];
  return region ? `en-${region}` : 'en';
};

export function fmtTime(unixSeconds) {
  return new Intl.DateTimeFormat(locale(), { timeStyle: 'medium', hourCycle: 'h23' }).format(new Date(unixSeconds * 1000));
}

/// A file's date the way a file list writes it: "Today, 10:31" or a short
/// date with the time. `compact`, for a narrow column: the time alone
/// today, "Yesterday", else the date without the time.
export function fmtFileDate(unixSeconds, compact = false) {
  if (unixSeconds === null || unixSeconds === undefined) return '—';
  const d = new Date(unixSeconds * 1000);
  const today = new Date();
  const sameDay = d.toDateString() === today.toDateString();
  const yesterday = new Date(today); yesterday.setDate(today.getDate() - 1);
  const time = new Intl.DateTimeFormat(locale(), { timeStyle: 'short', hourCycle: 'h23' }).format(d);
  if (sameDay) return compact ? time : t('Today') + ', ' + time;
  if (d.toDateString() === yesterday.toDateString()) return compact ? t('Yesterday') : t('Yesterday') + ', ' + time;
  if (compact) return new Intl.DateTimeFormat(locale(), { dateStyle: 'medium' }).format(d);
  return new Intl.DateTimeFormat(locale(), { dateStyle: 'medium', timeStyle: 'short', hourCycle: 'h23' }).format(d);
}

export function abbreviateHome(path, home) {
  return home && path.startsWith(home + '/') ? '~' + path.slice(home.length) : path === home ? '~' : path;
}

// MARK: the app's state

let state = null;
const watchers = [];

export function currentState() { return state; }

/// Call `fn` with the state now and after every change.
export async function watchState(fn) {
  watchers.push(fn);
  if (state) fn(state);
}

export async function connectState() {
  await listen('state', s => { state = s; for (const w of watchers) w(s); });
  state = await invoke('get_state');
  for (const w of watchers) w(state);
  return state;
}

/// What every page does first: the state and the translation, fetched
/// side by side. The language comes from the window (`__SHELLDER_LANG`,
/// set by the app when it built it), so the fetch need not wait for the
/// state to say which it is.
export async function startPage() {
  const lang = window.__SHELLDER_LANG;
  const [s] = await Promise.all([connectState(), lang ? loadLanguage(lang) : null]);
  if (!lang) await loadLanguage(s.settings.language);
  return s;
}

/// Tell the app the page is drawn: the window, built hidden, comes on
/// screen. Until then the user would see the web view's blank first frame.
export function pageReady() { invoke('window_ready'); }

/// A sentence for a host's state, for the dot's tooltip, the host page
/// and the menu. `socket` false: the master keeps the connection without a
/// socket other ssh commands could reuse.
export function stateLabel(st, socket) {
  if (!st) return t('Disconnected');
  switch (st.kind) {
    case 'off': return t('Disconnected');
    case 'up': return socket === false ? t('Connected (no socket)') : t('Connected');
    case 'connecting': return t('Connecting…');
    case 'foreign': return t('Connected (external master)');
    case 'waiting': return st.retryIn > 0 ? t('Dropped — retrying in {n}s', { n: st.retryIn }) : t('Retrying…');
    case 'waitingForJump': return t('Waiting for jump host {j}', { j: st.jump });
    case 'error': return st.message;
  }
  return '';
}

export const isUp = st => st && (st.kind === 'up' || st.kind === 'foreign');
export const isRunning = st => st && (st.kind === 'up' || st.kind === 'connecting');
export const failed = status => status && status.state.kind === 'off' && status.lastError;

export function statusSummary(status) {
  const base = failed(status) ? t('Failed') : stateLabel(status && status.state, status && status.socket);
  const by = status ? status.neededBy : [];
  return by.length ? t('{base} · jump host for {hosts}', { base, hosts: by.join(', ') }) : base;
}

/// One colour per host, worn by its dot and by its switch: red when the
/// last attempt failed, yellow while one is under way, green once the
/// master is up, blue once it is up and locked, cyan when the master up is
/// somebody else's (an interactive ssh's ControlMaster=auto): the lock
/// cannot keep that one, so it does not turn blue. Off is grey.
export function hostColor(status, locked) {
  if (!status) return 'off';
  if (failed(status)) return 'red';
  switch (status.state.kind) {
    case 'up': return locked ? 'blue' : 'green';
    case 'foreign': return 'cyan';
    case 'connecting': case 'waiting': case 'waitingForJump': return 'yellow';
    case 'error': return 'red';
    default: return 'off';
  }
}

/// Modal dialogs drawn in the page: ask for a line of text, confirm.
/// `secret` masks what is typed, with an eye button that shows it.
export function dialog({ title, message, preset, placeholder, action, destructive, cancel, input, secret, extra }) {
  return new Promise(resolve => {
    const field = input ? el('input.text', { type: secret ? 'password' : 'text', value: preset || '', placeholder: placeholder || '', spellcheck: false, autocomplete: 'off', autocapitalize: 'off' }) : null;
    const eye = field && secret ? el('button.borderless', { title: t('Show what you typed'), onclick: () => {
      field.type = field.type === 'password' ? 'text' : 'password';
      eye.replaceChildren(icon(field.type === 'password' ? 'eye' : 'eyeoff'));
      eye.title = field.type === 'password' ? t('Show what you typed') : t('Hide');
      field.focus();
    } }, icon('eye')) : null;
    const extraBox = extra ? el('label.row', el('input', { type: 'checkbox' }), ' ', extra) : null;
    const done = v => { back.remove(); resolve(v); };
    const ok = el('button.primary', { class: destructive ? 'destructive' : '', onclick: () => done({ ok: true, value: field ? field.value.trim() : true, extra: extraBox ? extraBox.firstChild.checked : false }) }, action || t('OK'));
    const no = el('button', { onclick: () => done({ ok: false, extra: extraBox ? extraBox.firstChild.checked : false }) }, cancel || t('Cancel'));
    const box = el('div.dialog', el('h3', title), message ? el('p.secondary', message) : null, eye ? el('div.row', field, eye) : field, extraBox, el('div.buttons', no, ok));
    const back = el('div.backdrop', { onkeydown: e => { if (e.key === 'Escape') done({ ok: false }); if (e.key === 'Enter' && (!field || field.value.trim())) ok.click(); } }, box);
    document.body.append(back);
    (field || ok).focus();
    if (field) field.select();
  });
}

export function toast(text, isError) {
  const e = el('div.toast', { class: isError ? 'error' : '' }, text);
  document.body.append(e);
  setTimeout(() => e.remove(), 4000);
}

// MARK: menus

let openMenu = null;

/// Show a popup under its button and return what closes it. The button
/// toggles: when its popup is already open the click closes it and this
/// returns null. A mousedown anywhere else, another such button included,
/// closes the popup.
export function popup(anchor, m) {
  if (openMenu && openMenu.anchor === anchor) { openMenu.close(); return null; }
  if (openMenu) openMenu.close();
  const rect = anchor.getBoundingClientRect();
  const close = () => { m.remove(); document.removeEventListener('mousedown', outside); if (openMenu && openMenu.anchor === anchor) openMenu = null; };
  const outside = e => { if (!m.contains(e.target) && !anchor.contains(e.target)) close(); };
  openMenu = { anchor, close };
  // Measured at the window's left edge first: a fixed box placed at the
  // button is no wider than the room right of it, so near the right edge
  // its lines would wrap and it would measure as fitting. Then put under
  // the button, shifted left near the right edge and above the button
  // when there is no room below it.
  m.style.left = '0px'; m.style.top = '0px'; m.style.visibility = 'hidden';
  document.body.append(m);
  const w = m.offsetWidth, h = m.offsetHeight;
  m.style.left = `${Math.max(8, Math.min(rect.left, window.innerWidth - 8 - w))}px`;
  m.style.top = `${rect.bottom + 4 + h > window.innerHeight - 8 ? Math.max(8, rect.top - 4 - h) : rect.bottom + 4}px`;
  m.style.visibility = '';
  setTimeout(() => document.addEventListener('mousedown', outside), 0);
  return close;
}

/// What a popup opened at a point is placed against: a right-click's.
export function pointAnchor(x, y) {
  return { getBoundingClientRect: () => ({ left: x, top: y, bottom: y - 4, right: x }), contains: () => false };
}

/// A menu at a point: what a right-click opens.
export function menuAt(x, y, items) {
  menu(pointAnchor(x, y), items);
}

/// `items`: `{title, icon, run, disabled}` or `{sep: true}`; a null is
/// left out, and so is a separator that would start or end the menu or
/// follow another.
export function menu(anchor, items) {
  const m = el('div.menu');
  let close = null;
  const shown = [];
  for (const it of items) if (it && !(it.sep && (!shown.length || shown[shown.length - 1].sep))) shown.push(it);
  if (shown.length && shown[shown.length - 1].sep) shown.pop();
  for (const it of shown) m.append(it.sep ? el('div.sep') : el('div.item', { class: it.disabled ? 'disabled' : '', onclick: () => { close(); it.run(); } }, it.icon ? icon(it.icon) : null, it.title));
  close = popup(anchor, m);
}

/// Keyboard shortcuts common to every window.
export function commonKeys() {
  document.addEventListener('keydown', e => {
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.key === 'w') { e.preventDefault(); tauri.window.getCurrentWindow().close(); }
    if (mod && e.key === ',') { e.preventDefault(); invoke('open_window', { which: 'settings' }); }
    if (mod && e.key === 'q') { e.preventDefault(); invoke('quit'); }
  });
}
