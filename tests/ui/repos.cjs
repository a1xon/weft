// Several repositories on one board: the git tree shows the one where you are, and each other
// repository the agents work in is a folded line on top of it (branch, remote, what happened last).
// A commit there unfolds its line for a moment; a click on the line shows that repository in the
// tree. Folders of one repository are one line; where you are is no repository, only lines show.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs'); const cp = require('child_process');
const W = process.env.SANDBOX, PJ = W + '/proj', CR = W + '/crawler';
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
const until = async (f, ms = 15000) => { const t = Date.now(); while (Date.now() - t < ms) { try { if (await f()) return true; } catch {} await sleep(250); } return false; };
const sh = (cmd, cwd) => cp.execSync(cmd, { cwd, encoding: 'utf8' }).trim();
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const ctx = await b.newContext({ viewport: { width: 1500, height: 900 } });
  const p = await ctx.newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type() === 'error' && errs.push(m.text()));
  const url = fs.readFileSync(W + '/url.txt', 'utf8').trim();
  const lines = () => p.locator('#gtree .gothers .grepo').allInnerTexts();
  const treeDir = () => p.locator('#gtree .gh .gdir').innerText();
  const add = node => p.evaluate(n => req('node_add', { node: n }).then(r => r.id), node);
  await p.goto(url); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(450);
  await p.keyboard.press('Escape');

  // --- one repository: the tree alone
  ok(await until(async () => (await p.locator('#gtree .gr').count()) >= 1), 'the tree shows the project');
  await add({ agent: 'claude', name: 'here', prompt: '', x: 100, y: 100 });
  await add({ agent: 'claude', name: 'deeper', prompt: '', cwd: PJ + '/textkit', x: 100, y: 500 });
  await sleep(6000);
  ok((await lines()).length === 0, 'agents in one repository (one of them in a folder inside it): no other line');

  // --- an agent in a second repository: a line for it on top of the tree
  sh(`mkdir -p ${CR} && cd ${CR} && git init -q -b main && git -c user.name=w -c user.email=w@w commit -q --allow-empty -m "crawler starts"`, W);
  await add({ agent: 'claude', name: 'crawl', prompt: '', cwd: CR, x: 800, y: 100 });
  ok(await until(async () => (await lines()).length === 1), 'an agent in another repository: a line for it: ' + JSON.stringify(await lines()));
  const l1 = (await lines())[0];
  ok(/crawler/.test(l1) && /main/.test(l1) && /crawler starts/.test(l1), 'naming the repository, its branch and what happened there last: ' + l1.replace(/\s+/g, ' '));
  ok(/proj/.test(await treeDir()), 'the tree still shows the project');
  const lb = await p.locator('#gtree .gothers').boundingBox(), pb = await p.locator('#gtree .gpanel').boundingBox();
  ok(lb.y + lb.height <= pb.y + 1, 'the line sits on top of the tree');
  await p.screenshot({ path: W + '/repos-1-line.png' });

  // --- a commit there: its line unfolds for a moment with it
  sh(`git -c user.name=w -c user.email=w@w commit -q --allow-empty -m "Retry on 429"`, CR);
  ok(await until(async () => (await p.locator('#gtree .gother .gpr.fresh').count()) === 1), 'a commit in it unfolds its line');
  ok(/Retry on 429/.test(await p.locator('#gtree .gother .gpr.fresh').innerText()), 'with the new commit lit');
  ok(/committed/.test((await lines())[0]), 'and the line says what happened: ' + (await lines())[0].replace(/\s+/g, ' '));
  await p.screenshot({ path: W + '/repos-2-peek.png' });
  ok(await until(async () => (await p.locator('#gtree .gother').count()) === 0, 12000), 'and folds again');

  // --- a click on the line: that repository in the tree, the project becomes a line
  await p.locator('#gtree .gothers .grepo').click();
  ok(await until(async () => /crawler/.test(await treeDir())), 'a click shows that repository in the tree: ' + await treeDir());
  ok(await until(async () => (await p.locator('#gtree .gr .gs').allInnerTexts()).includes('Retry on 429')), 'with its commits');
  ok(await until(async () => (await lines()).length === 1 && /proj/.test((await lines())[0])), 'and the project is the line now: ' + JSON.stringify(await lines()));
  await p.screenshot({ path: W + '/repos-3-switched.png' });

  // --- where new agents start is no repository: no tree, both repositories as lines
  await p.evaluate(w => setNewDir(w), W);
  ok(await until(async () => (await lines()).length === 2), 'where you are is no repository: both repositories are lines: ' + JSON.stringify(await lines()));
  ok(await p.locator('#gtree .gpanel').isHidden(), 'and no tree');
  await p.screenshot({ path: W + '/repos-4-bare.png' });
  await p.evaluate(d => setNewDir(d), PJ);
  ok(await until(async () => (await lines()).length === 1 && /crawler/.test((await lines())[0]) && /proj/.test(await treeDir())), 'back in the project: its tree, the crawler a line');

  // --- the last agent there gone: the line goes
  const crawl = await p.evaluate(() => [...S.nodes.values()].find(n => n.name === 'crawl').id);
  await p.evaluate(id => send('node_del', { id }), crawl);
  ok(await until(async () => (await lines()).length === 0), 'no agent left in the other repository: no line');

  ok(errs.length === 0, 'no page errors' + (errs.length ? ': ' + errs.join(' | ') : ''));
  await b.close();
  console.log(fails ? `${fails} FAILED` : 'all passed');
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
