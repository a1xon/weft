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
// an answer that asks three things at its end (after enough text to scroll)
const ASKS = [
  'I fixed the slug function: digits stay now.',
  '',
  ...Array.from({ length: 40 }, (_, i) => `- checked case ${i + 1}: "Title ${i + 1}" -> title-${i + 1}`),
  '',
  'Which way should the old slugs go?',
  '1. Keep them as they are',
  '2. Redirect them to the new ones',
  '',
  'Should I also update the README? Want me to commit this now, or wait until the CI is green?',
].join('\n');
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const ctx = await b.newContext({ viewport: { width: 1600, height: 900 } });
  const p = await ctx.newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message));
  const waitFor = async (fn, t = 30000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (await fn()) return true; await sleep(200); } return false; };
  const add = async node => p.evaluate(nd => new Promise(res => {
    const before = new Set(S.nodes.keys());
    send('node_add', { node: nd });
    const t = setInterval(() => { const id = [...S.nodes.keys()].find(k => !before.has(k)); if (id) { clearInterval(t); res(id); } }, 50);
  }), node);
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
  ok((await p.locator('#reader .qb').count()) === 0 && await p.locator('#reader .rq').isHidden(), 'an answer that asks nothing has no question marks');
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
  ok(!(await p.locator('#dock').getAttribute('class') || '').includes('over'), 'and the bar goes back under the canvas chrome');

  // --- a report that still asks you something is not done: the card asks you, under it and in the reader
  fs.writeFileSync(W + '/asks.txt', ASKS);
  const asker = name => add({ agent: 'claude', name, prompt: `Fix the slug of titles with digits. REPORT_FILE=${W}/asks.txt`, addenda: ['report-done'], x: 300, y: name === 'chat' ? 650 : 1000 });
  const chat = await asker('chat');
  await p.evaluate(id => send('arm', { id }), chat);
  const st = id => p.locator(`.nw[data-id="${id}"]`).getAttribute('data-st');
  ok(await waitFor(async () => (await st(chat)) === 'attention', 90000), 'its report asks three things: the card asks you instead of being done (' + await st(chat) + ')');
  const plea = p.locator(`.nw[data-id="${chat}"] .plea`);
  await p.evaluate(id => showPlea(id), chat); await sleep(600);
  ok(await waitFor(async () => (await plea.locator('.pqa').count()) === 3, 5000), 'under the card, each question on its own row');
  await p.screenshot({ path: W + '/reader-plea.png' });
  ok((await plea.locator('.pqa').nth(0).locator('.qc').allInnerTexts()).join('|') === '1. Keep them as they are|2. Redirect them to the new ones|you decide|skip|answer…',
    'with its quick answers: the options it listed, you decide, skip, answer (' + (await plea.locator('.pqa').nth(0).locator('.qc').allInnerTexts()).join('|') + ')');
  ok(await plea.locator('.pskip').isVisible() && await plea.locator('.fbtn', { hasText: 'read it all' }).isVisible(), 'it can be let go as it is, or read in full');
  await plea.locator('.pqa').nth(1).locator('.qc', { hasText: /^yes$/ }).click();
  ok((await plea.locator('textarea').inputValue()) === '> Should I also update the README?\nyes', 'a pick under the card writes the question and its answer into the box under it');
  await plea.locator('textarea').fill('');
  await plea.locator('.fbtn', { hasText: 'read it all' }).click();
  ok(await waitFor(async () => (await p.locator('#reader .qb').count()) === 2, 5000), 'read it all: the reader marks the two places that ask something');
  ok((await p.locator('#reader .cli.ok').innerText()).includes('checked case 40'), 'and shows the whole report');
  const onTop = await p.evaluate(() => { const r = omniTa.getBoundingClientRect(), e = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2); return !!(e && e.closest('#omniw')); });
  ok(onTop, 'the prompt bar is over the reader, not under it');
  ok((await p.locator('#omni .tgt').innerText()).includes('chat'), 'and talks to the card being read');
  ok((await p.locator('#reader .rq').innerText()) === '? 3 questions for you', 'the header counts the questions (' + await p.locator('#reader .rq').innerText() + ')');
  ok(await p.locator('#reader .rskip').isVisible(), 'and offers to skip them all');
  ok((await p.evaluate(() => omniTa.value)) === '', 'finding them types nothing into the bar');
  const chips = async i => (await p.locator('#reader .qb').nth(i).locator('.qc').allInnerTexts()).join('|');
  ok(await chips(0) === '1. Keep them as they are|2. Redirect them to the new ones|you decide|skip|answer…', 'the listed options are its quick answers (' + await chips(0) + ')');
  ok(await chips(1) === 'yes|no|you decide|skip|answer…|commit this now|wait until the CI is green|you decide|skip|answer…', 'yes / no, and the two things it weighs (' + await chips(1) + ')');
  ok((await p.locator('#reader .qb').nth(1).locator('.qa.row .qt').count()) === 2, 'a paragraph that asks two things has a row of answers for each');
  // the end of the text scrolls clear of the bar
  await p.locator('#reader .rbody').evaluate(e => { e.scrollTop = e.scrollHeight; });
  const lastB = await p.locator('#reader .cli.ok .qb').last().evaluate(e => e.getBoundingClientRect().bottom);
  const barT = await p.locator('#omniw').evaluate(e => e.getBoundingClientRect().top);
  ok(lastB < barT, `the last line can be read above the bar (${Math.round(lastB)} < ${Math.round(barT)})`);
  await p.screenshot({ path: W + '/reader-asks.png' });
  const qb = i => p.locator('#reader .qb').nth(i);

  await qb(0).locator('.qc', { hasText: /^2\. Redirect/ }).click();
  ok((await p.evaluate(() => omniTa.value)) === '> Which way should the old slugs go?\n2. Redirect them to the new ones', 'a pick quotes the question with its answer in the bar');
  ok(await p.evaluate(() => document.activeElement === omniTa), 'and the cursor is in the bar');
  await qb(1).locator('.qa.row').nth(0).locator('.qc', { hasText: /^yes$/ }).click();
  await qb(1).locator('.qa.row').nth(0).locator('.qc', { hasText: /^no$/ }).click();
  const v1 = await p.evaluate(() => omniTa.value);
  ok(v1.split('README').length === 2 && v1.includes('> Should I also update the README?\nno'), 'picking again changes that answer in place');
  ok((await p.locator('#reader .rq').innerText()) === '? 1 of 3 open', 'the header counts what is still open');
  ok((await qb(1).locator('.qc.on').allInnerTexts()).join() === 'no', 'the picked answer is lit');
  await qb(1).locator('.qa.row').nth(1).locator('.qc', { hasText: 'answer…' }).click();
  await p.keyboard.type('after the release');
  ok((await p.evaluate(() => omniTa.value)).endsWith('> Want me to commit this now, or wait until the CI is green?\nafter the release'), 'answer… lets you write your own, right under the question');
  ok((await p.locator('#reader .rq').innerText()).startsWith('✓ 3 answered'), 'all answered (' + await p.locator('#reader .rq').innerText() + ')');
  await p.keyboard.press('Escape'); await sleep(100);
  ok(await p.locator('#reader').isVisible() && await p.evaluate(() => document.activeElement.classList.contains('rbody')), 'Esc in the bar goes back to reading');
  ok((await p.evaluate(() => omniTa.value)).includes('after the release'), 'and keeps what was written');
  await p.keyboard.type('Thanks');
  const v2 = await p.evaluate(() => omniTa.value);
  ok(v2.endsWith('release\n\nThanks') && await p.evaluate(() => document.activeElement === omniTa), 'typing while reading writes into the bar, under the answers');
  await p.keyboard.press('Control+Enter');
  ok(await waitFor(async () => (await p.locator('#reader .rwork').count()) === 1, 15000), 'sent: while the agent works on the answers, the reader says this is its report from before');
  ok((await p.locator('#reader .qb').count()) === 0 && await p.locator('#reader .rq').isHidden(), 'and the questions are no longer open');
  const log = id => W + '/proj/.weft/run/received-' + id + '.log';
  ok(await waitFor(async () => fs.existsSync(log(chat)) && fs.readFileSync(log(chat), 'utf8').includes('> Which way should the old slugs go?\n2. Redirect them to the new ones'), 30000), 'the agent got the quoted questions with their answers');
  ok(fs.readFileSync(log(chat), 'utf8').includes('report again with a complete summary'), 'and is told its next report replaces the one it gave');
  ok(await waitFor(async () => (await st(chat)) === 'done', 90000), 'its next report asks nothing: the card is done');
  ok(await waitFor(async () => (await p.locator('#reader .cli.ok').count()) === 1 && (await p.locator('#reader .cli.ok').innerText()).includes('Nothing is left to decide'), 30000), 'and the open reader shows it');

  // the same report let go as it is: from the box under the card, or by skipping every question
  await p.locator('#reader .btn', { hasText: 'back to canvas' }).click();
  const two = await asker('two');
  await p.evaluate(id => send('arm', { id }), two);
  ok(await waitFor(async () => (await st(two)) === 'attention', 90000), 'a second card asks the same');
  await p.evaluate(id => showPlea(id), two); await sleep(600);
  await p.locator(`.nw[data-id="${two}"] .plea .pskip`).click();
  ok(await waitFor(async () => (await st(two)) === 'done', 8000), 'skip all: it is done');
  ok((await p.evaluate(async id => (await req('result', { id })).text, two)) === ASKS, 'and its report, as it stands, is its result');
  await p.evaluate(id => send('arm', { id }), two);
  ok(await waitFor(async () => (await st(two)) === 'attention', 90000), 'run again, it asks again');
  await p.evaluate(id => openReader(id), two);
  ok(await waitFor(async () => (await p.locator('#reader .qb').count()) === 2, 5000), 'read in full');
  for (const q of await p.locator('#reader .qc', { hasText: /^skip$/ }).all()) await q.click();
  ok((await p.locator('#reader .rq').innerText()).startsWith('✓ 3 answered'), 'every question skipped');
  const prompts = () => (fs.readFileSync(log(two), 'utf8').match(/=== prompt ===/g) || []).length;
  const before = prompts();
  await p.keyboard.press('Control+Enter');
  ok(await waitFor(async () => (await st(two)) === 'done', 8000), 'sending only skips lets the report go on as it is');
  await sleep(1500);
  ok(prompts() === before, 'and types nothing into the agent');
  await p.evaluate(id => openReader(id), chat);
  ok(await waitFor(async () => await p.locator('#reader').isVisible(), 3000), 'back to the first card');

  // the terminal still covers the bar; a new agent from the bar is shown on the canvas
  await p.locator('#reader .btn', { hasText: 'terminal' }).click();
  ok(await waitFor(async () => await p.locator('#focus').isVisible(), 3000), 'the terminal opens from the reader');
  const covered = await p.evaluate(() => { const r = omniTa.getBoundingClientRect(), e = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2); return !(e && e.closest('#omniw')); });
  ok(covered, 'and the bar is under it');
  await p.keyboard.press('Escape'); await sleep(200);
  if (await p.locator('#focus').isVisible()) { await p.evaluate(() => closeFocus()); await sleep(100); }
  ok(await p.locator('#reader').isVisible(), 'Esc in the terminal comes back to the reader');
  await p.locator('#omni .tgx').click();
  await p.evaluate(() => { omniTa.value = 'a new agent'; omniTa.focus(); });
  await p.keyboard.press('Control+Enter');
  ok(await waitFor(async () => await p.locator('#reader').isHidden(), 3000), 'a prompt for a new agent closes the reader');

  ok(errs.length === 0, 'no page errors' + (errs.length ? ': ' + errs.join(' | ') : ''));
  await b.close();
  console.log(fails ? `${fails} FAILED` : 'all passed');
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
