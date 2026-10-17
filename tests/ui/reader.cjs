// A result is read full size, laid out the way the agent's terminal showed it: tables whole, indents kept.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs');
const W = process.env.SANDBOX;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
const MARKDOWN = [
  '## What changed',
  'The slug function now keeps **digits** and drops `--` runs. See https://example.com/pr/42 for the review.',
  '',
  '| file | lines | why |',
  '|------|------:|-----|',
  '| textkit/slug.py | 12 | the fix, with a note on why digits must stay because the old tests expected them to vanish |',
  '| tests/test_slug.py | 30 | new cases |',
  '',
  '- Left open',
  '  - a long nested item that goes on and on so that it has to wrap in a narrow window, and when it does the next line starts under the words, not under the dash',
  '',
  '```',
  'def slug(s):',
  '    return "-".join(w for w in s.split())',
  '```',
].join('\n');
const SCREEN = [
  '┌─────────┬───────┐',
  '│ Planet  │ Moons │',
  '├─────────┼───────┤',
  '│ Earth   │ 1     │',
  '└─────────┴───────┘',
  '',
  '- Planets',
  '  - Inner, rocky',
].join('\n');
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const ctx = await b.newContext({ viewport: { width: 1600, height: 900 } });
  const p = await ctx.newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message));
  const waitFor = async (fn, t = 30000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (await fn()) return true; await sleep(200); } return false; };
  await p.goto(fs.readFileSync(W + '/url.txt', 'utf8').trim()); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400);
  await p.keyboard.press('Escape'); await sleep(200);

  await p.evaluate(() => send('node_add', { node: { name: 'writer', agent: 'sh', cmd: 'true', x: 300, y: 200 } }));
  await p.evaluate(() => send('node_add', { node: { name: 'screen', agent: 'sh', cmd: 'true', x: 900, y: 200 } }));
  ok(await waitFor(async () => (await p.locator('.nw').count()) === 2, 8000), 'two cards are on the canvas');
  await p.evaluate(t => send('done', { id: 'n1', text: t }), MARKDOWN);
  await p.evaluate(t => send('done', { id: 'n2', text: t }), SCREEN);
  ok(await waitFor(async () => (await p.locator('.nw[data-id="n1"] .ans.ok').count()) === 1, 8000), 'the first card shows its result');

  // a click on the answer opens it full size
  await p.locator('.nw[data-id="n1"] .ans').click();
  ok(await waitFor(async () => await p.locator('#reader').isVisible(), 3000), 'a click on the answer opens the reader');
  ok(await p.locator('#fly').isHidden(), 'not the small flyout');
  await waitFor(async () => (await p.locator('#reader .cli.ok').count()) === 1, 3000);
  const body = await p.locator('#reader .rbody').boundingBox();
  ok(body.width > 1300 && body.height > 700, 'it fills the window (' + Math.round(body.width) + 'x' + Math.round(body.height) + ')');
  const size = await p.locator('#reader .cli.ok').evaluate(e => parseFloat(getComputedStyle(e).fontSize));
  ok(size >= 16, 'at a size made to be read (' + size.toFixed(1) + 'px)');
  ok((await p.locator('#reader .hx').innerText()) === 'What changed', 'a heading reads as a heading, without its #');
  ok((await p.locator('#reader .cli.ok b').innerText()) === 'digits' && (await p.locator('#reader .cli.ok .ic').innerText()) === '--', 'bold and code are shown as such');
  ok((await p.locator('#reader a[href="https://example.com/pr/42"]').count()) === 1, 'a link can be followed');
  ok((await p.locator('#reader .mt tr').count()) === 3 && (await p.locator('#reader .mt th').count()) === 3, 'a markdown table is a table: 3 columns, a head and 2 rows');
  ok((await p.locator('#reader .mt td').nth(1).evaluate(e => getComputedStyle(e).textAlign)) === 'right', 'aligned the way the table says');
  ok((await p.locator('#reader .cb').innerText()).includes('    return'), 'code keeps its indent');
  const item = p.locator('#reader .cl', { hasText: 'a long nested item' });
  const ind = await item.evaluate(e => [getComputedStyle(e).paddingLeft, getComputedStyle(e).textIndent].map(parseFloat));
  const ch = await item.evaluate(e => { const s = document.createElement('span'); s.textContent = '0'.repeat(10); e.append(s); const w = s.getBoundingClientRect().width / 10; s.remove(); return w; });
  ok(Math.abs(ind[0] - 4 * ch) < 1 && Math.abs(ind[1] + 2 * ch) < 1, 'a nested list item keeps its indent and wraps under its words (' + ind.map(Math.round) + ')');
  await p.screenshot({ path: W + '/reader-markdown.png' });

  // keys: bigger text, nothing on the canvas behind it, Esc closes
  await p.keyboard.press('+'); await sleep(100);
  const bigger = await p.locator('#reader .cli.ok').evaluate(e => parseFloat(getComputedStyle(e).fontSize));
  ok(bigger > size, '+ makes the text larger (' + bigger.toFixed(1) + 'px)');
  await p.keyboard.press('0');
  await p.keyboard.press('Delete'); await sleep(300);
  ok((await p.locator('.nw[data-id="n1"]').count()) === 1, 'Delete in the reader does not delete the card behind it');
  await p.keyboard.press('Escape');
  ok(await p.locator('#reader').isHidden(), 'Esc closes it');

  // what the terminal drew stays as drawn: every row of a box table, one terminal column per character
  await p.evaluate(() => select(['n2'])); await p.keyboard.press('r');
  ok(await waitFor(async () => (await p.locator('#reader .bx').count()) === 1, 3000), 'r opens the selected card\'s result');
  const bx = await p.locator('#reader .bx').evaluate(e => ({ text: e.innerText, ws: getComputedStyle(e).whiteSpace, h: e.getBoundingClientRect().height, lh: parseFloat(getComputedStyle(e).lineHeight) }));
  ok(bx.text.split('\n').length === 5 && bx.text.includes('├─────────┼───────┤') && bx.text.includes('└─────────┴───────┘'), 'the box table keeps all its rules');
  ok(bx.ws === 'pre' && Math.abs(bx.h - 5 * bx.lh) < 2, 'and is never wrapped (' + Math.round(bx.h) + 'px for 5 rows)');
  const sub = await p.locator('#reader .cl', { hasText: 'Inner, rocky' }).evaluate(e => parseFloat(getComputedStyle(e).paddingLeft));
  const top = await p.locator('#reader .cl', { hasText: 'Planets' }).evaluate(e => parseFloat(getComputedStyle(e).paddingLeft));
  ok(sub > top, 'a nested list stays nested');
  await p.screenshot({ path: W + '/reader-screen.png' });
  await p.locator('#reader .btn', { hasText: 'back to canvas' }).click();
  ok(await p.locator('#reader').isHidden(), 'back to canvas closes it');

  ok(errs.length === 0, 'no page errors' + (errs.length ? ': ' + errs.join(' | ') : ''));
  await b.close();
  console.log(fails ? `${fails} FAILED` : 'all passed');
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
