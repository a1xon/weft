// A link's end moves by dragging the link: the end nearer to where it was grabbed comes loose and
// goes where it is let go (a card: the link and its condition go there; empty space: the dial).
// Alt keeps the old link (a copy), a plain click still selects it, ctrl+z puts a move back.
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
  const st = id => p.evaluate(i => (S.rt.get(i) || {}).status, id);
  const links = () => p.evaluate(() => [...S.edges.values()].map(e => `${e.from}>${e.to}:${e.on}`).sort().join(' '));
  const has = async (f, t) => (await links()).split(' ').some(x => x.startsWith(`${f}>${t}:`));
  const got = id => { const f = W + '/proj/.weft/run/received-' + id + '.log'; return fs.existsSync(f) ? fs.readFileSync(f, 'utf8').split('=== prompt ===').slice(1) : []; };
  // a point on the link a share of the way along it, in the page's pixels
  const onLink = (f, t, share) => p.evaluate(([f, t, share]) => {
    const e = [...S.edges.values()].find(x => x.from === f && x.to === t);
    const el = e && document.getElementById('p-' + e.id); if (!el) return null;
    const q = el.getPointAtLength(el.getTotalLength() * share).matrixTransform(el.getScreenCTM());
    return { x: q.x, y: q.y };
  }, [f, t, share]);
  const mid = id => p.evaluate(i => { const r = nodeEls.get(i).root.getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + Math.min(r.height / 2, 60) }; }, id);
  const drag = async (from, to, alt) => {
    await p.mouse.move(from.x, from.y); await p.mouse.down();
    if (alt) await p.keyboard.down('Alt');
    await p.mouse.move(from.x + 12, from.y + 12, { steps: 3 });
    await p.mouse.move(to.x, to.y, { steps: 12 });
    await p.mouse.up();
    if (alt) await p.keyboard.up('Alt');
    await sleep(700);
  };
  // a spot near (x0, y0) with nothing on it but the canvas
  const emptyNear = at => p.evaluate(({ x: x0, y: y0 }) => {
    for (let r = 120; r < 700; r += 40) for (let a = 0; a < 360; a += 20) {
      const x = x0 + r * Math.cos(a * Math.PI / 180), y = y0 + r * Math.sin(a * Math.PI / 180);
      if (x < 60 || y < 110 || x > innerWidth - 60 || y > innerHeight - 200) continue;
      const t = document.elementFromPoint(x, y);
      if (t && (t.closest('#grid') || ['canvas', 'world', 'edges', 'nodes'].includes(t.id))) return { x, y };
    }
    return null;
  }, at);
  const unfocus = () => p.evaluate(() => document.activeElement && document.activeElement.blur());
  const toast = () => p.evaluate(() => [...document.querySelectorAll('.toast')].map(t => t.textContent).join(' | '));

  await p.goto(url); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400); await p.keyboard.press('Escape');
  const a = await add({ name: 'A', agent: 'sh', cmd: 'sleep 5; echo result-from-A', prompt: 'go', x: 100, y: 100 }, true);
  const bb = await add({ name: 'B', agent: 'claude', prompt: 'Take:\n{{inputs}}', addenda: [], x: 950, y: 100 }, false);
  const c = await add({ name: 'C', agent: 'claude', prompt: 'Take:\n{{inputs}}', addenda: [], x: 950, y: 560 }, false);
  await p.evaluate(([a, b]) => req('edge_add', { from: a, to: b, on: 'ok' }), [a, bb]);
  await sleep(500); await p.evaluate(() => fit()); await sleep(600);

  // --- a plain click on the link selects it, as before
  const pt = await onLink(a, bb, 0.3);
  await p.mouse.click(pt.x, pt.y);
  ok(await p.evaluate(() => !!S.selEdge && !$('#edgebar').hidden), 'a click on the link selects it');
  await p.keyboard.press('Escape'); await p.evaluate(() => select([]));

  // --- the head end, grabbed near B, goes onto C while A still works: C gets A's result, B nothing
  await drag(await onLink(a, bb, 0.85), await mid(c));
  ok(await has(a, c) && !(await has(a, bb)), 'the link goes A > C, A > B is gone (' + await links() + ')');
  ok((await links()).includes(`${a}>${c}:ok`), 'it keeps its condition (only if ok)');
  ok(/link moved to C/.test(await toast()), 'a toast says where it went (' + await toast() + ')');
  ok(await p.evaluate(() => !document.querySelector('.ed.moving')), 'no link is left dimmed');
  ok(await waitFor(async () => await st(c) === 'done', 60000), 'C ran when A was done');
  ok(got(c).length === 1 && got(c)[0].includes('result-from-A'), 'with A\'s result');
  ok(got(bb).length === 0 && await st(bb) !== 'done', 'B got nothing');

  // --- ctrl+z puts it back: A > B again, and A, done, hands B its result
  await unfocus(); await p.keyboard.press('Control+z'); await sleep(800);
  ok(await has(a, bb) && !(await has(a, c)), 'undo: A > B is back, A > C is gone (' + await links() + ')');
  ok((await links()).includes(`${a}>${bb}:ok`), 'with its condition');
  ok(await waitFor(async () => got(bb).length === 1, 30000) && got(bb)[0].includes('result-from-A'), 'B gets A\'s result at once');

  // --- Alt: the tail end, grabbed near A, copied onto D: D > B joins A > B
  const d = await add({ name: 'D', agent: 'sh', cmd: 'echo result-from-D', prompt: 'go', x: 100, y: 560 }, true);
  ok(await waitFor(async () => await st(d) === 'done'), 'D is done');
  await p.evaluate(() => fit()); await sleep(600);
  await drag(await onLink(a, bb, 0.15), await mid(d), true);
  ok(await has(a, bb) && await has(d, bb), 'Alt kept A > B and made D > B (' + await links() + ')');
  ok(/link copied to D/.test(await toast()), 'the toast says copied');
  await unfocus(); await p.keyboard.press('Control+z'); await sleep(800);
  ok(await has(a, bb) && !(await has(d, bb)), 'undo of a copy removes only the copy (' + await links() + ')');

  // --- empty space: the dial for the source, without "hand off" (no link); Esc leaves the link as it was
  const e = await add({ name: 'E', agent: 'claude', prompt: 'say hi', addenda: [], x: 100, y: 1000 }, true);
  ok(await waitFor(async () => await st(e) === 'done', 60000), 'E is done');
  const f = await add({ name: 'F', agent: 'claude', prompt: 'Take:\n{{inputs}}', addenda: [], x: 950, y: 1000, draft: true }, false);
  await p.evaluate(([e, f]) => req('edge_add', { from: e, to: f }), [e, f]);
  await sleep(500); await p.evaluate(() => fit()); await sleep(600);
  const empty = await emptyNear(await onLink(e, f, 0.85));
  ok(!!empty, 'there is empty canvas near the link');
  await drag(await onLink(e, f, 0.85), empty);
  ok(await p.evaluate(() => !$('#dial').hidden), 'letting go in empty space opens the dial');
  const keys = await p.evaluate(() => [...document.querySelectorAll('#dial .dpill')].map(x => x.dataset.k).join(','));
  ok(keys === 'queue,git', 'its options make a link: no hand off (' + keys + ')');
  await p.keyboard.press('Escape'); await sleep(400);
  ok(await has(e, f) && await p.evaluate(() => !document.querySelector('.ed.moving')), 'Esc: the link stays as it was');
  const before = await p.evaluate(() => [...S.nodes.keys()]);
  await drag(await onLink(e, f, 0.85), empty);
  await p.keyboard.press('ArrowRight'); await p.keyboard.press('Enter');
  let q = null;
  ok(await waitFor(async () => { q = (await p.evaluate(() => [...S.nodes.keys()])).find(i => !before.includes(i)); return !!q; }, 5000), 'queue: a prompt card is made there');
  ok(await waitFor(async () => await has(e, q) && !(await has(e, f))), 'the link goes E > the new card, E > F is gone (' + await links() + ')');
  ok(await p.evaluate(i => S.nodes.get(i).agent === 'step', q), 'it is a prompt queued on E');

  ok(errs.length === 0, 'no page errors ' + errs.join(' | '));
  await b.close();
  console.log(fails ? fails + ' FAILED' : 'all passed');
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
