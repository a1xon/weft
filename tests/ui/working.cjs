// The top bar's "N working": a click shows the next card that works (selected, in the middle of
// the view), in reading order, and comes round to the first again.
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
  const chip = () => p.locator('#tick .tk.wk');
  const sel = () => p.evaluate(() => [...S.sel]);
  const centred = id => p.evaluate(i => { const r = document.querySelector('.nw[data-id="' + i + '"]').getBoundingClientRect(); return Math.abs(r.left + r.width / 2 - innerWidth / 2) < 4; }, id);

  await p.goto(fs.readFileSync(W + '/url.txt', 'utf8').trim()); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400);
  await p.keyboard.type('Fix the slugify function. SLOW'); await p.keyboard.press('Control+Enter');
  await p.waitForSelector('.nw[data-id="n1"]');
  for (const [x, y, id, t] of [[300, 600, 'n2', 'Fix the wrap function. SLOW'], [1300, 250, 'n3', 'Write a README. SLOW']]) {
    await p.evaluate(() => select([]));
    await p.mouse.dblclick(x, y); await p.waitForSelector('.nw[data-id="' + id + '"] .ask-edit');
    await p.keyboard.type(t); await p.keyboard.press('Enter'); await sleep(300);
  }
  ok(await waitFor(async () => /^3 working$/.test(await chip().innerText().catch(() => ''))), 'the top bar says 3 working: ' + await chip().innerText().catch(() => '-'));
  const order = await p.evaluate(() => ['n1', 'n2', 'n3'].map(i => S.nodes.get(i)).sort((a, b) => a.y - b.y || a.x - b.x).map(n => n.id));
  await p.evaluate(() => select([]));
  const seen = [];
  for (let i = 0; i < 4; i++) {
    await chip().click(); await sleep(150);
    const s = await sel(); seen.push(s.join(','));
    ok(s.length === 1 && await centred(s[0]), 'click ' + (i + 1) + ' selects ' + s + ' and brings it to the middle');
  }
  ok(seen.slice(0, 3).join(' ') === order.join(' '), 'the clicks go through them top to bottom (' + seen.slice(0, 3) + ' vs ' + order + ')');
  ok(seen[3] === seen[0], 'and come round to the first again');
  await p.screenshot({ path: W + '/working.png' });

  ok(errs.length === 0, 'no page errors' + (errs.length ? ': ' + errs.join(' | ') : ''));
  await b.close();
  console.log(fails ? `\n${fails} FAILED` : '\nall passed'); process.exit(fails ? 1 : 0);
})();
