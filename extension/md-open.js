// A Markdown file opened from this computer (dropped on the browser, say) is shown by weft's
// Markdown viewer instead of as plain text: drawn as it reads, its text marked for an agent by
// selecting it (md.html). Replaced, so back goes to where the tab was before the file.
if (/^text\/((x-)?markdown|plain)$/.test(document.contentType)) {
  location.replace(`${chrome.runtime.getURL('md.html')}?file=${encodeURIComponent(location.href)}`);
}
