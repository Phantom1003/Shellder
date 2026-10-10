// The Files window: two trees, either of which can be this machine or a
// connected host, and the copies between them underneath.
import { invoke, listen, loadLanguage, t, el, clear, icon, fmtBytes, fmtTime, fmtFileDate, connectState, dialog, commonKeys } from './shellder.js';

const host = new URLSearchParams(location.search).get('host');
const root = document.getElementById('files');
let snap = null;
let transferHeight = 160;
let lastClicked = { left: null, right: null };
let questionShown = null;

const call = (cmd, args) => invoke(cmd, { host, ...args });
const DRAG = 'application/x-shellder-rows';

// MARK: panes

function sourceTitle(src) { return src.kind === 'local' ? t('Local') : src.host; }

function pane(key, p) {
  const other = key === 'left' ? 'right' : 'left';
  const op = snap.panes[other];
  const local = p.source.kind === 'local';
  const top = el('div.top', { 'data-tauri-drag-region': true },
    key === 'left' ? el('div.titlebar-pad', { 'data-tauri-drag-region': true }) : null,
    el('button.source', { title: t('Show another machine in this pane'), onclick: e => menu(e.currentTarget, [
      { title: t('Local'), icon: 'laptop', run: () => call('files_set_source', { pane: key, source: { kind: 'local' } }) },
      ...(snap.hosts.length ? [{ sep: true }] : []),
      ...snap.hosts.map(h => ({ title: h, icon: 'server', run: () => call('files_set_source', { pane: key, source: { kind: 'host', host: h } }) })),
    ]) }, icon(local ? 'laptop' : 'server'), el('span.label', p.title), icon('chevrondown')),
    el('button', { title: t('Go to a folder on {where}', { where: p.title }), onclick: e => menu(e.currentTarget, [
      ...p.shortcuts.map(s => ({ title: s.title, run: () => call('files_set_root', { pane: key, path: s.path }) })),
      { sep: true },
      { title: t('Go to Folder…'), run: async () => { const r = await dialog({ title: t('Go to Folder'), message: t('A path on {where}', { where: p.title }), input: true, preset: p.root, action: t('Go') }); if (r.ok && r.value) call('files_set_root', { pane: key, path: r.value }); } },
    ]) }, icon('folder')),
    el('div.spacer', { 'data-tauri-drag-region': true }),
    el('button', { title: t('New Folder'), onclick: () => create(key, true) }, icon('folderplus')),
    el('button', { title: t('New File'), onclick: () => create(key, false) }, icon('docplus')),
    el('button', { title: local ? t('Move to Trash') : t('Delete'), disabled: !p.selected.length, onclick: () => remove(key) }, icon('trash')),
    el('button', { title: t('Show the entries whose name starts with a dot'), class: p.showHidden ? 'active' : '', onclick: () => call('files_nav', { pane: key, action: 'toggle-hidden' }) }, icon(p.showHidden ? 'eye' : 'eyeoff')),
    el('button', { title: t('Back to the directory before this one'), disabled: !p.canGoBack, onclick: () => call('files_nav', { pane: key, action: 'back' }) }, icon('arrowleft')),
    el('button', { title: t('Up one directory'), onclick: () => call('files_nav', { pane: key, action: 'up' }) }, icon('arrowup')),
    el('button', { title: t('Refresh'), onclick: () => call('files_nav', { pane: key, action: 'reload' }) }, icon('refresh')),
    el('div.sep'),
    el('button', { title: t('Copy what is selected to {where}', { where: op.title }), disabled: !snap.canCopy[key], onclick: () => call('files_copy_selection', { pane: key }) }, icon(key === 'left' ? 'arrowright' : 'arrowleft')));

  const tree = el('div.tree');
  const table = el('table');
  const head = el('tr', ['name', 'modified', 'size'].map(col => {
    const titles = { name: t('Name'), modified: t('Date Modified'), size: t('Size') };
    const active = p.sort.key === col;
    return el('th.sortable', { style: col === 'name' ? '' : col === 'size' ? 'width:90px;text-align:right' : 'width:150px',
      onclick: () => call('files_set_sort', { pane: key, sort: { key: col, ascending: active ? !p.sort.ascending : col === 'name' } }) },
      titles[col], active ? (p.sort.ascending ? ' ↑' : ' ↓') : '');
  }));
  table.append(el('thead', head));
  const body = el('tbody');
  for (const r of p.rows) body.append(row(key, p, r));
  table.append(body);
  tree.append(table);
  tree.onclick = e => { if (e.target === tree || e.target === table) call('files_select', { pane: key, paths: [] }); };
  dropTarget(tree, key, null);
  tree.onkeydown = e => {
    if (e.key === 'Backspace' && (e.metaKey || e.ctrlKey) && p.selected.length) { e.preventDefault(); remove(key); }
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const paths = p.rows.map(r => r.path);
      const cur = p.selected.length ? paths.indexOf(lastClicked[key] || p.selected[p.selected.length - 1]) : -1;
      const next = Math.max(0, Math.min(paths.length - 1, cur + (e.key === 'ArrowDown' ? 1 : -1)));
      if (paths[next] !== undefined) { lastClicked[key] = paths[next]; call('files_select', { pane: key, paths: [paths[next]] }); }
    }
    if (e.key === 'ArrowRight' && p.selected.length === 1) call('files_nav', { pane: key, action: 'expand', path: p.selected[0] });
    if (e.key === 'ArrowLeft' && p.selected.length === 1) call('files_nav', { pane: key, action: 'collapse', path: p.selected[0] });
    if (e.key === 'Enter' && p.selected.length === 1) { const r = p.rows.find(x => x.path === p.selected[0]); if (r && r.isDir) call('files_set_root', { pane: key, path: r.path }); }
  };
  tree.tabIndex = 0;

  const parts = [top, tree];
  if (p.error) parts.push(el('div.error', icon('warning', 'orange'), el('span.selectable', p.error)));
  parts.push(pathBar(key, p));
  return el('div.pane', { dataset: { pane: key } }, parts);
}

function row(key, p, r) {
  const selected = p.selected.includes(r.path);
  const tr = el('tr', { class: selected ? 'selected' : '', draggable: true, dataset: { path: r.path } });
  const disclosure = r.isDir
    ? el('span.disclosure', { class: r.expanded ? 'open' : '', onclick: e => { e.stopPropagation(); call('files_nav', { pane: key, action: 'toggle', path: r.path }); } }, icon('chevron'))
    : el('span.disclosure');
  const name = el('td.name', { style: `padding-left:${8 + r.depth * 16}px` }, disclosure, icon(r.isDir ? 'folder' : 'doc', r.isDir ? 'dir' : 'file'), el('span', { style: 'overflow:hidden;text-overflow:ellipsis' }, r.name), r.loading ? el('span.caption', ' …') : null);
  tr.append(name, el('td.date', fmtFileDate(r.modified)), el('td.size', r.isDir ? '—' : fmtBytes(r.size)));
  tr.onmousedown = e => {
    if (e.button !== 0) return;
    let paths;
    if (e.shiftKey && lastClicked[key]) {
      const all = p.rows.map(x => x.path);
      const [a, b] = [all.indexOf(lastClicked[key]), all.indexOf(r.path)].sort((x, y) => x - y);
      paths = all.slice(a, b + 1);
    } else if (e.metaKey || e.ctrlKey) {
      paths = selected ? p.selected.filter(x => x !== r.path) : [...p.selected, r.path];
      lastClicked[key] = r.path;
    } else {
      if (selected) return; // keep the selection for a drag of several rows
      paths = [r.path];
      lastClicked[key] = r.path;
    }
    call('files_select', { pane: key, paths });
  };
  tr.ondblclick = () => { if (r.isDir) call('files_set_root', { pane: key, path: r.path }); else if (p.source.kind === 'local') invoke('open_path', { path: r.path, editor: false }); };
  tr.ondragstart = e => {
    const paths = selected ? p.selected : [r.path];
    e.dataTransfer.setData(DRAG, JSON.stringify(paths.map(path => ({ source: p.source, path }))));
    e.dataTransfer.effectAllowed = 'copy';
  };
  dropTarget(tr, key, r.path);
  return tr;
}

/// What a drop here does: rows from the other side are copied over.
function dropTarget(node, key, onto) {
  node.ondragover = e => {
    if (!e.dataTransfer.types.includes(DRAG)) return;
    e.preventDefault();
    e.stopPropagation();
    e.dataTransfer.dropEffect = 'copy';
    node.classList.add('droptarget');
  };
  node.ondragleave = () => node.classList.remove('droptarget');
  node.ondrop = async e => {
    node.classList.remove('droptarget');
    const raw = e.dataTransfer.getData(DRAG);
    if (!raw) return;
    e.preventDefault();
    e.stopPropagation();
    const dragged = JSON.parse(raw);
    await call('files_drop', { pane: key, dragged, onto });
  };
}

function pathBar(key, p) {
  const bar = el('div.pathbar');
  const parts = p.root.split('/').filter(Boolean);
  bar.append(el('button', { onclick: () => call('files_set_root', { pane: key, path: '/' }) }, icon('folder'), p.source.kind === 'local' ? t('Local') : '/'));
  let acc = '';
  for (const part of parts) {
    acc += '/' + part;
    const path = acc;
    bar.append(el('span.sepchev', icon('chevron')), el('button', { onclick: () => call('files_set_root', { pane: key, path }) }, part));
  }
  return bar;
}

async function create(key, directory) {
  const target = await call('files_target', { pane: key });
  const r = await dialog({ title: directory ? t('New Folder') : t('New File'), message: t('It goes into {dir}', { dir: target }), input: true,
    preset: directory ? t('untitled folder') : t('untitled.txt'), action: t('Create') });
  if (r.ok && r.value) call('files_create', { pane: key, name: r.value, directory });
}

async function remove(key) {
  const p = snap.panes[key];
  const paths = p.selected;
  if (!paths.length) return;
  const local = p.source.kind === 'local';
  const name = paths[0].split('/').pop();
  const title = paths.length === 1
    ? (local ? t('Move “{name}” to the Trash?', { name }) : t('Delete “{name}” on {where}?', { name, where: p.title }))
    : (local ? t('Move {n} items to the Trash?', { n: paths.length }) : t('Delete {n} items on {where}?', { n: paths.length, where: p.title }));
  const r = await dialog({ title, message: local ? null : t('This cannot be undone.'), action: local ? t('Move to Trash') : t('Delete'), destructive: true });
  if (r.ok) call('files_delete', { pane: key, paths });
}

// MARK: transfers

function stateIcon(rec) {
  const map = { waiting: 'clock', running: rec.isUpload ? 'arrowupcircle' : 'arrowdowncircle', done: 'circlecheck', cancelled: 'xcircle', skipped: 'minuscircle', failed: 'warning' };
  const i = icon(map[rec.state.kind]);
  i.style.color = rec.state.kind === 'running' ? 'var(--accent)' : rec.state.kind === 'failed' ? 'var(--orange)' : 'var(--secondary)';
  return i;
}

function statusText(rec) {
  switch (rec.state.kind) {
    case 'waiting': return t('Waiting');
    case 'running': return t('Copying…');
    case 'cancelled': return t('Cancelled');
    case 'skipped': return t('Skipped, it was already there');
    case 'failed': return rec.state.why;
    case 'done': return (rec.total != null ? fmtBytes(rec.total) + ' · ' : '') + fmtTime(rec.endedAt || rec.queuedAt);
  }
}

function transfers() {
  const s = snap;
  const strip = el('div.strip',
    el('button', { title: t('Every copy this window has run'), onclick: () => call('files_transfer', { action: 'history', on: !s.showHistory }) }, icon('chevron', s.showHistory ? 'open' : '')),
    el('span', { style: 'font-weight:600;color:var(--secondary)' }, t('Transfers')));
  strip.firstChild.firstChild.style.transform = s.showHistory ? 'rotate(90deg)' : '';
  if (s.active) {
    const a = s.active;
    strip.append(stateIcon(a), el('span', { style: 'overflow:hidden;text-overflow:ellipsis;white-space:nowrap' }, `${a.name} → ${a.job.destination}`));
    if (a.progress != null) strip.append(el('div.bar', el('div', { style: `width:${Math.round(a.progress * 100)}%` })), el('span.secondary', `${Math.round(a.progress * 100)}%`));
    else strip.append(el('div.bar.indeterminate', el('div')));
    strip.append(el('button', { title: t('Stop this copy'), onclick: () => call('files_transfer', { action: 'cancel' }) }, icon('xcircle')));
    if (s.queued) strip.append(el('span.secondary', t('+{n} waiting', { n: s.queued })));
  } else if (s.notice) {
    strip.append(icon(s.noticeIsError ? 'warning' : 'circlecheck', s.noticeIsError ? 'orange' : ''), el('span.selectable', { class: s.noticeIsError ? 'red' : 'secondary', style: 'overflow:hidden;text-overflow:ellipsis;white-space:nowrap' }, s.notice));
  } else if (s.lastFinished) {
    strip.append(stateIcon(s.lastFinished), el('span.secondary', { style: 'overflow:hidden;text-overflow:ellipsis;white-space:nowrap' }, `${s.lastFinished.name} → ${s.lastFinished.job.destination}`));
  }
  strip.append(el('div.spacer'), el('button', { disabled: !s.transfers.some(r => !['waiting', 'running'].includes(r.state.kind)), onclick: () => call('files_transfer', { action: 'clear' }) }, t('Clear')));
  const parts = [strip];
  if (s.showHistory) {
    const list = el('div.history', { style: `height:${transferHeight}px` });
    if (!s.transfers.length) list.append(el('div.empty', t('Nothing has been copied yet.')));
    for (const rec of [...s.transfers].reverse()) {
      const r = el('div.trow', { title: `${sourceTitle(rec.job.from)}:${rec.job.source} → ${sourceTitle(rec.job.to)}:${rec.job.destination}\n${statusText(rec)}` },
        stateIcon(rec), el('span', rec.name),
        el('span.where', `${sourceTitle(rec.job.from)}:${rec.job.source}  →  ${sourceTitle(rec.job.to)}:${rec.job.destination}`));
      if (rec.state.kind === 'running' && rec.progress != null) r.append(el('div.bar', el('div', { style: `width:${Math.round(rec.progress * 100)}%` })), el('span.status', `${Math.round(rec.progress * 100)}%`));
      else r.append(el('span.status', { class: rec.state.kind === 'failed' ? 'err' : '' }, statusText(rec)));
      if (s.canRetry.includes(rec.id)) r.append(el('button.borderless', { title: t('Try Again'), onclick: () => call('files_transfer', { action: 'retry', id: rec.id }) }, icon('refresh')));
      list.append(r);
    }
    const handle = el('div.drag-handle');
    let startY = 0, startH = 0;
    handle.onmousedown = e => {
      startY = e.clientY; startH = list.offsetHeight;
      const move = ev => { transferHeight = Math.min(600, Math.max(60, startH - (ev.clientY - startY))); list.style.height = `${transferHeight}px`; };
      const up = () => { document.removeEventListener('mousemove', move); document.removeEventListener('mouseup', up); invoke('set_setting', { key: 'transferPanelHeight', value: transferHeight }); };
      document.addEventListener('mousemove', move); document.addEventListener('mouseup', up);
    };
    parts.unshift(handle);
    parts.push(list);
  }
  return el('div.transfers', parts);
}

async function askReplace(q) {
  if (questionShown === q.id) return;
  questionShown = q.id;
  const r = await dialog({ title: t('Replace “{name}”?', { name: q.name }), message: t('{where}:{dir} already has one.', { where: q.toTitle, dir: q.destination }),
    action: t('Replace'), destructive: true, cancel: t('Skip'), extra: q.offerAll ? t('Apply to all') : null });
  questionShown = null;
  const answer = r.ok ? (r.extra ? 'replaceAll' : 'replace') : (r.extra ? 'skipAll' : 'skip');
  call('files_transfer', { action: 'answer', answer });
}

// MARK: menus

let openMenu = null;

/// A popup under its button. The button toggles: a second click on it
/// closes the menu instead of opening another, a click anywhere else
/// (including another such button) just closes it.
function menu(anchor, items) {
  if (openMenu && openMenu.anchor === anchor) { openMenu.close(); return; }
  if (openMenu) openMenu.close();
  const m = el('div.menu');
  for (const it of items) m.append(it.sep ? el('div.sep') : el('div.item', { onclick: () => { close(); it.run(); } }, it.icon ? icon(it.icon) : null, it.title));
  const rect = anchor.getBoundingClientRect();
  m.style.left = `${rect.left}px`; m.style.top = `${rect.bottom + 4}px`;
  const close = () => { m.remove(); document.removeEventListener('mousedown', outside); if (openMenu && openMenu.anchor === anchor) openMenu = null; };
  const outside = e => { if (!m.contains(e.target) && !anchor.contains(e.target)) close(); };
  openMenu = { anchor, close };
  document.body.append(m);
  setTimeout(() => document.addEventListener('mousedown', outside), 0);
}

// MARK: render

function render(s) {
  snap = s;
  // Keep the scroll positions and the focus across a redraw.
  const scrolls = {}; for (const k of ['left', 'right']) { const tr = root.querySelector(`.pane[data-pane=${k}] .tree`); if (tr) scrolls[k] = tr.scrollTop; }
  const focused = document.activeElement && document.activeElement.closest('.pane') ? document.activeElement.closest('.pane').dataset.pane : null;
  clear(root);
  root.append(el('div.panes', pane('left', s.panes.left), pane('right', s.panes.right)), transfers());
  for (const k of ['left', 'right']) { const tr = root.querySelector(`.pane[data-pane=${k}] .tree`); if (tr && scrolls[k]) tr.scrollTop = scrolls[k]; }
  if (focused) { const tr = root.querySelector(`.pane[data-pane=${focused}] .tree`); if (tr) tr.focus({ preventScroll: true }); }
  if (s.question) askReplace(s.question);
}

/// Files dragged in from the desktop land in the pane under the cursor.
function nativeDrops() {
  const w = window.__TAURI__.webview.getCurrentWebview();
  w.onDragDropEvent(e => {
    const p = e.payload;
    if (p.type === 'over') {
      for (const n of root.querySelectorAll('.droptarget')) n.classList.remove('droptarget');
      const target = targetAt(p.position);
      if (target) target.node.classList.add('droptarget');
    } else if (p.type === 'drop') {
      for (const n of root.querySelectorAll('.droptarget')) n.classList.remove('droptarget');
      const target = targetAt(p.position);
      if (target) call('files_drop_local', { pane: target.pane, paths: p.paths, onto: target.path });
    } else {
      for (const n of root.querySelectorAll('.droptarget')) n.classList.remove('droptarget');
    }
  });
}

function targetAt(position) {
  const scale = window.devicePixelRatio || 1;
  const node = document.elementFromPoint(position.x / scale, position.y / scale);
  if (!node) return null;
  const paneEl = node.closest('.pane');
  if (!paneEl) return null;
  const tr = node.closest('tr[data-path]');
  return { pane: paneEl.dataset.pane, path: tr ? tr.dataset.path : null, node: tr || paneEl.querySelector('.tree') };
}

async function init() {
  const app = await connectState();
  await loadLanguage(app.settings.language);
  document.title = t('Files — {host}', { host });
  transferHeight = app.transferPanelHeight || 160;
  await listen(`files:files-${host.replace(/[^A-Za-z0-9_-]/g, '_')}`, render);
  render(await call('files_open', {}));
  nativeDrops();
  commonKeys();
}
init();
