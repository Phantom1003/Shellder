// The lines of a made-up page, set as a typesetter would, and what a
// copy of them gives. Run by Tests/pdf-copy.sh.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { paragraphStarts, joined } from '../../ui/pdfcopy.js';

const SIZE = 10;   // the letters' height
const LETTER = 5;  // a letter's width: every one the same
const PITCH = 12;  // from a line's top to the next one's

/// A column being set: `line` puts the next line under the last.
function column({ left = 100, width = 240, top = 100, page = 0, size = SIZE } = {}) {
  const lines = [];
  let at = top;
  const col = {
    lines,
    /// A line of the column. `full`: set flush, to the column's edge.
    /// `indent`: its left edge, in letters. `gap`: extra room above.
    /// `mono`: its letters are known to be all one width.
    line(text, { full = false, indent = 0, gap = 0, picked = text, centre = false, mono = false } = {}) {
      at += gap;
      const w = text.length * LETTER * size / SIZE;
      let l = left + indent * LETTER;
      if (centre) l = left + (width - w) / 2;
      lines.push({ text, picked, left: l, right: full ? left + width : l + w, top: at, size, word: text.trim().split(/\s+/)[0].length * LETTER * size / SIZE, page, pace: mono ? LETTER : 0, span: w });
      at += PITCH * size / SIZE;
      return col;
    },
    /// A paragraph, the lines as given: all but the last set flush.
    paragraph(texts, { indent = 0, hang = 0, gap = 0 } = {}) {
      texts.forEach((text, i) => col.line(text, { full: i < texts.length - 1, indent: i ? hang : indent, gap: i ? 0 : gap }));
      return col;
    },
  };
  return col;
}

const copy = lines => joined(lines, paragraphStarts(lines));

test('a paragraph is one line, a word cut at a line\'s end put together', () => {
  const c = column({ width: 350 }).paragraph([
    'In DRAM, repeatedly activating aggressor rows may disturb neigh-',
    'boring victim rows until their bits flip [14]. Modern defenses against',
    'this Rowhammer threat count activations and refresh victims before',
    'their aggressors are activated too often [13, 22, 25, 26, 34, 36, 39].',
    'Defenses aim to guarantee that victims have absorbed too little',
    'disturbance to fail as long as counters stay below their thresholds.',
  ]);
  assert.equal(copy(c.lines), 'In DRAM, repeatedly activating aggressor rows may disturb neighboring victim rows until their bits flip [14]. Modern defenses against this Rowhammer threat count activations and refresh victims before their aggressors are activated too often [13, 22, 25, 26, 34, 36, 39]. Defenses aim to guarantee that victims have absorbed too little disturbance to fail as long as counters stay below their thresholds.');
});

test('only what was chosen is copied', () => {
  const c = column({ width: 350 })
    .line('In DRAM, repeatedly activating aggressor rows may disturb neigh-', { full: true, picked: 'disturb neigh-' })
    .line('boring victim rows until their bits flip [14]. Modern defenses against', { full: true })
    .line('this Rowhammer threat count activations and refresh victims before', { full: true, picked: 'this Rowhammer' })
    .line('their aggressors are activated too often.', { picked: '' });
  assert.equal(copy(c.lines), 'disturb neighboring victim rows until their bits flip [14]. Modern defenses against this Rowhammer');
});

test('a paragraph ends at a line that stops short', () => {
  const c = column()
    .paragraph(['The first paragraph has two lines of which the second', 'is short.'])
    .paragraph(['The second paragraph follows it at once and is also of', 'two lines.'], { indent: 3 });
  assert.equal(copy(c.lines), 'The first paragraph has two lines of which the second is short.\nThe second paragraph follows it at once and is also of two lines.');
});

test('a paragraph after a full last line is told by its first line set in', () => {
  const c = column()
    .paragraph(['The first paragraph has two lines of which the second', 'is as long as the first, to the very edge of the column.'])
    .line('The second paragraph follows it at once and is also of', { full: true, indent: 3 })
    .line('two lines.');
  c.lines[1].right = 340;
  assert.equal(copy(c.lines).split('\n').length, 2);
});

test('a list\'s items stay apart, each one line', () => {
  const c = column()
    .paragraph(['[1] A. Author and B. Writer. A title that is long enough to', 'go over the line. In Proceedings, 2014.'], { hang: 4 })
    .paragraph(['[2] C. Scribe. A short one. 2020.'])
    .paragraph(['[3] D. Person. Another title, long enough to go over the', 'line as the first did. 2021.'], { hang: 4 });
  assert.deepEqual(copy(c.lines).split('\n'), [
    '[1] A. Author and B. Writer. A title that is long enough to go over the line. In Proceedings, 2014.',
    '[2] C. Scribe. A short one. 2020.',
    '[3] D. Person. Another title, long enough to go over the line as the first did. 2021.',
  ]);
  const bullets = column()
    .paragraph(['• A formal model of mitigation that relates counters to', 'the disturbance.'], { hang: 2 })
    .paragraph(['• An evaluation of five designs whose counters are not', 'sound.'], { hang: 2 });
  bullets.lines[1].right = 340; // a full line before the next item
  assert.equal(copy(bullets.lines).split('\n').length, 2);
});

test('a program\'s lines and a table\'s rows stay lines', () => {
  const code = column()
    .line('for (i = 0; i < n; i++) {', { mono: true })
    .line('activate(row[i]);', { indent: 4, mono: true })
    .line('count[i] += 1;', { indent: 4, mono: true })
    .line('}')
    .paragraph(['The loop above is the simplest pattern, and what follows', 'it is a paragraph.']);
  assert.equal(copy(code.lines), 'for (i = 0; i < n; i++) {\nactivate(row[i]);\ncount[i] += 1;\n}\nThe loop above is the simplest pattern, and what follows it is a paragraph.');
  const table = column()
    .paragraph(['A paragraph as wide as the column, before the table and', 'above it.'])
    .line('Design Sound Cost').line('Alpha yes 12').line('Beta no 7');
  assert.equal(copy(table.lines), 'A paragraph as wide as the column, before the table and above it.\nDesign Sound Cost\nAlpha yes 12\nBeta no 7');
});

test('a heading, and a gap between paragraphs', () => {
  const lines = [
    ...column({ size: 14 }).line('1 Introduction').lines,
    ...column({ top: 130 }).paragraph(['A paragraph under the heading, of two lines, of which the', 'second ends it.'])
      .paragraph(['One more after a gap, with no line of it set in, also of', 'two lines.'], { gap: 6 }).lines,
  ];
  lines[2].right = 340; // the first paragraph's last line is full: only the gap tells
  assert.deepEqual(copy(lines).split('\n').map(l => l.slice(0, 5)), ['1 Int', 'A par', 'One m']);
});

test('centred lines stand alone', () => {
  const c = column()
    .line('A. Author', { centre: true })
    .line('November 25, 2026', { centre: true });
  assert.equal(copy(c.lines), 'A. Author\nNovember 25, 2026');
});

test('a paragraph goes on in the next column and on the next page', () => {
  const first = column().paragraph(['Some paragraph that ends', 'here.']).line('A paragraph begun at the foot of the column, its line set', { full: true, indent: 3 });
  const second = column({ left: 360 }).paragraph(['flush, goes on at the head of the next column and then, set', 'flush again, runs to the']);
  second.lines[1].right = 600;
  const third = column({ page: 1, top: 900 }).paragraph(['head of the next page, where', 'it ends.']);
  assert.deepEqual(copy([...first.lines, ...second.lines, ...third.lines]).split('\n'), [
    'Some paragraph that ends here.',
    'A paragraph begun at the foot of the column, its line set flush, goes on at the head of the next column and then, set flush again, runs to the head of the next page, where it ends.',
  ]);
  // A page's number, a line alone, is no paragraph's last line.
  const number = column({ top: 700 }).line('12', { centre: true });
  const next = column({ page: 1, top: 900 }).paragraph(['The next page begins with a line', 'of its own.']);
  assert.equal(copy([...number.lines, ...next.lines]).split('\n')[0], '12');
});

test('a page\'s number and the next page\'s head are stepped over, mid sentence only', () => {
  const foot = () => column({ top: 600 }).paragraph(['(denoted as s) and produces one 64-bit output. Some more', 'words that fill the line to its end, and so We can then']);
  const end = lines => { lines[lines.length - 1].right = 340; return lines; };
  const number = () => column({ top: 700 }).line('5', { centre: true });
  const head = () => column({ page: 1, top: 40 }).line('A Running Head', { centre: true });
  const body = () => column({ page: 1, top: 100 }).paragraph(['substitute every instance of the adder with applications', 'of F.']);
  const whole = '(denoted as s) and produces one 64-bit output. Some more words that fill the line to its end, and so We can then substitute every instance of the adder with applications of F.';
  assert.equal(copy([...end(foot().lines), ...number().lines, ...body().lines]), whole);
  assert.equal(copy([...end(foot().lines), ...number().lines, ...head().lines, ...body().lines]), whole);
  // After a sentence's end a line alone may be a heading: all stay.
  const done = column({ top: 600 }).paragraph(['A paragraph whose last line is as full as the others are', 'and ends with the page, at the end of its last sentence.']);
  const title = column({ page: 1, top: 60 }).line('2.5. The Interplay');
  const after = column({ page: 1, top: 100 }).paragraph(['The analysis above reveals significant potential for the', 'speedup.']);
  assert.equal(copy([...end(done.lines), ...number().lines, ...title.lines, ...after.lines]).split('\n').length, 4);
});

test('a list\'s item goes on at the head of the next column, set in as its lines are', () => {
  const first = column({ top: 500 }).paragraph(['can be abstracted away. The abstraction should satisfy two', 'requirements:'])
    .paragraph(['1) The abstraction must preserve the essential information-', 'flow properties of the module so that relevant'], { hang: 3 });
  first.lines[3].right = 340;
  const second = column({ left: 360 }).paragraph(['information-flow invariants still hold on the abstracted', 'machine;'], { indent: 3, hang: 3 })
    .paragraph(['2) The abstraction must abstract large modules with com-', 'plex state updates.'], { hang: 3 });
  assert.deepEqual(copy([...first.lines, ...second.lines]).split('\n'), [
    'can be abstracted away. The abstraction should satisfy two requirements:',
    '1) The abstraction must preserve the essential information-flow properties of the module so that relevant information-flow invariants still hold on the abstracted machine;',
    '2) The abstraction must abstract large modules with complex state updates.',
  ]);
  // The item's last line alone at the column's head, the next item under it
  // (and nothing left that has the word with its hyphen inside a line).
  const last = column({ left: 360 }).line('machine holds;', { indent: 3 }).paragraph(['2) The abstraction must abstract large modules with com-', 'plex state updates.'], { hang: 3 });
  first.lines[3].text = first.lines[3].picked = 'flow properties of the module so that the abstracted';
  assert.equal(copy([...first.lines, ...last.lines]).split('\n')[1], '1) The abstraction must preserve the essential informationflow properties of the module so that the abstracted machine holds;');
});

test('a ragged line that stops mid sentence with little room goes on', () => {
  const c = column({ width: 300 })
    .line('RAGGED The disturbance of a victim under a trace is the')
    .line('weighted number of activations of its')
    .line('neighbours since the victim was last refreshed, where it')
    .line('ends.');
  c.lines[0].right = 398; // the column's edge is at 400
  c.lines[1].right = 340; // "neighbours" (50) had room, by a little
  assert.equal(copy(c.lines).split('\n').length, 1);
});

test('hyphens: a word\'s own is kept, one at a line\'s end goes', () => {
  const para = (...texts) => copy(column().paragraph(texts).lines);
  assert.equal(para('We study counter-based designs, and every counter-', 'based mitigation.'), 'We study counter-based designs, and every counter-based mitigation.');
  // Nothing on the page says the word has a hyphen of its own: it is taken for cut.
  assert.equal(para('Every design here is counter-', 'based, and so are the mitigations.'), 'Every design here is counterbased, and so are the mitigations.');
  assert.equal(para('The exchange of Diffie-', 'Hellman and a semi-', 'conductor in 2019–', '2021, or pages 10-', '20, and a soft\u00AD', 'hyphen.'),
    'The exchange of Diffie-Hellman and a semiconductor in 2019–2021, or pages 10-20, and a softhyphen.');
  assert.equal(para('A dash at the end -', 'is no hyphen.'), 'A dash at the end - is no hyphen.');
});

test('writing without spaces is joined without one', () => {
  const c = column({ width: 200 });
  const wide = text => ({ text, picked: text, left: 100, right: 100 + text.length * 10, top: 0, size: SIZE, word: 10, page: 0 });
  const lines = ['动态存储器把每一位数据保存在电容的电荷里', '电荷会随时间泄漏。反复激活某一行会干扰', '相邻的行。', '第二段从这里开始并且也有两行文字内容。'].map(wide);
  lines.forEach((l, i) => { l.top = 100 + i * PITCH; });
  assert.equal(copy([...c.lines, ...lines]), '动态存储器把每一位数据保存在电容的电荷里电荷会随时间泄漏。反复激活某一行会干扰相邻的行。\n第二段从这里开始并且也有两行文字内容。');
  assert.equal(copy(column().paragraph(['我们使用 counter', '来统计 the number', 'of activations。']).lines), '我们使用 counter 来统计 the number of activations。');
});
