// The Settings window: the app's own settings in cards (the rows and
// switches are termprefs.js's, the reconnect rule's reconnect.js's). The
// page is drawn again with every change of the state, except while a
// field is being typed in; a change the app refuses is told here.
import { invoke, startPage, pageReady, t, el, clear, icon, abbreviateHome, watchState, commonKeys, setTitle } from './shellder.js';
import { ruleRows, ruleSummary } from './reconnect.js';
import { section, row, toggle, setSetting, sparingFields } from './termprefs.js';

const LANGUAGES = [['en', 'English'], ['zh-Hans', '简体中文']];
const form = document.getElementById('form');

function pathRow(label, path, home, editor) {
  return [el('span.k', label), el('span.v.selectable', abbreviateHome(path, home)),
    el('button.borderless', { title: t('Open'), onclick: () => invoke('open_path', { path, editor }) }, icon('link'))];
}

const render = sparingFields(form, s => {
  clear(form);
  const st = s.settings, home = s.paths.home;
  form.append(section(t('Startup'), [
    toggle(t('Start at login'), 'startAtLogin', st.startAtLogin),
    toggle(t('Start silently'), 'silentLaunch', st.silentLaunch, !st.keepInBackground || !st.startAtLogin, { sub: true }),
  ]));
  form.append(section(t('Background'), [
    toggle(t('Run in background'), 'keepInBackground', st.keepInBackground),
    toggle(t('Show in menu bar'), 'showMenuBarIcon', st.showMenuBarIcon, !st.keepInBackground, { sub: true }),
    // macOS tints the picture itself; a Linux panel shows it as drawn.
    st.platform !== 'macos' ? toggle(t('Invert menu bar icon'), 'invertMenuBarIcon', st.invertMenuBarIcon, !st.keepInBackground, { sub: true }) : null,
  ]));
  form.append(section(t('Appearance'), [
    [el('span.k', t('Language')), el('select', { onchange: e => setSetting('language', e.target.value) },
      LANGUAGES.map(([id, name]) => el('option', { value: id, selected: st.language === id }, name)))],
    st.language !== st.launchedLanguage ? [el('span.k'), el('button', { onclick: () => invoke('relaunch') }, t('Relaunch to apply'))] : null,
  ]));
  form.append(section(t('Keychain'), [
    [el('span.k', t('Vault')), s.keychainRefusal ? el('span.orange', { style: 'text-align:right' }, s.keychainRefusal) : el('span.secondary', t('readable'))],
    row([el('button', { onclick: () => invoke('request_keychain_access') }, t('Ask the keychain for access'))],
      { tip: t('Secrets live in one keychain item. If the keychain was not allowed to open it, nothing is lost. Ask again here.') }),
  ]));
  form.append(reconnectSection(s));
  form.append(section(t('Files'), [
    pathRow(t('ssh config'), s.paths.sshConfig, home, true),
    s.configFiles.length > 1 ? [el('span.k', t('Included')), el('span.v', s.configFiles.slice(1).map(f => abbreviateHome(f, home)).join('\n'))] : null,
    pathRow(t('Log'), s.paths.log, home, false),
    [el('span.k', t('State directory')), el('span.v.selectable', abbreviateHome(s.paths.stateDir, home))],
    st.autostartOtherCopy ? el('span.orange', icon('warning'), ' ', t('Login item points at another copy: {path}', { path: abbreviateHome(st.autostartOtherCopy, home) })) : null,
  ]));
  form.append(updateSection(s));
});

/// The reconnect rule every locked host follows unless its page gives it
/// one of its own (core::reconnect); the rows are ui/reconnect.js's.
function reconnectSection(s) {
  const c = s.constants, st = s.settings;
  const own = Object.keys(st.hostReconnect);
  return section(t('Reconnect policy'), [
    ...ruleRows(st.reconnect, patch => { for (const [k, v] of Object.entries(patch)) setSetting('reconnect.' + k, v); }).map(([k, v], i) => row([el('span.k', k), v],
      { tip: t('How a locked host comes back after a drop: the wait before each try, and how long to keep trying. A quick failure lengthens the wait, a connection that held starts over. With doubling, the third quick failure in a row jumps to the longest wait, so a wrong password does not hammer the server.'), sub: i > 0 })),
    own.length ? [el('span.k', t('Own rules')), el('span.v', own.map(h => `${h}: ${ruleSummary(st.hostReconnect[h])}`).join('\n'))] : null,
    [el('span.k', t('Health check')), el('span.v', t('every {n}s', { n: c.healthInterval }))],
    [el('span.k', t('Connect timeout')), el('span.v', t('{n}s', { n: c.connectTimeout }))],
  ]);
}

function updateSection(s) {
  const u = s.update, st = s.settings;
  let status, button;
  switch (u.state) {
    case 'idle': status = el('span.secondary', t('Not checked yet')); break;
    case 'checking': status = el('span.secondary', t('Checking…')); break;
    case 'upToDate': status = el('span.secondary', t('Up to date')); break;
    case 'available': status = el('span', t('Version {v} is available', { v: u.version }), ' ', el('a', { href: '#', onclick: e => { e.preventDefault(); invoke('open_url', { url: u.page }); } }, t('Release notes'))); break;
    case 'downloading': status = el('span.secondary', t('Downloading {v}…', { v: u.version }), ' ', u.progress != null ? `${Math.round(u.progress * 100)}%` : ''); break;
    case 'installing': status = el('span.secondary', t('Installing…')); break;
    case 'failed': status = el('span.red', u.message || ''); break;
  }
  if (u.state === 'available') button = el('button.primary', { onclick: () => invoke('install_update') }, t('Install and relaunch'));
  else if (!['checking', 'downloading', 'installing'].includes(u.state)) button = el('button', { onclick: () => invoke('check_update') }, t('Check now'));
  return section(t('Software update'), [
    [el('span.k', t('Version')), el('span.v', s.version)],
    toggle(t('Check for updates automatically'), 'autoUpdate', st.autoUpdate),
    [status, el('div.spacer'), button],
  ]);
}

async function init() {
  await startPage();
  setTitle(t('Shellder Settings'));
  watchState(render);
  pageReady();
  commonKeys();
}
init();
