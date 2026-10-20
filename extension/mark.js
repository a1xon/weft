// weft marks: the picker and the note box, drawn over the page in a closed shadow root.
// Click marks an element, a drag marks an area, ↑/↓ make the mark bigger/smaller, esc stops.
// Text selected on the page is a mark of its own (select it, then the weft button), highlighted
// by the browser. Each mark keeps a screenshot (the mark outlined, and the whole view), what the
// element is (selector, text, html, box, styles) and, from probe.js, its component and where it is
// written. Send hands it all to weft (background.js -> native messaging -> `weft`): a new agent card
// on the chosen board, or a message to one of its agents. In weft's viewers (viewer.js for PDFs,
// md.js for Markdown) the viewer says what can be marked (lines, paragraphs, pages, blocks), what a
// mark is and what it looks like (`doc`); there a drag over text selects it, and selected text is
// marked at once.
(() => {
  if (globalThis.__weftMark) { globalThis.__weftMark.again(); return; }

  const ACCENT = '#5cc8ff';
  const ask = msg => new Promise(res => {
    try { chrome.runtime.sendMessage(msg, r => res(r || { error: chrome.runtime.lastError && chrome.runtime.lastError.message })); }
    catch (e) { res({ error: String(e && e.message || e) }); }
  });
  const weft = msg => ask({ t: 'weft', msg });
  const frames = n => new Promise(r => { const f = () => (--n <= 0 ? r() : requestAnimationFrame(f)); requestAnimationFrame(f); });
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const store = {
    get: k => new Promise(r => { try { chrome.storage.local.get(k, v => r(v && v[k])); } catch { r(undefined); } }),
    set: (k, v) => { try { chrome.storage.local.set({ [k]: v }); } catch {} },
  };
  const esc = s => String(s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);
  const short = p => String(p || '').split('/').slice(-2).join('/');
  // a page that is no web page: one of weft's viewers
  const doc = globalThis.__weftDoc || null;
  // the browser draws the highlight of marked text (CSS custom highlights)
  const HL = typeof Highlight === 'function' && globalThis.CSS && CSS.highlights ? CSS.highlights : null;

  // ------------------------------------------------------------------ the layer over the page
  const host = document.createElement('weft-mark');
  host.style.cssText = 'all:initial;position:fixed;inset:0;z-index:2147483647;pointer-events:none;display:block';
  const root = host.attachShadow({ mode: 'closed' });
  const chamfer = c => `polygon(0 0,calc(100% - ${c}) 0,100% ${c},100% 100%,${c} 100%,0 calc(100% - ${c}))`;
  root.innerHTML = `<style>
    :host { all: initial; }
    * { box-sizing: border-box; }
    .ui { font: 12px/1.5 weft-mono, "JetBrains Mono", ui-monospace, "SF Mono", Menlo, Consolas, monospace; color: #c6d2e4;
      -webkit-font-smoothing: antialiased; font-variant-ligatures: none; }
    .hov { position: fixed; pointer-events: none; outline: 2px solid ${ACCENT}; background: rgba(92,200,255,.10); border-radius: 2px; display: none; }
    .tag { position: fixed; pointer-events: none; display: none; max-width: 70vw; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
      background: #04060a; color: ${ACCENT}; border: 1px solid #2a3a50; padding: 2px 8px; border-radius: 2px; }
    .tag i { color: #8496b0; font-style: normal; }
    .drag { position: fixed; pointer-events: none; display: none; border: 2px dashed ${ACCENT}; background: rgba(92,200,255,.10); }
    .m { position: fixed; pointer-events: none; outline: 2px solid ${ACCENT}; box-shadow: 0 0 0 1px #000, 0 0 14px rgba(92,200,255,.35); border-radius: 2px; }
    .m.on { outline-width: 3px; box-shadow: 0 0 0 1px #000, 0 0 24px rgba(92,200,255,.75); }
    .m.t { outline: none; box-shadow: none; }
    .m b { position: absolute; left: -11px; top: -11px; width: 20px; height: 20px; border-radius: 10px; background: ${ACCENT}; color: #04121c;
      font-weight: 700; text-align: center; line-height: 20px; box-shadow: 0 0 0 2px #04121c; }
    .m.t b { left: -25px; top: calc(50% - 10px); }
    .m.on b { box-shadow: 0 0 0 2px #04121c, 0 0 12px ${ACCENT}; }
    .hint { position: fixed; top: 10px; left: 50%; transform: translateX(-50%); pointer-events: none; display: none; max-width: calc(100vw - 24px);
      background: #04060af0; border: 1px solid #2a3a50; border-radius: 2px; padding: 5px 12px; color: #8496b0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
    .hint b { color: ${ACCENT}; font-weight: 600; }
    .box { position: fixed; width: 420px; max-width: calc(100vw - 16px); pointer-events: auto; display: none; user-select: none;
      filter: drop-shadow(0 10px 22px rgba(0,0,0,.38)) drop-shadow(0 0 1px rgba(0,0,0,.7)); }
    .in { position: relative; background: #2a3a50; clip-path: ${chamfer('12px')}; }
    .in::before { content: ""; position: absolute; inset: 1px; background: #0a0e15; clip-path: ${chamfer('11.6px')}; }
    .in > * { position: relative; }
    .hd { display: flex; align-items: center; gap: 10px; padding: 8px 14px 8px 12px; border-bottom: 1px solid #1b2634; cursor: move; user-select: none; }
    .logo { font: 600 13px/1 weft-display, "Chakra Petch", ui-sans-serif, system-ui, sans-serif; color: ${ACCENT}; letter-spacing: .06em; }
    .cnt { flex: 1; min-width: 0; color: #566782; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
    select, textarea, button { font: inherit; color: inherit; }
    button { background: #0e141d; border: 1px solid #2a3a50; border-radius: 2px; padding: 4px 10px; cursor: pointer; color: #c6d2e4; }
    button:hover { border-color: ${ACCENT}; color: #eaf2ff; }
    button:focus-visible, select:focus-visible { outline: 1px solid ${ACCENT}; outline-offset: 1px; }
    .ib { border: none; background: none; color: #8496b0; padding: 2px 5px; line-height: 1; }
    .ib:hover { color: #eaf2ff; }
    .list { max-height: 46vh; overflow: auto; padding: 0 12px; scrollbar-width: thin; scrollbar-color: #2a3a50 transparent; }
    .mk { padding: 10px 0; }
    .mk + .mk { border-top: 1px solid #111823; }
    .lab { display: flex; align-items: center; gap: 7px; color: #8496b0; min-width: 0; cursor: pointer; }
    .lab .n { flex: none; width: 18px; height: 18px; border-radius: 9px; background: ${ACCENT}; color: #04121c; font-weight: 700; text-align: center; line-height: 18px; }
    .lab code { font: inherit; color: #eaf2ff; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
    .lab:hover code { color: ${ACCENT}; }
    .lab .c { color: #7aa7e0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; flex: 1; min-width: 0; }
    .lab .rm { margin-left: auto; flex: none; color: #566782; }
    .q { margin: 7px 0 0 25px; padding: 1px 0 1px 10px; border-left: 2px solid ${ACCENT}; color: #dbe6f5; white-space: pre-wrap; overflow-wrap: anywhere;
      font: 12.5px/1.5 ui-sans-serif, system-ui, sans-serif; display: -webkit-box; -webkit-line-clamp: 4; -webkit-box-orient: vertical; overflow: hidden; }
    .q.dim { border-left-color: #2a3a50; color: #8496b0; -webkit-line-clamp: 2; }
    .th { display: block; margin: 7px 0 0 25px; max-width: calc(100% - 25px); max-height: 76px; border: 1px solid #1b2634; border-radius: 2px; }
    textarea, .q { user-select: text; }
    textarea { display: block; width: calc(100% - 25px); margin: 7px 0 0 25px; min-height: 54px; resize: vertical; background: #04060a; border: 1px solid #2a3a50;
      border-radius: 2px; padding: 6px 8px; color: #eaf2ff; outline: none; line-height: 1.45; }
    textarea::placeholder { color: #566782; }
    textarea:focus { border-color: ${ACCENT}; }
    .add { display: flex; flex-wrap: wrap; align-items: center; gap: 4px 10px; padding: 4px 12px 10px; }
    .more { border-style: dashed; flex: none; white-space: nowrap; }
    .tip { color: #566782; }
    .ctx { color: #8496b0; margin-left: auto; }
    .ctx.bad { color: #ffb454; }
    .dest { display: grid; grid-template-columns: auto minmax(0, 1fr); align-items: center; gap: 6px 10px; padding: 9px 12px; border-top: 1px solid #1b2634; }
    .dest label { color: #566782; text-align: right; }
    select { width: 100%; background: #0e141d; border: 1px solid #2a3a50; border-radius: 2px; padding: 4px 6px; color: #c6d2e4; }
    select:hover { border-color: #3d5370; }
    .ft { display: flex; flex-wrap: wrap; justify-content: flex-end; align-items: center; gap: 8px; padding: 9px 14px 11px 12px; border-top: 1px solid #1b2634; }
    .st { flex: 1; min-width: 0; color: #8496b0; }
    .st.err, .st.ok { flex-basis: 100%; }
    .st.err { color: #ff5c7a; }
    .st.ok { color: #3ddc97; }
    .st code { color: #eaf2ff; background: #04060a; padding: 0 4px; border-radius: 2px; }
    .st button { margin-left: 6px; padding: 2px 8px; }
    .go { background: ${ACCENT}; color: #04121c; border-color: ${ACCENT}; font-weight: 700; }
    .go:hover { background: #8fdbff; color: #04121c; }
    .go:disabled, .draft:disabled { opacity: .4; cursor: default; }
    .folded .body { display: none; }
    .folded .hd { border-bottom: none; }
    kbd { font: inherit; opacity: .7; font-size: 10px; margin-left: 3px; }
    .shake { animation: sh .3s; }
    @keyframes sh { 25% { transform: translateX(-4px); } 75% { transform: translateX(4px); } }
  </style>
  <div class="ui">
    <div class="hov"></div><div class="tag"></div><div class="drag"></div>
    <div class="marks"></div>
    <div class="hint">${doc ? doc.hint : 'click to mark · drag for an area · <b>↑ ↓</b> bigger / smaller · <b>esc</b> stop'}</div>
    <div class="box"><div class="in">
      <div class="hd"><span class="logo">◇ weft</span><span class="cnt"></span><button class="ib fold" title="fold the box">▾</button><button class="ib x" title="close (esc)">✕</button></div>
      <div class="body">
        <div class="list"></div>
        <div class="add"><button class="more" title="Alt+Shift+W">+ mark another</button><span class="tip">${doc && doc.select ? 'or select text' : 'or select text first'}</span><span class="ctx"></span></div>
        <div class="dest"><label>to</label><select class="to"></select><label>on</label><select class="board" title="the board the card goes to"></select></div>
      </div>
      <div class="ft"><span class="st"></span><button class="draft" title="put the card on the canvas without starting it">card only</button><button class="go">send <kbd>ctrl ⏎</kbd></button></div>
    </div></div>
  </div>`;
  const $ = s => root.querySelector(s);
  const hov = $('.hov'), tag = $('.tag'), drag = $('.drag'), marksEl = $('.marks'), hint = $('.hint'), box = $('.box');
  // (a viewer has its bar on top)
  if (doc) hint.style.top = '50px';
  const boardSel = $('.board'), toSel = $('.to'), list = $('.list'), st = $('.st'), ctx = $('.ctx'), cnt = $('.cnt');
  const goBtn = $('.go'), draftBtn = $('.draft'), foldBtn = $('.fold');

  // ------------------------------------------------------------------ state
  const marks = [];          // { el, range (selected text) | null, area (page px) | null, info, shots, note }
  let picking = false, hover = null, smaller = [], down = null, dragging = false, selDown = false, eatUntil = 0;
  let boards = null, sending = false, placed = false, discardAt = 0, raf = 0, lit = -1;
  const origin = doc ? doc.key : location.origin;

  // weft's fonts for the box: added to the page under names of their own (its styles do not reach
  // into the box; its own fonts stay as they are). A viewer has them already.
  async function fonts() {
    try {
      if ([...document.fonts].some(f => f.family.replace(/["']/g, '') === 'weft-mono')) return;
      const r = await ask({ t: 'fonts' });
      for (const f of (r && r.fonts) || []) document.fonts.add(new FontFace(f.family, Uint8Array.from(atob(f.data), c => c.charCodeAt(0)), { weight: f.weight }));
    } catch {}
  }

  // ------------------------------------------------------------------ what an element is
  const probe = el => {
    let got = null;
    const h = e => { got = e.detail; };
    document.addEventListener('weft:probed', h, { once: true });
    try { el.dispatchEvent(new CustomEvent('weft:probe')); } catch {}
    document.removeEventListener('weft:probed', h);
    try { return got ? JSON.parse(got) : {}; } catch { return {}; }
  };
  const pageLog = () => {
    let got = null;
    const h = e => { got = e.detail; };
    document.addEventListener('weft:logged', h, { once: true });
    document.dispatchEvent(new CustomEvent('weft:log'));
    document.removeEventListener('weft:logged', h);
    try { return got ? JSON.parse(got) : { log: [], failed: [] }; } catch { return { log: [], failed: [] }; }
  };
  // class names a build tool made up say nothing about the code
  const madeUp = c => /^(css|sc|jsx|emotion|svelte|astro|tw|chakra|mui|ant)-[\w-]{4,}$/i.test(c) || /^_?[a-z0-9]{5,}$/i.test(c) && /\d/.test(c) && !/-/.test(c);
  const classes = el => [...(el.classList || [])].filter(c => !madeUp(c));
  const label = el => {
    if (!el || !el.localName) return '';
    const id = el.id && !/\d{4,}/.test(el.id) ? '#' + el.id : '';
    return el.localName + id + classes(el).slice(0, 2).map(c => '.' + c).join('');
  };
  function selector(el) {
    const one = s => { try { return document.querySelectorAll(s).length === 1; } catch { return false; } };
    const e = CSS.escape;
    if (el.id && one('#' + e(el.id))) return '#' + e(el.id);
    const parts = [];
    for (let n = el; n && n.nodeType === 1; n = n.parentElement) {
      if (n === document.documentElement) { parts.unshift('html'); break; }
      if (n !== el && n.id && one('#' + e(n.id))) {
        parts.unshift('#' + e(n.id));
        return parts.join(' > ');
      }
      let s = n.localName + classes(n).slice(0, 2).map(c => '.' + e(c)).join('');
      const p = n.parentElement;
      if (p) {
        const same = [...p.children].filter(c => c.localName === n.localName);
        if (same.length > 1) s += `:nth-of-type(${same.indexOf(n) + 1})`;
      }
      parts.unshift(s);
      if (one(parts.join(' > '))) return parts.join(' > ');
    }
    return parts.join(' > ');
  }
  const squash = s => String(s || '').replace(/\s+/g, ' ').trim();
  function textOf(el) {
    if (/^(input|textarea|select)$/.test(el.localName)) return squash(el.value || el.placeholder || '');
    const t = squash(el.innerText || el.textContent || '');
    return t.length > 200 ? t.slice(0, 200) + '…' : t;
  }
  function html(el) {
    const big = el.getElementsByTagName('*').length > 1500;
    const c = el.cloneNode(!big);
    const all = [c, ...(c.querySelectorAll ? c.querySelectorAll('*') : [])];
    for (const x of all) {
      if (/^(script|style|noscript|template|weft-mark)$/.test(x.localName) && x !== c) { x.remove(); continue; }
      if (x.localName === 'svg') x.innerHTML = '…';
      for (const a of [...(x.attributes || [])]) {
        if (a.value.length > 140) x.setAttribute(a.name, a.value.slice(0, 120) + '…');
      }
    }
    let h = squash(c.outerHTML);
    if (big) h = h.replace(new RegExp(`</${el.localName}>$`), `…</${el.localName}>`);
    return h.length > 900 ? h.slice(0, 900) + ' …' : h;
  }
  function bgOf(el) {
    for (let n = el, up = 0; n && n.nodeType === 1; n = n.parentElement, up++) {
      const b = getComputedStyle(n).backgroundColor;
      if (b && b !== 'transparent' && !/rgba\(.*,\s*0\)$/.test(b)) return up ? `${b} (from <${label(n)}>)` : b;
    }
    return '';
  }
  function styles(el) {
    const s = getComputedStyle(el), out = [];
    const side = p => {
      const v = ['Top', 'Right', 'Bottom', 'Left'].map(k => s[p + k]);
      if (v.every(x => x === '0px')) return '';
      return v[0] === v[2] && v[1] === v[3] ? (v[0] === v[1] ? v[0] : `${v[0]} ${v[1]}`) : v.join(' ');
    };
    out.push('display:' + s.display);
    if (s.position !== 'static') out.push('position:' + s.position);
    const m = side('margin'), p = side('padding');
    if (m) out.push('margin:' + m);
    if (p) out.push('padding:' + p);
    if (/flex/.test(s.display)) out.push(`flex:${s.flexDirection} justify:${s.justifyContent} align:${s.alignItems}`);
    if (/grid/.test(s.display)) out.push('grid-template-columns:' + s.gridTemplateColumns.slice(0, 80));
    if (/flex|grid/.test(s.display) && s.gap !== 'normal' && s.gap !== '0px') out.push('gap:' + s.gap);
    out.push(`font:${s.fontSize}/${s.lineHeight} ${s.fontFamily.split(',')[0].replace(/["']/g, '')} ${s.fontWeight}`);
    out.push('color:' + s.color);
    const bg = bgOf(el);
    if (bg) out.push('background:' + bg);
    if (s.borderTopWidth !== '0px' && s.borderTopStyle !== 'none') out.push(`border:${s.borderTopWidth} ${s.borderTopStyle} ${s.borderTopColor}`);
    if (s.borderRadius !== '0px') out.push('radius:' + s.borderRadius);
    if (s.textAlign !== 'start') out.push('text-align:' + s.textAlign);
    if (s.overflow !== 'visible') out.push('overflow:' + s.overflow);
    if (s.zIndex !== 'auto') out.push('z-index:' + s.zIndex);
    if (s.opacity !== '1') out.push('opacity:' + s.opacity);
    if (s.visibility !== 'visible') out.push('visibility:' + s.visibility);
    if (s.transform !== 'none') out.push('transform:' + s.transform.slice(0, 60));
    const pa = el.parentElement && getComputedStyle(el.parentElement);
    if (pa && /flex|grid/.test(pa.display)) {
      out.push(`parent:${pa.display}${/flex/.test(pa.display) ? ' ' + pa.flexDirection : ''}${pa.gap !== 'normal' && pa.gap !== '0px' ? ' gap ' + pa.gap : ''}`);
    }
    return out.join('; ');
  }
  function describe(el, area, r) {
    const a = n => el.getAttribute && el.getAttribute(n);
    const extra = {};
    for (const k of ['role', 'aria-label', 'alt', 'title', 'href', 'src', 'type', 'name', 'placeholder', 'for']) {
      const v = a(k);
      if (v) extra[k] = v.length > 160 ? v.slice(0, 160) + '…' : v;
    }
    const p = probe(el);
    return {
      label: label(el), selector: selector(el), text: textOf(el), html: html(el), styles: styles(el), attrs: extra,
      rect: { x: Math.round(r.left), y: Math.round(r.top), w: Math.round(r.width), h: Math.round(r.height) },
      area: !!area, comp: p.comp || null, hints: p.attrs || {},
    };
  }
  const compLabel = c => {
    if (!c) return '';
    const top = c.chain && c.chain[0];
    const where = c.at || (top && top.src) || '';
    return (top ? `<${top.name}>` : c.fw) + (where ? ' ' + short(where) : '');
  };

  // ------------------------------------------------------------------ selected text
  // the selection on the page, when there is one to mark (in a viewer: in what it shows)
  function selected() {
    const s = getSelection();
    if (!s || !s.rangeCount || s.isCollapsed) return null;
    const quote = s.toString().replace(/\u00a0/g, ' ').trim();
    const range = s.getRangeAt(0), c = range.commonAncestorContainer;
    const el = c.nodeType === 1 ? c : c.parentElement;
    if (!quote || !el || el === host || (doc && !doc.root().contains(el))) return null;
    return { range: range.cloneRange(), quote: quote.length > 3000 ? quote.slice(0, 3000) + '…' : quote, el };
  }
  // the boxes of the lines it covers (of its text only: an element it holds whole has a box of its own)
  function rectsOf(range) {
    const out = [], c = range.commonAncestorContainer;
    const walk = document.createTreeWalker(c.nodeType === 3 ? c.parentNode : c, NodeFilter.SHOW_TEXT);
    const sub = document.createRange();
    for (let t = walk.nextNode(); t && out.length < 400; t = walk.nextNode()) {
      if (!range.intersectsNode(t)) continue;
      sub.selectNodeContents(t);
      if (t === range.startContainer) sub.setStart(t, range.startOffset);
      if (t === range.endContainer) sub.setEnd(t, range.endOffset);
      for (const r of sub.getClientRects()) if (r.width >= 1 && r.height >= 1) out.push(r);
    }
    return out;
  }
  const firstRect = range => { for (const r of range.getClientRects()) if (r.width >= 1 && r.height >= 1) return r; return null; };
  // what is painted into the page: the highlights of marked text (the one the pointer is on in the box
  // brighter) and, while picking, the cursor
  function paint() {
    if (HL) {
      const ranges = marks.filter((m, i) => m.range && i !== lit).map(m => m.range);
      if (ranges.length) HL.set('weft-mark', new Highlight(...ranges)); else HL.delete('weft-mark');
      const on = marks[lit] && marks[lit].range;
      if (on) HL.set('weft-mark-on', new Highlight(on)); else HL.delete('weft-mark-on');
    }
    const want = (HL && marks.some(m => m.range) ? '::highlight(weft-mark){background-color:rgba(92,200,255,.32)}::highlight(weft-mark-on){background-color:rgba(92,200,255,.62)}' : '')
      + (picking ? '*{cursor:crosshair!important}' + ((doc && doc.css) || '') : '');
    let s = document.getElementById('weft-mark-style');
    if (!want) { if (s) s.remove(); return; }
    if (!s) {
      s = document.createElement('style');
      s.id = 'weft-mark-style';
      (document.head || document.documentElement).appendChild(s);
    }
    if (s.textContent !== want) s.textContent = want;
  }

  // ------------------------------------------------------------------ drawing
  const place = (el, r) => { el.style.left = r.left + 'px'; el.style.top = r.top + 'px'; el.style.width = r.width + 'px'; el.style.height = r.height + 'px'; };
  const rectOf = m => (m.area ? new DOMRect(m.area.x - scrollX, m.area.y - scrollY, m.area.w, m.area.h)
    : m.range ? m.range.getBoundingClientRect() : m.el.getBoundingClientRect());
  function draw() {
    raf = 0;
    if (!host.isConnected) return;
    if (picking && hover && !dragging) {
      const r = hover.getBoundingClientRect();
      place(hov, r); hov.style.display = 'block';
      tag.style.display = 'block';
      tag.style.left = Math.max(4, Math.min(r.left, innerWidth - 300)) + 'px';
      tag.style.top = (r.top > 26 ? r.top - 24 : Math.min(r.bottom + 4, innerHeight - 24)) + 'px';
    } else { hov.style.display = 'none'; tag.style.display = 'none'; }
    const kids = marksEl.children;
    while (kids.length < marks.length) marksEl.appendChild(document.createElement('div')).className = 'm';
    while (kids.length > marks.length) kids[kids.length - 1].remove();
    marks.forEach((m, i) => {
      const k = kids[i];
      // (marked text: the browser draws its highlight, the number goes before its first line)
      const text = !!(m.range && HL);
      const r = text ? firstRect(m.range) : rectOf(m);
      // (not on the page now: the other view of a viewer)
      if (!r || (!r.width && !r.height)) { k.style.display = 'none'; return; }
      k.style.display = '';
      place(k, r);
      k.classList.toggle('t', text);
      k.classList.toggle('on', i === lit);
      if (k.dataset.n !== String(i + 1)) { k.dataset.n = String(i + 1); k.innerHTML = `<b>${i + 1}</b>`; }
      // (in the margin left of the text it is in, level with its first line: not over the words before it)
      k.firstChild.style.left = text ? `${Math.min(0, (doc ? doc.edge() : m.el.getBoundingClientRect().left) - r.left) - 25}px` : '';
    });
    if (marks.length || picking) raf = requestAnimationFrame(draw);
  }
  const redraw = () => { if (!raf) raf = requestAnimationFrame(draw); };
  function setHover(el) {
    if (el === hover) return;
    hover = el;
    if (!el) { redraw(); return; }
    const r = el.getBoundingClientRect();
    const c = doc ? '' : compLabel(probe(el).comp);
    tag.innerHTML = `${esc(doc ? doc.label(el) : label(el))} <i>${Math.round(r.width)}×${Math.round(r.height)}</i>${c ? ' ' + esc(c) : ''}`;
    redraw();
  }

  // ------------------------------------------------------------------ screenshots
  const load = url => new Promise((res, rej) => { const i = new Image(); i.onload = () => res(i); i.onerror = rej; i.src = url; });
  function badge(g, x, y, n, s) {
    const rad = 11 * s, cx = Math.max(rad + 2, x), cy = Math.max(rad + 2, y);
    g.beginPath(); g.arc(cx, cy, rad, 0, Math.PI * 2); g.fillStyle = ACCENT; g.fill();
    g.lineWidth = 2 * s; g.strokeStyle = '#04121c'; g.stroke();
    g.fillStyle = '#04121c'; g.font = `700 ${13 * s}px ui-monospace, monospace`; g.textAlign = 'center'; g.textBaseline = 'middle';
    g.fillText(String(n), cx, cy + s);
  }
  function outline(g, x, y, w, h, n, s) {
    g.lineJoin = 'round';
    g.strokeStyle = 'rgba(0,0,0,.85)'; g.lineWidth = 5 * s; g.strokeRect(x, y, w, h);
    g.strokeStyle = ACCENT; g.lineWidth = 2.5 * s; g.strokeRect(x, y, w, h);
    badge(g, x, y, n, s);
  }
  // marked text: its lines highlighted, the number before the first
  function fill(g, rects, ox, oy, k, n, s) {
    g.fillStyle = 'rgba(92,200,255,.38)';
    for (const q of rects) g.fillRect((q.left - ox) * k, (q.top - oy) * k, q.width * k, q.height * k);
    const f = rects[0];
    if (f) badge(g, (f.left - ox) * k - 14 * s, (f.top + f.height / 2 - oy) * k, n, s);
  }
  async function cut(url, r, n, rects) {
    const img = await load(url);
    const s = img.naturalWidth / innerWidth;
    const mark = (g, ox, oy, k, w) => {
      if (rects && rects.length) fill(g, rects, ox, oy, k, n, w);
      else outline(g, (r.left - ox) * k, (r.top - oy) * k, r.width * k, r.height * k, n, w);
    };
    // the whole view, the mark outlined, at most 1600 px wide
    const vs = Math.min(1, 1600 / img.naturalWidth);
    const v = document.createElement('canvas');
    v.width = Math.round(img.naturalWidth * vs); v.height = Math.round(img.naturalHeight * vs);
    const vg = v.getContext('2d');
    vg.drawImage(img, 0, 0, v.width, v.height);
    mark(vg, 0, 0, s * vs, s * vs);
    // the mark itself with some room around it, unless it fills most of the view anyway
    let crop = null;
    const pad = 28;
    const x0 = Math.max(0, r.left - pad), y0 = Math.max(0, r.top - pad);
    const x1 = Math.min(innerWidth, r.right + pad), y1 = Math.min(innerHeight, r.bottom + pad);
    if (x1 > x0 && y1 > y0 && (x1 - x0) * (y1 - y0) < 0.6 * innerWidth * innerHeight) {
      const cs = Math.min(1, 1600 / ((x1 - x0) * s), 1600 / ((y1 - y0) * s));
      const c = document.createElement('canvas');
      c.width = Math.round((x1 - x0) * s * cs); c.height = Math.round((y1 - y0) * s * cs);
      const cg = c.getContext('2d');
      cg.drawImage(img, x0 * s, y0 * s, (x1 - x0) * s, (y1 - y0) * s, 0, 0, c.width, c.height);
      const k = s * cs;
      mark(cg, x0, y0, k, Math.max(1, k));
      crop = c.toDataURL('image/png');
    }
    return { view: v.toDataURL('image/png'), crop };
  }
  async function capture() {
    if (doc) return doc.capture();
    // (the screenshot without weft: the mark is drawn into it afterwards)
    host.style.visibility = 'hidden';
    if (HL) { HL.delete('weft-mark'); HL.delete('weft-mark-on'); }
    await frames(2);
    let r = await ask({ t: 'capture' });
    // (the browser allows two captures a second)
    if (r.error && /MAX_CAPTURE|quota|per second/i.test(r.error)) { await sleep(700); r = await ask({ t: 'capture' }); }
    host.style.visibility = '';
    paint();
    return r;
  }

  // ------------------------------------------------------------------ picking
  const ours = t => t === host;
  const at = (x, y) => {
    const t = document.elementFromPoint(x, y);
    if (!t || ours(t)) return null;
    return doc ? doc.pick(t, x, y) : t;
  };
  function startPicking() {
    picking = true; smaller = []; hover = null;
    hint.style.display = 'block';
    paint();
    redraw();
  }
  function stopPicking() {
    picking = false; hover = null; down = null; dragging = false; selDown = false;
    hint.style.display = 'none'; drag.style.display = 'none';
    paint();
    redraw();
  }
  // the next mark: the selected text, else what the picker is pointed at
  function more() {
    const sel = selected();
    if (sel) takeText(sel); else startPicking();
  }
  const swallow = e => { e.preventDefault(); e.stopImmediatePropagation(); };
  function onDown(e) {
    if (!picking || e.button !== 0 || ours(e.target)) return;
    down = { x: e.clientX, y: e.clientY };
    dragging = false;
    // (in a viewer a drag that starts on text selects it, Alt draws an area anyway)
    selDown = !!(doc && doc.select && !e.altKey && doc.selectable(e.target));
    if (!selDown) swallow(e);
  }
  function onMove(e) {
    if (!picking) return;
    if (down) {
      if (!dragging && Math.hypot(e.clientX - down.x, e.clientY - down.y) > 6) { dragging = true; redraw(); }
      if (dragging) {
        if (selDown) return;
        place(drag, areaRect(e));
        drag.style.display = 'block';
        redraw();
        return;
      }
    }
    if (ours(e.target)) { setHover(null); return; }
    const t = at(e.clientX, e.clientY);
    if (t && t !== hover && !(smaller.length && hover && hover.contains(t))) { smaller = []; setHover(t); }
  }
  const areaRect = e => new DOMRect(Math.min(down.x, e.clientX), Math.min(down.y, e.clientY), Math.abs(e.clientX - down.x), Math.abs(e.clientY - down.y));
  function onUp(e) {
    if (!picking || !down) return;
    swallow(e);
    // (the click that follows this release is the mark's too)
    eatUntil = Date.now() + 400;
    if (selDown) {
      // text was selected: that is the mark (a drag that selected nothing marks nothing); a click marks what is under it
      const was = dragging;
      down = null; dragging = false; selDown = false;
      const sel = selected();
      if (was && sel) takeText(sel);
      else if (!was) { const el = hover || at(e.clientX, e.clientY); if (el) take(el, null); }
      else redraw();
      return;
    }
    if (dragging) {
      const r = areaRect(e);
      down = null; dragging = false; drag.style.display = 'none';
      take(inArea(r), r);
    } else {
      down = null;
      const el = hover || at(e.clientX, e.clientY);
      if (el) take(el, null);
    }
  }
  // what an area is about: the element that holds what lies inside it (sampled at its middle and around)
  function inArea(r) {
    const found = [];
    for (const [fx, fy] of [[0.5, 0.5], [0.25, 0.25], [0.75, 0.25], [0.25, 0.75], [0.75, 0.75]]) {
      const t = at(r.left + r.width * fx, r.top + r.height * fy);
      if (t && t !== document.documentElement) found.push(t);
    }
    if (!found.length) return document.body;
    let el = found[0];
    while (el.parentElement && !found.every(t => el.contains(t))) el = el.parentElement;
    return el;
  }
  // (the press that starts a selection is the browser's)
  const block = e => {
    if (!(picking || Date.now() < eatUntil) || ours(e.target)) return;
    if (selDown && e.type === 'mousedown') return;
    swallow(e);
  };

  function take(el, area) {
    stopPicking();
    const r = area || el.getBoundingClientRect();
    const m = { el, range: null, area: area ? { x: r.left + scrollX, y: r.top + scrollY, w: r.width, h: r.height } : null, info: doc ? doc.describe(el, r, !!area) : describe(el, area, r), shots: null, note: '' };
    keep(m, r, null);
  }
  function takeText(sel) {
    stopPicking();
    getSelection().removeAllRanges();
    const r = sel.range.getBoundingClientRect();
    const info = doc ? doc.describeText(sel.range, r, sel.quote) : describe(sel.el, null, r);
    info.quote = sel.quote;
    keep({ el: sel.el, range: sel.range, area: null, info, shots: null, note: '' }, r, rectsOf(sel.range));
  }
  async function keep(m, r, rects) {
    marks.push(m);
    const n = marks.length;
    paint();
    redraw();
    const shot = await capture();
    if (shot.url) {
      try { m.shots = await cut(shot.url, r, n, rects); } catch {}
    }
    renderMarks();
    showBox();
    if (!shot.url && !shot.none) status('err', 'no screenshot: ' + (shot.error || 'the browser refused'));
  }

  // ------------------------------------------------------------------ the box
  // the box, open for the note of the newest mark
  function showBox() {
    box.style.display = 'block';
    box.classList.remove('folded');
    if (!placed) {
      placed = true;
      const m = marks[marks.length - 1], own = rectOf(m);
      // (marked text: what it is in too, the paragraph or the list, so the rest of it can still be read)
      let r = own;
      const block = m.range && (doc ? doc.around(m.range) : m.el);
      const around = block && block.getBoundingClientRect();
      if (around && around.height < innerHeight / 2) {
        const x0 = Math.min(r.left, around.left), y0 = Math.min(r.top, around.top);
        r = new DOMRect(x0, y0, Math.max(r.right, around.right) - x0, Math.max(r.bottom, around.bottom) - y0);
      }
      // (as tall as it is, and below a viewer's bar)
      const W = Math.min(420, innerWidth - 16), H = Math.max(box.offsetHeight, 240), top = doc ? 48 : 8;
      const fits = v => v >= top && v + H <= innerHeight - 8;
      let x = r.left, y = null;
      // a viewer with room beside what it shows: there, level with the mark; else under or over what
      // the mark is in, under or over the mark itself, beside it, or where there is more room
      const g = doc && doc.gutter ? doc.gutter() : 0;
      if (g && innerWidth - g - 16 >= W) { x = g + 8; y = own.top - 40; }
      if (y == null) y = [r.bottom + 14, r.top - H - 14, own.bottom + 14, own.top - H - 14].find(fits) ?? null;
      if (y == null && (r.right + 14 + W < innerWidth - 8 || r.left - W - 14 >= 8)) {
        y = r.top;
        x = r.right + 14 + W < innerWidth - 8 ? r.right + 14 : r.left - W - 14;
      }
      if (y == null) y = innerHeight - own.bottom > own.top ? own.bottom + 14 : own.top - H - 14;
      y = Math.max(top, Math.min(y, innerHeight - H - 8));
      box.style.left = Math.max(8, Math.min(x, innerWidth - W - 8)) + 'px';
      // (over the mark it grows upwards: a status line that comes later never covers the mark)
      if (y + H <= own.top) { box.style.top = 'auto'; box.style.bottom = `${innerHeight - y - H}px`; }
      else { box.style.bottom = 'auto'; box.style.top = y + 'px'; }
    }
    if (!boards) loadBoards();
    const tas = list.querySelectorAll('textarea');
    if (tas.length) { tas[tas.length - 1].focus(); list.scrollTop = list.scrollHeight; }
  }
  // the pointer on a mark in the box lights it on the page; a click on its name scrolls to it
  function light(i) { lit = i; paint(); redraw(); }
  function show(m) {
    const r = rectOf(m);
    if (r.top >= 40 && r.bottom <= innerHeight - 20) return;
    if (m.area) scrollTo({ top: m.area.y - innerHeight / 3, behavior: 'smooth' });
    else {
      const n = m.range ? m.range.startContainer : m.el;
      const el = n.nodeType === 1 ? n : n.parentElement;
      if (el) el.scrollIntoView({ block: 'center', behavior: 'smooth' });
    }
  }
  function renderMarks() {
    list.innerHTML = '';
    cnt.textContent = marks.length === 1 ? '1 mark' : `${marks.length} marks`;
    marks.forEach((m, i) => {
      const d = document.createElement('div');
      d.className = 'mk';
      const c = compLabel(m.info.comp);
      const what = doc ? m.info.label : (m.range ? 'text in ' : m.info.area ? 'area in ' : '') + m.info.label;
      const crop = !m.range && m.shots && m.shots.crop;
      const said = m.range ? m.info.quote : crop ? '' : (m.info.text || '').trim();
      d.innerHTML = `<div class="lab" title="show it on the page"><span class="n">${i + 1}</span><code>${esc(what)}</code>`
        + `<span class="c">${esc(c)}</span><button class="ib rm" title="remove this mark">✕</button></div>`
        + (said ? `<div class="q${m.range ? '' : ' dim'}">${esc(said)}</div>` : crop ? '<img class="th" alt="">' : '')
        + `<textarea placeholder="${i ? 'and here?' : m.range ? 'what is wrong, what should it say?' : 'what is wrong here, what should it be?'}"></textarea>`;
      const th = d.querySelector('.th');
      // (a page that allows no data: images: no picture then)
      if (th) { th.onerror = () => th.remove(); th.src = crop; }
      const ta = d.querySelector('textarea');
      ta.value = m.note;
      ta.addEventListener('input', () => { m.note = ta.value; });
      d.querySelector('.rm').addEventListener('click', () => {
        marks.splice(i, 1);
        lit = -1;
        if (!marks.length) { close(); return; }
        paint();
        renderMarks();
      });
      d.querySelector('.lab').addEventListener('click', e => { if (!e.target.closest('.rm')) show(m); });
      d.addEventListener('pointerenter', () => light(i));
      d.addEventListener('pointerleave', () => light(-1));
      list.appendChild(d);
    });
    redraw();
  }
  function status(kind, html) { st.className = 'st' + (kind ? ' ' + kind : ''); st.innerHTML = html; }

  async function loadBoards() {
    status('', 'asking weft…');
    const r = await weft({ op: 'boards', url: doc ? doc.page().url : location.href });
    if (!r || !r.ok) {
      boards = { boards: [], agents: [] };
      goBtn.disabled = draftBtn.disabled = true;
      if (r && r.offline) {
        status('err', 'weft is not connected. Run <code>weft extension</code> in a terminal once, then try again.');
      } else status('err', esc((r && r.error) || 'weft did not answer'));
      boards.down = true;
      return;
    }
    boards = r;
    const mem = (await store.get('o:' + origin)) || {};
    const lastAgent = (await store.get('agent')) || r.agent;
    // a server running in a folder says best which board it is, unless the person picked another for it
    const g = r.guess;
    let pick = g && g.dir;
    if (mem.board && (!g || mem.guessed === (g && g.dir))) pick = mem.board;
    if (!r.boards.some(b => b.dir === pick) && !(g && g.new && pick === g.dir)) pick = r.boards.length ? r.boards[0].dir : '';
    boardSel.innerHTML = '';
    for (const b of r.boards) {
      const o = new Option(`${b.name}${b.up ? (b.working ? ` · ${b.working} working` : '') : ' · asleep'}${g && g.dir === b.dir ? (g.file ? ' · this file' : ' · this server') : ''}`, b.dir);
      boardSel.add(o);
    }
    if (g && g.new) boardSel.add(new Option(`+ new board: ${short(g.dir)}${g.file ? ' · this file' : ' · this server'}`, g.dir));
    if (!boardSel.options.length) {
      status('err', 'weft has no boards yet: run <code>weft</code> in your project folder first');
      goBtn.disabled = draftBtn.disabled = true;
      return;
    }
    boardSel.value = pick;
    boardSel.title = g && g.why ? (g.file ? g.why : 'this page: ' + g.why) : 'the board the card goes to';
    boardSel.dataset.agent = lastAgent || '';
    fillTo(mem.to);
    status('', '');
    const log = pageLog();
    const errs = log.log.filter(x => x.k !== 'warn').length, fails = log.failed.length;
    ctx.className = 'ctx' + (errs || fails ? ' bad' : '');
    ctx.textContent = [errs && `${errs} console error${errs > 1 ? 's' : ''}`, fails && `${fails} failed request${fails > 1 ? 's' : ''}`].filter(Boolean).join(' · ')
      + (errs || fails ? ' go along' : '');
  }
  function fillTo(want) {
    const b = boards.boards.find(x => x.dir === boardSel.value);
    toSel.innerHTML = '';
    const agent = boardSel.dataset.agent || boards.agent;
    const kinds = boards.agents.length ? boards.agents : ['claude'];
    for (const a of [agent, ...kinds.filter(k => k !== agent)].filter(Boolean)) toSel.add(new Option(`a new ${a} agent`, 'new:' + a));
    const working = (b && b.agents) || [];
    if (working.length) {
      const g = document.createElement('optgroup');
      g.label = 'an agent on this board';
      for (const a of working) g.appendChild(new Option(`${a.id} · ${a.name}${a.status === 'busy' || a.status === 'starting' ? ' (working: waits as a card)' : ''}`, 'id:' + a.id));
      toSel.add(g);
    }
    if (want && [...toSel.options].some(o => o.value === want)) toSel.value = want;
    draftBtn.disabled = !toSel.value.startsWith('new:');
  }
  boardSel.addEventListener('change', () => fillTo(''));
  toSel.addEventListener('change', () => {
    draftBtn.disabled = !toSel.value.startsWith('new:');
    if (toSel.value.startsWith('new:')) { boardSel.dataset.agent = toSel.value.slice(4); store.set('agent', toSel.value.slice(4)); }
  });

  async function send(draft) {
    if (sending || !boards || boards.down) return;
    if (!marks.some(m => m.note.trim())) {
      box.classList.remove('folded');
      const ta = list.querySelector('textarea');
      if (ta) { ta.classList.remove('shake'); void ta.offsetWidth; ta.classList.add('shake'); ta.focus(); }
      status('err', 'say what is wrong or what you want first');
      return;
    }
    sending = true; goBtn.disabled = draftBtn.disabled = true;
    status('', 'sending…');
    const g = boards.guess;
    const dir = boardSel.value;
    const to = toSel.value;
    const msg = {
      op: 'send', board: dir, new_board: !!(g && g.new && g.dir === dir),
      // a new agent works where the server runs, when that is this board's
      cwd: g && g.dir === dir ? g.cwd || '' : '',
      to: to.startsWith('id:') ? to.slice(3) : '', agent: to.startsWith('new:') ? to.slice(4) : '', draft: !!draft,
      page: doc ? doc.page() : { url: location.href, title: document.title, w: innerWidth, h: innerHeight, dpr: devicePixelRatio },
      marks: marks.map(m => ({ note: m.note.trim(), ...m.info, crop: m.shots && m.shots.crop, view: m.shots && m.shots.view })),
      log: pageLog(),
    };
    const r = await weft(msg);
    sending = false;
    if (!r || !r.ok) {
      goBtn.disabled = false; draftBtn.disabled = !to.startsWith('new:');
      status('err', esc((r && r.error) || 'weft did not answer'));
      return;
    }
    store.set('o:' + origin, { board: dir, guessed: g && g.dir, to: to.startsWith('id:') ? to : (r.id ? 'id:' + r.id : '') });
    const what = r.queued ? `queued for ${esc(r.host || '')} as ${esc(r.id)}` : r.typed ? `told ${esc(r.id)}` : `${esc(r.id)} ${draft ? 'is on the canvas' : 'is on it'}`;
    status('ok', `✓ ${what}${r.name ? ' · ' + esc(r.name) : ''} <button class="open">open board</button>`);
    const ob = st.querySelector('.open');
    if (ob) ob.addEventListener('click', () => { ask({ t: 'open', url: r.url }); close(); });
    marks.length = 0;
    lit = -1;
    list.innerHTML = '';
    cnt.textContent = '';
    paint();
    redraw();
    setTimeout(() => { if (!marks.length && !sending) close(); }, 6000);
  }
  goBtn.addEventListener('click', () => send(false));
  draftBtn.addEventListener('click', () => send(true));
  $('.x').addEventListener('click', () => close());
  $('.more').addEventListener('click', () => more());
  // folded, the box is its title and its buttons: the page shows under it
  foldBtn.addEventListener('click', () => {
    const f = box.classList.toggle('folded');
    foldBtn.textContent = f ? '▸' : '▾';
    foldBtn.title = f ? 'unfold the box' : 'fold the box';
  });

  // the header moves the box
  $('.hd').addEventListener('pointerdown', e => {
    if (e.target.closest('select,button')) return;
    const r = box.getBoundingClientRect(), dx = e.clientX - r.left, dy = e.clientY - r.top;
    const mv = ev => {
      box.style.left = Math.max(0, Math.min(ev.clientX - dx, innerWidth - 60)) + 'px';
      box.style.top = Math.max(0, Math.min(ev.clientY - dy, innerHeight - 30)) + 'px';
      box.style.bottom = 'auto';
    };
    const up = () => { removeEventListener('pointermove', mv, true); removeEventListener('pointerup', up, true); };
    addEventListener('pointermove', mv, true); addEventListener('pointerup', up, true);
  });

  // ------------------------------------------------------------------ keys
  function onKey(e) {
    if (picking) {
      if (e.key === 'Escape') { swallow(e); stopPicking(); if (!marks.length) close(); }
      else if (e.key === 'ArrowUp' && hover) {
        const h = hover, up = doc ? doc.up(h) : h !== document.body && h.parentElement;
        if (up) { swallow(e); setHover(up); smaller.push(h); }
      }
      else if (e.key === 'ArrowDown' && smaller.length) { swallow(e); setHover(smaller.pop()); }
      else if (e.key === 'Enter' && (e.ctrlKey || e.metaKey) && marks.length) { swallow(e); stopPicking(); send(false); }
      else if (e.key === 'Enter' && hover) { swallow(e); take(hover, null); }
      return;
    }
    if (!ours(e.target) || box.style.display !== 'block') return;
    // typing in the box is for the box, not for the page's own shortcuts
    e.stopPropagation();
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); send(false); }
    else if (e.key === 'Escape') {
      e.preventDefault();
      const typed = marks.some(m => m.note.trim());
      if (!typed || Date.now() - discardAt < 1500) close();
      else { discardAt = Date.now(); status('err', 'esc again to throw the note away'); }
    }
  }
  const listeners = [
    ['pointerdown', onDown], ['pointermove', onMove], ['pointerup', onUp], ['keydown', onKey],
    ['mousedown', block], ['mouseup', block], ['click', block], ['dblclick', block], ['contextmenu', block], ['auxclick', block],
  ];
  function open() {
    document.documentElement.appendChild(host);
    for (const [k, f] of listeners) addEventListener(k, f, true);
    fonts();
    more();
  }
  function close() {
    stopPicking();
    marks.length = 0;
    lit = -1;
    paint();
    for (const [k, f] of listeners) removeEventListener(k, f, true);
    host.remove();
    box.style.display = 'none';
    box.classList.remove('folded');
    foldBtn.textContent = '▾';
    placed = false; boards = null;
    goBtn.disabled = draftBtn.disabled = false;
    status('', '');
  }

  globalThis.__weftMark = {
    // the button (or Alt+Shift+W) again: mark another, stop picking, or come back
    again() {
      if (!host.isConnected) { open(); return; }
      if (picking) { stopPicking(); if (!marks.length) close(); return; }
      more();
    },
  };
  open();
})();
