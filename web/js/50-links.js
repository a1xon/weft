/* ------------------------------------------------------------ runtime messages (coalesced per frame) */
const dirtyRt = new Set(); let rtFrame = 0;
function queueRt(m) {
  S.rt.set(m.id, m);
  dirtyRt.add(m.id);
  if (!rtFrame) rtFrame = requestAnimationFrame(() => {
    rtFrame = 0;
    for (const id of dirtyRt) renderRt(id);
    dirtyRt.clear();
    renderTicker();
  });
}
const prevStatus = new Map();
function renderRt(id) {
  const r = S.rt.get(id), n = S.nodes.get(id);
  if (r && n) {
    const was = prevStatus.get(id); prevStatus.set(id, r.status);
    if (was && was !== r.status && hasPing(n) && ['done', 'failed', 'attention', 'blocked'].includes(r.status) && !['done', 'failed'].includes(was)) notifyPing(n, r);
    if (r.status === 'attention' && was !== 'attention') {
      const gate = isGate(kindOf(n)), q = (r.issue || '').trim().split('\n')[0];
      toast('warn', gate ? `${n.name} is waiting for you.` : q ? `${n.name} asks you: ${q.length > 90 ? q.slice(0, 89) + '\u2026' : q}` : `${n.name} waits at a dialog.`,
        { ms: 12000, action: gate ? { label: 'review', run: () => { select([id]); centerOn(id); } } : { label: q ? 'answer' : 'show', run: () => showPlea(id) } });
    }
  }
  renderNode(id);
  renderEtaTop();
  if (SB.id === id) renderShotbox();
  if (r && n && n.takeover) drawHands();
  for (const e of S.edges.values()) if (e.from === id || e.to === id) styleEdge(e);
  updateJunction(id);
  for (const e of outgoing(id)) updateJunction(e.to);
  if (S.sel.has(id)) { renderOmni(); renderQueue(); }
  if (S.fly && S.fly.id === id) updateFlyLive();
  if (RD.id === id) updateReaderLive();
  if (S.focus === id) updateFocusHead();
}
function renderTicker() {
  const c = { busy: 0, starting: 0, attention: 0, waiting: 0, done: 0, failed: 0, blocked: 0 };
  for (const [id, r] of S.rt) { const n = S.nodes.get(id); if (kindOf(n) === 'note') continue; if (r.status in c) c[r.status]++; }
  const work = c.busy + c.starting;
  $('#tick').replaceChildren(...nn([
    work ? h('span', { class: 'tk am' }, h('b', {}, work), ' working') : null,
    c.waiting ? h('span', { class: 'tk' }, h('b', {}, c.waiting), ' waiting') : null,
    c.attention ? h('span', { class: 'tk vi', title: 'jump to what needs you', onclick: () => {
      const id = [...S.rt.entries()].find(([, r]) => r.status === 'attention')?.[0];
      if (id) showPlea(id); } }, h('b', {}, c.attention), ' needs you') : null,
    c.blocked ? h('span', { class: 'tk rd' }, h('b', {}, c.blocked), ' blocked') : null,
    c.failed ? failedChip(c.failed) : null,
    readyChip(),
    c.done ? h('span', { class: 'tk gr' }, h('b', {}, c.done), ' done') : null,
  ]));
}
setInterval(() => {
  if (document.hidden) return;
  for (const [id, n] of S.nodes) {
    const r = S.rt.get(id);
    if (r && ['busy', 'done', 'failed'].includes(r.status)) { const el = nodeEls.get(id); if (el) el.stx.textContent = statusText(n, r); }
  }
  if (S.focus) updateFocusHead();
}, 1000);

/* ------------------------------------------------------------ flow reconcile */
function applyFlow(nodes, edges) {
  const seen = new Set();
  for (const n of nodes) { seen.add(n.id); S.nodes.set(n.id, n); placeNode(n); renderNode(n.id); }
  for (const id of [...S.nodes.keys()]) if (!seen.has(id)) {
    S.nodes.delete(id);
    const el = nodeEls.get(id); if (el) el.root.remove();
    nodeEls.delete(id); S.rt.delete(id); S.sel.delete(id);
    if (S.fly && S.fly.id === id) closeFly();
    if (S.focus === id) closeFocus();
    if (RD.id === id) closeReader();
  }
  const seenE = new Set();
  for (const e of edges) { seenE.add(e.id); S.edges.set(e.id, e); }
  for (const id of [...S.edges.keys()]) if (!seenE.has(id)) {
    S.edges.delete(id);
    const el = edgeEls.get(id); if (el) el.g.remove();
    edgeEls.delete(id);
    if (S.selEdge === id) { S.selEdge = null; buildEdgeBar(); }
  }
  drawAllEdges();
  folderSpread();   // (whether cards say which folder they work in depends on all of them)
  for (const id of S.nodes.keys()) renderNode(id);
  $('#empty').hidden = S.nodes.size > 0;
  renderOmni(); renderQueue();
  if (pad.open) renderPad();
  if (S.fly && !insideFly()) renderFly();
}
function whenNode(id) {
  return new Promise(res => {
    let n = 0;
    const t = () => { if (nodeEls.has(id) || ++n > 40) res(); else requestAnimationFrame(t); };
    t();
  });
}

/* ------------------------------------------------------------ links and junctions */
const edgeG = $('#edgeG'), juncG = $('#juncG'), beadG = $('#beadG'), handG = $('#handG');
const handEls = new Map();
/* a fresh agent that took over from another: a dotted baton line, not a link (nothing fires along it) */
function drawHands() {
  const want = new Set();
  for (const n of S.nodes.values()) {
    // a hand-over's baton, or a twin's thread back to the agent whose conversation it copied
    const a = (n.takeover && S.nodes.get(n.takeover)) || (n.twin && S.nodes.get(n.twin));
    if (!a) continue;
    want.add(n.id);
    let el = handEls.get(n.id);
    if (!el) { el = sv('path', { class: 'hand' }); handG.append(el); handEls.set(n.id, el); }
    el.setAttribute('d', pathBetween(a.x + nw(a), a.y + PY, n.x, n.y + PY));
    const r = S.rt.get(n.id);
    el.classList.toggle('wait', !!(r && r.hand_wait));
    el.classList.toggle('twin', !n.takeover && !!n.twin);
  }
  for (const [id, el] of handEls) if (!want.has(id)) { el.remove(); handEls.delete(id); }
}
function targetPoint(e) {
  const t = S.nodes.get(e.to);
  if (incoming(e.to).length >= 2) return { x: t.x - JX - 15, y: t.y + PY, junction: true };
  return { x: t.x, y: t.y + PY, junction: false };
}
function pathBetween(x1, y1, x2, y2) {
  const dx = Math.max(54, Math.abs(x2 - x1) * 0.5);
  return `M${x1} ${y1}C${x1 + dx} ${y1} ${x2 - dx} ${y2} ${x2} ${y2}`;
}
const NEXT_ON = { done: 'ok', ok: 'fail', fail: 'done' };
function drawEdge(e) {
  const a = S.nodes.get(e.from), b = S.nodes.get(e.to);
  if (!a || !b) return;
  let el = edgeEls.get(e.id);
  if (!el) {
    const g = sv('g', { class: 'ed' }), vis = sv('path', { class: 'vis', id: 'p-' + e.id }), hit = sv('path', { class: 'hit' });
    g.dataset.edge = e.id;
    const chip = sv('g', { class: 'chip' }), rect = sv('rect', { x: -9, y: -9, width: 18, height: 18 }), tx = sv('text');
    const title = sv('title');
    chip.append(title, rect, tx); g.append(hit, vis, chip);
    hit.addEventListener('pointerdown', ev => { ev.stopPropagation(); closeMenu(); closeFly(); selectEdge(e.id); });
    hit.addEventListener('dblclick', ev => { ev.stopPropagation(); spliceInto(e.id, 'agent'); });
    hit.addEventListener('contextmenu', ev => { ev.preventDefault(); ev.stopPropagation(); selectEdge(e.id); edgeMenu(e.id, ev.clientX, ev.clientY); });
    // the chip is a switch: click it to cycle always / only if ok / only if it fails
    chip.addEventListener('pointerdown', ev => ev.stopPropagation());
    chip.addEventListener('click', ev => { ev.stopPropagation(); const cur = S.edges.get(e.id); if (cur) send('edge_set', { id: e.id, on: NEXT_ON[cur.on] || 'done' }); });
    chip.addEventListener('contextmenu', ev => { ev.preventDefault(); ev.stopPropagation(); selectEdge(e.id); edgeMenu(e.id, ev.clientX, ev.clientY); });
    const rest = sv('circle', { class: 'rest', r: 4 }); rest.style.display = 'none'; g.append(rest);
    edgeG.append(g);
    el = { g, vis, hit, chip, tx, rest, title };
    edgeEls.set(e.id, el);
  }
  const p = targetPoint(e);
  const x1 = a.x + nw(a), y1 = a.y + PY;
  const d = pathBetween(x1, y1, p.x, p.y);
  el.vis.setAttribute('d', d); el.hit.setAttribute('d', d);
  const dx = Math.max(54, Math.abs(p.x - x1) * 0.5);
  const mx = (x1 + 3 * (x1 + dx) + 3 * (p.x - dx) + p.x) / 8, my = (y1 + p.y) / 2;
  el.chip.setAttribute('transform', `translate(${mx} ${my})`);
  el.mid = { x: mx, y: my };
  el.rest.setAttribute('cx', p.x - 12); el.rest.setAttribute('cy', p.y);
}
function styleEdge(e) {
  const el = edgeEls.get(e.id); if (!el) return;
  const to = S.rt.get(e.to), from = S.rt.get(e.from);
  const arrived = !!(to && to.arrived.includes(e.id));
  el.g.dataset.k = e.on;
  el.g.classList.toggle('arr', arrived);
  el.g.classList.toggle('run', !!(from && from.status === 'busy') && !arrived);
  el.g.classList.toggle('sel', S.selEdge === e.id);
  el.tx.textContent = e.on === 'ok' ? '✓' : e.on === 'fail' ? '✕' : '›';
  el.title.textContent = { done: 'fires when the source finishes (click to change)', ok: 'fires only if the source succeeds (click to change)', fail: 'fires only if the source fails (click to change)' }[e.on];
}
function drawAllEdges() {
  drawHands();
  drawTalks();
  for (const e of S.edges.values()) drawEdge(e);
  for (const id of S.nodes.keys()) updateJunction(id);
  for (const e of S.edges.values()) styleEdge(e);
}
function redrawEdgesOf(id) {
  drawHands();
  drawTalksOf(id);
  for (const e of S.edges.values()) if (e.from === id || e.to === id) drawEdge(e);
  updateJunction(id);
  for (const e of outgoing(id)) updateJunction(e.to);
}
function arc(cx, cy, r, a0, a1) {
  const p = a => `${cx + r * Math.cos(a)} ${cy + r * Math.sin(a)}`;
  return `M${p(a0)}A${r} ${r} 0 ${a1 - a0 > Math.PI ? 1 : 0} 1 ${p(a1)}`;
}
function updateJunction(id) {
  const n = S.nodes.get(id); if (!n) return;
  const inc = incoming(id);
  let el = juncEls.get(id);
  if (inc.length < 2) { if (el) { el.link.remove(); el.g.remove(); juncEls.delete(id); } return; }
  if (!el) {
    const g = sv('g', { class: 'jn' });
    el = { g, segs: [], key: '' };
    g.append(sv('circle', { class: 'base', r: 15 }), sv('circle', { r: 22, fill: 'transparent' }));
    el.link = sv('path', { class: 'jl' });
    el.gl = sv('text', { class: 'gl' }); el.ct = sv('text', { class: 'ct', y: 29 });
    g.append(el.gl, el.ct);
    const open = ev => {
      ev.preventDefault(); ev.stopPropagation(); closeMenu();
      const cur = S.nodes.get(id);
      popup([
        { cap: `${cur.name}: start when…` },
        { k: cur.join !== 'any' ? '●' : ' ', label: 'all inputs have arrived', hint: 'Promise.all', run: () => send('node_set', { id, patch: { join: 'all' } }) },
        { k: cur.join === 'any' ? '●' : ' ', label: 'the first input arrives', hint: 'Promise.any', run: () => send('node_set', { id, patch: { join: 'any' } }) },
      ], ev.clientX, ev.clientY);
    };
    g.addEventListener('pointerdown', ev => { if (ev.button === 0) open(ev); });
    g.addEventListener('contextmenu', open);
    juncG.append(el.link, g);
    juncEls.set(id, el);
  }
  const cx = n.x - JX, cy = n.y + PY;
  el.g.setAttribute('transform', `translate(${cx} ${cy})`);
  el.link.setAttribute('d', `M${cx + 15} ${cy}H${n.x}`);
  const r = S.rt.get(id), any = n.join === 'any', k = inc.length;
  const states = inc.map(e => {
    if (r && r.arrived.includes(e.id)) return 'arr';
    const s = S.rt.get(e.from);
    if (s && (s.status === 'blocked' || ((s.status === 'done' || s.status === 'failed') && !(e.on === 'done' || (e.on === 'ok' && s.ok !== false) || (e.on === 'fail' && s.ok === false))))) return 'bad';
    if (s && (s.status === 'busy' || s.status === 'starting')) return 'run';
    return '';
  });
  const key = k + '|' + states.join() + '|' + any + '|' + (r && r.status === 'blocked');
  if (el.key !== key) {
    el.key = key;
    for (const s of el.segs) s.remove();
    el.segs = [];
    const gap = 0.34;
    for (let i = 0; i < k; i++) {
      const a0 = -Math.PI / 2 + i * 2 * Math.PI / k + gap / 2, a1 = -Math.PI / 2 + (i + 1) * 2 * Math.PI / k - gap / 2;
      const p = sv('path', { class: 'seg ' + states[i], d: arc(0, 0, 15, a0, a1) });
      el.g.insertBefore(p, el.gl); el.segs.push(p);
    }
    el.gl.textContent = any ? '≥1' : '&';
    const have = states.filter(s => s === 'arr').length;
    el.ct.textContent = any ? 'any' : `${have}/${k}`;
    const full = any ? have > 0 : have === k;
    el.g.classList.toggle('full', full);
    el.g.classList.toggle('blk', !!(r && r.status === 'blocked'));
    el.link.classList.toggle('on', full);
  }
}
function bead(edgeId) {
  const el = edgeEls.get(edgeId); if (!el) return;
  if (matchMedia('(prefers-reduced-motion: reduce)').matches) return;
  const c = sv('circle', { class: 'bead', r: 4.5 });
  const am = sv('animateMotion', { dur: '0.85s', fill: 'freeze', calcMode: 'spline', keySplines: '.3 0 .2 1', keyTimes: '0;1' });
  const mp = sv('mpath'); mp.setAttribute('href', '#p-' + edgeId);
  am.append(mp); c.append(am); beadG.append(c);
  try { am.beginElement(); } catch {}
  setTimeout(() => c.remove(), 1100);
}
