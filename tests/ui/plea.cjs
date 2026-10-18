// "Needs you": the question unfolds under the card with an answer box; a dialog shows its options;
// a queued prompt rewired to another agent is asked of that agent.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs');
const path = require('path');
const W = process.env.SANDBOX;
const SIM = path.resolve(__dirname, '../../examples/sim-agent.py');
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const p = await (await b.newContext({ viewport: { width: 1700, height: 1000 } })).newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type() === 'error' && errs.push(m.text()));
  const st = id => p.locator('.nw[data-id="' + id + '"]').getAttribute('data-st');
  const waitSt = async (id, want, t = 60000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (want.includes(await st(id))) return true; await sleep(250); } return false; };
  const waitFor = async (fn, t = 30000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (await fn()) return true; await sleep(200); } return false; };
  const add = async node => p.evaluate(nd => new Promise(res => {
    const before = new Set(S.nodes.keys());
    send('node_add', { node: nd });
    const t = setInterval(() => { const id = [...S.nodes.keys()].find(k => !before.has(k)); if (id) { clearInterval(t); res(id); } }, 50);
  }), node);
  const plea = id => p.locator('.nw[data-id="' + id + '"] .plea');

  await p.goto(fs.readFileSync(W + '/url.txt', 'utf8').trim()); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400);
  await p.keyboard.press('Escape'); await sleep(200);

  // --- a step of a chain asks a question: it unfolds under the card, with a box to answer in
  const asker = await add({ agent: 'claude', name: 'asker', prompt: 'ASK_QUESTION about the slug of a title with unicode.', x: 120, y: 80 });
  const next = await add({ agent: 'claude', name: 'after', prompt: 'Summarise: {{inputs}}', x: 620, y: 80 });
  await p.evaluate(([a, b]) => send('edge_add', { from: a, to: b }), [asker, next]);
  await sleep(300);
  await p.evaluate(() => send('run'));
  ok(await waitSt(asker, ['attention'], 90000), 'the agent that asks shows as needing you (' + await st(asker) + ')');
  ok(await waitFor(async () => (await plea(asker).count()) === 1, 5000), 'its question unfolds under the card');
  ok((await plea(asker).getAttribute('data-mode')) === 'ask', 'as a question');
  const q = (await plea(asker).locator('.pq').innerText()).trim();
  ok(/unicode/i.test(q), 'with the question in full: "' + q.slice(0, 80) + '"');
  const fsz = await plea(asker).locator('.pq').evaluate(e => parseFloat(getComputedStyle(e).fontSize));
  const scroll = await plea(asker).locator('.pq').evaluate(e => getComputedStyle(e).overflowY);
  ok(fsz >= 13 && scroll === 'auto', `readable (${fsz}px) and scrollable (${scroll})`);
  const cb = await p.locator('.nw[data-id="' + asker + '"] .node').boundingBox(), pb = await plea(asker).boundingBox();
  ok(pb.y > cb.y + cb.height - 2 && Math.abs(pb.x - cb.x) < 4, 'attached right under the card');
  ok(await plea(asker).locator('textarea').isVisible(), 'with an answer box');
  ok(/asker asks you: should the slug/i.test(await p.locator('#toasts').innerText()), 'the toast says what it asks, not "a dialog"');
  await p.screenshot({ path: W + '/plea-1-question.png' });
  // enter on the selected card goes to the answer box, not to the terminal
  await p.locator('.nw[data-id="' + asker + '"] .hd').click(); await sleep(150);
  await p.keyboard.press('Enter'); await sleep(150);
  ok(await plea(asker).locator('textarea').evaluate(e => document.activeElement === e), 'enter on the card puts the cursor in the answer box');
  ok(await p.locator('#focus').isHidden(), 'and does not open the terminal');
  await p.keyboard.type('Transliterate to plain ascii.'); await p.keyboard.press('Enter');
  ok(await waitFor(async () => (await plea(asker).count()) === 0, 8000), 'answered, the question folds away');
  ok(await waitSt(asker, ['busy', 'done']), 'the agent goes on (' + await st(asker) + ')');
  ok(await waitSt(asker, ['done'], 90000), 'and finishes (' + await st(asker) + ')');
  ok(await waitSt(next, ['done'], 90000), 'and the flow goes on (' + await st(next) + ')');
  const rt = await p.evaluate(id => S.rt.get(id).runs, asker);
  ok(rt === 1, 'in the same run (' + rt + ')');

  // --- a dialog on the screen: its options are buttons, and answering it ends "needs you"
  const truster = await add({ agent: 'claude', name: 'truster', cmd: `python3 ${SIM} --name claude --trust`, prompt: '', x: 1120, y: 80 });
  await p.evaluate(id => send('start', { id }), truster);
  ok(await waitSt(truster, ['attention'], 30000), 'an agent at a trust dialog needs you (' + await st(truster) + ')');
  ok(await waitFor(async () => (await plea(truster).getAttribute('data-mode').catch(() => '')) === 'dialog', 8000), 'the dialog unfolds under the card');
  const opts = await plea(truster).locator('.popts .opt').allInnerTexts();
  ok(opts.length === 2 && /trust this folder/.test(opts[0]), 'its options are buttons (' + opts.map(t => t.replace(/\s+/g, ' ')).join(' | ') + ')');
  ok(/truster waits at a dialog/.test(await p.locator('#toasts').innerText()), 'and the toast calls it a dialog');
  ok(/trust this folder/.test(await plea(truster).locator('.pq').innerText()), 'with the dialog itself above them');
  ok(await plea(truster).locator('textarea').isHidden(), 'and no text box: a dialog is answered by picking');
  await p.screenshot({ path: W + '/plea-2-dialog.png' });
  await plea(truster).locator('.popts .opt').first().click();
  ok(await waitSt(truster, ['ready', 'idle', 'starting'], 15000), 'picked, the agent is ready (' + await st(truster) + ')');
  ok(await waitFor(async () => (await plea(truster).count()) === 0, 5000), 'and the dialog folds away');

  // --- Claude Code 2.1.302 numbers nothing and highlights "No, exit": the Enter of a typed task
  // picked it and the agent quit. A message waits for the dialog, whose options move there and confirm
  const doubter = await add({ agent: 'claude', name: 'doubter', cmd: `python3 ${SIM} --name claude --trust-arrows --think 1`, prompt: '', x: 1120, y: 300 });
  await p.evaluate(id => send('start', { id }), doubter);
  ok(await waitSt(doubter, ['attention'], 30000), 'an agent at the unnumbered trust dialog needs you (' + await st(doubter) + ')');
  ok(await waitFor(async () => (await plea(doubter).getAttribute('data-mode').catch(() => '')) === 'dialog', 8000), 'that dialog unfolds under the card');
  const opts2 = await plea(doubter).locator('.popts .opt').allInnerTexts();
  ok(opts2.length === 2 && /No, exit/.test(opts2[0]) && /trust this folder/.test(opts2[1]), 'its options are buttons (' + opts2.join(' | ') + ')');
  await p.evaluate(id => send('send', { id, text: 'Say hello to the folder.', mode: 'auto' }), doubter);
  await sleep(2500);
  ok(await st(doubter) === 'attention', 'a message sent meanwhile is not typed into the dialog (' + await st(doubter) + ')');
  await p.evaluate(id => centerOn(id), doubter); await sleep(400);
  await p.screenshot({ path: W + '/plea-2b-unnumbered.png' });
  // (the bar and the git tree may cover it here: the button itself is what is tested)
  await plea(doubter).locator('.popts .opt').nth(1).dispatchEvent('click');
  ok(await waitFor(async () => (await plea(doubter).count()) === 0, 8000), 'picked, the dialog folds away');
  ok(await waitSt(doubter, ['busy'], 15000), 'and the message that waited goes to work (' + await st(doubter) + ')');
  ok(await waitSt(doubter, ['done'], 30000), 'and comes back done (' + await st(doubter) + ')');

  // --- a queued prompt rewired after another agent is asked of that agent
  const step = await add({ agent: 'step', host: asker, name: 'prompt 2', prompt: 'and now wrap.py', x: 1120, y: 560, draft: true });
  await p.evaluate(([a, s]) => send('edge_add', { from: a, to: s }), [asker, step]);
  await sleep(300);
  const eid = await p.evaluate(s => S.edges ? [...S.edges.values()].find(e => e.to === s).id : null, step);
  ok(!!eid, 'the prompt is linked after the first agent');
  await p.evaluate(id => send('edge_del', { id }), eid);
  await p.evaluate(([t, s]) => send('edge_add', { from: t, to: s }), [truster, step]);
  ok(await waitFor(async () => (await p.evaluate(s => S.nodes.get(s).host, step)) === truster, 5000), 'linked after the other agent, it is asked of that one');
  ok(await waitFor(async () => /is now asked of truster/.test(await p.locator('#toasts').innerText().catch(() => '')), 3000), 'and weft says so');
  ok((await p.locator('.nw[data-id="' + step + '"] .stx').innerText()).includes('truster'), 'its card names the agent: ' + await p.locator('.nw[data-id="' + step + '"] .stx').innerText());

  ok(errs.length === 0, 'no page errors' + (errs.length ? ': ' + errs.join(' | ') : ''));
  await b.close();
  console.log(fails ? `${fails} FAILED` : 'ALL PASS');
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
