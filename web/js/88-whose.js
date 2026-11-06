/* ------------------------------------------------------------ whose the uncommitted files are (engine/owners.rs)
   Every uncommitted file of a work tree the board's agents work in is put to the cards whose steps
   changed it; what no card on the board changed is left over (a card that was filed or deleted,
   another session, the person). Each card says how many it has. The git tree's "N uncommitted" line
   says how many no card has, and a click unfolds the list there, grouped by card: a click on a file
   shows its diff, on a card shows the card, and "commit with an agent" has a fresh agent commit what
   is left over (what is still valid and needed; the rest it names). Where several cards were at work
   at the same time and none of them said it changed a file, the file goes to all of them, "maybe".
   A card's count is a notch hanging from its bottom edge, in the card's own border colour: it reads
   at any zoom, and what unfolds under the card moves down to make room for it. */
const OWN = { tops: new Map(), cards: new Map(), open: false, focus: null, focusUntil: 0 };

function loadOwners(all) {
  OWN.tops = new Map(Object.entries(all || {}).filter(([, o]) => o));
  indexOwners();
}
function onOwners(top, own) {
  if (own) OWN.tops.set(top, own); else OWN.tops.delete(top);
  indexOwners();
  if (!treeEl.hidden && (GT.open || GT.peeking) && GT.data && treeTop() === top) renderRows();
}
/* each card's files, from every work tree */
function indexOwners() {
  const cards = new Map();
  for (const [top, o] of OWN.tops) for (const f of o.files || []) for (const id of f.by || []) {
    let c = cards.get(id);
    if (!c) cards.set(id, c = { top, files: [], unsure: 0 });
    c.files.push(f);
    if (f.unsure) c.unsure++;
  }
  const old = OWN.cards;
  OWN.cards = cards;
  for (const id of new Set([...old.keys(), ...cards.keys()])) renderOwn(id);
}
const ownName = id => { const n = S.nodes.get(id); return n ? n.name : id; };
const stWord = st => ({ '?': 'new', D: 'deleted', A: 'added', R: 'renamed', M: 'changed' })[st] || 'changed';

/* ---- on the card: a notch under its bottom edge, "3 uncommitted" (a click shows them in the git tree) */
function renderOwn(id) {
  const el = nodeEls.get(id); if (!el) return;
  const c = OWN.cards.get(id), n = c ? c.files.length : 0;
  const key = c ? JSON.stringify(c.files.map(f => [f.p, f.st, f.unsure])) : '';
  if (el._own === key) return;
  el._own = key;
  el.root.classList.toggle('notched', n > 0);
  if (!n) { if (el.uc) el.uc.hidden = true; return; }
  if (!el.uc) {
    el.uc = h('button', { class: 'ucn' });
    el.uc.addEventListener('click', e => { e.stopPropagation(); showOwn(id); });
    el.uc.addEventListener('pointerdown', e => e.stopPropagation());
    el.root.append(el.uc);
  }
  el.uc.hidden = false;
  el.uc.classList.toggle('maybe', c.unsure > 0);
  el.uc.replaceChildren(...nn([h('b', {}, String(n)), ' uncommitted', c.unsure ? h('i', {}, ` · ${c.unsure} maybe`) : null]));
  const list = c.files.slice(0, 14).map(f => `${stWord(f.st).padEnd(8)} ${f.p}${f.unsure ? '   (maybe)' : ''}`);
  el.uc.title = `${counted(n, 'uncommitted file')} this card changed`
    + (c.unsure ? `; ${c.unsure} maybe: another card was at work at the same time and neither said it changed ${c.unsure === 1 ? 'it' : 'them'}` : '')
    + `\n\n${list.join('\n')}${n > list.length ? `\n… ${n - list.length} more` : ''}\n\nclick: show them in the git tree`;
}
/* the card's files, unfolded in the git tree (which follows the selected card's folder) */
function showOwn(id) {
  select([id]);
  OWN.open = true; OWN.focus = id; OWN.focusUntil = Date.now() + 4000;
  if (!GT.open) setTreeOpen(true); else if (GT.data && GT.dir) renderTree();
}

/* ---- in the git tree */
/* the work tree the tree shows (none from a daemon that does not say) */
function treeTop() { const g = gitOf(GT.dir); return (g && g.top) || ''; }
function treeOwn() { const t = treeTop(); return (t && OWN.tops.get(t)) || null; }
const strayFiles = own => (own.files || []).filter(f => !(f.by || []).length);

/* the "N uncommitted" row on top of what is checked out: how many no card has, and (a click) the list */
function workRow(n, own) {
  const left = own ? strayFiles(own).length : 0;
  const tip = !own ? 'changed files no one has committed yet'
    : `${counted(n, 'changed file')} no one has committed yet${left ? `; ${left} of them no card on this board changed` : ', every one changed by a card on this board'}\nclick: ${OWN.open ? 'fold the list' : 'which card has which'}`;
  const row = h('div', { class: 'gr work' + (own ? ' own' : '') + (own && OWN.open ? ' open' : '') },
    h('div', { class: 'gline', title: tip }, h('span', { class: 'gs' }, `${n} uncommitted`),
      left ? h('span', { class: 'gleft' }, `${left} with no card`) : null,
      own ? h('span', { class: 'gfold' }, OWN.open ? '▴' : '▾') : null));
  if (!own) return row;
  row.addEventListener('click', e => { if (!e.target.closest('.gdet')) { OWN.open = !OWN.open; OWN.focus = null; renderRows(); } });
  if (OWN.open) row.append(workDetail(own));
  return row;
}
/* the files by card: what no card has first (with the agent that commits it), then each card's, then
   the ones several cards may share */
function workDetail(own) {
  const top = treeTop();
  const groups = new Map();
  for (const f of own.files || []) {
    const by = f.by || [], k = by.join('+');
    if (!groups.has(k)) groups.set(k, { by, files: [], unsure: false });
    const g = groups.get(k);
    g.files.push(f);
    if (f.unsure) g.unsure = true;
  }
  const order = [...S.nodes.keys()];
  const rank = g => !g.by.length ? -1 : g.by.length > 1 ? 1e6 + order.indexOf(g.by[0]) : order.indexOf(g.by[0]);
  const list = [...groups.values()].sort((a, b) => rank(a) - rank(b));
  const box = h('div', { class: 'gdet gwork' });
  const focus = OWN.focus && Date.now() < OWN.focusUntil ? OWN.focus : null;
  for (const g of list) {
    let head;
    if (!g.by.length) {
      head = h('div', { class: 'gwh left' }, h('b', {}, 'no card'), h('span', { class: 'gwn' }, String(g.files.length)),
        h('button', { class: 'gwgo', title: 'a fresh agent looks at each of these files, commits the ones that are still valid and needed (only these, by name) and names the rest for you; it deletes nothing', onclick: e => { e.stopPropagation(); commitStrays(top); } }, 'commit with an agent'));
    } else {
      head = h('div', { class: 'gwh' }, ...g.by.flatMap((id, i) => [i ? h('span', { class: 'gwp' }, '+') : null,
        h('button', { class: 'gwc', title: `${id}: show the card`, onclick: e => { e.stopPropagation(); select([id]); reveal(id); } }, ownName(id))]).filter(Boolean),
        h('span', { class: 'gwn' }, String(g.files.length)),
        g.unsure ? h('span', { class: 'gwm', title: 'they were at work at the same time and neither said it changed these: either may have' }, 'maybe') : null);
    }
    const files = g.files.map(f => h('button', { class: 'gwf', title: `${f.p}\n${stWord(f.st)}${f.m ? ', written ' + agoWords(f.m / 1000) : ''}\nclick: what is uncommitted in it`, onclick: e => { e.stopPropagation(); openDirt(top, f); } },
      h('b', { class: 'gst s' + (f.st === '?' ? 'n' : f.st) }, f.st === '?' ? 'N' : f.st), h('span', {}, '\u200e' + f.p + '\u200e'), f.m ? h('span', { class: 'ga', 'data-t': f.m / 1000 }, ago(f.m / 1000)) : null));
    const isFocus = focus && g.by.includes(focus);
    box.append(h('div', { class: 'gwg' + (isFocus ? ' focus' : '') + (g.by.length ? '' : ' left') }, head, ...files));
  }
  if ((own.n || 0) > (own.files || []).length) box.append(h('div', { class: 'gnote' }, `+${own.n - own.files.length} more files (git status lists them)`));
  if (!(own.files || []).length) box.append(h('div', { class: 'gnote' }, 'nothing uncommitted'));
  return box;
}
/* after the rows are drawn: the card asked about is in view */
function revealOwnFocus() {
  const g = treeEl.inner && treeEl.inner.querySelector('.gwg.focus');
  if (g) g.scrollIntoView({ block: 'nearest' });
}

/* a fresh agent commits what no card has */
async function commitStrays(top) {
  if (!top) return;
  const c = toWorld(innerWidth / 2, innerHeight / 2);
  const r = await req('strays', { top, x: c.x - NW / 2, y: c.y - NH / 2 });
  if (!r.ok) { toast('warn', /not understood/.test(r.error || '') ? 'this weft daemon is older than the page: restart it with `weft`' : r.error || 'no agent could be started'); return; }
  await whenNode(r.id);
  select([r.id]); reveal(r.id);
}

/* what is uncommitted in one file, full size, in the reader's window */
async function openDirt(top, f) {
  closeMenu(); closeFly();
  RD.id = null; dockOver(false);
  const b = (label, title, fn) => h('button', { class: 'btn sm', title, onclick: fn }, label);
  const who = (f.by || []).length ? f.by.map(ownName).join(' + ') + (f.unsure ? ' (maybe)' : '') : 'no card';
  const body = h('div', { class: 'rbody diff', tabindex: '0', 'aria-label': `What is uncommitted in ${f.p}` }, h('div', { class: 'note' }, 'loading…'));
  readerEl.replaceChildren(h('div', { class: 'win chamf' },
    h('div', { class: 'fh2' }, h('span', { class: 'nm' }, f.p),
      h('span', { class: 'stx' }, `uncommitted · ${stWord(f.st)} · ${who}`),
      h('span', { class: 'rz' }, b('A−', 'smaller text (-)', () => zoomReader(-1)), b('A+', 'larger text (+)', () => zoomReader(1))),
      b('back to canvas', 'Esc', closeReader)),
    body));
  readerEl._refs = { body };
  readerEl.style.setProperty('--rz', RD.z);
  readerEl.hidden = false;
  readerEl.onpointerdown = e => { if (e.target === readerEl) closeReader(); };
  const r = await req('dirt_diff', { top, path: f.p });
  if (readerEl.hidden || readerEl._refs?.body !== body) return;
  if (!r.ok) { body.replaceChildren(h('div', { class: 'note' }, r.error || 'it cannot be shown')); return; }
  body.replaceChildren(r.diff ? diffView(r.diff) : h('div', { class: 'note' }, r.note || 'no change'));
  setTimeout(() => body.focus(), 30);
}
