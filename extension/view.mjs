// What weft's two viewers share (viewer.js draws PDFs, md.js Markdown): the file named in the
// address and reading it (or saying why it cannot be read), the message in the middle, and the
// picker (mark.js): started by the viewer's button, by the weft button on this tab, as soon as the
// page can be marked when the weft button opened it, and by text selected in the page.

export const $ = s => document.querySelector(s);
export const esc = s => String(s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);
export const src = new URLSearchParams(location.search).get('file') || '';
// the file on this machine, for a file:// address
export const file = (() => { try { const u = new URL(src); return u.protocol === 'file:' ? decodeURIComponent(u.pathname) : ''; } catch { return ''; } })();
export const name = (() => { try { return decodeURIComponent(src.split(/[?#]/)[0].split('/').pop()) || 'file'; } catch { return 'file'; } })();

const msg = $('#msg');
export function say(html, err) { msg.hidden = !html; msg.className = err ? 'err' : ''; msg.innerHTML = html || ''; }

export async function bytes() {
  const r = await fetch(src);
  if (!r.ok) throw new Error(`${r.status} ${r.statusText}`);
  return new Uint8Array(await r.arrayBuffer());
}

export async function cannotRead(e) {
  const why = esc(e && e.message || e);
  if (file) {
    say(`weft cannot read files on this computer. In <code>brave://extensions</code> (<code>chrome://extensions</code>), open the details for weft marks, turn on “Allow access to file URLs” and reload this page.<br><small>${why}</small>`, true);
    return;
  }
  let origin = '';
  try { const u = new URL(src); origin = `${u.protocol}//${u.hostname}/*`; } catch {}
  if (origin && !(await chrome.permissions.contains({ origins: [origin] }).catch(() => false))) {
    say(`to open this file, weft needs to read it from ${esc(new URL(src).host)}.<br><small>${why}</small><br><button id="allow">allow ${esc(new URL(src).host)}</button>`, true);
    $('#allow').addEventListener('click', async () => {
      if (await chrome.permissions.request({ origins: [origin] }).catch(() => false)) location.reload();
    });
    return;
  }
  say(`${esc(name)} could not be read: ${why}`, true);
}

// ------------------------------------------------------------------ the picker
export function mark() {
  if (globalThis.__weftMark) { globalThis.__weftMark.again(); return; }
  const s = document.createElement('script');
  s.src = 'mark.js';
  document.head.appendChild(s);
}

// `ready()` once the page can be marked: when the weft button opened it, the picker starts then
let markWhenReady = location.hash === '#mark';
if (markWhenReady) history.replaceState(null, '', location.pathname + location.search);
export function ready() {
  if (!markWhenReady) return;
  markWhenReady = false;
  mark();
}

// the viewer's button, and the weft button (or Alt+Shift+W) on this tab: background.js says so
export function buttons(btn) {
  btn.addEventListener('click', mark);
  chrome.tabs.getCurrent().then(me => {
    chrome.runtime.onMessage.addListener(m => { if (m && m.t === 'mark' && me && m.tab === me.id) mark(); });
  });
}

// Text selected in `inside` is a mark at once (the picker takes the selection when it starts). A
// double click waits a moment for a third (a word, then the paragraph). While the picker is on it
// takes a selection by itself.
export function selecting(inside) {
  let t = 0;
  addEventListener('pointerdown', () => clearTimeout(t));
  addEventListener('mouseup', e => {
    if (e.button !== 0) return;
    clearTimeout(t);
    t = setTimeout(() => {
      const s = getSelection();
      if (!s.rangeCount || s.isCollapsed || !s.toString().trim()) return;
      const c = s.getRangeAt(0).commonAncestorContainer;
      if (inside().contains(c.nodeType === 1 ? c : c.parentElement)) mark();
    }, e.detail === 2 ? 280 : 0);
  });
}
