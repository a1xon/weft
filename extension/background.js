// weft marks: the toolbar button (or Alt+Shift+W) starts the picker on the page; this worker takes
// the screenshots and carries what was marked to weft. weft is reached through the browser's
// native messaging (the `weft` binary itself, installed by `weft extension`), so no address and no
// token of a weft daemon is ever known to a web page or to this extension. A PDF opens in weft's
// own viewer (viewer.html): the browser's PDF viewer lets nothing in. Markdown opens in weft's
// Markdown viewer (md.html), drawn as it reads, its text marked by selecting it; a Markdown file
// from this computer goes there by itself (md-open.js).

const HOST = 'weft.browser';
const VIEWER = chrome.runtime.getURL('viewer.html');
const MD = chrome.runtime.getURL('md.html');

const path = url => { try { return new URL(url).pathname; } catch { return ''; } };
const isPdf = url => /\.pdf$/i.test(path(url));
const isMd = url => /\.(md|markdown|mdown|mkdn?)$/i.test(path(url));
// (back goes to the browser's own view again; `#mark` starts the picker once there is something to mark)
const openIn = (viewer, tab) => chrome.tabs.update(tab.id, { url: `${viewer}?file=${encodeURIComponent(tab.url)}#mark` });

async function start(tab) {
  if (!tab || tab.id == null) return;
  // weft's viewers mark by themselves
  if ((tab.url || '').startsWith(VIEWER) || (tab.url || '').startsWith(MD)) {
    chrome.runtime.sendMessage({ t: 'mark', tab: tab.id }).catch(() => {});
    return;
  }
  try {
    const [probe] = await chrome.scripting.executeScript({ target: { tabId: tab.id }, func: () => document.contentType });
    const type = probe && probe.result;
    if (type === 'application/pdf') { openIn(VIEWER, tab); return; }
    if (/^text\/(x-)?markdown$/.test(type) || (type === 'text/plain' && isMd(tab.url))) { openIn(MD, tab); return; }
    // the page's own world (components, console) first, then the picker beside it
    await chrome.scripting.executeScript({ target: { tabId: tab.id }, files: ['probe.js'], world: 'MAIN' });
    await chrome.scripting.executeScript({ target: { tabId: tab.id }, files: ['mark.js'] });
  } catch (e) {
    if (isPdf(tab.url)) { openIn(VIEWER, tab); return; }
    if (isMd(tab.url)) { openIn(MD, tab); return; }
    // the browser's own pages, the web store: nothing may run there
    chrome.action.setBadgeBackgroundColor({ tabId: tab.id, color: '#ff5c7a' });
    chrome.action.setBadgeText({ tabId: tab.id, text: '×' });
    chrome.action.setTitle({ tabId: tab.id, title: 'weft cannot mark this page: ' + (e && e.message || e) });
    setTimeout(() => chrome.action.setBadgeText({ tabId: tab.id, text: '' }), 2500);
  }
}

chrome.action.onClicked.addListener(start);

// weft's fonts, for the box on a web page (it adds them to the page itself: a page may load nothing from here)
const FONTS = [['weft-mono', '100 800', 'fonts/jbmono.woff2'], ['weft-display', '600', 'fonts/chakra-600.woff2']];
let fonts = null;
const base64 = buf => {
  const b = new Uint8Array(buf);
  let s = '';
  for (let i = 0; i < b.length; i += 0x8000) s += String.fromCharCode(...b.subarray(i, i + 0x8000));
  return btoa(s);
};
const loadFonts = () => (fonts ||= Promise.all(FONTS.map(async ([family, weight, file]) => {
  const r = await fetch(chrome.runtime.getURL(file));
  if (!r.ok) throw new Error(file);
  return { family, weight, data: base64(await r.arrayBuffer()) };
})).then(fonts => ({ fonts }), () => { fonts = null; return { fonts: [] }; }));

chrome.runtime.onMessage.addListener((m, sender, reply) => {
  const tab = sender.tab;
  if (!tab) return false;
  if (m.t === 'capture') {
    chrome.tabs.captureVisibleTab(tab.windowId, { format: 'png' })
      .then(url => reply({ url }), e => reply({ error: String(e && e.message || e) }));
    return true;
  }
  if (m.t === 'fonts') {
    loadFonts().then(reply);
    return true;
  }
  if (m.t === 'weft') {
    chrome.runtime.sendNativeMessage(HOST, m.msg)
      .then(r => reply(r || { ok: false, error: 'weft gave no answer' }),
        e => reply({ ok: false, offline: true, error: String(e && e.message || e) }));
    return true;
  }
  if (m.t === 'open' && /^http:\/\/127\.0\.0\.1:\d+\//.test(m.url || '')) {
    chrome.tabs.create({ url: m.url, index: tab.index + 1, openerTabId: tab.id });
  }
  return false;
});
