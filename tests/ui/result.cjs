// The result on a card: small and close like the live peek, the reader's formatting, the result / live
// switch covering none of it. A report reads from its start; a result weft only read off the screen (no
// report) from its end, and while the terminal is still there the card shows that instead.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs');
const W = process.env.SANDBOX;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
const REPORT = [
  '## Done: digits stay in slugs',
  '',
  '',
  'The slug function now keeps **digits** and drops `--` runs.',
  '',
  '| file | lines |',
  '|------|------:|',
  '| textkit/slug.py | 12 and a long note on why digits must stay because the old tests expected them to vanish |',
  '',
  '---',
  '- Left open: the old slugs in the database, which still have no digits and may need a migration later on',
  '```',
  'def slug(s): ...',
  '```',
].join('\n');
(async () => {
  fs.writeFileSync(W + '/report.md', REPORT);
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const p = await (await b.newContext({ viewport: { width: 1600, height: 900 } })).newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message));
  const waitFor = async (fn, t = 30000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (await fn()) return true; await sleep(200); } return false; };
  await p.goto(fs.readFileSync(W + '/url.txt', 'utf8').trim()); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400);
  await p.keyboard.press('Escape'); await sleep(200);
  const rt = id => p.evaluate(id => S.rt.get(id) || {}, id);
  const card = id => p.locator(`.nw[data-id="${id}"]`);

  // --- a report handed in with `weft done`
  await p.evaluate(W => send('node_add', { node: { name: 'reporter', agent: 'claude', prompt: `Fix the slug of titles with digits. REPORT_FILE=${W}/report.md`, addenda: ['report-done'], x: 200, y: 200 }, run: true }), W);
  ok(await waitFor(async () => (await rt('n1')).status === 'done', 60000), 'the reporter is done');
  await p.evaluate(() => { const n = S.nodes.get('n1'); S.v = { x: 60 - n.x, y: 120 - n.y, k: 1 }; applyView(); }); await sleep(300);
  const ans = card('n1').locator('.ans');
  ok(await ans.isVisible() && await card('n1').locator('.live').isHidden(), 'a finished card shows its result');
  ok(!(await rt('n1')).out_read && !(await ans.getAttribute('class')).includes('tail'), 'a report reads from its start');
  const text = await ans.innerText();
  ok(/^Done: digits stay in slugs/.test(text) && !/##|\*\*|```|---/.test(text), 'headings, bold, fences and rules lose their marks (' + text.slice(0, 60).replace(/\n/g, ' | ') + ')');
  ok(await ans.locator('.hx').count() === 1 && await ans.locator('b', { hasText: 'digits' }).count() === 1 && await ans.locator('.ic', { hasText: '--' }).count() === 1, 'and are drawn as the reader draws them');
  const rows = await ans.locator('.cl').evaluateAll(es => es.map(e => ({ t: e.textContent, gap: e.classList.contains('gap'), h: e.getBoundingClientRect().height, lh: parseFloat(getComputedStyle(e).lineHeight) })));
  ok(rows.every(r => r.t.trim()), 'no row is blank (' + rows.length + ' rows)');
  ok(rows[1] && rows[1].gap && rows.filter(r => r.gap).length === 3, 'a run of blank lines is one small gap (' + rows.map(r => r.gap ? 1 : 0).join('') + ')');
  const table = rows.find(r => r.t.includes('textkit/slug.py'));
  ok(table && Math.abs(table.h - table.lh) < 2, 'a table row is not wrapped');
  const fsz = await p.evaluate(() => parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--fs')) || 1);
  const size = await ans.evaluate(e => parseFloat(getComputedStyle(e).fontSize));
  ok(size >= 10.5 * fsz && size <= 11.5 * fsz, 'its text is about the size of the live peek (' + size + 'px)');
  // the switch in the corner covers no word of it
  ok(await card('n1').locator('.lens').isVisible(), 'its agent is still there, so live can be switched to');
  const covered = await p.evaluate(() => {
    const el = nodeEls.get('n1'), lb = el.lens.getBoundingClientRect(), boxes = [];
    const walk = document.createTreeWalker(el.ans.querySelector('.resin'), NodeFilter.SHOW_TEXT);
    for (let t; (t = walk.nextNode());) { const rg = document.createRange(); rg.selectNodeContents(t); boxes.push(...rg.getClientRects()); }
    return boxes.filter(r => r.width > 0 && r.left < lb.right && r.right > lb.left && r.top < lb.bottom && r.bottom > lb.top).length;
  });
  ok(covered === 0, 'the result / live switch covers none of its text (' + covered + ' boxes under it)');
  await p.screenshot({ path: W + '/result-report.png', clip: await card('n1').boundingBox() });

  // --- no report: an agent never told about `weft done` goes quiet, its result is read off the screen
  await p.evaluate(() => send('node_add', { node: { name: 'quiet', agent: 'claude', prompt: 'Look at the slug function and tell me what is wrong.', addenda: [], x: 900, y: 200 }, run: true }));
  ok(await waitFor(async () => (await rt('n2')).status === 'done', 60000), 'the quiet one is done');
  const r2 = await rt('n2');
  ok(r2.out_read === true && !!(r2.out || '').trim(), 'its result was read off its screen (' + (r2.out || '').slice(-60).replace(/\n/g, ' | ') + ')');
  ok(await card('n2').locator('.live').isVisible() && await card('n2').locator('.ans').isHidden(), 'and while its terminal is there, the card shows that');
  await card('n2').locator('.lt', { hasText: 'result' }).click();
  const tail = card('n2').locator('.ans');
  ok(await tail.isVisible() && (await tail.getAttribute('class')).includes('tail'), 'switched to the result, it is shown from its end');
  const ends = await p.evaluate(() => {
    const a = nodeEls.get('n2').ans, rows = a.querySelectorAll('.cl'), last = rows[rows.length - 1];
    return Math.abs(a.getBoundingClientRect().bottom - parseFloat(getComputedStyle(a).paddingBottom) - last.getBoundingClientRect().bottom);
  });
  ok(ends < 2, 'its last row sits at the bottom of the box, as in the peek (' + ends.toFixed(1) + 'px off)');
  // (the sim's terminal wrapped its answer: a row that starts 100 columns in keeps an indent the card has room for)
  const seen = await tail.evaluate(e => { const b = e.getBoundingClientRect(); return [...e.querySelectorAll('.cl')].filter(r => { const x = r.getBoundingClientRect(); return x.top >= b.top && x.bottom <= b.bottom && x.width > 0; }).map(r => r.textContent.trim()).join(' | '); });
  ok(/off-by-one/.test(seen) && /comparison\.$/.test(seen), 'its last rows are all in view (' + seen.slice(-90) + ')');
  await p.screenshot({ path: W + '/result-read.png', clip: await card('n2').boundingBox() });

  // --- without a terminal (an agent that rests: after a restart its screen is gone) there is nothing live to switch to
  await p.evaluate(() => { const r = S.rt.get('n2'); S.rt.set('n2', { ...r, alive: false, pv: [] }); renderNode('n2'); });
  ok(await card('n2').locator('.lens').isHidden() && await card('n2').locator('.ans').isVisible(), 'without a terminal the switch is gone and the result shows');
  ok(!(await card('n2').locator('.body').getAttribute('class')).includes('lensed'), 'and its first row has the whole width');
  // a link in a result opens itself, not the reader
  await p.evaluate(() => { const r = S.rt.get('n2'); S.rt.set('n2', { ...r, out: 'See https://example.com/pr/42 for it.', out_read: false }); renderNode('n2'); });
  const link = card('n2').locator('.ans a');
  ok(await link.count() === 1, 'a link in a result is a link');
  await p.evaluate(() => { window.open = () => null; });
  await link.evaluate(a => a.addEventListener('click', e => e.preventDefault()));
  await link.click(); await sleep(300);
  ok(await p.locator('#reader').isHidden(), 'and a click on it does not open the reader');
  await card('n2').locator('.ans').click({ position: { x: 20, y: 40 } }); await sleep(300);
  ok(await p.locator('#reader').isVisible(), 'a click elsewhere on the result still does');

  ok(errs.length === 0, 'no page errors' + (errs.length ? ': ' + errs.join(' | ') : ''));
  await b.close();
  console.log(fails ? fails + ' FAILED' : 'ALL PASSED');
  process.exit(fails ? 1 : 0);
})();
