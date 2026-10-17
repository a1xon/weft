// Rewind: a step that changed files is a dot on its card; a click shows its diff; "undo this step" puts
// just its files back (the repository's index, branches and refs stay as they are); the undo is a dot
// too and can be taken back; the agent hears about it with its next prompt.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs');
const { execFileSync } = require('child_process');
const W = process.env.SANDBOX;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const p = await (await b.newContext({ viewport: { width: 1700, height: 1000 } })).newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type() === 'error' && errs.push(m.text()));
  const st = id => p.locator('.nw[data-id="' + id + '"]').getAttribute('data-st');
  const waitSt = async (id, want, t = 60000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (want.includes(await st(id))) return true; await sleep(250); } return false; };
  const waitFor = async (fn, t = 30000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (await fn()) return true; await sleep(200); } return false; };
  const add = async node => p.evaluate(nd => new Promise(res => {
    const before = new Set(S.nodes.keys());
    send('node_add', { node: nd });
    const t = setInterval(() => { const id = [...S.nodes.keys()].find(k => !before.has(k)); if (id) { clearInterval(t); res(id); } }, 50);
  }), node);
  const PROJ = W + '/proj', SLUG = PROJ + '/textkit/slug.py';
  const git = (...a) => execFileSync('git', ['-C', PROJ, ...a], { encoding: 'utf8' });
  const orig = fs.readFileSync(SLUG, 'utf8');
  const dots = id => p.locator('.nw[data-id="' + id + '"] .cps .cpd');

  await p.goto(fs.readFileSync(W + '/url.txt', 'utf8').trim()); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400);
  await p.keyboard.press('Escape'); await sleep(200);
  const refs0 = git('for-each-ref');

  // --- an agent changes a file: its step is a dot
  const a = await add({ agent: 'claude', name: 'fixer', prompt: 'Fix the slugify function in textkit/slug.py so that tests/test_slug.py passes.', x: 120, y: 80 });
  await p.evaluate(id => send('arm', { id }), a);
  ok(await waitSt(a, ['done'], 90000), 'the agent works and finishes (' + await st(a) + ')');
  const fixed = fs.readFileSync(SLUG, 'utf8');
  ok(fixed !== orig, 'it changed textkit/slug.py');
  ok(await waitFor(async () => (await dots(a).count()) === 1, 15000), 'its step is a dot above the footer (' + await dots(a).count() + ')');
  ok(/textkit\/slug\.py/.test(await dots(a).first().getAttribute('title') || ''), 'that names the file it changed: "' + (await dots(a).first().getAttribute('title') || '').replace(/\n/g, ' | ') + '"');

  // --- its diff, full size
  await dots(a).first().click();
  ok(await waitFor(async () => (await p.locator('#reader .dv .dl').count()) > 3, 10000), 'a click shows what the step changed');
  ok((await p.locator('#reader .dv .df').first().innerText()).includes('textkit/slug.py'), 'as a diff of its file');
  ok((await p.locator('#reader .dv .da').count()) > 0 && (await p.locator('#reader .dv .dd').count()) > 0, 'added and removed lines in their colours');
  await p.screenshot({ path: W + '/rewind-1-diff.png' });

  // --- undo it: only its file goes back, the repository itself is untouched
  await p.locator('#reader button', { hasText: 'undo this step' }).click();
  ok(await waitFor(async () => fs.readFileSync(SLUG, 'utf8') === orig, 15000), 'undo puts textkit/slug.py back as it was before the step');
  ok(await waitFor(async () => (await p.locator('.nw[data-id="' + a + '"] .cpd.rw').count()) === 1, 8000), 'the undo is a dot of its own (↺)');
  ok((await p.locator('.nw[data-id="' + a + '"] .cpd.undone').count()) === 1, 'and the step shows as undone');
  ok(git('for-each-ref') === refs0, 'no branch, tag or ref appeared in the repository');
  ok(!git('status', '--porcelain', '--', '.', ':(exclude).weft').trim(), 'and the folder is clean again: "' + git('status', '--porcelain', '--', '.', ':(exclude).weft').trim() + '"');
  ok(/back as before/.test(await p.locator('#toasts').innerText()), 'a toast says what went back');

  // --- take the undo back
  await p.locator('.nw[data-id="' + a + '"] .cpd.rw').click();
  await p.waitForSelector('#reader button:has-text("undo this rewind")', { timeout: 8000 }).catch(() => {});
  await p.locator('#reader button', { hasText: 'undo this rewind' }).click();
  ok(await waitFor(async () => fs.readFileSync(SLUG, 'utf8') === fixed, 15000), 'undoing the undo puts the fix back');
  ok(await waitFor(async () => (await p.locator('.nw[data-id="' + a + '"] .cpd.undone').count()) === 1 && (await p.locator('.nw[data-id="' + a + '"] .cpd.rw.undone').count()) === 1, 8000), 'the step is not undone any more; the undo is');

  // --- undo again, then a new prompt: the agent hears what was undone first
  await dots(a).first().click();
  await p.locator('#reader button', { hasText: 'undo this step' }).click();
  ok(await waitFor(async () => fs.readFileSync(SLUG, 'utf8') === orig, 15000), 'undone once more');
  await sleep(500);
  const runs0 = (await p.evaluate(id => S.rt.get(id).runs, a));
  await p.evaluate(id => send('send', { id, text: 'Explain what textkit/wrap.py does in one sentence.', mode: 'auto' }), a);
  const answered = await waitFor(async () => (await p.evaluate(id => S.rt.get(id).runs, a)) > runs0 && await st(a) === 'done', 90000);
  ok(answered, 'it answers the next prompt (' + await st(a) + ')');
  if (!answered) console.log((await p.evaluate(id => (S.rt.get(id).pv || []).join('\n'), a)).replace(/<[^>]+>/g, ''));
  const got = fs.readFileSync(PROJ + '/.weft/run/received-' + a + '.log', 'utf8').split('=== prompt ===').pop();
  ok(/the person undid what you did for "Fix the slugify/.test(got) && /textkit\/slug\.py/.test(got), 'and that prompt told it what was undone');

  // --- a step still running can be looked at too; one that changed nothing is no dot
  // (the step, the undo, the undo taken back, the second undo)
  ok((await dots(a).count()) === 4, 'the prompt that changed nothing added no dot (' + await dots(a).count() + ')');
  ok(errs.length === 0, 'no page errors' + (errs.length ? ': ' + errs.join(' | ') : ''));
  await b.close();
  console.log(fails ? `${fails} FAILED` : 'all passed');
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
