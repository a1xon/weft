// Talks: drag the round port on top of an agent onto another agent and the two coordinate while
// they work. Each is told who the other is and what to coordinate on (the field on the line); they
// tell each other what they work on with `weft tell` (the simulated agent does, as a real one
// would), only they hear it, and nothing fires along the line.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs');
const W = process.env.SANDBOX;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const p = await (await b.newContext({ viewport: { width: 1700, height: 950 } })).newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type() === 'error' && errs.push(m.text()));
  const waitFor = async (fn, t = 30000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (await fn()) return true; await sleep(200); } return false; };
  const st = id => p.locator('.nw[data-id="' + id + '"]').getAttribute('data-st');
  const received = id => { try { return fs.readFileSync(W + '/proj/.weft/run/received-' + id + '.log', 'utf8'); } catch { return ''; } };
  const talks = () => { try { return JSON.parse(fs.readFileSync(W + '/proj/.weft/flow.json', 'utf8')).talks || []; } catch { return []; } };
  const center = async loc => { const r = await loc.boundingBox(); return [r.x + r.width / 2, r.y + r.height / 2]; };

  await p.goto(fs.readFileSync(W + '/url.txt', 'utf8').trim()); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400);
  await p.keyboard.press('Escape'); await sleep(200);
  // one agent at work, one not started yet, a note
  await p.evaluate(() => send('node_add', { node: { name: 'left', agent: 'claude', prompt: 'LONG=6 tidy up textkit/wrap.py', x: 80, y: 330 }, run: true }));
  await p.evaluate(() => send('node_add', { node: { name: 'right', agent: 'codex', prompt: 'fix the slug function in textkit/slug.py', x: 760, y: 330 } }));
  await p.evaluate(() => send('node_add', { node: { name: 'context', agent: 'note', prompt: 'a note', x: 1260, y: 640 } }));
  ok(await waitFor(async () => (await p.locator('.nw').count()) === 3, 8000), 'two agents and a note are on the canvas');
  await p.evaluate(() => { S.v = { x: 40, y: 60, k: 0.9 }; applyView(); });
  ok(await waitFor(async () => ['busy', 'starting'].includes(await st('n1')), 30000), 'the left one works');
  ok(await p.locator('.nw[data-id="n1"] .port.talk').isVisible() && await p.locator('.nw[data-id="n2"] .port.talk').isVisible(), 'agents have a round port on top');
  ok(await p.locator('.nw[data-id="n3"] .port.talk').isHidden(), 'a note has none');
  const pb = await p.locator('.nw[data-id="n1"] .port.talk').boundingBox(), cb = await p.locator('.nw[data-id="n1"]').boundingBox();
  ok(Math.abs(pb.x + pb.width / 2 - (cb.x + cb.width / 2)) < 3 && Math.abs(pb.y + pb.height / 2 - cb.y) < 3, 'in the middle of its top edge');

  // a click on it says what it is for; a drop on something that is not an agent makes no talk
  await p.locator('.nw[data-id="n1"] .port.talk').click();
  ok(await waitFor(async () => /Drag this onto another agent/.test(await p.locator('#toasts').innerText()), 3000), 'a click on the port says how to use it');
  let [x, y] = await center(p.locator('.nw[data-id="n1"] .port.talk'));
  let [tx, ty] = await center(p.locator('.nw[data-id="n3"]'));
  await p.mouse.move(x, y); await p.mouse.down(); await p.mouse.move(tx, ty, { steps: 10 }); await p.mouse.up(); await sleep(500);
  ok(talks().length === 0 && await p.locator('.tkpill').count() === 0, 'dropped on a note: no talk');

  // drag it onto the other agent: the line arches between their tops, the field takes the topic
  [x, y] = await center(p.locator('.nw[data-id="n1"] .port.talk'));
  [tx, ty] = await center(p.locator('.nw[data-id="n2"] .body'));
  await p.mouse.move(x, y); await p.mouse.down(); await p.mouse.move(x + 200, y - 60, { steps: 6 });
  ok(await p.locator('#app.talking').count() === 1 && await p.locator('.trub').isVisible(), 'while drawing, a teal line follows the pointer');
  await p.mouse.move(tx, ty, { steps: 10 });
  ok(await p.locator('.nw[data-id="n2"].talkdrop').count() === 1, 'the agent under the pointer lights up');
  await p.mouse.up();
  ok(await waitFor(async () => talks().length === 1, 5000), 'letting go there links them (kept in the flow)');
  ok(await waitFor(async () => await p.locator('.tkpill').count() === 1, 3000), 'a field sits on the line');
  ok(await p.evaluate(() => document.activeElement && document.activeElement.classList.contains('tk-topic')), 'and has the keyboard, for the topic');
  const line = await p.locator('#talkG .tk .tv').getAttribute('d');
  ok(/^M\d/.test(line || ''), 'the line is drawn');
  const pill = await p.locator('.tkpill').boundingBox(), c1 = await p.locator('.nw[data-id="n1"]').boundingBox();
  ok(pill.y + pill.height < c1.y, 'it arches over the cards (' + Math.round(pill.y + pill.height) + ' < ' + Math.round(c1.y) + ')');
  await p.keyboard.type('the slug module and CPU use'); await p.keyboard.press('Enter');
  ok(await waitFor(async () => (talks()[0] || {}).topic === 'the slug module and CPU use', 5000), 'Enter keeps the topic');
  await p.screenshot({ path: W + '/talk-line.png' });

  // the agent at work is told about the other one; it tells it what it works on (the sim runs
  // `weft tell`), and the other one, not started yet, hears it all with its task
  ok(await waitFor(async () => /You are not working alone/.test(received('n1')), 40000), 'the working agent is told it is not alone');
  const brief1 = received('n1');
  ok(brief1.includes('- n2 "right"') && brief1.includes('Coordinate mainly on: the slug module and CPU use.'), 'with who the other is and the topic');
  ok(!/The person changed what you/.test(received('n1')), 'the briefing waited for the topic: one message, not two');
  ok(await waitFor(async () => await p.evaluate(() => S.talkLog.length) >= 1, 30000), 'it said something to the other one');
  ok(await waitFor(async () => await p.locator('.tsay').count() >= 1, 3000), 'it shows over the one who said it');
  ok(/claude here/.test(await p.locator('.tsay').first().innerText()), 'with what it said');
  ok(received('n2') === '' && ['new', 'ready'].includes(await st('n2')), 'nothing started the other one');
  await p.evaluate(() => runNode('n2'));
  ok(await waitFor(async () => /You are not working alone/.test(received('n2')), 40000), 'the other one gets the briefing with its task');
  const brief2 = received('n2');
  ok(brief2.indexOf('fix the slug function') < brief2.indexOf('You are not working alone'), 'after the task');
  ok(/\[weft\] n1 "left" says: claude here/.test(brief2), 'and what was said to it before it started');
  ok(await waitFor(async () => /\[weft\] n2 "right" says: codex here/.test(received('n1')), 40000), 'its answer reaches the first one');
  ok(await waitFor(async () => (await p.locator('.tkpill .tk-ic b').innerText()) === '2', 5000), 'the line counts what they said');
  await p.locator('.tkpill .tk-ic').click();
  ok(await waitFor(async () => await p.locator('.tk-log .tm').count() === 2, 3000), 'its bubble unfolds what they said');
  ok(/left → right/.test(await p.locator('.tk-log .tm').first().innerText()), 'who said it to whom');
  await p.screenshot({ path: W + '/talk-log.png' });
  await p.locator('.tkpill .tk-ic').click();
  ok(await p.locator('.tk-log').isHidden(), 'and folds again');
  // nothing fired along the talk: no link, the results stay their own
  ok(await p.evaluate(() => S.edges.size) === 0, 'a talk is not a link');

  // a third agent: select the three, right-click, let them talk
  await p.evaluate(() => send('node_add', { node: { name: 'third', agent: 'claude', prompt: 'write the docs', x: 80, y: 760 } }));
  ok(await waitFor(async () => (await p.locator('.nw').count()) === 4, 8000), 'a third agent');
  await p.evaluate(() => select(['n1', 'n2', 'n4']));
  const [mx, my] = await center(p.locator('.nw[data-id="n4"] .hd'));
  await p.mouse.click(mx, my, { button: 'right' });
  await p.locator('.menu .mi', { hasText: 'let them talk' }).click();
  ok(await waitFor(async () => talks().length === 2, 5000), 'the new one joins the two that talk already: one more line (' + talks().length + ' talks)');
  ok(await waitFor(async () => await p.locator('#talkG .tk').count() === 2, 3000), 'two lines');
  ok(talks().some(t => [t.a, t.b].includes('n4')), 'to the third agent');

  // moving a card takes its line along
  await p.evaluate(() => select([]));
  const before = await p.locator('#talkG .tk .tv').first().getAttribute('d');
  const [hx, hy] = await center(p.locator('.nw[data-id="n2"] .hd'));
  await p.mouse.move(hx, hy); await p.mouse.down(); await p.mouse.move(hx + 60, hy + 40, { steps: 6 }); await p.mouse.up(); await sleep(300);
  ok(before !== await p.locator('#talkG .tk .tv').first().getAttribute('d'), 'a moved card takes its line along');

  // right-click on the field: stop the talk
  const t1 = await p.locator('.tkpill').first().getAttribute('data-talk');
  const [px, py] = await center(p.locator('.tkpill[data-talk="' + t1 + '"] .tk-ic'));
  await p.mouse.click(px, py, { button: 'right' });
  await p.locator('.menu .mi', { hasText: 'stop this talk' }).click();
  ok(await waitFor(async () => talks().length === 1 && !talks().some(t => t.id === t1), 5000), 'a talk can be stopped');
  // deleting an agent takes its talks with it
  const left = talks()[0], gone = [left.a, left.b].find(i => i !== 'n1') || left.b;
  await p.evaluate(id => send('node_del', { id }), gone);
  ok(await waitFor(async () => talks().length === 0, 5000), 'a deleted agent takes its talks along');
  ok(await waitFor(async () => (await p.locator('.tkpill').count()) === 0 && (await p.locator('#talkG .tk').count()) === 0, 3000), 'and the canvas shows it');
  ok(errs.length === 0, 'no page errors' + (errs.length ? ': ' + errs.join(' | ') : ''));
  await b.close();
  console.log(fails ? fails + ' FAILED' : 'ALL PASSED');
  process.exit(fails ? 1 : 0);
})();
