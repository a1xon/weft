// Usage limits: an agent whose CLI stops at its limit sleeps until the reset instead of being "done"; the
// cards after it wait; it carries on by itself at the reset, on "try now", or a fresh agent takes over.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs');
const W = process.env.SANDBOX;
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
  const limitOf = id => p.evaluate(i => (S.rt.get(i) || {}).limit || null, id);
  const line = id => p.locator('.nw[data-id="' + id + '"] .stx').innerText().catch(() => '');

  await p.goto(fs.readFileSync(W + '/url.txt', 'utf8').trim()); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400);
  await p.keyboard.press('Escape'); await sleep(200);

  // --- a step of a chain stops at its limit: it sleeps, the card says until when, the next card waits
  const a = await add({ agent: 'claude', name: 'sleeper', prompt: 'HIT_LIMIT=2 Fix the slugify function in textkit/slug.py so that tests/test_slug.py passes.', x: 120, y: 80 });
  const a2 = await add({ agent: 'claude', name: 'after it', prompt: 'Summarise: {{inputs}}', x: 720, y: 80 });
  // the second pair: hit the limit, then hand over; the third: wakes up at the reset by itself
  const h = await add({ agent: 'claude', name: 'handed', prompt: 'HIT_LIMIT=30 Fix the wrap function in textkit/wrap.py.', x: 120, y: 480 });
  const h2 = await add({ agent: 'claude', name: 'after the handover', prompt: 'Summarise: {{inputs}}', x: 720, y: 480 });
  const z = await add({ agent: 'claude', name: 'waits it out', prompt: 'HIT_LIMIT=0 Say hello.', x: 1220, y: 80 });
  await p.evaluate(([x, y, u, v]) => { send('edge_add', { from: x, to: y }); send('edge_add', { from: u, to: v }); }, [a, a2, h, h2]);
  await sleep(300);
  await p.evaluate(ids => ids.forEach(id => send('arm', { id })), [a, h, z]);
  ok(await waitFor(async () => !!(await limitOf(a)), 60000), 'the agent at its limit sleeps instead of finishing');
  const l = await limitOf(a);
  ok(l && l.known && /resets \d/.test(l.said), 'the reset time was read from what the CLI said: "' + (l && l.said) + '"');
  ok(l && l.until > Date.now() + 60000, 'and it sleeps until then (' + Math.round(((l && l.until) - Date.now()) / 1000) + 's)');
  ok(await waitFor(async () => (await p.locator('.nw[data-id="' + a + '"] .eta.lim').count()) === 1, 5000), 'a LIMIT countdown sits on its card');
  const clockTxt = (await p.locator('.nw[data-id="' + a + '"] .eta.lim .et').innerText()).trim();
  ok(/^\d+:\d\d$/.test(clockTxt), 'counting down to the reset (' + clockTxt + ')');
  ok(/usage limit · resumes \d/.test(await line(a)), 'its status line says so: "' + await line(a) + '"');
  await sleep(4000);
  ok(!['done', 'failed'].includes(await st(a)), 'it is not taken for done (' + await st(a) + ')');
  ok(!['busy', 'done'].includes(await st(a2)), 'the card after it waits (' + await st(a2) + ')');
  ok(!(await p.evaluate(id => (S.rt.get(id).out || ''), a)).includes('hit your'), 'the limit message is no result');
  await p.screenshot({ path: W + '/limit-1-asleep.png' });

  // --- a click on the countdown: try now
  await p.locator('.nw[data-id="' + a + '"] .eta.lim').click(); await sleep(250);
  const items = await p.locator('.menu .mi').allInnerTexts();
  ok(items.some(t => /try now/.test(t)) && items.some(t => /hand over to Codex/.test(t)), 'it offers to try now or to hand over (' + items.map(t => t.replace(/\s+/g, ' ').trim()).join(' | ') + ')');
  await p.locator('.menu .mi', { hasText: 'try now' }).click();
  ok(await waitFor(async () => !(await limitOf(a)), 5000), 'try now wakes it');
  ok(await waitSt(a, ['done'], 60000), 'it carries on with its task and finishes (' + await st(a) + ')');
  ok((await p.evaluate(id => S.rt.get(id).out || '', a)).includes('slug.py'), 'with the result of the task it was in: "' + (await p.evaluate(id => S.rt.get(id).out || '', a)).slice(0, 60) + '"');
  ok(await waitSt(a2, ['done'], 60000), 'and the flow goes on (' + await st(a2) + ')');
  ok(await p.locator('.nw[data-id="' + a + '"] .eta.lim').count() === 0, 'the countdown is gone');

  // --- hand over: a fresh agent of another CLI takes the work and the links; the sleeper ends
  ok(await waitFor(async () => !!(await limitOf(h)), 30000), 'the second agent sleeps at its limit too');
  await p.locator('.nw[data-id="' + h + '"] .hd').click({ button: 'right' }); await sleep(250);
  ok(await p.locator('.menu .cap', { hasText: 'at its usage limit' }).count() === 1, 'its menu starts with what the limit allows');
  const before = await p.evaluate(() => new Set(S.nodes.keys()).size);
  await p.locator('.menu .mi', { hasText: 'hand over to Codex' }).click();
  ok(await waitFor(async () => (await p.evaluate(() => S.nodes.size)) > before, 8000), 'a new agent appears');
  const nid = await p.evaluate(old => [...S.nodes.values()].find(n => n.takeover === old).id, h);
  ok(await p.evaluate(id => S.nodes.get(id).agent, nid) === 'codex', 'of the CLI picked (' + await p.evaluate(id => S.nodes.get(id).agent, nid) + ')');
  ok(await waitSt(h, ['stopped'], 8000), 'the sleeper ends, so it never wakes up in the same files (' + await st(h) + ')');
  ok(await p.evaluate(([n, b2]) => [...S.edges.values()].some(e => e.from === n && e.to === b2), [nid, h2]), 'the link to the next card is the new agent\'s now');
  ok(await waitSt(nid, ['done'], 90000), 'the new agent does the work (' + await st(nid) + ')');
  ok(await waitSt(h2, ['done'], 60000), 'and the flow goes on from it (' + await st(h2) + ')');

  // --- nobody touches the third: at the reset (plus a margin) weft wakes it and it carries on
  ok(await waitFor(async () => !!(await limitOf(z)) || await st(z) === 'done', 30000), 'the third sleeps at its limit');
  const lz = await limitOf(z);
  ok(!lz || lz.until - Date.now() < 120000, 'it sleeps only until the reset it was told, plus a margin (' + (lz ? Math.round((lz.until - Date.now()) / 1000) + 's' : 'awake') + ')');
  ok(await waitSt(z, ['done'], 200000), 'at the reset it is woken and finishes by itself (' + await st(z) + ')');

  await p.screenshot({ path: W + '/limit-2-after.png' });
  ok(errs.length === 0, 'no page errors' + (errs.length ? ': ' + errs.join(' | ') : ''));
  await b.close();
  console.log(fails ? `${fails} FAILED` : 'all passed');
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
