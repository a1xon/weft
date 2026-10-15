// Dragging the first card of a finished flow moves the whole flow; any other card, an unfinished
// flow or ctrl+drag moves just the one card.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs');
const W = process.env.SANDBOX;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const p = await (await b.newContext({ viewport: { width: 1500, height: 880 } })).newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message));
  const url = fs.readFileSync(W + '/url.txt', 'utf8').trim();
  const waitFor = async (fn, t = 30000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (await fn()) return true; await sleep(200); } return false; };
  const card = id => p.locator('.nw[data-id="' + id + '"]');
  const st = id => card(id).getAttribute('data-st');
  const add = (node, run) => p.evaluate(async a => (await req('node_add', a)).id, { node, run });
  const at = id => p.evaluate(i => { const n = S.nodes.get(i); return [n.x, n.y]; }, id);
  const drag = async (id, dx, dy, mod) => {
    const hb = await card(id).locator('.hd').boundingBox();
    const sx = hb.x + 70, sy = hb.y + hb.height / 2;
    if (mod) await p.keyboard.down(mod);
    await p.mouse.move(sx, sy); await p.mouse.down();
    for (let i = 1; i <= 8; i++) { await p.mouse.move(sx + dx * i / 8, sy + dy * i / 8); await sleep(20); }
    await p.mouse.up();
    if (mod) await p.keyboard.up(mod);
    await sleep(400);
  };
  const moved = async (before, id) => { const [x, y] = await at(id); return x !== before.get(id)[0] || y !== before.get(id)[1]; };
  const snap = async ids => new Map(await Promise.all(ids.map(async i => [i, await at(i)])));

  await p.goto(url); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400); await p.keyboard.press('Escape');
  await p.evaluate(() => { S.v = { x: 0, y: 0, k: 1 }; applyView(); });
  const a = await add({ name: 'first', agent: 'agy', prompt: 'Explain textkit/slug.py', x: 100, y: 120 }, true);
  const m = await add({ name: 'middle', agent: 'agy', prompt: 'Explain textkit/wrap.py', x: 520, y: 120 }, false);
  const z = await add({ name: 'last', agent: 'agy', prompt: 'Explain textkit/__init__.py', x: 940, y: 120 }, false);
  await p.evaluate(([a, m, z]) => { send('edge_add', { from: a, to: m }); send('edge_add', { from: m, to: z }); }, [a, m, z]);
  const all = [a, m, z];

  // --- still at work: the first card moves alone
  ok(await waitFor(async () => (await st(a)) === 'done', 45000), 'first is done (' + await st(a) + ')');
  ok((await st(z)) !== 'done', 'last is not done yet (' + await st(z) + ')');
  let before = await snap(all);
  await drag(a, 0, 60);
  ok(await moved(before, a) && !(await moved(before, m)) && !(await moved(before, z)), 'while the flow works the first card moves alone');

  // --- all finished: the first card brings the flow
  ok(await waitFor(async () => (await Promise.all(all.map(st))).every(s => s === 'done'), 60000), 'the whole flow is done');
  before = await snap(all);
  await drag(a, 80, 200);
  const d = id => at(id).then(([x, y]) => [x - before.get(id)[0], y - before.get(id)[1]]);
  const da = await d(a);
  ok(da[1] > 100, 'first moved down ' + da);
  ok(JSON.stringify(await d(m)) === JSON.stringify(da) && JSON.stringify(await d(z)) === JSON.stringify(da), 'the rest of the flow came along by the same amount');
  await p.reload(); await p.waitForSelector('.nw'); await sleep(500); await p.keyboard.press('Escape'); await sleep(200);
  ok(JSON.stringify(await d(z)) === JSON.stringify(da), 'and the daemon kept where they went');

  // --- a card further down moves alone, and so does the first one with ctrl
  before = await snap(all);
  await drag(m, 0, 80);
  ok(await moved(before, m) && !(await moved(before, a)) && !(await moved(before, z)), 'a card further down moves alone');
  before = await snap(all);
  await drag(a, 0, -60, 'Control');
  ok(await moved(before, a) && !(await moved(before, m)) && !(await moved(before, z)), 'ctrl+drag moves just the first card');

  // --- dropped on the archive, the first card files the whole flow
  const hb = await card(a).locator('.hd').boundingBox(), tb = await p.locator('#arch .abox').boundingBox();
  const sx = hb.x + 70, sy = hb.y + hb.height / 2;
  await p.mouse.move(sx, sy); await p.mouse.down();
  for (let i = 1; i <= 14; i++) { await p.mouse.move(sx + (tb.x + tb.width / 2 - sx) * i / 14, sy + (tb.y + tb.height / 2 - sy) * i / 14); await sleep(20); }
  ok((await p.locator('#arch .al').innerText()).includes('file 3 cards'), 'over the archive it says it files all 3');
  await p.mouse.up();
  ok(await waitFor(async () => (await p.locator('.nw').count()) === 0, 5000), 'let go: the whole flow leaves the canvas');
  ok(JSON.stringify(await p.evaluate(() => AR.groups.map(g => g.nodes.length))) === '[3]', 'filed as one group of 3');

  ok(!errs.length, 'no page errors ' + errs.join(' | '));
  await b.close();
  process.exit(fails ? 1 : 0);
})();
