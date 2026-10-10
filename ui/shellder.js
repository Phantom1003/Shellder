// Glue every page shares: the bridge to the app, translations, and a few
// helpers for building the DOM.

import { ICONS } from './icons.js';

const tauri = window.__TAURI__;
export const invoke = (cmd, args) => tauri.core.invoke(cmd, args);
export const listen = (name, fn) => tauri.event.listen(name, e => fn(e.payload));

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

export function fmtBytes(n) {
  if (n === null || n === undefined) return '';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let i = 0, v = Number(n);
  while (v >= 1000 && i < units.length - 1) { v /= 1000; i++; }
  return (i === 0 ? v.toFixed(0) : v < 10 ? v.toFixed(1) : v.toFixed(0)) + ' ' + units[i];
}

export function fmtDuration(seconds) {
  const s = Math.max(0, Math.floor(seconds));
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m ${s % 60}s`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ${Math.floor((s % 3600) / 60)}m`;
  return `${Math.floor(s / 86400)}d ${Math.floor((s % 86400) / 3600)}h`;
}

const locale = () => (language === 'zh-Hans' ? 'zh-CN' : language === 'en' ? navigator.language : language);

export function fmtTime(unixSeconds) {
  return new Intl.DateTimeFormat(locale(), { timeStyle: 'medium' }).format(new Date(unixSeconds * 1000));
}

/// A file's date the way a file list writes it: "Today, 10:31" or a short date.
export function fmtFileDate(unixSeconds) {
  if (unixSeconds === null || unixSeconds === undefined) return '—';
  const d = new Date(unixSeconds * 1000);
  const today = new Date();
  const sameDay = d.toDateString() === today.toDateString();
  const yesterday = new Date(today); yesterday.setDate(today.getDate() - 1);
  const time = new Intl.DateTimeFormat(locale(), { timeStyle: 'short' }).format(d);
  if (sameDay) return t('Today') + ', ' + time;
  if (d.toDateString() === yesterday.toDateString()) return t('Yesterday') + ', ' + time;
  return new Intl.DateTimeFormat(locale(), { dateStyle: 'medium', timeStyle: 'short' }).format(d);
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

/// A sentence for a host's state, for the dot's tooltip, the host page
/// and the menu.
export function stateLabel(st) {
  if (!st) return t('Disconnected');
  switch (st.kind) {
    case 'off': return t('Disconnected');
    case 'up': return t('Connected');
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
  const base = failed(status) ? t('Failed') : stateLabel(status && status.state);
  const by = status ? status.neededBy : [];
  return by.length ? t('{base} · jump host for {hosts}', { base, hosts: by.join(', ') }) : base;
}

export function dotClass(status) {
  if (!status) return 'off';
  if (failed(status)) return 'red';
  switch (status.state.kind) {
    case 'up': return 'green';
    case 'foreign': return 'teal';
    case 'connecting': case 'waitingForJump': return 'yellow';
    case 'waiting': return 'orange';
    case 'error': return 'red';
    default: return 'off';
  }
}

/// Modal dialogs drawn in the page: ask for a line of text, confirm.
export function dialog({ title, message, preset, action, destructive, cancel, input, extra }) {
  return new Promise(resolve => {
    const field = input ? el('input.text', { value: preset || '', spellcheck: false }) : null;
    const extraBox = extra ? el('label.row', el('input', { type: 'checkbox' }), ' ', extra) : null;
    const done = v => { back.remove(); resolve(v); };
    const ok = el('button.primary', { class: destructive ? 'destructive' : '', onclick: () => done({ ok: true, value: field ? field.value.trim() : true, extra: extraBox ? extraBox.firstChild.checked : false }) }, action || t('OK'));
    const no = el('button', { onclick: () => done({ ok: false, extra: extraBox ? extraBox.firstChild.checked : false }) }, cancel || t('Cancel'));
    const box = el('div.dialog', el('h3', title), message ? el('p.secondary', message) : null, field, extraBox, el('div.buttons', no, ok));
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

/// Keyboard shortcuts common to every window.
export function commonKeys() {
  document.addEventListener('keydown', e => {
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.key === 'w') { e.preventDefault(); tauri.window.getCurrentWindow().close(); }
    if (mod && e.key === ',') { e.preventDefault(); invoke('open_window', { which: 'settings' }); }
    if (mod && e.key === 'q') { e.preventDefault(); invoke('quit'); }
  });
}
