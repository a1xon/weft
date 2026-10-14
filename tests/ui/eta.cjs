// More checks: the ETA countdown, what it changed, and a ping.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs');
const W = process.env.SANDBOX;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const ctx = await b.newContext({ viewport: { width: 1700, height: 900 } });
  const p = await ctx.newPage();
  // desktop notifications: record them instead of showing them
  await p.addInitScript(() => {
    window.__notes = [];
    window.Notification = function (title, opts) { window.__notes.push([title, (opts && opts.body) || '']); };
    window.Notification.permission = 'granted';
    window.Notification.requestPermission = async () => 'granted';
  });
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type() === 'error' && errs.push(m.text()));
  const st = id => p.locator('.nw[data-id="' + id + '"]').getAttribute('data-st');
  const waitSt = async (id, want, t = 60000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (want.includes(await st(id))) return true; await sleep(250); } return false; };
  const waitFor = async (fn, t = 30000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (await fn()) return true; await sleep(200); } return false; };
  const dropChip = async (kind, id) => {
    const chip = await p.locator('#tray .tchip[data-k=' + kind + ']').boundingBox();
    const hd = await p.locator('.nw[data-id="' + id + '"] .hd').boundingBox();
    await p.mouse.move(chip.x + chip.width / 2, chip.y + chip.height / 2); await p.mouse.down();
    await p.mouse.move(hd.x + 100, hd.y + 14, { steps: 12 }); await p.mouse.up();
  };

  await p.goto(fs.readFileSync(W + '/url.txt', 'utf8').trim()); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400);
  await p.keyboard.press('Escape'); await sleep(200);

  // --- the tray: six checks, one column, out of the way of the bar
  const chips = await p.locator('#tray .tchip').allInnerTexts();
  ok(['approve', 'screenshot', 'tests', 'ETA', 'changes', 'ping'].every(c => chips.some(t => t.trim() === c)), 'the tray offers approve, screenshot, tests, ETA, changes, ping (' + chips.map(t => t.trim()).join(' | ') + ')');
  const tb = await p.locator('#tray').boundingBox(), ob = await p.locator('#omniw').boundingBox();
  ok(tb.x + tb.width < ob.x || tb.y + tb.height < ob.y, 'and it does not sit on the prompt bar');

  // --- an agent that has not started yet gets an ETA, then runs: the first prompt carries the order
  await p.mouse.dblclick(500, 300);
  await p.waitForSelector('.ask-edit');
  await p.keyboard.type('LONG=18 count slowly and tell me');
  await p.mouse.click(1300, 700); await sleep(300);
  await dropChip('eta', 'n1');
  await waitFor(async () => (await p.evaluate(() => (S.nodes.get('n1').checks || []).length)) === 1, 5000);
  ok(await p.evaluate(() => (S.nodes.get('n1').checks || [])[0].kind) === 'eta', 'the ETA is attached to the agent');
  ok(await p.locator('.nw[data-id="n1"] .film').count() === 0 && await p.locator('.nw').count() === 1, 'it is a watch with no grid and no gate card');
  await p.evaluate(() => send('arm', { id: 'n1' }));   // enter would open the terminal of the selected card
  ok(await waitSt('n1', ['busy', 'starting']), 'it runs (' + await st('n1') + ')');
  ok(await waitFor(async () => (await p.locator('.nw[data-id="n1"] .eta').count()) === 1, 15000), 'a countdown appears on its card');
  ok(await waitFor(async () => /^\d+:\d\d$/.test((await p.locator('.nw[data-id="n1"] .eta .et').innerText()).trim()), 30000), 'it shows the estimate the agent gave');
  const v1 = (await p.locator('.nw[data-id="n1"] .eta .et').innerText()).trim();
  const sec = s => { const [m, x] = s.split(':').map(Number); return m * 60 + x; };
  await sleep(3200);
  const v2 = (await p.locator('.nw[data-id="n1"] .eta .et').innerText()).trim();
  ok(sec(v2) < sec(v1), 'it counts down (' + v1 + ' -> ' + v2 + ')');
  ok((await p.locator('.nw[data-id="n1"] .eta .en').innerText()).length > 0, 'with what the agent said it is doing: "' + await p.locator('.nw[data-id="n1"] .eta .en').innerText() + '"');
  const fs1 = await p.locator('.nw[data-id="n1"] .eta .et').evaluate(e => parseFloat(getComputedStyle(e).fontSize));
  const eb = await p.locator('.nw[data-id="n1"] .eta').boundingBox(), nb = await p.locator('.nw[data-id="n1"] .node').boundingBox();
  ok(fs1 >= 24, 'it is large (' + fs1 + 'px)');
  ok(Math.abs((eb.x + eb.width) - (nb.x + nb.width)) < 24 && eb.y < nb.y + 8, 'and sits at the top right of the card');
  ok(await p.locator('#etaTop').isVisible() && (await p.locator('#etaTop .et').evaluate(e => parseFloat(getComputedStyle(e).fontSize))) >= 20, 'the flow\'s number is in the top bar, large too: ' + (await p.locator('#etaTop').innerText()).replace(/\s+/g, ' '));
  await p.screenshot({ path: W + '/eta-1-countdown.png' });
  // the agent updates it halfway: the note changes
  ok(await waitFor(async () => (await p.locator('.nw[data-id="n1"] .eta .en').innerText()).includes('halfway'), 20000), 'at the halfway point the agent updated it');
  ok(await waitSt('n1', ['done']), 'it finishes (' + await st('n1') + ')');
  ok(await waitFor(async () => (await p.locator('.nw[data-id="n1"] .eta').count()) === 0, 5000), 'the countdown goes away with it');
  ok(await p.locator('#etaTop').isHidden(), 'and so does the top bar number');

  // --- what it changed: git, no questions asked
  await p.mouse.dblclick(500, 560);
  await p.waitForSelector('.nw[data-id="n2"] .ask-edit');
  await p.keyboard.type('Fix the slugify function in textkit/slug.py so that tests/test_slug.py passes. LONG=6');
  await p.mouse.click(1300, 760); await sleep(300);
  await dropChip('changes', 'n2');
  ok(await waitFor(async () => (await p.locator('.nw[data-id="n2"] .film').count()) === 1, 8000), 'a "what it changed" grid appears under the card');
  await p.evaluate(() => send('arm', { id: 'n2' }));
  ok(await waitSt('n2', ['done']), 'the agent works and finishes (' + await st('n2') + ')');
  ok(await waitFor(async () => (await p.locator('.nw[data-id="n2"] .film .ftxt.chg').innerText().catch(() => '')).includes('slug.py'), 20000), 'it lists the file the agent touched: ' + (await p.locator('.nw[data-id="n2"] .film .ftxt.chg').innerText().catch(() => '')).replace(/\n/g, ' | ').slice(0, 90));
  const sumLine = (await p.locator('.nw[data-id="n2"] .film .ftxt.chg .res').innerText()).trim();
  ok(/\d+ files? · \+\d+ −\d+/.test(sumLine), 'with the size of the change: ' + sumLine);
  await p.screenshot({ path: W + '/eta-2-changes.png' });
  await p.locator('.nw[data-id="n2"] .film .ftxt.chg').click();
  await p.waitForSelector('#shotbox:not([hidden]) .sbtxt');
  ok((await p.locator('#shotbox .sbtxt').innerText()).includes('slug.py'), 'a click opens the whole list');
  await p.keyboard.press('Escape');

  // --- ping: a notification when it is done
  await p.mouse.dblclick(900, 330);
  await p.waitForSelector('.nw[data-id="n3"] .ask-edit');
  await p.keyboard.type('LONG=3 say hello');
  await p.mouse.click(1400, 800); await sleep(300);
  await p.locator('.nw[data-id="n3"] .hd').click({ button: 'right' });
  await p.locator('.menu .mi', { hasText: 'ping me when it is done' }).click();
  await waitFor(async () => (await p.evaluate(() => (S.nodes.get('n3').checks || []).length)) === 1, 5000);
  await p.evaluate(() => send('arm', { id: 'n3' }));
  ok(await waitSt('n3', ['done']), 'the watched agent finishes');
  ok(await waitFor(async () => (await p.evaluate(() => window.__notes.length)) >= 1, 5000), 'a notification was raised');
  const note = await p.evaluate(() => window.__notes[0]);
  ok(/is done/.test(note[0]), 'it says what happened: "' + note[0] + '"');

  ok(errs.length === 0, 'no page errors' + (errs.length ? ': ' + errs.slice(0, 3).join(' | ') : ''));
  await b.close();
  console.log(fails ? fails + ' FAILED' : 'ETA OK');
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); process.exit(2); });
