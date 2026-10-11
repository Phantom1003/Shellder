import { invoke, loadLanguage, t, el, clear, icon, abbreviateHome, connectState, watchState, commonKeys } from './shellder.js';

const LANGUAGES = [['en', 'English'], ['zh-Hans', '简体中文']];
const form = document.getElementById('form');

function section(title, rows) {
  return el('div', el('div.card-title', title), el('div.card', rows.filter(Boolean).map(r => el('div.crow', r))));
}

function toggle(label, key, value, disabled) {
  return [el('span.k', label), el('label.switch', el('input', { type: 'checkbox', checked: value, disabled, onchange: e => invoke('set_setting', { key, value: e.target.checked }) }), el('span.knob'))];
}

function pathRow(label, path, home, editor) {
  return [el('span.k', label), el('span.v.selectable', abbreviateHome(path, home)),
    el('button.borderless', { title: t('Open'), onclick: () => invoke('open_path', { path, editor }) }, icon('link'))];
}

function render(s) {
  clear(form);
  const st = s.settings, home = s.paths.home;
  form.append(section(t('Startup'), [
    toggle(t('Start at login'), 'startAtLogin', st.startAtLogin),
    toggle(t('Start silently'), 'silentLaunch', st.silentLaunch, !st.keepInBackground || !st.startAtLogin),
    st.error ? el('span.red', st.error) : null,
  ]));
  form.append(section(t('Background'), [
    toggle(t('Run in background'), 'keepInBackground', st.keepInBackground),
    toggle(t('Show in menu bar'), 'showMenuBarIcon', st.showMenuBarIcon, !st.keepInBackground),
    // macOS tints the picture itself; a Linux panel shows it as drawn.
    st.platform !== 'macos' ? toggle(t('Invert menu bar icon'), 'invertMenuBarIcon', st.invertMenuBarIcon, !st.keepInBackground) : null,
  ]));
  form.append(section(t('Appearance'), [
    [el('span.k', t('Language')), el('select', { onchange: e => invoke('set_setting', { key: 'language', value: e.target.value }) },
      LANGUAGES.map(([id, name]) => el('option', { value: id, selected: st.language === id }, name)))],
    st.language !== st.launchedLanguage ? [el('span.k'), el('button', { onclick: () => invoke('relaunch') }, t('Relaunch to apply'))] : null,
  ]));
  form.append(section(t('Keychain'), [
    [el('span.k', t('Vault')), s.keychainRefusal ? el('span.orange', { style: 'text-align:right' }, s.keychainRefusal) : el('span.secondary', t('readable'))],
    [el('button', { onclick: () => invoke('request_keychain_access') }, t('Ask the keychain for access'))],
    el('span.note', t('Secrets live in one keychain item. If the keychain was not allowed to open it, nothing is lost — ask again here.')),
  ]));
  const c = s.constants;
  form.append(section(t('Reconnect policy'), [
    [el('span.k', t('Health check')), el('span.v', t('every {n}s', { n: c.healthInterval }))],
    [el('span.k', t('Back-off')), el('span.v', `${c.backoffMin}s → ${c.backoffMax}s, ×2`)],
    [el('span.k', t('Give up after')), el('span.v', t('never'))],
    [el('span.k', t('Connect timeout')), el('span.v', `${c.connectTimeout}s`)],
  ]));
  form.append(section(t('Files'), [
    pathRow(t('ssh config'), s.paths.sshConfig, home, true),
    s.configFiles.length > 1 ? [el('span.k', t('Included')), el('span.v', s.configFiles.slice(1).map(f => abbreviateHome(f, home)).join('\n'))] : null,
    pathRow(t('Log'), s.paths.log, home, false),
    [el('span.k', t('State directory')), el('span.v.selectable', abbreviateHome(s.paths.stateDir, home))],
    st.autostartOtherCopy ? el('span.orange', icon('warning'), ' ', t('Login item points at another copy: {path}', { path: abbreviateHome(st.autostartOtherCopy, home) })) : null,
  ]));
  form.append(updateSection(s));
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
  const s = await connectState();
  await loadLanguage(s.settings.language);
  watchState(render);
  commonKeys();
}
init();
