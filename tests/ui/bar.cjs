// The prompt bar at the bottom always carries the choices: agent, model, reasoning, yolo.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs');
const W = process.env.SANDBOX;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const p = await (await b.newContext({ viewport: { width: 1500, height: 900 } })).newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type() === 'error' && errs.push(m.text()));
  const waitFor = async (fn, t = 15000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (await fn()) return true; await sleep(150); } return false; };
  const seg = () => p.locator('#omni .oc .peff .seg button').allInnerTexts();
  const running = () => p.evaluate(() => document.getAnimations().filter(a => { const t = a.effect && a.effect.target; return t && t.closest && (t.closest('#omni') || t.closest('.menu')); }).length);

  await p.goto(fs.readFileSync(W + '/url.txt', 'utf8').trim()); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400);
  await p.keyboard.press('Escape'); await sleep(250);

  // --- with nothing selected the bar is for a new agent: all the choices are there
  ok(await p.locator('#omni .oc .pagents').count() === 1, 'the bar has the agent switch');
  ok(await p.locator('#omodel').count() === 1, 'and the model');
  ok(await p.locator('#omni .oc .peff').count() === 1, 'and the reasoning level');
  ok(await p.locator('#oyolo').count() === 1, 'and yolo');
  const r1 = await p.evaluate(() => { const a = document.querySelector('#omni .oc .pagents').getBoundingClientRect(); const m = document.querySelector('#omodel').getBoundingClientRect(); const y = document.querySelector('#oyolo').getBoundingClientRect(); const t = document.querySelector('#omni textarea').getBoundingClientRect(); return { a: a.top, m: m.top, y: y.top, t: t.top }; });
  ok(Math.abs(r1.a - r1.m) < 10 && Math.abs(r1.a - r1.y) < 10 && r1.t > r1.a + 14, 'on one line above the prompt');
  const names = await seg();
  ok(names.length === 6 && names.includes('xhigh'), 'claude: auto low med high xhigh max (' + names.join(' ') + ')');

  // --- agy: its reasoning is low / medium / high, and only with its own default model
  await p.locator('#omni .oc .pagents button', { hasText: /^agy$/ }).click();
  await waitFor(async () => (await p.locator('#omni .oc .pagents button[aria-pressed=true]').innerText()) === 'agy');
  ok((await running()) === 0, 'switching agent animates nothing');
  const agyNames = await seg();
  ok(agyNames.join(' ') === 'auto low med high', 'agy offers auto low med high (' + agyNames.join(' ') + ')');
  await p.locator('#omodel').click();
  const menu = p.locator('.menu .mi', { hasText: 'Sim Deep' });
  await menu.waitFor({ timeout: 8000 });
  const mbox = await p.locator('.menu').boundingBox(), bbox = await p.locator('#omodel').boundingBox();
  ok(mbox.y + mbox.height <= bbox.y + 2, 'the model menu opens upward, clear of the bar');
  await menu.click();
  await waitFor(async () => (await p.locator('#omodel b').innerText()) === 'Sim Deep');
  ok((await p.locator('#omni .oc .peff').count()) === 0, 'with a model of its own agy has no separate reasoning choice (the level is in the model)');
  await p.locator('#omodel').click();
  await p.locator('.menu .mi', { hasText: 'default' }).first().click();
  await waitFor(async () => (await p.locator('#omni .oc .peff').count()) === 1);
  ok((await p.locator('#omni .oc .peff').count()) === 1, 'back to its default model the reasoning choice is back');

  // --- codex
  await p.locator('#omni .oc .pagents button', { hasText: /^codex$/ }).click();
  await waitFor(async () => (await seg()).length === 6);
  ok((await seg()).length === 6, 'codex: six levels again');
  await p.locator('#omni .oc .peff .seg button', { hasText: /^high$/ }).click();
  ok((await p.locator('#omni .oc .peff .seg button[aria-pressed=true]').innerText()) === 'high', 'one click sets it');
  ok(await p.locator('#omni textarea').evaluate(e => document.activeElement === e), 'and the prompt keeps the keyboard');

  // --- a new agent takes exactly those choices
  await p.locator('#omni textarea').fill('Say hello in one word.');
  await p.keyboard.press('Enter');
  await p.waitForSelector('.nw[data-id="n1"]');
  await sleep(600);
  const flow = JSON.parse(fs.readFileSync(W + '/proj/.weft/flow.json', 'utf8'));
  const n1 = flow.nodes.find(n => n.id === 'n1');
  ok(n1.agent === 'codex' && n1.effort === 'high', 'it was created as codex with reasoning high (' + n1.agent + ', ' + n1.effort + ')');
  ok(n1.yolo === true, 'in yolo mode');

  // --- with the agent selected the bar shows its own settings, and changing them changes the agent
  await p.locator('.nw[data-id="n1"] .hd').click();
  await waitFor(async () => (await p.locator('#omni .oc .tgt').count()) === 1);
  ok(await p.locator('#omni .oc .pagents').count() === 0, 'a selected agent has no agent switch: it is what it is');
  ok((await p.locator('#omni .oc .peff .seg button[aria-pressed=true]').innerText()) === 'high', 'the bar shows that agent\'s reasoning (high)');
  await p.locator('#omni .oc .peff .seg button', { hasText: /^low$/ }).click();
  await sleep(500);
  const flow2 = JSON.parse(fs.readFileSync(W + '/proj/.weft/flow.json', 'utf8'));
  ok(flow2.nodes.find(n => n.id === 'n1').effort === 'low', 'one click changes it for that agent (applies when it starts)');
  await p.locator('#oyolo').click();
  await sleep(500);
  const flow3 = JSON.parse(fs.readFileSync(W + '/proj/.weft/flow.json', 'utf8'));
  ok(flow3.nodes.find(n => n.id === 'n1').yolo === false, 'and so does yolo');

  // --- two agents: nothing to choose for both
  await p.mouse.dblclick(400, 300); await p.waitForSelector('.ask-edit'); await p.keyboard.type('x'); await p.mouse.click(1200, 600); await sleep(300);
  await p.locator('.nw[data-id="n1"] .nm').click();
  await p.locator('.nw[data-id="n2"] .nm').click({ modifiers: ['Shift'] });
  await waitFor(async () => (await p.evaluate(() => S.sel.size)) === 2, 4000);
  await waitFor(async () => (await p.locator('#omni .oc .tgt').first().innerText().catch(() => '')).includes('2 agents'), 4000);
  const two = await p.evaluate(() => ({ sel: [...S.sel].join(','), bar: document.querySelector('#omni').innerText.replace(/\s+/g, ' ') }));
  ok(await p.locator('#omni .oc .pmodel').count() === 0 && two.bar.includes('2 agents'), 'with two selected the bar just says so (' + two.sel + ': ' + two.bar.slice(0, 60) + ')');

  await p.screenshot({ path: W + '/bar-1.png' });
  ok(errs.length === 0, 'no page errors' + (errs.length ? ': ' + errs.slice(0, 3).join(' | ') : ''));
  await b.close();
  console.log(fails ? fails + ' FAILED' : 'BAR OK');
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); process.exit(2); });
