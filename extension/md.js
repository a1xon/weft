// weft's own Markdown viewer. A Markdown file opened from this computer (dropped on the browser,
// say) comes here (md-open.js), and so does one the weft button is pressed on. It is drawn as it
// reads, every block knowing the lines of the file it comes from; *source* shows the file as it is,
// line by line. Selected text is a mark at once (highlighted, with its own note); with the picker on
// a click marks a block, ↑ its list, its section, the whole file. What a mark is (its lines, the
// text, those lines of the file) the picker (mark.js) learns through `__weftDoc`.
import markdownit from './mdit/markdown-it.min.mjs';
import { $, src, file, name, say, bytes, cannotRead, ready, buttons, selecting } from './view.mjs';

const readEl = $('#read'), srcEl = $('#src');
const VIEWER = location.href.split(/[?#]/)[0];
let lines = [], title = name, shown = readEl;

const KIND = {
  h1: 'heading', h2: 'heading', h3: 'heading', h4: 'heading', h5: 'heading', h6: 'heading', p: 'paragraph',
  ul: 'list', ol: 'list', li: 'item', blockquote: 'quote', pre: 'code', code: 'code', table: 'table', tr: 'row', hr: 'divider',
};
// a block's lines of the file: `data-l="a-b"`, from 1
const span = el => (el.dataset.l || '1-1').split('-').map(Number);
const kindOf = el => el === readEl || el === srcEl ? 'file' : el.classList.contains('sec') ? 'section'
  : el.classList.contains('sl') ? 'line' : el.dataset.k || KIND[el.localName] || 'block';
const L = (a, b) => (a === b ? `line ${a}` : `lines ${a}–${b}`);
const cut = (s, n) => (s.length > n ? s.slice(0, n) + '…' : s);
const source = (a, b) => cut(lines.slice(a - 1, b).join('\n'), 4000);
// (the blank lines after a block are not its own)
const trimmed = ([a, b]) => { while (b - 1 > a && !(lines[b - 1] || '').trim()) b--; return [a + 1, Math.max(a + 1, b)]; };

// ------------------------------------------------------------------ reading Markdown
const mdi = markdownit({ linkify: true });
mdi.core.ruler.push('weft_lines', state => {
  for (const t of state.tokens) {
    if (!t.map || t.nesting < 0 || t.type === 'inline' || /^t(head|body)_open$/.test(t.type)) continue;
    t.attrSet('data-l', trimmed(t.map).join('-'));
  }
});
// the file's images are loaded from beside it (below), not from the extension
const image = mdi.renderer.rules.image;
mdi.renderer.rules.image = (tokens, i, ...rest) => {
  const t = tokens[i];
  t.attrSet('data-src', t.attrGet('src') || '');
  t.attrs = t.attrs.filter(([k]) => k !== 'src');
  return image(tokens, i, ...rest);
};
// YAML front matter is no Markdown: it is shown as a block of its own (as many lines, so every line keeps its number)
function prepared() {
  const out = lines.slice();
  if (/^---\s*$/.test(out[0] || '')) {
    const end = out.findIndex((l, i) => i > 0 && /^(---|\.\.\.)\s*$/.test(l));
    if (end > 0) { out[0] = '```yaml'; out[end] = '```'; }
  }
  return out.join('\n');
}
const make = (tag, cls) => { const el = document.createElement(tag); el.className = cls; return el; };

// A heading and what follows it, up to the next heading of its level or above, is a section (↑ from a block in it).
// `head` says the level of a child that starts one.
function sections(root, head) {
  const stack = [{ el: root, lvl: 0 }];
  for (const k of [...root.children]) {
    const lvl = head(k);
    if (lvl) {
      while (stack[stack.length - 1].lvl >= lvl) stack.pop();
      const sec = make('section', 'sec');
      stack[stack.length - 1].el.appendChild(sec);
      stack.push({ el: sec, lvl });
    }
    stack[stack.length - 1].el.appendChild(k);
  }
  for (const sec of root.querySelectorAll('.sec')) {
    const all = [...sec.querySelectorAll('[data-l]')].map(span);
    sec.dataset.l = `${all[0][0]}-${Math.max(...all.map(s => s[1]))}`;
  }
  root.dataset.l = `1-${lines.length}`;
}

function drawRead() {
  readEl.innerHTML = mdi.render(prepared());
  // a fenced block's lines go on the block (markdown-it puts them on its <code>)
  for (const c of readEl.querySelectorAll('pre > code[data-l]')) { c.parentElement.dataset.l = c.dataset.l; c.removeAttribute('data-l'); }
  sections(readEl, k => (/^h[1-6]$/.test(k.localName) ? +k.localName[1] : 0));
  for (const el of readEl.querySelectorAll(':scope > [data-l]:not(.sec), .sec > [data-l]:not(.sec)')) {
    const [a, b] = span(el);
    el.dataset.g = a === b ? `${a}` : `${a}–${b}`;
  }
  // headings can be linked to (#install), as on GitHub
  const used = new Map();
  for (const h of readEl.querySelectorAll('h1, h2, h3, h4, h5, h6')) {
    const id = h.textContent.trim().toLowerCase().replace(/[^\p{L}\p{N}\s_-]/gu, '').replace(/\s/g, '-');
    const n = used.get(id) || 0;
    used.set(id, n + 1);
    h.id = n ? `${id}-${n}` : id;
  }
  // links are the file's: another Markdown file opens here too, the rest in a new tab
  for (const a of readEl.querySelectorAll('a[href]')) {
    const h = a.getAttribute('href');
    if (h.startsWith('#')) continue;
    let u;
    try { u = new URL(h, src); } catch { continue; }
    if (/\.(md|markdown|mdown|mkdn?)$/i.test(u.pathname)) {
      const hash = u.hash;
      u.hash = '';
      a.href = `${VIEWER}?file=${encodeURIComponent(u.href)}${hash}`;
    } else { a.href = u.href; a.target = '_blank'; a.rel = 'noopener noreferrer'; }
  }
  for (const img of readEl.querySelectorAll('img[data-src]')) {
    let u;
    try { u = new URL(img.dataset.src, src); } catch { continue; }
    if (u.protocol === 'file:') {
      fetch(u).then(r => (r.ok ? r.blob() : null)).then(b => { if (b) img.src = URL.createObjectURL(b); }).catch(() => {});
    } else if (/^(https?|data):$/.test(u.protocol)) img.src = u.href;
  }
}

// the file as it is: a line per row, in the blocks and sections the read view has
function drawSource() {
  const line = n => {
    const d = make('div', 'sl');
    d.dataset.n = String(n + 1);
    d.dataset.l = `${n + 1}-${n + 1}`;
    d.textContent = lines[n];
    return d;
  };
  const kids = [];
  let i = 0;
  for (const t of mdi.parse(prepared(), {})) {
    if (t.level !== 0 || !t.map || t.nesting < 0 || t.type === 'inline') continue;
    while (i < t.map[0]) kids.push(line(i++));
    const [a, b] = trimmed(t.map);
    const blk = make('div', 'blk');
    blk.dataset.l = `${a}-${b}`;
    blk.dataset.k = t.type === 'fence' || t.type === 'code_block' ? 'code' : KIND[t.tag] || 'block';
    if (t.type === 'heading_open') blk.dataset.h = t.tag[1];
    for (i = Math.max(i, a - 1); i < b; i++) blk.appendChild(line(i)).classList.toggle('h', !!blk.dataset.h);
    kids.push(blk);
  }
  while (i < lines.length) kids.push(line(i++));
  srcEl.replaceChildren(...kids);
  sections(srcEl, k => +(k.dataset.h || 0));
}

// ------------------------------------------------------------------ read / source
// (the line at the top of the view stays at the top)
const blocks = () => shown.querySelectorAll('[data-l]:not(.sec)');
function topLine() {
  for (const el of blocks()) { const r = el.getBoundingClientRect(); if (r.height && r.bottom > 52) return span(el)[0]; }
  return 1;
}
function goLine(n) {
  let best = null;
  for (const el of blocks()) { if (span(el)[0] <= n) best = el; else break; }
  if (best) scrollTo(0, best.getBoundingClientRect().top + scrollY - 56);
}
function view(v, keep) {
  const at = keep ? topLine() : 0;
  if (v === 'src' && !srcEl.childElementCount) drawSource();
  shown = v === 'src' ? srcEl : readEl;
  readEl.hidden = v === 'src';
  srcEl.hidden = v !== 'src';
  $('#v-read').classList.toggle('on', v !== 'src');
  $('#v-src').classList.toggle('on', v === 'src');
  try { localStorage.setItem('weft.md.view', v); } catch {}
  if (at > 1) goLine(at);
}
$('#v-read').addEventListener('click', () => view('read', true));
$('#v-src').addEventListener('click', () => view('src', true));

async function load() {
  if (!src) { say('no file given', true); return; }
  // (never inside another page: what it shows is the person's)
  if (top !== self) { say('weft shows Markdown in a tab of its own', true); return; }
  $('#name').textContent = name;
  document.title = `${name} · weft`;
  let data;
  try { data = await bytes(); } catch (e) { await cannotRead(e); return; }
  lines = new TextDecoder().decode(data).split(/\r?\n/);
  if (lines.length > 1 && lines[lines.length - 1] === '') lines.pop();
  drawRead();
  const h1 = readEl.querySelector('h1');
  if (h1 && h1.textContent.trim()) title = h1.textContent.trim();
  if (title !== name) { $('#name').textContent = `${title} · ${name}`; document.title = `${title} · weft`; }
  $('#at').textContent = `${lines.length} lines`;
  $('#tip').innerHTML = 'select text to mark it · <b>mark</b> picks a block';
  let v = 'read';
  try { v = localStorage.getItem('weft.md.view') || 'read'; } catch {}
  view(v);
  if (location.hash.length > 1 && location.hash !== '#mark') {
    const el = document.getElementById(decodeURIComponent(location.hash.slice(1)));
    if (el && !srcEl.contains(el)) { view('read'); el.scrollIntoView(); }
  }
  ready();
}

// ------------------------------------------------------------------ marking
buttons($('#mark'));
selecting(() => shown);

const block = n => { const el = n && (n.nodeType === 1 ? n : n.parentElement); return el && el.closest('[data-l]'); };
// the outermost of the blocks around one that are the same lines (a list item and its paragraph)
function outer(el) {
  for (let p = block(el.parentElement); p && shown.contains(p) && p.dataset.l === el.dataset.l; p = block(el.parentElement)) el = p;
  return el;
}
// The lines selected text is on: those of the blocks it starts and ends in, narrowed to the ones
// that hold the text (compared without the marks Markdown makes and the page does not show).
const plain = s => s.replace(/!?\[([^\]]*)\]\([^)]*\)/g, '$1').replace(/^\s{0,3}(?:[-+*]|\d+[.)])\s+(?:\[[ xX]\]\s+)?/, '')
  .replace(/[*_~`#>|\\]/g, '').replace(/\s+/g, ' ').trim().toLowerCase();
function locate(range, quote) {
  const s = block(range.startContainer), e = block(range.endContainer);
  if (!s || !e) return [1, lines.length];
  const a = span(s)[0], b = Math.max(span(e)[1], a);
  let t = '';
  const at = [];
  for (let n = a; n <= b; n++) {
    const p = plain(lines[n - 1] || '');
    if (!p) continue;
    if (t) { t += ' '; at.push(n); }
    t += p;
    for (let k = 0; k < p.length; k++) at.push(n);
  }
  const q = quote.split('\n').map(plain).filter(Boolean).join(' ');
  if (!q) return [a, b];
  // (the same words may be there twice: from about where the selection starts in its block)
  const r0 = document.createRange();
  r0.setStart(s, 0);
  r0.setEnd(range.startContainer, range.startOffset);
  const from = Math.max(0, plain(r0.toString()).length - 8);
  let i = t.indexOf(q, from);
  if (i < 0) i = t.indexOf(q);
  if (i >= 0) return [at[i], at[i + q.length - 1]];
  // (a link's address or other text the page does not show is in between: its first and last words)
  const k = Math.min(24, q.length);
  const i0 = t.indexOf(q.slice(0, k), from), i1 = t.lastIndexOf(q.slice(-k));
  if (i0 >= 0 && i1 >= i0) return [at[i0], at[i1 + k - 1]];
  return [a, b];
}
const overlaps = (x, r) => x.height > 0 && x.right > r.left && x.left < r.right && x.bottom > r.top && x.top < r.bottom;
function about(a, b, label, kind, text, r, area) {
  return {
    label, selector: '', text, html: '', styles: '', attrs: {}, area, comp: null, hints: {},
    rect: { x: Math.round(r.left), y: Math.round(r.top), w: Math.round(r.width), h: Math.round(r.height) },
    md: { from: a, to: b, kind, source: source(a, b) },
  };
}

globalThis.__weftDoc = {
  key: src.replace(/[^/]*$/, ''),
  hint: 'select text to highlight it · click a block · <b>↑ ↓</b> block / list / section · <b>esc</b> stop',
  select: true,
  root: () => shown,
  selectable: t => !!t && t.nodeType === 1 && shown.contains(t),
  css: '#read,#read *,#src,#src *{cursor:text!important}',
  gutter: () => shown.getBoundingClientRect().right - 24,
  // the block selected text starts in (the whole list, quote or table)
  around(range) {
    let b = block(range.startContainer);
    while (b && b.parentElement && b.parentElement !== shown && !b.parentElement.classList.contains('sec')) b = b.parentElement;
    return b && shown.contains(b) ? b : null;
  },
  // where the text starts: marked text has its number left of it
  edge: () => shown.getBoundingClientRect().left + (shown === readEl ? 40 : 0),
  page: () => ({ url: src, title, w: innerWidth, h: innerHeight, dpr: devicePixelRatio, md: { file, lines: lines.length } }),
  pick(t) { const b = block(t); return b && shown.contains(b) ? outer(b) : null; },
  // ↑: the block around it (a list, a quote), its section, the whole file
  up(el) {
    let p = block(el.parentElement);
    while (p && p.dataset.l === el.dataset.l) p = block(p.parentElement);
    return p && shown.contains(p) ? outer(p) : null;
  },
  label: el => `${L(...span(el))} · ${kindOf(el)}`,
  describe(el, r, area) {
    if (area) {
      // (an area drawn beside the text: the blocks it touches)
      const hit = [...blocks()].filter(e => overlaps(e.getBoundingClientRect(), r)).map(span);
      const a = hit.length ? Math.min(...hit.map(s => s[0])) : 1, b = hit.length ? Math.max(...hit.map(s => s[1])) : lines.length;
      return about(a, b, `${L(a, b)} · area`, 'area', cut(lines.slice(a - 1, b).join('\n'), 3000), r, true);
    }
    const [a, b] = span(el), kind = kindOf(el);
    const head = kind === 'section' && el.firstElementChild ? ` “${cut(el.firstElementChild.textContent.replace(/^\s*#+\s*/, '').trim(), 60)}”` : '';
    return about(a, b, `${L(a, b)} · ${kind}${head}`, kind, cut((el.innerText || '').trim(), 3000), r, false);
  },
  describeText(range, r, quote) {
    const [a, b] = locate(range, quote);
    return about(a, b, `${L(a, b)} · text`, 'text', cut(quote, 3000), r, false);
  },
  // (no screenshot: the browser gives none of an extension's own page, and the lines say more)
  capture: async () => ({ none: true }),
};

load();
