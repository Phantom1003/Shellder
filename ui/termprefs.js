// The terminals' settings, the pages of the Console's settings window
// (consoleprefs.js, opened from the gear in a Console's bar): the letters,
// the cursor, the scrollback, the colours (the built-in palettes and the
// user's theme files), how see-through; the manners; and the quick
// terminal's key and host. Every change goes to the app (`set_setting`)
// and comes back in the state, which every Console window applies to its
// terminals.
import { invoke, t, el, icon, LOCAL, toast } from './shellder.js';
import { THEMES } from './themes.js';

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


/// The user's theme files: where they go, the format, and the ones found.
export function themesSection(files) {
  const names = THEMES.filter(x => x.file).map(x => x.name);
  return section(t('Theme files'), [
    row([el('span.k', names.length ? t('{n} theme files', { n: names.length }) : t('No theme file yet')),
      el('button', { onclick: async () => { try { const dir = await invoke('themes_dir'); await invoke('open_path', { path: dir, editor: false }); } catch (e) { toast(String(e), true); } } }, icon('folder'), ' ', t('Open the themes folder'))],
      { tip: t('One file per theme, in Ghostty\'s format: `background = 1d1f21`, `foreground = c5c8c6`, `palette = 0=#1d1f21` up to 15, with `cursor-color`, `selection-background` and `selection-foreground` when present. Ghostty\'s themes and the Ghostty exports of iTerm2-Color-Schemes go in as they are. The file\'s name is the theme\'s. New files appear in the colour lists the next time this window opens.') }),
    names.length ? row([el('span.k'), el('span.v', names.join('\n'))], { sub: true }) : null,
  ]);
}
