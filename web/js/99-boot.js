/* ------------------------------------------------------------ boot */
(function boot() {
  const q = new URLSearchParams(location.hash.slice(1)).get('t');
  if (q) { sessionStorage.setItem('weft-token', q); history.replaceState(null, '', location.pathname); }
  token = q || sessionStorage.getItem('weft-token');
  if (!token) {
    document.body.replaceChildren(h('div', { style: 'padding:44px;max-width:600px;font:calc(14px*var(--fs))/1.7 var(--mono);color:var(--i1)' },
      h('h2', { style: 'font:700 calc(22px*var(--fs)) var(--display);color:var(--i0)' }, 'open weft from your terminal'),
      h('p', {}, 'This page needs the secure link that ', h('code', {}, 'weft'), ' prints. Run it in your project folder and it opens the canvas for you.')));
    return;
  }
  $('#bNew').addEventListener('click', () => openPad());
  applyView(); renderOmni(); initTray(); connect();
})();
