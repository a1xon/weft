/* ------------------------------------------------------------ tidy
   Lays the canvas out the way a person would. Each flow runs left to right in columns, and a card
   sits level with the cards it is linked to wherever it can: a chain is one straight line, a fork
   fans out around the card it starts from, a join sits level with the middle of what it waits for.
   Links do not cross where another order of the cards avoids it, and none runs through a card: a
   link that skips columns gets a lane of its own. Each column stacks only its own cards, so a tall
   card pushes nothing else down. Agents that talk stand side by side under their arches; lone cards
   gather in a block of their own (one per folder while the canvas works in several). Those groups
   are then packed to suit the window: the arrangement that shows the cards largest, or one nearly
   as large that keeps closer to where things stood (what was on top stays on top), with more room
   between groups than inside one. ctrl+z puts everything back. */
const TIDY = { gx: 130, gy: 56, lane: 42, apartX: 260, apartY: 180, loose: 70 };

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
  // pieces: the cards joined by links (a flow, laid out on its own), or a card alone
  const nb = new Map(ns.map(n => [n.id, []]));
  for (const [a, b] of links.values()) { nb.get(a).push(b); nb.get(b).push(a); }
  const pieceOf = new Map(), pieces = [];
  for (const n of ns) {
    if (pieceOf.has(n.id)) continue;
    const ids = [n.id]; pieceOf.set(n.id, pieces.length);
    for (let i = 0; i < ids.length; i++) for (const m of nb.get(ids[i])) if (!pieceOf.has(m)) { pieceOf.set(m, pieces.length); ids.push(m); }
    const bs = ids.map(id => box.get(id));
    pieces.push({ ...(bs.length > 1 ? settledFlow(bs, [...links.values()]) : { at: new Map([[n.id, { x: 0, y: 0 }]]), w: bs[0].w, h: bs[0].h }), was: around(bs) });
  }
  // groups: pieces that talk to each other (nothing fires along a talk, it only keeps them together)
  const up = pieces.map((_, i) => i), root = i => (up[i] === i ? i : (up[i] = root(up[i])));
  const pairs = [];
  for (const t of S.talks ? S.talks.values() : []) {
    if (!box.has(t.a) || !box.has(t.b)) continue;
    const a = pieceOf.get(t.a), b = pieceOf.get(t.b);
    if (a !== b) { pairs.push([a, b]); up[root(a)] = root(b); }
  }
  const groups = new Map();
  pieces.forEach((_, i) => { const r = root(i); if (!groups.has(r)) groups.set(r, []); groups.get(r).push(i); });
  // the folder a group works in (where most of its cards do), while the canvas works in several
  const folder = ids => {
    const count = new Map();
    if (FD.many) for (const id of ids) if (kindOf(S.nodes.get(id)) !== 'note') { const d = workDirs(S.nodes.get(id)).join('|'); count.set(d, (count.get(d) || 0) + 1); }
    return [...count].reduce((a, b) => (b[1] > a[1] ? b : a), ['', 0])[0];
  };
  const items = [], loose = new Map();
  for (const g of groups.values()) {
    const it = g.length > 1 ? talkRow(g, pieces, pairs) : pieces[g[0]];
    if (it.at.size > 1) { items.push({ ...it, dir: folder([...it.at.keys()]) }); continue; }
    const id = [...it.at.keys()][0], d = folder([id]);
    if (!loose.has(d)) loose.set(d, []);
    loose.get(d).push(box.get(id));
  }
  for (const [dir, cards] of loose) items.push({ cards: inReadingOrder(cards, b => b), was: around(cards), dir });
  // in the order they stood, one folder's groups after another
  const order = inReadingOrder(items, it => it.was), first = new Map();
  for (const it of order) if (!first.has(it.dir)) first.set(it.dir, first.size);
  const at = compose(order.sort((a, b) => first.get(a.dir) - first.get(b.dir)), box);
  // from the top left corner of what was there, on a 10 px raster (the corner too, or a second tidy
  // would round differently)
  const snap = v => Math.round(v / 10) * 10, X0 = snap(Math.min(...ns.map(n => n.x))), Y0 = snap(Math.min(...ns.map(n => n.y)));
  for (const [id, p] of at) at.set(id, { x: X0 + snap(p.x), y: Y0 + snap(p.y) });
  return at;
}
const around = bs => {
  const x = Math.min(...bs.map(b => b.x)), y = Math.min(...bs.map(b => b.y));
  return { x, y, w: Math.max(...bs.map(b => b.x + b.w)) - x, h: Math.max(...bs.map(b => b.y + b.h)) - y };
};
/* the order things stood in: rows of what starts about level with the row's first, each left to right */
function inReadingOrder(list, rect) {
  const s = [...list].sort((a, b) => rect(a).y - rect(b).y || rect(a).x - rect(b).x), out = [];
  for (let i = 0; i < s.length;) {
    const lim = rect(s[i]).y + Math.min(rect(s[i]).h, NH) * 0.75;
    let j = i + 1;
    while (j < s.length && rect(s[j]).y < lim) j++;
    out.push(...s.slice(i, j).sort((a, b) => rect(a).x - rect(b).x)); i = j;
  }
  return out;
}
/* agents that talk, side by side in one row (their arches rise less than the room between groups),
   everyone next to someone they talk to where it can be */
function talkRow(g, pieces, pairs) {
  const next = new Map(g.map(i => [i, []]));
  for (const [a, b] of pairs) if (next.has(a)) { next.get(a).push(b); next.get(b).push(a); }
  const byX = (a, b) => pieces[a].was.x - pieces[b].was.x, order = [];
  const walk = i => { if (order.includes(i)) return; order.push(i); for (const j of next.get(i).sort(byX)) walk(j); };
  walk([...g].sort(byX)[0]);
  const at = new Map();
  let x = 0, h = 0;
  for (const i of order) {
    const p = pieces[i];
    for (const [id, q] of p.at) at.set(id, { x: x + q.x, y: q.y });
    x += p.w + TIDY.gx; h = Math.max(h, p.h);
  }
  return { at, w: x - TIDY.gx, h, was: around(g.map(i => pieces[i].was)) };
}
/* lone cards in rows of up to `cols`, the rows filled evenly (4 + 3, not 6 + 1), in the order they stood */
function looseBlock(cards, cols) {
  const at = new Map(), rows = Math.ceil(cards.length / cols);
  let y = 0, w = 0, i = 0;
  for (let r = 0; r < rows; r++) {
    const row = cards.slice(i, i += Math.ceil((cards.length - i) / (rows - r)));
    let x = 0;
    for (const b of row) { at.set(b.id, { x, y }); x += b.w + TIDY.loose; }
    w = Math.max(w, x - TIDY.loose); y += Math.max(...row.map(b => b.h)) + TIDY.gy;
  }
  return { at, w, h: y - TIDY.gy };
}
/* the groups, in reading order, packed into one picture: every width the rows could take and every
   shape of the lone blocks is tried. The picture that the window shows largest wins, unless one
   nearly as large keeps the cards closer to where they stood. Positions from 0,0. */
function compose(items, box) {
  const G = TIDY.apartX, aw = innerWidth - 120, ah = Math.min(innerHeight - 200, barTop() - 80) - 40;
  const most = Math.min(10, Math.max(1, ...items.map(it => (it.cards ? it.cards.length : 1))));
  const X0 = Math.min(...[...box.values()].map(b => b.x)), Y0 = Math.min(...[...box.values()].map(b => b.y));
  const tries = [];
  for (let cols = 1; cols <= most; cols++) {
    const shaped = items.map(it => (it.cards ? looseBlock(it.cards, cols) : it));
    const widths = new Set([Math.max(...shaped.map(it => it.w))]);
    for (let i = 0; i < shaped.length; i++) for (let j = i, w = -G; j < shaped.length; j++) widths.add(w += shaped[j].w + G);
    for (const W of widths) {
      const P = shelves(shaped, W);
      const at = new Map();
      shaped.forEach((it, i) => { for (const [id, q] of it.at) at.set(id, { x: P.xy[i].x + q.x, y: P.xy[i].y + q.y }); });
      const x0 = Math.min(...[...at.values()].map(p => p.x)), y0 = Math.min(...[...at.values()].map(p => p.y));
      let moved = 0;
      for (const [id, p] of at) { const b = box.get(id); moved += Math.hypot(p.x - x0 - (b.x - X0), p.y - y0 - (b.y - Y0)); }
      tries.push({ at, x0, y0, k: Math.min(1.1, aw / (P.w + JX), ah / P.h), moved });
    }
  }
  const top = Math.max(...tries.map(t => t.k));
  const pick = tries.filter(t => t.k >= top * 0.97).reduce((a, b) => (b.moved < a.moved - 1 ? b : a));
  return new Map([...pick.at].map(([id, p]) => [id, { x: p.x - pick.x0, y: p.y - pick.y0 }]));
}
/* rows in the given order, each as full as width W allows, the boxes in a row level at the top and
   TIDY.apartX / apartY apart. (Rows keep the order: a second tidy reads the same order back.) */
function shelves(list, W) {
  const xy = [];
  let x = 0, y = 0, tall = 0, w = 0;
  for (const it of list) {
    if (x > 0 && x + it.w > W) { x = 0; y += tall + TIDY.apartY; tall = 0; }
    xy.push({ x, y });
    w = Math.max(w, x + it.w); x += it.w + TIDY.apartX; tall = Math.max(tall, it.h);
  }
  return { xy, w, h: y + tall };
}

/* a flow laid out again from where it lands until it stays put, the way the next tidy would see it:
   tidy on a tidy canvas moves nothing */
function settledFlow(bs, links) {
  const snap = v => Math.round(v / 10) * 10;
  let L = layoutFlow(bs, links);
  for (let i = 0; i < 4; i++) {
    const again = layoutFlow(bs.map(b => ({ ...b, x: snap(L.at.get(b.id).x), y: snap(L.at.get(b.id).y) })), links);
    if (bs.every(b => again.at.get(b.id).x === L.at.get(b.id).x && again.at.get(b.id).y === L.at.get(b.id).y)) break;
    L = again;
  }
  return L;
}

/* one flow (layered, Sugiyama style): columns, then the order in each column (fewest crossings),
   then heights (each card level with the median of its neighbours). Positions relative to 0,0. */
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

  // heights: each card as close as it can be to the (weighted) median height of its neighbours, the
  // order in its column kept and each column stacking only its own cards (a tall card pushes nothing
  // in the next column). Per column that is an L1 isotonic fit (pool adjacent violators) on the top
  // minus the room the cards above need; a tie stays where it is. A lane outweighs a fan: a link drawn
  // from one height to another across columns would cut through the cards in between, so a long link
  // must run straight from end to end.
  const room = id => (nodes.get(id).real ? nodes.get(id).h : TIDY.lane) + TIDY.gy;
  const top = new Map();
  for (const ly of layers) ly.reduce((t, id) => (top.set(id, t), t + room(id)), 0);
  const settle = b => {
    const c = Math.round(b.cur.reduce((s, x) => s + x, 0) / b.cur.length);
    if (!b.pts.length) return (b.s = c);
    const p = [...b.pts].sort((x, y) => x[0] - y[0]), half = p.reduce((s, x) => s + x[1], 0) / 2;
    let acc = 0, lo = null, hi = null;
    for (const [v, w] of p) { acc += w; if (lo === null && acc >= half) lo = v; if (acc > half) { hi = v; break; } }
    b.s = Math.min(hi, Math.max(lo, c));
  };
  for (let it = 0; it < 80; it++) {
    let moved = false;
    for (const ly of it % 2 ? [...layers].reverse() : layers) {
      const blocks = [];
      ly.reduce((off, id) => {
        const n = nodes.get(id);
        const b = { ids: [id], cur: [top.get(id) - off], pts: [...n.up, ...n.down].map(d => [top.get(d.id) - off, d.w]) };
        settle(b); blocks.push(b);
        while (blocks.length > 1 && blocks.at(-2).s > blocks.at(-1).s) {
          const z = blocks.pop(), a = blocks.at(-1);
          a.ids.push(...z.ids); a.cur.push(...z.cur); a.pts.push(...z.pts); settle(a);
        }
        return off + room(id);
      }, 0);
      let off = 0;
      for (const b of blocks) for (const id of b.ids) { const t = b.s + off; off += room(id); if (t !== top.get(id)) { top.set(id, t); moved = true; } }
    }
    if (!moved) break;
  }

  // columns as wide as their widest card; a column whose card has a junction in front of it leaves
  // room for it
  const colW = [], colGap = [];
  for (const [id, n] of nodes) {
    const c = L.get(id);
    colW[c] = Math.max(colW[c] || 0, n.real ? n.w : TIDY.lane);
    if (n.real && incoming(id).length >= 2) colGap[c] = JX + 15;
  }
  const xs = [];
  let x = 0;
  for (let c = 0; c < colW.length; c++) { x += colGap[c] || 0; xs[c] = x; x += colW[c] + TIDY.gx; }
  const t0 = Math.min(...top.values()), at = new Map();
  let h = 0;
  for (const [id, n] of nodes) {
    h = Math.max(h, top.get(id) - t0 + room(id) - TIDY.gy);
    if (n.real) at.set(id, { x: xs[L.get(id)], y: top.get(id) - t0 });
  }
  return { at, w: x - TIDY.gx, h };
}
