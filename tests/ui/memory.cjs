// A card keeps what its agent knows when the agent's process ends (engine/resume.rs): a stopped (or
// rested) agent comes back on the same conversation, and a twin starts on a copy of it. The simulated
// agent remembers the first line of every prompt per session and says so when it is asked to RECALL.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs');
const W = process.env.SANDBOX;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const p = await (await b.newContext({ viewport: { width: 1600, height: 900 } })).newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type() === 'error' && errs.push(m.text()));
  const waitFor = async (fn, t = 20000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (await fn()) return true; await sleep(200); } return false; };
  const rt = id => p.evaluate(i => S.rt.get(i) || {}, id);
  const received = id => { try { return fs.readFileSync(W + '/proj/.weft/run/received-' + id + '.log', 'utf8'); } catch { return ''; } };

  await p.goto(fs.readFileSync(W + '/url.txt', 'utf8').trim()); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400);
  await p.keyboard.type('Remember the word PELICAN'); await p.keyboard.press('Control+Enter');
  await p.waitForSelector('.nw[data-id="n1"]');
  ok(await waitFor(async () => (await rt('n1')).status === 'done', 40000), 'the agent is done');
  ok(await waitFor(async () => (await rt('n1')).twin === true), 'weft knows its conversation: it can have a twin');

  // --- its process ends (as when it rests): the card stays done, and remembers
  await p.evaluate(() => send('stop', { id: 'n1' }));
  ok(await waitFor(async () => { const r = await rt('n1'); return r.alive === false && r.rests === true; }), 'stopped, it rests on its conversation');
  ok(await waitFor(async () => /resting/.test(await p.locator('.nw[data-id="n1"]').innerText())), 'the card says it rests');
  ok((await rt('n1')).status === 'done', 'and is still done');

  // --- a follow-up from the bar: a new process, the same conversation
  await p.evaluate(() => { select(['n1']); centerOn('n1'); });
  await p.locator('#omni textarea').click();
  await p.keyboard.type('RECALL what you were told'); await p.keyboard.press('Control+Enter');
  ok(await waitFor(async () => { const r = await rt('n1'); return r.runs >= 2 && r.status === 'done'; }, 40000), 'the follow-up ran');
  const said = (await rt('n1')).out || '';
  ok(/I remember: Remember the word PELICAN/.test(said), 'it remembers what it was told before its process ended: ' + said.slice(0, 120));
  ok(!/new session/.test(received('n1')), 'nobody had to tell it what it did');
  await p.screenshot({ path: W + '/memory-1-resumed.png' });

  // --- a twin: a card of its own on a copy of that conversation
  await p.evaluate(() => openNext('n1', 'twin'));
  await p.waitForSelector('#nextbar:not([hidden]) textarea');
  const modes = await p.locator('#nextbar .seg button').allInnerTexts();
  ok(modes.includes('twin'), 'the composer offers a twin: ' + modes.join(' | '));
  await p.keyboard.type('RECALL as the twin'); await p.keyboard.press('Enter');
  ok(await waitFor(async () => (await p.evaluate(() => [...S.nodes.values()].some(n => n.twin === 'n1')))), 'a twin card comes up');
  const tid = await p.evaluate(() => [...S.nodes.values()].find(n => n.twin === 'n1').id);
  ok(await waitFor(async () => (await rt(tid)).status === 'done', 40000), 'the twin is done');
  const twin = (await rt(tid)).out || '';
  ok(/PELICAN/.test(twin) && /RECALL what you were told/.test(twin), 'the twin knows everything the first one knew: ' + twin.slice(0, 160));
  ok(received(tid).includes('separate agent, ' + tid), 'it is told that it is a copy');
  ok(await p.locator('path.hand.twin').count() === 1, 'a thread ties it to the agent it copied');
  // and the first one goes on by itself: it knows nothing of the twin's prompt
  await p.evaluate(() => send('send', { id: 'n1', text: 'RECALL once more', mode: 'auto' }));
  ok(await waitFor(async () => (await rt('n1')).runs >= 3 && (await rt('n1')).status === 'done', 40000), 'the first one answers again');
  const own = (await rt('n1')).out || '';
  ok(/PELICAN/.test(own) && !/as the twin/.test(own), 'the first one goes on by itself: ' + own.slice(0, 160));
  await p.screenshot({ path: W + '/memory-2-twin.png' });

  ok(errs.length === 0, 'no page errors' + (errs.length ? ': ' + errs.join(' | ') : ''));
  await b.close();
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
