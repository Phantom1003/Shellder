// The terminals' colours: a few well-known palettes, each a background, a
// foreground and the sixteen ANSI colours, chosen in Settings for light
// and for dark terminals ("default" is the window's own two). A palette
// is given to xterm as its theme; the group's body takes its background
// too, so the terminal and the room around it are one.
export const BUILT_IN = [
  { id: 'default', name: 'Default', light: null },
  { id: 'solarized-light', name: 'Solarized Light', light: true, background: '#fdf6e3', foreground: '#657b83',
    ansi: ['#073642', '#dc322f', '#859900', '#b58900', '#268bd2', '#d33682', '#2aa198', '#eee8d5', '#002b36', '#cb4b16', '#586e75', '#657b83', '#839496', '#6c71c4', '#93a1a1', '#fdf6e3'] },
  { id: 'solarized-dark', name: 'Solarized Dark', light: false, background: '#002b36', foreground: '#839496',
    ansi: ['#073642', '#dc322f', '#859900', '#b58900', '#268bd2', '#d33682', '#2aa198', '#eee8d5', '#002b36', '#cb4b16', '#586e75', '#657b83', '#839496', '#6c71c4', '#93a1a1', '#fdf6e3'] },
  { id: 'one-light', name: 'One Light', light: true, background: '#fafafa', foreground: '#383a42',
    ansi: ['#383a42', '#e45649', '#50a14f', '#c18401', '#4078f2', '#a626a4', '#0184bc', '#a0a1a7', '#4f525e', '#e06c75', '#98c379', '#e5c07b', '#61afef', '#c678dd', '#56b6c2', '#ffffff'] },
  { id: 'one-dark', name: 'One Dark', light: false, background: '#282c34', foreground: '#abb2bf',
    ansi: ['#282c34', '#e06c75', '#98c379', '#e5c07b', '#61afef', '#c678dd', '#56b6c2', '#abb2bf', '#5c6370', '#e06c75', '#98c379', '#e5c07b', '#61afef', '#c678dd', '#56b6c2', '#ffffff'] },
  { id: 'github-light', name: 'GitHub Light', light: true, background: '#ffffff', foreground: '#24292f',
    ansi: ['#24292f', '#cf222e', '#116329', '#4d2d00', '#0969da', '#8250df', '#1b7c83', '#6e7781', '#57606a', '#a40e26', '#1a7f37', '#633c01', '#218bff', '#a475f9', '#3192aa', '#8c959f'] },
  { id: 'github-dark', name: 'GitHub Dark', light: false, background: '#0d1117', foreground: '#c9d1d9',
    ansi: ['#484f58', '#ff7b72', '#3fb950', '#d29922', '#58a6ff', '#bc8cff', '#39c5cf', '#b1bac4', '#6e7681', '#ffa198', '#56d364', '#e3b341', '#79c0ff', '#d2a8ff', '#56d4dd', '#f0f6fc'] },
  { id: 'catppuccin-latte', name: 'Catppuccin Latte', light: true, background: '#eff1f5', foreground: '#4c4f69',
    ansi: ['#5c5f77', '#d20f39', '#40a02b', '#df8e1d', '#1e66f5', '#ea76cb', '#179299', '#acb0be', '#6c6f85', '#d20f39', '#40a02b', '#df8e1d', '#1e66f5', '#ea76cb', '#179299', '#bcc0cc'] },
  { id: 'catppuccin-mocha', name: 'Catppuccin Mocha', light: false, background: '#1e1e2e', foreground: '#cdd6f4',
    ansi: ['#45475a', '#f38ba8', '#a6e3a1', '#f9e2af', '#89b4fa', '#f5c2e7', '#94e2d5', '#bac2de', '#585b70', '#f38ba8', '#a6e3a1', '#f9e2af', '#89b4fa', '#f5c2e7', '#94e2d5', '#a6adc8'] },
  { id: 'gruvbox-light', name: 'Gruvbox Light', light: true, background: '#fbf1c7', foreground: '#3c3836',
    ansi: ['#fbf1c7', '#cc241d', '#98971a', '#d79921', '#458588', '#b16286', '#689d6a', '#7c6f64', '#928374', '#9d0006', '#79740e', '#b57614', '#076678', '#8f3f71', '#427b58', '#3c3836'] },
  { id: 'gruvbox-dark', name: 'Gruvbox Dark', light: false, background: '#282828', foreground: '#ebdbb2',
    ansi: ['#282828', '#cc241d', '#98971a', '#d79921', '#458588', '#b16286', '#689d6a', '#a89984', '#928374', '#fb4934', '#b8bb26', '#fabd2f', '#83a598', '#d3869b', '#8ec07c', '#ebdbb2'] },
  { id: 'tokyo-night-day', name: 'Tokyo Night Day', light: true, background: '#e1e2e7', foreground: '#3760bf',
    ansi: ['#e9e9ed', '#f52a65', '#587539', '#8c6c3e', '#2e7de9', '#9854f1', '#007197', '#6172b0', '#a1a6c5', '#f52a65', '#587539', '#8c6c3e', '#2e7de9', '#9854f1', '#007197', '#3760bf'] },
  { id: 'tokyo-night', name: 'Tokyo Night', light: false, background: '#1a1b26', foreground: '#c0caf5',
    ansi: ['#15161e', '#f7768e', '#9ece6a', '#e0af68', '#7aa2f7', '#bb9af7', '#7dcfff', '#a9b1d6', '#414868', '#f7768e', '#9ece6a', '#e0af68', '#7aa2f7', '#bb9af7', '#7dcfff', '#c0caf5'] },
  { id: 'nord', name: 'Nord', light: false, background: '#2e3440', foreground: '#d8dee9',
    ansi: ['#3b4252', '#bf616a', '#a3be8c', '#ebcb8b', '#81a1c1', '#b48ead', '#88c0d0', '#e5e9f0', '#4c566a', '#bf616a', '#a3be8c', '#ebcb8b', '#81a1c1', '#b48ead', '#8fbcbb', '#eceff4'] },
  { id: 'dracula', name: 'Dracula', light: false, background: '#282a36', foreground: '#f8f8f2',
    ansi: ['#21222c', '#ff5555', '#50fa7b', '#f1fa8c', '#bd93f9', '#ff79c6', '#8be9fd', '#f8f8f2', '#6272a4', '#ff6e6e', '#69ff94', '#ffffa5', '#d6acff', '#ff92df', '#a4ffff', '#ffffff'] },
];

/// The built-in palettes and the user's theme files (`setThemeFiles`).
export let THEMES = [...BUILT_IN];

/// The user's theme files, in Ghostty's format (what Ghostty ships and
/// what iTerm2-Color-Schemes exports): `background = 282c34`,
/// `foreground = #abb2bf`, `palette = 0=#282c34` … `palette = 15=…`,
/// `cursor-color`, `selection-background`, `selection-foreground`; a
/// `#` or a blank line is skipped. One becomes a palette named after
/// the file (`file:NAME`), dark or light by its background.
export function parseTheme(name, text) {
  const ansi = [];
  const t = { id: 'file:' + name, name, file: true };
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith('#')) continue;
    const m = /^([\w-]+)\s*=\s*(.*)$/.exec(line);
    if (!m) continue;
    const [, key, val] = m;
    const color = v => { const c = v.trim().replace(/^#?/, '#'); return /^#[0-9a-f]{6}$/i.test(c) ? c.toLowerCase() : null; };
    switch (key) {
      case 'background': t.background = color(val); break;
      case 'foreground': t.foreground = color(val); break;
      case 'cursor-color': t.cursor = color(val); break;
      case 'cursor-text': t.cursorAccent = color(val); break;
      case 'selection-background': t.selectionBackground = color(val); break;
      case 'selection-foreground': t.selectionForeground = color(val); break;
      case 'palette': { const p = /^(\d+)\s*=\s*(.+)$/.exec(val.trim()); if (p && Number(p[1]) < 16) ansi[Number(p[1])] = color(p[2]); break; }
    }
  }
  if (!t.background || !t.foreground) return null;
  for (let i = 0; i < 16; i++) if (!ansi[i]) ansi[i] = null;
  t.ansi = ansi;
  const n = parseInt(t.background.slice(1), 16);
  const lum = 0.2126 * (n >> 16) + 0.7152 * ((n >> 8) & 255) + 0.0722 * (n & 255);
  t.light = lum > 128;
  return t;
}

/// The theme files as the app read them ({name, text}): parsed, after
/// the built-in ones.
export function setThemeFiles(files) {
  THEMES = [...BUILT_IN, ...files.map(f => parseTheme(f.name, f.text)).filter(Boolean)];
}

const ANSI = ['black', 'red', 'green', 'yellow', 'blue', 'magenta', 'cyan', 'white', 'brightBlack', 'brightRed', 'brightGreen', 'brightYellow', 'brightBlue', 'brightMagenta', 'brightCyan', 'brightWhite'];

/// The window's own colours for a light or a dark terminal.
const DEFAULT = isDark => isDark
  ? { background: '#1e1e1e', foreground: '#f2f2f2' }
  : { background: '#ffffff', foreground: '#1d1d1f' };

/// `#rrggbb` with an alpha, as CSS and xterm take it.
export function withAlpha(hex, alpha) {
  if (alpha >= 1 || !/^#[0-9a-f]{6}$/i.test(hex)) return hex;
  const n = parseInt(hex.slice(1), 16);
  return `rgba(${n >> 16}, ${(n >> 8) & 255}, ${n & 255}, ${alpha})`;
}

/// The background of the palette `id` (the default's for `isDark`
/// without one), as `#rrggbb`.
export function paletteBackground(id, isDark) {
  const p = THEMES.find(x => x.id === id && x.ansi) || null;
  return p ? p.background : DEFAULT(isDark).background;
}

/// xterm's theme for the palette `id` (the default's two without one),
/// `isDark` saying which of the default's. A see-through terminal
/// (`alpha` under 1) gets a background that is wholly transparent: the
/// group's body paints the tint under it, once, over the whole box, the
/// scrollbar's lane included, and neither renderer paints a second
/// layer (xterm draws inverse text in the opaque background colour, so
/// nothing is lost). The default and inverse colours keep their hex.
export function xtermTheme(id, isDark, alpha = 1) {
  const p = THEMES.find(x => x.id === id && x.ansi) || null;
  const base = p ? { background: p.background, foreground: p.foreground } : DEFAULT(isDark);
  const dark = p ? !p.light : isDark;
  const theme = {
    background: alpha < 1 ? withAlpha(base.background, 0) : base.background,
    foreground: base.foreground,
    cursor: (p && p.cursor) || base.foreground,
    cursorAccent: (p && p.cursorAccent) || base.background,
    selectionBackground: (p && p.selectionBackground) ? withAlpha(p.selectionBackground, 0.6) : dark ? 'rgba(64,156,255,0.35)' : 'rgba(10,132,255,0.25)',
    selectionInactiveBackground: dark ? 'rgba(64,156,255,0.2)' : 'rgba(10,132,255,0.14)',
    // The overview ruler beside the scrollbar (the search's marks) has a
    // border, white unless told otherwise: none.
    overviewRulerBorder: 'rgba(0, 0, 0, 0)',
    // The scrollbar's slider, a plain grey as macOS draws it.
    scrollbarSliderBackground: dark ? 'rgba(255,255,255,0.45)' : 'rgba(0,0,0,0.4)',
    scrollbarSliderHoverBackground: dark ? 'rgba(255,255,255,0.6)' : 'rgba(0,0,0,0.55)',
    scrollbarSliderActiveBackground: dark ? 'rgba(255,255,255,0.7)' : 'rgba(0,0,0,0.65)',
  };
  if (p && p.selectionForeground) theme.selectionForeground = p.selectionForeground;
  if (p) ANSI.forEach((k, i) => { if (p.ansi[i]) theme[k] = p.ansi[i]; });
  return theme;
}

/// Whether the terminal drawn with `id` is dark: the palette's own
/// darkness, or `isDark` for the default.
export const themeIsDark = (id, isDark) => { const p = THEMES.find(x => x.id === id && x.ansi); return p ? !p.light : isDark; };
