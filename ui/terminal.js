// A shell on a host, in a window of its own: xterm.js draws what ssh
// writes to its screen pty and sends back what is typed. The questions
// ssh asks (password, host key) never show here: they go down the
// controlling terminal, answered from the keychain or the prompt window.
import { invoke, listen, loadLanguage, t, connectState, commonKeys } from './shellder.js';

const host = new URLSearchParams(location.search).get('host');
const box = document.getElementById('term');
const dark = window.matchMedia('(prefers-color-scheme: dark)');

function theme() {
  return dark.matches
    ? { background: '#1e1e1e', foreground: '#f2f2f2', cursor: '#f2f2f2', selectionBackground: 'rgba(64,156,255,0.35)' }
    : { background: '#ffffff', foreground: '#1d1d1f', cursor: '#1d1d1f', selectionBackground: 'rgba(10,132,255,0.25)' };
}

const term = new Terminal({
  fontFamily: 'ui-monospace, "SF Mono", Menlo, Consolas, monospace',
  fontSize: 13,
  lineHeight: 1.15,
  cursorBlink: true,
  scrollback: 10000,
  macOptionIsMeta: true,
  theme: theme(),
});
const fit = new FitAddon.FitAddon();
term.loadAddon(fit);

/// The bytes of a base64 string, as ssh wrote them.
function decode(b64) {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

let ended = false;

async function init() {
  const s = await connectState();
  await loadLanguage(s.settings.language);
  document.title = t('{host} — Shell', { host });
  document.getElementById('title').textContent = document.title;
  term.open(box);
  fit.fit();
  dark.addEventListener('change', () => { term.options.theme = theme(); });
  await listen('term-data', b64 => term.write(decode(b64)));
  await listen('term-exit', code => {
    ended = true;
    term.write(`\r\n\x1b[2m[${t('The connection to {host} is closed (exit status {code}). Press Return to connect again, or close the window.', { host, code })}]\x1b[0m\r\n`);
  });
  term.onData(async data => {
    if (ended) {
      if (data === '\r') { ended = false; term.reset(); await open(); }
      return;
    }
    invoke('term_write', { data }).catch(() => {});
  });
  // Binary input (a paste that is not text): bytes as a latin1 string.
  term.onBinary(data => { if (!ended) invoke('term_write', { data }).catch(() => {}); });
  term.onResize(({ cols, rows }) => { if (!ended) invoke('term_resize', { cols, rows }).catch(() => {}); });
  window.addEventListener('resize', () => fit.fit());
  // ⌘C copies a selection (otherwise it is the interrupt it always was),
  // ⌘V pastes, ⌘K clears, ⌘A selects all; any other ⌘ key is the
  // window's (⌘W, ⌘,, ⌘Q) and goes past the terminal to the page.
  term.attachCustomKeyEventHandler(e => {
    if (e.type !== 'keydown' || !e.metaKey) return true;
    if (e.key === 'c' && term.hasSelection()) { navigator.clipboard.writeText(term.getSelection()); return false; }
    if (e.key === 'v') { navigator.clipboard.readText().then(text => { if (text) term.paste(text); }); return false; }
    if (e.key === 'k') { term.clear(); return false; }
    if (e.key === 'a') { term.selectAll(); return false; }
    return false;
  });
  commonKeys();
  await open();
  term.focus();
}

async function open() {
  term.write(`\x1b[2m${t('Connecting to {host}…', { host })}\x1b[0m\r\n`);
  try {
    await invoke('term_open', { host, cols: term.cols, rows: term.rows });
  } catch (e) {
    ended = true;
    term.write(`\r\n\x1b[31m${String(e)}\x1b[0m\r\n`);
  }
}

init();
