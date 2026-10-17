// Pushes need no arming, loose ends and deploys (engine/loose.rs, engine/deploy.rs). A push runs when
// its commits are in. A push an older weft left waiting to be armed says what it has left ("ready to
// push", a chip in the top bar arms it); one whose commit reached the remote meanwhile is done by
// itself; a finished flow such a push holds says so on its cards. A deploy after a push runs its
// command on a checkout of exactly the pushed commit, never on unfinished work; with no command known
// it waits until one is typed into its options, then goes.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs');
const { execFileSync } = require('child_process');
const W = process.env.SANDBOX, PROJ = W + '/proj';
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const p = await (await b.newContext({ viewport: { width: 1600, height: 900 } })).newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type() === 'error' && errs.push(m.text()));
  const waitFor = async (fn, t = 20000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (await fn()) return true; await sleep(250); } return false; };
  const rt = id => p.evaluate(i => S.rt.get(i) || {}, id);
  const chip = () => p.locator('#tick .tk.rdy');
  const git = (...a) => execFileSync('git', ['-C', PROJ, ...a], { encoding: 'utf8' }).trim();
  const agent = async (text, y) => {
    const before = await p.evaluate(() => [...S.nodes.keys()]);
    await p.evaluate(([t, y]) => spawnAt('claude', 120, y, { text: t, run: true, draft: false, edit: false }), [text, y]);
    let id = null;
    await waitFor(async () => (id = (await p.evaluate(() => [...S.nodes.keys()])).find(k => !before.includes(k))));
    return id;
  };

  await p.goto(fs.readFileSync(W + '/url.txt', 'utf8').trim()); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400);
  await p.keyboard.press('Escape'); await sleep(300);

  // --- agent, commit, push: nothing to arm
  const a = await agent('Fix the slug bug', 100);
  ok(await waitFor(async () => (await rt(a)).status === 'done', 40000), 'the agent is done');
  const c = await p.evaluate(i => addGit(i, { x: 800, y: 100 }, 'commit'), a);
  ok(await waitFor(async () => (await rt(c)).status === 'done', 40000), 'its commit is done');
  const push = await p.evaluate(i => addGit(i, { x: 1150, y: 100 }, 'push'), c);
  ok(!(await p.evaluate(i => S.nodes.get(i).draft, push)), 'the push needs no arming');
  ok(await waitFor(async () => (await rt(push)).status === 'done', 30000), 'its commit is in: it pushes');
  ok(git('rev-parse', 'HEAD') === git('rev-parse', 'origin/main'), 'the remote has the commit');
  ok(await p.locator(`.nw[data-id="${push}"] .fb.cy`).isHidden(), 'the card has no arm button');

  // --- pushes an older weft left waiting to be armed: ready to push, or done by themselves
  const a2 = await agent('Fix the wrap bug', 420);
  ok(await waitFor(async () => (await rt(a2)).status === 'done', 40000), 'a second agent is done');
  const c2 = await p.evaluate(i => addGit(i, { x: 800, y: 420 }, 'commit'), a2);
  ok(await waitFor(async () => (await rt(c2)).status === 'done', 40000), 'its commit is done');
  const old = y => p.evaluate(([i, y]) => req('node_add', { node: { agent: 'git', git_op: 'push', draft: true, x: 1150, y }, from: i }).then(r => r.id), [c2, y]);
  const p2 = await old(420), p3 = await old(760);
  ok(await waitFor(async () => ((await rt(p2)).git || {}).left?.push === 1 && ((await rt(p3)).git || {}).left?.push === 1, 30000), 'git says each has one commit to push');
  ok(await waitFor(async () => /2 ready to push/.test(await chip().innerText().catch(() => ''))), 'the top bar says they are ready');
  ok(/ready · 1 commit to push/.test(await p.locator(`.nw[data-id="${p2}"]`).innerText()), 'so does the card');
  ok(await waitFor(async () => /held · push not armed/.test(await p.locator(`.nw[data-id="${a2}"] .fil`).innerText().catch(() => ''))),
    'the finished cards before them say what keeps them on the canvas');
  await p.screenshot({ path: W + '/loose-1-ready.png' });
  await chip().click(); await sleep(250);
  const items = await p.locator('.menu .mi').allInnerTexts();
  ok(items.filter(t => /^arm /.test(t) && /1 commit to push/.test(t)).length === 2, 'its menu arms them: ' + items.join(' | '));
  await p.locator('.menu .mi', { hasText: /^arm / }).first().click();
  ok(await waitFor(async () => (await rt(p2)).status === 'done' && (await rt(p3)).status === 'done', 45000),
    'the one armed pushes, and the other is done by itself: its commit is on the remote now');
  const logs = [p2, p3].map(async i => (((await rt(i)).git || {}).log || []).join('\n'));
  ok((await Promise.all(logs)).some(l => /pushed before/.test(l)), 'one of them says it had nothing left to push');
  ok(await waitFor(async () => (await chip().count()) === 0), 'nothing is ready any more');

  // --- deploy what the first push pushed: on that commit, not on the folder's unfinished work
  fs.appendFileSync(PROJ + '/textkit/slug.py', '\n# UNFINISHED work of another agent\n');
  const d = await p.evaluate(i => spawnDeploy(i, { x: 1500, y: 100 }), push);
  ok(!!d && await p.evaluate(i => !!S.nodes.get(i).deploy, d), 'a deploy card after the push');
  ok(await waitFor(async () => !(await p.locator('#fly').isHidden())), 'no deploy command known here: its options open');
  ok(/type its command/.test(await p.locator(`.nw[data-id="${d}"]`).innerText()), 'the card waits for its command');
  const cmd = 'grep -c UNFINISHED textkit/slug.py || echo CLEAN-CHECKOUT';
  await p.locator('#fly input[aria-label="Command"]').fill(cmd); await p.keyboard.press('Tab');
  ok(await waitFor(async () => Object.values(await p.evaluate(() => S.prefs.deploy || {})).includes(cmd)), 'the command is remembered for the folder');
  ok(await waitFor(async () => ['done', 'failed'].includes((await rt(d)).status), 40000), 'typed in, the deploy runs: '
    + JSON.stringify(await p.evaluate(i => { const r = S.rt.get(i) || {}; return { st: r.status, out: r.out }; }, d)));
  const out = (await rt(d)).out || '';
  ok((await rt(d)).status === 'done' && /\[weft\] deploying [0-9a-f]{7}/.test(out) && /CLEAN-CHECKOUT/.test(out), 'it ran on the pushed commit: ' + out.slice(0, 200));
  ok(/deployed/.test(await p.locator(`.nw[data-id="${d}"]`).innerText()), 'the card says it deployed');
  // with the command known, the next deploy goes at once
  await p.keyboard.press('Escape');
  const d2 = await p.evaluate(i => spawnDeploy(i, { x: 1500, y: 600 }), push);
  ok(await p.evaluate(i => S.nodes.get(i).cmd === 'grep -c UNFINISHED textkit/slug.py || echo CLEAN-CHECKOUT' && !S.nodes.get(i).draft, d2), 'the next deploy has the folder\'s command and waits for nothing');
  ok(await waitFor(async () => (await rt(d2)).status === 'done', 40000), 'it deploys right away: its push is in');
  await p.screenshot({ path: W + '/loose-2-deployed.png' });

  ok(errs.length === 0, 'no page errors' + (errs.length ? ': ' + errs.join(' | ') : ''));
  await b.close();
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
