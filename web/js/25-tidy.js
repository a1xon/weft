/* ------------------------------------------------------------ tidy
   Lines the canvas up. Each flow runs left to right in columns, and a card sits on the same row as
   the cards it is linked to wherever it can: a chain is one straight line, a fork fans out around
   the card it starts from, a join sits level with the middle of what it waits for. Links do not
   cross where another order of the cards avoids it, and none runs through a card: a link that
   skips columns gets a lane of its own. Flows are stacked in the order they had; lone cards line
   up in rows under them. ctrl+z puts everything back. */
const TIDY = { gx: 130, gy: 56, lane: 42, flows: 120 };

let glideRun = 0, gliding = false;
function tidy() {
  if (gliding) return;   // a second T while the cards are still on their way
  const to = tidyLayout(); if (!to) return;
  const back = new Map([...to.keys()].map(id => [id, { x: S.nodes.get(id).x, y: S.nodes.get(id).y }]));
  const view = { ...S.v };
  S.undo.push(() => glide(back, view));
  glide(to, fitView(to));
}
/* move cards (Map id -> {x, y}) and the view there together, then tell the daemon; a newer glide
   (ctrl+z right away) takes over from where the cards are */
function glide(to, view) {
  const run = ++glideRun; gliding = true;
  const ids = [...to.keys()].filter(id => S.nodes.has(id));
  const from = new Map(ids.map(id => [id, { x: S.nodes.get(id).x, y: S.nodes.get(id).y }])), v0 = { ...S.v };
  const step = t => {
    const e = 1 - Math.pow(1 - t, 3), mix = (a, b) => a + (b - a) * e;
    for (const id of ids) {
      const n = S.nodes.get(id), a = from.get(id), b = to.get(id); if (!n) continue;
      n.x = t < 1 ? mix(a.x, b.x) : b.x; n.y = t < 1 ? mix(a.y, b.y) : b.y; placeNode(n);
    }
    if (view) { S.v = t < 1 ? { x: mix(v0.x, view.x), y: mix(v0.y, view.y), k: mix(v0.k, view.k) } : { ...view }; applyView(); }
    drawAllEdges();
  };
  const done = () => { gliding = false; send('node_move', { moves: ids.filter(id => S.nodes.has(id)).map(id => ({ id, ...to.get(id) })) }); };
  if (matchMedia('(prefers-reduced-motion: reduce)').matches) { step(1); return done(); }
  const t0 = performance.now();
  const tick = now => { if (run !== glideRun) return; const t = Math.min(1, (now - t0) / 320); step(t); if (t < 1) requestAnimationFrame(tick); else done(); };
  requestAnimationFrame(tick);
}

/* where every card goes: Map id -> {x, y}, from the top left corner of what is there now */
function tidyLayout() {
  const ns = [...S.nodes.values()]; if (!ns.length) return null;
  const box = new Map(ns.map(n => [n.id, { id: n.id, w: nw(n), h: nh(n) + filmH(n.id), x: n.x, y: n.y }]));
  // links, and the dotted line from an agent to the fresh one that took over from it
  const links = new Map();
  const link = (a, b) => { if (a !== b && box.has(a) && box.has(b)) links.set(a + '\n' + b, [a, b]); };
  for (const e of S.edges.values()) link(e.from, e.to);
  for (const n of ns) if (n.takeover) link(n.takeover, n.id);
  // flows: the cards joined by links
  const nb = new Map(ns.map(n => [n.id, []]));
  for (const [a, b] of links.values()) { nb.get(a).push(b); nb.get(b).push(a); }
  const seen = new Set(), flows = [], loose = [];
  for (const n of ns) {
    if (seen.has(n.id)) continue;
    const ids = [n.id]; seen.add(n.id);
    for (let i = 0; i < ids.length; i++) for (const m of nb.get(ids[i])) if (!seen.has(m)) { seen.add(m); ids.push(m); }
    if (ids.length > 1) flows.push(ids.map(id => box.get(id))); else loose.push(box.get(n.id));
  }
  const top = f => Math.min(...f.map(b => b.y)), left = f => Math.min(...f.map(b => b.x));
  flows.sort((a, b) => top(a) - top(b) || left(a) - left(b));
  loose.sort((a, b) => a.y - b.y || a.x - b.x);
  const X0 = Math.min(...ns.map(n => n.x)), Y0 = Math.min(...ns.map(n => n.y));
  const at = new Map(), snap = v => Math.round(v / 10) * 10;
  let y = Y0, wide = 0;
  for (const f of flows) {
    const L = layoutFlow(f, [...links.values()]);
    for (const [id, p] of L.at) at.set(id, { x: snap(X0 + p.x), y: snap(y + p.y) });
    y += L.h + TIDY.flows; wide = Math.max(wide, L.w);
  }
  if (loose.length) {
    // spaced like the flows' columns, as wide as the widest flow (a balanced grid when there is none),
    // rows filled evenly (3+2, not 4+1)
    const n = loose.length, cw = Math.max(...loose.map(b => b.w)) + TIDY.gx;
    let cols = wide ? Math.max(1, Math.floor((wide + TIDY.gx) / cw)) : n <= 4 ? n : Math.ceil(n / Math.ceil(Math.sqrt(n / 1.8)));
    cols = Math.ceil(n / Math.ceil(n / cols));
    for (let i = 0; i < n; i += cols) {
      const row = loose.slice(i, i + cols);
      row.forEach((b, j) => at.set(b.id, { x: snap(X0 + j * cw), y: snap(y) }));
      y += Math.max(...row.map(b => b.h)) + TIDY.gy;
    }
  }
  return at;
}

/* one flow (layered, Sugiyama style): columns, then the order in each column (fewest crossings),
   then rows (each card level with the median of its neighbours). Positions relative to 0,0. */
function layoutFlow(list, links) {
  const V = new Map(list.map(b => [b.id, { ...b, real: true, ins: [], outs: [], up: [], down: [] }]));
  const es = links.filter(([a, b]) => V.has(a) && V.has(b));
  // a loop is cut where it runs back: start from what nothing feeds, leftmost first
  const out = new Map([...V.keys()].map(id => [id, []])), fed = new Set(es.map(e => e[1]));
  for (const [a, b] of es) out.get(a).push(b);
  const state = new Map(), keep = new Map();
  const visit = u => {
    state.set(u, 1);
    for (const v of out.get(u)) {
      if (state.get(v) === 1) keep.set(v + '\n' + u, [v, u]);
      else { keep.set(u + '\n' + v, [u, v]); if (!state.has(v)) visit(v); }
    }
    state.set(u, 2);
  };
  const starts = [...V.values()].sort((a, b) => fed.has(a.id) - fed.has(b.id) || a.x - b.x);
  for (const s of starts) if (!state.has(s.id)) visit(s.id);
  const E = [...keep.values()];
  for (const [a, b] of E) { V.get(a).outs.push(b); V.get(b).ins.push(a); }

  // columns: the longest path from a start, then cards pulled right towards what they feed
  const order = [], deg = new Map([...V.values()].map(v => [v.id, v.ins.length]));
  const q = [...V.keys()].filter(id => !deg.get(id));
  while (q.length) { const u = q.shift(); order.push(u); for (const w of V.get(u).outs) { deg.set(w, deg.get(w) - 1); if (!deg.get(w)) q.push(w); } }
  const L = new Map();
  for (const u of order) L.set(u, Math.max(0, ...V.get(u).ins.map(p => L.get(p) + 1)));
  // a card that hands on more than it takes moves up to what it feeds (a note next to its agent),
  // and so does a lone lead-in (agent → commit next to the push it shares with longer chains)
  const lead = new Map();
  for (const u of order) { const v = V.get(u); lead.set(u, !v.ins.length || (v.ins.length === 1 && V.get(v.ins[0]).outs.length === 1 && lead.get(v.ins[0]))); }
  for (const u of [...order].reverse()) {
    const v = V.get(u);
    if (v.outs.length && (v.ins.length < v.outs.length || lead.get(u))) L.set(u, Math.min(...v.outs.map(s => L.get(s))) - 1);
  }
  const l0 = Math.min(...L.values());
  for (const [id, l] of L) L.set(id, l - l0);

  // a link that skips columns runs through a placeholder in each one it crosses: its lane
  const nodes = new Map(V);
  const seg = (a, b) => {
    const A = nodes.get(a), B = nodes.get(b), w = A.real && B.real ? 1 : 8;
    A.down.push({ id: b, w }); B.up.push({ id: a, w });
  };
  let k = 0;
  for (const [a, b] of E) {
    let prev = a;
    const la = L.get(a), lb = L.get(b), ya = V.get(a).y, yb = V.get(b).y;
    for (let l = la + 1; l < lb; l++) {
      const d = '\u0000' + k++;
      nodes.set(d, { id: d, real: false, y: ya + (yb - ya) * (l - la) / (lb - la), up: [], down: [] });
      L.set(d, l); seg(prev, d); prev = d;
    }
    seg(prev, b);
  }

  // order in each column: as they stand now, then barycenter sweeps and swaps while that uncrosses links
  let layers = [];
  for (const id of nodes.keys()) (layers[L.get(id)] ||= []).push(id);
  layers = Array.from(layers, ly => (ly || []).sort((a, b) => nodes.get(a).y - nodes.get(b).y));
  const pos = new Map();
  const index = () => layers.forEach(ly => ly.forEach((id, i) => pos.set(id, i)));
  const crossings = () => {
    let c = 0;
    for (const ly of layers) {
      const s = ly.flatMap(a => nodes.get(a).down.map(d => [pos.get(a), pos.get(d.id)]));
      for (let i = 0; i < s.length; i++) for (let j = i + 1; j < s.length; j++) if ((s[i][0] - s[j][0]) * (s[i][1] - s[j][1]) < 0) c++;
    }
    return c;
  };
  const sweep = (ly, side) => {
    const key = new Map(ly.map((id, i) => { const n = nodes.get(id)[side]; return [id, n.length ? n.reduce((s, d) => s + pos.get(d.id), 0) / n.length : i]; }));
    ly.sort((a, b) => key.get(a) - key.get(b));
    ly.forEach((id, i) => pos.set(id, i));
  };
  // links crossed if u stands above v
  const pair = (u, v) => {
    let c = 0;
    for (const side of ['up', 'down']) for (const a of nodes.get(u)[side]) for (const b of nodes.get(v)[side]) if (pos.get(a.id) > pos.get(b.id)) c++;
    return c;
  };
  const transpose = () => {
    for (let round = 0, again = true; again && round < 8; round++) {
      again = false;
      for (const ly of layers) for (let i = 0; i + 1 < ly.length; i++) {
        if (pair(ly[i + 1], ly[i]) < pair(ly[i], ly[i + 1])) {
          [ly[i], ly[i + 1]] = [ly[i + 1], ly[i]]; pos.set(ly[i], i); pos.set(ly[i + 1], i + 1); again = true;
        }
      }
    }
  };
  index();
  let best = layers.map(ly => [...ly]), fewest = crossings();
  for (let it = 0; it < 12 && fewest; it++) {
    if (it % 2 === 0) for (let l = 1; l < layers.length; l++) sweep(layers[l], 'up');
    else for (let l = layers.length - 2; l >= 0; l--) sweep(layers[l], 'down');
    transpose();
    const c = crossings();
    if (c < fewest) { fewest = c; best = layers.map(ly => [...ly]); }
  }
  layers = best;

  // rows: each card as close as it can be to the (weighted) median row of its neighbours, the order
  // in its column kept. Per column that is an L1 isotonic fit (pool adjacent violators); a tie
  // stays where it is. A lane outweighs a fan: a link drawn from one row to another across columns
  // would cut through the cards in between, so a long link must run straight from end to end.
  const row = new Map();
  layers.forEach(ly => ly.forEach((id, i) => row.set(id, i)));
  const settle = b => {
    const c = Math.round(b.cur.reduce((s, x) => s + x, 0) / b.cur.length);
    if (!b.pts.length) return (b.s = c);
    const p = [...b.pts].sort((x, y) => x[0] - y[0]), half = p.reduce((s, x) => s + x[1], 0) / 2;
    let acc = 0, lo = null, hi = null;
    for (const [v, w] of p) { acc += w; if (lo === null && acc >= half) lo = v; if (acc > half) { hi = v; break; } }
    b.s = Math.min(hi, Math.max(lo, c));
  };
  for (let it = 0; it < 60; it++) {
    let moved = false;
    for (const ly of it % 2 ? [...layers].reverse() : layers) {
      const blocks = [];
      ly.forEach((id, i) => {
        const n = nodes.get(id);
        const b = { ids: [id], cur: [row.get(id) - i], pts: [...n.up, ...n.down].map(d => [row.get(d.id) - i, d.w]) };
        settle(b); blocks.push(b);
        while (blocks.length > 1 && blocks.at(-2).s > blocks.at(-1).s) {
          const z = blocks.pop(), a = blocks.at(-1);
          a.ids.push(...z.ids); a.cur.push(...z.cur); a.pts.push(...z.pts); settle(a);
        }
      });
      let i = 0;
      for (const b of blocks) for (const id of b.ids) { const r = b.s + i++; if (r !== row.get(id)) { row.set(id, r); moved = true; } }
    }
    if (!moved) break;
  }

  // sizes: a row as tall as its tallest card (a lane is thin), a column as wide as its widest; a
  // column whose card has a junction in front of it leaves room for it
  const r0 = Math.min(...row.values()), rowH = [], colW = [], colGap = [];
  for (const [id, n] of nodes) {
    const r = row.get(id) - r0, c = L.get(id);
    rowH[r] = Math.max(rowH[r] || 0, n.real ? n.h : TIDY.lane);
    colW[c] = Math.max(colW[c] || 0, n.real ? n.w : TIDY.lane);
    if (n.real && incoming(id).length >= 2) colGap[c] = JX + 15;
  }
  const ys = [], xs = [];
  let y = 0, x = 0;
  for (let r = 0; r < rowH.length; r++) { ys[r] = y; if (rowH[r]) y += rowH[r] + TIDY.gy; }
  for (let c = 0; c < colW.length; c++) { x += colGap[c] || 0; xs[c] = x; x += colW[c] + TIDY.gx; }
  const at = new Map();
  for (const [id, n] of nodes) if (n.real) at.set(id, { x: xs[L.get(id)], y: ys[row.get(id) - r0] });
  return { at, w: x - TIDY.gx, h: y - TIDY.gy };
}
