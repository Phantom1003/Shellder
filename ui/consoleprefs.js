// The Console's settings window (the gear in a Console's bar), in pages
// picked from the bar at its top: the terminals' look, their manners, the
// quick terminal, the theme files and the keys. The terminal pages are
// termprefs.js's, the keys keys.js's (kept on this machine, shared by
// every window). The page shown is drawn again with every change of the
// state, except while a field is being typed in, and remembered for the
// next time the window opens.
import { invoke, startPage, pageReady, t, el, clear, watchState, commonKeys, currentState } from './shellder.js';
import { setThemeFiles } from './themes.js';
import { lookSection, mannersSection, quickTerminalSection, themesSection } from './termprefs.js';
import * as keys from './keys.js';

const STORE = 'consoleprefs-page';
const PAGES = [
  { id: 'look', title: () => t('Terminal'), draw: s => lookSection(s) },
  { id: 'manners', title: () => t('Manners'), draw: s => mannersSection(s) },
  { id: 'quick', title: () => t('Quick terminal'), draw: s => quickTerminalSection(s) },
  { id: 'themes', title: () => t('Theme files'), draw: () => themesSection(files) },
  { id: 'keys', title: () => t('Keys'), draw: () => { keysEl = keys.keysView(); return el('div', el('div.card.keyscard', keysEl)); } },
];
const bar = document.getElementById('pages');
const form = document.getElementById('form');
let files = [];
let keysEl = null;
let page = (() => { try { return localStorage.getItem(STORE); } catch { return null; } })();
if (!PAGES.some(p => p.id === page)) page = PAGES[0].id;

function show(id) {
  page = id;
  try { localStorage.setItem(STORE, id); } catch {}
  for (const b of bar.children) b.classList.toggle('active', b.dataset.page === id);
  form.scrollTop = 0;
  render(currentState(), true);
}

function render(s, fresh = false) {
  if (!fresh && form.contains(document.activeElement) && document.activeElement.tagName === 'INPUT' && document.activeElement.type !== 'checkbox') return;
  if (keysEl) { keysEl.dispose(); keysEl = null; }
  clear(form);
  form.append(PAGES.find(p => p.id === page).draw(s));
}

async function init() {
  await startPage();
  for (const p of PAGES) bar.append(el('button', { class: p.id === page ? 'active' : '', 'data-page': p.id, onclick: () => show(p.id) }, p.title()));
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
