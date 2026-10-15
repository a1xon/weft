// The live output on a card is a peek: a click opens the whole terminal, large.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs');
const W = process.env.SANDBOX;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const ctx = await b.newContext({ viewport: { width: 1600, height: 900 } });
  const p = await ctx.newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message));
  const waitFor = async (fn, t = 30000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (await fn()) return true; await sleep(200); } return false; };
  await p.goto(fs.readFileSync(W + '/url.txt', 'utf8').trim()); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400);
  await p.keyboard.press('Escape'); await sleep(200);

  await p.evaluate(() => send('node_add', { node: { name: 'looker', agent: 'claude', prompt: 'LONG=20 count slowly and tell me', x: 300, y: 200 }, run: true }));
  ok(await waitFor(async () => (await p.locator('.nw[data-id="n1"]').count()) === 1, 8000), 'an agent is on the canvas');
  ok(await waitFor(async () => ['busy', 'starting'].includes(await p.locator('.nw[data-id="n1"]').getAttribute('data-st')), 40000), 'and works');
  // its live output is a lens on the card
  await p.locator('.nw[data-id="n1"] .lt', { hasText: 'live' }).click();
  ok(await p.locator('.nw[data-id="n1"] .live').isVisible(), 'the live output is shown');
  const small = await p.locator('.nw[data-id="n1"] .pvin').evaluate(e => parseFloat(getComputedStyle(e).fontSize));
  const fs1 = await p.evaluate(() => parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--fs')) || 1);
  ok(small >= 10 * fs1 && small < 12 * fs1, 'its text is small but readable on the card (' + small + 'px)');
  ok((await p.locator('.nw[data-id="n1"] .live').evaluate(e => getComputedStyle(e).cursor)) === 'zoom-in', 'and says it opens bigger');
  // one click opens the whole terminal, large
  await p.locator('.nw[data-id="n1"] .live').click();
  ok(await waitFor(async () => await p.locator('#focus').isVisible(), 5000), 'a click on it opens the terminal');
  await waitFor(async () => (await p.locator('#term .r').count()) > 5, 5000);
  const big = await p.locator('#term').evaluate(e => parseFloat(getComputedStyle(e).fontSize));
  ok(big >= 16, 'at a size that is made to be read (' + big.toFixed(1) + 'px)');
  const box = await p.locator('#term').boundingBox();
  ok(box.width > 900 && box.y + box.height <= 900, 'filling the window without leaving it (' + Math.round(box.width) + 'x' + Math.round(box.height) + ')');
  await p.screenshot({ path: W + '/peek-large.png' });
  await p.locator('#focus .btn', { hasText: 'back to canvas' }).click();
  ok(await p.locator('#focus').isHidden(), 'and it closes again');
  // a busy agent repaints the peek between press and release: it still opens (a 'click' would not fire)
  const lb = await p.locator('.nw[data-id="n1"] .live').boundingBox();
  await p.mouse.move(lb.x + lb.width / 2, lb.y + lb.height - 12); await p.mouse.down(); await sleep(120);
  await p.locator('.nw[data-id="n1"] .pvin').evaluate(e => { e.innerHTML = '<div>repainted</div>'.repeat(12); });
  await p.mouse.up();
  ok(await waitFor(async () => await p.locator('#focus').isVisible(), 3000), 'a press held while the peek repaints still opens the terminal');
  await p.locator('#focus .btn', { hasText: 'back to canvas' }).click();

  // --- the card itself: wider by default, and resized by its corner for more of the terminal
  await p.evaluate(() => send('node_add', { node: { name: 'printer', agent: 'sh', cmd: 'seq 1 60; sleep 120', x: 780, y: 150 }, run: true }));
  ok(await waitFor(async () => (await p.locator('.nw[data-id="n2"]').count()) === 1, 8000), 'a shell that prints sixty lines is on the canvas');
  const card = p.locator('.nw[data-id="n2"]'), grip = p.locator('.nw[data-id="n2"] .grip');
  const rows = () => p.evaluate(() => ((S.rt.get('n2') || {}).pv || []).length);
  ok(await waitFor(async () => (await rows()) === 12, 20000), 'a card of the default size shows 12 rows of its terminal (' + await rows() + ')');
  const k = await p.evaluate(() => S.v.k), b0 = await card.boundingBox();
  ok(Math.round(b0.width / k) === 500, 'a card is 500 wide by default (' + Math.round(b0.width / k) + ')');
  let g = await grip.boundingBox();
  await p.mouse.move(g.x + g.width / 2, g.y + g.height / 2); await sleep(250);
  ok(await grip.evaluate(e => getComputedStyle(e).opacity) === '1', 'pointing at its corner shows the grip');
  await p.mouse.down(); await p.mouse.move(g.x + 150 * k, g.y + 200 * k, { steps: 8 }); await p.mouse.up(); await sleep(400);
  const b1 = await card.boundingBox();
  ok(b1.width > b0.width + 120 * k && b1.height > b0.height + 170 * k, 'dragging the grip makes the card bigger (' + Math.round(b1.width / k) + 'x' + Math.round(b1.height / k) + ')');
  const port = await p.locator('.nw[data-id="n2"] .port.out').boundingBox();
  ok(Math.abs(port.x + port.width / 2 - (b1.x + b1.width)) < 4, 'its out port moves with its edge');
  const saved = () => { const n = JSON.parse(fs.readFileSync(W + '/proj/.weft/flow.json', 'utf8')).nodes.find(x => x.id === 'n2'); return n && n.w >= 550 && n.h >= 420; };
  ok(await waitFor(async () => saved(), 5000), 'the size is kept in the flow');
  ok(await waitFor(async () => (await rows()) > 20, 10000), 'and the taller card gets more rows of the terminal (' + await rows() + ')');
  ok(await p.locator('#focus').isHidden(), 'resizing does not open the terminal');
  await p.screenshot({ path: W + '/peek-resized.png' });
  g = await grip.boundingBox();
  await p.mouse.dblclick(g.x + g.width / 2, g.y + g.height / 2); await sleep(400);
  const b2 = await card.boundingBox();
  ok(Math.round(b2.width / k) === 500 && Math.round(b2.height / k) === 260 && await p.locator('#focus').isHidden(), 'a double-click on the grip puts it back to the default size (' + Math.round(b2.width / k) + 'x' + Math.round(b2.height / k) + ')');
  ok(await waitFor(async () => !saved(), 5000), 'and the flow forgets the size');
  ok(errs.length === 0, 'no page errors' + (errs.length ? ': ' + errs.join(' | ') : ''));
  await b.close();
  console.log(fails ? fails + ' FAILED' : 'ALL PASSED');
  process.exit(fails ? 1 : 0);
})();
