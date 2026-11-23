// What a Console's terminal costs once the bytes are in the page: the
// same output given to xterm.js the way console.js gives it (base64 in
// 16 KB pieces, `atob`, `term.write`), with no renderer at all (the
// terminal never opened: the parser alone), with the DOM renderer (the
// default) and with the WebGL one. The difference between the first and
// the other two is what drawing costs.
//
// `Tests/render-bench.sh` runs this page in the system's WebKit, the
// app's engine, and prints what it writes into #results. Opened in a
// browser by hand it measures that browser instead. `?mb=4` makes the
// runs shorter, `?runs=1` fewer. The page feeds a terminal no faster
// than it parses (4 MB waiting at most): xterm throws what it is given
// away once 50 MB wait, and the app has nothing that holds a flood back.
'use strict';

const query = new URLSearchParams(location.search);
const MB = 1048576;
const SCALE = Number(query.get('mb')) || 128;
const RUNS = Number(query.get('runs')) || 3;
const CHUNK = 16384; // term.rs reads the screen's pty in pieces of this size
const COLS = 120, ROWS = 40;

// The Console's own terminal (console.js `openTabIn`, `TSET_DEFAULT`).
const OPTIONS = {
  cols: COLS, rows: ROWS,
  fontFamily: 'ui-monospace, "SF Mono", Menlo, Consolas, monospace',
  fontSize: 13,
  lineHeight: 1.15,
  cursorStyle: 'block',
  cursorBlink: true,
  scrollback: 10000,
  allowProposedApi: true,
};

// As console.js: on a display of one pixel per point xterm is told two.
const realDpr = window.devicePixelRatio || 1;
if (realDpr < 2) { try { Object.defineProperty(window, 'devicePixelRatio', { get: () => 2, configurable: true }); } catch {} }

const out = document.getElementById('results');
const say = line => { out.textContent += line + '\n'; };
const row = (name, value, note = '') => say(`  ${name.padEnd(54)} ${value.padStart(12)}${note ? '   ' + note : ''}`);
const frame = () => new Promise(r => requestAnimationFrame(r));
const median = list => [...list].sort((a, b) => a - b)[list.length >> 1];

// console.js's `decode`.
function decode(b64) {
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return bytes;
}

/// `lines` (a function of the line's number) up to `mb` megabytes, as
/// the base64 pieces the app's `term-data` events carry: a round of
/// some 2 MB, ending where a line ends, gone through `repeat` times.
function pieces(mb, lines) {
  let text = '';
  for (let i = 0; text.length < 4 * CHUNK; i++) text += lines(i);
  const block = new TextEncoder().encode(text);
  const round = Math.max(1, Math.floor(2 * MB / block.length)) * block.length;
  const list = [];
  for (let at = 0; at < round; at += CHUNK) {
    const n = Math.min(CHUNK, round - at);
    let bin = '';
    for (let i = 0; i < n; i++) bin += String.fromCharCode(block[(at + i) % block.length]);
    list.push(btoa(bin));
  }
  const repeat = Math.max(1, Math.round(mb * MB / round));
  return { list, repeat, count: list.length * repeat, total: round * repeat };
}

// 64 bytes a line, as the pipes bench's.
const PLAIN = 'the quick brown fox jumps over the lazy dog 0123456789'.padEnd(62, ' abcdefg') + '\r\n';

const WORKLOADS = [
  { name: 'plain lines', mb: SCALE, lines: () => PLAIN },
  // What a build or a coloured listing prints: several SGR runs a line.
  { name: 'coloured lines', mb: SCALE / 2, lines: i =>
      `\x1b[1;32m   Compiling\x1b[0m crate-${i % 97} v0.${i % 13}.2 \x1b[38;5;244m(/home/t/src/crate-${i % 97})\x1b[0m ` +
      `\x1b[38;2;255;128;0mwarning\x1b[0m: unused variable \x1b[1m\`x${i % 31}\`\x1b[0m \x1b[4;34msrc/lib.rs:${i % 900}\x1b[0m\r\n` },
  // Wide characters: two cells each, the Unicode 11 widths asked for.
  { name: 'wide characters (CJK, emoji)', mb: SCALE / 2, lines: i =>
      `终端里的中文输出，宽字符占两格 🙂 日本語のテキスト 한국어 ${i % 1000} 文件列表与日志\r\n` },
  // A full-screen program redrawing itself: every row addressed and
  // painted, nothing scrolling.
  { name: 'a full screen redrawn', mb: SCALE / 2, frames: true, lines: i => {
      let s = '\x1b[H';
      for (let r = 1; r <= ROWS; r++) {
        s += `\x1b[${r};1H\x1b[48;5;${(i + r) % 6 + 17}m\x1b[38;5;${(i * 7 + r) % 200 + 20}m` +
          `${String(i).padStart(6)} │ ${'row ' + r + ' of the screen '.repeat(6)}`.slice(0, COLS - 1) + '\x1b[0m';
      }
      return s;
    } },
];

const CONFIGS = [
  { id: 'none', name: 'no renderer (the parser alone)' },
  { id: 'dom', name: 'DOM renderer (the default)' },
  { id: 'webgl', name: 'WebGL renderer' },
];

/// A terminal as the Console makes it, drawn by `config`'s renderer.
async function terminal(config) {
  const stage = document.getElementById('stage');
  stage.textContent = '';
  const term = new Terminal(OPTIONS);
  term.loadAddon(new Unicode11Addon.Unicode11Addon());
  term.unicode.activeVersion = '11';
  const drawn = { count: 0 };
  if (config !== 'none') {
    term.open(stage);
    if (config === 'webgl') term.loadAddon(new WebglAddon.WebglAddon());
    term.onRender(() => drawn.count++);
    await frame(); await frame();
  }
  return { term, drawn };
}

/// The page's frames while a run lasts: how many, and the longest wait
/// between two (the stall a user would see).
function watchFrames() {
  const w = { frames: 0, longest: 0, on: true };
  let last = performance.now();
  const tick = () => {
    if (!w.on) return;
    const now = performance.now();
    w.frames++;
    w.longest = Math.max(w.longest, now - last);
    last = now;
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
  return w;
}

/// Four pieces a task, as events come, while fewer than `WAITING` bytes
/// wait in xterm, which takes them in its own time slices; the last
/// write's callback is the end.
const WAITING = 4 * MB;
function feed(term, work) {
  return new Promise(done => {
    let i = 0, waiting = 0, asked = false;
    const ch = new MessageChannel();
    const more = () => { if (!asked && i < work.count && waiting < WAITING) { asked = true; ch.port2.postMessage(0); } };
    ch.port1.onmessage = () => {
      asked = false;
      for (let k = 0; k < 4 && i < work.count; k++, i++) {
        const bytes = decode(work.list[i % work.list.length]);
        waiting += bytes.length;
        term.write(bytes, () => { waiting -= bytes.length; more(); });
      }
      if (i >= work.count) term.write('', done); else more();
    };
    more();
  });
}

async function once(work, config) {
  const { term, drawn } = await terminal(config);
  const w = watchFrames();
  const t = performance.now();
  await feed(term, work);
  const took = performance.now() - t;
  w.on = false;
  term.dispose();
  return { took, frames: w.frames, longest: w.longest, drawn: drawn.count };
}

function report(name, total, r, base, work) {
  const rate = total / MB / (r.took / 1000);
  let note = '';
  if (base) {
    const share = Math.max(0, 1 - base.took / r.took);
    note = `drawing ${Math.round(share * 100)}% of the time, ${r.drawn} frames drawn, ` +
      `${Math.round(r.frames / (r.took / 1000))} fps, longest stall ${Math.round(r.longest)} ms`;
  }
  if (work && work.frames) note += `${note ? ', ' : ''}${Math.round(work.screens / (r.took / 1000))} screens/s taken`;
  row(name, `${rate.toFixed(1)} MB/s`, note);
}

// What the host program (render-bench.sh's WebKit) pushes in: the same
// pieces through `evaluateJavaScript`, the way the app's events arrive,
// at the pace a pty gives a flood (`PTY_RATE`, what the pipes bench
// measures for a terminal on a Mac), nothing holding them back. Whether
// the page keeps up: the frames meanwhile, and how far behind the
// terminal is when the last piece has come.
const PTY_RATE = 64; // MB/s
const pushed = { term: null, drawn: null, watch: null, start: 0, bytes: 0 };
window.__pushBegin = async config => {
  const { term, drawn } = await terminal(config);
  Object.assign(pushed, { term, drawn, watch: null, start: 0, bytes: 0 });
  return true;
};
window.__push = ({ data }) => {
  if (!pushed.start) { pushed.start = performance.now(); pushed.watch = watchFrames(); }
  const bytes = decode(data);
  pushed.bytes += bytes.length;
  pushed.term.write(bytes);
};
window.__pushEnd = () => {
  const came = performance.now();
  pushed.term.write('', () => {
    const took = performance.now() - pushed.start;
    const behind = performance.now() - came;
    pushed.watch.on = false;
    pushed.term.dispose();
    state.pushed = { took, frames: pushed.watch.frames, longest: pushed.watch.longest, drawn: pushed.drawn.count, bytes: pushed.bytes, behind };
  });
  return true;
};

// What the host program polls: `want` asks it for a push into a
// terminal of that renderer, `done` ends it.
const state = { want: null, piece: null, count: 0, pushed: null, done: false };
window.__state = () => JSON.stringify({ want: state.want, count: state.count, rate: PTY_RATE * MB / CHUNK, done: state.done });
window.__piece = () => state.piece;

async function pushedRun(config, work) {
  if (!navigator.userAgent.includes('ShellderBench')) return;
  state.pushed = null;
  state.piece = work.list[0];
  state.count = Math.round(Math.min(work.total, 96 * MB) / CHUNK);
  state.want = config.id;
  while (!state.pushed) await new Promise(r => setTimeout(r, 50));
  state.want = null;
  const r = state.pushed;
  row(`  as events at a pty's pace, ${config.name.split(' (')[0]}`, `${(r.bytes / MB / (r.took / 1000)).toFixed(1)} MB/s`,
    `${r.drawn} frames drawn, ${Math.round(r.frames / (r.took / 1000))} fps, longest stall ${Math.round(r.longest)} ms, ` +
    `${Math.round(r.behind)} ms behind at the end`);
}

async function main() {
  say(`terminal rendering (xterm.js ${COLS}x${ROWS}, ${navigator.userAgent.match(/(AppleWebKit|Chrome|Firefox)\/[\d.]+/g)?.pop() || 'this browser'}, pixel ratio ${realDpr})`);
  // The engine's compilers warmed, the fonts in.
  const warm = pieces(0.5, () => PLAIN);
  for (const c of CONFIGS) await once(warm, c.id);
  if (document.fonts) await document.fonts.ready;

  {
    const { list } = pieces(2, () => PLAIN);
    const times = [];
    let bytes = 0;
    for (let n = 0; n < 5; n++) {
      const t = performance.now();
      bytes = 0;
      for (let turn = 0; turn < 16; turn++) for (const b64 of list) bytes += decode(b64).length;
      times.push(performance.now() - t);
    }
    row('base64 decoded (atob and a loop)', `${(bytes / MB / (median(times) / 1000)).toFixed(0)} MB/s`);
  }

  for (const work of WORKLOADS) {
    say(`\n${work.name}`);
    const made = pieces(work.mb, work.lines);
    if (work.frames) made.screens = made.total / new TextEncoder().encode(work.lines(0)).length;
    let base = null;
    for (const config of CONFIGS) {
      let r;
      try {
        const runs = [];
        for (let n = 0; n < RUNS; n++) runs.push(await once(made, config.id));
        r = runs.sort((a, b) => a.took - b.took)[runs.length >> 1];
      } catch (e) {
        row(`  ${config.name}`, 'failed', String(e));
        continue;
      }
      report(`  ${config.name}`, made.total, r, base, work.frames ? { frames: true, screens: made.screens } : null);
      if (config.id === 'none') base = r;
      else if (work === WORKLOADS[0]) await pushedRun(config, made);
    }
  }
  say('DONE');
  state.done = true;
}

main().catch(e => { say(`error: ${e && e.stack || e}`); say('DONE'); state.done = true; });
