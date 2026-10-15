// The archive: drop finished cards on it (bottom left), read and bring back what is filed, forget it;
// what is still at work stays; keep holds a card; lone cards and whole flows file themselves.
const { chromium } = require(process.env.PLAYWRIGHT);
const { execFileSync } = require('child_process');
const fs = require('fs');
const W = process.env.SANDBOX, BIN = process.env.WEFT_UI_BIN;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const ctx = await b.newContext({ viewport: { width: 1500, height: 880 } });
  const p = await ctx.newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message));
  const url = fs.readFileSync(W + '/url.txt', 'utf8').trim();
  const waitFor = async (fn, t = 30000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (await fn()) return true; await sleep(200); } return false; };
  const card = id => p.locator('.nw[data-id="' + id + '"]');
  const st = id => card(id).getAttribute('data-st');
  const add = (node, extra = {}) => p.evaluate(async a => (await req('node_add', a)).id, { node, run: true, ...extra });
  const filed = () => p.evaluate(() => (AR.groups || []).map(g => g.nodes.map(n => n.name).join('+')));
  const open = async () => { await p.goto('about:blank'); await p.goto(url); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400); await p.keyboard.press('Escape'); await sleep(200); };
  // drag a card by its header onto the archive box; `look` runs while it hovers there
  const dragToArchive = async (id, look) => {
    const hb = await card(id).locator('.hd').boundingBox();
    const sx = hb.x + 70, sy = hb.y + hb.height / 2;
    const tb = await p.locator('#arch .abox').boundingBox();
    const tx = tb.x + tb.width / 2, ty = tb.y + tb.height / 2;
    await p.mouse.move(sx, sy); await p.mouse.down();
    for (let i = 1; i <= 14; i++) { await p.mouse.move(sx + (tx - sx) * i / 14, sy + (ty - sy) * i / 14); await sleep(20); }
    if (look) await look();
    await p.mouse.up(); await sleep(400);
  };

  await open();
  // --- where it is: the bottom left corner, the zoom right above it
  const ab = await p.locator('#arch .abox').boundingBox(), zb = await p.locator('#zoom').boundingBox();
  ok(ab && ab.x < 40 && ab.y + ab.height > 880 - 60, 'the archive sits in the bottom left corner');
  ok(zb && zb.y + zb.height <= ab.y + 1 && Math.abs(zb.x - ab.x) < 4, 'the zoom moved up above it');
  ok((await p.locator('#arch .an').innerText()) === '', 'nothing is filed yet');

  // --- a finished card dropped on it is filed; ctrl+z brings it back where it was
  const alpha = await add({ name: 'alpha', agent: 'claude', prompt: 'Explain what textkit/slug.py does', x: 160, y: 140 });
  const slow = await add({ name: 'slow', agent: 'codex', prompt: 'LONG=120 count slowly', x: 160, y: 420 });
  ok(await waitFor(async () => (await st(alpha)) === 'done', 45000), 'alpha is done (' + await st(alpha) + ')');
  const said = await p.evaluate(id => S.rt.get(id).out, alpha);
  const at = id => p.evaluate(i => { const n = S.nodes.get(i); return n && [n.x, n.y]; }, id);
  const before = await at(alpha), view = await p.evaluate(() => ({ ...S.v }));
  await dragToArchive(alpha, async () => {
    ok(await p.locator('#arch.drag.over').count() === 1, 'the archive opens up as a drop zone under the card');
    ok((await p.locator('#arch .al').innerText()).includes('file it'), 'and says what letting go does');
    await p.screenshot({ path: W + '/archive-1-drop.png' });
  });
  ok(await waitFor(async () => (await card(alpha).count()) === 0, 5000), 'let go: the card leaves the canvas');
  ok((await p.locator('#arch .an').innerText()) === '1', 'the archive counts it');
  ok(await waitFor(async () => (await p.locator('#arch.drag').count()) === 0, 2000), 'and is a small box again: ' + await p.locator('#arch').getAttribute('class'));
  ok(await p.locator('.toast', { hasText: 'Filed "alpha"' }).count() === 1, 'a toast says it was filed and how to undo');
  await p.keyboard.press('Control+z');
  ok(await waitFor(async () => (await card(alpha).count()) === 1, 5000), 'ctrl+z brings it back');
  ok(JSON.stringify(await at(alpha)) === JSON.stringify(before), 'to where it was: ' + await at(alpha));
  ok(JSON.stringify(await p.evaluate(() => S.v)) === JSON.stringify(view), 'and the view did not jump (it was in sight)');
  ok(await st(alpha) === 'done' && (await p.evaluate(id => S.rt.get(id).out, alpha)) === said, 'done, with its result');

  // --- what is filed unfolds above the box: read it, bring it back
  await dragToArchive(alpha);
  ok(await waitFor(async () => (await card(alpha).count()) === 0, 5000), 'filed again');
  await p.click('#arch .abox');
  ok(await p.locator('#arch .apanel').isVisible(), 'a click on the archive unfolds it');
  const row = p.locator('#arch .arow', { hasText: 'alpha' });
  ok(await row.count() === 1, 'alpha is listed');
  await row.locator('.anm').click();
  const first = said.trim().split('\n').find(l => l.trim()).slice(0, 40);
  ok(await waitFor(async () => (await p.locator('#arch .aout').count()) === 1 && (await p.locator('#arch .aout').innerText()).includes(first), 5000), 'a click on the row shows what it said');
  await p.screenshot({ path: W + '/archive-2-open.png' });
  await row.locator('.fb.cy').click();
  ok(await waitFor(async () => (await card(alpha).count()) === 1, 5000), 'bring back puts the card on the canvas');
  ok(await p.locator('#arch .apanel').isHidden(), 'and folds the archive away');
  ok(await p.evaluate(id => S.sel.has(id), alpha), 'the card is selected');

  // --- what is still at work stays
  ok(await waitFor(async () => (await st(slow)) === 'busy', 30000), 'slow is working');
  await dragToArchive(slow);
  ok(await card(slow).count() === 1 && (await st(slow)) === 'busy', 'a card at work stays on the canvas');
  ok(await p.locator('.toast', { hasText: 'still at work' }).count() >= 1, 'and a toast says why');
  ok((await p.locator('#arch .an').innerText()) === '', 'nothing more was filed');

  // --- right-click: keep it, archive it; and forget it for good
  await card(alpha).locator('.nm').click({ button: 'right' }); await sleep(200);
  ok(await p.locator('.menu .mi', { hasText: 'keep on the canvas' }).count() === 1, 'the card menu offers keep');
  await p.locator('.menu .mi', { hasText: 'keep on the canvas' }).click(); await sleep(400);
  ok(await card(alpha).locator('.fil.on').count() === 1 && (await card(alpha).locator('.fil').innerText()).includes('kept'), 'a kept card says so on its footer');
  await card(alpha).locator('.nm').click({ button: 'right' }); await sleep(200);
  await p.locator('.menu .mi', { hasText: /^archive/ }).first().click();
  ok(await waitFor(async () => (await card(alpha).count()) === 0, 5000), 'archive in the menu files it too (kept or not)');
  await p.click('#arch .abox');
  await p.locator('#arch .arow .fb:not(.cy)').click();
  ok((await p.locator('#arch .arow .fb.no').innerText()).includes('forget for good'), 'forgetting asks once more');
  await p.locator('#arch .arow .fb.no').click();
  ok(await waitFor(async () => (await p.locator('#arch .arow').count()) === 0, 5000), 'and then it is gone');
  ok(await p.locator('#arch .aempty').isVisible(), 'the archive says it is empty');
  await p.keyboard.press('Escape'); await sleep(150);
  ok(await p.locator('#arch .apanel').isHidden(), 'escape folds it away');

  // --- by itself: a lone card soon after it is done, a flow once all of it is done; kept ones stay
  const cfgPath = W + '/cfg/weft/config.json';
  const cfg = JSON.parse(fs.readFileSync(cfgPath, 'utf8'));
  cfg.archive_s = 6; cfg.archive_flow_s = 9;
  fs.writeFileSync(cfgPath, JSON.stringify(cfg));
  execFileSync(BIN, ['stop', W + '/proj'], { env: process.env });
  execFileSync(BIN, ['open', W + '/proj', '--no-open'], { env: process.env });
  await open();
  // someone is at the canvas all along: the archive clock runs
  await p.evaluate(() => setInterval(() => send('here'), 10000));
  const lone = await add({ name: 'lone', agent: 'claude', prompt: 'Explain textkit in one line', x: 160, y: 140 });
  const held = await add({ name: 'held', agent: 'agy', prompt: 'Explain textkit in one line', keep: true, x: 160, y: 640 });
  const up = await add({ name: 'up', agent: 'claude', prompt: 'Say what textkit/slug.py is for', x: 700, y: 140 });
  const down = await add({ name: 'down', agent: 'claude', prompt: 'Summarize what you got' }, { from: up, run: false });
  await p.evaluate(([id, x, y]) => send('node_move', { moves: [{ id, x, y }] }), [down, 1200, 140]);
  ok(await card(held).locator('.fil.on').count() === 1, 'held is kept from the start');
  ok(await waitFor(async () => (await st(lone)) === 'done', 45000), 'lone is done');
  ok(await waitFor(async () => (await card(lone).locator('.fil:not([hidden])').count()) === 1, 3000), 'its footer says it goes to the archive soon: ' + await card(lone).locator('.fil').innerText().catch(() => ''));
  await card(lone).locator('.fil').click(); await sleep(300);
  ok(await card(lone).locator('.fil.on').count() === 1, 'one click keeps it');
  await card(lone).locator('.fil').click(); await sleep(300);
  ok(await card(lone).locator('.fil.on').count() === 0, 'another lets it go again');
  ok(await waitFor(async () => (await card(lone).count()) === 0, 30000), 'lone files itself');
  ok(await p.locator('.toast', { hasText: 'went to the archive' }).count() >= 1, 'a toast says where it went');
  ok(await waitFor(async () => (await st(down)) === 'done', 60000), 'the flow up -> down ran (' + await st(up) + ', ' + await st(down) + ')');
  ok(await waitFor(async () => (await card(up).count()) === 0 && (await card(down).count()) === 0, 30000), 'the whole flow files itself, together');
  const groups = await filed();
  ok(groups.includes('up+down') && groups.includes('lone'), 'as one drop, the lone card as another: ' + JSON.stringify(groups));
  ok(await st(held) === 'done' && (await card(held).count()) === 1, 'the kept card is still there');
  // the flow comes back with its link, and its clock starts again
  await p.click('#arch .abox');
  await p.locator('#arch .arow', { hasText: 'up' }).locator('.fb.cy').click();
  ok(await waitFor(async () => (await card(up).count()) === 1 && (await card(down).count()) === 1, 5000), 'brought back, both cards');
  ok(await p.evaluate(([a, z]) => [...S.edges.values()].some(e => e.from === a && e.to === z), [up, down]), 'with the link between them');
  ok(await p.evaluate(id => (S.rt.get(id).archive_at || 0) > Date.now() + 3000, up), 'and a new countdown');
  await p.screenshot({ path: W + '/archive-3-back.png' });
  ok(errs.length === 0, 'no page errors' + (errs.length ? ': ' + errs.join(' | ') : ''));
  await b.close();
  console.log(fails ? fails + ' FAILED' : 'ALL PASSED');
  process.exit(fails ? 1 : 0);
})();
