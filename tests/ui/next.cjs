// "And then…": continue the same agent, hand its work to a fresh one, or branch.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs');
const W = process.env.SANDBOX;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const p = await (await b.newContext({ viewport: { width: 1700, height: 900 } })).newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type() === 'error' && errs.push(m.text()));
  const st = id => p.locator('.nw[data-id="' + id + '"]').getAttribute('data-st');
  const waitSt = async (id, want, t = 40000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (want.includes(await st(id))) return true; await sleep(250); } return false; };
  const nodes = () => p.locator('.nw').count();
  const port = async id => p.locator('.nw[data-id="' + id + '"] .port.out').boundingBox();
  const clickPort = async id => { const o = await port(id); await p.mouse.click(o.x + o.width / 2, o.y + o.height / 2); };

  await p.goto(fs.readFileSync(W + '/url.txt', 'utf8').trim()); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400);
  await p.keyboard.type('Explain what textkit/slug.py does in one sentence.'); await p.keyboard.press('Enter');
  await p.waitForSelector('.nw[data-id="n1"]');
  ok(await waitSt('n1', ['done']), 'the first agent finishes');

  // --- the diamond asks what comes next; the default keeps the same agent
  await clickPort('n1');
  await p.waitForSelector('#nextbar:not([hidden]) textarea');
  const modes = await p.locator('#nextbar .seg button').allInnerTexts();
  ok(['then', 'hand over', 'branch'].every(m => modes.includes(m)), 'the diamond offers then / hand over / branch (' + modes.join(' | ') + ')');
  ok((await p.locator('#nextbar .seg button[aria-pressed=true]').first().innerText()) === 'then', 'continuing the same agent is the default');
  ok(await p.locator('#nextbar textarea').evaluate(e => document.activeElement === e), 'the prompt is focused');
  ok((await p.locator('#nextbar .nx').innerText()).includes('keeps its context'), 'it says what that means: ' + (await p.locator('#nextbar .nx').innerText()).slice(0, 70));
  await p.screenshot({ path: W + '/next-1-composer.png' });

  // --- then: same node, one more step
  const n0 = await nodes();
  await p.keyboard.type('Now explain textkit/wrap.py in one sentence.'); await p.keyboard.press('Enter');
  await sleep(500);
  ok((await nodes()) === n0, 'no new agent appeared: the prompt went to the same one');
  ok(await p.locator('#nextbar').isHidden(), 'the composer closes');
  ok(await waitSt('n1', ['busy', 'starting', 'done']), 'the same agent works on it (' + await st('n1') + ')');
  ok(await waitSt('n1', ['done']), 'and finishes');
  const ls = require('child_process').execFileSync(process.env.WEFT_UI_BIN, ['ls'], { cwd: W + '/proj', encoding: 'utf8' });
  ok(/^n1\s.*runs=2/m.test(ls), 'it is the second run of that agent: ' + (ls.split('\n').find(l => l.startsWith('n1')) || '').replace(/\s+/g, ' '));

  // --- hand over: a fresh agent takes over, a dotted baton line shows the lineage
  await clickPort('n1');
  await p.waitForSelector('#nextbar:not([hidden]) textarea');
  await p.locator('#nextbar .seg button', { hasText: 'hand over' }).click();
  ok((await p.locator('#nextbar .nx').innerText()).includes('fresh'), 'hand over explains itself: ' + (await p.locator('#nextbar .nx').innerText()).slice(0, 80));
  ok(await p.locator('#nextbar .nf .seg button').count() >= 3, 'and offers the agent that takes over');
  await p.locator('#nextbar textarea').fill('Review what was explained and list one improvement.');
  await p.screenshot({ path: W + '/next-2-handover.png' });
  await p.keyboard.press('Enter');
  await p.waitForSelector('.nw[data-id="n2"]');
  ok((await nodes()) === n0 + 1, 'a second agent appears');
  ok(await p.locator('#handG .hand').count() === 1, 'a dotted baton line ties it to the agent it took over from');
  ok(await waitSt('n2', ['starting', 'busy', 'done'], 40000), 'it starts once the note is in (' + await st('n2') + ')');
  ok(await waitSt('n2', ['done']), 'and finishes');
  const got = fs.readFileSync(W + '/proj/.weft/run/received-n2.log', 'utf8');
  ok(got.includes('Taking over from') && got.includes('goal: make the textkit tests pass') && got.includes('Review what was explained'), 'it received the note and the instruction');
  ok((await p.locator('.nw[data-id="n2"] .ag').innerText()).includes('claude'), 'it is a normal agent card');
  await p.screenshot({ path: W + '/next-3-after.png' });

  // --- branch: a new agent gets the result (a real link)
  const e0 = await p.locator('#edgeG .ed').count();
  await p.locator('.nw[data-id="n1"] .hd').click({ button: 'right' });
  ok((await p.locator('.menu .mi').allInnerTexts()).some(t => /hand over to a fresh agent/.test(t)), 'the card menu lists the three ways too');
  await p.locator('.menu .mi', { hasText: 'branch' }).click();
  await p.waitForSelector('#nextbar:not([hidden]) textarea');
  ok((await p.locator('#nextbar .seg button[aria-pressed=true]').first().innerText()) === 'branch', 'the menu opens the composer on branch');
  await p.keyboard.type('Summarize it in five words.'); await p.keyboard.press('Enter');
  await p.waitForSelector('.nw[data-id="n3"]');
  await sleep(500);
  ok((await p.locator('#edgeG .ed').count()) === e0 + 1, 'branch makes a link (the result is piped)');
  ok(await waitSt('n3', ['done']), 'and the new agent works on it');

  // --- the message bar: tab switches between continuing and handing over
  await p.locator('.nw[data-id="n1"] .hd').click();
  await p.locator('#omni textarea').click();
  const chip0 = await p.locator('#omni .tgt').first().innerText();
  await p.keyboard.press('Tab');
  const chip1 = await p.locator('#omni .tgt').first().innerText();
  ok(chip0.includes('n1') || chip0.includes('Explain') || chip0.length > 1, 'the bar targets the selected agent: ' + chip0);
  ok(/hand over/.test(chip1) && await p.locator('#omni .oc .pagents').count() === 1, 'tab switches to "hand over" and shows which agent takes over: ' + chip1);
  await p.keyboard.press('Tab');
  ok(!/hand over/.test(await p.locator('#omni .tgt').first().innerText()), 'tab again goes back to continuing');

  ok(errs.length === 0, 'no page errors' + (errs.length ? ': ' + errs.slice(0, 3).join(' | ') : ''));
  await b.close();
  console.log(fails ? fails + ' FAILED' : 'NEXT OK');
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); process.exit(2); });
