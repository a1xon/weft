// The CI check: after a push it waits for the CI runs of the pushed commit (a fake gh whose "CI" runs the
// unit tests on that commit); red goes back to the agent that did the work, which fixes, commits and
// pushes; the runs of the new commit pass and the flow goes on. From the tray it lands on a link.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs');
const W = process.env.SANDBOX;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const p = await (await b.newContext({ viewport: { width: 1800, height: 1000 } })).newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type() === 'error' && errs.push(m.text()));
  const st = id => p.locator('.nw[data-id="' + id + '"]').getAttribute('data-st');
  const waitSt = async (id, want, t = 60000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (want.includes(await st(id))) return true; await sleep(250); } return false; };
  const waitFor = async (fn, t = 30000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (await fn()) return true; await sleep(200); } return false; };
  const add = async node => p.evaluate(nd => new Promise(res => {
    const before = new Set(S.nodes.keys());
    send('node_add', { node: nd });
    const t = setInterval(() => { const id = [...S.nodes.keys()].find(k => !before.has(k)); if (id) { clearInterval(t); res(id); } }, 50);
  }), node);
  const line = id => p.locator('.nw[data-id="' + id + '"] .stx').innerText().catch(() => '');
  const rt = id => p.evaluate(i => S.rt.get(i) || {}, id);

  await p.goto(fs.readFileSync(W + '/url.txt', 'utf8').trim()); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400);
  await p.keyboard.press('Escape'); await sleep(200);
  const chips = await p.locator('#tray .tchip').allInnerTexts();
  ok(chips.some(t => t.trim() === 'CI'), 'the tray offers a CI check (' + chips.map(t => t.trim()).join(' | ') + ')');

  // --- build -> commit -> push -> CI -> next: the builder fixes only slug.py, so CI (all the tests) goes red
  const a = await add({ agent: 'claude', name: 'builder', prompt: 'Fix the slugify function in textkit/slug.py so that tests/test_slug.py passes.', x: 60, y: 80 });
  const c = await add({ agent: 'git', git_op: 'commit', x: 560, y: 80 });
  const ps = await add({ agent: 'git', git_op: 'push', x: 960, y: 80 });
  const g = await add({ agent: 'tests', name: 'CI', write_tests: false, ci: true, x: 1360, y: 80 });
  const z = await add({ agent: 'claude', name: 'after CI', prompt: 'Summarise: {{inputs}}', x: 1360, y: 480 });
  await p.evaluate(([a1, c1, p1, g1, z1]) => {
    send('edge_add', { from: a1, to: c1 }); send('edge_add', { from: c1, to: p1 }); send('edge_add', { from: p1, to: g1 }); send('edge_add', { from: g1, to: z1, on: 'ok' });
  }, [a, c, ps, g, z]);
  await sleep(300);
  ok((await p.locator('.nw[data-id="' + g + '"] .hd').innerText()).includes('gate · CI'), 'the card says it is a CI gate');
  ok(/waits for (a|the) push/.test(await line(g)), 'before anything is pushed it waits for the push: "' + await line(g) + '"');
  await p.evaluate(id => send('arm', { id }), ps);
  await p.evaluate(() => send('run'));
  ok(await waitSt(ps, ['done'], 90000), 'the work is committed and pushed (' + await st(ps) + ')');
  ok(await waitFor(async () => (await p.locator('.nw[data-id="' + g + '"] .cipill').count()) > 0, 20000), 'the CI runs of the pushed commit show on the card');
  ok(await waitFor(async () => /watching CI/.test(await line(g)) || (await p.locator('.nw[data-id="' + g + '"] .cipill.run').count()) > 0, 5000), 'while they run: "' + await line(g) + '"');
  await p.screenshot({ path: W + '/ci-1-watching.png' });
  const sha1 = (await rt(g)).ci && (await rt(g)).ci.sha;
  ok(await waitFor(async () => /is fixing what CI found/.test(await line(g)), 30000), 'red goes back to the agent that did the work: "' + await line(g) + '"');
  ok(await waitSt(g, ['done'], 120000), 'the runs of the new commit pass, and the check opens (' + await st(g) + ': ' + await line(g) + ')');
  const got = fs.readFileSync(W + '/proj/.weft/run/received-' + a + '.log', 'utf8');
  ok(/CI failed on what was pushed/.test(got) && /test_wrap|wrap/.test(got), 'it got the failing part of the log');
  ok(/commit the fix and push/i.test(got), 'and was asked to commit and push the fix');
  const sha2 = (await rt(g)).ci && (await rt(g)).ci.sha;
  ok(sha1 && sha2 && sha1 !== sha2, 'on the new commit (' + (sha1 || '').slice(0, 7) + ' -> ' + (sha2 || '').slice(0, 7) + ')');
  ok((await p.locator('.nw[data-id="' + g + '"] .cipill.ok').count()) > 0, 'its run is green on the card');
  ok(await waitSt(z, ['done'], 60000), 'the flow goes on (' + await st(z) + ')');
  ok(/CI passed/.test(fs.readFileSync(W + '/proj/.weft/run/received-' + z + '.log', 'utf8')), 'and the next agent hears that CI passed');
  await p.screenshot({ path: W + '/ci-2-passed.png' });

  // --- from the tray: a CI chip dropped on a link becomes a CI gate on it
  const x1 = await add({ agent: 'claude', name: 'one', prompt: 'say one', x: 60, y: 700, draft: true });
  const x2 = await add({ agent: 'claude', name: 'two', prompt: 'say two', x: 760, y: 700, draft: true });
  await p.evaluate(([u, v]) => send('edge_add', { from: u, to: v }), [x1, x2]);
  await sleep(500);
  await p.evaluate(() => fit()); await sleep(600);
  const eid = await p.evaluate(([u, v]) => [...S.edges.values()].find(e => e.from === u && e.to === v).id, [x1, x2]);
  const chip = await p.locator('#tray .tchip[data-k=ci]').boundingBox();
  const path = p.locator('.ed[data-edge="' + eid + '"] path').first();
  const pt = await path.evaluate(el => { const l = el.getTotalLength(), q = el.getPointAtLength(l / 2), m = el.getScreenCTM(); return { x: q.x * m.a + m.e, y: q.y * m.d + m.f }; });
  await p.mouse.move(chip.x + chip.width / 2, chip.y + chip.height / 2); await p.mouse.down();
  await p.mouse.move(pt.x, pt.y, { steps: 14 }); await p.mouse.up();
  ok(await waitFor(async () => p.evaluate(([u, v]) => [...S.nodes.values()].some(n => n.ci && n.agent === 'tests' && n.write_tests === false
    && [...S.edges.values()].some(e => e.from === u && e.to === n.id) && [...S.edges.values()].some(e => e.from === n.id && e.to === v)), [x1, x2]), 6000), 'dropped on a link, the CI chip makes a CI gate there');

  ok(errs.length === 0, 'no page errors' + (errs.length ? ': ' + errs.join(' | ') : ''));
  await b.close();
  console.log(fails ? `${fails} FAILED` : 'all passed');
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
