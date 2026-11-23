// What a terminal costs in the app's own window: loaded into every page
// of a probe build by SHELLDER_PROBE (Tests/gui-bench.sh), it has the
// main window open this machine's Console and there opens tabs that
// `cat` a file each, so that the bytes take the real way: the pty,
// term.rs's reader, a `term-data` event for every piece, console.js's
// `termData`, xterm.js and its renderer, in a window the window server
// composites, see-through and blurred or not. What it measures goes to
// the server that served it, a line at a time (`serve.py` puts
// `window.__BENCH` in front of this file).
(async () => {
  'use strict';
  const BENCH = window.__BENCH;
  const T = window.__TAURI__;
  const invoke = T.core.invoke;
  const label = T.window.getCurrentWindow().label;
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const now = () => performance.now();

  // One request at a time, so the lines arrive in their order.
  let said = Promise.resolve();
  const say = line => {
    said = said.then(() => new Promise(done => {
      const img = new Image();
      img.onload = img.onerror = done;
      img.src = `${BENCH.origin}/say?l=${encodeURIComponent(line)}&t=${Date.now()}${Math.random()}`;
    }));
    return said;
  };
  const row = (name, value, note = '') => say(`  ${name.padEnd(40)} ${value.padStart(12)}${note ? '   ' + note : ''}`);

  // What goes wrong in the page while a run lasts is part of its result.
  const errors = [];
  window.addEventListener('error', e => errors.push(String(e.message || e.error || e)));
  window.addEventListener('unhandledrejection', e => errors.push(String(e.reason && e.reason.message || e.reason)));

  if (label === 'main') {
    await invoke('open_window', { which: 'console', host: 'this machine' });
    return;
  }
  if (!label.startsWith('console-')) return;

  async function until(what, ms, test) {
    const t = now();
    for (;;) {
      const got = test();
      if (got) return got;
      if (now() - t > ms) throw new Error(`${what}: not within ${Math.round(ms / 1000)} s`);
      await sleep(20);
    }
  }

  /// The page's frames while a run lasts: how many, and the longest
  /// wait between two.
  function watchFrames() {
    const w = { frames: 0, longest: 0, on: true, since: now() };
    let last = w.since;
    const tick = () => {
      if (!w.on) return;
      const t = now();
      w.frames++;
      w.longest = Math.max(w.longest, t - last);
      last = t;
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
    return w;
  }

  /// A tab on `command`, once its terminal runs.
  async function tab(S, name, command) {
    const x = S.run(name, command);
    await until(`the tab ${name}`, 20000, () => x.id !== null && x.id !== undefined || x.ended);
    if (x.ended) throw new Error(`the tab ${name} did not start`);
    return x;
  }

  /// A file through a tab: the rate between the two marks around it,
  /// the frames meanwhile, and how long after the last piece came the
  /// terminal had taken it all.
  async function play(S, work) {
    const x = S.run(work.name, `sh ${BENCH.play} ${work.file}`);
    let start = 0, end = 0, watch = null, drawn = 0, arrived = 0, last = 0;
    const osc = x.term.parser.registerOscHandler(7777, mark => {
      if (mark === 'start') { start = now(); watch = watchFrames(); } else if (mark === 'end') end = now();
      return true;
    });
    const render = x.term.onRender(() => { if (start && !end) drawn++; });
    let parsedAt = now();
    const parse = x.term.onWriteParsed(() => { parsedAt = now(); });
    const unlisten = await T.event.listen('term-data', ({ payload }) => {
      if (payload.id !== x.id) return;
      if (BENCH.debug && arrived < 400) say(`  [${work.name}] ${JSON.stringify(atob(payload.data).slice(0, 160))}`);
      last = now();
      arrived += Math.floor(payload.data.length * 3 / 4);
    });
    try {
      // Without its mark: nothing came and xterm has had nothing left
      // to parse for a while.
      await until(work.name, 300000, () => end || (start && now() - last > 5000 && now() - parsedAt > 5000));
      if (!end) {
        // What the terminal shows instead, for whoever reads the failure.
        const buffer = x.term.buffer.active;
        const lines = [];
        for (let i = Math.max(0, buffer.length - 6); i < buffer.length; i++) lines.push(buffer.getLine(i).translateToString(true));
        const said = [...new Set(errors.splice(0))].slice(0, 3).join('; ');
        throw new Error(`the output ended without its mark; ${Math.round(arrived / 1024)} KB came` +
          (said ? `; the page said: ${said}` : '') + `; the terminal shows: ${lines.filter(l => l).join(' / ').slice(0, 160)}`);
      }
    } finally {
      if (watch) watch.on = false;
      osc.dispose(); render.dispose(); parse.dispose(); unlisten();
      const webgl = !!x.gl;
      S.close(x);
      x.webgl = webgl;
    }
    if (BENCH.debug) {
      const buffer = x.term.buffer.active;
      for (let i = Math.max(0, buffer.length - 4); i < buffer.length; i++) say(`  [${work.name}] row ${i}: ${JSON.stringify(buffer.getLine(i).translateToString(true))}`);
    }
    const took = end - start;
    return { rate: arrived / 1048576 / (took / 1000), fps: watch.frames / (took / 1000), longest: watch.longest, drawn, behind: end - last, webgl: x.webgl };
  }

  /// A key's way round: written to the pty, echoed by it, back as an
  /// event, parsed, drawn. `cat` holds the terminal; the pty echoes.
  async function keys(S) {
    const x = await tab(S, 'keys', 'cat');
    await sleep(600);
    const parsed = [], drawn = [];
    for (let i = 0; i < 60; i++) {
      const t = now();
      const wrote = new Promise(done => { const d = x.term.onWriteParsed(() => { d.dispose(); done(now() - t); }); });
      const drew = new Promise(done => {
        const d = x.term.onRender(() => { d.dispose(); done(now() - t); });
        setTimeout(() => { d.dispose(); done(null); }, 500);
      });
      await invoke('term_write', { id: x.id, data: i % 40 === 39 ? '\r' : 'a' });
      parsed.push(await wrote);
      const d = await drew;
      if (d !== null) drawn.push(d);
      await sleep(60 + Math.random() * 40);
    }
    S.close(x);
    const sorted = list => [...list].sort((a, b) => a - b);
    const mid = list => sorted(list)[list.length >> 1];
    const worst = list => sorted(list)[Math.floor(list.length * 0.95)];
    return { parsed: mid(parsed), drawn: drawn.length ? mid(drawn) : null, worst: drawn.length ? worst(drawn) : worst(parsed) };
  }

  const CASES = [
    { name: 'DOM renderer', webgl: false, opacity: 1, blur: 0 },
    { name: 'WebGL renderer', webgl: true, opacity: 1, blur: 0 },
    { name: 'DOM renderer, see-through and blurred', webgl: false, opacity: 0.8, blur: 3 },
    { name: 'WebGL renderer, see-through and blurred', webgl: true, opacity: 0.8, blur: 3 },
  ];

  try {
    const S = await until('the Console page', 30000, () => window.__shellder && window.__shellder.run && window.__shellder.terms.some(x => x.term) ? window.__shellder : null);
    await sleep(1500);
    // The frames of a window nobody draws (a screen asleep, a window
    // covered) say nothing.
    const idle = watchFrames();
    await sleep(1000);
    idle.on = false;
    const first = S.terms.find(x => x.term);
    await say(`terminal rendering in the app's window (xterm.js ${first.term.cols}x${first.term.rows}, ${first.term.options.fontSize} px, pixel ratio ${window.devicePixelRatio}, ${idle.frames} frames a second at rest)`);
    if (document.hidden || idle.frames < 20) await say('  the window is not being drawn (a screen asleep or locked, the window covered): the frames below say nothing, the rates still do');

    for (const c of CASES) {
      await invoke('set_setting', { key: 'terminal.webgl', value: c.webgl });
      await invoke('set_setting', { key: 'terminal.opacity', value: c.opacity });
      await invoke('set_setting', { key: 'terminal.blurStrength', value: c.blur });
      await sleep(800);
      await say(`\n${c.name}`);
      for (const work of BENCH.works) {
        try {
          const r = await play(S, work);
          await row(work.name, `${r.rate.toFixed(1)} MB/s`,
            `${Math.round(r.fps)} fps, ${r.drawn} frames drawn, longest stall ${Math.round(r.longest)} ms, ${Math.round(r.behind)} ms behind at the end` +
            (r.webgl !== c.webgl ? ` (drawn by ${r.webgl ? 'WebGL' : 'the DOM'}, not as asked)` : ''));
        } catch (e) {
          await row(work.name, 'failed', String(e && e.message || e));
        }
        await sleep(300);
      }
      try {
        const k = await keys(S);
        await row('a key echoed (median of 60)', `${k.parsed.toFixed(1)} ms`,
          `parsed; ${k.drawn === null ? 'never drawn' : `drawn after ${k.drawn.toFixed(1)} ms`}, 19 in 20 within ${k.worst.toFixed(1)} ms`);
      } catch (e) {
        await row('a key echoed', 'failed', String(e && e.message || e));
      }
    }
  } catch (e) {
    await say(`error: ${e && e.stack || e}`);
  }
  await say('DONE');
  invoke('quit').catch(() => {});
})();
