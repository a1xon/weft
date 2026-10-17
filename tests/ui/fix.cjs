// What failed, fixed: a click on the top bar's "N failed" starts a fresh agent a reasoning level up,
// told what the failed card was asked and said; a second click shows it instead of starting another.
// The card says who is fixing it, its menu offers the same, and `weft retry` runs it again with
// what it had and what was fixed.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs');
const { execFileSync } = require('child_process');
const W = process.env.SANDBOX;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const p = await (await b.newContext({ viewport: { width: 1600, height: 900 } })).newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type() === 'error' && errs.push(m.text()));
  const waitFor = async (fn, t = 15000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (await fn()) return true; await sleep(200); } return false; };
  const st = id => p.evaluate(i => (S.rt.get(i) || {}).status || '', id);
  const ids = () => p.evaluate(() => [...S.nodes.keys()]);
  const chip = () => p.locator('#tick .tk.fx');

  await p.goto(fs.readFileSync(W + '/url.txt', 'utf8').trim()); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400);
  await p.keyboard.type('Make slugify keep unicode letters. FAIL_TASK'); await p.keyboard.press('Control+Enter');
  await p.waitForSelector('.nw[data-id="n1"]');
  ok(await waitFor(async () => (await st('n1')) === 'failed', 30000), 'the agent fails');
  ok(await waitFor(async () => /1 failed · fix it/.test(await chip().innerText().catch(() => ''))), 'the top bar says so and offers to fix it');
  await p.screenshot({ path: W + '/fix-1-failed.png' });

  // --- one click: a fresh agent, a level up, told what failed
  await chip().click();
  ok(await waitFor(async () => (await ids()).includes('n2')), 'a fixer comes up');
  const fixer = await p.evaluate(() => S.nodes.get('n2'));
  ok(JSON.stringify(fixer.fixes) === '["n1"]' && fixer.agent === 'claude', 'it knows what it fixes (' + JSON.stringify(fixer.fixes) + ', ' + fixer.agent + ')');
  ok(fixer.effort === 'high', 'it thinks a level above the default (' + fixer.effort + ')');
  ok(fixer.prompt.includes('## n1 "') && fixer.prompt.includes('FAIL_TASK') && /retry <id>/.test(fixer.prompt), 'its brief has what the card was asked and how to run it again');
  ok(await p.evaluate(() => S.sel.has('n2')), 'it is selected');
  ok(await waitFor(async () => /fixing/.test(await chip().innerText())), 'the chip says it is being fixed: ' + await chip().innerText());
  ok(await waitFor(async () => /n2 is fixing it/.test(await p.locator('.nw[data-id="n1"]').innerText())), 'the failed card says who fixes it');
  await p.screenshot({ path: W + '/fix-2-fixing.png' });

  // --- a second click while it works shows it, starts nothing
  await p.evaluate(() => select([]));
  await chip().click(); await sleep(600);
  ok((await ids()).length === 2 && await p.evaluate(() => S.sel.has('n2')), 'clicked again: the same fixer is shown');
  await p.evaluate(() => centerOn('n1')); await sleep(300);
  await p.locator('.nw[data-id="n1"] .hd').click({ button: 'right' }); await sleep(200);
  const items = await p.locator('.menu .mi').allInnerTexts();
  ok(items.some(t => /show fix:/.test(t)) && items.some(t => /run it again/.test(t)), 'the failed card\'s menu shows the fixer and runs it again: ' + items.slice(0, 3).join(' | '));
  await p.keyboard.press('Escape'); await sleep(150);

  // --- the fixer runs it again with what was fixed (here by hand: the simulated agent does not)
  ok(await waitFor(async () => !['busy', 'starting', 'new', 'ready'].includes(await st('n2')), 30000), 'the fixer finishes');
  const proj = W + '/proj', log = proj + '/.weft/run/received-n1.log';
  execFileSync(process.env.WEFT_UI_BIN, ['retry', 'n1', 'slugify keeps unicode now'], { cwd: proj, env: { ...process.env, WEFT_NODE: 'n2' } });
  ok(await waitFor(async () => fs.readFileSync(log, 'utf8').includes('what was fixed since (by "' + fixer.name + '")\nslugify keeps unicode now'), 20000),
    'run again, its task carries what was fixed');
  ok(await waitFor(async () => (await p.evaluate(() => (S.rt.get('n1') || {}).runs)) >= 2, 20000), 'it ran a second time');
  ok(await waitFor(async () => (await st('n1')) === 'failed', 30000) && await waitFor(async () => /fix it/.test(await chip().innerText())),
    'failing again (it was told to), it can be fixed again');
  await p.screenshot({ path: W + '/fix-3-again.png' });

  ok(errs.length === 0, 'no page errors' + (errs.length ? ': ' + errs.join(' | ') : ''));
  await b.close();
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
