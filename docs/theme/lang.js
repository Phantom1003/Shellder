// The books' own additions to mdBook's page (book.toml, additional-js):
// a button in the bar that goes to the same page of the other
// language's book, or to that book's first page when it has no such
// page yet, and one back to the site's front page.
(() => {
  'use strict';
  const here = location.pathname;
  const m = here.match(/^(.*)\/(en|zh)\/(.*)$/);
  const bar = document.querySelector('.right-buttons');
  if (!m || !bar) return;
  const [, base, lang, page] = m;
  const other = lang === 'en' ? 'zh' : 'en';
  const link = (href, text, title) => {
    const a = document.createElement('a');
    a.href = href;
    a.textContent = text;
    a.title = title;
    a.className = 'shellder-bar';
    bar.prepend(a);
    return a;
  };
  const there = `${base}/${other}/${page}${location.hash}`;
  const a = link(there, other === 'zh' ? '中文' : 'English', other === 'zh' ? '阅读中文版' : 'Read this in English');
  a.addEventListener('click', async e => {
    e.preventDefault();
    let found = false;
    try { found = (await fetch(there, { method: 'HEAD' })).ok; } catch (_) { /* a file: page, or offline */ }
    location.href = found ? there : `${base}/${other}/index.html`;
  });
  link(`${base}/index.html?lang=${lang}`, 'Shellder', lang === 'zh' ? '回到首页' : 'The front page');
  try { localStorage.setItem('shellder-docs-lang', lang); } catch (_) { /* no storage */ }
})();

// A figure's pin or name under the pointer shows that part's outline.
for (const figure of document.querySelectorAll('figure.shot')) {
  const light = (n, on) => { for (const e of figure.querySelectorAll(`[data-n="${n}"]`)) e.classList.toggle('on', on); };
  for (const e of figure.querySelectorAll('.shot-pin, .shot-legend li')) {
    e.addEventListener('mouseenter', () => light(e.dataset.n, true));
    e.addEventListener('mouseleave', () => light(e.dataset.n, false));
  }
}
