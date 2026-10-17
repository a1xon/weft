// networks: the top bar holds the two networks used most; drag one onto the canvas and its outline
// follows, let go and a box asks what it is for, enter places and starts it (links in order, the push
// disarmed, the first agent told the task). Dropped on an agent the rest carries on from that agent.
// The library lists them with a picture; roles go to different agents; "place only" starts nothing;
// selected cards are saved as a network of your own (the task turns back into {{task}}) and deleted.
// Check gates run the folder's check command (guessed from its files, typed in the box); every
// built-in network is placed once: its links, the agents its queued prompts are asked of, nothing
// left unfilled, nothing started.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs');
const W = process.env.SANDBOX;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const p = await (await b.newContext({ viewport: { width: 1600, height: 900 } })).newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type() === 'error' && errs.push(m.text()));
  const waitFor = async (fn, t = 30000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (await fn()) return true; await sleep(200); } return false; };
  const received = id => { try { return fs.readFileSync(W + '/proj/.weft/run/received-' + id + '.log', 'utf8'); } catch { return ''; } };
  const graph = () => p.evaluate(() => ({
    nodes: [...S.nodes.values()].map(n => ({ id: n.id, agent: n.agent, name: n.name, prompt: n.prompt, draft: n.draft, host: n.host, git_op: n.git_op, cmd: n.cmd, write_tests: n.write_tests, branch: n.branch, checks: (n.checks || []).map(c => c.kind), x: n.x, y: n.y })),
    edges: [...S.edges.values()].map(e => ({ from: e.from, to: e.to, on: e.on })),
  }));
  await p.goto(fs.readFileSync(W + '/url.txt', 'utf8').trim()); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400);
  await p.keyboard.press('Escape'); await sleep(300);

  // the top bar: two networks, the first two before any was used
  const chips = await p.$$eval('#nets .netchip', bs => bs.map(x => x.dataset.net));
  ok(chips.length === 2 && chips[0] === 'look' && chips[1] === 'review', 'the top bar holds two networks: ' + chips.join(', '));

  // drag "screenshot & ship" onto the canvas: the outline follows, the box opens where it was let go
  const chip = await p.locator('#nets .netchip[data-net=look]').boundingBox();
  await p.mouse.move(chip.x + 20, chip.y + 14); await p.mouse.down();
  await p.mouse.move(400, 300, { steps: 6 }); await p.mouse.move(300, 380, { steps: 6 });
  ok(await p.evaluate(() => !netGhost.hidden && netGhost.querySelectorAll('.ngc').length === 5), 'while dragged, its five cards follow as an outline');
  await p.screenshot({ path: W + '/nets-drag.png' });
  await p.mouse.up(); await sleep(500);
  ok(await p.locator('#netbox:not([hidden]) textarea').count() === 1, 'let go: the box asks what it is for');
  ok(await p.evaluate(() => document.activeElement === netbox.querySelector('textarea')), 'and has the keyboard');
  ok(await p.evaluate(() => S.nodes.size === 0), 'nothing is made yet');
  const guess = 'python3 -m unittest discover -s tests -q';
  ok(await p.locator('#netbox input[aria-label="must pass"]').inputValue() === guess, 'the box offers the folder\'s check command, guessed from its files: ' + guess);
  await p.keyboard.press('Enter');
  ok(await p.evaluate(() => netbox.querySelector('textarea').classList.contains('nope') && !netbox.hidden), 'enter with no task: it shakes, nothing starts');
  const task = 'Fix the slugify function in textkit/slug.py so that tests/test_slug.py passes.';
  await p.keyboard.type(task);
  await p.screenshot({ path: W + '/nets-box.png' });
  await p.keyboard.press('Enter');
  ok(await waitFor(() => p.evaluate(() => S.nodes.size === 5 && S.edges.size === 4)), 'enter: five cards and four links');
  let g = await graph();
  const by = k => g.nodes.find(k);
  const A = by(n => n.agent === 'claude'), K = by(n => n.agent === 'tests'), SH = by(n => n.agent === 'shot'), C = by(n => n.git_op === 'commit'), P = by(n => n.git_op === 'push');
  ok(A && K && SH && C && P, 'an agent, a check, a screenshot gate, a commit and a push');
  ok(K && K.cmd === guess && K.write_tests === false, 'the check runs the folder\'s command and writes no tests');
  ok(K && await p.locator(`.nw[data-id="${K.id}"] .ag`).innerText() === 'gate · check', 'its card says it is a check');
  const has = (f, t, on) => g.edges.some(e => e.from === f && e.to === t && (!on || e.on === on));
  ok(A && has(A.id, K.id) && has(K.id, SH.id, 'ok') && has(SH.id, C.id, 'ok') && has(C.id, P.id), 'linked agent → check → screenshot →(if ok) commit → push');
  ok(A && A.prompt === task && A.checks.includes('eta'), 'the agent got the task as its prompt, and an ETA watch');
  ok(P && P.draft && C && !C.draft, 'the push waits to be armed, the commit does not');
  ok(await waitFor(() => received(A.id).includes('slugify'), 40000), 'the agent started and was told the task');
  ok(await p.evaluate(() => S.prefs.netUse && S.prefs.netUse.look === 1), 'its use is counted');
  ok(g.nodes.every(n => n.y === A.y), 'the cards stand in one row');

  // dropped on an agent: review & fix carries on from it; the fix is asked of that same agent
  const before = g.nodes.length;
  const card = await p.locator(`.nw[data-id="${A.id}"] .hd`).boundingBox();
  const rv = await p.locator('#nets .netchip[data-net=review]').boundingBox();
  await p.mouse.move(rv.x + 20, rv.y + 14); await p.mouse.down();
  await p.mouse.move(card.x + 60, card.y + 20, { steps: 10 });
  ok(await p.evaluate(id => nodeEls.get(id).root.classList.contains('drop'), A.id), 'over an agent: the agent lights up');
  ok(await p.evaluate(() => netGhost.querySelectorAll('.ngc').length === 5), 'and the outline shows what comes after it (five cards)');
  await p.mouse.up();
  ok(await waitFor(() => p.evaluate(n => S.nodes.size === n + 5 && S.edges.size === 4 + 6, before)), 'let go: five more cards, six more links');
  g = await graph();
  const R = g.nodes.find(n => n.name === 'review'), F = g.nodes.find(n => n.agent === 'step');
  ok(R && R.agent !== 'claude' && R.agent !== 'sh', 'the review goes to a second agent: ' + (R && R.agent));
  ok(F && F.host === A.id, 'the fix is asked of the agent that did the work');
  if (!(F && F.host === A.id)) console.log(JSON.stringify(g, null, 0));
  ok(R && F && has(A.id, R.id) && has(R.id, F.id) && has(A.id, F.id), 'linked agent → review → fix, and agent → fix');
  ok(R && R.prompt.includes('slugify'), 'the reviewer is told the task (the agent\'s prompt)');
  ok(await p.locator('#netbox:not([hidden])').count() === 0, 'no box: it has its task already');

  // the library: networks first, with pictures; drag one out of it
  await p.click('#bLib'); await sleep(300);
  const items = await p.$$eval('#modal .li.net', xs => xs.length);
  ok(items >= 8, `the library lists the networks (${items})`);
  ok(await p.locator('#modal .ndia').count() === 1 && await p.locator('#modal .steps li').count() >= 4, 'the chosen one: a picture and what each card does');
  await p.screenshot({ path: W + '/nets-library.png' });
  const council = p.locator('#modal .li.net', { hasText: 'three plans' });
  await council.click(); await sleep(200);
  ok(await p.locator('#modal .me.nets h3', { hasText: 'three plans' }).count() === 1, 'a click shows it');
  await council.scrollIntoViewIfNeeded();
  const cb = await council.boundingBox();
  await p.mouse.move(cb.x + 30, cb.y + 10); await p.mouse.down();
  await p.mouse.move(800, 600, { steps: 8 });
  ok(await p.locator('#modal').isHidden(), 'dragged out of the library: the library goes away');
  await p.mouse.move(820, 640, { steps: 4 }); await p.mouse.up(); await sleep(500);
  ok(await p.locator('#netbox:not([hidden]) .nbr .seg').count() === 3, 'the box offers a choice of agent for each of the three roles');
  const picked = await p.$$eval('#netbox .nbr .seg', ss => ss.map(s => (s.querySelector('[aria-pressed=true]') || {}).textContent));
  ok(new Set(picked).size === 3, 'three different agents by default: ' + picked.join(', '));
  await p.keyboard.type('Add a title-case helper to textkit.');
  await p.click('#netbox .btn.dim'); // place only
  const n0 = g.nodes.length;
  ok(await waitFor(() => p.evaluate(n => S.nodes.size === n + 7 && S.edges.size === 4 + 6 + 6, n0)), 'place only: seven cards, six links');
  g = await graph();
  const plans = g.nodes.filter(n => (n.name || '').startsWith('plan ·'));
  const B = g.nodes.find(n => n.name === 'build the best');
  ok(plans.length === 3 && plans.every(n => n.draft), 'three plans, all waiting (nothing started)');
  ok(plans.every(n => n.name === 'plan · ' + n.agent), 'each named after its agent');
  ok(B && plans[0] && B.host === g.nodes.find(n => n.name === 'plan · ' + picked[0]).id, 'the best is built by the first planner, in its own session');
  ok(plans.every(n => n.prompt.includes('title-case helper') && n.prompt.includes('Do not modify')), 'every planner has the task inside the plan prompt');

  // save the council as a network of your own: the task becomes {{task}} again
  const ids = [...plans.map(n => n.id), B.id, ...g.edges.filter(e => e.from === B.id).map(e => e.to)];
  await p.evaluate(ids => select(ids), ids);
  const cbox = await p.locator(`.nw[data-id="${plans[0].id}"] .hd`).boundingBox();
  await p.mouse.click(cbox.x + 80, cbox.y + 18, { button: 'right' });
  await p.locator('.menu .mi', { hasText: 'save as network' }).click();
  await p.keyboard.type('my council'); await p.keyboard.press('Enter');
  ok(await waitFor(() => p.evaluate(() => Array.isArray(S.nets) && S.nets.length === 1)), 'saved: the daemon keeps it');
  const mine = await p.evaluate(() => S.nets[0]);
  ok(mine.title === 'my council' && mine.nodes.length === 5 && mine.links.length === 4, 'five cards, four links');
  if (mine.nodes.length !== 5 || mine.links.length !== 4) console.log(JSON.stringify(mine));
  const p1 = mine.nodes.find(t => t.agent === '@main');
  ok(p1 && p1.prompt.includes('{{task}}') && !p1.prompt.includes('title-case') && p1.prompt.includes('Do not modify'), 'the plan prompt is kept, the task is {{task}} again');
  ok(p1 && p1.name === 'plan · {{agent}}', 'the first agent is a role again: ' + (p1 && p1.name));
  const stepLink = mine.links.findIndex(l => mine.nodes.find(t => t.key === l.to).agent === 'step' && l.from === p1.key);
  ok(stepLink === Math.max(...mine.links.map((l, i) => (mine.nodes.find(t => t.key === l.to).agent === 'step' ? i : -1))), 'the link from the planner that builds is the last one into the step');
  const cfg = () => { try { return JSON.parse(fs.readFileSync(W + '/cfg/weft/config.json', 'utf8')); } catch { return {}; } };
  ok(await waitFor(async () => (cfg().networks || []).length === 1), 'it is in config.json, so every project has it');

  // it is in the library under "yours"; delete it there
  await p.click('#bLib'); await sleep(300);
  await p.locator('#modal .li.net', { hasText: 'my council' }).click(); await sleep(200);
  await p.locator('#modal .me.nets .btn.danger').click();
  ok(await waitFor(() => p.evaluate(() => S.nets.length === 0)), 'deleted');
  await p.keyboard.press('Escape');

  // the network used most moves up: after two councils it is in the top bar
  await p.evaluate(() => { S.prefs.netUse.council = 3; savePrefs(); renderNetChips(); });
  await sleep(700);
  ok((await p.$$eval('#nets .netchip', bs => bs.map(x => x.dataset.net)))[0] === 'council', 'the one used most comes first in the top bar');

  // every built-in network, placed once far below: links, hosts, placeholders filled, nothing started
  const report = await p.evaluate(async () => {
    const out = [];
    let y = 20000;
    for (const net of NETS) {
      const at = { x: 0, y }; y += 3000;
      const before = new Set(S.nodes.keys());
      await buildNet(net, { at, task: 'Fix issue #42: an empty title gives an empty slug', roles: netRoles(net), start: false, arm: false,
        check: 'make check', branch: branchFor('Fix issue #42') });
      await new Promise(r => setTimeout(r, 250));
      const made = [...S.nodes.keys()].filter(k => !before.has(k));
      const spot = new Map(made.map(id => { const n = S.nodes.get(id); return [(n.x - at.x) + ',' + (n.y - at.y), id]; }));
      const id = new Map(net.nodes.map(t => [t.key, spot.get(Math.round(t.x) + ',' + Math.round(t.y))]));
      const errs = [], entries = netEntries(net);
      if (made.length !== net.nodes.length) errs.push(`cards ${made.length}/${net.nodes.length}`);
      for (const l of net.links) if (![...S.edges.values()].some(e => e.from === id.get(l.from) && e.to === id.get(l.to) && e.on === (l.on || 'done'))) errs.push(`link ${l.from}->${l.to}`);
      for (const t of net.nodes) {
        const n = S.nodes.get(id.get(t.key)); if (!n) { errs.push('no card for ' + t.key); continue; }
        if (t.agent === 'step' && n.host !== id.get(t.host)) errs.push(`${t.key} is asked of ${(S.nodes.get(n.host) || {}).name}, not ${t.host}`);
        if ((t.cmd || '').includes('{{check}}') && n.cmd !== 'make check') errs.push(`${t.key} runs "${n.cmd}"`);
        if ((t.branch || '').includes('{{branch}}') && n.branch !== 'issue-42') errs.push(`${t.key} commits on "${n.branch}"`);
        if (/\{\{(task|check|branch|agent)\}\}/.test([n.prompt, n.name, n.cmd, n.branch].join(' '))) errs.push(`${t.key} keeps a placeholder`);
        if (t.agent === 'git' && t.git_op !== 'commit' && !n.draft) errs.push(`${t.key} is armed`);
        if (entries.includes(t.key) && !n.draft) errs.push(`${t.key} started`);
      }
      out.push(net.id + (errs.length ? ': ' + errs.join(', ') : ''));
    }
    return out;
  });
  const bad = report.filter(r => r.includes(':'));
  ok(report.length >= 15 && !bad.length, `every built-in network places right (${report.length}) ${bad.join(' | ')}`);

  // after a reload it is all still there
  // (its agents are at work, so the page opens on the canvas, not the pad)
  await p.reload(); await p.waitForFunction(() => padShown); await sleep(500);
  ok((await p.$$eval('#nets .netchip', bs => bs.map(x => x.dataset.net)))[0] === 'council', 'and stays there after a reload');
  ok(errs.length === 0, 'no page errors ' + errs.join(' | '));
  await b.close();
  console.log(fails ? `${fails} FAILED` : 'all passed');
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
