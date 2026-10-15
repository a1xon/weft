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

  // --- the wheel in the terminal: weft's history of a plain shell
  await p.locator('.nw[data-id="n2"] .live').click();
  ok(await waitFor(async () => await p.locator('#focus').isVisible(), 5000), 'the printer opens large');
  const termText = () => p.locator('#term').innerText();
  ok(!/^1$/m.test(await termText()), 'its first line has scrolled away');
  const tb = await p.locator('#term').boundingBox();
  await p.mouse.move(tb.x + tb.width / 2, tb.y + tb.height / 2);
  for (let i = 0; i < 12; i++) { await p.mouse.wheel(0, -120); await sleep(30); }
  ok(await waitFor(async () => /^1$/m.test(await termText()), 5000), 'the wheel scrolls back to it');
  ok(await p.locator('#scrollbadge').isVisible(), 'and offers the way back');
  await p.locator('#scrollbadge').click();
  ok(await waitFor(async () => /^60$/m.test(await termText()) && await p.locator('#scrollbadge').isHidden(), 5000), 'which shows the latest again');
  await p.locator('#focus .btn', { hasText: 'back to canvas' }).click();
  // a program that reads the mouse (Claude Code on its alternate screen) gets the wheel itself
  await p.evaluate(() => send('node_add', { node: { name: 'mouser', agent: 'sh', x: 1350, y: 150,
    cmd: "printf '\\033[?1049h\\033[?1003h\\033[?1006hmouse on\\n'; stty raw -echo; dd bs=1 count=12 2>/dev/null | od -An -c; sleep 120" }, run: true }));
  ok(await waitFor(async () => /mouse on/.test(await p.evaluate(() => ((S.rt.get('n3') || {}).pv || []).join('\n'))), 20000), 'a program that asks for the mouse runs');
  await p.locator('.nw[data-id="n3"] .live').click();
  ok(await waitFor(async () => await p.locator('#focus').isVisible(), 5000), 'it opens large');
  const mb = await p.locator('#term').boundingBox();
  await p.mouse.move(mb.x + mb.width / 2, mb.y + mb.height / 2);
  await p.mouse.wheel(0, -120);
  ok(await waitFor(async () => /033\s+\[\s+<\s+6\s+4\s+;/.test(await termText()), 5000), 'the wheel reaches it as a mouse wheel event (' + (await termText()).replace(/\s+/g, ' ').trim().slice(0, 80) + ')');
  ok(await p.locator('#scrollbadge').isVisible(), 'and "jump to latest" is offered (it sends ctrl+end)');
  await p.locator('#focus .btn', { hasText: 'back to canvas' }).click();
  // --- zoomed in, the cards are drawn at that size, not a stretched picture of them. Only a full
  // screenshot shows it (an element screenshot makes Chrome draw afresh); the reference is the same
  // view with the layer forced away, which always draws at the real size
  const zoomed = async () => {
    await p.evaluate(() => { S.v = { x: 0, y: 0, k: 0.4 }; applyView(); }); await sleep(600);
    const nb = await p.locator('.nw[data-id="n1"] .nm').boundingBox();
    await p.mouse.move(nb.x + 4, nb.y + nb.height / 2);
    for (let i = 0; i < 30; i++) { await p.mouse.wheel(0, -100); await sleep(16); }
    await sleep(900);
    const r = await p.locator('.nw[data-id="n1"] .ask').boundingBox();
    return { x: Math.max(0, Math.round(r.x)), y: Math.max(60, Math.round(r.y)), width: Math.min(700, Math.round(r.width)), height: Math.min(120, Math.round(r.height)) };
  };
  const crop = async c => {
    const png = await p.screenshot();
    return p.evaluate(async ([b64, c]) => {
      const img = new Image(); img.src = 'data:image/png;base64,' + b64; await img.decode();
      const cv = document.createElement('canvas'); cv.width = c.width; cv.height = c.height;
      const x = cv.getContext('2d'); x.drawImage(img, -c.x, -c.y); return Array.from(x.getImageData(0, 0, c.width, c.height).data);
    }, [png.toString('base64'), c]);
  };
  const differ = (a, b) => { let d = 0; for (let i = 0; i < a.length; i += 4) if (Math.abs(a[i] - b[i]) + Math.abs(a[i + 1] - b[i + 1]) + Math.abs(a[i + 2] - b[i + 2]) > 24) d++; return d; };
  const fresh = async c => { const t = await p.addStyleTag({ content: '#world{will-change:auto!important}' }); await sleep(600); const px = await crop(c); await t.evaluate(e => e.remove()); return px; };
  // the old way (a layer all the time) is caught by this check
  let old = await p.addStyleTag({ content: '#world{will-change:transform!important}' });
  let c = await zoomed();
  const stretched = await crop(c);
  await old.evaluate(e => e.remove());
  const dOld = differ(stretched, await fresh(c));
  c = await zoomed();
  ok(await p.evaluate(() => S.v.k) === 2 && !(await p.locator('#world').getAttribute('class') || '').includes('moving'), 'zoomed to 200%, the canvas is at rest');
  const dNew = differ(await crop(c), await fresh(c));
  ok(dOld > 200 && dNew < 40, 'its text is drawn at 200%, as sharp as drawn afresh (' + dNew + ' pixels differ; a layer kept at the old scale: ' + dOld + ')');
  await p.evaluate(() => fit());

  ok(errs.length === 0, 'no page errors' + (errs.length ? ': ' + errs.join(' | ') : ''));
  await b.close();
  console.log(fails ? fails + ' FAILED' : 'ALL PASSED');
  process.exit(fails ? 1 : 0);
})();
