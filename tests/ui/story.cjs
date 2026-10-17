const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs'); const W = process.env.SANDBOX;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let gestures = 0, chars = 0; const log = []; let fails = 0;
const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const p = await (await b.newContext({ viewport: { width: 1700, height: 900 } })).newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type() === 'error' && errs.push(m.text()));
  const G = async (what, fn) => { gestures++; log.push(gestures + '. ' + what); await fn(); await sleep(260); };
  const T = async s => { chars += s.length; log.push('   type ' + JSON.stringify(s.slice(0, 54)) + (s.length > 54 ? '…' : '')); await p.keyboard.type(s); };
  const st = id => p.locator('.nw[data-id="' + id + '"]').getAttribute('data-st');
  await p.goto(fs.readFileSync(W + '/url.txt', 'utf8').trim()); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(500);

  // 1. first agent: the launch pad greets you with the folder and a focused prompt: type, enter
  ok(await p.evaluate(() => document.activeElement.tagName === 'TEXTAREA' && !!document.activeElement.closest('#pad')), 'the launch pad opens with its prompt focused');
  await T('Fix the slugify function in textkit/slug.py so that tests/test_slug.py passes. Keep the public API.');
  await G('press ctrl+enter (creates the agent in the middle and runs it)', () => p.keyboard.press('Control+Enter'));
  await p.waitForSelector('.nw[data-id="n1"]');
  await sleep(500);
  ok((await st('n1')) === 'busy' || (await st('n1')) === 'starting', 'enter runs it (' + await st('n1') + ')');
  ok((await p.locator('.nw[data-id="n1"] .nm').innerText()) === 'Fix the slugify function', 'the agent is titled from its prompt: "' + await p.locator('.nw[data-id="n1"] .nm').innerText() + '"');

  // 2. second agent in parallel, left of the first (which the pad put in the middle)
  await G('double-click the canvas', () => p.mouse.dblclick(300, 330));
  await p.waitForSelector('.nw[data-id="n2"] .ask-edit');
  await T('Fix the wrap function in textkit/wrap.py so that tests/test_wrap.py passes. Keep the public API.');
  await G('press enter', () => p.keyboard.press('Enter'));
  await sleep(300);
  await p.screenshot({ path: W + '/v3-1-two-agents.png' });

  // 3. drag from agent 1's diamond to empty space with alt: choose a shell command there
  const o1 = await p.locator('.nw[data-id="n1"] .port.out').boundingBox();
  await G('alt-drag from the diamond to empty space', async () => {
    await p.keyboard.down('Alt');
    await p.mouse.move(o1.x + 6, o1.y + 6); await p.mouse.down(); await p.mouse.move(o1.x + 8, o1.y + 300, { steps: 8 });
    await p.screenshot({ path: W + '/v3-2-dragging.png' });
    await p.mouse.up(); await p.keyboard.up('Alt');
  });
  ok(await p.locator('.menu .mi', { hasText: 'shell' }).count() > 0, 'alt-drop offers the types');
  await G('choose "$ shell"', () => p.locator('.menu .mi', { hasText: '$ shell' }).first().click());
  await p.waitForSelector('.nw[data-id="n3"] .ask-edit');
  await T('python3 -m unittest discover -s tests -q');
  await G('press enter', () => p.keyboard.press('Enter'));

  // 4. link agent 2 into the test run: a visible Promise.all appears
  const o2 = await p.locator('.nw[data-id="n2"] .port.out').boundingBox();
  const h3 = await p.locator('.nw[data-id="n3"] .hd').boundingBox();
  await G('drag agent 2\'s diamond onto the test node', async () => {
    await p.mouse.move(o2.x + 6, o2.y + 6); await p.mouse.down(); await p.mouse.move(h3.x + 120, h3.y + 14, { steps: 8 }); await p.mouse.up();
  });
  await sleep(500);
  ok(await p.locator('#juncG .jn').count() === 1, 'two links into one node draw a junction');
  ok((await p.locator('#juncG .jn .gl').textContent()) === '&', 'the junction is an AND gate (Promise.all)');

  // 5. commit & push: right-click the canvas, place it
  await G('right-click empty canvas', () => p.mouse.click(1250, 130, { button: 'right' }));
  await G('choose "push"', () => p.locator('.menu .mi', { hasText: /^push/ }).click());
  await p.waitForSelector('.nw[data-kind=git]');
  ok((await p.locator('.nw[data-kind=git]').getAttribute('data-st')) === 'draft', 'the push node starts disarmed');
  const o3 = await p.locator('.nw[data-id="n3"] .port.out').boundingBox();
  const gh = await p.locator('.nw[data-kind=git] .hd').boundingBox();
  await G('drag the test node onto commit & push', async () => {
    await p.mouse.move(o3.x + 6, o3.y + 6); await p.mouse.down(); await p.mouse.move(gh.x + 100, gh.y + 14, { steps: 8 }); await p.mouse.up();
  });
  await sleep(400);
  // only push when the tests pass: click the link's chip until it shows the check mark
  const chip = p.locator('#edgeG .ed').last().locator('.chip');
  await G('click the link chip (always -> only if ok)', async () => { const bb = await chip.boundingBox(); await p.mouse.click(bb.x + bb.width / 2, bb.y + bb.height / 2); });
  await sleep(300);
  ok((await p.locator('#edgeG .ed').last().getAttribute('data-k')) === 'ok', 'the chip switched the link to "only if it succeeds"');
  await p.screenshot({ path: W + '/v3-3-built.png' });

  // 6. arm it, let everything finish
  await G('click arm', () => p.locator('.nw[data-kind=git] .fb.cy').click());
  for (let i = 0; i < 80; i++) { const g = await p.locator('.nw[data-kind=git]').getAttribute('data-st'); if (g === 'done' || g === 'failed') break; await sleep(500); }
  await sleep(600);
  await p.screenshot({ path: W + '/v3-4-done.png' });
  console.log(log.join('\n'));
  console.log('\nGESTURES=' + gestures + ' CHARS=' + chars + ' ERRORS=' + JSON.stringify(errs));
  console.log('states:', await p.$$eval('.nw', ns => ns.map(n => n.dataset.id + ':' + n.dataset.kind + ':' + n.dataset.st).join('  ')));
  ok(errs.length === 0, 'no page errors');
  await b.close();
  console.log(fails ? fails + ' FAILED' : 'ALL PASSED');
})().catch(e => { console.error('CRASH', String(e).slice(0, 900)); process.exit(1); });
