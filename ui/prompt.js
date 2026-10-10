import { invoke, loadLanguage, t, el, clear, icon, connectState, watchState } from './shellder.js';

const box = document.getElementById('prompt');
let shownId = null;

function render(s) {
  const p = s.prompt;
  if (!p) { clear(box); box.append(el('div.secondary', { style: 'margin:auto' }, t('No pending prompt'))); shownId = null; return; }
  if (p.id === shownId) {
    const more = box.querySelector('.more');
    if (more) more.textContent = s.pendingPrompts > 1 ? t('+{n} more', { n: s.pendingPrompts - 1 }) : '';
    return;
  }
  shownId = p.id;
  clear(box);
  const titles = { password: t('Password needed'), passphrase: t('Key passphrase needed'), totp: t('Verification code needed'), confirm: t('Confirm host key'), other: t('ssh is asking') };
  const placeholders = { password: t('password'), passphrase: t('passphrase'), totp: t('code from your authenticator'), other: t('answer') };
  const icons = { password: 'key', passphrase: 'key', totp: 'number', confirm: 'shield', other: 'question' };
  const confirm = p.kind === 'confirm';
  box.append(el('div.head', icon(icons[p.kind], confirm ? 'orange' : ''),
    el('div', { style: 'flex:1' }, el('h3', { style: 'font-size:15px' }, titles[p.kind]), el('div.secondary', `ssh → ${p.host}`)),
    el('div.caption.more', s.pendingPrompts > 1 ? t('+{n} more', { n: s.pendingPrompts - 1 }) : '')));
  box.append(el('div.text', p.prompt.trim()));
  let field = null, save = null;
  if (confirm) {
    box.append(el('div.secondary', { style: 'font-size:12px' }, t('Compare the fingerprint with what the server\'s administrator gave you. Answering yes adds the key to ~/.ssh/known_hosts.')));
  } else {
    const secret = p.kind === 'password' || p.kind === 'passphrase';
    field = el('input', { type: secret ? 'password' : 'text', placeholder: placeholders[p.kind], autocomplete: 'off', autocapitalize: 'off', spellcheck: false, class: secret ? '' : 'mono' });
    const eye = secret ? el('button.borderless', { title: t('Show what you typed'), onclick: () => { field.type = field.type === 'password' ? 'text' : 'password'; eye.replaceChildren(icon(field.type === 'password' ? 'eye' : 'eyeoff')); field.focus(); } }, icon('eye')) : null;
    box.append(el('div.row', field, eye));
    if (p.allowSave) {
      save = el('input', { type: 'checkbox' });
      box.append(el('label.row', save, t('Save in Keychain so shellder can answer this itself next time')));
    } else if (p.kind === 'totp') {
      box.append(el('div.caption', t('Store the TOTP secret under Credentials and shellder will generate these codes for you.')));
    }
  }
  const cancel = el('button', { onclick: () => invoke('answer_prompt', { id: p.id, answer: confirm ? 'no' : null, save: false }) }, confirm ? t("Don't connect") : t('Cancel'));
  const ok = el('button.primary', { disabled: !confirm, onclick: () => invoke('answer_prompt', { id: p.id, answer: confirm ? 'yes' : field.value, save: !!(save && save.checked) }) }, confirm ? t('Yes, connect') : t('Continue'));
  box.append(el('div.buttons', cancel, ok));
  if (field) {
    field.oninput = () => { ok.disabled = !field.value; };
    field.onkeydown = e => { if (e.key === 'Enter' && field.value) ok.click(); };
    setTimeout(() => field.focus(), 50);
  }
  document.onkeydown = e => { if (e.key === 'Escape') cancel.click(); if (e.key === 'Enter' && confirm) ok.click(); };
}

async function init() {
  const s = await connectState();
  await loadLanguage(s.settings.language);
  watchState(render);
}
init();
