// The view of a host's files, shared by the Files window (two panes, any
// machine in either) and the Console's Files tab (one pane, the host):
// the trees, what a drag or a drop does, the folder menu, and the copies
// underneath. The model is the app's, one per host (`files_*`), so the
// window and the tab show the same state.
import { invoke, t, el, clear, icon, fmtBytes, fmtTime, fmtFileDate, dialog, menu, menuAt, pointAnchor, popup, smoothWheel, REAL_DPR } from './shellder.js';

/// The drag type of rows from a tree: `[{source, path}]` as JSON.
export const DRAG = 'application/x-shellder-rows';

// The rows of a drag begun in a tree, while it lasts. On macOS the page
// never gets an HTML drop: wry answers every drag itself (Tauri's
// drag-drop handler) and WebKit is not told of it, so a drag of the
// page's own lands as the webview's native drop event, with no paths,
// at a position (`nativeDrops`); the HTML handlers serve where that
// event is not the only one. `true` once a drop took the rows, so the
// other path does not repeat it; forgotten a moment after the drag
// ends, since the native drop can arrive after the source's dragend.
let dragged = null;
let dragForgets = null;

function takeDrag() {
  const rows = Array.isArray(dragged) ? dragged : null;
  if (rows) dragged = true;
  return rows;
}

/// What an HTML drop event brings: the rows of this page's drag, or of
/// another window's through the data transfer; nothing when the native
/// drop already took them.
export function dropRows(e) {
  if (dragged === true) return null;
  const rows = takeDrag();
  if (rows) return rows;
  const raw = e.dataTransfer.types.includes(DRAG) ? e.dataTransfer.getData(DRAG) : '';
  return raw ? JSON.parse(raw) : null;
}

/// Draw into `root`. `single`: one pane, the one showing `host`, with the
/// copies strip under it; otherwise both panes side by side.
/// `onUpload(dir)` and `onDownload(paths)` are what the single pane's
/// context menu offers: a drop of files from this machine into `dir`, a
/// copy of `paths` to a folder here. `onOpen(row, keep)` is what a click
/// on a file does there: one click shows it for a look (`keep` false),
/// a double click, Return or *Preview* in the menu for good; without it
/// a double click opens a file on this machine in its application.
/// `onShell(dir)` is *Shell here* in the menus of a folder and of the
/// blank: a terminal in that directory. `dropOutside(node)` says what a
/// drag of rows may end on outside the trees: `{node, drop(rows)}` for
/// an element under the pointer that takes them, or null.
/// A popup to pick a folder on one side (`where` names it): the path at
/// the top, to edit and confirm with Return, and under it the tree of
/// that path: the directories inside it (`list(path)` gives them, as
/// {title, path}), each one opening on demand and a click away;
/// `pick(path)` is the click. Typing another path in the field shows
/// that path's tree instead; the buttons go up a directory and `home`.
/// `above` puts the popup over a dialog's backdrop.
export function folderPicker(anchor, { where, root: at, home, list, pick, above = false }) {
  const m = el('div.menu.folders', { class: above ? 'above' : '' });
  let close = null;
  const go = path => { close(); pick(path); };
  let timer = null;
  const parent = path => { const bare = path.replace(/\/+$/, ''); const i = bare.lastIndexOf('/'); return i <= 0 ? '/' : bare.slice(0, i); };
  const field = el('input.text.path', { value: at, spellcheck: false, placeholder: t('A path on {where}', { where }),
    onkeydown: e => {
      if (e.key === 'Enter') { e.preventDefault(); const v = field.value.trim(); if (v) go(v); }
      else if (e.key === 'Escape') { e.preventDefault(); close(); }
    },
    oninput: () => { clearTimeout(timer); timer = setTimeout(() => { const v = field.value.trim(); if (v && v !== root) show(v); }, 300); } });
  const tree = el('div.tree');
  smoothWheel(tree);
  const listed = new Map(); // path -> its subdirectories, null while they load, {error} when they could not be listed
  const opened = new Set();
  let root = at;
  const fetch = async path => {
    listed.set(path, null);
    try { listed.set(path, await list(path)); }
    catch (e) { listed.set(path, { error: String(e) }); }
    draw();
  };
  const node = (path, title, depth) => {
    const open = opened.has(path);
    const disclosure = el('span.disclosure', { class: open ? 'open' : '', onclick: e => { e.stopPropagation(); toggle(path); } }, icon('chevron'));
    const rows = [el('div.item', { style: `padding-left: ${6 + depth * 14}px`, onclick: () => go(path) }, disclosure, title)];
    if (open) {
      const got = listed.get(path);
      const note = text => el('div.note', { style: `padding-left: ${26 + depth * 14}px` }, text);
      if (got && got.error) rows.push(note(got.error));
      else if (got && !got.length) rows.push(note(t('No folders inside')));
      else if (got) for (const k of got) rows.push(...node(k.path, k.title, depth + 1));
    }
    return rows;
  };
  const toggle = path => { if (opened.has(path)) opened.delete(path); else { opened.add(path); if (!listed.has(path)) fetch(path); } draw(); };
  const draw = () => { clear(tree); tree.append(...node(root, root, 0)); };
  const up = el('button', { title: t('Up one directory'), onclick: () => show(parent(root)) }, icon('arrowup'));
  const show = path => {
    root = path;
    if (field.value !== path) field.value = path;
    up.disabled = parent(path) === path;
    opened.clear(); opened.add(path);
    if (!listed.has(path)) fetch(path);
    draw();
  };
  m.append(el('div.pathrow', up, el('button', { title: t('Home'), onclick: () => show(home) }, icon('home')), field), tree);
  close = popup(anchor, m);
  if (!close) return;
  show(at);
  field.focus();
  field.select();
}

export function createFilesView(root, { host, single = false, transferHeight: initialHeight = 160, onUpload = null, onDownload = null, onWindow = null, onOpen = null, onShell = null, dropOutside = null } = {}) {
  let snap = null;
  let transferHeight = initialHeight;
  let lastClicked = { left: null, right: null };
  let pressed = null;     // the row the mouse went down on, for the click it may end as
  let shownRoot = { left: null, right: null };   // the root each pane last drew
  const scrollAt = { left: {}, right: {} };      // root -> where the tree was scrolled to, while it showed
  const scrollDue = { left: null, right: null }; // {root, top}: to scroll to once that root's rows are there
  let questionShown = null;
  const call = (cmd, args) => invoke(cmd, { host, ...args });

  // MARK: panes

  function sourceTitle(src) { return src.kind === 'local' ? t('Local') : src.host; }

  function pane(key, p) {
    const other = key === 'left' ? 'right' : 'left';
    const op = snap.panes[other];
    const local = p.source.kind === 'local';
    const top = el('div.top', { 'data-tauri-drag-region': !single },
      key === 'left' && !single ? el('div.titlebar-pad', { 'data-tauri-drag-region': true }) : null,
      // The single pane (the Console's Files tab) is its host's: the
      // window says which, the row does not.
      single ? null : el('button.source', { title: t('Show another machine in this pane: a host with a master up through it, any other on a login of its own'), onclick: e => menu(e.currentTarget, [
          { title: t('Local'), icon: 'laptop', run: () => call('files_set_source', { pane: key, source: { kind: 'local' } }) },
          ...(snap.hosts.length ? [{ sep: true }] : []),
          ...snap.hosts.map(h => ({ title: h, icon: 'server', run: () => call('files_set_source', { pane: key, source: { kind: 'host', host: h } }) })),
        ]) }, icon(local ? 'laptop' : 'server'), el('span.label', p.title), icon('chevrondown')),
      el('button', { title: t('Go to a folder on {where}', { where: p.title }), onclick: e => folderMenu(e.currentTarget, key, p) }, icon('folder')),
      el('div.spacer', { 'data-tauri-drag-region': !single }),
      el('button', { title: t('New Folder'), onclick: () => create(key, true) }, icon('folderplus')),
      el('button', { title: t('New File'), onclick: () => create(key, false) }, icon('docplus')),
      el('button', { title: local ? t('Move to Trash') : t('Delete'), disabled: !p.selected.length, onclick: () => remove(key) }, icon('trash')),
      el('button', { title: t('Show the entries whose name starts with a dot'), class: p.showHidden ? 'active' : '', onclick: () => call('files_nav', { pane: key, action: 'toggle-hidden' }) }, icon(p.showHidden ? 'eye' : 'eyeoff')),
      el('button', { title: t('Back to the directory before this one'), disabled: !p.canGoBack, onclick: () => call('files_nav', { pane: key, action: 'back' }) }, icon('arrowleft')),
      el('button', { title: t('Up one directory'), onclick: () => call('files_nav', { pane: key, action: 'up' }) }, icon('arrowup')),
      el('button', { title: t('Refresh'), onclick: () => call('files_nav', { pane: key, action: 'reload' }) }, icon('refresh')),
      single ? null : el('div.sep'),
      single ? null : el('button', { title: t('Copy what is selected to {where}', { where: op.title }), disabled: !snap.canCopy[key], onclick: () => call('files_copy_selection', { pane: key }) }, icon(key === 'left' ? 'arrowright' : 'arrowleft')),
      single && onWindow ? el('div.sep') : null,
      single && onWindow ? el('button', { title: t('The Files window: two trees side by side — {alias}, this machine, or another host in either one, a drag copies', { alias: host }), onclick: onWindow }, icon('link')) : null);

    const tree = el('div.tree');
    const table = el('table');
    const head = el('tr', ['name', 'modified', 'size'].map(col => {
      const titles = { name: t('Name'), modified: t('Date Modified'), size: t('Size') };
      const active = p.sort.key === col;
      return el('th.sortable', { style: col === 'name' ? '' : col === 'size' ? `width:${single ? 64 : 90}px;text-align:right` : `width:${single ? 100 : 150}px`,
        onclick: () => call('files_set_sort', { pane: key, sort: { key: col, ascending: active ? !p.sort.ascending : col === 'name' } }) },
        titles[col], active ? (p.sort.ascending ? ' ↑' : ' ↓') : '');
    }));
    table.append(el('thead', head));
    const body = el('tbody');
    for (const r of p.rows) body.append(row(key, p, r));
    table.append(body);
    tree.append(table);
    tree.onclick = e => { if (e.target === tree || e.target === table) call('files_select', { pane: key, paths: [] }); };
    // A click on a file, judged when the mouse comes up: the row was
    // redrawn in between (the press selected it), so the browser's own
    // click event would not reach it. The file shows for a look; the
    // second press of a double click does not. A folder's row only
    // selects: its disclosure opens and closes it.
    tree.onmouseup = e => {
      const was = pressed;
      pressed = null;
      if (!was || e.button !== 0 || was.detail !== 1 || !onOpen) return;
      const tr = e.target.closest('tr[data-path]');
      if (!tr || tr.dataset.path !== was.path || Math.abs(e.clientX - was.x) > 4 || Math.abs(e.clientY - was.y) > 4) return;
      const r = snap.panes[key].rows.find(x => x.path === was.path);
      if (r && !r.isDir) onOpen(r, false);
    };
    // The blank's menu: what the buttons above do to the folder shown.
    // Like a click there it leaves nothing selected, so a new folder
    // goes into the folder shown.
    tree.oncontextmenu = e => {
      if (e.target.closest('tr[data-path]')) return;
      e.preventDefault();
      if (p.selected.length) call('files_select', { pane: key, paths: [] });
      const at = pointAnchor(e.clientX, e.clientY);
      menu(at, [
        onUpload ? { title: t('Upload here…'), icon: 'arrowupcircle', run: () => onUpload(p.root) } : null,
        onShell ? { title: t('Shell here'), icon: 'terminal', run: () => onShell(p.root) } : null,
        { sep: true },
        ...createItems(key),
        { sep: true },
        { title: t('Go to folder…'), icon: 'folder', run: () => folderMenu(at, key, snap.panes[key]) },
        { title: t('Back'), icon: 'arrowleft', disabled: !p.canGoBack, run: () => call('files_nav', { pane: key, action: 'back' }) },
        { title: t('Up one directory'), icon: 'arrowup', disabled: p.root === '/', run: () => call('files_nav', { pane: key, action: 'up' }) },
        { sep: true },
        { title: p.showHidden ? t('Hide hidden files') : t('Show hidden files'), icon: p.showHidden ? 'eyeoff' : 'eye', run: () => call('files_nav', { pane: key, action: 'toggle-hidden' }) },
        refreshItem(key),
        { sep: true },
        onWindow ? { title: t('Open in the Files window'), icon: 'link', run: onWindow } : null]);
    };
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
      if (e.key === 'Enter' && p.selected.length === 1) { const r = p.rows.find(x => x.path === p.selected[0]); if (r && r.isDir) call('files_set_root', { pane: key, path: r.path }); else if (r && onOpen) onOpen(r, true); }
    };
    tree.tabIndex = 0;
    smoothWheel(tree);

    const parts = [top, tree];
    if (p.error) parts.push(el('div.error', icon('warning', 'orange'), el('span.selectable', p.error)));
    parts.push(pathBar(key, p));
    return el('div.pane', { dataset: { pane: key } }, parts);
  }

  function row(key, p, r) {
    const selected = p.selected.includes(r.path);
    const tr = el('tr', { class: selected ? 'selected' : '', draggable: true, dataset: { path: r.path } });
    const disclosure = r.isDir
      ? el('span.disclosure', { class: r.expanded ? 'open' : '', onmousedown: e => { e.stopPropagation(); e.preventDefault(); if (e.button === 0) call('files_nav', { pane: key, action: 'toggle', path: r.path }); } }, icon('chevron'))
      : el('span.disclosure');
    const name = el('td.name', { style: `padding-left:${8 + r.depth * 16}px` }, disclosure, icon(r.isDir ? 'folder' : 'doc', r.isDir ? 'dir' : 'file'), el('span', { style: 'overflow:hidden;text-overflow:ellipsis' }, r.name), r.loading ? el('span.caption', ' …') : null);
    // A folder has no size: its cell holds the button, shown under the
    // pointer, that shuts every folder inside it.
    const size = r.isDir
      ? el('td.size', el('button.act', { title: t('Shut every folder inside'), onmousedown: e => e.stopPropagation(), onclick: e => { e.stopPropagation(); call('files_nav', { pane: key, action: 'collapse-all', path: r.path }); } }, icon('collapseall')))
      : el('td.size', fmtBytes(r.size));
    // The single pane (the Console's column) is narrow: the date without
    // its time there, the full one in a tooltip.
    const date = el('td.date', fmtFileDate(r.modified, single));
    if (single && r.modified !== null && r.modified !== undefined) date.title = fmtFileDate(r.modified);
    tr.append(name, date, size);
    tr.onmousedown = e => {
      if (e.button !== 0) return;
      pressed = e.shiftKey || e.metaKey || e.ctrlKey ? null : { path: r.path, x: e.clientX, y: e.clientY, detail: e.detail };
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
    tr.ondblclick = () => { if (r.isDir) call('files_set_root', { pane: key, path: r.path }); else if (onOpen) onOpen(r, true); else if (p.source.kind === 'local') invoke('open_path', { path: r.path, editor: false }); };
    // A row's menu: what the buttons above do to the rows selected, the
    // row clicked when it was not. A new folder goes into the folder
    // clicked, or beside the file.
    tr.oncontextmenu = e => {
      e.preventDefault();
      const paths = selected ? p.selected : [r.path];
      if (!selected) { lastClicked[key] = r.path; call('files_select', { pane: key, paths }); }
      const one = paths.length === 1;
      const local = p.source.kind === 'local';
      const op = single ? null : snap.panes[key === 'left' ? 'right' : 'left'];
      menuAt(e.clientX, e.clientY, [
        one && r.isDir ? { title: t('Open'), icon: 'folder', run: () => call('files_set_root', { pane: key, path: r.path }) } : null,
        one && !r.isDir && onOpen ? { title: t('Preview'), icon: 'doc', run: () => onOpen(r, true) } : null,
        one && !r.isDir && !onOpen && local ? { title: t('Open'), icon: 'doc', run: () => invoke('open_path', { path: r.path, editor: false }) } : null,
        one && r.isDir && onUpload ? { title: t('Upload here…'), icon: 'arrowupcircle', run: () => onUpload(r.path) } : null,
        one && r.isDir && onShell ? { title: t('Shell here'), icon: 'terminal', run: () => onShell(r.path) } : null,
        onDownload ? { title: one ? t('Download…') : t('Download {n} items…', { n: paths.length }), icon: 'arrowdowncircle', run: () => onDownload(paths) } : null,
        op ? { title: t('Copy to {where}', { where: op.title }), icon: key === 'left' ? 'arrowright' : 'arrowleft',
          disabled: JSON.stringify(p.source) === JSON.stringify(op.source) || !op.root, run: () => call('files_copy_selection', { pane: key }) } : null,
        { sep: true },
        ...createItems(key),
        { sep: true },
        { title: local ? t('Move to Trash') : t('Delete…'), icon: 'trash', run: () => remove(key, paths) },
        { sep: true },
        refreshItem(key)]);
    };
    tr.ondragstart = e => {
      const paths = selected ? p.selected : [r.path];
      const rows = paths.map(path => ({ source: p.source, path }));
      e.dataTransfer.setData(DRAG, JSON.stringify(rows));
      e.dataTransfer.effectAllowed = 'copy';
      clearTimeout(dragForgets);
      dragged = rows;
    };
    tr.ondragend = () => { clearTimeout(dragForgets); dragForgets = setTimeout(() => { dragged = null; }, 1000); };
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
      const rows = dropRows(e);
      if (!rows) return;
      e.preventDefault();
      e.stopPropagation();
      await call('files_drop', { pane: key, dragged: rows, onto });
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

  // What the menus share with the buttons.
  const createItems = key => [
    { title: t('New folder…'), icon: 'folderplus', run: () => create(key, true) },
    { title: t('New file…'), icon: 'docplus', run: () => create(key, false) }];
  const refreshItem = key => ({ title: t('Refresh'), icon: 'refresh', run: () => call('files_nav', { pane: key, action: 'reload' }) });

  async function create(key, directory) {
    const target = await call('files_target', { pane: key });
    const r = await dialog({ title: directory ? t('New Folder') : t('New File'), message: t('It goes into {dir}', { dir: target }), input: true,
      preset: directory ? t('untitled folder') : t('untitled.txt'), action: t('Create') });
    if (r.ok && r.value) call('files_create', { pane: key, name: r.value, directory });
  }

  async function remove(key, paths = snap.panes[key].selected) {
    const p = snap.panes[key];
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
      const list = el('div.history', { style: `height:${single ? Math.min(transferHeight, 120) : transferHeight}px` });
      if (!s.transfers.length) list.append(el('div.empty', t('Nothing has been copied yet.')));
      for (const rec of [...s.transfers].reverse()) {
        const r = el('div.trow', { title: `${sourceTitle(rec.job.from)}:${rec.job.source} → ${sourceTitle(rec.job.to)}:${rec.job.destination}\n${statusText(rec)}` },
          stateIcon(rec), el('span', rec.name),
          el('span.where', `${sourceTitle(rec.job.from)}:${rec.job.source}  →  ${sourceTitle(rec.job.to)}:${rec.job.destination}`));
        if (rec.state.kind === 'running' && rec.progress != null) r.append(el('div.bar', el('div', { style: `width:${Math.round(rec.progress * 100)}%` })), el('span.status', `${Math.round(rec.progress * 100)}%`));
        else r.append(el('span.status', { class: rec.state.kind === 'failed' ? 'err' : '' }, statusText(rec)));
        if (s.canRetry.includes(rec.id)) {
          const goOn = rec.canResume && rec.state.kind !== 'skipped';
          r.append(el('button.borderless', { title: goOn ? t('Continue where it stopped') : t('Try Again'), onclick: () => call('files_transfer', { action: 'retry', id: rec.id }) }, icon(goOn ? 'play' : 'refresh')));
        }
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

  /// The folder button's popup: the pane's path at the top, to edit and
  /// confirm with Return, and under it the tree of that path, where a
  /// click shows the folder as the pane's root.
  function folderMenu(anchor, key, p) {
    folderPicker(anchor, { where: p.title, root: p.root, home: p.home, list: path => call('files_subdirectories', { pane: key, path }), pick: path => call('files_set_root', { pane: key, path }) });
  }

// MARK: render

  /// The pane showing the host, when one does.
  function hostPane() {
    for (const k of ['left', 'right']) { const p = snap.panes[k]; if (p && p.source.kind === 'host' && p.source.host === host) return k; }
    return null;
  }

  function render(s) {
    snap = s;
    const keys = single ? [hostPane()].filter(Boolean) : ['left', 'right'];
    if (single && !keys.length) { call('files_set_source', { pane: 'left', source: { kind: 'host', host } }); return; }
    // Keep the scroll positions and the focus across a redraw. A pane
    // whose root changed remembers where its tree was, per root, and a
    // root shown again (Back) is scrolled to where it was once its rows
    // are there.
    const scrolls = {};
    for (const k of keys) {
      const tr = root.querySelector(`.pane[data-pane=${k}] .tree`);
      if (!tr) continue;
      if (shownRoot[k] === s.panes[k].root) scrolls[k] = tr.scrollTop;
      else { if (shownRoot[k]) scrollAt[k][shownRoot[k]] = tr.scrollTop; scrollDue[k] = scrollAt[k][s.panes[k].root] ? { root: s.panes[k].root, top: scrollAt[k][s.panes[k].root] } : null; }
    }
    const focused = document.activeElement && document.activeElement.closest('.pane') ? document.activeElement.closest('.pane').dataset.pane : null;
    clear(root);
    root.append(el('div.panes', keys.map(k => pane(k, s.panes[k]))), transfers());
    for (const k of keys) {
      shownRoot[k] = s.panes[k].root;
      const tr = root.querySelector(`.pane[data-pane=${k}] .tree`);
      if (!tr) continue;
      if (scrolls[k]) tr.scrollTop = scrolls[k];
      const due = scrollDue[k];
      if (due && due.root === s.panes[k].root && tr.scrollHeight - tr.clientHeight >= due.top) { tr.scrollTop = due.top; scrollDue[k] = null; }
      else if (due && due.root !== s.panes[k].root) scrollDue[k] = null;
    }
    if (focused) { const tr = root.querySelector(`.pane[data-pane=${focused}] .tree`); if (tr) tr.focus({ preventScroll: true }); }
    if (s.question) askReplace(s.question);
  }

  /// The webview's own drag events: files dragged in from the desktop
  /// land in the pane under the cursor; a drag of rows begun here (no
  /// paths in the event) lands where the HTML drop would have, on a
  /// pane or on what `dropOutside` names.
  function nativeDrops() {
    const w = window.__TAURI__.webview.getCurrentWebview();
    w.onDragDropEvent(e => {
      // A view out of the page (another host's, kept while the Console
      // shows a host) leaves the drop to the one in it.
      if (!root.isConnected) return;
      const p = e.payload;
      for (const n of document.querySelectorAll('.droptarget')) n.classList.remove('droptarget');
      if (p.type === 'over') {
        const target = targetAt(p.position);
        if (target) target.node.classList.add('droptarget');
      } else if (p.type === 'drop') {
        const target = targetAt(p.position);
        if (!target) return;
        if (p.paths.length) { if (!target.drop) call('files_drop_local', { pane: target.pane, paths: p.paths, onto: target.path }); }
        else {
          const rows = takeDrag();
          if (rows && target.drop) target.drop(rows);
          else if (rows) call('files_drop', { pane: target.pane, dragged: rows, onto: target.path });
        }
      }
    });
  }

    function targetAt(position) {
    const scale = REAL_DPR;
    const node = document.elementFromPoint(position.x / scale, position.y / scale);
    if (!node) return null;
    const paneEl = node.closest('.pane');
    if (!paneEl) return dropOutside ? dropOutside(node) : null;
    const tr = node.closest('tr[data-path]');
    return { pane: paneEl.dataset.pane, path: tr ? tr.dataset.path : null, node: tr || paneEl.querySelector('.tree') };
  }


  return { render, nativeDrops, hostPane, snapshot: () => snap, setTransferHeight: h => { transferHeight = h; } };
}
