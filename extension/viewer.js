// weft's own PDF viewer. The browser's viewer lets no extension in, so a PDF the weft button is
// pressed on opens here (`viewer.html?file=<its address>`), drawn by PDF.js with its text laid over
// the pages. The text is grouped into lines and paragraphs (invisible boxes on the page), which the
// picker (mark.js, the same as on web pages) marks; what a mark is in a PDF (the page, the text, where
// on the page in points) and its screenshot this page tells the picker through `__weftDoc`.
import * as pdfjs from './pdfjs/pdf.min.mjs';

pdfjs.GlobalWorkerOptions.workerSrc = 'pdfjs/pdf.worker.min.mjs';

const BG = '#20252d';
const $ = s => document.querySelector(s);
const pagesEl = $('#pages'), msg = $('#msg'), zoomBtn = $('#zoom'), atEl = $('#at');
const src = new URLSearchParams(location.search).get('file') || '';
// the file on this machine, for a file:// address
const file = (() => { try { const u = new URL(src); return u.protocol === 'file:' ? decodeURIComponent(u.pathname) : ''; } catch { return ''; } })();
const name = decodeURIComponent((src.split(/[?#]/)[0].split('/').pop()) || 'PDF');

let doc = null, title = name, base = 1, zoom = 1, busy = false;
const pages = [];            // { n, div, canvas, text, w, h (pt), drawn: scale | 0, laid: scale | 0, paras, runs }
const byDiv = new WeakMap(); // page div -> page
const what = new WeakMap();  // a line or paragraph box -> { kind, pg, para, line }
const seen = new Set();      // pages in or near the view
const scale = () => base * zoom;

function say(html, err) { msg.hidden = !html; msg.className = err ? 'err' : ''; msg.innerHTML = html || ''; }
const esc = s => String(s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);

// ------------------------------------------------------------------ the file
async function bytes() {
  const r = await fetch(src);
  if (!r.ok) throw new Error(`${r.status} ${r.statusText}`);
  return new Uint8Array(await r.arrayBuffer());
}

async function load() {
  if (!src) { say('no PDF given', true); return; }
  $('#name').textContent = name;
  document.title = `${name} · weft`;
  say(`opening ${esc(name)}…`);
  let data;
  try { data = await bytes(); } catch (e) { await cannotRead(e); return; }
  try {
    doc = await pdfjs.getDocument({ data, isEvalSupported: false }).promise;
  } catch (e) { say(`${esc(name)} is not a PDF weft can read: ${esc(e && e.message || e)}`, true); return; }
  // (a page printed from the browser has its address as title when it had none)
  try { const t = ((await doc.getMetadata()).info.Title || '').trim(); if (t && !/^(about|https?|file|data|blob):/.test(t)) title = t; } catch {}
  if (title !== name) { $('#name').textContent = `${title} · ${name}`; document.title = `${title} · weft`; }
  const sizes = [];
  for (let n = 1; n <= doc.numPages; n++) {
    const p = await doc.getPage(n);
    const vp = p.getViewport({ scale: 1 });
    sizes.push({ p, w: vp.width, h: vp.height });
  }
  say('');
  for (const [i, s] of sizes.entries()) {
    const div = document.createElement('div');
    div.className = 'pg';
    div.dataset.n = String(i + 1);
    const canvas = document.createElement('canvas');
    const text = document.createElement('div');
    text.className = 'textLayer';
    div.append(canvas, text);
    pagesEl.appendChild(div);
    const pg = { n: i + 1, p: s.p, div, canvas, text, w: s.w, h: s.h, drawn: 0, laid: 0, paras: [], runs: [] };
    pages.push(pg);
    byDiv.set(div, pg);
  }
  fit();
  const io = new IntersectionObserver(es => {
    for (const e of es) {
      const pg = byDiv.get(e.target);
      if (e.isIntersecting) seen.add(pg);
      else { seen.delete(pg); pg.canvas.width = pg.canvas.height = 0; pg.drawn = 0; }
    }
    pump();
  }, { rootMargin: '150% 0px' });
  for (const pg of pages) io.observe(pg.div);
  addEventListener('scroll', where, { passive: true });
  addEventListener('resize', () => { const at = anchor(); fit(); restore(at); });
}

async function cannotRead(e) {
  const why = esc(e && e.message || e);
  if (file) {
    say(`weft cannot read files on this computer. In <code>brave://extensions</code> (<code>chrome://extensions</code>), open the details for weft marks, turn on “Allow access to file URLs” and reload this page.<br><small>${why}</small>`, true);
    return;
  }
  let origin = '';
  try { const u = new URL(src); origin = `${u.protocol}//${u.hostname}/*`; } catch {}
  if (origin && !(await chrome.permissions.contains({ origins: [origin] }).catch(() => false))) {
    say(`to open this PDF, weft needs to read it from ${esc(new URL(src).host)}.<br><small>${why}</small><br><button id="allow">allow ${esc(new URL(src).host)}</button>`, true);
    $('#allow').addEventListener('click', async () => {
      if (await chrome.permissions.request({ origins: [origin] }).catch(() => false)) location.reload();
    });
    return;
  }
  say(`${esc(name)} could not be read: ${why}`, true);
}

// ------------------------------------------------------------------ drawing the pages
// the scale that fits the widest page in the window (at most about A4 at 1000 px wide)
function fit() {
  const widest = Math.max(...pages.map(p => p.w), 1);
  base = Math.min((innerWidth - 32) / widest, 1000 / 595);
  size();
}
function size() {
  const s = scale();
  for (const pg of pages) {
    pg.div.style.width = Math.floor(pg.w * s) + 'px';
    pg.div.style.height = Math.floor(pg.h * s) + 'px';
    pg.div.style.setProperty('--scale-factor', String(s));
  }
  zoomBtn.textContent = Math.round(zoom * 100) + '%';
  pump();
}
async function pump() {
  if (busy) return;
  busy = true;
  try {
    for (;;) {
      const s = scale();
      const next = [...seen].sort((a, b) => a.n - b.n).find(pg => pg.drawn !== s || pg.laid !== s);
      if (!next) break;
      await draw(next, s);
    }
  } finally { busy = false; }
  where();
  ready();
}
async function draw(pg, s) {
  const vp = pg.p.getViewport({ scale: s });
  if (pg.drawn !== s) {
    const dpr = devicePixelRatio || 1;
    const c = document.createElement('canvas');
    c.width = Math.floor(vp.width * dpr); c.height = Math.floor(vp.height * dpr);
    try {
      await pg.p.render({ canvas: c, canvasContext: c.getContext('2d'), viewport: vp, transform: dpr !== 1 ? [dpr, 0, 0, dpr, 0, 0] : null }).promise;
      // (drawn aside and swapped in: the old picture stays until the new one is there)
      pg.canvas.replaceWith(c);
      pg.canvas = c;
      pg.drawn = s;
    } catch { pg.drawn = s; }
  }
  if (pg.laid !== s) {
    const text = document.createElement('div');
    text.className = 'textLayer';
    pg.text.replaceWith(text);
    pg.text = text;
    try { await new pdfjs.TextLayer({ textContentSource: pg.p.streamTextContent(), container: text, viewport: vp }).render(); } catch {}
    lay(pg);
    pg.laid = s;
  }
}

// ------------------------------------------------------------------ lines and paragraphs
// The text layer is runs of text placed where the page draws them. Runs on one baseline make a line
// (split where a gap is wide enough to be between columns); lines close under each other, overlapping
// and of about the same size make a paragraph. Each gets an invisible box on the page to be marked.
function lay(pg) {
  for (const el of pg.div.querySelectorAll('.para')) el.remove();
  const b = pg.div.getBoundingClientRect();
  const runs = [];
  for (const sp of pg.text.querySelectorAll('span:not(.markedContent)')) {
    const t = sp.textContent;
    if (!t || !t.trim() || sp.getAttribute('role') === 'img') continue;
    const r = sp.getBoundingClientRect();
    if (r.width < 0.5 || r.height < 0.5) continue;
    runs.push({ t, x: r.left - b.left, y: r.top - b.top, w: r.width, h: r.height });
  }
  pg.runs = runs;
  const bands = [];
  for (const r of [...runs].sort((a, b) => (a.y + a.h / 2) - (b.y + b.h / 2))) {
    const band = bands[bands.length - 1];
    const mid = r.y + r.h / 2;
    if (band && Math.abs(band.mid - mid) < Math.min(band.h, r.h) * 0.5) band.runs.push(r);
    else bands.push({ mid, h: r.h, runs: [r] });
  }
  const lines = [];
  for (const band of bands) {
    let line = null;
    for (const r of band.runs.sort((a, b) => a.x - b.x)) {
      const gap = line ? r.x - (line.x + line.w) : 0;
      if (!line || gap > Math.max(line.h, r.h)) {
        line = { x: r.x, y: r.y, w: r.w, h: r.h, t: r.t, runs: [r] };
        lines.push(line);
        continue;
      }
      if (gap > 0.15 * r.h && !/\s$/.test(line.t) && !/^\s/.test(r.t)) line.t += ' ';
      line.t += r.t;
      const x1 = Math.max(line.x + line.w, r.x + r.w), y1 = Math.max(line.y + line.h, r.y + r.h);
      line.y = Math.min(line.y, r.y);
      line.w = x1 - line.x; line.h = y1 - line.y;
      line.runs.push(r);
    }
  }
  for (const l of lines) { l.t = l.t.replace(/\s+/g, ' ').trim(); for (const r of l.runs) r.line = l; }
  const paras = [];
  for (const l of lines.sort((a, b) => a.y - b.y || a.x - b.x)) {
    const p = paras.find(p => {
      const last = p.lines[p.lines.length - 1];
      const gap = l.y - (last.y + last.h);
      const h = Math.min(l.h, last.h);
      return gap > -0.3 * h && gap < 0.9 * h && l.x < last.x + last.w && l.x + l.w > last.x && Math.max(l.h, last.h) / h < 1.35;
    });
    if (p) p.lines.push(l); else paras.push({ lines: [l] });
  }
  for (const p of paras) {
    p.x = Math.min(...p.lines.map(l => l.x)); p.y = Math.min(...p.lines.map(l => l.y));
    p.w = Math.max(...p.lines.map(l => l.x + l.w)) - p.x; p.h = Math.max(...p.lines.map(l => l.y + l.h)) - p.y;
    p.el = box('para', p.x, p.y, p.w, p.h);
    what.set(p.el, { kind: 'para', pg, para: p });
    for (const l of p.lines) {
      l.el = box('ln', l.x - p.x, l.y - p.y, l.w, l.h);
      what.set(l.el, { kind: 'line', pg, para: p, line: l });
      p.el.appendChild(l.el);
    }
    pg.div.appendChild(p.el);
  }
  pg.paras = paras;
}
function box(cls, x, y, w, h) {
  const el = document.createElement('div');
  el.className = cls;
  el.style.cssText = `left:${x}px;top:${y}px;width:${w}px;height:${h}px`;
  return el;
}

// ------------------------------------------------------------------ zoom, and where one is
function anchor() {
  const pg = pages.find(p => p.div.getBoundingClientRect().bottom > 40) || pages[0];
  if (!pg) return null;
  const r = pg.div.getBoundingClientRect();
  return { pg, f: (40 - r.top) / r.height };
}
function restore(at) {
  if (!at) return;
  const r = at.pg.div.getBoundingClientRect();
  scrollBy(0, r.top + at.f * r.height - 40);
}
function setZoom(z) {
  const at = anchor();
  zoom = Math.min(5, Math.max(0.25, z));
  size();
  restore(at);
}
function where() {
  if (!pages.length) return;
  const mid = innerHeight * 0.4;
  const pg = pages.find(p => p.div.getBoundingClientRect().bottom > mid) || pages[pages.length - 1];
  atEl.textContent = `${pg.n} / ${pages.length}`;
}
$('#out').addEventListener('click', () => setZoom(zoom / 1.2));
$('#in').addEventListener('click', () => setZoom(zoom * 1.2));
zoomBtn.addEventListener('click', () => setZoom(1));
addEventListener('wheel', e => {
  if (!e.ctrlKey) return;
  e.preventDefault();
  setZoom(zoom * (e.deltaY < 0 ? 1.1 : 1 / 1.1));
}, { passive: false });
addEventListener('keydown', e => {
  if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
  if (e.key === '+' || e.key === '=') { e.preventDefault(); setZoom(zoom * 1.2); }
  else if (e.key === '-') { e.preventDefault(); setZoom(zoom / 1.2); }
  else if (e.key === '0') { e.preventDefault(); setZoom(1); }
});

// ------------------------------------------------------------------ marking
function mark() {
  if (!pages.length) return;
  if (globalThis.__weftMark) { globalThis.__weftMark.again(); return; }
  const s = document.createElement('script');
  s.src = 'mark.js';
  document.head.appendChild(s);
}
$('#mark').addEventListener('click', mark);
// the weft button (or Alt+Shift+W) on this tab: background.js says so
chrome.tabs.getCurrent().then(me => {
  chrome.runtime.onMessage.addListener(m => { if (m && m.t === 'mark' && me && m.tab === me.id) mark(); });
});
// opened by the weft button: the picker starts as soon as the first pages can be marked
let markWhenReady = location.hash === '#mark';
if (markWhenReady) history.replaceState(null, '', location.pathname + location.search);
function ready() {
  if (!markWhenReady || ![...seen].some(pg => pg.laid)) return;
  markWhenReady = false;
  mark();
}

const label = (k, n) => ({ line: `page ${n} · line`, para: `page ${n} · paragraph`, page: `page ${n}` })[k];
// the lines of what was marked, each with only the runs in the area when an area was drawn
function linesIn(r) {
  const out = [];
  for (const pg of pages) {
    const b = pg.div.getBoundingClientRect();
    if (b.bottom < r.top || b.top > r.bottom) continue;
    const got = new Map();
    for (const run of pg.runs) {
      const cx = b.left + run.x + run.w / 2, cy = b.top + run.y + run.h / 2;
      if (cx < r.left || cx > r.right || cy < r.top || cy > r.bottom) continue;
      if (!got.has(run.line)) got.set(run.line, []);
      got.get(run.line).push(run);
    }
    for (const [line, runs] of got) {
      const t = runs.length === line.runs.length ? line.t : runs.map(x => x.t).join(' ').replace(/\s+/g, ' ').trim();
      out.push({ pg, y: line.y, x: line.x, t });
    }
  }
  return out.sort((a, b) => a.pg.n - b.pg.n || a.y - b.y || a.x - b.x);
}

globalThis.__weftDoc = {
  key: src.replace(/[^/]*$/, ''),
  hint: 'click a line · drag for an area · <b>↑ ↓</b> line / paragraph / page · <b>esc</b> stop',
  page: () => ({ url: src, title, w: innerWidth, h: innerHeight, dpr: devicePixelRatio, pdf: { file, pages: pages.length } }),
  // what under the pointer can be marked: the line (the nearest one of the paragraph it is in), else the page
  pick(t, x, y) {
    const div = t && t.closest ? t.closest('.pg') : null;
    const pg = div && byDiv.get(div);
    if (!pg) return null;
    const b = div.getBoundingClientRect(), px = x - b.left, py = y - b.top;
    const p = pg.paras.find(p => px >= p.x - 4 && px <= p.x + p.w + 4 && py >= p.y - 4 && py <= p.y + p.h + 4);
    if (!p) return div;
    let best = p.lines[0], d = Infinity;
    for (const l of p.lines) {
      const dd = py < l.y ? l.y - py : py > l.y + l.h ? py - l.y - l.h : 0;
      if (dd < d) { d = dd; best = l; }
    }
    return best.el;
  },
  // ↑: a line's paragraph (when it has more than that line), then its page
  up(el) {
    const w = what.get(el);
    if (!w) return null;
    if (w.kind === 'line' && w.para.lines.length > 1) return w.para.el;
    return w.pg.div;
  },
  label(el) {
    const w = what.get(el), pg = w ? w.pg : byDiv.get(el);
    return pg ? label(w ? w.kind : 'page', pg.n) : '';
  },
  describe(el, r, area) {
    const w = what.get(el);
    let lines;
    if (area) lines = linesIn(r);
    else if (w && w.kind === 'line') lines = [{ pg: w.pg, t: w.line.t }];
    else if (w) lines = w.para.lines.map(l => ({ pg: w.pg, t: l.t }));
    else lines = (byDiv.get(el) ? byDiv.get(el).paras.flatMap(p => p.lines) : []).map(l => ({ pg: byDiv.get(el), t: l.t }));
    // the page under the middle of the mark says where it is
    const cy = r.top + r.height / 2;
    const pg = (w && w.pg) || byDiv.get(el) || pages.find(p => p.div.getBoundingClientRect().bottom >= cy) || pages[pages.length - 1];
    const b = pg.div.getBoundingClientRect(), s = scale();
    let text = lines.map(l => l.t).join('\n');
    if (text.length > 3000) text = text.slice(0, 3000) + '…';
    const last = lines.length ? Math.max(pg.n, ...lines.map(l => l.pg.n)) : pg.n;
    return {
      label: area ? (last > pg.n ? `pages ${pg.n}–${last} · area` : `page ${pg.n} · area`) : label(w ? w.kind : 'page', pg.n),
      selector: '', text, html: '', styles: '', attrs: {}, area, comp: null, hints: {},
      rect: { x: Math.round(r.left), y: Math.round(r.top), w: Math.round(r.width), h: Math.round(r.height) },
      pdf: {
        page: pg.n, last, pw: Math.round(pg.w), ph: Math.round(pg.h),
        x: Math.round((r.left - b.left) / s), y: Math.round((r.top - b.top) / s), w: Math.round(r.width / s), h: Math.round(r.height / s),
      },
    };
  },
  // the view as the pages are drawn (the browser gives no screenshot of an extension's own page)
  async capture() {
    const dpr = devicePixelRatio || 1;
    const c = document.createElement('canvas');
    c.width = Math.round(innerWidth * dpr); c.height = Math.round(innerHeight * dpr);
    const g = c.getContext('2d');
    g.fillStyle = BG; g.fillRect(0, 0, c.width, c.height);
    for (const pg of pages) {
      const r = pg.div.getBoundingClientRect();
      if (r.bottom < 0 || r.top > innerHeight) continue;
      g.fillStyle = '#fff'; g.fillRect(r.left * dpr, r.top * dpr, r.width * dpr, r.height * dpr);
      if (pg.canvas.width) g.drawImage(pg.canvas, r.left * dpr, r.top * dpr, r.width * dpr, r.height * dpr);
    }
    return { url: c.toDataURL('image/png') };
  },
};

load();
