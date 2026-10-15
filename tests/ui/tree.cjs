// The git tree: bottom right, beside the bar, the commits of the branch where you are. Live: a
// commit made outside weft shows up by itself; merges draw a second lane; another branch is picked
// from its name without checking anything out; a click on a commit unfolds its files.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs'); const cp = require('child_process');
const W = process.env.SANDBOX, PJ = W + '/proj';
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
const until = async (f, ms = 12000) => { const t = Date.now(); while (Date.now() - t < ms) { try { if (await f()) return true; } catch {} await sleep(250); } return false; };
const git = args => cp.execSync('git ' + args, { cwd: PJ, encoding: 'utf8' }).trim();
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const ctx = await b.newContext({ viewport: { width: 1500, height: 900 } });
  const p = await ctx.newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type() === 'error' && errs.push(m.text()));
  const url = fs.readFileSync(W + '/url.txt', 'utf8').trim();
  const subjects = () => p.locator('#gtree .gr:not(.work) .gs').allInnerTexts();
  const head = () => p.locator('#gtree .gh').innerText();
  await p.goto(url); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(450);

  // --- under the launch pad it is out of the way; on the canvas it is in the corner
  ok(await p.locator('#gtree').evaluate(e => e.hidden || getComputedStyle(e).visibility === 'hidden'), 'the launch pad hides it');
  await p.keyboard.press('Escape');
  ok(await until(async () => (await subjects()).length === 1), 'the canvas shows the tree of what is checked out');
  ok((await subjects())[0] === 'textkit with two bugs', 'with its commit');
  const h1 = await head();
  ok(/main/.test(h1) && /pushed/.test(h1) && /proj/.test(h1), 'the head names the branch, that it is pushed and the folder: ' + h1.replace(/\s+/g, ' '));
  const box = await p.locator('#gtree').boundingBox(), bar = await p.locator('#omniw').boundingBox();
  ok(box.x >= bar.x + bar.width && box.x + box.width <= 1500 && box.y + box.height <= 900 - 10, `bottom right, beside the bar (${Math.round(box.x)}..${Math.round(box.x + box.width)} vs bar ..${Math.round(bar.x + bar.width)})`);
  ok(await p.locator('#gtree .rf.cur').innerText() === 'main⇅', 'the checked-out branch wears a badge that says origin is there too');
  ok(await p.locator('#devtag').count() === 0, 'no "live ui" badge in the corner');

  // --- live: a commit made outside weft lands by itself, lit up, and the head counts it
  fs.writeFileSync(PJ + '/NOTES.md', 'one\n');
  ok(await until(async () => (await p.locator('#gtree .gr.work .gs').innerText()) === '1 uncommitted'), 'a changed file shows as uncommitted on top');
  git('add NOTES.md'); git('commit -qm "write notes"');
  ok(await until(async () => (await subjects())[0] === 'write notes'), 'a new commit shows up without a click');
  ok(await p.locator('#gtree .gr.work').count() === 0, 'and the uncommitted line is gone');
  ok(/↑1/.test(await head()), 'the head says one commit is not pushed');

  // --- a branch merged back: two lanes, a merge dot
  git('switch -q -c side'); fs.writeFileSync(PJ + '/SIDE.md', 'a\nb\nc\n'); git('add SIDE.md'); git('commit -qm "work on the side"');
  git('switch -q main'); git('commit -q --allow-empty -m "meanwhile on main"'); git('merge -q --no-ff -m "merge side" side');
  ok(await until(async () => (await subjects())[0] === 'merge side'), 'the merge arrives');
  const subs = await subjects();
  ok(subs.length === 5 && subs[0] === 'merge side' && subs.slice(3).join('|') === 'write notes|textkit with two bugs'
    && subs.includes('meanwhile on main') && subs.includes('work on the side'), 'children before their parents: ' + subs.join(' | '));
  const lanes = await p.locator('#gtree svg.glanes').evaluate(s => ({ colours: new Set([...s.querySelectorAll('path')].map(e => e.getAttribute('class'))).size,
    merge: s.querySelectorAll('circle.m').length, headRing: s.querySelectorAll('circle.hd').length, dots: s.querySelectorAll('circle:not(.hd)').length }));
  ok(lanes.colours >= 2 && lanes.merge === 1 && lanes.headRing === 1 && lanes.dots === 5, 'two lanes, one merge, a ring on what is checked out: ' + JSON.stringify(lanes));
  const xs = await p.locator('#gtree svg.glanes circle:not(.hd)').evaluateAll(cs => cs.map(c => +c.getAttribute('cx')));
  const side = subs.indexOf('work on the side');
  ok(xs.every((x, i) => i === side ? x > xs[0] : x === xs[0]), 'main reads straight down, the side commit in a lane of its own: ' + xs.join(','));
  await p.screenshot({ path: W + '/tree-1-merge.png' });

  // --- a click on a commit unfolds what it changed
  await p.locator('#gtree .gr', { hasText: 'work on the side' }).locator('.gline').click();
  ok(await until(async () => /SIDE\.md/.test(await p.locator('#gtree .gdet').innerText())), 'a click on a commit shows its files');
  const det = await p.locator('#gtree .gdet').innerText();
  ok(/\+3/.test(det) && !/null|undefined/.test(det), 'with what was added: ' + det.replace(/\s+/g, ' '));
  const unfolded = await p.locator('#gtree .gr.open').evaluate(e => e.offsetHeight);
  const lineEnd = await p.locator('#gtree svg.glanes').evaluate(s => Math.max(...[...s.querySelectorAll('path')].map(e => e.getBBox().y + e.getBBox().height)));
  const listH = await p.locator('#gtree .gin').evaluate(e => e.offsetHeight);
  ok(unfolded > 60 && lineEnd > listH - 30, `the lanes run on past the unfolded commit (row ${unfolded}px, lines to ${Math.round(lineEnd)} of ${listH})`);
  await p.screenshot({ path: W + '/tree-2-commit.png' });
  await p.locator('#gtree .gr.open .gline').click();
  ok(await p.locator('#gtree .gdet').count() === 0, 'a second click folds it');

  // --- another branch, picked from the name: nothing is checked out
  await p.locator('#gtree .gbr').click();
  const items = await p.locator('.menu .mi').allInnerTexts();
  ok(items.some(t => t.startsWith('side')) && items.some(t => t.startsWith('every branch')) && items.some(t => t.startsWith('origin/main')), 'the menu offers the branches: ' + items.map(t => t.replace(/\s+/g, ' ')).join(' | '));
  await p.locator('.menu .mi', { hasText: /^side/ }).click();
  ok(await until(async () => (await subjects()).join('|') === 'work on the side|write notes|textkit with two bugs'), 'the tree of side');
  ok(/side/.test(await head()) && await p.locator('#gtree .gbr.pinned').count() === 1, 'the head says which, marked as not checked out');
  ok(git('branch --show-current') === 'main', 'and main is still checked out');
  await p.reload(); await p.waitForSelector('#pad:not([hidden]) textarea'); await p.keyboard.press('Escape');
  ok(await until(async () => (await subjects())[0] === 'work on the side'), 'the choice is kept for this folder');
  await p.locator('#gtree .gbr').click();
  await p.locator('.menu .mi', { hasText: /^main/ }).click();
  ok(await until(async () => (await subjects())[0] === 'merge side'), 'picking the checked-out branch follows it again');

  // --- every branch at once: one cut from an older commit and not merged stands beside main
  git('switch -q -c next side'); git('commit -q --allow-empty -m "an idea not merged yet"'); git('switch -q main');
  await p.locator('#gtree .gbr').click();
  await p.locator('.menu .mi', { hasText: /^every branch/ }).click();
  ok(await until(async () => (await subjects()).includes('an idea not merged yet') && (await subjects()).includes('merge side')), 'every branch: ' + (await subjects()).join(' | '));
  const tips = await p.locator('#gtree svg.glanes circle:not(.hd)').evaluateAll(cs => cs.slice(0, 2).map(c => +c.getAttribute('cx')));
  ok(tips[0] !== tips[1], 'the two tips stand side by side: ' + tips.join(','));
  ok((await p.locator('#gtree .rf').allInnerTexts()).includes('next'), 'with the name of the branch at its tip');
  await p.screenshot({ path: W + '/tree-4-every.png' });
  await p.locator('#gtree .gbr').click();
  await p.locator('.menu .mi', { hasText: /^main/ }).click();
  ok(await until(async () => !(await subjects()).includes('an idea not merged yet')), 'and back to what is checked out');

  // --- folded away: a box with the branch and its last commit, kept across a reload
  await p.locator('#gtree .gh .ib').click();
  ok(await p.locator('#gtree .gpanel').isHidden() && /main/.test(await p.locator('#gtree .gbox').innerText()) && /merge side/.test(await p.locator('#gtree .gbox').innerText()), 'folded: a box says the branch and its last commit');
  await p.reload(); await p.waitForSelector('#pad:not([hidden]) textarea'); await p.keyboard.press('Escape');
  ok(await until(() => p.locator('#gtree .gbox').isVisible()) && await p.locator('#gtree .gpanel').isHidden(), 'and stays folded after a reload');
  await p.locator('#gtree .gbox').click();
  ok(await p.locator('#gtree .gpanel').isVisible(), 'a click unfolds it');

  // --- narrow: no room beside the bar, so it sits above it
  await p.setViewportSize({ width: 1000, height: 800 });
  await sleep(300);
  const nb = await p.locator('#gtree').boundingBox(), dock = await p.locator('#dock').boundingBox();
  const where = await p.locator('#where').boundingBox();
  ok(nb.y + nb.height <= Math.min(dock.y, where.y) + 2 && nb.x + nb.width <= 1000, `a narrow window: above the bar (${Math.round(nb.y + nb.height)} <= ${Math.round(dock.y)})`);
  await p.screenshot({ path: W + '/tree-3-narrow.png' });

  // --- fit keeps the cards out from under it
  await p.setViewportSize({ width: 1500, height: 900 }); await sleep(300);
  await p.evaluate(() => { [[0, 0], [1500, 0], [0, 1000], [1500, 1000]].forEach(([x, y], i) => send('node_add', { node: { name: 'c' + i, agent: 'claude', prompt: 'x', x, y } })); });
  ok(await until(async () => (await p.locator('.nw').count()) === 4), '(four cards in the corners of the canvas)');
  await p.evaluate(() => fit()); await sleep(300);
  const tb = await p.locator('#gtree').boundingBox();
  const under = await p.locator('.nw').evaluateAll((ns, t) => ns.filter(n => { const r = n.getBoundingClientRect(); return r.right > t.x && r.bottom > t.y; }).length, tb);
  ok(under === 0, 'fit keeps every card out from under the tree');
  await p.screenshot({ path: W + '/tree-5-fit.png' });

  ok(errs.length === 0, 'no page errors: ' + errs.join(' | ').slice(0, 300));
  await b.close();
  console.log(fails ? `${fails} FAILED` : 'all passed');
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
