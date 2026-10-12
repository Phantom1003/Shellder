// A file in a tab of the Console, beside the terminals: text as it is,
// Markdown rendered (marked, ui/vendor), a web page in a sandboxed frame,
// a PDF drawn page by page (pdf.js, ui/vendor), an image, a video or a
// piece of music in the system's player; anything else is named for what
// it is. The bytes come the way a listing does, through the host's master
// or its session (`files_read`), up to a limit per kind of file.
import { invoke, t, el, clear, append, icon, fmtBytes } from './shellder.js';

const MB = 1000 * 1000; // as fmtBytes counts
/// How much of a file comes over, by what it is shown as: a text cut
/// short is still readable, the rest is no use incomplete.
export const LIMITS = { text: 16 * MB, markdown: 16 * MB, html: 16 * MB, image: 32 * MB, pdf: 32 * MB, video: 64 * MB, audio: 64 * MB };

const IMAGE = { png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif', webp: 'image/webp', bmp: 'image/bmp', svg: 'image/svg+xml', ico: 'image/x-icon', avif: 'image/avif' };
const MEDIA = { mp4: 'video/mp4', m4v: 'video/mp4', mov: 'video/quicktime', webm: 'video/webm', ogv: 'video/ogg',
  mp3: 'audio/mpeg', m4a: 'audio/mp4', aac: 'audio/aac', wav: 'audio/wav', flac: 'audio/flac', ogg: 'audio/ogg', oga: 'audio/ogg', opus: 'audio/ogg', aiff: 'audio/aiff', aif: 'audio/aiff' };

const extOf = name => (name.match(/\.([^.]+)$/) || [, ''])[1].toLowerCase();

/// What a file is shown as, from its name: 'markdown', 'html', 'pdf',
/// 'image', 'video', 'audio', or null for the rest, which is text unless
/// its bytes say otherwise.
export function kindOf(name) {
  const ext = extOf(name);
  if (ext === 'md' || ext === 'markdown') return 'markdown';
  if (ext === 'html' || ext === 'htm' || ext === 'xhtml') return 'html';
  if (ext === 'pdf') return 'pdf';
  if (IMAGE[ext]) return 'image';
  if (MEDIA[ext]) return MEDIA[ext].startsWith('video/') ? 'video' : 'audio';
  return null;
}

/// Bytes that are text: no NUL in the first 8 KB, and UTF-8 that decodes
/// but for the odd character.
export function looksLikeText(bytes) {
  const head = bytes.subarray(0, 8192);
  if (head.includes(0)) return false;
  const s = new TextDecoder('utf-8').decode(head);
  let bad = 0;
  for (const c of s) if (c === '�') bad++;
  return bad <= s.length / 100;
}

const asText = bytes => new TextDecoder('utf-8').decode(bytes);

// MARK: Markdown

// marked writes HTML; what goes into the page is built from it again,
// element by element, keeping these tags and attributes and nothing
// else: a file on a host is not to run anything in a window that can
// call the app. A link opens in the browser; an image loads from the web
// or from its own data, a relative one shows its alt text.
const MD_TAGS = new Set(['p', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'ul', 'ol', 'li', 'pre', 'code', 'em', 'strong', 'del', 'a', 'img', 'table', 'thead', 'tbody', 'tr', 'th', 'td', 'blockquote', 'hr', 'br', 'input', 'sup', 'sub', 'kbd', 'details', 'summary']);
const MD_DROP = new Set(['script', 'style', 'template', 'iframe', 'object', 'embed', 'svg', 'math', 'head', 'title']);
const MD_ATTRS = { a: ['href', 'title'], img: ['src', 'alt', 'title'], th: ['align'], td: ['align'], ol: ['start'], code: ['class'], input: ['type', 'checked', 'disabled'] };
const linkOk = href => /^(https?:|mailto:)/i.test(href);
const imageOk = src => /^(https?:|data:image\/)/i.test(src);

export function markdownToDom(html) {
  const doc = new DOMParser().parseFromString(html, 'text/html');
  const out = document.createDocumentFragment();
  const walk = (from, into) => {
    for (const n of Array.from(from.childNodes)) {
      if (n.nodeType === Node.TEXT_NODE) { into.append(document.createTextNode(n.data)); continue; }
      if (n.nodeType !== Node.ELEMENT_NODE) continue;
      const tag = n.localName;
      if (MD_DROP.has(tag)) continue;
      if (!MD_TAGS.has(tag)) { walk(n, into); continue; } // an unknown element is its content
      if (tag === 'img') {
        const src = n.getAttribute('src') || '';
        const alt = n.getAttribute('alt') || src;
        if (imageOk(src)) into.append(el('img', { src, alt, title: n.getAttribute('title') || alt }));
        else into.append(el('span.caption', `[${alt}]`));
        continue;
      }
      if (tag === 'input' && n.getAttribute('type') !== 'checkbox') continue;
      const e = document.createElement(tag);
      for (const a of MD_ATTRS[tag] || []) {
        if (!n.hasAttribute(a)) continue;
        const v = n.getAttribute(a);
        if (a === 'href' && !linkOk(v)) continue;
        if (a === 'class' && !/^language-[\w+-]*$/.test(v)) continue;
        e.setAttribute(a, v);
      }
      if (tag === 'input') e.disabled = true;
      if (tag === 'a') {
        const href = e.getAttribute('href');
        if (href) { e.title = href; e.onclick = ev => { ev.preventDefault(); invoke('open_url', { url: href }).catch(() => {}); }; }
      }
      walk(n, e);
      into.append(e);
    }
  };
  walk(doc.body, out);
  return out;
}

// MARK: the preview

/// Draw the file into `root`: a header with the path, the content under
/// it. Returns what the tab needs: focus the content, read it again,
/// and let go of it when the tab closes.
export function createPreview(root, { source, path, name, size }) {
  const body = el('div.body', { tabIndex: 0 });
  const status = el('span.caption');
  let wrap = false;
  let showSource = false;
  const wrapButton = el('button.hidden', { title: t('Wrap long lines'), onclick: () => { wrap = !wrap; wrapButton.classList.toggle('active', wrap); const pre = body.querySelector('pre.text'); if (pre) pre.classList.toggle('wrap', wrap); } }, icon('wrap'));
  const sourceButton = el('button.hidden', { title: t('The file as written, instead of the page it makes'), onclick: () => { showSource = !showSource; sourceButton.classList.toggle('active', showSource); if (got) show(); } }, icon('code'));
  const head = el('div.top', icon('doc'), el('span.path.selectable', { title: path }, path), el('div.spacer'), status, wrapButton, sourceButton,
    el('button', { title: t('Read it again'), onclick: () => load() }, icon('refresh')));
  root.append(head, body);
  let token = 0;         // which load the content on screen belongs to
  let release = null;    // what the content holds: a PDF document, a blob URL
  let got = null;        // the bytes on screen, what they are, whether they were cut

  const letGo = () => { if (release) release(); release = null; };
  const message = (text, isError) => { clear(body).append(el('div.message', icon(isError ? 'warning' : 'info', isError ? 'orange' : ''), el('span.selectable', text))); };
  const tooBig = limit => message(t('{name} is {size}, more than the {limit} a preview can show.', { name, size: fmtBytes(size), limit: fmtBytes(limit) }), true);
  const blobUrl = (bytes, type) => { const url = URL.createObjectURL(new Blob([bytes], { type })); release = () => URL.revokeObjectURL(url); return url; };

  async function load() {
    const mine = ++token;
    letGo();
    got = null;
    let kind = kindOf(name);
    const limit = LIMITS[kind || 'text'];
    wrapButton.classList.add('hidden');
    sourceButton.classList.add('hidden');
    status.textContent = fmtBytes(size);
    message(t('Reading {name}…', { name }));
    // A PDF, a picture or a recording cut short is no use: refuse it before it comes.
    if (kind && kind !== 'markdown' && kind !== 'html' && size > limit) { tooBig(limit); return; }
    let buffer;
    try { buffer = await invoke('files_read', { source, path, limit }); }
    catch (e) { if (mine === token) message(String(e), true); return; }
    if (mine !== token) return;
    let bytes = new Uint8Array(buffer);
    const truncated = bytes.length > limit;
    if (truncated) bytes = bytes.subarray(0, limit);
    if (bytes.length > 5 && String.fromCharCode(...bytes.subarray(0, 5)) === '%PDF-') kind = 'pdf';
    if (kind === null) kind = looksLikeText(bytes) ? 'text' : 'binary';
    got = { bytes, kind, truncated, limit, mine };
    await show();
  }

  /// What `got` is, on screen.
  async function show() {
    const { bytes, kind, truncated, limit, mine } = got;
    letGo();
    const cut = truncated ? el('div.note', icon('info'), t('Only the first {limit} of {size} are shown.', { limit: fmtBytes(limit), size: fmtBytes(size) })) : null;
    clear(body);
    try {
      if (showSource && (kind === 'markdown' || kind === 'html')) { text(bytes, cut); return; }
      switch (kind) {
        case 'text':
          text(bytes, cut);
          if (!bytes.length) message(t('{name} is empty.', { name }));
          break;
        case 'markdown': {
          sourceButton.classList.remove('hidden');
          const { marked } = await import('./vendor/marked.esm.js');
          if (mine !== token) return;
          const html = marked.parse(asText(bytes), { gfm: true, async: false });
          append(body, [cut, el('div.md', markdownToDom(html))]);
          break;
        }
        case 'html': {
          // A frame with every restriction the sandbox has: no script,
          // no form, no plugin, no way out of the frame, an origin of
          // its own, so nothing in the file reaches this window or the
          // app. The file's own styles apply, images from the web load,
          // relative ones cannot.
          sourceButton.classList.remove('hidden');
          const frame = el('iframe.webpage', { title: name });
          frame.setAttribute('sandbox', '');
          frame.setAttribute('referrerpolicy', 'no-referrer');
          frame.srcdoc = asText(bytes);
          append(body, [cut, frame]);
          break;
        }
        case 'image': {
          const img = el('img', { src: blobUrl(bytes, IMAGE[extOf(name)]), alt: name, onload: () => { status.textContent = `${fmtBytes(size)} · ${img.naturalWidth}×${img.naturalHeight}`; } });
          body.append(el('div.picture', img));
          break;
        }
        case 'video':
        case 'audio': {
          const player = el(kind, { controls: true, playsInline: true, preload: 'metadata', src: blobUrl(bytes, MEDIA[extOf(name)]),
            onerror: () => { if (mine === token) message(t('{name} cannot be played here: the system has no decoder for it.', { name }), true); },
            onloadedmetadata: () => { const d = player.duration; status.textContent = `${fmtBytes(size)} · ${Math.floor(d / 60)}:${String(Math.floor(d % 60)).padStart(2, '0')}${kind === 'video' ? ` · ${player.videoWidth}×${player.videoHeight}` : ''}`; } });
          body.append(el('div.picture', player));
          break;
        }
        case 'pdf':
          await showPdf(bytes, mine);
          break;
        default:
          message(t('{name} is not a text file ({size}); there is no preview for it.', { name, size: fmtBytes(size) }));
      }
    } catch (e) {
      if (mine === token) message(t('Could not show {name}: {why}', { name, why: String(e && e.message || e) }), true);
    }
  }

  function text(bytes, cut) {
    wrapButton.classList.remove('hidden');
    append(body, [cut, el('pre.text', { class: wrap ? 'wrap' : '' }, asText(bytes))]);
  }

  /// The pages one under the other, as wide as the tab, each drawn when
  /// it scrolls into view and drawn again at a new width; one scrolled
  /// far away lets its picture go.
  async function showPdf(bytes, mine) {
    const pdfjs = await import('./vendor/pdf.min.mjs');
    pdfjs.GlobalWorkerOptions.workerSrc = new URL('./vendor/pdf.worker.min.mjs', import.meta.url).href;
    // A copy: pdf.js hands the buffer to its worker, and `got` keeps the bytes for a second look.
    const doc = await pdfjs.getDocument({ data: bytes.slice(), isEvalSupported: false }).promise;
    if (mine !== token) { doc.destroy(); return; }
    const first = await doc.getPage(1);
    const vp = first.getViewport({ scale: 1 });
    const pages = [];
    const sheet = el('div.sheet');
    for (let i = 1; i <= doc.numPages; i++) {
      const p = { index: i, page: i === 1 ? first : null, w: vp.width, h: vp.height, box: el('div.page', { title: t('Page {n} of {total}', { n: i, total: doc.numPages }) }), drawnAt: 0, visible: false, task: null };
      pages.push(p);
      sheet.append(p.box);
    }
    status.textContent = `${fmtBytes(size)} · ${doc.numPages === 1 ? t('1 page') : t('{n} pages', { n: doc.numPages })}`;
    body.append(sheet);
    const width = () => Math.max(100, Math.min(1200, body.clientWidth - 40));
    const layout = () => { const w = width(); for (const p of pages) { p.box.style.width = `${w}px`; p.box.style.height = `${Math.round(w * p.h / p.w)}px`; } return w; };
    const draw = async p => {
      const w = width();
      if (p.drawnAt === w) return;
      if (p.task) { p.task.cancel(); p.task = null; }
      if (!p.page) {
        p.page = await doc.getPage(p.index);
        const v = p.page.getViewport({ scale: 1 });
        if (v.width !== p.w || v.height !== p.h) { p.w = v.width; p.h = v.height; p.box.style.height = `${Math.round(w * p.h / p.w)}px`; }
      }
      if (mine !== token || !p.visible) return;
      const dpr = window.devicePixelRatio || 1;
      const viewport = p.page.getViewport({ scale: w / p.w * dpr });
      const canvas = el('canvas', { width: Math.floor(viewport.width), height: Math.floor(viewport.height), style: `width:${w}px;height:${Math.round(w * p.h / p.w)}px` });
      const task = p.page.render({ canvasContext: canvas.getContext('2d'), viewport });
      p.task = task;
      try { await task.promise; } catch { return; } // cancelled: another draw is on its way
      if (p.task !== task) return;
      p.task = null;
      clear(p.box).append(canvas);
      p.drawnAt = w;
    };
    const watcher = new IntersectionObserver(entries => {
      for (const e of entries) {
        const p = pages.find(x => x.box === e.target);
        if (!p) continue;
        p.visible = e.isIntersecting;
        if (p.visible) draw(p);
        else if (p.drawnAt) { clear(p.box); p.drawnAt = 0; }
      }
    }, { root: body, rootMargin: '400px 0px' });
    let lastWidth = layout();
    let timer = null;
    const sizer = new ResizeObserver(() => {
      clearTimeout(timer);
      timer = setTimeout(() => { if (layout() !== lastWidth) { lastWidth = width(); for (const p of pages) if (p.visible) draw(p); } }, 150);
    });
    sizer.observe(body);
    for (const p of pages) watcher.observe(p.box);
    release = () => { watcher.disconnect(); sizer.disconnect(); clearTimeout(timer); for (const p of pages) if (p.task) p.task.cancel(); doc.destroy(); };
  }

  load();
  return {
    focus: () => body.focus({ preventScroll: true }),
    reload: load,
    dispose: () => { token++; letGo(); },
  };
}
