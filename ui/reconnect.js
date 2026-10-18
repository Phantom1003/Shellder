// The reconnect rule (core::reconnect): how long a locked host waits
// between the attempts that bring a dropped connection back, and when it
// stops trying. One rule is the default for every host, set in Settings;
// a host's page may give it one of its own. Both pages draw the same
// rows: `ruleRows` returns [label, control] pairs, and `send` gets the
// field changed ({first: 30}, a null for the built-in value).
import { t, el } from './shellder.js';

const GIVE_UPS = [0, 600, 1800, 3600, 3 * 3600, 12 * 3600, 86400, 7 * 86400];

/// "5s", "10 min", "1 h", "2 d".
export function brief(secs) {
  if (secs < 60) return t('{n}s', { n: secs });
  if (secs % 86400 === 0) return t('{n} d', { n: secs / 86400 });
  if (secs % 3600 === 0) return t('{n} h', { n: secs / 3600 });
  if (secs % 60 === 0) return t('{n} min', { n: secs / 60 });
  return `${Math.floor(secs / 60)}m ${secs % 60}s`;
}

/// One line for a rule: "5s → 10 min, doubling", "every 30s, gives up after 1 h".
export function ruleSummary(r) {
  let s = r.growth === 'fixed' ? t('every {d}', { d: brief(r.first) })
    : t('{a} → {b}, doubling', { a: brief(r.first), b: brief(r.max) });
  if (r.giveUp > 0) s += t(', gives up after {d}', { d: brief(r.giveUp) });
  return s;
}

function number(value, { min, max, width = 80 }, onchange) {
  const input = el('input.text', { type: 'number', value, min, max, step: 1, spellcheck: false, style: `width:${width}px; text-align:right` });
  input.onchange = () => {
    const v = input.value.trim();
    if (v === '') { onchange(null); return; }
    const n = Math.min(max, Math.max(min, Math.round(Number(v))));
    if (Number.isNaN(n)) { input.value = value; return; }
    input.value = n;
    if (n !== value) onchange(n);
  };
  input.onkeydown = e => { if (e.key === 'Enter') { e.preventDefault(); input.blur(); } };
  return input;
}

/// The rows that edit a rule `r` (every field decided, as the state has
/// it). An empty wait sends null: the page gets the value from below
/// (the default rule, or the built-in one) back in the state.
export function ruleRows(r, send) {
  const growth = el('select', { onchange: e => send({ growth: e.target.value }) },
    el('option', { value: 'doubling', selected: r.growth === 'doubling' }, t('Doubling')),
    el('option', { value: 'fixed', selected: r.growth === 'fixed' }, t('Fixed')));
  const first = number(r.first, { min: 1, max: 3600 }, v => send({ first: v }));
  const inputs = [first];
  const rows = [
    [t('Wait between tries'), growth],
    [r.growth === 'fixed' ? t('Wait') : t('First wait'), el('span.v', first, el('span.secondary', ' ' + t('seconds')))],
  ];
  if (r.growth !== 'fixed') {
    const max = number(r.max, { min: 1, max: 86400 }, v => send({ max: v }));
    inputs.push(max);
    rows.push([t('Longest wait'), el('span.v', max, el('span.secondary', ' ' + t('seconds')))]);
  }
  // WebKit draws a select as the native popup button, shorter than a
  // text field with the same padding: the fields take the select's
  // height once both are on the page (the caller puts them there in
  // this very task), so the rows come out alike.
  setTimeout(() => {
    const h = growth.getBoundingClientRect().height;
    if (h) for (const i of inputs) { i.style.height = h + 'px'; i.style.paddingTop = i.style.paddingBottom = '0'; i.style.boxSizing = 'border-box'; }
  });
  const choices = GIVE_UPS.includes(r.giveUp) ? GIVE_UPS : [...GIVE_UPS, r.giveUp].sort((a, b) => a - b);
  const giveUp = el('select', { onchange: e => send({ giveUp: Number(e.target.value) }) },
    choices.map(secs => el('option', { value: secs, selected: r.giveUp === secs }, secs ? t('After {d} down', { d: brief(secs) }) : t('Never'))));
  rows.push([t('Give up'), giveUp]);
  return rows;
}
