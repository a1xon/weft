// The launch pad: where am I, which agent / model / reasoning, what should it do.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs'); const cp = require('child_process');
const W = process.env.SANDBOX, BIN = process.env.WEFT_UI_BIN;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const p = await (await b.newContext({ viewport: { width: 1500, height: 900 } })).newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type() === 'error' && errs.push(m.text()));
  const url = fs.readFileSync(W + '/url.txt', 'utf8').trim();
  const st = id => p.locator('.nw[data-id="' + id + '"]').getAttribute('data-st');
  const open = async () => { await p.goto(url); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(450); };
  const pressed = async label => (await p.locator('#pad .seg button', { hasText: new RegExp('^' + label + '$') }).first().getAttribute('aria-pressed')) === 'true';

  await open();
  await p.screenshot({ path: W + '/pad-1-open.png' });

  // --- where am I
  const name = await p.locator('#pad .pname').innerText();
  const full = await p.locator('#pad .ppath').getAttribute('title');
  ok(name === 'proj' && full.endsWith('/proj'), 'the folder is the headline: "' + name + '" (' + full + ')');
  ok(await p.locator('#pad .pgit .br').innerText().then(t => t.trim() === 'main'), 'the branch is shown');
  ok((await p.locator('#pad .pgit').innerText()).includes('clean'), 'a clean tree says so');
  const ta = p.locator('#pad textarea');
  ok(await ta.evaluate(e => document.activeElement === e), 'the prompt is focused without a click');
  const box = await ta.boundingBox();
  ok(Math.abs(box.x + box.width / 2 - 750) < 40 && box.y > 250 && box.y < 520, 'the prompt sits in the middle of the screen (' + Math.round(box.x + box.width / 2) + ',' + Math.round(box.y) + ')');
  ok(box.height >= 120 && box.width > 600, 'and it is large (' + Math.round(box.width) + 'x' + Math.round(box.height) + ')');
  const pathBox = await p.locator('#pad .pname').boundingBox();
  ok(pathBox.y + pathBox.height < box.y, 'the path is above the prompt');
  ok((await ta.getAttribute('placeholder')).includes('proj'), 'the placeholder names the folder: ' + (await ta.getAttribute('placeholder')));

  // --- the choices are a grid: agents on the left (centred), model, then reasoning and yolo on the right
  const ab0 = await p.locator('#pad .pagents').boundingBox();
  const yb0 = await p.locator('#pyolo').boundingBox();
  const mb0 = await p.locator('#pmodel').boundingBox();
  const eb0 = await p.locator('#pad .peff').boundingBox().catch(() => null);
  ok(mb0.x > ab0.x + ab0.width && mb0.y + mb0.height <= yb0.y + 2, 'the model is on the first line to the right of the agents, yolo below it');
  ok(!eb0 || (Math.abs(eb0.y - yb0.y) < 12 && yb0.x > eb0.x + eb0.width), 'the reasoning level and yolo share the second line');
  const mid = (mb0.y + yb0.y + yb0.height) / 2;
  ok(Math.abs(ab0.y + ab0.height / 2 - mid) < 10, 'the agents are centred against the two lines (' + Math.round(ab0.y + ab0.height / 2) + ' vs ' + Math.round(mid) + ')');
  const running = () => p.evaluate(() => document.getAnimations().filter(a => { const t = a.effect && a.effect.target; return t && t.closest && (t.closest('#pad') || t.closest('.menu')); }).length);
  await sleep(450);
  ok((await running()) === 0, 'nothing is animating once the pad has opened');

  const cell = s => p.evaluate(q => { const cs = getComputedStyle(document.querySelector(q)); return [cs.paddingTop, cs.paddingLeft, cs.borderTopWidth, cs.fontSize].join(' '); }, s);
  ok((await cell('#pad .peff .seg button')) === (await cell('#pmodel .seg button')) && parseFloat((await cell('#pad .peff .seg button'))) <= 5, 'reasoning and model are the same selector style, with little padding (' + (await cell('#pad .peff .seg button')) + ')');
  await p.locator('#pad .pbox .xp').click();
  const bigBox = await p.locator('#pbox').boundingBox();
  ok(bigBox.height > 300, 'the prompt can be expanded here too (' + Math.round(bigBox.height) + 'px)');
  await p.locator('#pad .pbox .xp').click();

  // --- one click to switch agent
  const names = await p.locator('#pad .pagents button').allInnerTexts();
  ok(['claude', 'agy', 'codex'].every(n => names.includes(n)), 'claude, agy and codex are one click each (' + names.join(' | ') + ')');
  await p.locator('#pad .pagents button', { hasText: /^agy$/ }).click();
  ok((await running()) === 0, 'selecting an agent does not replay any animation');
  await sleep(350);
  ok((await running()) === 0, 'nor when its model list arrives');
  ok(await pressed('agy') && (await ta.getAttribute('placeholder')).includes('agy'), 'clicking agy selects it and the prompt follows');
  ok(await ta.evaluate(e => document.activeElement === e), 'the prompt keeps the keyboard');
  await p.keyboard.press('Tab');
  ok(await p.locator('#pad .pagents button[aria-pressed=true]').innerText().then(t => t !== 'agy'), 'tab cycles to the next agent');
  await p.locator('#pad .pagents button', { hasText: /^codex$/ }).click();

  // --- the model is a row of one-click choices, like the reasoning level
  await p.locator('#pmodel .seg button', { hasText: /^Sim Deep$/ }).waitFor({ timeout: 8000 });
  const items = await p.locator('#pmodel .seg button').allInnerTexts();
  ok(items.some(t => t.includes('default')) && items.some(t => t.includes('Sim Deep')) && await p.locator('.menu').count() === 0, 'it offers default and what the CLI lists, no menu to open (' + items.join(' | ') + ')');
  ok((await running()) === 0, 'the model row appears without an animation');
  await p.locator('#pmodel .seg button', { hasText: /^Sim Deep$/ }).click();
  ok((await running()) === 0, 'and choosing a model does not animate');
  ok((await p.locator('#pmodel .seg button[aria-pressed=true]').innerText()) === 'Sim Deep', 'the chosen model is the one selected');
  await p.locator('#pad .peff .seg button', { hasText: /^high$/ }).click();
  ok((await running()) === 0, 'choosing a reasoning level does not animate either');
  ok(await pressed('high'), 'one click sets the reasoning level');
  await p.keyboard.down('Alt'); await p.keyboard.press('ArrowUp'); await p.keyboard.up('Alt');
  await sleep(100);
  ok(await pressed('xhigh') || await ta.evaluate(e => document.activeElement === e), 'alt+up raises it from the keyboard');
  await p.locator('#pad .peff .seg button', { hasText: /^high$/ }).click();

  // --- the choice is remembered, server side
  await sleep(500);
  await p.reload(); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(500);
  ok(await pressed('codex'), 'after a reload the last agent is preselected');
  ok(await pressed('high') && (await p.locator('#pmodel .seg button[aria-pressed=true]').innerText()) === 'Sim Deep', 'and so are its model and reasoning level');
  await p.screenshot({ path: W + '/pad-2-codex.png' });

  // --- addenda: click to switch, curate in place
  const chip = t => p.locator('#pad .pchips .chip', { hasText: t }).first();
  const on = async t => (await chip(t).getAttribute('aria-pressed')) === 'true';
  ok(!(await on('Report with weft commands')), 'the report-with-commands addendum is off by default: the closing question does the job');
  ok(!(await on('Be concise')), 'others are off');
  await chip('Be concise').click();
  ok(await on('Be concise'), 'one click switches an addendum on');
  await p.locator('#pad .chip.add', { hasText: '+ add' }).click();
  await p.waitForSelector('#pad .aed');
  await p.keyboard.type('Run the tests'); await p.keyboard.press('Enter');
  await p.keyboard.type('Run the whole test suite before you say you are done.');
  await p.keyboard.press('Control+Enter');
  await p.waitForSelector('#pad .pchips');
  ok(await chip('Run the tests').count() > 0 && await on('Run the tests'), 'a new addendum is written in place and switched on');
  await sleep(500);
  await chip('Run the tests').click({ button: 'right' });
  await p.locator('.menu .mi', { hasText: 'remove' }).click();
  ok((await chip('Run the tests').count()) === 0, 'right-click removes it again');

  // --- empty prompt: nothing happens
  await ta.click(); await p.keyboard.press('Control+Enter');
  await sleep(250);
  ok((await p.locator('.nw').count()) === 0, 'ctrl+enter on an empty prompt creates nothing');
  await p.keyboard.type('first line'); await p.keyboard.press('Enter'); await p.keyboard.type('second line'); await sleep(250);
  ok((await ta.inputValue()) === 'first line\nsecond line' && (await p.locator('.nw').count()) === 0, 'a plain enter is a new line and starts nothing');
  await ta.fill('');

  // --- yolo: on by default, one click to make agents ask first
  ok((await p.locator('#pyolo').getAttribute('aria-pressed')) === 'true' && (await p.locator('#pyolo').innerText()).includes('yolo'), 'yolo is on by default, and visible');
  await p.locator('#pyolo').click();
  ok((await p.locator('#pyolo').getAttribute('aria-pressed')) === 'false' && (await p.locator('#pyolo').innerText()).includes('asks first'), 'one click switches it off');
  await p.locator('#pyolo').click();
  ok((await p.locator('#pyolo').getAttribute('aria-pressed')) === 'true', 'and on again');

  // --- start: the node appears in the middle and runs, with the chosen model
  await ta.click();
  await p.keyboard.type('Explain what textkit/slug.py does in one sentence.');
  await p.keyboard.press('Control+Enter');
  await p.waitForSelector('.nw[data-id="n1"]');
  ok(await p.locator('#pad').isHidden(), 'the pad gets out of the way');
  await sleep(700);
  const nb = await p.locator('.nw[data-id="n1"]').boundingBox();
  ok(Math.abs(nb.x + nb.width / 2 - 750) < 260 && Math.abs(nb.y + nb.height / 2 - 450) < 230, 'the agent is created in the middle of the screen (' + Math.round(nb.x + nb.width / 2) + ',' + Math.round(nb.y + nb.height / 2) + ')');
  ok(['starting', 'busy', 'done'].includes(await st('n1')), 'and it is already working (' + await st('n1') + ')');
  const tag = await p.locator('.nw[data-id="n1"] .ag').innerText();
  ok(tag.includes('codex') && tag.includes('high'), 'the card says what it runs on: "' + tag + '"');
  await sleep(2500);
  const screen = cp.execFileSync(BIN, ['read', 'n1', '-n', '60'], { cwd: W + '/proj', encoding: 'utf8' });
  ok(/model=sim-deep effort=high yolo=on/.test(screen), 'the CLI was started with yolo, the model and reasoning as plain flags');
  const flow = JSON.parse(fs.readFileSync(W + '/proj/.weft/flow.json', 'utf8'));
  const n1 = flow.nodes.find(n => n.id === 'n1');
  ok(n1.model === 'sim-deep' && n1.effort === 'high', 'the node stores model and reasoning (' + n1.model + ', ' + n1.effort + ')');
  ok(n1.addenda.includes('concise') && n1.addenda.length === 1, 'and exactly the addenda that were switched on (' + n1.addenda.join(', ') + ')');
  await p.screenshot({ path: W + '/pad-3-started.png' });

  // --- the path stays above the omnibar
  const wb = await p.locator('#where').boundingBox(), ob = await p.locator('#omni').boundingBox();
  ok((await p.locator('#where').innerText()).includes('proj') && wb.y + wb.height <= ob.y + 2, 'the folder is shown right above the prompt bar');

  // --- reopen with N, leave with Esc; existing agents stay one key away
  await p.mouse.click(200, 300);
  await p.keyboard.press('n');
  await p.waitForSelector('#pad:not([hidden]) textarea');
  ok(await p.locator('#pad .pcanvas').count() === 1, 'N reopens the pad and shows how many agents are on the canvas');
  await p.keyboard.press('Escape');
  ok(await p.locator('#pad').isHidden(), 'escape returns to the canvas');
  await p.locator('#bNew').click();
  await p.waitForSelector('#pad:not([hidden]) textarea');
  ok(await p.locator('#pad textarea').evaluate(e => document.activeElement === e), 'the "new task" button does the same, focused');
  await p.locator('#pad .pcanvas').click();
  ok(await p.locator('#pad').isHidden(), 'the canvas button closes it');

  // --- switched off in the pad, the next agent asks first, and the choice is remembered
  await p.keyboard.press('n');
  await p.waitForSelector('#pad:not([hidden]) textarea');
  await p.locator('#pyolo').click();
  await p.locator('#pad textarea').fill('Say hello in one word.');
  await p.keyboard.press('Control+Enter');
  await p.waitForSelector('.nw[data-id="n2"]');
  await sleep(2500);
  const flow2 = JSON.parse(fs.readFileSync(W + '/proj/.weft/flow.json', 'utf8'));
  ok(flow2.nodes.find(n => n.id === 'n2').yolo === false, 'the card remembers that it asks first');
  ok(/yolo=off/.test(cp.execFileSync(BIN, ['read', 'n2', '-n', '60'], { cwd: W + '/proj', encoding: 'utf8' })), 'and the CLI was started without the flag');
  ok((await p.locator('.nw[data-id="n2"] .ag').innerText()).includes('asks first'), 'its card says so: ' + (await p.locator('.nw[data-id="n2"] .ag').innerText()));
  await p.reload(); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(500);
  ok((await p.locator('#pyolo').getAttribute('aria-pressed')) === 'false', 'the choice survives a reload');
  await p.locator('#pyolo').click();   // back to the default for whoever uses this next

  // --- the wrong folder is the classic mistake: the home folder is called out
  await p.evaluate(() => { S.project.home = S.project.dir; openPad(); renderPad(); });
  await p.waitForSelector('#pad .pwarn');
  ok((await p.locator('#pad .pwarn').first().innerText()).includes('home folder'), 'starting in the home folder says so: ' + (await p.locator('#pad .pwarn').first().innerText()).slice(0, 60));
  ok((await p.locator('#pad .pname').innerText()) === '~', 'and the headline is "~"');
  await p.screenshot({ path: W + '/pad-4-home.png' });

  ok(errs.length === 0, 'no page errors' + (errs.length ? ': ' + errs.slice(0, 3).join(' | ') : ''));
  await b.close();
  console.log(fails ? fails + ' FAILED' : 'PAD OK');
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); process.exit(2); });
