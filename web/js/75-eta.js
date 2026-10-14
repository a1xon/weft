/* ------------------------------------------------------------ ETA: the agent's own estimate, counted down */
const etaClock = ms => {
  const neg = ms < 0;
  const s = Math.ceil(Math.abs(ms) / 1000);
  const hh = Math.floor(s / 3600), mm = Math.floor(s % 3600 / 60), ss = s % 60;
  return (neg ? '+' : '') + (hh ? `${hh}:${String(mm).padStart(2, '0')}` : String(mm)) + ':' + String(ss).padStart(2, '0');
};
const hasEta = n => !!n && (n.checks || []).some(c => c.kind === 'eta');
const hasPing = n => !!n && (n.checks || []).some(c => c.kind === 'ping');
function renderEta(id, n, r) {
  const el = nodeEls.get(id); if (!el) return;
  const live = hasEta(n) && r && (r.eta || r.status === 'busy' || r.status === 'starting');
  if (!live) { if (el.eta) { el.eta.remove(); el.eta = null; } return; }
  if (!el.eta) {
    el.eta = h('div', { class: 'eta', title: 'how long the agent says it still needs' },
      h('span', { class: 'el' }, 'ETA'), h('b', { class: 'et' }), h('span', { class: 'en' }), h('i', { class: 'bar' }));
    el.root.append(el.eta);
  }
  el.eta._e = r.eta || null;
  tickEta(el);
}
function tickEta(el) {
  const b = el.eta; if (!b) return;
  const e = b._e, now = Date.now();
  const t = b.querySelector('.et'), note = b.querySelector('.en'), bar = b.querySelector('.bar');
  if (!e) { t.textContent = '…'; note.textContent = 'estimating'; bar.style.width = '0'; b.className = 'eta wait'; return; }
  const left = e.at - now;
  t.textContent = etaClock(left);
  note.textContent = e.note || '';
  const span = Math.max(1, e.at - e.set);
  bar.style.width = Math.max(0, Math.min(100, (now - e.set) / span * 100)) + '%';
  b.className = 'eta' + (left < 0 ? ' over' : left < Math.max(15000, span * 0.15) ? ' soon' : '');
}
/* the one number for the whole flow: when the last agent that has given an estimate says it is done */
function renderEtaTop() {
  const box = $('#etaTop'); if (!box) return;
  const items = [];
  for (const [id, r] of S.rt) {
    const n = S.nodes.get(id);
    if (r.eta && hasEta(n) && (r.status === 'busy' || r.status === 'starting')) items.push({ id, n, e: r.eta });
  }
  if (!items.length) { box.hidden = true; return; }
  const last = items.reduce((a, b) => (b.e.at > a.e.at ? b : a));
  const left = last.e.at - Date.now();
  box.hidden = false;
  box.dataset.id = last.id;
  box.className = left < 0 ? 'over' : left < 15000 ? 'soon' : '';
  box.querySelector('.et').textContent = etaClock(left);
  box.querySelector('.en').textContent = items.length > 1 ? `${items.length} agents` : last.n.name;
  box.title = items.length > 1 ? 'when the last of them says it will be done' : `${last.n.name} says it will be done in ${etaClock(left)}`;
}
setInterval(() => {
  for (const el of nodeEls.values()) if (el.eta) tickEta(el);
  renderEtaTop();
}, 1000);
/* ping: a notification when a watched agent is done or needs you */
const baseTitle = document.title;
function notifyPing(n, r) {
  const what = { done: 'is done', failed: 'failed', attention: 'needs you', blocked: 'is blocked' }[r.status];
  if (!what) return;
  const body = (r.out || '').replace(/\s+/g, ' ').slice(0, 140);
  let shown = false;
  if ('Notification' in window && Notification.permission === 'granted') {
    try {
      const nt = new Notification(`${n.name} ${what}`, { body, tag: 'weft-' + n.id });
      nt.onclick = () => { window.focus(); select([n.id]); centerOn(n.id); };
      shown = true;
    } catch {}
  }
  if (!shown) toast(r.status === 'done' ? 'ok' : 'warn', `${n.name} ${what}`, { ms: 9000 });
  if (!document.hasFocus()) document.title = `● ${n.name} ${what} – weft`;
}
addEventListener('focus', () => { document.title = baseTitle; });
