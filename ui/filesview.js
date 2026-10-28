// The view of a host's files, shared by the Files window (two panes, any
// machine in either) and the Console's Files tab (one pane, the host):
// the trees, what a drag or a drop does, the folder menu, and the copies
// underneath. The model is the app's, one per host (`files_*`), so the
// window and the tab show the same state.
import { invoke, listen, t, el, clear, icon, fmtBytes, fmtTime, fmtFileDate, dialog, toast, menu, menuAt, pointAnchor, popup, smoothWheel, REAL_DPR, LOCAL, hostName } from './shellder.js';

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

// Icons drawn once per name and copied after: a row has two or three, and
// icon() parses each one's markup anew.
const iconCopies = new Map();
function ico(name, cls) {
  const k = `${name}.${cls || ''}`;
  let i = iconCopies.get(k);
  if (!i) iconCopies.set(k, i = icon(name, cls));
  return i.cloneNode(true);
}

/// Past this many rows a tree draws only the ones in view, and a few
/// either side; the rest is room to scroll through.
const WINDOWED_FROM = 300;
const OVERSCAN = 30;
/// A row's height until one is measured, about what app.css makes it.
const ROW_HEIGHT = 22;

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
export function createFilesView(root, { host, single = false, transferHeight: initialHeight = 160, onUpload = null, onDownload = null, onWindow = null, onOpen = null, onShell = null, dropOutside = null } = {}) {
  let snap = null;
  let transferHeight = initialHeight;
  let lastClicked = { left: null, right: null };
  let pressed = null;     // the row the mouse went down on, for the click it may end as
  let shownRoot = { left: null, right: null };   // the root each pane last drew
  const scrollAt = { left: {}, right: {} };      // root -> where the tree was scrolled to, while it showed
  const scrollDue = { left: null, right: null }; // {root, top}: to scroll to once that root's rows are there
  let questionShown = null;
  // A snapshot that came while the view was out of the page (the Console
  // keeps one per host it has shown): drawn when it is back.
  let stale = false;
  const call = (cmd, args) => invoke(cmd, { host, ...args });
  const other = key => key === 'left' ? 'right' : 'left';
  const hostSource = () => host === LOCAL ? { kind: 'local' } : { kind: 'host', host };

  // What is on the page: the panes by key, each with its elements and what
  // they were drawn from; the box they are in, and the copies under it.
  let panes = {};
  let layout = null;
  const panesBox = el('div.panes');
  const transfersBox = el('div.transfers');
  const watcher = new ResizeObserver(entries => {
    for (const e of entries) {
      if (e.target === root) { if (stale && root.isConnected) { stale = false; draw(); } continue; }
      const v = Object.values(panes).find(x => x.tree === e.target);
      if (v && v.windowed) drawWindow(v, false);
    }
  });
  watcher.observe(root);

  // MARK: panes

  function sourceTitle(src) { return src.kind === 'local' ? t('Local') : src.host; }

  /// What a pane has selected, in the order of its rows.
  function selection(v) {
    if (!v.selected.size) return [];
    return v.rows.filter(r => v.selected.has(r.path)).map(r => r.path);
  }

  function rowAt(v, path) {
    const i = v.index.get(path);
    return i === undefined ? null : v.rows[i];
  }

  function topBar(key, p) {
    const op = snap.panes[other(key)];
    const local = p.source.kind === 'local';
    return el('div.top', { 'data-tauri-drag-region': !single },
      key === 'left' && !single ? el('div.titlebar-pad', { 'data-tauri-drag-region': true }) : null,
      // The single pane (the Console's Files tab) is its host's: the
      // window says which, the row does not.
      single ? null : el('button.source', { title: t('Show another machine in this pane: a host with a master up through it, any other on a login of its own'), onclick: e => menu(e.currentTarget, [
          { title: t('Local'), icon: 'laptop', run: () => call('files_set_source', { pane: key, source: { kind: 'local' } }) },
          ...(snap.hosts.length ? [{ sep: true }] : []),
          ...snap.hosts.map(h => ({ title: h, icon: 'server', run: () => call('files_set_source', { pane: key, source: { kind: 'host', host: h } }) })),
        ]) }, icon(local ? 'laptop' : 'server'), el('span.label', p.title), icon('chevrondown')),
      el('button', { title: t('Go to a folder on {where}', { where: p.title }), onclick: e => folderMenu(e.currentTarget, key) }, icon('folder')),
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
  }

  function header(key, p) {
    return el('tr', ['name', 'modified', 'size'].map(col => {
      const titles = { name: t('Name'), modified: t('Date Modified'), size: t('Size') };
      const active = p.sort.key === col;
      return el('th.sortable', { style: col === 'name' ? '' : col === 'size' ? `width:${single ? 64 : 90}px;text-align:right` : `width:${single ? 100 : 150}px`,
        onclick: () => call('files_set_sort', { pane: key, sort: { key: col, ascending: active ? !p.sort.ascending : col === 'name' } }) },
        titles[col], active ? (p.sort.ascending ? ' ↑' : ' ↓') : '');
    }));
  }

  /// A pane's elements, made once and kept while it shows: its tree keeps
  /// its scroll position and the keyboard across snapshots, and what the
  /// mouse does on a row is handled by the tree, for whichever rows are
  /// drawn at the time.
  function makePane(key) {
    const tree = el('div.tree');
    const table = el('table');
    const thead = el('thead');
    const body = el('tbody');
    table.append(thead, body);
    tree.append(table);
    const v = { key, tree, table, thead, body, top: el('div.top'), error: el('div.error', { style: 'display:none' }), path: el('div.pathbar'),
      version: null, topKey: null, sortKey: null, pathKey: null, rows: [], index: new Map(), selected: new Set(),
      windowed: false, start: 0, end: 0, rowHeight: ROW_HEIGHT, frame: 0, mark: null };
    v.el = el('div.pane', { dataset: { pane: key } }, v.top, tree, v.error, v.path);
    const cur = () => snap.panes[key];

    tree.addEventListener('click', e => {
      const act = e.target.closest('button.act');
      if (act) { e.stopPropagation(); const tr = act.closest('tr[data-path]'); if (tr) call('files_nav', { pane: key, action: 'collapse-all', path: tr.dataset.path }); return; }
      if (e.target === tree || e.target === table || e.target.closest('tr.gap')) call('files_select', { pane: key, paths: [] });
    });
    tree.addEventListener('mousedown', e => {
      const tr = e.target.closest('tr[data-path]');
      if (!tr || e.target.closest('button.act')) return;
      const r = rowAt(v, tr.dataset.path);
      if (!r) return;
      if (r.isDir && e.target.closest('.disclosure')) {
        e.preventDefault();
        if (e.button === 0) call('files_nav', { pane: key, action: 'toggle', path: r.path });
        return;
      }
      if (e.button !== 0) return;
      pressed = e.shiftKey || e.metaKey || e.ctrlKey ? null : { path: r.path, x: e.clientX, y: e.clientY, detail: e.detail };
      const selected = v.selected.has(r.path);
      let paths;
      const from = e.shiftKey && lastClicked[key] !== null ? v.index.get(lastClicked[key]) : undefined;
      if (from !== undefined) {
        const [a, b] = [from, v.index.get(r.path)].sort((x, y) => x - y);
        paths = v.rows.slice(a, b + 1).map(x => x.path);
      } else if (e.metaKey || e.ctrlKey) {
        paths = selected ? selection(v).filter(x => x !== r.path) : [...selection(v), r.path];
        lastClicked[key] = r.path;
      } else {
        if (selected) return; // keep the selection for a drag of several rows
        paths = [r.path];
        lastClicked[key] = r.path;
      }
      // Drawn at once; the snapshot that follows says the same.
      v.selected = new Set(paths);
      markSelection(v);
      call('files_select', { pane: key, paths });
    });
    // A click on a file, judged when the mouse comes up, so that a drag
    // that starts on it is not one. The file shows for a look; the second
    // press of a double click does not. A folder's row only selects: its
    // disclosure opens and closes it.
    tree.addEventListener('mouseup', e => {
      const was = pressed;
      pressed = null;
      if (!was || e.button !== 0 || was.detail !== 1 || !onOpen) return;
      const tr = e.target.closest('tr[data-path]');
      if (!tr || tr.dataset.path !== was.path || Math.abs(e.clientX - was.x) > 4 || Math.abs(e.clientY - was.y) > 4) return;
      const r = rowAt(v, was.path);
      if (r && !r.isDir) onOpen(r, false);
    });
    tree.addEventListener('dblclick', e => {
      const tr = e.target.closest('tr[data-path]');
      const r = tr && rowAt(v, tr.dataset.path);
      if (!r || e.target.closest('button.act') || e.target.closest('.disclosure') && r.isDir) return;
      if (r.isDir) call('files_set_root', { pane: key, path: r.path });
      else if (onOpen) onOpen(r, true);
      else if (cur().source.kind === 'local') invoke('open_path', { path: r.path, editor: false });
    });
    tree.addEventListener('contextmenu', e => {
      e.preventDefault();
      const tr = e.target.closest('tr[data-path]');
      const r = tr && rowAt(v, tr.dataset.path);
      if (r) rowMenu(v, r, e); else blankMenu(v, e);
    });
    tree.addEventListener('dragstart', e => {
      const tr = e.target.closest && e.target.closest('tr[data-path]');
      if (!tr) return;
      const path = tr.dataset.path;
      const paths = v.selected.has(path) ? selection(v) : [path];
      const rows = paths.map(p => ({ source: cur().source, path: p }));
      e.dataTransfer.setData(DRAG, JSON.stringify(rows));
      e.dataTransfer.effectAllowed = 'copy';
      clearTimeout(dragForgets);
      dragged = rows;
    });
    tree.addEventListener('dragend', () => { clearTimeout(dragForgets); dragForgets = setTimeout(() => { dragged = null; }, 1000); });
    // What a drop here does: rows from the other side are copied over,
    // into the folder under the pointer, beside the file, or into the
    // folder shown.
    const unmark = () => { if (v.mark) v.mark.classList.remove('droptarget'); v.mark = null; };
    tree.addEventListener('dragover', e => {
      if (!e.dataTransfer.types.includes(DRAG)) return;
      e.preventDefault();
      e.stopPropagation();
      e.dataTransfer.dropEffect = 'copy';
      const node = e.target.closest('tr[data-path]') || tree;
      if (node !== v.mark) { unmark(); node.classList.add('droptarget'); v.mark = node; }
    });
    tree.addEventListener('dragleave', e => { if (!tree.contains(e.relatedTarget)) unmark(); });
    tree.addEventListener('drop', async e => {
      unmark();
      const rows = dropRows(e);
      if (!rows) return;
      e.preventDefault();
      e.stopPropagation();
      const tr = e.target.closest('tr[data-path]');
      await call('files_drop', { pane: key, dragged: rows, onto: tr ? tr.dataset.path : null });
    });
    tree.addEventListener('keydown', e => {
      const sel = selection(v);
      if (e.key === 'Backspace' && (e.metaKey || e.ctrlKey) && sel.length) { e.preventDefault(); remove(key); }
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        const at = sel.length ? v.index.get(lastClicked[key] !== null && v.selected.has(lastClicked[key]) ? lastClicked[key] : sel[sel.length - 1]) : undefined;
        const from = at === undefined ? -1 : at;
        const next = Math.max(0, Math.min(v.rows.length - 1, from + (e.key === 'ArrowDown' ? 1 : -1)));
        const r = v.rows[next];
        if (r) {
          lastClicked[key] = r.path;
          v.selected = new Set([r.path]);
          reveal(v, next);
          markSelection(v);
          call('files_select', { pane: key, paths: [r.path] });
        }
      }
      if (e.key === 'ArrowRight' && sel.length === 1) call('files_nav', { pane: key, action: 'expand', path: sel[0] });
      if (e.key === 'ArrowLeft' && sel.length === 1) call('files_nav', { pane: key, action: 'collapse', path: sel[0] });
      if (e.key === 'Enter' && sel.length === 1) { const r = rowAt(v, sel[0]); if (r && r.isDir) call('files_set_root', { pane: key, path: r.path }); else if (r && onOpen) onOpen(r, true); }
    });
    tree.addEventListener('scroll', () => {
      if (v.windowed && !v.frame) v.frame = requestAnimationFrame(() => { v.frame = 0; drawWindow(v, false); });
    }, { passive: true });
    tree.tabIndex = 0;
    smoothWheel(tree);
    watcher.observe(tree);
    return v;
  }

  /// One row, without handlers of its own: the tree's take its clicks.
  function rowEl(v, r) {
    const disclosure = el('span.disclosure', { class: r.isDir && r.expanded ? 'open' : '' }, r.isDir ? ico('chevron') : null);
    const name = el('td.name', { style: `padding-left:${8 + r.depth * 16}px`, title: r.error || null }, disclosure, ico(r.isDir ? 'folder' : 'doc', r.isDir ? 'dir' : 'file'),
      el('span', { style: 'overflow:hidden;text-overflow:ellipsis' }, r.name), r.loading ? el('span.caption', ' …') : null, r.error ? ico('warning', 'orange') : null);
    // A folder has no size: its cell holds the button, shown under the
    // pointer, that shuts every folder inside it.
    const size = r.isDir
      ? el('td.size', el('button.act', { title: t('Shut every folder inside') }, ico('collapseall')))
      : el('td.size', fmtBytes(r.size));
    // The single pane (the Console's column) is narrow: the date without
    // its time there, the full one in a tooltip.
    const date = el('td.date', fmtFileDate(r.modified, single));
    if (single && r.modified !== null && r.modified !== undefined) date.title = fmtFileDate(r.modified);
    return el('tr', { class: v.selected.has(r.path) ? 'selected' : '', draggable: true, dataset: { path: r.path } }, name, date, size);
  }

  /// Room for rows not drawn, above or below those that are.
  const gap = px => el('tr.gap', el('td', { colSpan: 3, style: `height:${px}px;padding:0;background:none` }));

  /// The rows, all of them or those in view: drawn again from scratch,
  /// for rows that changed.
  function drawRows(v) {
    v.windowed = v.rows.length > WINDOWED_FROM;
    if (!v.windowed) {
      const f = document.createDocumentFragment();
      for (const r of v.rows) f.append(rowEl(v, r));
      v.body.replaceChildren(f);
      v.start = 0; v.end = v.rows.length;
      measure(v);
      return;
    }
    drawWindow(v, true);
  }

  /// The part of a long tree in view, and some of either side, between
  /// two gaps as high as the rows they stand for. Drawn again when the
  /// view nears an end of what is drawn. The first row drawn after a gap
  /// keeps its place among odd and even, so the stripes do not shift.
  function drawWindow(v, force) {
    if (!v.windowed) return;
    const h = v.rowHeight;
    const n = v.rows.length;
    const view = v.tree.clientHeight || 800;
    const top = v.tree.scrollTop;
    const first = Math.max(0, Math.floor(top / h));
    const last = Math.min(n, Math.ceil((top + view) / h));
    if (!force && first >= v.start && last <= v.end) return;
    // After the gap, the first row is the tbody's second child: an odd
    // index keeps the stripes where the whole list has them.
    let start = Math.max(0, first - OVERSCAN);
    if (start && start % 2 === 0) start--;
    const end = Math.min(n, last + OVERSCAN);
    const f = document.createDocumentFragment();
    if (start) f.append(gap(start * h));
    for (let i = start; i < end; i++) f.append(rowEl(v, v.rows[i]));
    if (end < n) f.append(gap((n - end) * h));
    v.body.replaceChildren(f);
    v.start = start; v.end = end;
    if (measure(v)) drawWindow(v, true);
  }

  /// A row's height as drawn: what the gaps of a long tree are made of.
  /// True when it is not what they were made with. Measured from the
  /// second row on, since the first carries half of the header's border.
  function measure(v) {
    const drawn = v.body.querySelectorAll('tr[data-path]');
    if (drawn.length < 3) return false;
    const h = (drawn[drawn.length - 1].getBoundingClientRect().bottom - drawn[1].getBoundingClientRect().top) / (drawn.length - 1);
    if (!h || Math.abs(h - v.rowHeight) < 0.01) return false;
    v.rowHeight = h;
    return v.windowed;
  }

  /// Scroll the tree so that row `i` is in view, below the sticky header.
  function reveal(v, i) {
    const head = v.thead.getBoundingClientRect().height;
    const top = i * v.rowHeight;
    const t = v.tree;
    if (top < t.scrollTop) t.scrollTop = top;
    else if (head + top + v.rowHeight > t.scrollTop + t.clientHeight) t.scrollTop = head + top + v.rowHeight - t.clientHeight;
    drawWindow(v, false);
  }

  /// The selection drawn on the rows there are, nothing else redrawn.
  function markSelection(v) {
    for (const tr of v.body.children) if (tr.dataset.path) tr.classList.toggle('selected', v.selected.has(tr.dataset.path));
  }

  /// Draw a pane from its part of the snapshot: what changed, and only
  /// that. The rows come again when the pane's version moved, the
  /// selection alone is put on the rows drawn, and the bar above when
  /// what it shows changed.
  function updatePane(v, p) {
    const key = v.key;
    const op = snap.panes[other(key)];
    const topKey = JSON.stringify([p.version, p.selected.length > 0, !!snap.canCopy[key], op && op.title]);
    if (topKey !== v.topKey) { const top = topBar(key, p); v.top.replaceWith(top); v.top = top; v.topKey = topKey; }
    const sortKey = JSON.stringify(p.sort);
    if (sortKey !== v.sortKey) { v.thead.replaceChildren(header(key, p)); v.sortKey = sortKey; }
    const pathKey = JSON.stringify([p.source, p.root]);
    if (pathKey !== v.pathKey) { const bar = pathBar(key, p); v.path.replaceWith(bar); v.path = bar; v.pathKey = pathKey; }
    if (p.error) { v.error.style.display = ''; v.error.replaceChildren(icon('warning', 'orange'), el('span.selectable', p.error)); }
    else v.error.style.display = 'none';
    const sameSelection = p.selected.length === v.selected.size && p.selected.every(x => v.selected.has(x));
    if (p.version === v.version) {
      if (!sameSelection) { v.selected = new Set(p.selected); markSelection(v); }
      return;
    }
    v.version = p.version;
    v.rows = p.rows;
    v.index = new Map(p.rows.map((r, i) => [r.path, i]));
    v.selected = new Set(p.selected);
    // A pane whose root changed remembers where its tree was, per root,
    // and a root shown again (Back) is scrolled to where it was once its
    // rows are there.
    const tree = v.tree;
    const moved = shownRoot[key] !== p.root;
    if (moved) {
      if (shownRoot[key] !== null) scrollAt[key][shownRoot[key]] = tree.scrollTop;
      scrollDue[key] = scrollAt[key][p.root] ? { root: p.root, top: scrollAt[key][p.root] } : null;
      shownRoot[key] = p.root;
      tree.scrollTop = 0;
    }
    drawRows(v);
    const due = scrollDue[key];
    if (due && due.root === p.root && tree.scrollHeight - tree.clientHeight >= due.top) { tree.scrollTop = due.top; scrollDue[key] = null; drawWindow(v, false); }
    else if (due && due.root !== p.root) scrollDue[key] = null;
  }

  // A row's menu: what the buttons above do to the rows selected, the
  // row clicked when it was not. A new folder goes into the folder
  // clicked, or beside the file.
  function rowMenu(v, r, e) {
    const key = v.key;
    const p = snap.panes[key];
    const selected = v.selected.has(r.path);
    const paths = selected ? selection(v) : [r.path];
    if (!selected) { lastClicked[key] = r.path; v.selected = new Set(paths); markSelection(v); call('files_select', { pane: key, paths }); }
    const one = paths.length === 1;
    const local = p.source.kind === 'local';
    const op = single ? null : snap.panes[other(key)];
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
  }

  // The blank's menu: what the buttons above do to the folder shown.
  // Like a click there it leaves nothing selected, so a new folder goes
  // into the folder shown.
  function blankMenu(v, e) {
    const key = v.key;
    const p = snap.panes[key];
    if (v.selected.size) { v.selected = new Set(); markSelection(v); call('files_select', { pane: key, paths: [] }); }
    const at = pointAnchor(e.clientX, e.clientY);
    menu(at, [
      onUpload ? { title: t('Upload here…'), icon: 'arrowupcircle', run: () => onUpload(p.root) } : null,
      onShell ? { title: t('Shell here'), icon: 'terminal', run: () => onShell(p.root) } : null,
      { sep: true },
      ...createItems(key),
      { sep: true },
      { title: t('Go to folder…'), icon: 'folder', run: () => folderMenu(at, key) },
      { title: t('Back'), icon: 'arrowleft', disabled: !p.canGoBack, run: () => call('files_nav', { pane: key, action: 'back' }) },
      { title: t('Up one directory'), icon: 'arrowup', disabled: p.root === '/', run: () => call('files_nav', { pane: key, action: 'up' }) },
      { sep: true },
      { title: p.showHidden ? t('Hide hidden files') : t('Show hidden files'), icon: p.showHidden ? 'eyeoff' : 'eye', run: () => call('files_nav', { pane: key, action: 'toggle-hidden' }) },
      refreshItem(key),
      { sep: true },
      onWindow ? { title: t('Open in the Files window'), icon: 'link', run: onWindow } : null]);
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

  async function remove(key, paths = panes[key] ? selection(panes[key]) : snap.panes[key].selected) {
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

  /// A progress bar and its percentage, which the progress events move
  /// in place (`showProgress`).
  const bar = (rec, cls) => [el('div.bar', el('div', { dataset: { progress: rec.id }, style: `width:${Math.round(rec.progress * 100)}%` })),
    el(cls, { dataset: { percent: rec.id } }, `${Math.round(rec.progress * 100)}%`)];

  function showProgress(id, fraction) {
    for (const b of transfersBox.querySelectorAll(`[data-progress="${id}"]`)) b.style.width = `${Math.round(fraction * 100)}%`;
    for (const s of transfersBox.querySelectorAll(`[data-percent="${id}"]`)) s.textContent = `${Math.round(fraction * 100)}%`;
  }

  let stripKey, listKey;
  const list = el('div.history');
  const handle = el('div.drag-handle');
  handle.onmousedown = e => {
    const startY = e.clientY, startH = list.offsetHeight;
    const move = ev => { transferHeight = Math.min(600, Math.max(60, startH - (ev.clientY - startY))); list.style.height = `${transferHeight}px`; };
    const up = () => { document.removeEventListener('mousemove', move); document.removeEventListener('mouseup', up); invoke('set_setting', { key: 'transferPanelHeight', value: transferHeight }).catch(e => toast(String(e), true)); };
    document.addEventListener('mousemove', move); document.addEventListener('mouseup', up);
  };
  const strip = el('div.strip');

  /// The copies under the panes: the strip, and the list when it is
  /// open. Each drawn again only when what it shows changed, a running
  /// copy's progress aside, which is moved where it is.
  function drawTransfers() {
    const s = snap;
    const canClear = s.transfers.some(r => !['waiting', 'running'].includes(r.state.kind));
    const nowStrip = JSON.stringify([s.active && { ...s.active, progress: s.active.progress != null }, s.queued, s.notice, s.noticeIsError,
      s.lastFinished && [s.lastFinished.id, s.lastFinished.state], s.showHistory, canClear]);
    if (nowStrip !== stripKey) {
      stripKey = nowStrip;
      const toggle = el('button', { title: t('Every copy this window has run'), onclick: () => call('files_transfer', { action: 'history', on: !s.showHistory }) }, icon('chevron', s.showHistory ? 'open' : ''));
      toggle.firstChild.style.transform = s.showHistory ? 'rotate(90deg)' : '';
      const parts = [toggle, el('span', { style: 'font-weight:600;color:var(--secondary)' }, t('Transfers'))];
      if (s.active) {
        const a = s.active;
        parts.push(stateIcon(a), el('span', { style: 'overflow:hidden;text-overflow:ellipsis;white-space:nowrap' }, `${a.name} → ${a.job.destination}`));
        if (a.progress != null) parts.push(...bar(a, 'span.secondary'));
        else parts.push(el('div.bar.indeterminate', el('div')));
        parts.push(el('button', { title: t('Stop this copy'), onclick: () => call('files_transfer', { action: 'cancel' }) }, icon('xcircle')));
      } else if (s.notice) {
        parts.push(icon(s.noticeIsError ? 'warning' : 'circlecheck', s.noticeIsError ? 'orange' : ''), el('span.selectable', { class: s.noticeIsError ? 'red' : 'secondary', style: 'overflow:hidden;text-overflow:ellipsis;white-space:nowrap' }, s.notice));
      } else if (s.lastFinished) {
        parts.push(stateIcon(s.lastFinished), el('span.secondary', { style: 'overflow:hidden;text-overflow:ellipsis;white-space:nowrap' }, `${s.lastFinished.name} → ${s.lastFinished.job.destination}`));
      }
      if (s.queued) parts.push(el('span.secondary', t('+{n} waiting', { n: s.queued })),
        el('button', { title: t('Take every waiting copy off the queue'), onclick: () => call('files_transfer', { action: 'cancel-all-waiting' }) }, icon('close')));
      parts.push(el('div.spacer'), el('button', { disabled: !canClear, onclick: () => call('files_transfer', { action: 'clear' }) }, t('Clear')));
      strip.replaceChildren(...parts);
    }
    const nowList = s.showHistory ? JSON.stringify([s.transfers.map(r => [r.id, r.state, r.total, r.endedAt, r.canResume, r.progress != null]), s.canRetry]) : null;
    if (nowList !== listKey) {
      listKey = nowList;
      if (s.showHistory) {
        const scrolled = list.scrollTop;
        list.style.height = `${single ? Math.min(transferHeight, 120) : transferHeight}px`;
        const rows = [];
        if (!s.transfers.length) rows.push(el('div.empty', t('Nothing has been copied yet.')));
        for (const rec of [...s.transfers].reverse()) {
          const r = el('div.trow', { title: `${sourceTitle(rec.job.from)}:${rec.job.source} → ${sourceTitle(rec.job.to)}:${rec.job.destination}\n${statusText(rec)}` },
            stateIcon(rec), el('span', rec.name),
            el('span.where', `${sourceTitle(rec.job.from)}:${rec.job.source}  →  ${sourceTitle(rec.job.to)}:${rec.job.destination}`));
          if (rec.state.kind === 'running' && rec.progress != null) r.append(...bar(rec, 'span.status'));
          else r.append(el('span.status', { class: rec.state.kind === 'failed' ? 'err' : '' }, statusText(rec)));
          if (rec.state.kind === 'waiting') r.append(el('button.borderless', { title: t('Take this copy off the queue'), onclick: () => call('files_transfer', { action: 'cancel-waiting', id: rec.id }) }, icon('close')));
          if (s.canRetry.includes(rec.id)) {
            const goOn = rec.canResume && rec.state.kind !== 'skipped';
            r.append(el('button.borderless', { title: goOn ? t('Continue where it stopped') : t('Try Again'), onclick: () => call('files_transfer', { action: 'retry', id: rec.id }) }, icon(goOn ? 'play' : 'refresh')));
          }
          rows.push(r);
        }
        list.replaceChildren(...rows);
        list.scrollTop = scrolled;
        transfersBox.replaceChildren(handle, strip, list);
      } else transfersBox.replaceChildren(strip);
    }
    for (const rec of [s.active, ...s.transfers]) if (rec && rec.progress != null) showProgress(rec.id, rec.progress);
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
  function folderMenu(anchor, key) {
    const p = snap.panes[key];
    folderPicker(anchor, { where: p.title, root: p.root, home: p.home, list: path => call('files_subdirectories', { pane: key, path }), pick: path => call('files_set_root', { pane: key, path }) });
  }

  // MARK: render

  /// The pane showing the host, when one does.
  function hostPane() {
    if (!snap) return null;
    for (const k of ['left', 'right']) {
      const p = snap.panes[k];
      if (p && (host === LOCAL ? p.source.kind === 'local' : p.source.kind === 'host' && p.source.host === host)) return k;
    }
    return null;
  }

  /// The single view's host in a pane again: the left one, which the
  /// Files window then shows too. What the view offers when both panes
  /// went elsewhere; a page can ask for it before showing a folder.
  async function showHost() {
    await call('files_set_source', { pane: 'left', source: hostSource() });
    return 'left';
  }

  /// The Files window took both panes elsewhere: the single view says so
  /// and leaves the shared model alone until asked.
  function placeholder() {
    return el('div.pane', { style: 'align-items:center;justify-content:center;gap:10px;padding:16px;text-align:center' },
      el('span.secondary', t('Both panes of the Files window show other machines.')),
      el('button', { onclick: () => showHost().catch(() => {}) }, t('Show {host} here', { host: hostName(host) })));
  }

  /// A snapshot. One older than the one drawn (a call's answer that an
  /// event overtook) is dropped; one that comes while the view is out of
  /// the page is drawn when it is back.
  function render(s) {
    if (snap && s.seq != null && snap.seq != null && s.seq < snap.seq) return;
    snap = s;
    if (!root.isConnected) { stale = true; return; }
    stale = false;
    draw();
  }

  function draw() {
    const s = snap;
    const keys = single ? [hostPane()].filter(Boolean) : ['left', 'right'];
    const now = keys.join(' ');
    if (now !== layout) {
      // Keep the keyboard where it was across the change.
      const focused = document.activeElement && document.activeElement.closest('.pane') ? document.activeElement.closest('.pane').dataset.pane : null;
      for (const v of Object.values(panes)) { watcher.unobserve(v.tree); if (v.frame) cancelAnimationFrame(v.frame); }
      panes = {};
      for (const k of keys) panes[k] = makePane(k);
      shownRoot = { left: null, right: null };
      panesBox.replaceChildren(...(keys.length ? keys.map(k => panes[k].el) : [placeholder()]));
      if (panesBox.parentElement !== root) root.replaceChildren(panesBox, transfersBox);
      layout = now;
      for (const k of keys) updatePane(panes[k], s.panes[k]);
      if (focused && panes[focused]) panes[focused].tree.focus({ preventScroll: true });
    } else {
      for (const k of keys) updatePane(panes[k], s.panes[k]);
    }
    drawTransfers();
    if (s.question) askReplace(s.question);
  }

  /// The running copy's progress, between snapshots: kept in the one
  /// drawn and moved on its bar.
  listen(`files-progress:files-${host.replace(/[^A-Za-z0-9_-]/g, '_')}`, x => {
    if (!snap) return;
    for (const rec of [snap.active, ...snap.transfers]) if (rec && rec.id === x.id) rec.progress = x.progress;
    if (!root.isConnected) { stale = true; return; }
    if (layout !== null) drawTransfers();
  });

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
    if (!paneEl || !paneEl.dataset.pane) return dropOutside ? dropOutside(node) : null;
    const tr = node.closest('tr[data-path]');
    return { pane: paneEl.dataset.pane, path: tr ? tr.dataset.path : null, node: tr || paneEl.querySelector('.tree') };
  }

  return { render, nativeDrops, hostPane, showHost, snapshot: () => snap, setTransferHeight: h => { transferHeight = h; list.style.height = `${single ? Math.min(h, 120) : h}px`; } };
}
