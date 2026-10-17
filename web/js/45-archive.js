/* ------------------------------------------------------------ the archive: finished work, filed */
/* Bottom left. Cards dragged onto it are filed: their terminals end, their results and links are
   kept. A flow files itself once it has been finished for a while (a lone card 30 min, a flow 60),
   unless a card of it is kept. A click unfolds what is there, newest first: read it, bring it back,
   or forget it. A daemon without an archive (an older build) shows none of this. */
ICON.archive = I('<path d="M2.4 3.2h11.2v3H2.4z"/><path d="M3.4 6.2v6.6h9.2V6.2"/><path d="M6.2 8.8h3.6"/>');
ICON.lock = I('<path d="M3.6 7.2h8.8v6.2H3.6z"/><path d="M5.4 7.2V5a2.6 2.6 0 0 1 5.2 0v2.2"/><path d="M8 9.6v1.6"/>');
ICON.unlock = I('<path d="M3.6 7.2h8.8v6.2H3.6z"/><path d="M5.4 7.2V5a2.6 2.6 0 0 1 5.1-.7"/>');
ICON.keep = I('<path d="M5.4 2.4h5.2M6.4 2.4v4.2L4.4 9h7.2l-2-2.4V2.4M8 9v4.8"/>');
const AR = { groups: null, open: false, q: '', unfold: null, full: new Map(), over: false, sure: null, leave: null };
const archEl = $('#arch');
const archOn = () => Array.isArray(AR.groups);
const mins = s => Math.round((s || 0) / 60);

function onArchive(m) {
  AR.groups = Array.isArray(m.groups) ? m.groups : null;
  for (const id of [...AR.full.keys()]) if (!archOn() || !AR.groups.some(g => g.id === id)) AR.full.delete(id);
  renderArch();
  for (const [id, n] of S.nodes) renderFiling(id, n, S.rt.get(id));
  const g = m.filed && archOn() && AR.groups.find(x => x.id === m.filed);
  if (!g) return;
  pulseArch();
  const what = groupLabel(g);
  if (g.auto) toast('info', `${what} ${g.nodes.length > 1 ? 'were' : 'was'} finished and went to the archive.`, { ms: 9000, action: { label: 'bring back', run: () => revive(g.id) } });
  else toast('ok', `Filed ${what}. ctrl+z brings ${g.nodes.length > 1 ? 'them' : 'it'} back.`, { ms: 4000 });
}
/* "fix login", "plan → build → review", "plan, build +3 more" */
function groupLabel(g) {
  const work = g.nodes.filter(n => n.agent !== 'note');
  const ns = (work.length ? work : g.nodes).map(n => `"${n.name}"`);
  if (ns.length <= 3) return ns.join(g.nodes.length > 1 && work.length === g.nodes.length ? ' → ' : ', ');
  return `${ns.slice(0, 2).join(', ')} +${ns.length - 2} more`;
}
function archAgo(ms) {
  const s = Math.max(0, (Date.now() - ms) / 1000);
  return s < 60 ? 'just now' : s < 3600 ? `${Math.floor(s / 60)} min ago` : s < 172800 ? `${Math.floor(s / 3600)} h ago` : `${Math.floor(s / 86400)} days ago`;
}
function pulseArch() {
  archEl.classList.remove('pulse'); void archEl.offsetWidth; archEl.classList.add('pulse');
}

/* ---- the box in the corner, and what unfolds above it */
function renderArch() {
  const on = archOn();
  archEl.hidden = !on;
  renderHold();
  document.body.classList.toggle('hasarch', on);
  if (!on) { AR.open = false; return; }
  const cards = AR.groups.reduce((s, g) => s + g.nodes.length, 0);
  if (!archEl.box) {
    archEl.box = h('button', { class: 'abox', title: 'the archive: drop finished cards here. click to see what is filed' },
      h('span', { class: 'ai', html: ICON.archive }), h('span', { class: 'al' }, 'archive'), h('b', { class: 'an' }));
    archEl.box.addEventListener('click', e => { e.stopPropagation(); AR.open ? closeArch() : openArch(); });
    archEl.panel = h('div', { class: 'apanel chamf', hidden: true });
    archEl.panel.addEventListener('pointerdown', e => e.stopPropagation());
    archEl.panel.addEventListener('wheel', e => e.stopPropagation());
    archEl.append(archEl.panel, archEl.box);
  }
  archEl.box.querySelector('.an').textContent = cards ? String(cards) : '';
  archEl.panel.hidden = !AR.open;
  archEl.classList.toggle('open', AR.open);
  if (AR.open) renderArchPanel();
}
function openArch() { closeMenu(); closeFly(); AR.open = true; AR.sure = null; renderArch(); const f = archEl.panel.querySelector('input'); if (f) f.focus(); }
function closeArch() { AR.open = false; AR.unfold = null; renderArch(); }
function renderArchPanel() {
  const p = archEl.panel;
  const keepQ = p.querySelector('input');
  const typing = keepQ && document.activeElement === keepQ;
  const q = AR.q.trim().toLowerCase();
  const hit = g => !q || g.nodes.some(n => [n.name, n.agent, n.asked, n.out].some(t => (t || '').toLowerCase().includes(q)));
  const groups = AR.groups.slice().reverse().filter(hit);
  const cards = AR.groups.reduce((s, g) => s + g.nodes.length, 0);
  const lone = mins(S.cfg.archive_s ?? 600), flow = mins(S.cfg.archive_flow_s ?? 900);
  const find = h('input', { class: 'aq', placeholder: 'find in the archive…', spellcheck: 'false', value: AR.q, 'aria-label': 'Find in the archive' });
  find.addEventListener('input', () => { AR.q = find.value; renderArchPanel(); });
  find.addEventListener('keydown', e => { e.stopPropagation(); if (e.key === 'Escape') closeArch(); });
  const by = [lone ? `a lone card ${lone} min after it is done` : '', flow ? `a whole flow ${flow} min after its last step` : ''].filter(Boolean);
  p.replaceChildren(...nn([
    h('div', { class: 'ah' }, h('b', {}, 'archive'), h('span', { class: 'sub' }, cards ? `${cards} card${cards > 1 ? 's' : ''} filed` : 'empty'),
      h('span', { class: 'sp' }), h('button', { class: 'ib', 'aria-label': 'Close', html: ICON.x, onclick: closeArch })),
    AR.groups.length > 4 ? find : null,
    h('div', { class: 'alist' }, groups.length ? groups.map(archRow)
      : h('div', { class: 'aempty' }, q ? 'nothing filed matches that.' : 'nothing filed yet. drag finished cards onto the archive.')),
    h('div', { class: 'afoot' }, by.length ? `files by itself: ${by.join(', ')} (counting while you work here). ` : 'nothing files itself. ', h('span', { class: 'kk', html: ICON.keep }), ' keep on a card holds it and its flow.')]));
  if (typing) { const f = p.querySelector('input'); if (f) { f.focus(); f.setSelectionRange(f.value.length, f.value.length); } }
}
function archRow(g) {
  const failed = g.nodes.some(n => n.ok === false);
  const lead = g.nodes.slice().reverse().find(n => n.agent !== 'note' && (n.out || '').trim()) || g.nodes.find(n => (n.out || n.asked || '').trim()) || g.nodes[0];
  const said = ((lead && (lead.out || lead.asked)) || '').trim().split('\n').find(l => l.trim()) || '';
  const open = AR.unfold === g.id;
  const sure = AR.sure === g.id;
  const back = h('button', { class: 'fb cy', title: 'back onto the canvas, where it was, with its results (a terminal starts fresh)' }, 'bring back');
  back.addEventListener('click', e => { e.stopPropagation(); revive(g.id); });
  const forget = h('button', { class: 'fb' + (sure ? ' no' : ''), title: 'forget it for good' }, sure ? 'forget for good?' : '×');
  forget.addEventListener('click', e => {
    e.stopPropagation();
    if (!sure) { AR.sure = g.id; renderArchPanel(); setTimeout(() => { if (AR.sure === g.id) { AR.sure = null; if (AR.open) renderArchPanel(); } }, 3500); return; }
    AR.sure = null; send('archive_del', { group: g.id });
  });
  const row = h('div', { class: 'arow' + (open ? ' open' : '') + (failed ? ' bad' : ''), 'data-group': g.id },
    h('div', { class: 'at' },
      h('span', { class: 'ak', html: ICON[kindIcon(lead)] || ICON.done }),
      h('span', { class: 'anm' }, groupLabel(g).replace(/"/g, '')),
      h('span', { class: 'aw' }, (g.auto ? 'by itself · ' : '') + archAgo(g.at)),
      back, forget),
    said ? h('div', { class: 'asay' }, said) : null,
    open ? archDetail(g) : null);
  row.addEventListener('click', () => { AR.unfold = open ? null : g.id; renderArchPanel(); if (!open) archFull(g.id); });
  return row;
}
const kindIcon = n => { const k = n ? kindOf(n) : 'agent'; return k === 'agent' ? (n && n.ok === false ? 'failed' : 'done') : k; };
/* unfolded: every card, what it was asked and what it said (in full once it is fetched) */
function archDetail(g) {
  const full = AR.full.get(g.id);
  return h('div', { class: 'adet' }, g.nodes.map(n => {
    const f = full && full.find(x => x.id === n.id);
    const asked = ((f ? f.asked || f.prompt : n.asked) || '').trim(), out = ((f ? f.out : n.out) || '').trim();
    return h('div', { class: 'acard' },
      h('div', { class: 'ach' }, h('span', { class: 'ak', html: ICON[kindIcon(n)] || ICON.done }), h('b', {}, n.name),
        h('span', { class: 'atag' }, n.agent === 'git' ? 'git · ' + (n.git_op || 'commit & push') : agentTag(n.agent))),
      asked && n.agent !== 'note' ? h('div', { class: 'aask' }, asked) : null,
      out ? h('pre', { class: 'aout' + (n.ok === false ? ' bad' : '') }, out) : h('div', { class: 'aask' }, 'no result'));
  }));
}
async function archFull(gid) {
  if (AR.full.has(gid)) return;
  const r = await req('archive_get', { group: gid });
  if (!r.ok) return;
  AR.full.set(gid, r.nodes || []);
  if (AR.open && AR.unfold === gid) renderArchPanel();
}
addEventListener('pointerdown', e => { if (AR.open && !archEl.contains(e.target)) closeArch(); }, true);
addEventListener('keydown', e => { if (e.key === 'Escape' && AR.open) { e.stopImmediatePropagation(); closeArch(); } }, true);

/* ---- filing: cards dragged onto the archive, or picked from a menu */
function archHit(x, y) {
  // while dragging the box is a quarter circle centred on the bottom left corner
  const r = (parseFloat(getComputedStyle(archEl).getPropertyValue('--qr')) || 255) + 16;
  return Math.hypot(x, innerHeight - y) <= r;
}
/* while cards move the archive shows itself as a place to drop them */
function archHover(ev, ids) {
  if (!archOn()) return;
  if (AR.leave) { clearTimeout(AR.leave); AR.leave = null; archEl.classList.remove('leave', 'back'); archEl.box.querySelector('.al').textContent = 'drop to file'; }
  archEl.classList.add('drag');
  const over = archHit(ev.clientX, ev.clientY);
  if (over === AR.over) return;
  AR.over = over;
  archEl.classList.toggle('over', over);
  for (const i of ids) { const el = nodeEls.get(i); if (el) el.root.classList.toggle('tofile', over); }
  archEl.box.querySelector('.al').textContent = over ? (ids.length > 1 ? `file ${ids.length} cards` : 'file it') : 'drop to file';
}
/* the cards were let go: true if that was on the archive (they are being filed) */
function archDrop(ids) {
  if (!archOn()) return false;
  const hit = AR.over;
  AR.over = false;
  archEl.classList.remove('over');
  // the quarter circle fades and shrinks away, then the button fades back in
  if (archEl.classList.contains('drag') && !AR.leave) {
    archEl.classList.add('leave');
    AR.leave = setTimeout(() => {
      AR.leave = null;
      archEl.classList.remove('drag', 'leave');
      archEl.box.querySelector('.al').textContent = 'archive';
      archEl.classList.remove('back'); void archEl.offsetWidth; archEl.classList.add('back');
      setTimeout(() => archEl.classList.remove('back'), 260);
    }, 750);
  }
  for (const i of ids) { const el = nodeEls.get(i); if (el) el.root.classList.remove('tofile'); }
  if (hit) fileAway(ids);
  return hit;
}
async function fileAway(ids) {
  ids = ids.filter(i => S.nodes.has(i));
  if (!ids.length || !archOn()) return;
  for (const i of ids) { const el = nodeEls.get(i); if (el) el.root.classList.add('filing'); }
  if (S.fly && ids.includes(S.fly.id)) closeFly();
  const r = await req('archive', { ids });
  // what is still at work stays where it is
  const stay = new Set(r.busy || []);
  for (const i of ids) { const el = nodeEls.get(i), n = S.nodes.get(i); if (el && (!r.ok || (n && stay.has(n.name)))) el.root.classList.remove('filing'); }
  if (stay.size) toast('info', `${[...stay].join(', ')} ${stay.size > 1 ? 'are' : 'is'} still at work and stays. Stop it or let it finish first.`, { ms: 6000 });
  else if (!r.ok) toast('err', r.error || 'could not file that');
  if (r.ok && r.group) S.undo.push(() => revive(r.group));
}
/* back onto the canvas: where it was, or as near as there is room */
async function revive(gid) {
  const g = archOn() && AR.groups.find(x => x.id === gid);
  if (!g) return toast('info', 'that is no longer in the archive');
  const off = freeOffset(g.nodes);
  const r = await req('revive', { group: gid, ...off });
  if (!r.ok) return toast('err', r.error || 'could not bring it back');
  closeArch();
  const ids = r.ids || [];
  if (!ids.length) return;
  await whenNode(ids[0]);
  select(ids.filter(i => S.nodes.has(i)));
  reveal(ids[0]);
}
/* how far to move a group of cards so it covers no other card */
function freeOffset(ns) {
  const pts = ns.filter(n => isFinite(n.x) && isFinite(n.y));
  if (!pts.length) return { dx: 0, dy: 0 };
  const x0 = Math.min(...pts.map(n => n.x)), y0 = Math.min(...pts.map(n => n.y));
  const w = Math.max(...pts.map(n => n.x + nw(n))) - x0, hh = Math.max(...pts.map(n => n.y + nh(n))) - y0;
  const boxes = [...S.nodes.values()].map(n => ({ x: n.x - JX - 20, y: n.y - 14, w: nw(n) + JX + 40, h: nh(n) + 28 + filmH(n.id) }));
  const clear = (dx, dy) => !boxes.some(b => x0 + dx - JX < b.x + b.w && x0 + dx + w > b.x && y0 + dy < b.y + b.h && y0 + dy + hh > b.y);
  if (clear(0, 0)) return { dx: 0, dy: 0 };
  for (let col = 0; col < 6; col++) for (let row = col ? 0 : 1; row < 8; row++) for (const sgn of [1, -1]) {
    const dx = col * (NW + 140), dy = sgn * row * (NH + 40);
    if (clear(dx, dy)) return { dx, dy };
  }
  const right = Math.max(...[...S.nodes.values()].map(n => n.x + nw(n))) + 150;
  return { dx: right - x0, dy: 0 };
}

/* ---- the lock in the top bar: archiving held, every countdown stands until it is let go */
const holdEl = $('#bHold');
function renderHold() {
  holdEl.hidden = !archOn();
  const on = !!S.hold;
  const key = on ? 'h' : 'r';
  if (holdEl._key === key) return;
  holdEl._key = key;
  holdEl.classList.toggle('held', on);
  document.body.classList.toggle('archheld', on);
  holdEl.setAttribute('aria-pressed', String(on));
  holdEl.innerHTML = (on ? ICON.lock : ICON.unlock) + `<span>${on ? 'archiving held' : 'archiving'}</span>`;
  holdEl.title = on ? 'archiving is held: no countdown runs and nothing goes to the archive by itself. click to let the countdowns run again'
    : 'finished cards go to the archive by themselves after a while. click to hold that: every countdown pauses';
}
holdEl.addEventListener('click', async () => {
  const on = !S.hold;
  S.hold = on; renderHold(); refreshFiling();
  const r = await req('archive_hold', { on });
  if (!r.ok) { S.hold = !on; renderHold(); refreshFiling(); toast('err', r.error || 'could not change that'); }
});

/* ---- on the card: when it files itself, and keep */
/* the clock runs while someone works on the canvas: stopped (S.idle: since when), what is left stays */
function filingLeft(at) {
  const s = (at - (S.idle || Date.now())) / 1000;
  if (s < 60) return 'soon';
  const m = Math.ceil(s / 60);
  return m < 60 ? `${m}m` : `${Math.floor(m / 60)}h${String(m % 60).padStart(2, '0')}`;
}
function renderFiling(id, n, r) {
  const el = nodeEls.get(id); if (!el || !n) return;
  const at = r && r.archive_at, kept = !!n.keep;
  if (!archOn() || !(at || kept)) { if (el.fil) el.fil.hidden = true; return; }
  if (!el.fil) {
    el.fil = h('button', { class: 'fil' });
    el.fil.addEventListener('click', e => { e.stopPropagation(); keepNode(id); });
    el.stx.after(el.fil);
  }
  el.fil.hidden = false;
  el.fil.classList.toggle('on', kept);
  const left = kept ? '' : filingLeft(at);
  const paused = !kept && (!!S.idle || !!S.hold);
  const key = kept ? 'k' : 'a' + left + (paused ? 'p' : '');
  if (el.fil._key === key) return;
  el.fil._key = key;
  el.fil.innerHTML = (kept ? ICON.keep : ICON.archive) + `<span>${kept ? 'kept' : left}</span>`;
  el.fil.classList.toggle('paused', paused);
  const flow = !kept && [...S.rt.values()].filter(x => x.archive_at === at).length > 1;
  el.fil.title = kept ? 'kept on the canvas: it does not go to the archive by itself, nor does its flow. click to let it go'
    : `finished: goes to the archive ${left === 'soon' ? 'any moment' : 'in ' + left}${flow ? ', with the rest of its flow' : ''}`
      + (S.hold && !kept ? ' once archiving is let go (it is held: the countdown pauses)' : paused ? ' of work here (the clock waits while nobody is at the canvas)' : '') + '. click to keep it on the canvas';
}
function refreshFiling() {
  if (!archOn()) return;
  for (const [id, n] of S.nodes) { const r = S.rt.get(id); if (r && r.archive_at) renderFiling(id, n, r); }
}
function keepNode(id) {
  const n = S.nodes.get(id); if (!n) return;
  patchNode(id, { keep: !n.keep }, 0);
}
setInterval(() => { if (!document.hidden) refreshFiling(); }, 15000);
/* someone is at the canvas: say so now and then, so the archive clock runs */
let hereAt = 0;
function here() {
  const t = Date.now();
  if (document.hidden || t - hereAt < 20000) return;
  hereAt = t; send('here');
}
for (const ev of ['pointerdown', 'pointermove', 'wheel', 'keydown']) addEventListener(ev, here, { capture: true, passive: true });
addEventListener('visibilitychange', () => { hereAt = 0; here(); });
/* menu entries: file it now, keep it */
function archItems(id) {
  const n = S.nodes.get(id);
  if (!archOn() || !n) return [];
  return [
    { label: 'archive', hint: 'drag onto ↙', run: () => fileAway([id]) },
    { k: n.keep ? '●' : ' ', label: 'keep on the canvas', hint: n.keep ? 'kept' : 'never filed by itself', run: () => keepNode(id) },
  ];
}
function archMulti(ids) {
  return archOn() ? [{ label: `archive ${ids.length}`, hint: 'drag onto ↙', run: () => fileAway(ids) }] : [];
}
