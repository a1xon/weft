// weft marks: the toolbar button (or Alt+Shift+W) starts the picker on the page; this worker takes
// the screenshots and carries what was marked to weft. weft is reached through the browser's
// native messaging (the `weft` binary itself, installed by `weft extension`), so no address and no
// token of a weft daemon is ever known to a web page or to this extension.

const HOST = 'weft.browser';

async function start(tab) {
  if (!tab || tab.id == null) return;
  try {
    // the page's own world (components, console) first, then the picker beside it
    await chrome.scripting.executeScript({ target: { tabId: tab.id }, files: ['probe.js'], world: 'MAIN' });
    await chrome.scripting.executeScript({ target: { tabId: tab.id }, files: ['mark.js'] });
  } catch (e) {
    // the browser's own pages, the web store, the PDF viewer: nothing may run there
    chrome.action.setBadgeBackgroundColor({ tabId: tab.id, color: '#ff5c7a' });
    chrome.action.setBadgeText({ tabId: tab.id, text: '×' });
    chrome.action.setTitle({ tabId: tab.id, title: 'weft cannot mark this page: ' + (e && e.message || e) });
    setTimeout(() => chrome.action.setBadgeText({ tabId: tab.id, text: '' }), 2500);
  }
}

chrome.action.onClicked.addListener(start);

chrome.runtime.onMessage.addListener((m, sender, reply) => {
  const tab = sender.tab;
  if (!tab) return false;
  if (m.t === 'capture') {
    chrome.tabs.captureVisibleTab(tab.windowId, { format: 'png' })
      .then(url => reply({ url }), e => reply({ error: String(e && e.message || e) }));
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
