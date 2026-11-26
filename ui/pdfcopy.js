// What a copy from a PDF's words gives. A PDF knows lines, not
// paragraphs: every line of the page ends where the page ended it, a
// word cut in two with a hyphen there. A copy puts a paragraph's lines
// back into one: the lines are read from the page's words with where
// they lie (`linesOf`), the ones that begin a paragraph are told by that
// (`paragraphStarts`), and the rest are joined to the line before them,
// the hyphen of a word cut at the line's end taken out (`joined`).
// Only the first is the page's; the other two know no DOM, and
// Tests/pdf-copy.sh runs them in node.

/// Lengths are told in the line's own letter size, so that the page's
/// zoom is not in them.
const SAME_SIZE = 0.12;   // two lines' sizes this near, in shares of the larger, are one size
const MIN_PITCH = 0.5;    // from a line's top to the next one's, in a run of lines: no less,
const MAX_PITCH = 2.5;    // and no more
const WIDE_PITCH = 1.3;   // a step this many times the run's own: a gap between paragraphs
const MAX_SHIFT = 4;      // how far a line's left edge is from the one before it, in a run
const INDENT = 0.5;       // a left edge this far right of another is indented
const SPACE = 0.2;        // the least a space takes
const SLACK = 0.1;        // how far a full line may stop short of the edge
const ROOMY = 1.5;        // room for this many times the next word: to spare
const SAME_PACE = 3e-4;   // two lines' widths a letter this near, in shares of it, are one
const BULLET = /^\s*[•◦▪■□●○◆‣⁃►▶·∙*]\s/u;
const MARKER = /^\s*(\[[\w,\s-]+\]|\(?\w{1,3}[.)])\s/u;
const SENTENCE_END = /[.!?:。！？：]["'”’)\]]*\s*$/u;
/// Scripts written without spaces between words: their lines join with nothing between them.
const UNSPACED = /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\u3000-\u303F\uFF00-\uFFEF]/u;

const sameSize = (a, b) => Math.abs(a.size - b.size) <= SAME_SIZE * Math.max(a.size, b.size);

/// The runs of lines that follow one another down a column: one page,
/// one size of letters, a steady step from a line to the next, each
/// under the one before. A run's lines share a right edge (`right`, the
/// farthest any of them reaches) and a step (`pitch`, the smallest).
/// Gives the run of every line.
function runsOf(lines) {
  const runs = [];
  let run = null;
  lines.forEach((line, i) => {
    const prev = lines[i - 1];
    const size = prev ? Math.max(prev.size, line.size) : 0;
    const pitch = prev ? line.top - prev.top : 0;
    const follows = prev && prev.page === line.page && sameSize(prev, line)
      && pitch > MIN_PITCH * size && pitch < MAX_PITCH * size
      && Math.min(prev.right, line.right) > Math.max(prev.left, line.left)
      && Math.abs(line.left - prev.left) <= MAX_SHIFT * size;
    if (follows) {
      run.to = i;
      run.right = Math.max(run.right, line.right);
      run.left = Math.min(run.left, line.left);
      run.pitch = Math.min(run.pitch, pitch);
    } else {
      run = { from: i, to: i, left: line.left, right: line.right, pitch: Infinity };
    }
    runs.push(run);
  });
  return runs;
}

/// Whether `next`'s first word had room after `line`'s last, in a column
/// that ends at `right`: a line stops before its column's edge only
/// where its paragraph does, so this is a paragraph's last line. It
/// holds for text set flush as for text set ragged, and for a table's
/// or a program's lines, which are short and so stay lines. Ragged
/// lines are not always filled to the last word that fits (TeX's are
/// not): a line that stops in the middle of a sentence, the next going
/// on in a small letter, ended its paragraph only with room to spare.
function endsShort(line, next, right) {
  const midSentence = !SENTENCE_END.test(line.text) && /^\s*\p{Ll}/u.test(next.text);
  return line.right + SPACE * line.size + (midSentence ? ROOMY : 1) * next.word < right - SLACK * line.size;
}

/// Two lines set about one middle, neither edge shared: a title's lines,
/// an author's and a date's. Each stands alone.
const centred = (a, b) => Math.abs(a.left - b.left) > a.size && Math.abs((a.left - b.left) + (a.right - b.right)) < INDENT * a.size;

/// The lines whose letters are all one width: a program's, which stay
/// lines (the longest line of a listing reaches the listing's edge as a
/// paragraph's lines do, and a name without a space in it is a first
/// word no line has room for). pdf.js does not say it of a font (TeX's
/// typewriter comes as a serif), but the lines do: three or more one
/// after another, not all as wide, each as wide as its letters are
/// many. Lines set flush are as wide as one another whatever their
/// letters, and writing without spaces is all one width and no program.
function fixedWidth(lines) {
  const fixed = lines.map(() => false);
  const paced = (a, b) => a.pace > 0 && b.pace > 0 && a.page === b.page && sameSize(a, b)
    && Math.abs(a.pace - b.pace) <= SAME_PACE * a.pace && !UNSPACED.test(a.text) && !UNSPACED.test(b.text);
  let from = 0;
  for (let i = 1; i <= lines.length; i++) {
    if (i < lines.length && paced(lines[i - 1], lines[i])) continue;
    const some = lines.slice(from, i);
    if (some.length >= 3 && some.some(l => Math.abs(l.span - some[0].span) > 0.5 * l.pace)) fixed.fill(true, from, i);
    from = i;
  }
  return fixed;
}

/// For every line, whether it begins a paragraph (the first always).
///
/// A line is `{ text, left, right, top, size, word, page, pace, span }`:
/// its words, its edges and its top in any one unit, the height of its
/// letters, the width of its first word, the page it is on, and of its
/// longest run of letters the width a letter (`pace`, 0 when not known)
/// and the width.
export function paragraphStarts(lines) {
  const runs = runsOf(lines);
  const fixed = fixedWidth(lines);
  return lines.map((line, i) => {
    if (i === 0) return true;
    const prev = lines[i - 1];
    const run = runs[i];
    if (BULLET.test(line.text) || fixed[i] || fixed[i - 1]) return true;
    if (runs[i - 1] !== run) return !continues(lines, runs, i);
    if (line.top - prev.top > WIDE_PITCH * run.pitch) return true;
    if (endsShort(prev, line, run.right)) return true;
    if (centred(prev, line)) return true;
    // The paragraph before ended in a full line: what is left to tell
    // this one by is its first line set in from the lines around it,
    // after a sentence's end. A list whose items hang (a number, then
    // lines set in under it) looks the same from its second line on,
    // and is told by the marker its items begin with.
    const size = line.size;
    if (line.left - prev.left > INDENT * size && SENTENCE_END.test(prev.text)) {
      const next = runs[i + 1] === run ? lines[i + 1] : null;
      if (next ? next.left < line.left - INDENT * size && !MARKER.test(next.text) : !MARKER.test(prev.text)) return true;
    }
    return false;
  });
}

/// Whether line `i`, the first of its run, goes on with the paragraph of
/// the line before it, the last of another run: a column's foot and the
/// head of the next column or the next page, the same size of letters,
/// the first a full line of a run of several (a page's number is a run
/// of one) and the second not set in.
function continues(lines, runs, i) {
  const prev = lines[i - 1];
  const line = lines[i];
  const before = runs[i - 1];
  const run = runs[i];
  if (!sameSize(prev, line) || before.from === before.to) return false;
  const jumped = prev.page !== line.page || (line.top < prev.top && line.left > prev.right - prev.size);
  if (!jumped) return false;
  if (endsShort(prev, line, before.right)) return false;
  return line.left <= run.left + INDENT * line.size;
}

/// Two lines of a paragraph as one. A line that ends in a hyphen after a
/// letter, the next beginning in a small one, is a word cut in two and
/// loses the hyphen, unless `corpus` (the lines of the pages the copy
/// is from, in small letters: their words as laid out, nothing is read
/// for it) has the word with its hyphen inside a line and nowhere
/// without: "counter-based" keeps it, "neigh-boring" does not. A word
/// with a hyphen that those pages have nowhere else is taken for cut.
/// Before a capital or a number the hyphen stays. A soft hyphen always
/// goes. Other lines get a space between them, none where the writing
/// has none.
function glue(a, b, corpus) {
  a = a.trimEnd();
  b = b.trimStart();
  if (!a || !b) return a + b;
  if (a.endsWith('\u00AD')) return a.slice(0, -1) + b;
  if (/[\p{L}\p{N}][-\u2010\u2011]$/u.test(a)) {
    const head = /(\p{L}+).$/u.exec(a);
    const tail = /^\p{Ll}+/u.exec(b);
    if (head && tail) {
      const whole = (head[1] + tail[0]).toLowerCase();
      const cut = `${head[1]}-${tail[0]}`.toLowerCase();
      const kept = corpus.includes(cut) && !corpus.includes(whole);
      return kept ? a + b : a.slice(0, -1) + b;
    }
    if (/^[\p{L}\p{N}]/u.test(b)) return a + b;
  }
  // A dash between two words or two numbers has no space on either side.
  if (/[\p{L}\p{N}][\u2013\u2014]$/u.test(a) && /^[\p{L}\p{N}]/u.test(b)) return a + b;
  if (UNSPACED.test(a[a.length - 1]) && UNSPACED.test(b[0])) return a + b;
  return `${a} ${b}`;
}

/// The text of the lines that have any of it chosen (`picked`, a line's
/// chosen part): a paragraph a line, as `starts` tells them.
export function joined(lines, starts) {
  const corpus = lines.map(line => line.text.toLowerCase().replaceAll('\u2010', '-')).join('\n');
  let out = '';
  let began = false; // a paragraph began since the last line taken
  let any = false;
  lines.forEach((line, i) => {
    if (starts[i]) began = true;
    if (!line.picked || !line.picked.trim()) return;
    if (!any) out = line.picked.trim();
    else if (began) out += `\n${line.picked.trim()}`;
    else out = glue(out, line.picked, corpus);
    any = true;
    began = false;
  });
  return out;
}

/// A page's lines, from the words pdf.js laid out in `layer` (a span a
/// run of letters, a `<br>` after the last of a line), each with the
/// part of it that `range` covers. What pdf.js ends a line at is a step
/// up or down as well, a raised number's or a table's next cell: a piece
/// that begins level with the line before it and to its right is that
/// line's.
export function linesOf(layer, range, page) {
  const lines = [];
  let line = null;
  let broke = true; // a line's end was passed: the next words begin a piece
  for (const node of layer.querySelectorAll('span:not(.markedContent), br')) {
    if (node.localName === 'br') { broke = true; continue; }
    if (node.getAttribute('role') === 'img') continue;
    const text = node.textContent;
    if (!text) continue;
    let picked = '';
    for (const child of node.childNodes) {
      if (child.nodeType !== Node.TEXT_NODE || !range.intersectsNode(child)) continue;
      picked += child.data.slice(child === range.startContainer ? range.startOffset : 0, child === range.endContainer ? range.endOffset : child.length);
    }
    const blank = !text.trim();
    const box = node.getBoundingClientRect();
    if (broke && !blank) {
      const level = line && box.left >= line.right - 0.5 * line.size
        && Math.min(box.bottom, line.top + line.size) - Math.max(box.top, line.top) >= 0.5 * Math.min(box.height, line.size);
      if (level) {
        if (!/\s$/.test(line.text)) { line.text += ' '; if (line.picked && picked) line.picked += ' '; }
      } else {
        line = { text: '', picked: '', left: Infinity, right: -Infinity, top: box.top, size: box.height, word: 0, page, pace: 0, span: 0, most: 0 };
        lines.push(line);
      }
      broke = false;
    }
    if (!line) continue;
    line.text += text;
    line.picked += picked;
    if (blank || !box.width) continue;
    if (!line.word) {
      const lead = text.length - text.trimStart().length;
      // Where the writing has no spaces a line may end after any letter.
      const first = UNSPACED.test(text.trim()[0]) ? 1 : text.trim().split(/\s+/)[0].length;
      line.word = box.width * (lead + first) / text.length;
    }
    line.left = Math.min(line.left, box.left);
    line.right = Math.max(line.right, box.right);
    // The line's top and the size of its letters are those of its longest
    // run (a single letter's width is not the page's: pdf.js leaves it
    // as the stand-in font has it).
    if (text.length > line.most) { line.most = text.length; line.top = box.top; line.size = box.height; line.span = box.width; line.pace = text.length > 1 ? box.width / text.length : 0; }
  }
  return lines.filter(l => l.right > l.left && l.size > 0);
}

/// What a copy of `range` gives, from the words of the pages it is on
/// (`layers`, in the document's order).
export function copied(range, layers) {
  const lines = layers.flatMap((layer, page) => linesOf(layer, range, page));
  return joined(lines, paragraphStarts(lines)).replaceAll('\0', '');
}
