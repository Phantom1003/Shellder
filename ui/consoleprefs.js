// The Console's settings window (the gear in a Console's bar): the
// terminals' look and manners, the quick terminal, the theme files and
// the keys. The terminal sections are termprefs.js's, the keys keys.js's
// (kept on this machine, shared by every window); drawn again with every
// change of the state, except while a field is being typed in.
import { invoke, startPage, pageReady, t, el, clear, watchState, commonKeys, currentState } from './shellder.js';
import { setThemeFiles } from './themes.js';
import { terminalSection, quickTerminalSection, themesSection, section } from './termprefs.js';
import * as keys from './keys.js';

const form = document.getElementById('form');
let files = [];
let keysEl = null;

function render(s) {
  if (form.contains(document.activeElement) && document.activeElement.tagName === 'INPUT' && document.activeElement.type !== 'checkbox') return;
  if (keysEl) keysEl.dispose();
  clear(form);
  form.append(terminalSection(s), quickTerminalSection(s), themesSection(files));
  keysEl = keys.keysView();
  form.append(el('div', el('div.card-title', t('Keys')), el('div.card.keyscard', keysEl)));
}

async function init() {
  await startPage();
  try { files = await invoke('terminal_themes'); setThemeFiles(files); } catch {}
  watchState(render);
  pageReady();
  commonKeys();
  // The theme files are read again when the window comes back.
  window.addEventListener('focus', async () => {
    try { const f = await invoke('terminal_themes'); if (JSON.stringify(f) !== JSON.stringify(files)) { files = f; setThemeFiles(files); render(currentState()); } } catch {}
  });
}
init();
