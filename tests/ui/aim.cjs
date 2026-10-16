// The bar says which card it talks to (chip + a tag on the card), lets go of it after 30 s
// untouched, and a prompt for a busy agent becomes a queued-prompt card linked after its work.
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
  const st = id => p.locator('.nw[data-id="' + id + '"]').getAttribute('data-st');
  const sel = () => p.evaluate(() => [...S.sel]);
  const aimed = () => p.evaluate(() => document.querySelector('#omniw').classList.contains('aimed'));
  const tag = id => p.evaluate(i => { const t = document.querySelector(`.nw[data-id="${i}"] .aimtag`); return t && !t.hidden ? t.textContent : ''; }, id);
  const steps = () => p.evaluate(() => [...S.nodes.values()].filter(n => n.agent === 'step').map(n => ({ id: n.id, host: n.host, prompt: n.prompt, from: [...S.edges.values()].filter(e => e.to === n.id).map(e => e.from) })));

  await p.goto(fs.readFileSync(W + '/url.txt', 'utf8').trim()); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400);
  await p.keyboard.type('Explain what textkit/slug.py does in one sentence.'); await p.keyboard.press('Control+Enter');
  await p.waitForSelector('.nw[data-id="n1"]');
  await p.locator('.nw[data-id="n1"] .hd').click();
  ok(await waitFor(async () => ['busy', 'starting'].includes(await st('n1'))), 'the agent works');

  // --- what the bar talks to is plain to see
  ok(await aimed(), 'with a card selected the bar is marked as aimed at it');
  ok((await p.locator('#omni .tgt').innerText()).includes('n1') || (await p.locator('#omni .tgt').innerText()).length > 2, 'its chip names the card (' + await p.locator('#omni .tgt').innerText() + ')');
  ok((await tag('n1')).includes('prompt bar'), 'the card carries a tag: ' + await tag('n1'));
  ok(/working/.test(await p.locator('#omni textarea').getAttribute('placeholder')), 'the prompt says the agent is working: ' + await p.locator('#omni textarea').getAttribute('placeholder'));
  await p.screenshot({ path: W + '/aim-1-aimed.png' });

  // --- a prompt for the busy agent is a card linked after it, the next one after that card
  await p.locator('#omni textarea').click();
  await p.keyboard.type('Now explain textkit/wrap.py in one sentence.'); await p.keyboard.press('Control+Enter');
  ok(await waitFor(async () => (await steps()).length === 1), 'queueing from the bar makes a visible queued-prompt card');
  const [s1] = await steps();
  ok(s1 && s1.host === 'n1' && s1.from.join() === 'n1' && s1.prompt.includes('wrap.py'), 'asked of the same agent, linked after it, with the prompt typed (' + JSON.stringify(s1) + ')');
  ok(!(await p.locator('#qpanel').innerText().catch(() => '')).includes('wrap.py'), 'nothing is queued out of sight');
  ok((await sel()).join() === 'n1', 'the bar stays on the agent, so more can be queued');
  await p.keyboard.type('Then say done.'); await p.keyboard.press('Control+Enter');
  ok(await waitFor(async () => (await steps()).length === 2), 'a second prompt makes a second card');
  const s2 = (await steps()).find(s => s.id !== s1.id);
  ok(s2 && s2.from.join() === s1.id && s2.host === 'n1', 'linked after the first queued prompt (' + JSON.stringify(s2) + ')');
  await p.screenshot({ path: W + '/aim-2-queued.png' });
  ok(await waitFor(async () => (await st(s1.id)) === 'done' && (await st(s2.id)) === 'done', 60000), 'both are asked in turn and answered (' + await st(s1.id) + ' ' + await st(s2.id) + ')');

  // --- untouched for 30 s the bar lets go
  await p.evaluate(() => centerOn('n1')); await sleep(200);
  await p.locator('.nw[data-id="n1"] .hd').click();
  await p.mouse.move(1500, 120); await p.evaluate(() => document.activeElement && document.activeElement.blur());
  await sleep(21500);
  ok(/lets go in \d+ s/.test(await tag('n1')) && (await sel()).join() === 'n1', 'ten seconds before, the tag counts down: ' + await tag('n1'));
  await sleep(9500);
  ok((await sel()).length === 0 && !(await aimed()) && (await tag('n1')) === '', 'after 30 s untouched nothing is selected and the bar starts a new agent again');
  ok((await p.locator('#omni .pfoot .btn').innerText()) === 'start', 'its button says start');

  // --- touching the card or holding text in the bar keeps it
  await p.locator('.nw[data-id="n1"] .hd').click();
  await p.evaluate(() => { aimAt = Date.now() - 29000; });
  await p.locator('.nw[data-id="n1"]').hover(); await sleep(1600);
  ok((await sel()).join() === 'n1' && !/lets go/.test(await tag('n1')), 'pointing at the card counts as using it');
  await p.locator('#omni textarea').click(); await p.keyboard.type('half a thought');
  await p.mouse.move(1500, 120);
  await p.evaluate(() => { aimAt = Date.now() - 31000; }); await sleep(1600);
  ok((await sel()).join() === 'n1', 'text waiting in the bar holds the selection');
  await p.locator('#omni textarea').fill('');

  // --- ✕ lets go at once
  await p.locator('#omni .tgx').click();
  ok((await sel()).length === 0 && await p.locator('#omni textarea').evaluate(e => document.activeElement === e), '✕ on the chip lets go and the prompt keeps the keyboard');

  console.log('errors:', JSON.stringify(errs));
  ok(errs.length === 0, 'no page errors');
  await b.close();
  console.log(fails ? fails + ' FAILED' : 'ALL PASSED');
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error('CRASH', String(e).slice(0, 1200)); process.exit(1); });
