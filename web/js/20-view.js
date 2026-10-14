/* ------------------------------------------------------------ view */
const world = $('#world'), canvas = $('#canvas');
let saveT = 0;
function applyView() {
  const v = S.v;
  world.style.transform = `translate(${v.x}px,${v.y}px) scale(${v.k})`;
  $('#pat').setAttribute('patternTransform', `translate(${v.x} ${v.y}) scale(${v.k})`);
  $('#app').classList.toggle('lod', v.k < 0.5);
  $('#zPct').textContent = Math.round(v.k * 100) + '%';
  placeEdgeBar(); placeFly();
  clearTimeout(saveT);
  saveT = setTimeout(() => { try { localStorage.setItem('weft-view:' + S.project.dir, JSON.stringify(S.v)); } catch {} }, 400);
}
const toWorld = (cx, cy) => ({ x: (cx - S.v.x) / S.v.k, y: (cy - S.v.y) / S.v.k });
function zoomAt(cx, cy, f) {
  const k = Math.min(2, Math.max(0.15, S.v.k * f)), r = k / S.v.k;
  S.v.x = cx - (cx - S.v.x) * r; S.v.y = cy - (cy - S.v.y) * r; S.v.k = k;
  applyView();
}
function fit() {
  const ns = [...S.nodes.values()];
  if (!ns.length) { S.v = { x: 90, y: 110, k: 1 }; return applyView(); }
  const x0 = Math.min(...ns.map(n => n.x)) - JX, y0 = Math.min(...ns.map(n => n.y));
  const x1 = Math.max(...ns.map(n => n.x + nw(n))), y1 = Math.max(...ns.map(n => n.y + nh(n)));
  const W = innerWidth, Hh = innerHeight - 200;
  const k = Math.min(1.1, Math.max(0.15, Math.min((W - 120) / (x1 - x0), (Hh - 40) / (y1 - y0))));
  S.v = { k, x: (W - (x1 - x0) * k) / 2 - x0 * k, y: 70 + (Hh - (y1 - y0) * k) / 2 - y0 * k };
  applyView();
}
/* pan just enough that a node is fully visible between the bars */
function reveal(id) {
  const n = S.nodes.get(id); if (!n) return;
  const k = S.v.k, x0 = S.v.x + n.x * k, y0 = S.v.y + n.y * k, x1 = x0 + nw(n) * k, y1 = y0 + nh(n) * k;
  const top = 60, bottom = innerHeight - 110, left = 24 + JX * k, right = innerWidth - 24;
  let dx = 0, dy = 0;
  if (x0 < left) dx = left - x0; else if (x1 > right) dx = right - x1;
  if (y0 < top) dy = top - y0; else if (y1 > bottom) dy = bottom - y1;
  if (dx || dy) { S.v.x += dx; S.v.y += dy; applyView(); }
}
function centerOn(id) {
  const n = S.nodes.get(id); if (!n) return;
  S.v.x = innerWidth / 2 - (n.x + nw(n) / 2) * S.v.k; S.v.y = (innerHeight - 160) / 2 - (n.y + nh(n) / 2) * S.v.k + 40;
  applyView();
}
/* layered left-to-right layout: depth = longest path from a start agent */
function tidy() {
  const ns = [...S.nodes.values()]; if (!ns.length) return;
  const out = new Map(ns.map(n => [n.id, []]));
  for (const e of S.edges.values()) if (out.has(e.from) && out.has(e.to)) out.get(e.from).push(e.to);
  const depth = new Map(), visiting = new Set();
  const dfs = (id, d) => {
    if (visiting.has(id) || (depth.get(id) ?? -1) >= d) return;
    depth.set(id, d); visiting.add(id);
    for (const t of out.get(id)) dfs(t, d + 1);
    visiting.delete(id);
  };
  const hasIn = new Set([...S.edges.values()].map(e => e.to));
  for (const n of ns) if (!hasIn.has(n.id)) dfs(n.id, 0);
  for (const n of ns) if (!depth.has(n.id)) dfs(n.id, 0);
  const cols = new Map();
  for (const n of ns) { const d = depth.get(n.id) || 0; if (!cols.has(d)) cols.set(d, []); cols.get(d).push(n); }
  const x0 = Math.min(...ns.map(n => n.x)), y0 = Math.min(...ns.map(n => n.y));
  const moves = [];
  let x = x0; // a column is as wide as its widest card
  for (const [, list] of [...cols.entries()].sort((a, b) => a[0] - b[0])) {
    list.sort((a, b) => a.y - b.y);
    let y = y0;
    list.forEach(n => { moves.push({ id: n.id, x, y }); y += nh(n) + 40 + filmH(n.id); });
    x += Math.max(...list.map(nw)) + 150;
  }
  for (const m of moves) { const n = S.nodes.get(m.id); n.x = m.x; n.y = m.y; placeNode(n); }
  send('node_move', { moves });
  drawAllEdges();
  setTimeout(fit, 30);
}

canvas.addEventListener('wheel', e => {
  e.preventDefault();
  const mouse = e.deltaMode === 1 || (e.deltaX === 0 && Number.isInteger(e.deltaY) && Math.abs(e.deltaY) >= 50);
  if (e.ctrlKey || e.metaKey || mouse) zoomAt(e.clientX, e.clientY, Math.exp(-e.deltaY * (e.ctrlKey ? 0.012 : 0.0016)));
  else { S.v.x -= e.deltaX; S.v.y -= e.deltaY; applyView(); }
}, { passive: false });

canvas.addEventListener('pointerdown', e => {
  if (e.target.closest('.nw, .ed, .jn, #edgebar')) return;
  if (e.button !== 0 && e.button !== 1) return;
  closeMenu(); closeFly();
  if (e.shiftKey && e.button === 0) return marquee(e);
  select([]);
  const sx = e.clientX, sy = e.clientY, ox = S.v.x, oy = S.v.y;
  canvas.classList.add('panning');
  const mv = ev => { S.v.x = ox + ev.clientX - sx; S.v.y = oy + ev.clientY - sy; applyView(); };
  const up = () => { canvas.classList.remove('panning'); removeEventListener('pointermove', mv); };
  addEventListener('pointermove', mv);
  addEventListener('pointerup', up, { once: true });
});
canvas.addEventListener('dblclick', e => {
  if (e.target.closest('.nw, .ed, .jn')) return;
  const w = toWorld(e.clientX, e.clientY);
  spawnHere(S.lastAgent, w.x - NW / 2, w.y - 30);
});
canvas.addEventListener('contextmenu', e => {
  if (e.target.closest('.nw, .ed, .jn')) return;
  e.preventDefault();
  canvasMenu(e.clientX, e.clientY);
});
function marquee(e) {
  const box = h('div', { id: 'marquee' }); document.body.append(box);
  const sx = e.clientX, sy = e.clientY;
  const mv = ev => {
    const x = Math.min(sx, ev.clientX), y = Math.min(sy, ev.clientY);
    Object.assign(box.style, { left: x + 'px', top: y + 'px', width: Math.abs(ev.clientX - sx) + 'px', height: Math.abs(ev.clientY - sy) + 'px' });
  };
  const up = ev => {
    removeEventListener('pointermove', mv); box.remove();
    const a = toWorld(Math.min(sx, ev.clientX), Math.min(sy, ev.clientY)), b = toWorld(Math.max(sx, ev.clientX), Math.max(sy, ev.clientY));
    select([...S.nodes.values()].filter(n => n.x < b.x && n.x + nw(n) > a.x && n.y < b.y && n.y + nh(n) > a.y).map(n => n.id));
  };
  addEventListener('pointermove', mv);
  addEventListener('pointerup', up, { once: true });
}
