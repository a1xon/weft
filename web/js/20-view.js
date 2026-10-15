/* ------------------------------------------------------------ view */
const world = $('#world'), canvas = $('#canvas');
let saveT = 0;
function applyView() {
  const v = S.v;
  world.style.transform = `translate(${v.x}px,${v.y}px) scale(${v.k})`;
  $('#pat').setAttribute('patternTransform', `translate(${v.x} ${v.y}) scale(${v.k})`);
  $('#app').classList.toggle('lod', v.k < 0.5);
  $('#zPct').textContent = Math.round(v.k * 100) + '%';
  placeEdgeBar(); placeFly(); placeNetBox();
  clearTimeout(saveT);
  saveT = setTimeout(() => { try { localStorage.setItem('weft-view:' + S.project.dir, JSON.stringify(S.v)); } catch {} }, 400);
}
const toWorld = (cx, cy) => ({ x: (cx - S.v.x) / S.v.k, y: (cy - S.v.y) / S.v.k });
function zoomAt(cx, cy, f) {
  const k = Math.min(2, Math.max(0.15, S.v.k * f)), r = k / S.v.k;
  S.v.x = cx - (cx - S.v.x) * r; S.v.y = cy - (cy - S.v.y) * r; S.v.k = k;
  applyView();
}
/* where the bar at the bottom starts (it is taller with the model row and the addenda): the canvas
   above it is what fit / reveal / centre work with */
function barTop() {
  const d = $('#dock'), r = d && d.getBoundingClientRect();
  return r && r.height ? Math.min(r.top, innerHeight - 70) : innerHeight - 117;
}
function fit() { S.v = fitView(); applyView(); }
/* the view that shows every card; `at` (Map id -> {x, y}) is where cards are about to be */
function fitView(at) {
  const ns = [...S.nodes.values()].map(n => (at && at.has(n.id) ? { ...n, ...at.get(n.id) } : n));
  if (!ns.length) return { x: 90, y: 110, k: 1 };
  const x0 = Math.min(...ns.map(n => n.x)) - JX, y0 = Math.min(...ns.map(n => n.y));
  const x1 = Math.max(...ns.map(n => n.x + nw(n))), y1 = Math.max(...ns.map(n => n.y + nh(n)));
  const W = innerWidth, Hh = Math.min(innerHeight - 200, barTop() - 10 - 70);
  const place = (w, hh) => {
    const k = Math.min(1.1, Math.max(0.15, Math.min((w - 120) / (x1 - x0), (hh - 40) / (y1 - y0))));
    return { k, x: (w - (x1 - x0) * k) / 2 - x0 * k, y: 70 + (hh - (y1 - y0) * k) / 2 - y0 * k };
  };
  // the git tree in the corner: cards that would reach under it go beside it or above it, whichever is larger
  const v = place(W, Hh), t = treeBox();
  if (!t || v.x + x1 * v.k <= t.left || v.y + y1 * v.k <= t.top) return v;
  const beside = place(t.left, Hh), above = place(W, Math.min(Hh, t.top - 80));
  return beside.k >= above.k ? beside : above;
}
/* pan just enough that a node is fully visible between the bars */
function reveal(id) {
  const n = S.nodes.get(id); if (!n) return;
  const k = S.v.k, x0 = S.v.x + n.x * k, y0 = S.v.y + n.y * k, x1 = x0 + nw(n) * k, y1 = y0 + nh(n) * k;
  const top = 60, bottom = Math.min(innerHeight - 110, barTop() - 10), left = 24 + JX * k, right = innerWidth - 24;
  let dx = 0, dy = 0;
  if (x0 < left) dx = left - x0; else if (x1 > right) dx = right - x1;
  if (y0 < top) dy = top - y0; else if (y1 > bottom) dy = bottom - y1;
  // (and out from under the git tree in the corner, the shorter way)
  const t = treeBox();
  if (t && x1 + dx > t.left - 10 && y1 + dy > t.top - 10) {
    const ux = t.left - 10 - (x1 + dx), uy = t.top - 10 - (y1 + dy);
    const canX = x0 + dx + ux >= left, canY = y0 + dy + uy >= top;
    if (canX && (!canY || ux >= uy)) dx += ux; else if (canY) dy += uy;
  }
  if (dx || dy) { S.v.x += dx; S.v.y += dy; applyView(); }
}
function centerOn(id) {
  const n = S.nodes.get(id); if (!n) return;
  S.v.x = innerWidth / 2 - (n.x + nw(n) / 2) * S.v.k; S.v.y = Math.min((innerHeight - 160) / 2 + 40, (42 + barTop()) / 2) - (n.y + nh(n) / 2) * S.v.k;
  applyView();
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
