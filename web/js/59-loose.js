/* ------------------------------------------------------------ loose ends
   A push nobody armed would hold its flow on the canvas for good. The daemon asks git what such a
   push has left (`r.git.left`): its commits on the remote already (with nothing after it, it settles
   by itself) or some still to send. Those, and a deploy nobody armed whose push is in, are "ready":
   a chip in the top bar counts them and arms them. A finished flow that such a card holds says so
   on its cards (`r.held`, 45-archive.js). */
function readyCards() {
  const out = [];
  for (const [id, n] of S.nodes) {
    const r = S.rt.get(id);
    if (!r || !n.draft || r.status !== 'draft') continue;
    if (kindOf(n) === 'git' && r.git && r.git.left) out.push(id);
    else if (n.deploy && r.need && (r.arrived || []).length >= r.need) out.push(id);
  }
  return out;
}
/* what a ready card has left, in a few words */
function readyWhat(n, r) {
  const left = r && r.git && r.git.left;
  if (n.deploy) return 'what it deploys is pushed';
  if (!left) return '';
  return left.push ? `${left.push} commit${left.push === 1 ? '' : 's'} to push` : `already on ${left.on} · arm to go on`;
}
function readyChip() {
  const ids = readyCards();
  if (!ids.length) return null;
  const pushes = ids.filter(id => kindOf(S.nodes.get(id)) === 'git').length;
  const word = pushes === ids.length ? 'to push' : pushes ? 'to arm' : 'to deploy';
  return h('span', { class: 'tk rdy', title: 'disarmed steps whose work is in: arm them', onclick: e => readyMenu(e.currentTarget) },
    h('b', {}, ids.length), ` ready ${word}`);
}
function readyMenu(el) {
  const ids = readyCards(); if (!ids.length) return;
  const b = el.getBoundingClientRect();
  const items = [{ cap: 'disarmed, their work is in' }];
  for (const id of ids) {
    const n = S.nodes.get(id);
    items.push({ label: 'arm ' + n.name, hint: readyWhat(n, S.rt.get(id)), run: () => send('arm', { id }) });
  }
  items.push({ sep: true });
  if (ids.length > 1) items.push({ label: `arm all ${ids.length}`, run: () => ids.forEach(id => send('arm', { id })) });
  items.push({ label: ids.length > 1 ? 'show them' : 'show it', run: () => { select(ids); centerOn(ids[0]); } });
  popup(items, b.left, b.bottom + 6);
}
