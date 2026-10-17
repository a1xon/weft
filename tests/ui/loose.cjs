// Loose ends and deploys (engine/loose.rs, engine/deploy.rs). A push nobody armed says what it has
// left ("ready to push", a chip in the top bar arms it); one whose commits reached the remote some
// other way is done by itself; a finished flow such a push holds says so on its cards. A deploy after
// a push runs its command on a checkout of exactly the pushed commit, never on unfinished work.
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

  // --- agent, commit, and a push nobody armed: it says what it has left
  const a = await agent('Fix the slug bug', 100);
  ok(await waitFor(async () => (await rt(a)).status === 'done', 40000), 'the agent is done');
  const c = await p.evaluate(i => addGit(i, { x: 800, y: 100 }, 'commit'), a);
  ok(await waitFor(async () => (await rt(c)).status === 'done', 40000), 'its commit is done');
  const push = await p.evaluate(i => addGit(i, { x: 1150, y: 100 }, 'push'), c);
  ok(await waitFor(async () => (await rt(push)).status === 'draft'), 'the push waits to be armed');
  ok(await waitFor(async () => ((await rt(push)).git || {}).left?.push === 1, 30000), 'git says it has one commit to push');
  ok(await waitFor(async () => /1 ready to push/.test(await chip().innerText().catch(() => ''))), 'the top bar says it is ready');
  ok(/ready · 1 commit to push/.test(await p.locator(`.nw[data-id="${push}"]`).innerText()), 'so does the card');
  ok(await waitFor(async () => /held · push not armed/.test(await p.locator(`.nw[data-id="${a}"] .fil`).innerText().catch(() => ''))),
    'the finished cards before it say what keeps them on the canvas');
  await p.screenshot({ path: W + '/loose-1-ready.png' });
  await chip().click(); await sleep(250);
  const items = await p.locator('.menu .mi').allInnerTexts();
  ok(items.some(t => /^arm /.test(t) && /1 commit to push/.test(t)), 'its menu arms it: ' + items.join(' | '));
  await p.locator('.menu .mi', { hasText: /^arm / }).first().click();
  ok(await waitFor(async () => (await rt(push)).status === 'done', 30000), 'armed, it pushes');
  ok(git('rev-parse', 'HEAD') === git('rev-parse', 'origin/main'), 'the remote has the commit');
  ok(await waitFor(async () => (await chip().count()) === 0), 'nothing is ready any more');

  // --- a push nobody armed whose commit reached the remote another way: done by itself
  const a2 = await agent('Fix the wrap bug', 420);
  ok(await waitFor(async () => (await rt(a2)).status === 'done', 40000), 'a second agent is done');
  const c2 = await p.evaluate(i => addGit(i, { x: 800, y: 420 }, 'commit'), a2);
  ok(await waitFor(async () => (await rt(c2)).status === 'done', 40000), 'its commit is done');
  const p2 = await p.evaluate(i => addGit(i, { x: 1150, y: 420 }, 'push'), c2);
  ok(await waitFor(async () => ((await rt(p2)).git || {}).left?.push === 1, 30000), 'its push has a commit to push');
  git('push', '-q', 'origin', 'main');
  ok(await waitFor(async () => (await rt(p2)).status === 'done', 45000), 'pushed by hand: the push nobody armed is done by itself');
  ok(/pushed before/.test(((await rt(p2)).git || {}).log?.join('\n') || ''), 'its log says why');

  // --- deploy what the first push pushed: on that commit, not on the folder's unfinished work
  fs.appendFileSync(PROJ + '/textkit/slug.py', '\n# UNFINISHED work of another agent\n');
  const d = await p.evaluate(i => spawnDeploy(i, { x: 1500, y: 100 }), push);
  ok(!!d && await p.evaluate(i => !!S.nodes.get(i).deploy && S.nodes.get(i).draft, d), 'a deploy card after the push, disarmed');
  ok(await waitFor(async () => !(await p.locator('#fly').isHidden())), 'no deploy command known here: its options open');
  const cmd = 'grep -c UNFINISHED textkit/slug.py || echo CLEAN-CHECKOUT';
  await p.locator('#fly input[aria-label="Command"]').fill(cmd); await p.keyboard.press('Tab'); await sleep(700);
  ok(await p.evaluate(c => Object.values(S.prefs.deploy || {}).includes(c), cmd), 'the command is remembered for the folder');
  ok(await waitFor(async () => /ready to deploy/.test(await chip().innerText().catch(() => ''))), 'its push is in: ready to deploy');
  await p.evaluate(i => send('arm', { id: i }), d);
  ok(await waitFor(async () => ['done', 'failed'].includes((await rt(d)).status), 40000), 'the deploy ran: '
    + JSON.stringify(await p.evaluate(i => { const r = S.rt.get(i) || {}; return { st: r.status, alive: r.alive, q: (r.queue || []).length, out: r.out, pv: (r.pv || []).slice(-6) }; }, d))
    + ' toasts: ' + await p.locator('#toasts').innerText());
  const out = (await rt(d)).out || '';
  ok((await rt(d)).status === 'done' && /\[weft\] deploying [0-9a-f]{7}/.test(out) && /CLEAN-CHECKOUT/.test(out), 'it ran on the pushed commit: ' + out.slice(0, 200));
  ok(/deployed/.test(await p.locator(`.nw[data-id="${d}"]`).innerText()), 'the card says it deployed');
  await p.screenshot({ path: W + '/loose-2-deployed.png' });

  ok(errs.length === 0, 'no page errors' + (errs.length ? ': ' + errs.join(' | ') : ''));
  await b.close();
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
