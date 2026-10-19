// The dial: let go of a link in empty space and a ring opens where the card will go, one slice per
// option. For an agent: right = a prompt the same agent answers, lower left = a fresh agent takes
// over, upper left = git.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs'); const cp = require('child_process');
const W = process.env.SANDBOX, BIN = process.env.WEFT_UI_BIN;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const p = await (await b.newContext({ viewport: { width: 1700, height: 900 } })).newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type() === 'error' && errs.push(m.text()));
  const st = id => p.locator('.nw[data-id="' + id + '"]').getAttribute('data-st');
  const waitSt = async (id, want, t = 60000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (want.includes(await st(id))) return true; await sleep(250); } return false; };
  const waitFor = async (fn, t = 30000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (await fn()) return true; await sleep(200); } return false; };
  const port = async id => p.locator('.nw[data-id="' + id + '"] .port.out').boundingBox();
  const nodes = () => p.locator('.nw').count();
  const edges = () => p.locator('#edgeG .ed').count();
  const dialOpen = () => p.locator('#dial:not([hidden])').count().then(n => n > 0);
  // press on a diamond, move by (dx, dy) and let go; the dial opens there: move toward pick
  // (default: the same way), optionally look, click
  let early = 0;
  const drag = async (id, dx, dy, look, pick = [dx, dy]) => {
    // a new card sits where its line was let go, maybe near the edge: pan so the drag stays in the window
    let o = await port(id);
    const py = Math.max(0, 60 - (o.y + dy)) - Math.max(0, o.y + dy - 840), px = Math.max(0, 60 - (o.x + dx)) - Math.max(0, o.x + dx - 1640);
    if (px || py) { await p.evaluate(([px, py]) => { S.v.x += px; S.v.y += py; applyView(); }, [px, py]); o = await port(id); }
    const x = o.x + o.width / 2, y = o.y + o.height / 2;
    await p.mouse.move(x, y); await p.mouse.down(); await p.mouse.move(x + dx, y + dy, { steps: 12 });
    if (await dialOpen()) early++;
    await p.mouse.up();
    await p.waitForSelector('#dial:not([hidden]) .dpill');
    const c = await p.evaluate(() => [parseFloat(dial.style.left), parseFloat(dial.style.top)]), r = Math.hypot(...pick);
    await p.mouse.move(c[0] + pick[0] / r * 96, c[1] + pick[1] / r * 96, { steps: 4 });
    if (look) await look();
    await p.mouse.down(); await p.mouse.up();
  };
  const received = id => { try { return fs.readFileSync(W + '/proj/.weft/run/received-' + id + '.log', 'utf8'); } catch { return ''; } };

  await p.goto(fs.readFileSync(W + '/url.txt', 'utf8').trim()); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400);
  await p.keyboard.type('Fix the slugify function in textkit/slug.py so that tests/test_slug.py passes. Keep the public API.');
  await p.keyboard.press('Control+Enter');
  await p.waitForSelector('.nw[data-id="n1"]');
  ok(await waitSt('n1', ['done']), 'the first agent finishes');
  // somewhere to the left, so there is room on the right
  const hd = await p.locator('.nw[data-id="n1"] .hd').boundingBox();
  await p.mouse.move(hd.x + 100, hd.y + 14); await p.mouse.down(); await p.mouse.move(hd.x - 250, hd.y + 14, { steps: 8 }); await p.mouse.up(); await sleep(300);

  // --- the dial itself
  const first = await nodes();
  {
    const o = await port('n1'), x = o.x + o.width / 2, y = o.y + o.height / 2;
    await p.mouse.move(x, y); await p.mouse.down(); await p.mouse.move(x + 240, y + 6, { steps: 12 });
    ok(!(await dialOpen()), 'while the link is being drawn there is no dial');
    ok((await p.locator('#ghost').innerText()) === 'let go here', 'the outline at the line\'s end is where the card will go');
    await p.mouse.up();
    ok(await dialOpen(), 'let go in empty space, the dial opens there');
    ok((await p.locator('#dial .dseg').count()) === 3, 'a ring in three equal slices');
    ok(await p.locator('#ghost').isVisible(), 'the card\'s outline stays where the line ends');
    await p.mouse.move(x + 240 + 400, y + 6 + 10, { steps: 4 });
    ok((await p.locator('#dial .dpill.on b').innerText()) === 'queue a prompt', 'far past the ring a slice still counts');
    const pills = await p.locator('#dial .dpill').allInnerTexts();
    ok(pills.length === 3 && ['git', 'queue a prompt', 'hand off'].every(t => pills.some(x => x.includes(t))), 'with git, queue a prompt and hand off (' + pills.map(x => x.split('\n')[0]).join(' | ') + ')');
    ok(await p.locator('#rubber').isVisible(), 'the line stays pinned where it was let go');
    await p.keyboard.press('Escape');
    ok(!(await dialOpen()) && (await nodes()) === first && !(await p.locator('#rubber').isVisible()), 'escape drops it: nothing added, no line left');
    await p.mouse.move(x, y); await p.mouse.down(); await p.mouse.move(x + 240, y + 6, { steps: 12 }); await p.mouse.up();
    await p.keyboard.press('ArrowDown');
    ok((await p.locator('#dial .dpill.on b').innerText()) === 'hand off', 'arrow keys pick a direction: down is hand off');
    await p.keyboard.press('ArrowRight');
    ok((await p.locator('#dial .dpill.on b').innerText()) === 'queue a prompt', 'right is queue a prompt');
    await sleep(250); await p.screenshot({ path: W + '/dial-1-right.png' });
    await p.keyboard.press('Enter');
  }

  // --- right: a prompt the same agent answers
  await p.waitForSelector('.nw[data-step=true] .ask-edit');
  ok((await nodes()) === first + 1 && (await edges()) === 1, 'a card appears where the link was let go, linked to the agent');
  const stepId = await p.locator('.nw[data-step=true]').getAttribute('data-id');
  ok((await p.locator('.nw[data-step=true] .ag').innerText()).includes('same agent'), 'it says it is the same agent');
  const before = (await p.evaluate(() => S.rt.get('n1').out));
  await p.keyboard.type('Now explain textkit/wrap.py in one sentence.'); await p.keyboard.press('Enter');
  ok(await waitSt(stepId, ['busy', 'starting', 'done']), 'it asks the agent (' + await st(stepId) + ')');
  ok(await waitSt(stepId, ['done']), 'and the answer comes back to the card');
  const ans = await p.locator('.nw[data-id="' + stepId + '"] .ans').innerText();
  ok(ans.length > 5, 'with its text: "' + ans.replace(/\s+/g, ' ').slice(0, 70) + '"');
  ok(received('n1').includes('Now explain textkit/wrap.py'), 'the prompt reached agent n1 in its own session (no new agent was started)');
  ok(received(stepId) === '', 'nobody else received it');
  ok((await p.evaluate(() => S.rt.get('n1').out)) === before && (await st('n1')) === 'done', 'the agent\'s own result is unchanged');
  await p.screenshot({ path: W + '/dial-2-step.png' });

  // --- down: a fresh agent takes over
  const nBefore = await nodes();
  await drag('n1', 8, 250, async () => {
    ok((await p.locator('#dial .dpill.on b').innerText()) === 'hand off', 'downward is highlighted: hand off');
  });
  await p.waitForSelector('.nw:not([data-step=true]):not([data-id="n1"]) .ask-edit');
  ok((await nodes()) === nBefore + 1, 'a new agent card appears below');
  const hid = await p.evaluate(() => [...S.nodes.values()].filter(n => n.takeover === 'n1').pop().id);
  ok(await p.locator('#handG .hand').count() >= 1, 'a baton line ties it to the agent it takes over from');
  await p.keyboard.type('Run the whole test suite and report.'); await p.keyboard.press('Enter');
  ok(await waitSt(hid, ['starting', 'busy', 'done'], 60000), 'when enter is pressed it starts once the note is in (' + await st(hid) + ')');
  ok(await waitFor(async () => received(hid).includes('Taking over from'), 30000), 'it was handed a handover document');
  ok(received('n1').includes('take over this work from you'), 'written by the old agent, asked in its own session');

  // --- up: git. A commit is one agent's work; the commits meet in one push; a PR comes after it
  await p.keyboard.press('Escape');
  await drag('n1', 6, -230, async () => {
    ok((await p.locator('#dial .dpill.on b').innerText()) === 'git', 'upward is highlighted: git');
  });
  await p.waitForSelector('.menu .mi');
  const gitItems = await p.locator('.menu .mi').allInnerTexts();
  ok(['commit', 'push', 'open PR'].every(w => gitItems.some(t => t.startsWith(w))), 'it offers commit, push and open PR (' + gitItems.map(t => t.split('\n')[0]).join(' | ') + ')');
  await p.screenshot({ path: W + '/dial-3-git.png' });
  await p.locator('.menu .mi', { hasText: /^commit/ }).click();
  await p.waitForSelector('.nw[data-kind=git]');
  const cid = await p.locator('.nw[data-kind=git]').first().getAttribute('data-id');
  ok((await p.locator('.nw[data-id="' + cid + '"] .ag').innerText()).includes('commit'), 'a commit step: ' + await p.locator('.nw[data-id="' + cid + '"] .ag').innerText());
  ok(await waitSt(cid, ['busy', 'done']), 'it starts at once: the agent is asked to commit its own work (' + await st(cid) + ')');
  ok(await waitSt(cid, ['done'], 60000), 'and it resolves (' + await st(cid) + ')');
  ok(received('n1').includes('Commit the work you just did'), 'the request went to the agent that made the change');
  ok((await p.locator('.nw[data-id="' + cid + '"] .ans').innerText()).includes('passes on'), 'the card says what it passes on: ' + (await p.locator('.nw[data-id="' + cid + '"] .ans').innerText()).replace(/\s+/g, ' ').slice(-90));

  // push after it: another drag up, from the commit step
  await drag(cid, 4, -220, async () => {
    ok((await p.locator('#dial .dpill.on b').innerText()) === 'push', 'from a commit, left is push');
    const pills2 = await p.locator('#dial .dpill b').allInnerTexts();
    ok(pills2.includes('next agent'), 'and to the right the next agent, which gets what was committed (' + pills2.join(' | ') + ')');
  }, [-1, 0]);
  const pid = await p.evaluate(() => { for (let i = 0; i < 40; i++) {} return null; }).then(async () => {
    for (let i = 0; i < 40; i++) { const id = await p.evaluate(() => { const g = [...S.nodes.values()].filter(n => n.agent === 'git' && n.git_op === 'push').pop(); return g ? g.id : null; }); if (id) return id; await sleep(150); }
    return null;
  });
  ok(!!pid && await p.evaluate(id => !S.nodes.get(id).draft, pid), 'the push needs no arming');
  ok(await waitSt(pid, ['done'], 60000), 'its commit is in: it pushes (' + await st(pid) + ')');

  // a second agent: its link dropped on the commit gets its own commit, joined to the same push
  const nAgent = await p.evaluate(async () => {
    const r = await req('node_add', { node: { agent: 'claude', x: 160, y: 560, prompt: 'Fix the wrap function in textkit/wrap.py so that tests/test_wrap.py passes. Keep the public API.', addenda: [] }, run: true });
    return r.id;
  });
  ok(await waitSt(nAgent, ['done'], 60000), 'a second agent finishes (' + await st(nAgent) + ')');
  const commitsBefore = await p.evaluate(() => [...S.nodes.values()].filter(n => n.agent === 'git' && n.git_op === 'commit').length);
  await p.evaluate(() => { fit(); });
  await sleep(500);
  await p.screenshot({ path: W + '/dial-3b-before-drop.png' });
  const o2 = await port(nAgent), ch = await p.locator('.nw[data-id="' + cid + '"] .hd').boundingBox();
  console.log('port', JSON.stringify(o2), 'commit header', JSON.stringify(ch));
  await p.mouse.move(o2.x + o2.width / 2, o2.y + o2.height / 2); await p.mouse.down();
  await p.mouse.move(ch.x + 100, ch.y + 14, { steps: 14 }); await p.mouse.up();
  ok(await waitFor(async () => (await p.evaluate(() => [...S.nodes.values()].filter(n => n.agent === 'git' && n.git_op === 'commit').length)) === commitsBefore + 1, 10000),
    'dropped on a commit that has an agent, the second agent gets its own commit');
  const sib = await p.evaluate(a => { const e = [...S.edges.values()].find(e => e.from === a); return e ? e.to : null; }, nAgent);
  ok(!!sib && sib !== cid, 'linked from the second agent (' + sib + ')');
  ok(await waitFor(() => p.evaluate(([s, pu]) => [...S.edges.values()].some(e => e.from === s && e.to === pu), [sib, pid]), 8000), 'and joined to the same push');
  ok((await p.locator('.nw[data-id="' + cid + '"]').count()) === 1 && (await p.evaluate(c => [...S.edges.values()].filter(e => e.to === c).length, cid)) === 1, 'the first commit still has only its own agent');
  ok(await waitSt(sib, ['done'], 60000), 'its commit resolves (' + await st(sib) + ')');
  await p.screenshot({ path: W + '/dial-4-two-commits.png' });

  // the push runs again with the commit linked in after it
  ok(await waitFor(async () => /Fix wrap\.py/.test(cp.execFileSync('git', ['--git-dir', W + '/remote.git', 'log', '--format=%s', 'main'], { encoding: 'utf8' })), 60000)
    && await waitSt(pid, ['done'], 30000), 'the commit linked in later is pushed too (' + await st(pid) + ')');
  const remote = cp.execFileSync('git', ['--git-dir', W + '/remote.git', 'log', '--format=%s', 'main'], { encoding: 'utf8' });
  ok(/Fix slug\.py/.test(remote) && /Fix wrap\.py/.test(remote), 'both agents\' commits are on the remote: ' + remote.split('\n').slice(0, 3).join(' | '));

  // up from the push: open the PR; an agent that did the work writes it
  await drag(pid, 4, -220, async () => {
    ok((await p.locator('#dial .dpill.on b').innerText()) === 'open PR', 'from a push, left is open PR');
  }, [-1, 0]);
  const prid = await waitFor(async () => !!(await p.evaluate(() => [...S.nodes.values()].find(n => n.agent === 'git' && n.git_op === 'pr')))).then(() => p.evaluate(() => [...S.nodes.values()].find(n => n.agent === 'git' && n.git_op === 'pr').id));
  ok(await p.evaluate(id => !S.nodes.get(id).draft, prid), 'the PR needs no arming either');
  ok(await waitSt(prid, ['done'], 60000), 'an agent opens it (' + await st(prid) + ')');
  ok((await p.locator('.nw[data-id="' + prid + '"] a.prlink').getAttribute('href')) === 'https://example.test/acme/textkit/pull/7', 'the card links to the pull request');
  const asked = received('n1') + received(nAgent);
  ok(asked.toLowerCase().includes('open a pull request for the work that was just pushed'), 'the request went to an agent that did the work, with the report');
  await p.screenshot({ path: W + '/dial-5-pr.png' });

  // to the right of the PR: an agent that tests it live, starting from everything that was carried
  await p.evaluate(id => centerOn(id), prid); await sleep(300); // in view, not under the toast in the corner
  const o4 = await p.locator('.nw[data-id="' + prid + '"] .port.out').boundingBox();
  await p.mouse.move(o4.x + o4.width / 2, o4.y + o4.height / 2); await p.mouse.down();
  await p.mouse.move(o4.x + o4.width / 2 + 240, o4.y + o4.height / 2 + 6, { steps: 10 });
  ok((await p.locator('#ghost').innerText()) === 'next agent', 'after the PR there is only the next agent: the label says so');
  await p.mouse.up();
  ok(!(await dialOpen()), 'one option needs no dial: letting go adds it');
  await p.waitForSelector('.ask-edit');
  await p.keyboard.type('Test the new features live and tell me what you see.'); await p.keyboard.press('Enter');
  const tid = await p.evaluate(() => [...S.nodes.values()].filter(n => n.prompt && n.prompt.startsWith('Test the new features live')).pop().id);
  ok(await waitSt(tid, ['done'], 60000), 'the tester runs once the PR is open (' + await st(tid) + ')');
  const rec = received(tid);
  // (the push ran for each commit as it came in: what travels on is its latest round, the wrap fix)
  for (const want of ['Opened a pull request for', 'pull/7', 'Files changed (1)', 'textkit/wrap.py', 'Fix the slugify function', 'Fix the wrap function', 'Test the new features live']) {
    ok(rec.includes(want), 'the tester received ' + JSON.stringify(want));
  }
  await p.screenshot({ path: W + '/dial-6-chain.png' });

  // a prompt queued on an agent at work, its box left without enter (a click elsewhere, as when its commit is
  // drawn next): it waits for that work and is asked then. Kept a draft, it was never asked (rubbermetrics n127)
  const busy = await p.evaluate(async () => (await req('node_add', { node: { agent: 'claude', x: 2600, y: 200, prompt: 'LONG=20 Explain textkit/slug.py in one sentence.', addenda: [] }, run: true })).id);
  ok(await waitSt(busy, ['busy']), 'another agent is at work (' + await st(busy) + ')');
  await p.evaluate(id => centerOn(id), busy); await sleep(400);
  await drag(busy, 240, 6, async () => {
    ok((await p.locator('#dial .dpill.on b').innerText()) === 'queue a prompt', 'to its right: queue a prompt');
  });
  await p.waitForSelector('.nw[data-step=true] .ask-edit');
  const qs = await p.locator('.nw[data-step=true]:has(.ask-edit)').getAttribute('data-id');
  await p.keyboard.type('Now explain textkit/wrap.py in one sentence.');
  await p.locator('.nw[data-id="' + busy + '"] .hd').click();
  ok(await waitFor(() => p.evaluate(id => !S.nodes.get(id).draft && S.nodes.get(id).prompt.startsWith('Now explain'), qs), 5000), 'left without enter, it keeps its prompt and is no draft');
  // (its status comes in the daemon's next rt message, after the flow that says it is no draft)
  ok(await waitSt(qs, ['waiting'], 5000), 'it waits for the agent (' + await st(qs) + ': ' + await p.locator('.nw[data-id="' + qs + '"] .stx').innerText() + ')');
  ok(!received(busy).includes('Now explain textkit/wrap.py'), 'nothing is asked while the agent works');
  // sent with ctrl+enter (the bar's send key), as rubbermetrics n143 was: it fired a draft, nothing fires one, it stayed one
  await drag(busy, 300, 330, null, [240, 6]);
  await p.waitForSelector('.nw[data-step=true] .ask-edit');
  const qc = await p.locator('.nw[data-step=true]:has(.ask-edit)').getAttribute('data-id');
  await p.keyboard.type('And textkit/__init__.py?'); await p.keyboard.press('Control+Enter');
  ok(await waitFor(() => p.evaluate(id => !S.nodes.get(id).draft && S.nodes.get(id).prompt.startsWith('And textkit'), qc), 5000), 'ctrl+enter: no draft either');
  ok(await waitSt(qc, ['waiting'], 5000), 'it waits for the agent too (' + await st(qc) + ')');
  ok(!received(busy).includes('And textkit/__init__.py'), 'nothing is asked while the agent works');
  ok(await waitSt(qs, ['done'], 60000), 'when the agent is done it is asked, and the answer comes back (' + await st(qs) + ')');
  ok(received(busy).includes('Now explain textkit/wrap.py'), 'in that agent\'s own session');
  ok(await waitSt(qc, ['done'], 60000), 'the one sent with ctrl+enter is asked after it (' + await st(qc) + ')');
  ok(received(busy).includes('And textkit/__init__.py'), 'in the same session');

  ok(early === 0, 'the dial never opened while a link was still being drawn');
  ok(errs.length === 0, 'no page errors' + (errs.length ? ': ' + errs.slice(0, 3).join(' | ') : ''));
  await b.close();
  console.log(fails ? fails + ' FAILED' : 'DIAL OK');
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); process.exit(2); });
