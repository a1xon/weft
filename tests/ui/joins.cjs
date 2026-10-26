// A finished junction and the cards made from it: a fork and an undone delete start once, with
// every result; a link into the done junction waits for its card alone and runs it with that result.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs');
const W = process.env.SANDBOX;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const p = await (await b.newContext({ viewport: { width: 1500, height: 880 } })).newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message));
  const url = fs.readFileSync(W + '/url.txt', 'utf8').trim();
  const waitFor = async (fn, t = 30000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (await fn()) return true; await sleep(200); } return false; };
  const add = (node, run) => p.evaluate(async a => (await req('node_add', a)).id, { node, run });
  const rt = id => p.evaluate(i => { const r = S.rt.get(i) || {}; return { st: r.status, runs: r.runs, arrived: r.arrived || [], need: r.need }; }, id);
  const ids = () => p.evaluate(() => [...S.nodes.keys()]);
  const got = id => { const f = W + '/proj/.weft/run/received-' + id + '.log'; return fs.existsSync(f) ? fs.readFileSync(f, 'utf8').split('=== prompt ===').slice(1) : []; };
  const linkCls = (from, to) => p.evaluate(([f, t]) => { const e = [...S.edges.values()].find(x => x.from === f && x.to === t); const g = e && document.getElementById('p-' + e.id); return g ? g.parentNode.getAttribute('class') : null; }, [from, to]);
  const done = async id => waitFor(async () => (await rt(id)).st === 'done', 60000);

  await p.goto(url); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400); await p.keyboard.press('Escape');
  const a = await add({ name: 'A', agent: 'sh', cmd: 'echo result-from-A', prompt: 'go', x: 100, y: 100 }, true);
  const bb = await add({ name: 'B', agent: 'sh', cmd: 'echo result-from-B', prompt: 'go', x: 100, y: 500 }, true);
  ok(await done(a) && await done(bb), 'A and B are done');
  const t = await add({ name: 'T', agent: 'claude', prompt: 'Combine:\n{{inputs}}', addenda: [], x: 800, y: 300 }, false);
  await p.evaluate(([a, b, t]) => { send('edge_add', { from: a, to: t }); send('edge_add', { from: b, to: t }); }, [a, bb, t]);
  ok(await done(t), 'T is done');
  let r = await rt(t);
  ok(r.runs === 1 && got(t).length === 1, 'T ran once (' + r.runs + ', ' + got(t).length + ' prompts)');
  // (linked one after the other: what arrives before T has taken up its prompt joins that prompt)
  ok(r.arrived.length === 2 && r.need === 2, 'both links are in (' + r.arrived.length + '/' + r.need + ')');
  ok(/\barr\b/.test(await linkCls(a, t) || '') && /\barr\b/.test(await linkCls(bb, t) || ''), 'and drawn as arrived');

  // --- a fork: one start, with both results
  let before = await ids();
  await p.evaluate(i => fork(i), t);
  let f = null;
  ok(await waitFor(async () => { f = (await ids()).find(i => !before.includes(i)); return !!f; }, 5000), 'the fork is there');
  ok(await done(f), 'the fork is done');
  await sleep(3000);
  let pr = got(f);
  ok((await rt(f)).runs === 1 && pr.length === 1, 'the fork started once (' + (await rt(f)).runs + ' runs, ' + pr.length + ' prompts)');
  ok(pr.length && pr[0].includes('result-from-A') && pr[0].includes('result-from-B'), 'with both results');

  // --- delete and undo: the card comes back and starts once, with both
  ok(await p.locator('#juncG .jn').count() === 2, 'the fork also has its junction');
  await p.evaluate(i => deleteNodes([i]), f);
  await sleep(600);
  ok(await p.locator('#juncG .jn').count() === 1, 'deleting the fork removes its junction');
  before = await ids();
  fs.rmSync(W + '/proj/.weft/run/received-' + f + '.log', { force: true }); // (ids are reused)
  await p.evaluate(() => undo());
  let u = null;
  ok(await waitFor(async () => { u = (await ids()).find(i => !before.includes(i)); return !!u; }, 5000), 'undo brings it back');
  ok(await waitFor(async () => (await p.locator('#juncG .jn').count()) === 2, 5000), 'undo brings its junction back');
  ok(await done(u), 'the restored card is done');
  await sleep(3000);
  pr = got(u);
  ok((await rt(u)).runs === 1 && pr.length === 1, 'it started once (' + (await rt(u)).runs + ' runs, ' + pr.length + ' prompts)');
  ok(pr.length && pr[0].includes('result-from-A') && pr[0].includes('result-from-B'), 'with both results');
  await p.evaluate(i => deleteNodes([i]), u);
  await sleep(600);
  ok(await p.locator('#juncG .jn').count() === 1, 'canvas is back to one junction for T');

  // --- C is linked into the done T while it works: T waits for C alone, then runs with C only
  const c = await add({ name: 'C', agent: 'sh', cmd: 'sleep 4; echo result-from-C', prompt: 'go', x: 100, y: 900 }, true);
  await sleep(500);
  await p.evaluate(([c, t]) => send('edge_add', { from: c, to: t }), [c, t]);
  await sleep(800);
  r = await rt(t);
  // (no card is done while one before it is not: T waits again, and has not run again)
  ok(r.st === 'waiting' && r.runs === 1, 'T waits again while C works (' + r.st + ', ' + r.runs + ')');
  ok(r.arrived.length === 2 && r.need === 3, 'A and B count as in, C is to come (' + r.arrived.length + '/' + r.need + ')');
  ok(!/\barr\b/.test(await linkCls(c, t) || ''), 'the link from C is not drawn as arrived');
  ok(await waitFor(async () => (await rt(t)).runs === 2, 30000), 'C resolved: T runs again');
  ok(await done(t), 'T is done again');
  pr = got(t);
  ok(pr.length === 2 && pr[1].includes('result-from-C') && !pr[1].includes('result-from-A') && !pr[1].includes('result-from-B'), 'with C\'s result only');
  ok((await rt(a)).runs === 1 && (await rt(bb)).runs === 1, 'A and B did not run again');

  // --- archiving: when archiving, the junction leaves the canvas
  ok((await p.locator('#juncG .jn').count()) >= 1, 'junction is on the canvas before archiving');
  await p.evaluate(i => fileAway([i]), t);
  ok(await waitFor(async () => (await p.locator('#juncG .jn').count()) === 0, 5000), 'archiving T removes the junction from the canvas');
  await p.evaluate(() => undo());
  ok(await waitFor(async () => (await p.locator('#juncG .jn').count()) >= 1, 5000), 'undo brings T and its junction back');

  ok(errs.length === 0, 'no page errors ' + errs.join(' | '));
  await b.close();
  console.log(fails ? fails + ' FAILED' : 'all passed');
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
