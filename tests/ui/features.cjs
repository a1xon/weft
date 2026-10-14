const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs'); const W = process.env.SANDBOX;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const p = await (await b.newContext({ viewport: { width: 1700, height: 900 } })).newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type() === 'error' && errs.push(m.text()));
  const nodes = () => p.locator('.nw').count();
  const edges = () => p.locator('#edgeG .ed').count();
  const st = id => p.locator('.nw[data-id="' + id + '"]').getAttribute('data-st');
  const waitSt = async (id, want, t = 30000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (want.includes(await st(id))) return true; await sleep(250); } return false; };
  const emptySpot = async (want = 1) => p.evaluate(() => {
    const rects = [...document.querySelectorAll('.nw, .menu, #fly, #omniw, #qpanel, #top, #zoom, #toasts')].map(e => e.getBoundingClientRect());
    const out = [];
    for (let y = 130; y < 700; y += 60) for (let x = 120; x < 1650; x += 60) {
      if (!rects.every(r => x < r.left - 30 || x > r.right + 30 || y < r.top - 30 || y > r.bottom + 30)) continue;
      // nor on a link: its hit area is wide and a right-click there is the link's menu
      const near = [[0, 0], [20, 0], [-20, 0], [0, 20], [0, -20]].some(([dx, dy]) => { const t = document.elementFromPoint(x + dx, y + dy); return t && t.closest && t.closest('#edgeG'); });
      if (!near) out.push({ x, y });
    }
    return out.length ? out[Math.floor(out.length / 2)] : { x: 1650, y: 120 };
  });
  const port = async (id, side) => (await p.locator('.nw[data-id="' + id + '"] .port.' + side).boundingBox());
  await p.goto(fs.readFileSync(W + '/url.txt', 'utf8').trim()); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400);
  await p.keyboard.press('Escape'); await p.waitForSelector('#empty', { state: 'visible' }); await sleep(200);

  // --- A: omnibar creates and runs an agent (0 clicks)
  await p.locator('#omni textarea').click();
  await p.keyboard.type('Explain what textkit/slug.py does in one sentence.'); await p.keyboard.press('Enter');
  await p.waitForSelector('.nw[data-id="n1"]');
  ok(await waitSt('n1', ['busy', 'starting', 'done']), 'typing in the omnibar creates and runs an agent');
  { const okd = await waitSt('n1', ['done']);
    ok(okd, 'it finishes (' + await st('n1') + ')');
    if (!okd && process.env.WEFT_UI_BIN) { try { console.log(require('child_process').execFileSync(process.env.WEFT_UI_BIN, ['read', 'n1', '-n', '30'], { cwd: W + '/proj', encoding: 'utf8' })); } catch (e) { console.log('read failed', String(e.message).slice(0, 200)); } } }
  await p.screenshot({ path: W + '/f-1-first.png' });

  // --- B: click the diamond: add the next step beside it, linked, prompt focused
  const n0 = await nodes(), e0 = await edges();
  let o = await port('n1', 'out');
  await p.mouse.click(o.x + o.width / 2, o.y + o.height / 2);
  await p.waitForSelector('#nextbar:not([hidden]) textarea');
  await p.locator('#nextbar .seg button', { hasText: 'branch' }).click(); // a new agent that receives the result
  await p.keyboard.press('Enter');
  await p.waitForSelector('.nw[data-id="n2"] .ask-edit');
  ok((await nodes()) === n0 + 1 && (await edges()) === e0 + 1, 'the diamond + branch adds a linked next step');
  await sleep(350);
  ok((await p.locator('.nw[data-id="n2"]').getAttribute('data-st')) === 'draft', 'the new step is a draft until you press enter');
  await p.keyboard.type('Review the explanation above. Reply with one improvement.');
  await p.keyboard.press('Enter');
  ok(await waitSt('n2', ['busy', 'starting', 'done']), 'enter arms it and, because the result already exists, it runs at once (' + await st('n2') + ')');
  { const okd = await waitSt("n2", ["done"]); ok(okd, "second step done" + (okd ? "" : " (state: " + await st("n2") + ")")); }

  // --- C: drag a link to nowhere: same agent spawns right there; esc cancels it
  const n1c = await nodes(), e1c = await edges();
  o = await port('n2', 'out');
  await p.mouse.move(o.x + 6, o.y + 6); await p.mouse.down(); await p.mouse.move(o.x + 250, o.y + 12, { steps: 10 });
  await p.screenshot({ path: W + '/f-2-dragging.png' });
  await p.mouse.up();
  ok(await p.locator('#dial .dpill[data-k=queue]').isVisible(), 'letting go in empty space opens the dial');
  await p.locator('#dial .dpill[data-k=queue]').click(); await sleep(500);
  ok((await nodes()) === n1c + 1 && (await edges()) === e1c + 1, 'dropping a link on empty canvas spawns a card there, linked');
  const sp = await p.locator('.ask-edit').count();
  ok(sp === 1, 'its prompt editor is focused and ready');
  await p.keyboard.press('Escape'); await sleep(500);
  ok((await nodes()) === n1c && (await edges()) === e1c, 'escape cancels the spawn: nothing left behind');

  // --- D: same again, this time keep it
  o = await port('n2', 'out');
  await p.mouse.move(o.x + 6, o.y + 6); await p.mouse.down(); await p.mouse.move(o.x + 250, o.y + 12, { steps: 10 }); await p.mouse.up();
  await p.locator('#dial .dpill[data-k=queue]').click();
  await p.waitForSelector('.ask-edit');
  await p.keyboard.type('Summarize the review in five words.'); await p.keyboard.press('Enter');
  await sleep(300);
  const idD = (await p.locator('.nw').evaluateAll(ns => ns.map(n => n.dataset.id))).pop();
  ok(await waitSt(idD, ['busy', 'starting', 'done']), 'the spawned step runs on enter (' + idD + ')');

  // --- E: alt-drag clones a node
  const nCl = await nodes();
  const hd = await p.locator('.nw[data-id="n1"] .hd').boundingBox();
  await p.keyboard.down('Alt');
  await p.mouse.move(hd.x + 100, hd.y + 12); await p.mouse.down(); await p.mouse.move(hd.x + 100, hd.y + 330, { steps: 6 }); await p.mouse.up();
  await p.keyboard.up('Alt'); await sleep(700);
  ok((await nodes()) === nCl + 1, 'alt-drag clones the node');

  // --- F: double-click a link to splice a step into it
  const nSp = await nodes(), eSp = await edges();
  const mid = await p.locator('#edgeG .ed').first().locator('.vis').evaluate(el => { const l = el.getTotalLength(); const pt = el.getPointAtLength(l * 0.28); const m = el.getScreenCTM(); return { x: pt.x * m.a + m.e, y: pt.y * m.d + m.f }; });
  await p.mouse.dblclick(mid.x, mid.y); await sleep(700);
  ok((await nodes()) === nSp + 1 && (await edges()) === eSp + 1, 'double-clicking a link splices a new step into it');
  await p.keyboard.press('Escape'); await sleep(300);
  await p.screenshot({ path: W + '/f-3-built.png' });

  // --- G: note: right-click canvas -> note -> type -> ctrl+enter
  let sp_ = await emptySpot();
  await p.mouse.click(sp_.x, sp_.y, { button: 'right' });
  await p.locator('.menu .mi', { hasText: /^note/ }).first().click();
  await p.waitForSelector('.note-edit');
  await p.keyboard.type('House rules: answer in at most ten words.'); await p.keyboard.press('Control+Enter'); await sleep(400);
  ok(await p.locator('.nw[data-kind=note] .ans').innerText().then(t => t.includes('House rules')), 'a note holds free text');

  // --- H: gate: right-click canvas -> approve; link; it holds until a human clicks
  await p.mouse.click(900, 600); await p.keyboard.press('f'); await sleep(600); // everything in view again
  sp_ = await emptySpot();
  await p.mouse.click(sp_.x, sp_.y, { button: 'right' });
  await p.locator('.menu .mi', { hasText: /^approve/ }).click();
  await p.waitForSelector('.nw[data-kind=gate]');
  ok(true, 'an approval gate can be placed');
  const gid = await p.locator('.nw[data-kind=gate]').getAttribute('data-id');
  const o2 = await port('n2', 'out'); const gh = await p.locator('.nw[data-id="' + gid + '"] .hd').boundingBox();
  await p.mouse.move(o2.x + 6, o2.y + 6); await p.mouse.down(); await p.mouse.move(gh.x + 80, gh.y + 14, { steps: 8 }); await p.mouse.up();
  ok(await waitSt(gid, ['attention']), 'the gate holds the result and asks for approval (' + await st(gid) + ')');
  await p.screenshot({ path: W + '/f-4-gate.png' });
  await p.locator('.nw[data-id="' + gid + '"] .fb.go').click(); await sleep(500);
  ok((await st(gid)) === 'done', 'approve lets the flow move on');

  // --- I: unfold a result under the card (no side panel)
  await p.locator('.nw[data-id="n1"] .ans').click(); await sleep(500);
  ok(await p.locator('#fly').isVisible(), 'clicking a result unfolds it attached to the card');
  const fb = await p.locator('#fly').boundingBox(), cb = await p.locator('.nw[data-id="n1"]').boundingBox();
  ok(Math.abs(fb.x - cb.x) < 40 && fb.y > cb.y, 'the flyout sits right under its card (' + Math.round(fb.x - cb.x) + 'px off, below)');
  await p.screenshot({ path: W + '/f-5-flyout.png' });
  await p.keyboard.press('Escape'); await sleep(200);

  // --- J: menus
  await p.locator('.nw[data-id="n1"] .hd').click({ button: 'right' }); await sleep(200);
  const items = await p.locator('.menu .mi').allInnerTexts();
  ok(items.some(t => /fork/.test(t)) && items.some(t => /hand over to a fresh agent/.test(t)) && items.some(t => /keep result as a note/.test(t)), 'right-click on an agent offers fork / then / hand over / keep as note');
  await p.screenshot({ path: W + '/f-6-menu.png' });
  await p.keyboard.press('Escape'); await sleep(150);

  // --- K: gather two into one
  await p.mouse.click(1100, 40); // neutral
  const a = p.locator('.nw[data-id="n1"] .hd'), c = p.locator('.nw[data-id="n2"] .hd');
  await a.click(); await c.click({ modifiers: ['Shift'] });
  const nG = await nodes(), eG = await edges();
  await p.keyboard.press('g'); await sleep(900);
  ok((await nodes()) === nG + 1 && (await edges()) === eG + 2, 'g gathers the selection into one new agent (two links in)');
  await p.keyboard.press('Escape'); await sleep(300);

  // --- L: queue: message a busy agent twice, see the queue, reorder/remove
  await p.mouse.click(1100, 40);
  await p.locator('.nw[data-id="n1"] .hd').click();
  await p.locator('#omni textarea').click();
  await p.keyboard.type('Now answer with only the file name.'); await p.keyboard.press('Enter'); await sleep(500);
  await p.keyboard.type('Then say done.'); await p.keyboard.press('Enter'); await sleep(500);
  const queued = await p.locator('#qpanel .qi').count();
  ok(queued >= 1, 'messages to a busy agent queue up and show as a list (' + queued + ')');
  await p.screenshot({ path: W + '/f-7-queue.png' });

  // --- M: library modal
  await p.keyboard.press('Escape'); await p.locator('#bLib').click(); await sleep(300);
  ok(await p.locator('#modal .li').count() >= 8, 'the prompt library opens');
  await p.keyboard.press('Escape');

  console.log('errors:', JSON.stringify(errs));
  ok(errs.length === 0, 'no page errors');
  await b.close();
  console.log(fails ? fails + ' FAILED' : 'ALL PASSED');
})().catch(e => { console.error('CRASH', String(e).slice(0, 1200)); process.exit(1); });
