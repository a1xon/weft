// weft marks: the picker and the note box, drawn over the page in a closed shadow root.
// Click marks an element, a drag marks an area, ↑/↓ make the mark bigger/smaller, esc stops.
// Each mark keeps a screenshot (the element outlined, and the whole view), what the element is
// (selector, text, html, box, styles) and, from probe.js, its component and where it is written.
// Send hands it all to weft (background.js -> native messaging -> `weft`): a new agent card on the
// chosen board, or a message to one of its agents.
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

  // ------------------------------------------------------------------ the layer over the page
  const host = document.createElement('weft-mark');
  host.style.cssText = 'all:initial;position:fixed;inset:0;z-index:2147483647;pointer-events:none;display:block';
  const root = host.attachShadow({ mode: 'closed' });
  root.innerHTML = `<style>
    :host { all: initial; }
    * { box-sizing: border-box; }
    .ui { font: 12px/1.45 ui-monospace, "JetBrains Mono", "SF Mono", Menlo, Consolas, monospace; color: #cfe3f5; }
    .hov { position: fixed; pointer-events: none; outline: 2px solid ${ACCENT}; background: rgba(92,200,255,.12); border-radius: 2px; display: none; }
    .tag { position: fixed; pointer-events: none; display: none; max-width: 70vw; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
      background: #05080d; color: ${ACCENT}; border: 1px solid #22405a; padding: 2px 7px; border-radius: 4px; }
    .tag i { color: #7f93ab; font-style: normal; }
    .drag { position: fixed; pointer-events: none; display: none; border: 2px dashed ${ACCENT}; background: rgba(92,200,255,.10); }
    .m { position: fixed; pointer-events: none; outline: 2px solid ${ACCENT}; box-shadow: 0 0 0 1px #000, 0 0 14px rgba(92,200,255,.35); border-radius: 2px; }
    .m b { position: absolute; left: -11px; top: -11px; width: 20px; height: 20px; border-radius: 10px; background: ${ACCENT}; color: #04121c;
      font-weight: 700; text-align: center; line-height: 20px; box-shadow: 0 0 0 2px #04121c; }
    .hint { position: fixed; top: 10px; left: 50%; transform: translateX(-50%); pointer-events: none; display: none;
      background: #05080dee; border: 1px solid #22405a; border-radius: 6px; padding: 5px 12px; color: #9fb3c8; }
    .hint b { color: ${ACCENT}; font-weight: 600; }
    .box { position: fixed; width: 400px; max-width: calc(100vw - 16px); pointer-events: auto; display: none; background: #090d14f2;
      border: 1px solid #2b3a4f; border-radius: 8px; box-shadow: 0 12px 40px rgba(0,0,0,.55), 0 0 0 1px #000; backdrop-filter: blur(6px); }
    .hd { display: flex; align-items: center; gap: 8px; padding: 7px 8px 7px 10px; border-bottom: 1px solid #1c2735; cursor: move; user-select: none; }
    .logo { color: ${ACCENT}; font-weight: 700; letter-spacing: .04em; }
    .hd select { flex: 1; min-width: 0; }
    select, textarea, button { font: inherit; color: inherit; }
    select { background: #0e1520; border: 1px solid #263547; border-radius: 4px; padding: 3px 4px; color: #cfe3f5; }
    button { background: #0e1520; border: 1px solid #263547; border-radius: 4px; padding: 4px 9px; cursor: pointer; color: #cfe3f5; }
    button:hover { border-color: ${ACCENT}; }
    .x { border: none; background: none; color: #7f93ab; padding: 2px 6px; }
    .list { max-height: 46vh; overflow: auto; padding: 4px 10px; }
    .mk { padding: 6px 0; }
    .mk + .mk { border-top: 1px dashed #1c2735; }
    .lab { display: flex; align-items: center; gap: 6px; color: #9fb3c8; min-width: 0; }
    .lab .n { flex: none; width: 18px; height: 18px; border-radius: 9px; background: ${ACCENT}; color: #04121c; font-weight: 700; text-align: center; line-height: 18px; }
    .lab code { color: #e8f4ff; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
    .lab .c { color: ${ACCENT}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; flex: 1; min-width: 0; }
    .lab .rm { margin-left: auto; flex: none; border: none; background: none; color: #5d6f85; padding: 0 4px; }
    textarea { display: block; width: 100%; margin-top: 5px; min-height: 52px; resize: vertical; background: #05080d; border: 1px solid #263547;
      border-radius: 5px; padding: 6px 8px; color: #e8f4ff; outline: none; }
    textarea:focus { border-color: ${ACCENT}; }
    .row { display: flex; align-items: center; gap: 8px; padding: 6px 10px; }
    .row label { color: #7f93ab; }
    .row select { flex: 1; min-width: 0; }
    .more { border-style: dashed; flex: none; white-space: nowrap; }
    .ctx { color: #7f93ab; margin-left: auto; }
    .ctx.bad { color: #ffb86b; }
    .ft { display: flex; align-items: center; gap: 8px; padding: 8px 10px; border-top: 1px solid #1c2735; }
    .st { flex: 1; min-width: 0; color: #7f93ab; }
    .st.err { color: #ff5c7a; }
    .st.ok { color: #4ade80; }
    .st code { color: #e8f4ff; background: #05080d; padding: 0 4px; border-radius: 3px; }
    .go { background: ${ACCENT}; color: #04121c; border-color: ${ACCENT}; font-weight: 700; }
    .go:disabled, .draft:disabled { opacity: .45; cursor: default; }
    kbd { font: inherit; opacity: .7; font-size: 10px; }
    .shake { animation: sh .3s; }
    @keyframes sh { 25% { transform: translateX(-4px); } 75% { transform: translateX(4px); } }
  </style>
  <div class="ui">
    <div class="hov"></div><div class="tag"></div><div class="drag"></div>
    <div class="marks"></div>
    <div class="hint">click to mark · drag for an area · <b>↑ ↓</b> bigger / smaller · <b>esc</b> stop</div>
    <div class="box">
      <div class="hd"><span class="logo">◇ weft</span><select class="board" title="the board the card goes to"></select><button class="x" title="close (esc)">✕</button></div>
      <div class="list"></div>
      <div class="row"><button class="more" title="Alt+Shift+W">+ mark another</button><span class="ctx"></span></div>
      <div class="row"><label>to</label><select class="to"></select></div>
      <div class="ft"><span class="st"></span><button class="draft" title="put the card on the canvas without starting it">card only</button><button class="go">send <kbd>ctrl ⏎</kbd></button></div>
    </div>
  </div>`;
  const $ = s => root.querySelector(s);
  const hov = $('.hov'), tag = $('.tag'), drag = $('.drag'), marksEl = $('.marks'), hint = $('.hint'), box = $('.box');
  const boardSel = $('.board'), toSel = $('.to'), list = $('.list'), st = $('.st'), ctx = $('.ctx');
  const goBtn = $('.go'), draftBtn = $('.draft');

  // ------------------------------------------------------------------ state
  const marks = [];          // { el, area (page px) | null, info, shots, note }
  let picking = false, hover = null, smaller = [], down = null, dragging = false, eatUntil = 0;
  let boards = null, sending = false, placed = false, discardAt = 0, raf = 0;
  const origin = location.origin;

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

  // ------------------------------------------------------------------ drawing
  const place = (el, r) => { el.style.left = r.left + 'px'; el.style.top = r.top + 'px'; el.style.width = r.width + 'px'; el.style.height = r.height + 'px'; };
  const rectOf = m => (m.area ? new DOMRect(m.area.x - scrollX, m.area.y - scrollY, m.area.w, m.area.h) : m.el.getBoundingClientRect());
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
      place(k, rectOf(m));
      if (k.dataset.n !== String(i + 1)) { k.dataset.n = String(i + 1); k.innerHTML = `<b>${i + 1}</b>`; }
    });
    if (marks.length || picking) raf = requestAnimationFrame(draw);
  }
  const redraw = () => { if (!raf) raf = requestAnimationFrame(draw); };
  function setHover(el) {
    if (el === hover) return;
    hover = el;
    if (!el) { redraw(); return; }
    const r = el.getBoundingClientRect();
    const c = compLabel(probe(el).comp);
    tag.innerHTML = `${esc(label(el))} <i>${Math.round(r.width)}×${Math.round(r.height)}</i>${c ? ' ' + esc(c) : ''}`;
    redraw();
  }

  // ------------------------------------------------------------------ screenshots
  const load = url => new Promise((res, rej) => { const i = new Image(); i.onload = () => res(i); i.onerror = rej; i.src = url; });
  function outline(g, x, y, w, h, n, s) {
    g.lineJoin = 'round';
    g.strokeStyle = 'rgba(0,0,0,.85)'; g.lineWidth = 5 * s; g.strokeRect(x, y, w, h);
    g.strokeStyle = ACCENT; g.lineWidth = 2.5 * s; g.strokeRect(x, y, w, h);
    const rad = 11 * s, cx = Math.max(rad + 2, x), cy = Math.max(rad + 2, y);
    g.beginPath(); g.arc(cx, cy, rad, 0, Math.PI * 2); g.fillStyle = ACCENT; g.fill();
    g.lineWidth = 2 * s; g.strokeStyle = '#04121c'; g.stroke();
    g.fillStyle = '#04121c'; g.font = `700 ${13 * s}px ui-monospace, monospace`; g.textAlign = 'center'; g.textBaseline = 'middle';
    g.fillText(String(n), cx, cy + s);
  }
  async function cut(url, r, n) {
    const img = await load(url);
    const s = img.naturalWidth / innerWidth;
    // the whole view, the mark outlined, at most 1600 px wide
    const vs = Math.min(1, 1600 / img.naturalWidth);
    const v = document.createElement('canvas');
    v.width = Math.round(img.naturalWidth * vs); v.height = Math.round(img.naturalHeight * vs);
    const vg = v.getContext('2d');
    vg.drawImage(img, 0, 0, v.width, v.height);
    outline(vg, r.left * s * vs, r.top * s * vs, r.width * s * vs, r.height * s * vs, n, s * vs);
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
      outline(cg, (r.left - x0) * k, (r.top - y0) * k, r.width * k, r.height * k, n, Math.max(1, k));
      crop = c.toDataURL('image/png');
    }
    return { view: v.toDataURL('image/png'), crop };
  }
  async function capture() {
    host.style.visibility = 'hidden';
    await frames(2);
    let r = await ask({ t: 'capture' });
    // (the browser allows two captures a second)
    if (r.error && /MAX_CAPTURE|quota|per second/i.test(r.error)) { await sleep(700); r = await ask({ t: 'capture' }); }
    host.style.visibility = '';
    return r;
  }

  // ------------------------------------------------------------------ picking
  const ours = t => t === host;
  const at = (x, y) => { const t = document.elementFromPoint(x, y); return t && !ours(t) ? t : null; };
  function startPicking() {
    picking = true; smaller = []; hover = null;
    hint.style.display = 'block';
    cursor(true);
    redraw();
  }
  function stopPicking() {
    picking = false; hover = null; down = null; dragging = false;
    hint.style.display = 'none'; drag.style.display = 'none';
    cursor(false);
    redraw();
  }
  function cursor(on) {
    let s = document.getElementById('weft-mark-cursor');
    if (on && !s) {
      s = document.createElement('style');
      s.id = 'weft-mark-cursor';
      s.textContent = '*{cursor:crosshair!important}';
      (document.head || document.documentElement).appendChild(s);
    } else if (!on && s) s.remove();
  }
  const swallow = e => { e.preventDefault(); e.stopImmediatePropagation(); };
  function onDown(e) {
    if (!picking || e.button !== 0 || ours(e.target)) return;
    swallow(e);
    down = { x: e.clientX, y: e.clientY };
    dragging = false;
  }
  function onMove(e) {
    if (!picking) return;
    if (down) {
      if (!dragging && Math.hypot(e.clientX - down.x, e.clientY - down.y) > 6) dragging = true;
      if (dragging) {
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
  const block = e => { if ((picking || Date.now() < eatUntil) && !ours(e.target)) swallow(e); };

  async function take(el, area) {
    stopPicking();
    const r = area || el.getBoundingClientRect();
    const n = marks.length + 1;
    const m = { el, area: area ? { x: r.left + scrollX, y: r.top + scrollY, w: r.width, h: r.height } : null, info: describe(el, area, r), shots: null, note: '' };
    marks.push(m);
    redraw();
    const shot = await capture();
    if (shot.url) {
      try { m.shots = await cut(shot.url, r, n); } catch {}
    }
    showBox();
    renderMarks(true);
    if (!shot.url) status('err', 'no screenshot: ' + (shot.error || 'the browser refused'));
  }

  // ------------------------------------------------------------------ the box
  function showBox() {
    box.style.display = 'block';
    if (!placed) {
      placed = true;
      const r = rectOf(marks[marks.length - 1]);
      const W = Math.min(400, innerWidth - 16), H = 300;
      let x = r.left, y = r.bottom + 14;
      if (y + H > innerHeight - 8) y = r.top - H - 14;
      if (y < 8) { y = r.top; x = r.right + 14 + W < innerWidth - 8 ? r.right + 14 : r.left - W - 14; }
      box.style.left = Math.max(8, Math.min(x, innerWidth - W - 8)) + 'px';
      box.style.top = Math.max(8, Math.min(y, innerHeight - H - 8)) + 'px';
    }
    if (!boards) loadBoards();
  }
  function renderMarks(focusLast) {
    list.innerHTML = '';
    marks.forEach((m, i) => {
      const d = document.createElement('div');
      d.className = 'mk';
      const c = compLabel(m.info.comp);
      d.innerHTML = `<div class="lab"><span class="n">${i + 1}</span><code>${esc(m.info.area ? 'area in ' + m.info.label : m.info.label)}</code>`
        + `<span class="c">${esc(c)}</span><button class="rm" title="remove this mark">✕</button></div>`
        + `<textarea placeholder="${i ? 'and here?' : 'what is wrong here, what should it be?'}"></textarea>`;
      const ta = d.querySelector('textarea');
      ta.value = m.note;
      ta.addEventListener('input', () => { m.note = ta.value; });
      d.querySelector('.rm').addEventListener('click', () => {
        marks.splice(i, 1);
        if (!marks.length) { close(); return; }
        renderMarks(false);
      });
      list.appendChild(d);
    });
    const tas = list.querySelectorAll('textarea');
    if (focusLast && tas.length) tas[tas.length - 1].focus();
    redraw();
  }
  function status(kind, html) { st.className = 'st' + (kind ? ' ' + kind : ''); st.innerHTML = html; }

  async function loadBoards() {
    status('', 'asking weft…');
    const r = await weft({ op: 'boards', url: location.href });
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
      const o = new Option(`${b.name}${b.up ? (b.working ? ` · ${b.working} working` : '') : ' · asleep'}${g && g.dir === b.dir ? ' · this server' : ''}`, b.dir);
      boardSel.add(o);
    }
    if (g && g.new) boardSel.add(new Option(`+ new board: ${short(g.dir)} · this server`, g.dir));
    if (!boardSel.options.length) {
      status('err', 'weft has no boards yet: run <code>weft</code> in your project folder first');
      goBtn.disabled = draftBtn.disabled = true;
      return;
    }
    boardSel.value = pick;
    boardSel.title = g && g.why ? 'this page: ' + g.why : 'the board the card goes to';
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
      page: { url: location.href, title: document.title, w: innerWidth, h: innerHeight, dpr: devicePixelRatio },
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
    list.innerHTML = '';
    redraw();
    setTimeout(() => { if (!marks.length && !sending) close(); }, 6000);
  }
  goBtn.addEventListener('click', () => send(false));
  draftBtn.addEventListener('click', () => send(true));
  $('.x').addEventListener('click', () => close());
  $('.more').addEventListener('click', () => startPicking());

  // the header moves the box
  $('.hd').addEventListener('pointerdown', e => {
    if (e.target.closest('select,button')) return;
    const r = box.getBoundingClientRect(), dx = e.clientX - r.left, dy = e.clientY - r.top;
    const mv = ev => { box.style.left = Math.max(0, Math.min(ev.clientX - dx, innerWidth - 60)) + 'px'; box.style.top = Math.max(0, Math.min(ev.clientY - dy, innerHeight - 30)) + 'px'; };
    const up = () => { removeEventListener('pointermove', mv, true); removeEventListener('pointerup', up, true); };
    addEventListener('pointermove', mv, true); addEventListener('pointerup', up, true);
  });

  // ------------------------------------------------------------------ keys
  function onKey(e) {
    if (picking) {
      if (e.key === 'Escape') { swallow(e); stopPicking(); if (!marks.length) close(); }
      else if (e.key === 'ArrowUp' && hover && hover.parentElement && hover !== document.body) { swallow(e); const h = hover; setHover(h.parentElement); smaller.push(h); }
      else if (e.key === 'ArrowDown' && smaller.length) { swallow(e); setHover(smaller.pop()); }
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
    startPicking();
  }
  function close() {
    stopPicking();
    marks.length = 0;
    for (const [k, f] of listeners) removeEventListener(k, f, true);
    host.remove();
    box.style.display = 'none';
    placed = false; boards = null;
    goBtn.disabled = draftBtn.disabled = false;
    status('', '');
  }

  globalThis.__weftMark = {
    // the button (or Alt+Shift+W) again: mark another, stop picking, or come back
    again() {
      if (!host.isConnected) { open(); return; }
      if (picking) { stopPicking(); if (!marks.length) close(); return; }
      startPicking();
    },
  };
  open();
})();
