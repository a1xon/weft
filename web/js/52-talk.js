/* ------------------------------------------------------------ talks
   Agents that coordinate while they work. Drag the round port on top of an agent onto another
   agent: a line arches between their tops, and the two (with everyone else a talk joins them to)
   are told who the others are. They tell each other what they are working on (`weft tell`) and
   only they hear it; nothing fires along a talk. The field on the line says what they should
   mainly coordinate on (GPU use, the same files, ...). What they say flies along the line, shows
   over the one who said it for a moment, and stays in the line's log (its bubble). */
const talkEls = new Map();
const talkG = sv('g', { id: 'talkG' });
$('#edges').insertBefore(talkG, $('#edgeG'));
const talkRubber = sv('path', { class: 'trub' });
talkG.append(talkRubber);
const talkPills = h('div', { id: 'talkpills' });
$('#nodes').after(talkPills);
S.talks = new Map(); S.talkLog = [];
let talkOpen = null;   // the talk whose log is unfolded
const TALK_ICON = I('<path d="M2.4 3h7.2v5.2H5.6L3.4 10.4V8.2h-1z"/><path d="M11.6 6h2v5.2h-1v2.2l-2.2-2.2H7.6V10"/>');
/* (a daemon of an older build knows no talks: its state has no `talk`, and the page offers none) */
const canTalk = n => !!S.talkOn && !!n && kindOf(n) === 'agent' && n.agent !== 'step' && n.agent !== 'sh' && n.finish !== 'exit';
const talkTop = n => ({ x: n.x + nw(n) / 2, y: n.y });
const nameOf = id => { const n = S.nodes.get(id); return n ? n.name : id; };
/* who talks with whom: everyone the talks reach, through the others too */
function talkGroup(id) {
  const seen = [id];
  for (let i = 0; i < seen.length; i++)
    for (const t of S.talks.values()) {
      const o = t.a === seen[i] ? t.b : t.b === seen[i] ? t.a : null;
      if (o && !seen.includes(o) && S.nodes.has(o)) seen.push(o);
    }
  return seen;
}
/* the arch between two tops: over both cards, or round the side of the upper one when one card is
   under the other */
function talkCurve(p, q, wa, wb) {
  if (Math.abs(q.x - p.x) < (wa + wb) / 2 - 40) {
    const s = Math.max(wa, wb) * 0.8, up = (r, other) => ({ x: r.x + s, y: r.y - (r.y <= other.y ? 70 : 34) });
    return [p, up(p, q), up(q, p), q];
  }
  const top = Math.min(p.y, q.y) - Math.min(190, 46 + Math.hypot(q.x - p.x, q.y - p.y) * 0.18);
  return [p, { x: p.x, y: top }, { x: q.x, y: top }, q];
}
const curveD = ([p, c1, c2, q]) => `M${p.x} ${p.y}C${c1.x} ${c1.y} ${c2.x} ${c2.y} ${q.x} ${q.y}`;

function applyTalks(list) {
  const seen = new Set();
  for (const t of list || []) { seen.add(t.id); S.talks.set(t.id, t); }
  for (const id of [...S.talks.keys()]) if (!seen.has(id)) { S.talks.delete(id); dropTalk(id); }
  drawTalks();
}
function dropTalk(id) {
  const el = talkEls.get(id); if (!el) return;
  el.g.remove(); el.pill.remove(); talkEls.delete(id);
  if (talkOpen === id) talkOpen = null;
}
function drawTalks() { for (const t of S.talks.values()) drawTalk(t); }
function drawTalksOf(id) { for (const t of S.talks.values()) if (t.a === id || t.b === id) drawTalk(t); }
function drawTalk(t) {
  const a = S.nodes.get(t.a), b = S.nodes.get(t.b);
  if (!a || !b) return dropTalk(t.id);
  const el = talkEls.get(t.id) || mkTalk(t);
  const c = talkCurve(talkTop(a), talkTop(b), nw(a), nw(b)), d = curveD(c);
  el.vis.setAttribute('d', d); el.hit.setAttribute('d', d);
  const [p, c1, c2, q] = c;
  el.pill.style.left = (p.x + 3 * c1.x + 3 * c2.x + q.x) / 8 + 'px';
  el.pill.style.top = (p.y + 3 * c1.y + 3 * c2.y + q.y) / 8 + 'px';
  if (document.activeElement !== el.inp && el.inp.value !== (t.topic || '')) { el.inp.value = t.topic || ''; fitTopic(el.inp); }
  el.pill.classList.toggle('has', !!t.topic);
  const said = talkSaid(t).length;
  el.ct.textContent = said || '';
  el.ic.title = said ? `what they said to each other (${said})` : 'nothing said yet';
  el.log.hidden = talkOpen !== t.id;
  el.pill.classList.toggle('open', talkOpen === t.id);
  // (redrawn while a card moves: the log only when something was said)
  const key = talkOpen === t.id ? said + ':' + (S.talkLog.length ? S.talkLog[S.talkLog.length - 1].at : 0) : null;
  if (key && el.logKey !== key) renderTalkLog(t, el);
  el.logKey = key;
}
/* what the agents of a talk's group said */
function talkSaid(t) { const g = talkGroup(t.a); return S.talkLog.filter(m => g.includes(m.from)); }
const fitTopic = inp => { inp.size = Math.max(14, Math.min(40, inp.value.length + 1)); };
function mkTalk(t) {
  const id = t.id;
  const g = sv('g', { class: 'tk' }), vis = sv('path', { class: 'tv', id: 'tp-' + id }), hit = sv('path', { class: 'th' });
  g.append(hit, vis); talkG.insertBefore(g, talkRubber);
  const ct = h('b'), ic = h('button', { class: 'tk-ic', html: TALK_ICON }, ct);
  const inp = h('input', { class: 'tk-topic', placeholder: 'coordinate on…', spellcheck: 'false',
    'aria-label': 'what they should mainly coordinate on', title: 'what they should mainly coordinate on: GPU use, the same files, … (optional)' });
  const x = h('button', { class: 'tk-x', title: 'stop this talk', 'aria-label': 'stop this talk', html: ICON.x });
  const log = h('div', { class: 'tk-log', hidden: true });
  const pill = h('div', { class: 'tkpill', 'data-talk': id }, h('div', { class: 'tk-row' }, ic, inp, x), log);
  talkPills.append(pill);
  const el = { g, vis, hit, pill, ic, ct, inp, log };
  talkEls.set(id, el);
  fitTopic(inp);
  // the pill and the line are not the canvas: no panning, no new card, no canvas menu from them
  for (const tg of [pill, hit]) {
    tg.addEventListener('pointerdown', e => { e.stopPropagation(); closeMenu(); });
    tg.addEventListener('dblclick', e => e.stopPropagation());
    tg.addEventListener('contextmenu', e => { e.preventDefault(); e.stopPropagation(); talkMenu(id, e.clientX, e.clientY); });
  }
  hit.addEventListener('pointerenter', () => g.classList.add('hov'));
  hit.addEventListener('pointerleave', () => g.classList.remove('hov'));
  hit.addEventListener('click', e => { e.stopPropagation(); inp.focus(); });
  log.addEventListener('wheel', e => e.stopPropagation(), { passive: true });
  ic.addEventListener('click', e => { e.stopPropagation(); talkOpen = talkOpen === id ? null : id; drawTalks(); });
  x.addEventListener('click', e => { e.stopPropagation(); send('talk_del', { id }); });
  inp.addEventListener('input', () => fitTopic(inp));
  inp.addEventListener('keydown', e => {
    e.stopPropagation();
    if (e.key === 'Enter') { e.preventDefault(); inp.blur(); }
    if (e.key === 'Escape') { const cur = S.talks.get(id); inp.value = cur ? cur.topic || '' : ''; fitTopic(inp); inp.blur(); }
  });
  // while the topic is being written the agents wait for it; leaving the field lets them hear it
  inp.addEventListener('focus', () => { pill.classList.add('on'); send('talk_set', { id, typing: true }); });
  inp.addEventListener('blur', () => {
    pill.classList.remove('on');
    const cur = S.talks.get(id), v = inp.value.trim();
    if (cur) cur.topic = v;
    send('talk_set', { id, topic: v });
  });
  return el;
}
function renderTalkLog(t, el) {
  const said = talkSaid(t).slice(-40);
  el.log.replaceChildren(...(said.length ? said.map(m => h('div', { class: 'tm' },
    h('span', { class: 'tm-w' }, `${nameOf(m.from)} → ${m.to.map(nameOf).join(', ')}`),
    h('span', { class: 'tm-a' }, age(m.at) + ' ago'),
    h('span', { class: 'tm-t' }, m.text)))
    : [h('div', { class: 'tm-none' }, 'nothing said yet. They tell each other what they work on as soon as they know it.')]));
  el.log.scrollTop = el.log.scrollHeight;
}
function talkMenu(id, x, y) {
  const t = S.talks.get(id); if (!t) return;
  popup([
    { cap: `${nameOf(t.a)} ↔ ${nameOf(t.b)}` },
    { label: 'what they should coordinate on…', run: () => { const el = talkEls.get(id); if (el) el.inp.focus(); } },
    { label: talkOpen === id ? 'hide what they said' : 'what they said', run: () => { talkOpen = talkOpen === id ? null : id; drawTalks(); } },
    { sep: true },
    { label: 'stop this talk', danger: true, run: () => send('talk_del', { id }) },
  ], x, y);
}
/* several agents selected: they all talk, each joined to its neighbour from left to right unless
   the talks already join them */
function talkMulti(ids) {
  const ags = ids.map(i => S.nodes.get(i)).filter(canTalk).sort((a, b) => a.x - b.x || a.y - b.y);
  if (ags.length < 2) return [];
  return [{ label: 'let them talk', hint: 'they coordinate directly', run: () => {
    const joined = new Set(talkGroup(ags[0].id));
    for (let i = 1; i < ags.length; i++) {
      const g = talkGroup(ags[i].id);
      if (!g.some(id => joined.has(id))) send('talk_add', { a: ags[i - 1].id, b: ags[i].id });
      for (const id of g) joined.add(id);
    }
    tipTalk();
  } }];
}
function tipTalk() {
  tip('talk', 'They are told who the other is and tell each other what they work on; only they hear it. Type on the line what they should mainly coordinate on.', 10000);
}

/* drag from the port on top of a card onto another agent */
function startTalk(e, fromId) {
  if (e.button !== 0) return;
  e.preventDefault(); e.stopPropagation(); closeMenu(); closeFly();
  const n = S.nodes.get(fromId); if (!n) return;
  const p0 = talkTop(n), sx = e.clientX, sy = e.clientY;
  let moved = false, over = null;
  const cardAt = ev => { const t = document.elementFromPoint(ev.clientX, ev.clientY); const c = t && t.closest && t.closest('.nw'); return c ? c.dataset.id : null; };
  const mark = (id, on) => { const el = id && nodeEls.get(id); if (el) el.root.classList.toggle(canTalk(S.nodes.get(id)) ? 'talkdrop' : 'nodrop', on); };
  const mv = ev => {
    if (!moved && Math.hypot(ev.clientX - sx, ev.clientY - sy) < 5) return;
    if (!moved) { moved = true; talkRubber.style.display = 'block'; $('#app').classList.add('talking'); }
    let t = cardAt(ev); if (t === fromId) t = null;
    if (t !== over) { mark(over, false); over = t; mark(over, true); }
    const tn = over && S.nodes.get(over);
    const q = tn && canTalk(tn) ? talkTop(tn) : toWorld(ev.clientX, ev.clientY);
    talkRubber.setAttribute('d', curveD(talkCurve(p0, q, nw(n), tn ? nw(tn) : NW)));
  };
  const up = ev => {
    removeEventListener('pointermove', mv);
    talkRubber.style.display = 'none'; $('#app').classList.remove('talking'); mark(over, false);
    if (!moved) return talkClick(fromId);
    const t = cardAt(ev);
    if (!t || t === fromId) return;
    if (!canTalk(S.nodes.get(t))) return toast('warn', 'Only agents can talk to each other: not a command, a queued prompt, a check or a git step.');
    req('talk_add', { a: fromId, b: t, typing: true }).then(r => {
      if (!r.ok) return toast('err', r.error || 'they cannot talk');
      tipTalk();
      // the topic is optional, but this is the moment to give one
      const focus = (k = 0) => { const el = talkEls.get(r.id); if (el) el.inp.focus(); else if (k < 40) requestAnimationFrame(() => focus(k + 1)); };
      focus();
    });
  };
  addEventListener('pointermove', mv);
  addEventListener('pointerup', up, { once: true });
}
/* a plain click on the port: the log of its talks, or how to start one */
function talkClick(id) {
  const t = [...S.talks.values()].find(x => x.a === id || x.b === id);
  if (!t) return toast('info', 'Drag this onto another agent: the two coordinate while they work, and only they hear what they say.', { ms: 7000 });
  talkOpen = talkOpen === t.id ? null : t.id; drawTalks();
}

/* an agent said something: it flies along the line and shows over the one who said it */
function onTalk(m) {
  S.talkLog.push(m);
  if (S.talkLog.length > 600) S.talkLog.splice(0, S.talkLog.length - 600);
  for (const to of m.to) {
    const t = [...S.talks.values()].find(x => (x.a === m.from && x.b === to) || (x.b === m.from && x.a === to));
    if (t) talkBead(t, t.a === m.from);
  }
  talkSay(m);
  drawTalks();
}
function talkBead(t, forward) {
  const el = talkEls.get(t.id); if (!el) return;
  el.g.classList.add('live'); clearTimeout(el.liveT); el.liveT = setTimeout(() => el.g.classList.remove('live'), 1400);
  if (matchMedia('(prefers-reduced-motion: reduce)').matches) return;
  const c = sv('circle', { class: 'tbead', r: 4 });
  const am = sv('animateMotion', { dur: '0.9s', fill: 'freeze', calcMode: 'spline', keySplines: '.3 0 .2 1', keyTimes: '0;1', keyPoints: forward ? '0;1' : '1;0' });
  const mp = sv('mpath'); mp.setAttribute('href', '#tp-' + t.id);
  am.append(mp); c.append(am); beadG.append(c);
  try { am.beginElement(); } catch {}
  setTimeout(() => c.remove(), 1100);
}
const talkBubbles = new Map();
function talkSay(m) {
  const n = S.nodes.get(m.from); if (!n) return;
  const old = talkBubbles.get(m.from); if (old) old.remove();
  const text = m.text.length > 160 ? m.text.slice(0, 159) + '…' : m.text;
  const p = talkTop(n);
  const b = h('div', { class: 'tsay', style: `left:${p.x}px;top:${p.y}px` }, h('b', {}, '→ ' + m.to.map(nameOf).join(', ')), ' ', text);
  talkPills.append(b); talkBubbles.set(m.from, b);
  setTimeout(() => { b.remove(); if (talkBubbles.get(m.from) === b) talkBubbles.delete(m.from); }, 7000);
}
