// Folders: agents can work in another folder than the project's (a sibling repository), picked in a
// click or two; a commit after such an agent lands in its repository, not the project's. Cards say
// where they work only while more than one folder is in use. Work in two repositories is pushed to
// each, with a pull request in each.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs'); const cp = require('child_process');
const W = process.env.SANDBOX, BIN = process.env.WEFT_UI_BIN;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
const until = async (f, ms = 9000) => { const t = Date.now(); while (Date.now() - t < ms) { if (await f()) return true; await sleep(250); } return false; };
const git = (dir, args) => cp.execSync('git ' + args, { cwd: dir, encoding: 'utf8' }).trim();
const CR = W + '/proj-crawler', PJ = W + '/proj';
(async () => {
  // a sibling repository next to the project (the same small library, so the agent has work there)
  fs.rmSync(CR, { recursive: true, force: true });
  cp.execSync(`cp -r ${PJ} ${CR} && rm -rf ${CR}/.git ${CR}/.weft && git init -q -b main ${CR} && git -C ${CR} add -A && git -C ${CR} commit -qm crawler`);
  const projHead = git(PJ, 'rev-parse HEAD'), crHead = git(CR, 'rev-parse HEAD');

  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const p = await (await b.newContext({ viewport: { width: 1500, height: 900 } })).newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type() === 'error' && errs.push(m.text()));
  const url = fs.readFileSync(W + '/url.txt', 'utf8').trim();
  const st = id => p.locator('.nw[data-id="' + id + '"]').getAttribute('data-st');
  const flow = () => JSON.parse(fs.readFileSync(PJ + '/.weft/flow.json', 'utf8'));
  const cwdOf = id => (flow().nodes.find(n => n.id === id) || {}).cwd;
  const where = () => p.locator('#where').evaluate(e => e.textContent);
  const rows = () => p.locator('.fpick .mi').allInnerTexts();
  const read = id => { try { return cp.execFileSync(BIN, ['read', id, '-n', '60'], { cwd: PJ, encoding: 'utf8' }); } catch { return ''; } };
  const tag = id => p.locator(`.nw[data-id="${id}"] .fdtag`);
  const tags = () => p.locator('.fdtag:visible').count();
  const st2 = async (id, want, ms = 30000) => until(async () => want.includes(await st(id)), ms);
  await p.goto(url); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(450);

  // --- the pad's headline picks the folder new agents start in
  await p.locator('#pad .pname').click();
  await p.waitForSelector('.fpick input');
  ok(await p.locator('.fpick input').evaluate(e => document.activeElement === e), 'a click on the folder opens the picker, ready to type');
  ok(await until(async () => (await rows()).some(t => t.includes('proj-crawler'))), 'the sibling repository is offered without typing');
  const r1 = await rows();
  ok(r1[0].includes('proj') && r1[0].includes('project') && r1[0].includes('●') && r1[1].includes('proj-crawler') && r1[1].includes('git'),
    'first the project (chosen), then the sibling that is named like it: ' + r1.slice(0, 3).map(t => t.replace(/\s+/g, ' ')).join(' | '));
  await p.screenshot({ path: W + '/folders-1-picker.png' });
  await p.keyboard.type('crawl');
  ok(await until(async () => (await p.locator('.fpick .mi.on').innerText().catch(() => '')).includes('proj-crawler')), 'a few letters find it');
  await p.keyboard.press('Enter');
  await sleep(300);
  ok((await p.locator('.fpick').count()) === 0, 'enter takes it');
  ok((await p.locator('#pad .pname').innerText()) === 'proj-crawler', 'the headline is that folder now');
  ok(await p.locator('#pad textarea').evaluate(e => document.activeElement === e), 'the keyboard is back in the prompt');
  ok((await p.locator('#pad textarea').getAttribute('placeholder')).includes('proj-crawler'), 'the placeholder names it');
  ok((await p.locator('#pad .pother').innerText()).includes('proj'), 'and the pad says it is not the canvas folder');
  fs.writeFileSync(CR + '/scratch.txt', 'x\n');
  ok(await until(async () => (await p.locator('#pad .pgit').innerText()).includes('1 uncommitted')), 'its own branch and uncommitted files: ' + (await p.locator('#pad .pgit').innerText()).replace(/\s+/g, ' '));
  fs.rmSync(CR + '/scratch.txt');

  // --- the agent starts there and its card says so
  await p.locator('#pad textarea').fill('Fix the slug function.');
  await p.keyboard.press('Control+Enter');
  await p.waitForSelector('.nw[data-id="n1"]');
  await sleep(600);
  ok(cwdOf('n1') === CR, 'the agent is made for that folder: ' + cwdOf('n1'));
  ok(await until(() => read('n1').includes(CR)), 'and runs in it (its banner shows the folder)');
  ok((await tags()) === 0, 'one folder in use (even another than the project\'s): no card needs to say so');
  ok((await where()).includes('proj-crawler'), 'the folder line above the bar shows where new agents start: ' + await where());
  ok(await until(async () => (await st('n1')) === 'done', 20000), 'the agent finishes: ' + await st('n1'));
  ok(git(CR, 'status --porcelain').includes('slug.py') && git(PJ, 'status --porcelain -- textkit') === '', 'it changed the sibling, not the project');

  // --- a commit after it lands in the agent's repository
  const gid = await p.evaluate(() => spawnAt('git:commit', 1100, 120, { from: 'n1', edit: false }));
  ok(await until(() => git(CR, 'rev-parse HEAD') !== crHead, 25000), 'the commit lands in the sibling repository: ' + git(CR, 'log -1 --format=%s'));
  ok(git(PJ, 'rev-parse HEAD') === projHead, 'the project repository is untouched');
  ok(await until(async () => (await st(gid)) === 'done', 15000), 'the commit step is done: ' + await st(gid));
  // a second folder in use: now every card says where it works, the project folder too
  const other = await p.evaluate(() => spawnAt('claude', 200, 520, { edit: false, extra: { cwd: '' } }));
  ok(await until(async () => (await tags()) >= 3), 'a card in a second folder: every card wears its folder (' + await tags() + ')');
  ok((await tag('n1').innerText()) === 'proj-crawler' && (await tag(other).innerText()) === 'proj', 'the agents say proj-crawler and proj');
  ok((await tag(gid).innerText()) === 'proj-crawler', 'and the commit says where it committed');
  await p.screenshot({ path: W + '/folders-2-cards.png' });
  await p.evaluate(id => send('node_del', { id }), other);
  ok(await until(async () => (await tags()) === 0), 'one folder again: the tags go');

  // --- remembered for this project
  await p.reload(); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(500);
  ok((await p.locator('#pad .pname').innerText()) === 'proj-crawler', 'the choice survives a reload');
  await p.locator('#pad .pother button').click();
  ok((await p.locator('#pad .pname').innerText()) === 'proj' && (await p.locator('#pad .pother').count()) === 0, '"back to it" returns to the project folder');
  await p.keyboard.press('Escape');
  await sleep(200);

  // --- with an agent selected, ctrl+o moves it; a typed path is completed from the disk
  await p.evaluate(() => select(['n1']));
  await sleep(150);
  ok((await where()).includes('proj-crawler'), 'with the agent selected the folder line shows its folder: ' + await where());
  await p.locator('#omni textarea').focus();
  await p.keyboard.press('Control+o');
  await p.waitForSelector('.fpick input');
  ok((await p.locator('.fpick .cap').first().innerText()).startsWith('move'), 'ctrl+o in the bar moves the selected agent: ' + await p.locator('.fpick .cap').first().innerText());
  await p.keyboard.type('../pr');
  ok(await until(async () => (await rows()).length >= 2), 'a typed path lists what completes it');
  const r2 = await rows();
  ok(r2[0].includes('proj') && !r2[0].includes('proj-crawler') && r2.some(t => t.includes('proj-crawler')), 'both folders that begin so, in order: ' + r2.map(t => t.replace(/\s+/g, ' ')).join(' | '));
  await p.keyboard.press('Enter');   // the first: the project itself
  await sleep(500);
  ok(cwdOf('n1') === '', 'the agent works in the project folder again (it stores nothing for it): "' + cwdOf('n1') + '"');
  ok((await p.evaluate(gid => workDir(S.nodes.get(gid)), gid)) === PJ, 'and the commit after it follows it');
  ok((await p.locator('#toasts').innerText()).includes('restart there'), 'a running agent is told it moves at its next start, with a restart at hand');

  // --- the folder line moves it back with a click; an agent linked after it works there too
  await p.locator('#where .wname').click();
  await p.waitForSelector('.fpick input');
  await p.keyboard.type('crawler');
  ok(await until(async () => (await p.locator('.fpick .mi.on').innerText().catch(() => '')).includes('proj-crawler')), 'a word in the middle of the name finds it too');
  await p.keyboard.press('Enter');
  await sleep(400);
  ok(cwdOf('n1') === CR, 'moved back: ' + cwdOf('n1'));
  const nid = await p.evaluate(() => spawnLinked('n1', 'next'));
  await sleep(300);
  ok(cwdOf(nid) === CR, 'a card linked after it works where the work is: ' + cwdOf(nid));

  // --- "/folder" in the bar; nothing selected, it is the folder new agents start in
  await p.evaluate(() => select([]));
  await p.locator('#omni textarea').fill('');
  await p.locator('#omni textarea').focus();
  await p.keyboard.type('/folder proj-cr');
  await p.waitForSelector('.menu[data-pal] .mi');
  await p.keyboard.press('Enter');
  await sleep(300);
  ok((await where()).includes('proj-crawler'), 'the palette picks the folder for new agents: ' + await where());
  const lone = await p.evaluate(() => spawnAt('claude', 200, 700, { edit: false }));
  await sleep(300);
  ok(cwdOf(lone) === CR, 'and the next new agent is made for it');
  await p.evaluate(() => setNewDir(projDir()));

  // --- work in two repositories: one pull request step pushes each and opens a pull request in each
  cp.execSync(`git init -q --bare ${W}/crawler-remote.git && git -C ${CR} remote add origin ${W}/crawler-remote.git && git -C ${CR} push -q -u origin main`);
  const before = { pj: git(W + '/remote.git', 'rev-parse main'), cr: git(W + '/crawler-remote.git', 'rev-parse main') };
  const wa = await p.evaluate(() => spawnAt('claude', 200, 1100, { text: 'Fix the wrap function.', draft: false, run: true, edit: false, extra: { cwd: '' } }));
  const wb = await p.evaluate(d => spawnAt('claude', 200, 1450, { text: 'Fix the wrap function.', draft: false, run: true, edit: false, extra: { cwd: d } }), CR);
  ok(await st2(wa, ['done']) && await st2(wb, ['done']), 'an agent in each repository fixed it');
  const ca = await p.evaluate(id => spawnAt('git:commit', 900, 1100, { from: id, edit: false }), wa);
  const cb = await p.evaluate(id => spawnAt('git:commit', 900, 1450, { from: id, edit: false }), wb);
  const pr = await p.evaluate(id => spawnAt('git:pr', 1500, 1250, { from: id, edit: false }), ca);
  await p.evaluate(([c, r]) => send('edge_add', { from: c, to: r }), [cb, pr]);
  ok(await st2(ca, ['done']) && await st2(cb, ['done']), 'each commit is done in its own repository');
  await p.evaluate(id => send('arm', { id }), pr);
  ok(await st2(pr, ['done', 'failed'], 60000) && (await st(pr)) === 'done', 'the pull request step is done: ' + await st(pr));
  const plog = await p.evaluate(id => (S.rt.get(id).git || {}).log || [], pr);
  ok(git(W + '/remote.git', 'rev-parse main') !== before.pj && git(W + '/remote.git', 'log -1 --format=%s main').includes('wrap'), 'the project\'s remote got its commit');
  ok(git(W + '/crawler-remote.git', 'rev-parse main') !== before.cr && git(W + '/crawler-remote.git', 'log -1 --format=%s main').includes('wrap'), 'the sibling\'s remote got its own');
  ok(plog.filter(l => l.startsWith('pushed ')).length === 2 && plog.some(l => l.startsWith('pushed ') && l.endsWith('(proj-crawler)')), 'one push each: ' + plog.filter(l => l.startsWith('pushed ')).join(' | '));
  ok(plog.filter(l => l.startsWith('asked ') && l.includes('pull request')).length === 2 && plog.filter(l => l.startsWith('opened ')).length === 2,
    'a pull request in each, written by an agent that worked there: ' + plog.filter(l => l.startsWith('opened ') || l.includes('pull request')).join(' | '));
  ok((await tag(pr).innerText()) === 'proj + proj-crawler', 'its card names both repositories: ' + await tag(pr).innerText());
  const got = n => cp.execSync(`cat ${PJ}/.weft/run/received-${n}.log 2>/dev/null || true`, { encoding: 'utf8' });
  ok(got(wb).includes('repository in `' + CR + '` only') && got(wa).includes('repository in `' + PJ + '` only'), 'each agent was told which repository its pull request is for');
  await p.screenshot({ path: W + '/folders-3-two-repos.png' });

  // --- a folder that is gone is not silently replaced by another one
  await p.evaluate(dir => spawnAt('claude', 600, 700, { text: 'Say hi.', draft: false, run: true, edit: false, extra: { cwd: dir } }), W + '/gone-folder');
  ok(await until(async () => (await p.locator('#toasts').innerText()).includes('does not exist')), 'starting in a missing folder fails and says why');

  ok(errs.length === 0, 'no page errors' + (errs.length ? ': ' + errs.slice(0, 3).join(' | ') : ''));
  await b.close();
  fs.rmSync(CR, { recursive: true, force: true }); fs.rmSync(W + '/crawler-remote.git', { recursive: true, force: true });
  console.log(fails ? fails + ' FAILED' : 'FOLDERS OK');
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); fs.rmSync(CR, { recursive: true, force: true }); process.exit(2); });
