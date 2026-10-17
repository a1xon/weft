// tidy: a messy canvas lines up. A chain is one straight row, a fork fans out around its start, a
// link that skips columns runs straight along a free lane, no link crosses another or runs through
// a card, a tall card pushes only its own column, agents that talk stand side by side, lone cards
// gather in a block of their own, the groups keep more room between them than cards inside a flow
// and fill the wide window instead of one tall column; the daemon keeps it, a second tidy changes
// nothing, ctrl+z undoes it.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs');
const W = process.env.SANDBOX;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const ctx = await b.newContext({ viewport: { width: 1500, height: 880 } });
  const p = await ctx.newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message));
  const url = fs.readFileSync(W + '/url.txt', 'utf8').trim();
  const open = async () => { await p.goto('about:blank'); await p.goto(url); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400); await p.keyboard.press('Escape'); await sleep(200); };
  await open();

  // drafts (nothing runs), dropped anywhere. One flow: plan fans out to three, all three feed review,
  // review goes on to ship; plan also hands straight to ship (a link that skips columns); a note
  // feeds review; b is tall. Another flow: fan out to x y z, all three join. Two lone cards, two agents
  // that talk to each other.
  await p.evaluate(async () => {
    const spots = { plan: [900, 700], a: [100, 100], b: [1400, 300], c: [300, 900], review: [60, 500], ship: [700, 80], note: [1200, 1000], lone1: [500, 400], lone2: [1700, 800],
      fan: [400, 1500], x: [1300, 1900], y: [100, 1700], z: [800, 1400], join: [200, 2000], tA: [2500, 200], tB: [100, 2400] };
    const id = {};
    for (const [name, [x, y]] of Object.entries(spots)) {
      const node = name === 'note' ? { name, agent: 'note', prompt: 'context', x, y } : { name, agent: 'claude', prompt: 'do ' + name, draft: true, x, y };
      id[name] = (await req('node_add', { node })).id;
    }
    for (const [f, t] of [['plan', 'a'], ['plan', 'b'], ['plan', 'c'], ['a', 'review'], ['b', 'review'], ['c', 'review'], ['review', 'ship'], ['plan', 'ship'], ['note', 'review'],
      ['fan', 'x'], ['fan', 'y'], ['fan', 'z'], ['x', 'join'], ['y', 'join'], ['z', 'join']]) await req('edge_add', { from: id[f], to: id[t] });
    await req('talk_add', { a: id.tA, b: id.tB });
    send('node_set', { id: id.b, patch: { h: 500 } });
    return id;
  });
  await p.waitForFunction(n => S.edges.size === n && S.talks.size === 1 && nh([...S.nodes.values()].find(m => m.name === 'b')) === 500, 15);
  const at = () => p.evaluate(() => Object.fromEntries([...S.nodes.values()].map(n => [n.name, { x: n.x, y: n.y, w: nw(n), h: nh(n) }])));
  const before = await at();

  await p.click('#bTidy'); await sleep(700);
  const L = await at();
  await p.screenshot({ path: W + '/tidy-1.png' });
  ok(L.plan.x < L.a.x && L.a.x === L.b.x && L.b.x === L.c.x && L.c.x < L.review.x && L.review.x < L.ship.x, 'columns left to right: plan | a b c | review | ship');
  ok(L.plan.y === L.ship.y, 'plan and ship are level: the link that skips columns runs straight');
  ok(['a', 'b', 'c', 'note'].every(k => L[k].y !== L.plan.y), 'with nothing else on its lane');
  ok(L.fan.x < L.x.x && L.x.x === L.y.x && L.y.x === L.z.x && L.z.x < L.join.x, 'the other flow: fan | x y z | join');
  const ys = ['x', 'y', 'z'].map(k => L[k].y).sort((u, v) => u - v);
  ok(L.fan.y === ys[1] && L.join.y === ys[1], 'fan and join sit level with the middle of the three');
  ok(L.note.x < L.review.x && L.note.x >= L.a.x, 'the note stands right before the card it feeds');
  ok(L.c.y - L.b.y - 500 >= 50 && L.c.y - L.b.y - 500 <= 60 && L.review.y === L.b.y, 'the tall card pushes only its own column: c right under it, review level with it');
  ok(L.tA.y === L.tB.y && Math.abs(L.tA.x - L.tB.x) === L.tA.w + 130, 'the two agents that talk stand side by side');
  ok(L.lone1.x === L.lone2.x || L.lone1.y === L.lone2.y, 'the lone cards line up in a block of their own');
  const group = k => (['plan', 'a', 'b', 'c', 'review', 'ship', 'note'].includes(k) ? 1 : ['fan', 'x', 'y', 'z', 'join'].includes(k) ? 2 : k.startsWith('lone') ? 3 : 4);
  const cards = Object.entries(L);
  const near = [];
  for (const [k, u] of cards) for (const [j, v] of cards) if (k < j && group(k) !== group(j) && Math.max(v.x - u.x - u.w, u.x - v.x - v.w, v.y - u.y - u.h, u.y - v.y - v.h) < 170) near.push(k + '~' + j);
  ok(!near.length, 'the groups keep more room between them than cards inside a flow ' + near.join(', '));
  const wide = Math.max(...cards.map(([, v]) => v.x + v.w)) - Math.min(...cards.map(([, v]) => v.x)), tall = Math.max(...cards.map(([, v]) => v.y + v.h)) - Math.min(...cards.map(([, v]) => v.y));
  ok(wide > tall, `and fill the wide window rather than one tall column (${wide} x ${tall})`);
  ok(!cards.some(([k, u]) => cards.some(([j, v]) => k < j && u.x < v.x + v.w && v.x < u.x + u.w && u.y < v.y + v.h && v.y < u.y + u.h)), 'no card overlaps another');

  // the links as drawn: none through a card that is not its end, none crossing another
  const geo = await p.evaluate(() => {
    const out = [], along = path => { const len = path.getTotalLength(); return Array.from({ length: 61 }, (_, i) => { const q = path.getPointAtLength(len * i / 60); return [q.x, q.y]; }); };
    for (const e of S.edges.values()) out.push({ from: S.nodes.get(e.from).name, to: S.nodes.get(e.to).name, pts: along(document.getElementById('p-' + e.id)) });
    for (const t of S.talks.values()) out.push({ from: S.nodes.get(t.a).name, to: S.nodes.get(t.b).name, pts: along(document.getElementById('tp-' + t.id)) });
    return out;
  });
  const through = [];
  for (const g of geo) for (const [x, y] of g.pts.slice(1, -1)) for (const [k, c] of cards) if (k !== g.from && k !== g.to && x > c.x && x < c.x + c.w && y > c.y && y < c.y + c.h) through.push(`${g.from}->${g.to} through ${k}`);
  ok(!through.length, 'no link (and no talk) runs through a card ' + [...new Set(through)].join(', '));
  const side = (a, b, c) => (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
  const hit = (P, Q) => { for (let i = 0; i < 60; i++) for (let j = 0; j < 60; j++) if (side(P[i], P[i + 1], Q[j]) * side(P[i], P[i + 1], Q[j + 1]) < 0 && side(Q[j], Q[j + 1], P[i]) * side(Q[j], Q[j + 1], P[i + 1]) < 0) return true; return false; };
  const crossed = [];
  for (let i = 0; i < geo.length; i++) for (let j = i + 1; j < geo.length; j++) if (geo[i].from !== geo[j].from && geo[i].to !== geo[j].to && hit(geo[i].pts, geo[j].pts)) crossed.push(`${geo[i].from}->${geo[i].to} x ${geo[j].from}->${geo[j].to}`);
  ok(!crossed.length, 'no two links cross ' + crossed.join(', '));
  const lane = geo.find(g => g.from === 'plan' && g.to === 'ship');
  ok(lane.pts.every(q => Math.abs(q[1] - lane.pts[0][1]) < 1), 'the link that skips columns runs straight along its own lane');
  ok(await p.evaluate(() => [...S.nodes.values()].every(n => { const r = nodeEls.get(n.id).root.getBoundingClientRect(); return r.right > 0 && r.left < innerWidth && r.top > 40 && r.bottom < innerHeight; })), 'and the view fits it all');

  // ctrl+z puts everything back where it was, a second tidy lands the same
  await p.keyboard.press('Control+z'); await sleep(700);
  ok(JSON.stringify(await at()) === JSON.stringify(before), 'ctrl+z puts every card back where it was');
  await p.keyboard.press('t'); await sleep(700);
  ok(JSON.stringify(await at()) === JSON.stringify(L), 't tidies the same way again');

  // the daemon has it: a reload shows the same; tidy on a tidy canvas changes nothing
  await sleep(600); await open();
  ok(JSON.stringify(await at()) === JSON.stringify(L), 'a reload shows the tidy canvas');
  await p.click('#bTidy'); await sleep(700);
  ok(JSON.stringify(await at()) === JSON.stringify(L), 'tidy on a tidy canvas moves nothing');

  ok(!errs.length, 'no page errors ' + errs.join(' | '));
  await b.close();
  console.log(fails ? `${fails} FAILED` : 'ALL PASS');
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
