// The terminals' settings, the pages of the Console's settings window
// (consoleprefs.js, opened from the gear in a Console's bar): the letters,
// the cursor, the scrollback, the colours (the built-in palettes and the
// user's theme files), how see-through; the manners; the quick
// terminal's key and host; and the Theme page, the palettes beside the
// one chosen, colour by colour. Every change goes to the app (`set_setting`)
// and comes back in the state, which every Console window applies to its
// terminals.
import { invoke, t, el, icon, LOCAL, toast, dialog, menuAt, append } from './shellder.js';
import { THEMES, resolveTheme, editThemeText, themeText, setThemeFiles, withAlpha } from './themes.js';

const MAC = document.documentElement.classList.contains('mac') || /^Mac/.test(navigator.platform);

/// A card of rows. A row is its cells, with `tip` for what a hover of it
/// explains and `sub` when it belongs to the row above (its label
/// indented), the way the main window draws a host's reconnect rule.
export function section(title, rows) {
  return el('div', el('div.card-title', title), el('div.card', rows.filter(Boolean).map(r => el('div.crow', { class: r.sub ? 'sub' : '', title: r.tip }, r))));
}

export function row(cells, { tip, sub } = {}) { return Object.assign(cells, { tip, sub }); }

function toggle(label, key, value, disabled, opts) {
  return row([el('span.k', label), el('label.switch', el('input', { type: 'checkbox', checked: value, disabled, onchange: e => invoke('set_setting', { key, value: e.target.checked }) }), el('span.knob'))], opts);
}

/// A setting typed in: a text or a number, sent when it is left or
/// Return is pressed (a number within its bounds, `null` for an empty
/// field, which puts the default back).
function field(label, key, value, { number = false, min, max, step, placeholder = '', width = 180, unit = null, tip, sub } = {}) {
  const input = el('input.text', { type: number ? 'number' : 'text', value: value ?? '', placeholder, spellcheck: false, style: `width:${width}px; text-align:${number ? 'right' : 'left'}` });
  if (number) { if (min !== undefined) input.min = min; if (max !== undefined) input.max = max; if (step !== undefined) input.step = step; }
  const send = () => {
    let v = input.value.trim();
    if (number) { v = v === '' ? null : Math.min(max ?? Infinity, Math.max(min ?? -Infinity, Number(v))); if (v !== null && Number.isNaN(v)) return; if (v !== null) input.value = v; }
    if (String(v ?? '') === String(value ?? '')) return;
    invoke('set_setting', { key, value: v });
  };
  input.onchange = send;
  input.onkeydown = e => { if (e.key === 'Enter') { e.preventDefault(); input.blur(); } };
  return row([el('span.k', label), el('span.v', input, unit ? el('span.secondary', ' ' + unit) : null)], { tip, sub });
}

function choice(label, key, value, options, opts) {
  return row([el('span.k', label), el('select', { onchange: e => invoke('set_setting', { key, value: e.target.value }) },
    options.map(([id, name]) => el('option', { value: id, selected: value === id }, name)))], opts);
}

/// The terminals' look (ui/console.js): their letters, cursor,
/// scrollback, colours, see-through and padding.
export function lookSection(s) {
  const st = s.settings, ts = st.terminal;
  const themes = THEMES.map(x => [x.id, x.id === 'default' ? t('Default') : x.file ? `${x.name} (${t('file')})` : x.name]);
  return section(t('Terminal'), [
    field(t('Font'), 'terminal.font', ts.font, { placeholder: 'SF Mono, Menlo, …', tip: t('The family. The size is ⌘+ and ⌘− in the Console itself.') }),
    field(t('Line height'), 'terminal.lineHeight', ts.lineHeight, { number: true, min: 1, max: 2, step: 0.05, width: 70 }),
    choice(t('Cursor'), 'terminal.cursor', ts.cursor, [['block', t('Block')], ['bar', t('Bar')], ['underline', t('Underline')]]),
    toggle(t('Blinking cursor'), 'terminal.cursorBlink', ts.cursorBlink, false, { sub: true }),
    field(t('Scrollback'), 'terminal.scrollback', ts.scrollback, { number: true, min: 100, max: 1000000, step: 1000, width: 90, unit: t('lines') }),
    choice(t('Colours, light'), 'terminal.themeLight', ts.themeLight, themes),
    choice(t('Colours, dark'), 'terminal.themeDark', ts.themeDark, themes),
    field(t('Opacity'), 'terminal.opacity', Math.round(ts.opacity * 100), { number: true, min: 30, max: 100, step: 5, width: 70, unit: '%' }).map((e, i) => i === 1 ? wrapOpacity(e) : e),
    st.platform === 'macos' ? toggle(t('Blur what is behind (macOS)'), 'terminal.blur', ts.blur, ts.opacity >= 1, { sub: true, tip: t('A blur is redrawn with each frame. Leave it off on a laptop or when typing feels slow.') }) : null,
    field(t('Padding'), 'terminal.padding', ts.padding, { number: true, min: 0, max: 40, step: 1, width: 70, unit: 'px' }),
  ]);
}

/// The terminals' manners: the clipboard, the questions asked, the
/// notifications, the shell integration and the renderer.
export function mannersSection(s) {
  const ts = s.settings.terminal;
  return section(t('Manners'), [
    toggle(t('Copy on select'), 'terminal.copyOnSelect', ts.copyOnSelect),
    toggle(t('Ask before pasting several lines'), 'terminal.pasteProtection', ts.pasteProtection),
    toggle(t('Ask before closing a tab with a command running'), 'terminal.confirmClose', ts.confirmClose),
    toggle(t('Notifications'), 'terminal.notifications', ts.notifications, false, { tip: t('A tab out of sight finishes a command, waits for input or rings the bell.') }),
    toggle(t('Shell integration'), 'terminal.shellIntegration', ts.shellIntegration, false, { tip: t('A shell opened from the Console (bash, zsh or fish) marks its prompts and reports its directory. The Console can then jump between prompts (⌘↑, ⌘↓), open a new tab in the same directory, name the tab after it, tell when a command is done and ask before closing a tab with one running. Shells of tmux and screen sessions started earlier are not covered.') }),
    toggle(t('Draw with WebGL'), 'terminal.webgl', ts.webgl, false, { tip: t('Faster with much output. The letters are less crisp.') }),
  ]);
}

/// The opacity field sends a fraction.
function wrapOpacity(v) {
  const input = v.querySelector('input');
  input.onchange = () => { const n = input.value.trim(); invoke('set_setting', { key: 'terminal.opacity', value: n === '' ? null : Math.min(100, Math.max(30, Number(n))) / 100 }); };
  return v;
}

// MARK: the quick terminal

const MODS = { Control: MAC ? '⌃' : 'Ctrl', Alt: MAC ? '⌥' : 'Alt', Shift: MAC ? '⇧' : 'Shift', Super: MAC ? '⌘' : 'Super' };
const CODE_GLYPHS = { ArrowUp: '↑', ArrowDown: '↓', ArrowLeft: '←', ArrowRight: '→', Enter: MAC ? '↩' : 'Enter', Space: MAC ? '␣' : 'Space', Backquote: '`', Minus: '-', Equal: '=', BracketLeft: '[', BracketRight: ']', Backslash: '\\', Semicolon: ';', Quote: "'", Comma: ',', Period: '.', Slash: '/', Escape: MAC ? '⎋' : 'Esc', Tab: MAC ? '⇥' : 'Tab' };

/// A key as the global shortcut plugin writes it ("Super+Shift+KeyK"),
/// shown as ⇧⌘K (Super+Shift+K elsewhere).
function describeKey(key) {
  if (!key) return '';
  const parts = key.split('+');
  const code = parts.pop();
  const m = /^(Key|Digit)(\w)$/.exec(code);
  const k = m ? m[2] : CODE_GLYPHS[code] || code;
  return MAC ? parts.map(p => MODS[p] || p).join('') + k : [...parts.map(p => MODS[p] || p), k].join('+');
}

/// The key that drops the quick terminal from the top of the screen in
/// any application, recorded on a click: the next keys pressed (a
/// modifier with them), Esc leaves it, ⌫ takes it off; and whose host
/// its shell is on.
export function quickTerminalSection(s) {
  const st = s.settings;
  const btn = el('button', { title: t('Click, then press the keys to use') }, st.quickConsoleKey ? describeKey(st.quickConsoleKey) : t('None'));
  let taking = null;
  btn.onclick = () => {
    if (taking) return;
    btn.textContent = t('Press the keys…');
    btn.classList.add('primary');
    const done = () => { document.removeEventListener('keydown', taking, true); taking = null; btn.classList.remove('primary'); };
    taking = e => {
      if (['Shift', 'Control', 'Alt', 'Meta', 'CapsLock', 'Fn', 'Dead'].includes(e.key)) return;
      e.preventDefault(); e.stopPropagation();
      done();
      if (e.key === 'Escape') { btn.textContent = st.quickConsoleKey ? describeKey(st.quickConsoleKey) : t('None'); return; }
      if (e.key === 'Backspace' || e.key === 'Delete') { invoke('set_setting', { key: 'quickConsoleKey', value: null }); return; }
      const mods = [e.ctrlKey && 'Control', e.altKey && 'Alt', e.shiftKey && 'Shift', e.metaKey && 'Super'].filter(Boolean);
      if (!mods.length || !e.code) { btn.textContent = st.quickConsoleKey ? describeKey(st.quickConsoleKey) : t('None'); return; }
      invoke('set_setting', { key: 'quickConsoleKey', value: [...mods, e.code].join('+') });
    };
    document.addEventListener('keydown', taking, true);
  };
  const hosts = [[LOCAL, t('Local')], ...(s.hosts || []).map(h => [h.alias, h.alias])];
  return section(t('Quick terminal'), [
    row([el('span.k', t('Key')), el('span.v', btn, st.quickConsoleKey ? el('button.borderless', { title: t('Take this key off'), onclick: () => invoke('set_setting', { key: 'quickConsoleKey', value: null }) }, icon('close')) : null)],
      { tip: t('The key works in any application. A shell on the host drops from the top of the pointer\'s screen, over all other windows, and goes away with the key again, with Esc at its prompt or when the keyboard moves elsewhere. The key needs a modifier, and one taken by the system or another application cannot be had.') }),
    choice(t('Host'), 'quickConsoleHost', st.quickConsoleHost || LOCAL, hosts, { sub: true }),
  ]);
}


// MARK: the Theme page

const SLOT_STORE = 'consoleprefs-themeslot';
/// The side being chosen for, 'light' or 'dark' terminals: the system's
/// look at first, then as last picked.
let slot = (() => { try { return localStorage.getItem(SLOT_STORE); } catch { return null; } })();
if (slot !== 'light' && slot !== 'dark') slot = matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
let listScroll = 0;
let lastWrite = 0;
/// When this page last wrote a theme file: the window ignores the
/// change event that follows its own write (the list already shows it).
export const lastThemeWrite = () => lastWrite;

const ANSI_LABELS = () => [t('Black'), t('Red'), t('Green'), t('Yellow'), t('Blue'), t('Magenta'), t('Cyan'), t('White'), t('Bright black'), t('Bright red'), t('Bright green'), t('Bright yellow'), t('Bright blue'), t('Bright magenta'), t('Bright cyan'), t('Bright white')];
/// What a colour the palette does not set falls back on.
const FROM = { window: () => t("the window's"), foreground: () => t('the foreground'), background: () => t('the background'), accent: () => t('the accent'), text: () => t('unchanged'), xterm: () => t("xterm's") };
const themeName = th => th.id === 'default' ? t('Default') : th.name;
const slotKey = () => slot === 'dark' ? 'terminal.themeDark' : 'terminal.themeLight';

/// A palette in small: its background with its foreground and six
/// colours as bars.
function swatch(r) {
  return el('span.sw', { style: `background:${r.background.value}` },
    [r.foreground, r.ansi[1], r.ansi[2], r.ansi[3], r.ansi[4], r.ansi[5], r.ansi[6]].map(c => el('i', { style: `background:${c.value}` })));
}

/// A terminal's worth of the palette: a prompt, an `ls`, a diff, a
/// warning and an error, a selection and the cursor, then its sixteen
/// colours in a strip.
function sample(r) {
  const a = i => r.ansi[i].value;
  const span = (color, text) => el('span', { style: `color:${color}` }, text);
  const prompt = () => [span(a(10), 'user@host'), ':', span(a(12), '~/project'), ' $ '];
  const selStyle = `background:${withAlpha(r.selectionBackground.value, r.selectionBackground.from ? (r.dark ? 0.35 : 0.25) : 0.6)}` + (r.selectionForeground.value ? `;color:${r.selectionForeground.value}` : '');
  return el('div.td-sample', { style: `background:${r.background.value}; color:${r.foreground.value}` },
    el('div', prompt(), 'ls -l'),
    el('div', span(a(4), 'docs   src   ui'), '   ', span(a(2), 'build.sh'), '   README.md   ', span(a(6), 'latest.json')),
    el('div', prompt(), 'git diff --stat'),
    el('div', ' ui/console.js | 12 ', span(a(2), '++++++++'), span(a(1), '----')),
    el('div', span(a(3), 'warning'), ': ', el('span', { style: selStyle }, 'selected text'), ' and ', span(a(5), 'magenta')),
    el('div', span(a(9), 'error'), ': ', span(a(14), 'cyan'), ', ', span(a(13), 'magenta'), ', ', span(a(11), 'yellow'), ', ', span(a(8), 'dim')),
    el('div', prompt(), el('span.cursor', { style: `background:${r.cursor.value}; color:${r.cursorAccent.value}` }, 'a')),
    el('div.strip', r.ansi.map((c, i) => el('i', { title: ANSI_LABELS()[i], style: `background:${c.value}` }))));
}

/// The Theme page: the palettes on the left (the window's own colours,
/// the built-in ones, the user's theme files), each with a swatch, the
/// one chosen for the side marked; a click chooses the one clicked
/// for the side picked at the top (light or dark terminals, the group's
/// button in a Console says which a terminal is) and the right shows it:
/// a sample terminal in its colours and every colour it sets, with what
/// an unset one falls back on. A colour is changed with its well; a
/// theme file takes the change (its line replaced, the rest of the
/// file kept, the terminals following), a built-in palette or the
/// default is copied to a theme file first, on a question. A theme file
/// can be renamed and deleted (the pencil and the bin beside Duplicate,
/// or a row's menu, a right click, which also chooses a palette for a
/// side); a built-in palette cannot.
export function themePage(s, files) {
  const ts = s.settings.terminal;
  const isDark = slot === 'dark';
  const chosen = isDark ? ts.themeDark : ts.themeLight;
  const id = THEMES.some(x => x.id === chosen) ? chosen : 'default';
  const th = THEMES.find(x => x.id === id);
  const file = th.file ? files.find(f => f.name === th.name) : null;
  /// A theme file gone: the sides that had it get the default.
  const unchoose = async x => {
    if (ts.themeLight === x.id) await invoke('set_setting', { key: 'terminal.themeLight', value: 'default' });
    if (ts.themeDark === x.id) await invoke('set_setting', { key: 'terminal.themeDark', value: 'default' });
  };
  const duplicate = async x => {
    const a = await dialog({ title: t('Duplicate the palette'), message: t('A copy as a theme file in the themes folder, yours to change colour by colour.'), input: true, preset: t('{name} copy', { name: themeName(x) }), action: t('Duplicate') });
    if (!a.ok || !a.value) return;
    const name = a.value;
    if (/[/\\]/.test(name) || name.startsWith('.')) { toast(t('Not a file name: {name}', { name }), true); return; }
    if (files.some(f => f.name.toLowerCase() === name.toLowerCase())) { toast(t('There is a theme file named {name} already', { name }), true); return; }
    // The list knows the file before the app says so: the state change
    // that follows draws the copy, not a fallback.
    const text = themeText(resolveTheme(x.id, isDark));
    try {
      lastWrite = Date.now();
      await invoke('theme_write', { name, text });
      files.push({ name, text });
      files.sort((a, b) => a.name.toLowerCase() < b.name.toLowerCase() ? -1 : a.name.toLowerCase() > b.name.toLowerCase() ? 1 : 0);
      setThemeFiles(files);
      await invoke('set_setting', { key: slotKey(), value: 'file:' + name });
    } catch (e) { toast(String(e), true); }
  };
  /// A theme file gets another name; the sides that had it follow.
  const rename = async x => {
    if (!x.file) return;
    const a = await dialog({ title: t('Rename the theme file'), input: true, preset: x.name, action: t('Rename') });
    if (!a.ok || !a.value || a.value === x.name) return;
    const to = a.value;
    if (/[/\\]/.test(to) || to.startsWith('.')) { toast(t('Not a file name: {name}', { name: to }), true); return; }
    if (files.some(f => f.name.toLowerCase() === to.toLowerCase())) { toast(t('There is a theme file named {name} already', { name: to }), true); return; }
    try {
      lastWrite = Date.now();
      await invoke('theme_rename', { name: x.name, to });
      const f = files.find(y => y.name === x.name); f.name = to;
      files.sort((a, b) => a.name.toLowerCase() < b.name.toLowerCase() ? -1 : a.name.toLowerCase() > b.name.toLowerCase() ? 1 : 0);
      setThemeFiles(files);
      if (ts.themeLight === x.id) await invoke('set_setting', { key: 'terminal.themeLight', value: 'file:' + to });
      if (ts.themeDark === x.id) await invoke('set_setting', { key: 'terminal.themeDark', value: 'file:' + to });
      // Chosen for no side, nothing in the state changes: drawn here.
      if (ts.themeLight !== x.id && ts.themeDark !== x.id) redraw();
    } catch (e) { toast(String(e), true); }
  };
  /// A theme file is deleted. Chosen for no side, nothing in the state
  /// changes: drawn here.
  const remove = async x => {
    if (!x.file) return;
    const a = await dialog({ title: t('Delete the theme file {name}?', { name: x.name }), message: t('The terminals using it get the default colours.'), action: t('Delete'), destructive: true });
    if (!a.ok) return;
    try {
      lastWrite = Date.now();
      await invoke('theme_delete', { name: x.name });
      files.splice(files.findIndex(f => f.name === x.name), 1);
      setThemeFiles(files);
      await unchoose(x);
      if (ts.themeLight !== x.id && ts.themeDark !== x.id) redraw();
    } catch (e) { toast(String(e), true); }
  };

  // The left: the side, the list, the folder.
  const seg = el('div.seg', { title: t("The colours of a light and of a dark terminal are chosen apart; the group's sun-and-moon button in a Console says which a terminal is.") },
    ['light', 'dark'].map(x => el('button', { class: slot === x ? 'active' : '', onclick: () => { if (slot === x) return; slot = x; try { localStorage.setItem(SLOT_STORE, x); } catch {} redraw(); } },
      icon(x === 'light' ? 'sun' : 'moon'), x === 'light' ? t('Light') : t('Dark'))));
  const list = el('div.tl-list');
  const items = {};
  const item = x => {
    const r = resolveTheme(x.id, isDark);
    const e = el('div.tl-item', { class: x.id === id ? 'active' : '', onclick: () => { if (x.id !== id) invoke('set_setting', { key: slotKey(), value: x.id }); },
      oncontextmenu: ev => { ev.preventDefault(); menuAt(ev.clientX, ev.clientY, [
        { title: t('Use for light terminals'), icon: 'sun', disabled: ts.themeLight === x.id, run: () => invoke('set_setting', { key: 'terminal.themeLight', value: x.id }) },
        { title: t('Use for dark terminals'), icon: 'moon', disabled: ts.themeDark === x.id, run: () => invoke('set_setting', { key: 'terminal.themeDark', value: x.id }) },
        { sep: true },
        { title: t('Duplicate…'), icon: 'duplicate', run: () => duplicate(x) },
        x.file ? { title: t('Rename…'), icon: 'pencil', run: () => rename(x) } : null,
        x.file ? { title: t('Delete the theme file'), icon: 'trash', run: () => remove(x) } : null,
      ]); } },
      swatch(r), el('span.name', themeName(x)));
    items[x.id] = e;
    return e;
  };
  const builtIn = THEMES.filter(x => x.id !== 'default' && !x.file), own = THEMES.filter(x => x.file);
  append(list, [item(THEMES[0]), el('div.tl-group', t('Built in')), builtIn.map(item),
    own.length ? [el('div.tl-group', t('Theme files')), own.map(item)] : null]);
  list.onscroll = () => { listScroll = list.scrollTop; };
  const openFolder = async () => { try { const dir = await invoke('themes_dir'); await invoke('open_path', { path: dir, editor: false }); } catch (e) { toast(String(e), true); } };
  const left = el('div.tl', seg, list,
    el('div.tl-foot', el('button', { onclick: openFolder, title: t('One file per theme, in Ghostty\'s format: `background = 1d1f21`, `foreground = c5c8c6`, `palette = 0=#1d1f21` up to 15, with `cursor-color`, `cursor-text`, `selection-background` and `selection-foreground` when present. Ghostty\'s themes and the Ghostty exports of iTerm2-Color-Schemes go in as they are. The file\'s name is the theme\'s.') }, icon('folder'), ' ', t('Open the themes folder'))));

  // The right: the palette chosen, its sample and its colours.
  let r = resolveTheme(id, isDark);
  const kind = id === 'default' ? t("The window's own colours, light or dark as the terminal is; xterm's sixteen.")
    : th.file ? t('A theme file in the themes folder. A colour changed here is written to it.')
    : t('Built in. Duplicate it to change its colours.');
  const head = el('div.td-head', el('div.text', el('div.title', themeName(th)), el('div.caption', kind)),
    el('div.acts', el('button', { onclick: () => duplicate(th), title: t('Duplicate') }, icon('duplicate')),
      th.file ? el('button', { onclick: () => rename(th), title: t('Rename the theme file') }, icon('pencil')) : null,
      th.file ? el('button', { onclick: () => remove(th), title: t('Delete the theme file') }, icon('trash')) : null));
  let sampleEl = sample(r);

  // A colour's row: its name, its value or what it falls back on, its
  // well, and for a file's own colour an × that takes it out again.
  const rows = [];
  let timer = null;
  const write = () => {
    clearTimeout(timer);
    timer = setTimeout(() => { lastWrite = Date.now(); invoke('theme_write', { name: file.name, text: file.text }).catch(e => toast(String(e), true)); }, 250);
  };
  /// The file gets `key` (its `index` for the palette) set to `value`,
  /// null taking it out; the page follows at once, the file a moment
  /// later, the terminals with it.
  const edit = (key, index, value) => {
    file.text = editThemeText(file.text, key, index, value);
    setThemeFiles(files);
    r = resolveTheme(id, isDark);
    const fresh = sample(r); sampleEl.replaceWith(fresh); sampleEl = fresh;
    for (const u of rows) u(r);
    const it = items[id]; if (it) it.firstChild.replaceWith(swatch(r));
    write();
  };
  const colourRow = (label, idx, pick, key, index, required = false) => {
    const from = el('span.from');
    const well = el('input', { type: 'color' });
    const reset = required ? null : el('button.borderless.reset', { title: t('Take this colour out of the file: it falls back') }, icon('close'));
    const update = rr => {
      const c = pick(rr);
      from.textContent = c.from ? FROM[c.from]() : c.value;
      from.classList.toggle('fb', !!c.from);
      if (document.activeElement !== well) well.value = c.value || rr.foreground.value;
      if (reset) reset.classList.toggle('hidden', !!c.from);
    };
    update(r);
    if (file) {
      well.oninput = () => edit(key, index, well.value);
      if (reset) reset.onclick = () => edit(key, index, null);
    } else {
      // Not a file: the well is the question, the picker is not opened.
      well.onclick = e => { e.preventDefault(); duplicate(th); };
      well.title = t('Duplicate the palette to change its colours');
      if (reset) reset.classList.add('hidden');
    }
    rows.push(update);
    return el('div.trow', el('span.k', idx !== null ? el('span.idx', String(idx)) : null, label), from, well, reset);
  };
  const colours = el('div.card.td-colours',
    colourRow(t('Background colour'), null, x => x.background, 'background', null, true),
    colourRow(t('Foreground colour'), null, x => x.foreground, 'foreground', null, true),
    colourRow(t('Cursor'), null, x => x.cursor, 'cursor-color', null),
    colourRow(t('Text under the cursor'), null, x => x.cursorAccent, 'cursor-text', null),
    colourRow(t('Selection'), null, x => x.selectionBackground, 'selection-background', null),
    colourRow(t('Selected text'), null, x => x.selectionForeground, 'selection-foreground', null));
  const ansi = el('div.card.td-colours', r.ansi.map((c, i) => colourRow(ANSI_LABELS()[i], i, x => x.ansi[i], 'palette', i)));
  const right = el('div.td', head, sampleEl, colours,
    el('div.td-sub', t('The sixteen colours a program names (palette 0 to 15 in the file): the normal eight and the bright eight.')), ansi);

  const page = el('div.themepage', left, right);
  const redraw = () => page.replaceWith(themePage(s, files));
  requestAnimationFrame(() => { list.scrollTop = listScroll; });
  return page;
}
