// The prompt bar at the bottom always carries the choices: agent, model, reasoning, yolo, and it is
// the launch pad's own prompt box (same component, compact) with the same keys and addenda.
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
  const padBox = await p.evaluate(() => ({ h: document.querySelector('#pbox').getBoundingClientRect().height, font: parseFloat(getComputedStyle(document.querySelector('#pad textarea')).fontSize) }));
  await p.keyboard.press('Escape'); await sleep(250);

  // --- the bar is the pad's prompt box, compact
  ok(await p.locator('#omni .pbox.compact textarea').count() === 1 && await p.locator('#omni .pbox.chamf').count() === 1, 'the bar holds the same prompt box as the launch pad, in its compact form');
  const barBox = await p.evaluate(() => ({ h: document.querySelector('#omni .pbox').getBoundingClientRect().height, font: parseFloat(getComputedStyle(document.querySelector('#omni textarea')).fontSize) }));
  ok(barBox.h < padBox.h * 0.6 && barBox.font < padBox.font, `with less padding and smaller type (${Math.round(barBox.h)}px / ${barBox.font}px against ${Math.round(padBox.h)}px / ${padBox.font}px)`);
  const keys = await p.locator('#omni .pfoot kbd').allInnerTexts();
  ok(['ctrl+enter', 'enter', 'tab', '/'].every(k => keys.includes(k)), 'its footer lists the keys of the pad (' + keys.join(' ') + ')');
  ok((await p.locator('#omni .pfoot .btn').innerText()) === 'start', 'and has its start button');
  const chips = p.locator('#omni .pchips .chip:not(.add)');
  ok(await chips.count() >= 2 && await p.locator('#omni .pchips .chip.add').count() === 2, 'the standard addenda sit under it, with + add and prompts');
  const pressed = () => chips.nth(1).getAttribute('aria-pressed');
  const a0 = await pressed();
  await chips.nth(1).click();
  ok((await pressed()) !== a0 && await p.locator('#omni textarea').evaluate(e => document.activeElement === e), 'one click switches one on, and the prompt keeps the keyboard');
  await chips.nth(1).click();
  ok((await pressed()) === a0, 'and off again');

  // --- the prompt: two lines to begin with, a little larger type, expandable for editing several lines
  const taInfo = () => p.evaluate(() => { const t = document.querySelector('#omni textarea'), cs = getComputedStyle(t); return { h: t.getBoundingClientRect().height, line: parseFloat(cs.lineHeight), font: parseFloat(cs.fontSize), fs: parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--fs')) || 1 }; });
  const ta0 = await taInfo();
  ok(Math.abs(ta0.h - 2 * ta0.line) < 3, `the prompt is two lines tall (${Math.round(ta0.h)}px for ${Math.round(ta0.line)}px lines)`);
  ok(ta0.font / ta0.fs > 13.5 && ta0.font < padBox.font, `in slightly larger type than before and still smaller than on the pad (${ta0.font}px)`);
  await p.locator('#omni textarea').fill('a\nb\nc\nd\ne');
  ok((await taInfo()).h > ta0.h + ta0.line, 'it grows with the lines typed');
  await p.locator('#omni textarea').fill('');
  await p.locator('#omni .pfoot .xp').click();
  const big = await taInfo();
  ok(big.h > ta0.h * 3 && await p.locator('#omni textarea').evaluate(e => document.activeElement === e), `expand gives a tall box to edit several lines in (${Math.round(big.h)}px) and the prompt keeps the keyboard`);
  await p.locator('#omni .pfoot .xp').click();
  ok(Math.abs((await taInfo()).h - ta0.h) < 3, 'and it folds back to two lines');
  const cell = s => p.evaluate(q => { const cs = getComputedStyle(document.querySelector(q)); return [cs.paddingTop, cs.paddingLeft, cs.borderTopWidth, cs.fontSize].join(' '); }, s);
  ok((await cell('#omni .peff .seg button')) === (await cell('#omodel .seg button')) && parseFloat((await cell('#omni .peff .seg button'))) <= 3, 'reasoning is the same selector style as the model, with little padding (' + (await cell('#omni .peff .seg button')) + ')');

  // --- with nothing selected the bar is for a new agent: all the choices are there
  ok(await p.locator('#omni .oc .pagents').count() === 1, 'the bar has the agent switch');
  ok(await p.locator('#omodel').count() === 1, 'and the model');
  ok(await p.locator('#omni .oc .peff').count() === 1, 'and the reasoning level');
  ok(await p.locator('#oyolo').count() === 1, 'and yolo');
  const r1 = await p.evaluate(() => { const q = s => { const r = document.querySelector(s).getBoundingClientRect(); return { t: r.top, b: r.bottom, l: r.left, r: r.right, c: (r.top + r.bottom) / 2 }; }; return { a: q('#omni .oc .pagents'), m: q('#omodel'), e: q('#omni .oc .peff'), y: q('#oyolo'), t: q('#omni textarea') }; });
  ok(r1.m.l > r1.a.r && r1.e.l > r1.a.r && r1.m.b <= r1.e.t + 2, 'the agents sit on the left, the model first and the reasoning level second on the right');
  ok(Math.abs(r1.a.c - (r1.m.t + r1.e.b) / 2) < 8 && r1.y.t > r1.m.b - 2 && r1.y.l > r1.e.r && r1.t.t > r1.e.b, 'the agents are centred against both lines, yolo ends the second one, the prompt is below');
  ok(await p.locator('#omodel .seg button').count() >= 4 && await p.locator('#omodel select, #omodel .pmodel').count() === 0, 'the model is a row of one-click choices like the reasoning level, not a menu');
  ok((await p.locator('#omodel .seg button[aria-pressed=true]').innerText()) === 'default', 'default is the one selected');
  const names = await seg();
  ok(names.length === 6 && names.includes('xhigh'), 'claude: auto low med high xhigh max (' + names.join(' ') + ')');

  // alt+up / alt+down: the reasoning level from the keyboard, as on the pad
  const level = () => p.locator('#omni .oc .peff .seg button[aria-pressed=true]').innerText();
  await p.locator('#omni textarea').focus();
  await p.keyboard.press('Alt+ArrowUp');
  ok((await level()) === 'low', 'alt+up raises the reasoning level (' + (await level()) + ')');
  await p.keyboard.press('Alt+ArrowDown');
  ok((await level()) === 'auto', 'alt+down lowers it again');

  // --- enter is a new line, ctrl+enter sends
  await p.locator('#omni textarea').fill('');
  await p.keyboard.type('one'); await p.keyboard.press('Enter'); await p.keyboard.type('two'); await sleep(200);
  ok((await p.locator('#omni textarea').inputValue()) === 'one\ntwo' && (await p.evaluate(() => S.nodes.size)) === 0, 'a plain enter is a new line in the bar and sends nothing');
  await p.locator('#omni textarea').fill('');

  // --- agy: its reasoning is low / medium / high, and only with its own default model
  await p.locator('#omni .oc .pagents button', { hasText: /^agy$/ }).click();
  await waitFor(async () => (await p.locator('#omni .oc .pagents button[aria-pressed=true]').innerText()) === 'agy');
  ok((await running()) === 0, 'switching agent animates nothing');
  const agyNames = await seg();
  ok(agyNames.join(' ') === 'auto low med high', 'agy offers auto low med high (' + agyNames.join(' ') + ')');
  const deep = p.locator('#omodel .seg button', { hasText: /^Sim Deep$/ });
  await deep.waitFor({ timeout: 8000 });
  ok(await p.locator('#omodel .seg button', { hasText: /^Sim Fast$/ }).count() === 1, 'the models its CLI lists are all there, no click needed to see them');
  await deep.click();
  await waitFor(async () => (await p.locator('#omodel .seg button[aria-pressed=true]').innerText()) === 'Sim Deep');
  ok((await p.locator('#omodel .seg button[aria-pressed=true]').innerText()) === 'Sim Deep' && (await p.locator('#omni .oc .peff').count()) === 0, 'one click picks it; with a model of its own agy has no separate reasoning choice (the level is in the model)');
  await p.locator('#omodel .seg button', { hasText: /^default$/ }).click();
  await waitFor(async () => (await p.locator('#omni .oc .peff').count()) === 1);
  ok((await p.locator('#omni .oc .peff').count()) === 1, 'back to its default model the reasoning choice is back');

  // --- a model that is not listed: other… takes a name
  await p.locator('#omodel .other').click();
  await p.keyboard.type('my-own-model'); await p.keyboard.press('Enter');
  await waitFor(async () => (await p.locator('#omodel .seg button[aria-pressed=true]').innerText().catch(() => '')) === 'my-own-model', 4000);
  ok((await p.locator('#omodel .seg button[aria-pressed=true]').innerText()) === 'my-own-model', 'other… takes any model name and shows it selected');
  await p.locator('#omodel .seg button', { hasText: /^default$/ }).click();
  await waitFor(async () => (await p.locator('#omodel .seg button', { hasText: /^my-own-model$/ }).count()) === 0, 4000);

  // --- codex
  await p.locator('#omni .oc .pagents button', { hasText: /^codex$/ }).click();
  await waitFor(async () => (await seg()).length === 6);
  ok((await seg()).length === 6, 'codex: six levels again');
  await p.locator('#omni .oc .peff .seg button', { hasText: /^high$/ }).click();
  ok((await p.locator('#omni .oc .peff .seg button[aria-pressed=true]').innerText()) === 'high', 'one click sets it');
  ok(await p.locator('#omni textarea').evaluate(e => document.activeElement === e), 'and the prompt keeps the keyboard');

  // --- a new agent takes exactly those choices
  await p.locator('#omni textarea').fill('Say hello in one word.');
  await p.keyboard.press('Control+Enter');
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

  // --- a message to an agent that exists: same box and button, no addenda (they make a new agent)
  ok(await p.locator('#omni .pbox.compact .pfoot .btn').count() === 1 && await p.locator('#omni .pchips').count() === 0, 'a message to an agent has the same box and button, without the addenda row');
  await p.locator('#omni textarea').click(); await p.keyboard.press('Tab');
  await waitFor(async () => (await p.locator('#omni .oc .tgt').innerText()).includes('hand over'), 4000);
  ok(await p.locator('#omni .pchips').count() === 1 && (await p.locator('#omni .pfoot .btn').innerText()) === 'hand over', 'handing over makes a new agent: its addenda are back, the button says so');
  await p.keyboard.press('Tab');
  await waitFor(async () => !(await p.locator('#omni .oc .tgt').innerText()).includes('hand over'), 4000);

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
