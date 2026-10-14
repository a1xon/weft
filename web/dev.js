/* dev: not part of weft proper (see src/dev.rs). The page was read from web/ on disk; this
   follows the edits there. A style sheet is swapped in place. A changed script or shell reloads
   the page, but not while you type or drag (the tag in the corner says it is waiting), and puts
   back what was open: selection, flyout, terminal, launch pad. An error in the page's script
   shows in a bar that names its file and line. Its own <script>, so it runs even when the
   page's script does not parse, and reloads once that is fixed. */
(cfg => {
  'use strict';
  const have = { css: cfg.css, page: cfg.page };
  const KEEP = 'weft-dev-keep';
  const sheet = document.querySelector('head style');
  const sleep = ms => new Promise(r => setTimeout(r, ms));

  const style = document.createElement('style');
  style.textContent = `
#devtag{position:fixed;z-index:90;right:14px;bottom:16px;pointer-events:none;font:500 10.5px/18px var(--mono,monospace);
  color:var(--i3,#566782);border:1px solid var(--l1,#1b2634);background:rgba(7,10,16,.9);padding:2px 8px;letter-spacing:.03em}
#devtag b{color:var(--cy,#5cc8ff);font-weight:600}
#devtag.flash{color:var(--cy,#5cc8ff);border-color:var(--cy2,#2f95c9)}
#devtag.stale{pointer-events:auto;cursor:pointer;color:#12040a;background:var(--am,#ffb454);border-color:var(--am,#ffb454)}
#devtag.away{opacity:.5}
#deverr{position:fixed;z-index:95;left:50%;top:50px;transform:translateX(-50%);max-width:min(900px,calc(100vw - 32px));display:flex;gap:12px;
  align-items:flex-start;padding:8px 10px 8px 12px;background:#1a0810;border:1px solid #5a1f2c;border-left:3px solid var(--rd,#ff5c7a);
  color:#ffd3dc;font:500 12px/1.5 var(--mono,monospace);white-space:pre-wrap;word-break:break-word}
#deverr b{color:var(--rd,#ff5c7a)}
#deverr button{font:inherit;color:inherit;background:none;border:0;cursor:pointer;opacity:.7;padding:0 2px}`;
  document.head.append(style);

  /* ---------------------------------------------------------- the tag and the error bar */
  let tag = null, bar = null, flashT = 0;
  const mount = () => {
    if (tag || !document.body) return !!tag;
    tag = document.createElement('div'); tag.id = 'devtag';
    tag.title = 'dev: this page follows edits to web/ on disk (src/dev.rs). WEFT_WEB_DIR=off turns it off.';
    tag.onclick = () => { if (stale) reload(); };
    document.body.append(tag); show('live');
    return true;
  };
  function show(what) {
    if (!mount()) return;
    tag.className = what === 'stale' ? 'stale' : what === 'away' ? 'away' : '';
    tag.innerHTML = what === 'stale' ? 'ui changed &middot; reloads when you are done &middot; <u>now</u>'
      : what === 'away' ? 'live ui &middot; waiting for weft' : '<b>&#9673;</b> live ui';
  }
  function flash(text) {
    if (!mount() || stale) return;
    tag.className = 'flash'; tag.innerHTML = '<b>&#8635;</b> ' + text;
    clearTimeout(flashT); flashT = setTimeout(() => { if (!stale) show('live'); }, 1400);
  }

  // an inline script's error is reported by its line in the page: name the file and its line instead
  const where = line => {
    let at = null;
    for (const [l, f] of cfg.lines) if (l <= line) at = `${f}:${line - l + 1}`;
    return at || `page:${line}`;
  };
  let last = '', count = 0;
  function oops(msg) {
    if (!document.body) return void addEventListener('DOMContentLoaded', () => oops(msg), { once: true });
    count = msg === last ? count + 1 : 1; last = msg;
    if (!bar) {
      bar = document.createElement('div'); bar.id = 'deverr';
      document.body.append(bar);
    }
    bar.replaceChildren();
    const text = document.createElement('span');
    text.innerHTML = '<b>ui error</b>  ';
    text.append(msg + (count > 1 ? `  (×${count})` : ''));
    const x = document.createElement('button');
    x.textContent = '×'; x.title = 'dismiss';
    x.onclick = () => { bar.remove(); bar = null; last = ''; };
    bar.append(text, x);
  }
  addEventListener('error', e => {
    if (!(e instanceof ErrorEvent)) return;
    const page = !e.filename || e.filename.split('#')[0] === location.href.split('#')[0];
    oops(`${page ? where(e.lineno) : e.filename + ':' + e.lineno}  ${e.message}`);
  });
  addEventListener('unhandledrejection', e => oops('unhandled: ' + ((e.reason && e.reason.message) || e.reason)));

  /* ---------------------------------------------------------- following the files */
  let stale = false, held = false, typed = null, retry = 0;
  const texty = t => t.tagName === 'TEXTAREA' || t.isContentEditable
    || (t.tagName === 'INPUT' && /^(text|search|url|email|tel|number|password)$/.test(t.type));
  addEventListener('input', e => { if (e.target && texty(e.target)) typed = e.target; }, true);
  addEventListener('pointerdown', () => { held = true; }, true);
  const up = () => { held = false; if (stale) setTimeout(settle, 0); };
  addEventListener('pointerup', up, true);
  addEventListener('pointercancel', up, true);
  addEventListener('blur', up);
  // a drag, or text you typed that is still on screen: a reload now would take it from you
  const busy = () => held || !!(typed && typed.isConnected && typed.getClientRects().length
    && String(typed.value ?? typed.textContent ?? '').trim());

  function settle() {
    if (!stale) return;
    clearTimeout(retry);
    if (busy()) { show('stale'); retry = setTimeout(settle, 1000); return; }
    reload();
  }
  function reload() { keep(); location.reload(); }

  async function restyle() {
    try {
      const r = await fetch('/dev/css', { cache: 'no-store' });
      if (!r.ok) throw new Error(r.status);
      const css = await r.text();
      if (sheet) sheet.textContent = css;
      have.css = r.headers.get('x-weft-css') || have.css;
      flash('styles');
    } catch { await sleep(1000); }
  }

  async function follow() {
    for (;;) {
      let r;
      try {
        const res = await fetch(`/dev/wait?css=${have.css}&page=${have.page}`, { cache: 'no-store' });
        if (!res.ok) throw new Error(res.status);
        r = await res.json();
      } catch { show('away'); await sleep(2000); continue; }
      if (r.page !== have.page) { have.page = r.page; stale = true; }
      if (r.css !== have.css) await restyle();
      if (stale) settle(); else if (tag && tag.className === 'away') show('live');
    }
  }

  /* ---------------------------------------------------------- what was open comes back */
  // these names belong to the page's script; when it is broken they are not there, and nothing happens
  function keep() {
    let k = null;
    try {
      k = { sel: [...S.sel], fly: S.fly, focus: S.focus, pad: pad.open };
      localStorage.setItem('weft-view:' + S.project.dir, JSON.stringify(S.v));
    } catch {}
    try {
      if (k) sessionStorage.setItem(KEEP, JSON.stringify(k));
      else if (!sessionStorage.getItem(KEEP)) sessionStorage.setItem(KEEP, '{}');
    } catch {}
  }
  function restore() {
    let k = null;
    try { k = JSON.parse(sessionStorage.getItem(KEEP) || 'null'); } catch {}
    if (!k) return;
    try { padShown = true; } catch { return; } // the page's script did not run: keep it for the next load
    const t0 = Date.now();
    const iv = setInterval(() => {
      let ready = false;
      try { ready = loaded; } catch {}
      if (!ready) { if (Date.now() - t0 > 20000) clearInterval(iv); return; }
      clearInterval(iv);
      try { sessionStorage.removeItem(KEEP); } catch {}
      try {
        const ids = (k.sel || []).filter(id => S.nodes.has(id));
        if (ids.length) select(ids);
        if (k.fly && S.nodes.has(k.fly.id)) openFly(k.fly.id, k.fly.tab);
        if (k.focus && S.nodes.has(k.focus)) openFocus(k.focus);
        if (k.pad && !pad.open) openPad();
        if (!k.pad && pad.open) closePad();
      } catch (e) { console.warn('weft dev: could not put back what was open', e); }
    }, 50);
  }

  addEventListener('DOMContentLoaded', () => { mount(); restore(); });
  addEventListener('load', () => follow());
})(__DEVCFG__);
