// A file in a tab of the Console, beside the terminals: text as it is,
// coloured by its syntax when its language is known (highlight.js,
// ui/vendor), Markdown rendered (marked, ui/vendor), a web page in a sandboxed frame,
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

// MARK: syntax

// highlight.js colours a text whose language is known: from the file's
// name, or from its first line for a script without a suffix. The
// languages it is built with are the common ones; a few more that a
// server tends to have (nginx, Apache, a Dockerfile) are modules of
// their own, loaded when such a file is shown. Its HTML is rebuilt the
// way Markdown's is below: text and spans with its classes, nothing
// else. A text past HIGHLIGHT_LIMIT is left plain: the grammars read it
// many times over, and a file that big is a log, not a program.
export const HIGHLIGHT_LIMIT = 512 * 1000;
const LANG_BY_EXT = {
  js: 'javascript', mjs: 'javascript', cjs: 'javascript', jsx: 'javascript', ts: 'typescript', tsx: 'typescript', mts: 'typescript',
  json: 'json', jsonc: 'json', json5: 'json', py: 'python', pyi: 'python', pyw: 'python', rb: 'ruby', gemspec: 'ruby', rs: 'rust', go: 'go',
  c: 'c', h: 'c', cc: 'cpp', cpp: 'cpp', cxx: 'cpp', hh: 'cpp', hpp: 'cpp', hxx: 'cpp', ino: 'cpp', cu: 'cpp', m: 'objectivec', mm: 'objectivec',
  java: 'java', kt: 'kotlin', kts: 'kotlin', swift: 'swift', cs: 'csharp', php: 'php', pl: 'perl', pm: 'perl', lua: 'lua', r: 'r', sql: 'sql',
  sh: 'bash', bash: 'bash', zsh: 'bash', ksh: 'bash', css: 'css', scss: 'scss', less: 'less', vue: 'xml', svelte: 'xml',
  xml: 'xml', svg: 'xml', plist: 'xml', xsl: 'xml', xsd: 'xml', rss: 'xml', atom: 'xml', xib: 'xml', storyboard: 'xml', csproj: 'xml', pom: 'xml',
  yml: 'yaml', yaml: 'yaml', toml: 'ini', ini: 'ini', cfg: 'ini', conf: 'ini', properties: 'ini', service: 'ini', timer: 'ini', socket: 'ini', mount: 'ini', desktop: 'ini',
  diff: 'diff', patch: 'diff', mk: 'makefile', make: 'makefile', graphql: 'graphql', gql: 'graphql', wat: 'wasm', vb: 'vbnet', dockerfile: 'dockerfile',
};
const LANG_BY_NAME = {
  makefile: 'makefile', gnumakefile: 'makefile', dockerfile: 'dockerfile', containerfile: 'dockerfile', pkgbuild: 'bash',
  '.bashrc': 'bash', '.bash_profile': 'bash', '.bash_aliases': 'bash', '.bash_logout': 'bash', '.zshrc': 'bash', '.zshenv': 'bash', '.zprofile': 'bash', '.profile': 'bash',
  'cargo.lock': 'ini', '.gitconfig': 'ini', '.gitmodules': 'ini', '.editorconfig': 'ini', '.npmrc': 'ini', 'php.ini': 'ini',
  'nginx.conf': 'nginx', 'httpd.conf': 'apache', 'apache2.conf': 'apache', '.htaccess': 'apache',
};
const SHEBANG = [[/\b(ba|z|k|da|a)?sh\b/, 'bash'], [/\bpython/, 'python'], [/\bperl/, 'perl'], [/\bruby/, 'ruby'], [/\b(node|deno|bun)\b/, 'javascript'], [/\blua/, 'lua'], [/\bphp/, 'php'], [/\bRscript/, 'r']];
const EXTRA = { dockerfile: './vendor/highlight-dockerfile.min.js', nginx: './vendor/highlight-nginx.min.js', apache: './vendor/highlight-apache.min.js' };
// What a Markdown fence calls a language, where highlight.js's aliases do not.
const FENCE_ALIAS = { shell: 'bash', jsonc: 'json', json5: 'json', dockerfile: 'dockerfile', docker: 'dockerfile', conf: 'ini', text: null, plaintext: null, txt: null };

/// The highlight.js language of a file, or null when nothing says it.
export function languageOf(name, path, bytes) {
  const lower = name.toLowerCase();
  if (LANG_BY_NAME[lower]) return LANG_BY_NAME[lower];
  if (lower.startsWith('dockerfile.')) return 'dockerfile';
  const ext = extOf(name);
  if (ext === 'conf' || ext === 'inc') {
    if (/\/nginx\//.test(path)) return 'nginx';
    if (/\/(apache2?|httpd)\//.test(path)) return 'apache';
  }
  if (LANG_BY_EXT[ext]) return LANG_BY_EXT[ext];
  const first = asText(bytes.subarray(0, 256)).split('\n', 1)[0];
  if (first.startsWith('#!')) { for (const [re, lang] of SHEBANG) if (re.test(first)) return lang; }
  if (/^\s*<\?xml/.test(first)) return 'xml';
  if (/^<\?php/.test(first)) return 'php';
  return null;
}

let hljsLoading = null;
/// highlight.js with `language` registered, or null when it has no such language.
async function highlighter(language) {
  if (!hljsLoading) hljsLoading = import('./vendor/highlight.min.js').then(m => m.default);
  const hljs = await hljsLoading;
  if (!hljs.getLanguage(language) && EXTRA[language]) {
    const mod = await import(EXTRA[language]);
    if (!hljs.getLanguage(language)) hljs.registerLanguage(language, mod.default);
  }
  return hljs.getLanguage(language) ? hljs : null;
}

const HL_CLASS = /^(hljs-[\w-]+|[a-z]+_|language-[\w+-]+)$/;
/// `code` coloured as `language`: text and spans, from highlight.js's HTML.
export function highlighted(hljs, code, language) {
  // A template's content is inert, and keeps the leading whitespace a
  // document parsed from the string would drop before its body.
  const tpl = document.createElement('template');
  tpl.innerHTML = hljs.highlight(code, { language, ignoreIllegals: true }).value;
  const out = document.createDocumentFragment();
  const walk = (from, into) => {
    for (const n of Array.from(from.childNodes)) {
      if (n.nodeType === Node.TEXT_NODE) { into.append(document.createTextNode(n.data)); continue; }
      if (n.nodeType !== Node.ELEMENT_NODE) continue;
      if (n.localName !== 'span') { walk(n, into); continue; }
      const span = document.createElement('span');
      const classes = [...n.classList].filter(c => HL_CLASS.test(c));
      if (classes.length) span.className = classes.join(' ');
      walk(n, span);
      into.append(span);
    }
  };
  walk(tpl.content, out);
  return out;
}

// MARK: lines

/// How many lines get an element of their own, and a number: past
/// this the text is unnumbered.
export const NUMBERED_LIMIT = 200 * 1000;

/// How many lines go into the page at a time: a long text comes in
/// pieces, the page answering in between. Bare text, past
/// NUMBERED_LIMIT, costs less a line and comes in bigger pieces.
const LINES_AT_ONCE = 2000;
const BARE_LINES_AT_ONCE = 20 * 1000;

/// `content` (text, or highlight.js's text and spans) cut into one
/// element per line, the numbers drawn by CSS from each line's place,
/// one line at a time. A span that runs over a line's end is closed
/// there and opened again on the next, so a line stays a block of its
/// own, which is what lets a wrapped line's rest sit under its first
/// row and not its number.
export function* linesOf(content) {
  let line = el('span.line');
  let made = 0;
  const stack = [];   // the spans open at this point, innermost last
  const into = () => stack.length ? stack[stack.length - 1].elem : line;
  // An empty line is a block with nothing in it: no height, and no
  // newline when the text is copied. A <br> gives it both.
  const done = () => { if (!line.textContent) line.append(el('br')); made++; return line; };
  const reopen = () => {
    line = el('span.line');
    for (const entry of stack) { const span = el('span', { class: entry.cls }); (entry.parent ? entry.parent.elem : line).append(span); entry.elem = span; }
  };
  function* walk(from) {
    for (const n of Array.from(from.childNodes)) {
      if (n.nodeType === Node.TEXT_NODE) {
        const parts = n.data.split('\n');
        for (let i = 0; i < parts.length; i++) {
          if (i) { yield done(); reopen(); }
          if (parts[i]) into().append(document.createTextNode(parts[i]));
        }
      } else if (n.nodeType === Node.ELEMENT_NODE) {
        const span = el('span', { class: n.className });
        into().append(span);
        stack.push({ cls: n.className, elem: span, parent: stack[stack.length - 1] || null });
        yield* walk(n);
        stack.pop();
      }
    }
  }
  yield* walk(content);
  // A text ending in a newline has no line after it.
  if (line.childNodes.length || !made) yield done();
}

/// All the lines of `content` at once.
export function asLines(content) {
  return [...linesOf(content)];
}

/// A plain text's lines, as linesOf would cut it, read straight from
/// the string.
function* plainLines(code) {
  for (let from = 0; ;) {
    const nl = code.indexOf('\n', from);
    const part = nl === -1 ? code.slice(from) : code.slice(from, nl);
    if (nl === -1 && !part && from) return;
    const line = document.createElement('span');
    line.className = 'line';
    line.append(part ? document.createTextNode(part) : document.createElement('br'));
    yield line;
    if (nl === -1) return;
    from = nl + 1;
  }
}

/// A text too long to number, in pieces of BARE_LINES_AT_ONCE lines.
function* plainSlices(code) {
  for (let from = 0; from < code.length;) {
    let to = from - 1;
    for (let n = 0; n < BARE_LINES_AT_ONCE; n++) { to = code.indexOf('\n', to + 1); if (to === -1) break; }
    const end = to === -1 ? code.length : to + 1;
    yield document.createTextNode(code.slice(from, end));
    from = end;
  }
}

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

const ZOOM_STEP = 1.25;
const ZOOM_MIN = 0.1;
const ZOOM_MAX = 64;        // a small picture fitted to the tab is well past 8×
const PAGE_WIDTH_MAX = 2400; // a PDF page is a bitmap: this wide at most, whatever the scale

/// Draw the file into `root`: a header with the path, the content under
/// it. Returns what the tab needs: focus the content, read it again,
/// and let go of it when the tab closes.
export function createPreview(root, { source, path, name, size }) {
  const body = el('div.body', { tabIndex: 0 });
  const status = el('span.caption');
  let wrap = false;
  let colour = (() => { try { return localStorage.getItem('preview-plain') !== '1'; } catch { return true; } })();
  let showSource = false;
  // How big the content is drawn: fitted to the tab's width or height
  // (a picture, a video, a PDF's pages), or a scale of its own size
  // (anything; for a text its letters). `relayout` is what the content
  // on screen does about it.
  let zoom = { mode: 'width', scale: 1 };
  let relayout = null;
  const button = (title, name, run) => el('button.hidden', { title, onclick: run }, icon(name));
  const wrapButton = button(t('Wrap long lines'), 'wrap', () => { wrap = !wrap; wrapButton.classList.toggle('active', wrap); const pre = body.querySelector('pre.text'); if (pre) pre.classList.toggle('wrap', wrap); });
  const colourButton = button(t('Colour the syntax'), 'highlighter', () => { colour = !colour; colourButton.classList.toggle('active', colour); try { localStorage.setItem('preview-plain', colour ? '0' : '1'); } catch {} if (got) show(); });
  colourButton.classList.toggle('active', colour);
  const sourceButton = button(t('The file as written, instead of the page it makes'), 'code', () => { showSource = !showSource; sourceButton.classList.toggle('active', showSource); if (got) show(); });
  const fitWidth = button(t('As wide as the tab (⌘0)'), 'fitwidth', () => setZoom('width'));
  const fitHeight = button(t('As tall as the tab: a PDF\'s pages side by side'), 'fitheight', () => setZoom('height'));
  const zoomOut = button(t('Smaller (⌘−, ⌘ and the wheel)'), 'zoomout', () => zoomBy(1 / ZOOM_STEP));
  const zoomIn = button(t('Bigger (⌘+, ⌘ and the wheel)'), 'zoomin', () => zoomBy(ZOOM_STEP));
  const percent = el('button.percent.hidden', { title: t('The size now; click for the file\'s own size'), onclick: () => setZoom('scale', 1) }, '100%');
  const head = el('div.top', icon('doc'), el('span.path.selectable', { title: path }, path), el('div.spacer'), status, wrapButton, colourButton, sourceButton,
    fitWidth, fitHeight, zoomOut, percent, zoomIn,
    el('button', { title: t('Read it again'), onclick: () => load() }, icon('refresh')));
  root.append(head, body);
  let token = 0;         // which load the content on screen belongs to
  let release = null;    // what the content holds: a PDF document, a blob URL
  let got = null;        // the bytes on screen, what they are, whether they were cut

  const letGo = () => { if (release) release(); release = null; relayout = null; };
  const message = (text, isError) => { clear(body).append(el('div.message', icon(isError ? 'warning' : 'info', isError ? 'orange' : ''), el('span.selectable', text))); };
  const tooBig = limit => message(t('{name} is {size}, more than the {limit} a preview can show.', { name, size: fmtBytes(size), limit: fmtBytes(limit) }), true);
  const blobUrl = (bytes, type) => { const url = URL.createObjectURL(new Blob([bytes], { type })); release = () => URL.revokeObjectURL(url); return url; };

  // MARK: zoom

  /// The buttons that make sense for what is shown: fitting a picture or
  /// pages, scaling anything but a web page.
  function zoomControls(kind) {
    const fits = kind === 'image' || kind === 'video' || kind === 'pdf';
    const scales = fits || kind === 'text' || kind === 'markdown';
    for (const b of [fitWidth, fitHeight]) b.classList.toggle('hidden', !fits);
    for (const b of [zoomOut, zoomIn, percent]) b.classList.toggle('hidden', !scales);
    if (!fits && zoom.mode !== 'scale') zoom = { mode: 'scale', scale: 1 };
  }

  function setZoom(mode, scale = zoom.scale) {
    zoom = { mode, scale: Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, scale)) };
    fitWidth.classList.toggle('active', mode === 'width');
    fitHeight.classList.toggle('active', mode === 'height');
    if (relayout) relayout();
  }

  /// A step from the size on screen, whatever fitted it.
  function zoomBy(factor) {
    setZoom('scale', (relayout ? relayout.scale() : zoom.scale) * factor);
  }

  /// The percent button says what the content is at; `scale` 1 is the
  /// file's own size (a PDF's points as pixels).
  function showScale(scale) { percent.textContent = `${Math.round(scale * 100)}%`; }

  root.addEventListener('keydown', e => {
    if (!(e.metaKey || e.ctrlKey) || percent.classList.contains('hidden')) return;
    if (e.key === '=' || e.key === '+') { e.preventDefault(); zoomBy(ZOOM_STEP); }
    else if (e.key === '-') { e.preventDefault(); zoomBy(1 / ZOOM_STEP); }
    else if (e.key === '0') { e.preventDefault(); setZoom(fitWidth.classList.contains('hidden') ? 'scale' : 'width', 1); }
  });
  body.addEventListener('wheel', e => {
    if (!(e.metaKey || e.ctrlKey) || percent.classList.contains('hidden')) return;
    e.preventDefault();
    zoomBy(e.deltaY < 0 ? 1.1 : 1 / 1.1);
  }, { passive: false });

  // MARK: loading

  async function load() {
    const mine = ++token;
    letGo();
    got = null;
    let kind = kindOf(name);
    const limit = LIMITS[kind || 'text'];
    for (const b of [wrapButton, colourButton, sourceButton, fitWidth, fitHeight, zoomOut, zoomIn, percent]) b.classList.add('hidden');
    status.textContent = fmtBytes(size);
    message(t('Reading {name}…', { name }));
    // A PDF, a picture or a recording cut short is no use: refuse it before it comes.
    // (A file opened from a terminal's text comes without a size.)
    if (kind && kind !== 'markdown' && kind !== 'html' && size != null && size > limit) { tooBig(limit); return; }
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
    const cut = truncated ? el('div.note', icon('info'), size == null ? t('Only the first {limit} are shown.', { limit: fmtBytes(limit) }) : t('Only the first {limit} of {size} are shown.', { limit: fmtBytes(limit), size: fmtBytes(size) })) : null;
    clear(body);
    // The text's buttons: shown again by what has a use for them.
    for (const b of [wrapButton, colourButton]) b.classList.add('hidden');
    const asSource = showSource && (kind === 'markdown' || kind === 'html');
    zoomControls(asSource ? 'text' : kind);
    try {
      if (asSource) { await text(bytes, cut, kind === 'markdown' ? 'markdown' : 'xml', mine); return; }
      switch (kind) {
        case 'text':
          if (!bytes.length) { message(t('{name} is empty.', { name })); break; }
          await text(bytes, cut, languageOf(name, path, bytes), mine);
          break;
        case 'markdown': {
          sourceButton.classList.remove('hidden');
          const { marked } = await import('./vendor/marked.esm.js');
          if (mine !== token) return;
          const html = marked.parse(asText(bytes), { gfm: true, async: false });
          const md = el('div.md', markdownToDom(html));
          append(body, [cut, md]);
          scaled(md);
          await colourFences(md, mine);
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
          const img = el('img', { src: blobUrl(bytes, IMAGE[extOf(name)]), alt: name, onload: () => { status.textContent = `${fmtBytes(size)} · ${img.naturalWidth}×${img.naturalHeight}`; fitted(img, img.naturalWidth, img.naturalHeight); } });
          body.append(el('div.picture', img));
          break;
        }
        case 'video':
        case 'audio': {
          const player = el(kind, { controls: true, playsInline: true, preload: 'metadata', src: blobUrl(bytes, MEDIA[extOf(name)]),
            onerror: () => { if (mine === token) message(t('{name} cannot be played here: the system has no decoder for it.', { name }), true); },
            onloadedmetadata: () => {
              const d = player.duration;
              status.textContent = `${fmtBytes(size)} · ${Math.floor(d / 60)}:${String(Math.floor(d % 60)).padStart(2, '0')}${kind === 'video' ? ` · ${player.videoWidth}×${player.videoHeight}` : ''}`;
              if (kind === 'video') fitted(player, player.videoWidth, player.videoHeight);
            } });
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

  /// The text as it is, then coloured as `language` when it has one
  /// and is not too long for that: the words come first, the colours
  /// when the grammar has read them.
  async function text(bytes, cut, language, mine) {
    wrapButton.classList.remove('hidden');
    const code = asText(bytes);
    const pre = el('pre.text', { class: wrap ? 'wrap' : '' });
    // Numbered when the lines are not too many for an element each.
    let lines = 1;
    for (let i = code.indexOf('\n'); i !== -1 && lines <= NUMBERED_LIMIT; i = code.indexOf('\n', i + 1)) lines++;
    const numbered = lines <= NUMBERED_LIMIT;
    if (numbered) { pre.classList.add('numbered'); pre.style.setProperty('--digits', String(lines).length); }
    append(body, [cut, pre]);
    scaled(pre);
    if (numbered) fill(pre, plainLines(code), LINES_AT_ONCE, mine);
    else fill(pre, plainSlices(code), 1, mine);
    if (!language || bytes.length > HIGHLIGHT_LIMIT) return;
    colourButton.classList.remove('hidden');
    if (!colour) return;
    const hljs = await highlighter(language);
    if (mine !== token || !hljs || !pre.isConnected) return;
    const coloured = highlighted(hljs, code, language);
    if (numbered) fill(pre, linesOf(coloured), LINES_AT_ONCE, mine);
    else { filling++; pre.replaceChildren(coloured); }
  }

  /// Put `pieces` (lines, or slices of a text too long to number) into
  /// `pre`, `each` at a time, between which the page goes on answering.
  /// They take the place of what is there already, one for one: the
  /// coloured lines come over the plain ones, so nothing moves under the
  /// reader. A newer fill, or another file, stops this one.
  let filling = 0;
  function fill(pre, pieces, each, mine) {
    const id = ++filling;
    let at = 0;
    const step = () => {
      if (id !== filling || mine !== token) return;
      const more = document.createDocumentFragment();
      for (let n = 0; n < each; n++) {
        const { value, done } = pieces.next();
        if (done) {
          pre.append(more);
          while (pre.childNodes.length > at) pre.lastChild.remove();
          return;
        }
        const old = pre.childNodes[at];
        if (old) old.replaceWith(value); else more.append(value);
        at++;
      }
      pre.append(more);
      setTimeout(step, 0);
    };
    step();
  }

  /// A Markdown page's fenced code, coloured as the fence says.
  async function colourFences(md, mine) {
    const blocks = [...md.querySelectorAll('pre > code[class^="language-"]')]
      .map(code => { const fence = code.className.slice('language-'.length).toLowerCase(); return { code, language: fence in FENCE_ALIAS ? FENCE_ALIAS[fence] : fence }; })
      .filter(b => b.language && b.code.textContent.length <= HIGHLIGHT_LIMIT);
    if (!blocks.length) return;
    colourButton.classList.remove('hidden');
    if (!colour) return;
    for (const { code, language } of blocks) {
      const source = code.textContent;
      const hljs = await highlighter(language);
      if (mine !== token) return;
      if (hljs) clear(code).append(highlighted(hljs, source, language));
    }
  }

  /// Text and Markdown: the scale is the letters' size.
  function scaled(node) {
    relayout = () => { node.style.zoom = zoom.scale; showScale(zoom.scale); };
    relayout.scale = () => zoom.scale;
    relayout();
  }

  /// A picture or a video of `w`×`h`: as wide or as tall as the tab (the
  /// padding left out), or at a scale of its own size.
  function fitted(media, w, h) {
    const picture = media.parentElement;
    relayout = () => {
      picture.classList.toggle('tall', zoom.mode === 'height');
      media.style.width = zoom.mode === 'width' ? '100%' : zoom.mode === 'height' ? 'auto' : `${Math.round(w * zoom.scale)}px`;
      media.style.height = zoom.mode === 'height' ? '100%' : 'auto';
      showScale(relayout.scale());
    };
    relayout.scale = () => media.getBoundingClientRect().width / w || 1;
    relayout();
    new ResizeObserver(() => { if (zoom.mode !== 'scale') showScale(relayout.scale()); }).observe(media);
  }

  /// The pages in reading order, each as wide as the tab, or as tall as
  /// it, or at a scale of its own size; pages narrower than the tab sit
  /// side by side. Each is drawn when it scrolls into view and drawn
  /// again at a new size; one scrolled far away lets its picture go.
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
    // A page's width on screen; the sheet's padding and the gap between
    // pages are what fitting leaves out.
    const widthOf = p => {
      if (zoom.mode === 'width') return Math.max(50, body.clientWidth - 40);
      if (zoom.mode === 'height') return Math.max(50, Math.round((body.clientHeight - 40) * p.w / p.h));
      return Math.min(PAGE_WIDTH_MAX, Math.round(p.w * zoom.scale));
    };
    const place = p => { const w = widthOf(p); p.box.style.width = `${w}px`; p.box.style.height = `${Math.round(w * p.h / p.w)}px`; return w; };
    const layout = () => { for (const p of pages) place(p); showScale(widthOf(pages[0]) / pages[0].w); };
    const draw = async p => {
      const w = widthOf(p);
      if (p.drawnAt === w) return;
      if (p.task) { p.task.cancel(); p.task = null; }
      if (!p.page) {
        p.page = await doc.getPage(p.index);
        const v = p.page.getViewport({ scale: 1 });
        if (v.width !== p.w || v.height !== p.h) { p.w = v.width; p.h = v.height; place(p); }
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
    const redraw = () => { layout(); for (const p of pages) if (p.visible) draw(p); };
    let timer = null;
    const sizer = new ResizeObserver(() => { clearTimeout(timer); timer = setTimeout(() => { if (zoom.mode !== 'scale') redraw(); }, 150); });
    relayout = redraw;
    relayout.scale = () => widthOf(pages[0]) / pages[0].w;
    layout();
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
