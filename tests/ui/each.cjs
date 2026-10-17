// "For each" (src/engine/each.rs): a link switched to it in its bar asks the planner to end with its
// jobs as a numbered list; the card after it takes the first job and weft makes a copy of it for each
// other one, all at work at the same time, and the card after them runs once with every result.
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
  const rt = id => p.evaluate(i => { const r = S.rt.get(i) || {}; return { st: r.status, runs: r.runs }; }, id);
  const done = async id => waitFor(async () => (await rt(id)).st === 'done', 60000);
  const got = id => { const f = W + '/proj/.weft/run/received-' + id + '.log'; return fs.existsSync(f) ? fs.readFileSync(f, 'utf8').split('=== prompt ===').slice(1) : []; };
  const copiesOf = id => p.evaluate(i => [...S.nodes.values()].filter(n => n.each_of === i).sort((a, b) => a.part - b.part).map(n => ({ id: n.id, name: n.name, part: n.part })), id);
  const edge = (f, t) => p.evaluate(([f, t]) => [...S.edges.values()].find(e => e.from === f && e.to === t) || null, [f, t]);
  const onLink = (f, t, share) => p.evaluate(([f, t, share]) => {
    const e = [...S.edges.values()].find(x => x.from === f && x.to === t);
    const el = e && document.getElementById('p-' + e.id); if (!el) return null;
    const q = el.getPointAtLength(el.getTotalLength() * share).matrixTransform(el.getScreenCTM());
    return { x: q.x, y: q.y };
  }, [f, t, share]);

  await p.goto(url); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400); await p.keyboard.press('Escape');
  const plan = await add({ name: 'plan', agent: 'claude', prompt: 'find the bugs', addenda: [], x: 100, y: 100 }, false);
  const fix = await add({ name: 'fixer', agent: 'claude', prompt: 'Fix this:\n{{inputs}}', addenda: [], x: 850, y: 100 }, false);
  const sum = await add({ name: 'report', agent: 'claude', prompt: 'Sum up:\n{{inputs}}', addenda: [], x: 1600, y: 100 }, false);
  await p.evaluate(([a, b, c]) => { send('edge_add', { from: a, to: b }); send('edge_add', { from: b, to: c }); }, [plan, fix, sum]);
  await sleep(500); await p.evaluate(() => fit()); await sleep(600);

  // --- the link's bar switches it to "for each"
  const pt = await onLink(plan, fix, 0.3);
  await p.mouse.click(pt.x, pt.y);
  const btn = p.locator('#edgebar button', { hasText: 'for each job it lists' });
  ok(await btn.count() === 1, 'the link bar offers "for each"');
  await btn.click();
  ok(await waitFor(async () => (await edge(plan, fix) || {}).each === true, 5000), 'the link is "for each" now');
  ok(await p.evaluate(([f, t]) => { const e = [...S.edges.values()].find(x => x.from === f && x.to === t); const g = document.getElementById('p-' + e.id).parentNode; return g.classList.contains('each') && g.querySelector('.eachtx').textContent === 'for each'; }, [plan, fix]),
    'and drawn so: a badge says "for each"');
  ok(/numbered list/.test(await p.evaluate(() => [...document.querySelectorAll('.toast')].map(t => t.textContent).join(' '))), 'a tip says what it does');
  await p.keyboard.press('Escape'); await p.evaluate(() => select([]));

  // --- the planner runs: it is asked for its jobs, and each job gets its own card
  await p.evaluate(i => runNode(i), plan);
  ok(await done(plan), 'the planner is done');
  ok(got(plan).length === 1 && got(plan)[0].includes('The next step runs once for each job you list'), 'it was asked to end with the jobs');
  let cs = [];
  ok(await waitFor(async () => (cs = await copiesOf(fix)).length === 2, 20000), 'two copies of the fixer for three jobs (' + JSON.stringify(cs) + ')');
  ok(cs.length === 2 && cs[0].name === 'fixer 2' && cs[1].part === 2, 'named after it, in the order of the list');
  for (const c of cs) ok(!!(await edge(plan, c.id) || {}).each && !!(await edge(c.id, sum)), `${c.name} is linked from the planner and on to the report`);
  ok(await waitFor(async () => [fix, ...cs.map(c => c.id)].every(i => got(i).length >= 1), 20000), 'all three got their job at the same time');
  const job = id => (got(id)[0] || '');
  ok(job(fix).includes('item 1 of 3') && job(fix).includes('1. Fix alpha'), 'the fixer takes job 1');
  ok(cs.length === 2 && job(cs[0].id).includes('item 2 of 3') && job(cs[0].id).includes('2. Fix beta'), 'its first copy job 2');
  ok(cs.length === 2 && job(cs[1].id).includes('item 3 of 3') && job(cs[1].id).includes('3. Fix gamma'), 'its second copy job 3');

  // --- the report waits for all of them and runs once, with every result
  ok(await done(sum), 'the report is done');
  const r = got(sum);
  ok(r.length === 1 && (await rt(sum)).runs === 1, 'it ran once (' + r.length + ' prompts)');
  ok(r.length && (r[0].match(/### Output of "/g) || []).length === 3, 'with three results');
  ok(await p.evaluate(i => { const j = document.querySelector(`.jn`); return !!j; }, sum), 'its links meet in an AND ring');

  // --- back to a plain link (the link menu): the copies stay, their links from the planner go
  const pt2 = await onLink(plan, fix, 0.3);
  await p.mouse.click(pt2.x, pt2.y, { button: 'right' });
  await p.locator('.menu .mi', { hasText: 'for each job it lists' }).click();
  ok(await waitFor(async () => !(await edge(plan, fix)).each && !(await edge(plan, cs[0].id)), 5000), 'the menu switches it back, and the copies are no longer fed by it');
  ok((await copiesOf(fix)).length === 2, 'the copies stay, with what they did');

  ok(errs.length === 0, 'no page errors ' + errs.join(' | '));
  await b.close();
  console.log(fails ? fails + ' FAILED' : 'all passed');
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
