// Where a card that starts working lands: never left of a card that runs or has run in the same
// row (what is queued after it would have no room there), from the pad, from the bar with nothing
// selected and as a hand over; a draft may still go there. A prompt queued after it lines up right
// next to it.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs');
const W = process.env.SANDBOX;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const p = await (await b.newContext({ viewport: { width: 1600, height: 900 } })).newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type() === 'error' && errs.push(m.text()));
  const waitFor = async (fn, t = 15000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (await fn()) return true; await sleep(200); } return false; };
  const st = id => p.evaluate(i => (S.rt.get(i) || {}).status || '', id);
  const box = id => p.evaluate(i => { const n = S.nodes.get(i); return n && { id: i, x: n.x, y: n.y, w: nw(n), h: nh(n) }; }, id);
  const ids = () => p.evaluate(() => [...S.nodes.keys()]);
  // the cards that run or have run, left of which `c` stands in their row
  const rightOfIt = (c, others) => others.filter(o => o.id !== c.id && o.x >= c.x + c.w && c.y < o.y + o.h && c.y + c.h > o.y).map(o => o.id);
  const started = () => p.evaluate(() => [...S.nodes.values()].filter(n => cat((S.rt.get(n.id) || {}).status) !== 'pre').map(n => ({ id: n.id, x: n.x, y: n.y, w: nw(n), h: nh(n) })));
  const newCard = async before => { let id = null; await waitFor(async () => !!(id = (await ids()).find(i => !before.includes(i)))); return id; };
  // look at the canvas so that the middle of the window is `dx` left of a card, in its row
  const lookLeftOf = (id, dx) => p.evaluate(([i, dx]) => {
    const n = S.nodes.get(i), k = S.v.k;
    S.v.x = innerWidth / 2 - k * (n.x - dx + NW / 2); S.v.y = innerHeight / 2 - k * (n.y + NH / 2); applyView();
  }, [id, dx]);

  await p.goto(fs.readFileSync(W + '/url.txt', 'utf8').trim()); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400);
  await p.keyboard.type('Explain what textkit/slug.py does in one sentence.'); await p.keyboard.press('Control+Enter');
  await p.waitForSelector('.nw[data-id="n1"]');
  ok(await waitFor(async () => (await st('n1')) === 'done', 30000), 'the first agent works and is done');
  const A = await box('n1');

  // --- the rule itself: left of n1 in its row there is room, but not for a card that starts working
  const spot = await p.evaluate(a => ({ draft: freeSpot(a.x - NW - 400, a.y), working: freeSpot(a.x - NW - 400, a.y, true) }), A);
  ok(rightOfIt({ id: 'x', ...spot.draft, w: 600, h: 260 }, [A]).length === 1, 'a draft may go left of the done card (' + JSON.stringify(spot.draft) + ')');
  ok(rightOfIt({ id: 'x', ...spot.working, w: 600, h: 260 }, [A]).length === 0, 'a card that starts working does not (' + JSON.stringify(spot.working) + ')');

  // --- the pad starts in the middle of the window: here that is left of n1, in its row
  await lookLeftOf('n1', 1000); await sleep(200);
  let before = await ids(), was = await started();
  await p.evaluate(() => openPad()); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(300);
  await p.keyboard.type('Explain textkit/wrap.py in one sentence.'); await p.keyboard.press('Control+Enter');
  const n2 = await newCard(before), B = await box(n2);
  ok(!!n2 && rightOfIt(B, was).length === 0, 'started from the pad it is not left of the done card (' + JSON.stringify(B) + ' vs ' + JSON.stringify(A) + ')');
  await p.screenshot({ path: W + '/place-1-pad.png' });

  // --- the bar with nothing selected starts right of the last card: a done card stands further on in that row
  ok(await waitFor(async () => (await st(n2)) === 'done', 30000), 'the second agent is done');
  const farX = B.x + B.w + 150 + 600 + 300;
  await p.evaluate(([x, y]) => send('node_set', { id: 'n1', patch: { x, y } }), [farX, B.y]);
  ok(await waitFor(async () => (await box('n1')).x === farX), 'the first card moved to the right, in the second one\'s row');
  await p.evaluate(() => { select([]); S.lastNode = [...S.nodes.keys()].find(i => i !== 'n1'); });
  before = await ids(); was = await started();
  await p.locator('#omni textarea').click();
  await p.keyboard.type('Explain textkit/__init__.py in one sentence.'); await p.keyboard.press('Control+Enter');
  const n3 = await newCard(before), C = await box(n3);
  ok(!!n3 && rightOfIt(C, was).length === 0, 'started from the bar it is not left of a card that ran (' + JSON.stringify(C) + ', right of it: ' + rightOfIt(C, was) + ')');

  // --- what is queued after it lines up next to it
  ok(await waitFor(async () => ['busy', 'starting'].includes(await st(n3)), 5000), 'the third agent works (' + await st(n3) + ')');
  await p.evaluate(i => select([i]), n3);
  before = await ids();
  await p.locator('#omni textarea').click();
  await p.keyboard.type('Then say done.'); await p.keyboard.press('Control+Enter');
  const s = await newCard(before), Sb = await box(s);
  ok(!!s && (await p.evaluate(i => S.nodes.get(i).agent, s)) === 'step', 'the prompt waits as a queued card');
  ok(Sb && Sb.y === C.y && Sb.x === C.x + C.w + 150, 'right next to it in its row (' + JSON.stringify(Sb) + ' vs ' + JSON.stringify(C) + ')');
  await p.evaluate(() => fit()); await sleep(300);
  await p.screenshot({ path: W + '/place-2-bar.png' });

  // --- a hand over starts right of the card it takes over from, not left of one that ran
  // (the queued card joins the agent when the agent takes it up: engine/merge.rs)
  ok(await waitFor(async () => (await p.evaluate(([i, q]) => !S.nodes.has(q) && /Then say done/.test(S.rt.get(i).asked || '') && S.rt.get(i).status === 'done', [n3, s])), 40000), 'the third agent answered its queued prompt, whose card joined it');
  before = await ids(); was = await started();
  await p.evaluate(i => takeoverFrom(i, 'claude', 'Carry on: explain textkit/slug.py edge cases.'), n2);
  const n4 = await newCard(before), D = await box(n4);
  ok(!!n4 && rightOfIt(D, was).length === 0, 'a hand over is not left of a card that ran (' + JSON.stringify(D) + ', right of it: ' + rightOfIt(D, was) + ')');
  const all = await p.evaluate(() => [...S.nodes.values()].map(n => ({ id: n.id, x: n.x, y: n.y, w: nw(n), h: nh(n) })));
  const hits = [];
  for (const u of all) for (const v of all) if (u.id < v.id && u.x < v.x + v.w && v.x < u.x + u.w && u.y < v.y + v.h && v.y < u.y + u.h) hits.push(u.id + '~' + v.id);
  ok(!hits.length, 'no card covers another ' + hits.join(', '));
  await p.evaluate(() => fit()); await sleep(300);
  await p.screenshot({ path: W + '/place-3-hand.png' });

  console.log('errors:', JSON.stringify(errs));
  ok(errs.length === 0, 'no page errors');
  await b.close();
  console.log(fails ? fails + ' FAILED' : 'ALL PASSED');
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error('CRASH', String(e).slice(0, 1200)); process.exit(1); });
