// A queued prompt joins its agent when the agent takes it up (engine/merge.rs): its card goes, the prompt is
// the agent's next turn in its own session, the card's links are the agent's, and a reasoning level set on
// it is the agent's from then on: the process is started again at that level, on the same conversation.
// The simulated agent says what it remembers of its session when it is asked to RECALL.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs'); const cp = require('child_process');
const W = process.env.SANDBOX;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const p = await (await b.newContext({ viewport: { width: 1600, height: 900 } })).newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type() === 'error' && errs.push(m.text()));
  const waitFor = async (fn, t = 20000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (await fn()) return true; await sleep(200); } return false; };
  const rt = id => p.evaluate(i => S.rt.get(i) || {}, id);
  const node = id => p.evaluate(i => S.nodes.get(i) || null, id);
  const st = id => p.locator('.nw[data-id="' + id + '"]').getAttribute('data-st');
  const received = id => { try { return fs.readFileSync(W + '/proj/.weft/run/received-' + id + '.log', 'utf8'); } catch { return ''; } };

  await p.goto(fs.readFileSync(W + '/url.txt', 'utf8').trim()); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400);
  await p.keyboard.type('LONG=10 Remember the word PELICAN'); await p.keyboard.press('Control+Enter');
  await p.waitForSelector('.nw[data-id="n1"]');
  ok(await waitFor(async () => (await rt('n1')).status === 'busy', 30000), 'the agent is at work');

  // --- a prompt for it from the bar waits as its own card
  await p.evaluate(() => { select(['n1']); centerOn('n1'); });
  await p.locator('#omni textarea').click();
  await p.keyboard.type('RECALL and explain textkit/wrap.py'); await p.keyboard.press('Control+Enter');
  ok(await waitFor(() => p.evaluate(() => [...S.nodes.values()].some(n => n.agent === 'step'))), 'it waits as a queued prompt');
  const sid = await p.evaluate(() => [...S.nodes.values()].find(n => n.agent === 'step').id);
  ok(await waitFor(async () => (await st(sid)) === 'waiting', 5000), 'for the agent (' + await st(sid) + ')');

  // --- its reasoning: a click on its tag
  await p.evaluate(id => centerOn(id), sid); await sleep(400);
  await p.locator('.nw[data-id="' + sid + '"] .ag').click();
  await p.waitForSelector('.menu .mi');
  const items = await p.locator('.menu .mi').allInnerTexts();
  ok(['as is', 'high', 'max'].every(w => items.some(t => t.trim().replace(/^●\s*/, '').startsWith(w))), 'its tag offers the agent\'s levels (' + items.map(t => t.trim()).join(' | ') + ')');
  await p.locator('.menu .mi').filter({ hasText: /^\W*high\s*$/ }).click();
  ok(await waitFor(async () => (await node(sid)).effort === 'high', 5000), 'the queued prompt is set to high');
  ok((await node('n1')).effort === '', 'the agent is not, before it takes the prompt up');
  ok((await p.locator('.nw[data-id="' + sid + '"] .ag').innerText()).includes('high'), 'the tag says so: ' + await p.locator('.nw[data-id="' + sid + '"] .ag').innerText());
  // the bar shows it while the card is selected
  await p.evaluate(id => select([id]), sid); await sleep(300);
  ok((await p.locator('#omni .peff button[aria-pressed=true]').innerText()) === 'high', 'the bar shows its level while it is selected');
  await p.screenshot({ path: W + '/merge-1-queued.png' });

  // --- a card linked after the queued prompt
  const x = await p.evaluate(async sid => {
    const s = S.nodes.get(sid);
    const r = await req('node_add', { node: { agent: 'claude', x: s.x + 700, y: s.y, prompt: 'Summarize what came in: {{inputs}}', addenda: [] } });
    send('edge_add', { from: sid, to: r.id });
    return r.id;
  }, sid);
  ok(await waitFor(() => p.evaluate(([s, x]) => [...S.edges.values()].some(e => e.from === s && e.to === x), [sid, x])), 'linked after it (' + x + ')');

  // --- the agent is done: it takes the prompt up, and the card joins it
  ok(await waitFor(() => p.evaluate(id => !S.nodes.has(id), sid), 60000), 'the card joins the agent: it is gone');
  const links = await p.evaluate(() => [...S.edges.values()].map(e => e.from + '>' + e.to));
  ok(links.length === 1 && links[0] === 'n1>' + x, 'its link is the agent\'s now: ' + links.join(', '));
  ok((await node('n1')).effort === 'high', 'the agent thinks at high from now on');
  ok(await waitFor(() => received('n1').includes('RECALL and explain textkit/wrap.py'), 40000), 'the prompt went to the agent');
  ok(await waitFor(async () => { const r = await rt('n1'); return r.status === 'done' && /explain/.test(r.asked || ''); }, 60000), 'which answered it');
  const said = (await rt('n1')).out || '';
  ok(/I remember: LONG=10 Remember the word PELICAN/.test(said), 'in the same conversation, its answer is the agent\'s result: ' + said.replace(/\s+/g, ' ').slice(0, 120));
  // (started again at the new level, on the conversation it had)
  const procs = cp.execSync('ps -eo args', { encoding: 'utf8' }).split('\n').filter(l => l.includes('sim-agent.py') && l.includes('--effort high'));
  ok(procs.length === 1 && /--resume \S+/.test(procs[0]), 'its process was started again at high, on its conversation: ' + (procs[0] || '(none)').replace(/^.*sim-agent\.py/, 'sim-agent.py'));
  await p.screenshot({ path: W + '/merge-2-joined.png' });

  // --- what was linked after the card follows the agent
  ok(await waitFor(async () => (await rt(x)).status === 'done', 60000), 'the card after it ran (' + (await rt(x)).status + ')');
  ok(received(x).includes('I remember: LONG=10 Remember the word PELICAN'), 'on the agent\'s answer to the prompt');

  ok(errs.length === 0, 'no page errors' + (errs.length ? ': ' + errs.join(' | ') : ''));
  await b.close();
  console.log(fails ? fails + ' FAILED' : 'MERGE OK');
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); process.exit(2); });
