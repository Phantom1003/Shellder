// The Console's keys: what each does, the defaults (a terminal's usual
// ones on each platform), the user's own, kept on this machine (`localStorage`, which
// every window shares; a window hears of another's change), and the
// panel that shows and changes them, opened from the keyboard button at
// the window's top right.
//
// A key is a string: the modifiers held, in the order Ctrl, Alt, Shift,
// Meta, then the key itself, '+' between (`Meta+Shift+T`, `Alt+ArrowLeft`,
// `Ctrl+Tab`). The key is the physical one (`KeyboardEvent.code`, so a
// layout's letters do not move it): a letter, a digit, a punctuation
// mark as on a US keyboard, or a name (ArrowLeft, Enter, Backspace, Tab,
// Home, End, PageUp, PageDown, Escape, Space, F1 …). An action may have
// several keys; the first action bound to a key gets it.
import { t, el, clear, icon, popup } from './shellder.js';

export const MAC = document.documentElement.classList.contains('mac') || /^Mac/.test(navigator.platform);
const STORE = 'console-keys';

/// Every action, in the order the panel lists them, with its title and
/// its keys on macOS and elsewhere (a terminal's usual ones where there
/// are any). `terminal` marks what the terminal does itself.
const ACTIONS = [
  { section: () => t('Tabs') },
  { id: 'new-tab', title: () => t('A new shell'), mac: ['Meta+T'], other: ['Ctrl+Shift+T'] },
  { id: 'close-tab', title: () => t('Close the tab (with no tab open, the window)'), mac: ['Meta+W'], other: ['Ctrl+Shift+W'] },
  { id: 'reopen', title: () => t('Bring the tab closed last back'), mac: ['Meta+Shift+T', 'Meta+Z'], other: ['Ctrl+Shift+Z'] },
  { id: 'next-tab', title: () => t('The next tab'), mac: ['Meta+Shift+]', 'Ctrl+Tab'], other: ['Ctrl+Tab', 'Ctrl+PageDown'] },
  { id: 'prev-tab', title: () => t('The previous tab'), mac: ['Meta+Shift+[', 'Ctrl+Shift+Tab'], other: ['Ctrl+Shift+Tab', 'Ctrl+PageUp'] },
  ...[1, 2, 3, 4, 5, 6, 7, 8].map(n => ({ id: `tab-${n}`, title: () => t('Tab {n}', { n }), mac: [`Meta+${n}`], other: [`Alt+${n}`] })),
  { id: 'tab-last', title: () => t('The last tab'), mac: ['Meta+9'], other: ['Alt+9'] },
  { section: () => t('Groups') },
  { id: 'split-right', title: () => t('Split right'), mac: ['Meta+D'], other: ['Ctrl+Shift+O'] },
  { id: 'split-down', title: () => t('Split down'), mac: ['Meta+Shift+D'], other: ['Ctrl+Shift+E'] },
  { id: 'close-group', title: () => t('Close the group, every tab of it'), mac: ['Alt+Meta+W'], other: ['Ctrl+Alt+Shift+W'] },
  { id: 'next-group', title: () => t('The keyboard to the next group'), mac: ['Meta+]'], other: ['Ctrl+Meta+]'] },
  { id: 'prev-group', title: () => t('The keyboard to the previous group'), mac: ['Meta+['], other: ['Ctrl+Meta+['] },
  { id: 'group-up', title: () => t('The keyboard to the group above'), mac: ['Alt+Meta+ArrowUp'], other: ['Ctrl+Alt+ArrowUp'] },
  { id: 'group-down', title: () => t('The keyboard to the group below'), mac: ['Alt+Meta+ArrowDown'], other: ['Ctrl+Alt+ArrowDown'] },
  { id: 'group-left', title: () => t('The keyboard to the group on the left'), mac: ['Alt+Meta+ArrowLeft'], other: ['Ctrl+Alt+ArrowLeft'] },
  { id: 'group-right', title: () => t('The keyboard to the group on the right'), mac: ['Alt+Meta+ArrowRight'], other: ['Ctrl+Alt+ArrowRight'] },
  { id: 'resize-up', title: () => t('Move the divider up'), mac: ['Ctrl+Meta+ArrowUp'], other: ['Ctrl+Shift+Meta+ArrowUp'] },
  { id: 'resize-down', title: () => t('Move the divider down'), mac: ['Ctrl+Meta+ArrowDown'], other: ['Ctrl+Shift+Meta+ArrowDown'] },
  { id: 'resize-left', title: () => t('Move the divider left'), mac: ['Ctrl+Meta+ArrowLeft'], other: ['Ctrl+Shift+Meta+ArrowLeft'] },
  { id: 'resize-right', title: () => t('Move the divider right'), mac: ['Ctrl+Meta+ArrowRight'], other: ['Ctrl+Shift+Meta+ArrowRight'] },
  { id: 'equalize', title: () => t('Even the splits out'), mac: ['Ctrl+Meta+='], other: ['Ctrl+Meta+='] },
  { id: 'zoom', title: () => t('The group alone, and back'), mac: ['Meta+Shift+Enter'], other: ['Ctrl+Shift+Enter'] },
  { section: () => t('Window') },
  { id: 'new-window', title: () => t('Another Console window on the host'), mac: ['Meta+N'], other: ['Ctrl+Shift+N'] },
  { id: 'close-window', title: () => t('Close the window'), mac: ['Meta+Shift+W'], other: ['Alt+F4'] },
  { id: 'fullscreen', title: () => t('Full screen, and back'), mac: ['Meta+Enter', 'Ctrl+Meta+F'], other: ['Ctrl+Enter'] },
  { id: 'refresh', title: () => t('Read the Status tab again now'), mac: ['Meta+R'], other: ['Ctrl+Shift+R'] },
  { id: 'toggle-side', title: () => t('Fold the column away, and back'), mac: ['Meta+Shift+S'], other: ['Ctrl+Shift+S'] },
  { id: 'settings', title: () => t('Settings'), mac: ['Meta+,'], other: ['Ctrl+,'] },
  { id: 'quit', title: () => t('Quit'), mac: ['Meta+Q'], other: ['Ctrl+Shift+Q'] },
  { section: () => t('Terminal') },
  { id: 'copy', title: () => t('Copy the selection (without one, the key goes to the shell)'), mac: ['Meta+C'], other: ['Ctrl+Shift+C'], terminal: true },
  { id: 'select-all', title: () => t('Select all'), mac: ['Meta+A'], other: ['Ctrl+Shift+A'], terminal: true },
  { id: 'clear', title: () => t('Clear the terminal'), mac: ['Meta+K'], other: ['Ctrl+Shift+K'], terminal: true },
  { id: 'font-bigger', title: () => t('Bigger letters'), mac: ['Meta+=', 'Meta+Shift+='], other: ['Ctrl+=', 'Ctrl+Shift+='], terminal: true },
  { id: 'font-smaller', title: () => t('Smaller letters'), mac: ['Meta+-'], other: ['Ctrl+-'], terminal: true },
  { id: 'font-reset', title: () => t('The letters back to 13 points'), mac: ['Meta+0'], other: ['Ctrl+0'], terminal: true },
  { id: 'scroll-top', title: () => t('Scroll to the top'), mac: ['Meta+Home'], other: ['Shift+Home'], terminal: true },
  { id: 'scroll-bottom', title: () => t('Scroll to the bottom'), mac: ['Meta+End'], other: ['Shift+End'], terminal: true },
  { id: 'page-up', title: () => t('Scroll a page up'), mac: ['Meta+PageUp'], other: ['Shift+PageUp'], terminal: true },
  { id: 'page-down', title: () => t('Scroll a page down'), mac: ['Meta+PageDown'], other: ['Shift+PageDown'], terminal: true },
  { id: 'line-start', title: () => t('The start of the line (^A to the shell)'), mac: ['Meta+ArrowLeft'], other: [], terminal: true },
  { id: 'line-end', title: () => t('The end of the line (^E to the shell)'), mac: ['Meta+ArrowRight'], other: [], terminal: true },
  { id: 'line-delete', title: () => t('The line gone (^U to the shell)'), mac: ['Meta+Backspace'], other: [], terminal: true },
  { id: 'word-back', title: () => t('A word back (ESC b to the shell)'), mac: ['Alt+ArrowLeft'], other: [], terminal: true },
  { id: 'word-forward', title: () => t('A word forward (ESC f to the shell)'), mac: ['Alt+ArrowRight'], other: [], terminal: true },
  { id: 'search', title: () => t('Find in the terminal'), mac: ['Meta+F'], other: ['Ctrl+Shift+F'], terminal: true },
  { id: 'prompt-prev', title: () => t('Scroll to the previous prompt (with the shell integration)'), mac: ['Meta+ArrowUp'], other: ['Ctrl+Shift+ArrowUp'], terminal: true },
  { id: 'prompt-next', title: () => t('Scroll to the next prompt'), mac: ['Meta+ArrowDown'], other: ['Ctrl+Shift+ArrowDown'], terminal: true },
  { id: 'select-output', title: () => t("Select the last command's output"), mac: ['Meta+Shift+A'], other: ['Ctrl+Alt+A'], terminal: true },
];
const actions = ACTIONS.filter(a => a.id);
const byId = Object.fromEntries(actions.map(a => [a.id, a]));
const defaults = a => a[MAC ? 'mac' : 'other'];

/// The key with its modifiers in the one order (`keyOf` writes them in
/// it; the defaults above and a saved key are put in it too).
const RANK = { Ctrl: 0, Alt: 1, Shift: 2, Meta: 3 };
function canon(key) {
  const parts = String(key).split('+');
  const k = parts.pop();
  return [...parts.filter(m => m in RANK).sort((a, b) => RANK[a] - RANK[b]), k].join('+');
}
for (const a of actions) { a.mac = a.mac.map(canon); a.other = a.other.map(canon); }

// MARK: the bindings

let own = {};            // the user's keys by action, only where they differ from the default
let bound = new Map();   // key -> the first action bound to it
const listeners = [];

function load() {
  try { own = JSON.parse(localStorage.getItem(STORE)) || {}; } catch { own = {}; }
  for (const [id, v] of Object.entries(own)) if (!byId[id] || !Array.isArray(v)) delete own[id]; else own[id] = v.map(canon);
  bound = new Map();
  for (const a of actions) for (const k of keysOf(a.id)) if (!bound.has(k)) bound.set(k, a.id);
  for (const f of listeners) f();
}

function save() {
  try { if (Object.keys(own).length) localStorage.setItem(STORE, JSON.stringify(own)); else localStorage.removeItem(STORE); } catch {}
  load();
}

/// The keys of the action, the user's or the default.
export function keysOf(id) { return own[id] || defaults(byId[id]); }
export const isDefault = id => !(id in own);

function setKeys(id, keys) {
  const d = defaults(byId[id]);
  if (keys.length === d.length && keys.every((k, i) => k === d[i])) delete own[id]; else own[id] = keys;
  save();
}

/// Called when the keys change, here or in another window.
export function onChange(f) { listeners.push(f); }

/// What the keyboard event asks for, or null.
export function actionFor(e) {
  const k = keyOf(e);
  return k ? bound.get(k) || null : null;
}

// MARK: keys from events and on screen

const CODES = {
  Enter: 'Enter', NumpadEnter: 'Enter', Backspace: 'Backspace', Tab: 'Tab', Escape: 'Escape', Space: 'Space',
  Home: 'Home', End: 'End', PageUp: 'PageUp', PageDown: 'PageDown', Delete: 'Delete', Insert: 'Insert',
  ArrowUp: 'ArrowUp', ArrowDown: 'ArrowDown', ArrowLeft: 'ArrowLeft', ArrowRight: 'ArrowRight',
  Equal: '=', Minus: '-', BracketLeft: '[', BracketRight: ']', Backslash: '\\', Comma: ',', Period: '.',
  Slash: '/', Semicolon: ';', Quote: "'", Backquote: '`', NumpadAdd: '=', NumpadSubtract: '-',
};
const MODIFIERS = new Set(['Shift', 'Control', 'Alt', 'Meta', 'CapsLock', 'Fn', 'Dead']);

/// The event's key as a binding string, or null for a modifier alone.
export function keyOf(e) {
  if (MODIFIERS.has(e.key)) return null;
  const c = e.code || '';
  let k = CODES[c];
  if (!k) {
    const m = /^(Key|Digit|Numpad)(\w)$/.exec(c) || /^(F)(\d{1,2})$/.exec(c);
    if (m) k = m[1] === 'F' ? 'F' + m[2] : m[2].toUpperCase();
    else if (e.key && e.key.length === 1) k = e.key.toUpperCase();
    else k = e.key || c;
  }
  return (e.ctrlKey ? 'Ctrl+' : '') + (e.altKey ? 'Alt+' : '') + (e.shiftKey ? 'Shift+' : '') + (e.metaKey ? 'Meta+' : '') + k;
}

const GLYPHS = { ArrowUp: '↑', ArrowDown: '↓', ArrowLeft: '←', ArrowRight: '→' };
const MAC_GLYPHS = { ...GLYPHS, Enter: '↩', Backspace: '⌫', Tab: '⇥', Escape: '⎋', Space: '␣', Home: '↖', End: '↘', PageUp: '⇞', PageDown: '⇟', Delete: '⌦' };
const MAC_MODS = { Ctrl: '⌃', Alt: '⌥', Shift: '⇧', Meta: '⌘' };
const OTHER_MODS = { Ctrl: 'Ctrl', Alt: 'Alt', Shift: 'Shift', Meta: 'Super' };

/// The key as it is written: ⌥⇧⌘T on macOS, Ctrl+Shift+T elsewhere.
export function describe(key) {
  const parts = key.split('+');
  const k = parts.pop();
  if (MAC) return parts.map(m => MAC_MODS[m] || m).join('') + (MAC_GLYPHS[k] || k);
  return [...parts.map(m => OTHER_MODS[m] || m), GLYPHS[k] || k].join('+');
}

// MARK: the panel

let openPanel = null;

/// The keys, each a chip to press and change: the list with its head
/// (a reset) and its note, as an element to put anywhere (the Console's
/// settings window), and `dispose` for when it leaves the page.
export function keysView() {
  const list = el('div.list');
  const m = el('div.keys',
    el('div.khead', el('h3', t('Keys')), el('div.spacer'),
      el('button.borderless', { title: t('Every key back to its default'), onclick: () => { own = {}; save(); } }, icon('undo'), ' ', t('Reset all'))),
    el('div.note', t('Click a key, then press the keys to use instead; × takes it off, + adds one, Esc stops a change. A letter needs a modifier with it.')),
    list);
  let recording = null;   // {id, i} while a chip waits for keys
  let taking = null;      // the listener that waits for them
  const render = () => {
    clear(list);
    for (const a of ACTIONS) {
      if (a.section) { list.append(el('div.sect', a.section())); continue; }
      const keys = keysOf(a.id);
      const chips = keys.map((k, i) => {
        const other = bound.get(k) !== a.id ? byId[bound.get(k)] : null;
        const rec = recording && recording.id === a.id && recording.i === i;
        return el('span.key', { class: (other ? 'conflict' : '') + (rec ? ' recording' : '') },
          el('kbd', { title: other ? t('{action} has this key too; the first in the list gets it', { action: other.title() }) : t('Click, then press the keys to use instead'), onclick: () => record(a.id, i) }, rec ? t('Press the keys…') : describe(k)),
          el('button.borderless.x', { title: t('Take this key off'), onclick: () => setKeys(a.id, keys.filter((_, j) => j !== i)) }, icon('close')));
      });
      const adding = recording && recording.id === a.id && recording.i === keys.length;
      list.append(el('div.krow', { class: isDefault(a.id) ? '' : 'own' },
        el('span.title', a.title()),
        ...chips,
        adding ? el('span.key.recording', el('kbd', t('Press the keys…'))) : el('button.borderless.add', { title: t('Add a key'), onclick: () => record(a.id, keys.length) }, icon('plus')),
        el('button.borderless.reset', { title: t('Back to the default'), onclick: () => { delete own[a.id]; save(); } }, icon('undo'))));
    }
  };
  // The next keys pressed go to the chip, before anything else in the
  // window sees them; a letter alone, which the terminal wants, is not
  // taken.
  const record = (id, i) => {
    recording = { id, i };
    render();
    if (taking) document.removeEventListener('keydown', taking, true);
    const take = taking = e => {
      if (MODIFIERS.has(e.key)) return;
      e.preventDefault(); e.stopPropagation();
      document.removeEventListener('keydown', take, true);
      taking = null;
      recording = null;
      if (e.key === 'Escape') { render(); return; }
      const k = keyOf(e);
      const plain = !e.ctrlKey && !e.altKey && !e.metaKey && !/^F\d/.test(e.code || '') && !CODES[e.code];
      if (!k || plain) { render(); return; }
      const keys = [...keysOf(id)];
      keys[i] = k;
      setKeys(id, [...new Set(keys)]);
    };
    document.addEventListener('keydown', take, true);
  };
  onChange(render);
  render();
  m.dispose = () => {
    listeners.splice(listeners.indexOf(render), 1);
    if (taking) document.removeEventListener('keydown', taking, true);
  };
  return m;
}

/// The same as a popup under a button; the button again, or a click
/// outside, closes it.
export function keysPanel(anchor) {
  if (openPanel) { openPanel(); return; }
  const m = keysView();
  m.classList.add('menu');
  const close = popup(anchor, m);
  const gone = () => { openPanel = null; m.dispose(); watch.disconnect(); };
  const watch = new MutationObserver(() => { if (!document.body.contains(m)) gone(); });
  watch.observe(document.body, { childList: true });
  openPanel = () => { close(); gone(); };
}

window.addEventListener('storage', e => { if (e.key === STORE) load(); });
load();
