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
  ok(small >= 12, 'its text is not tiny on the card (' + small + 'px)');
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
  ok(errs.length === 0, 'no page errors' + (errs.length ? ': ' + errs.join(' | ') : ''));
  await b.close();
  console.log(fails ? fails + ' FAILED' : 'ALL PASSED');
  process.exit(fails ? 1 : 0);
})();
