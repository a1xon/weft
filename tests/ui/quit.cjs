// Quitting: one question when agents would end, a last screen, and nothing left behind.
const { chromium } = require(process.env.PLAYWRIGHT);
const { execFileSync, spawnSync } = require('child_process');
const fs = require('fs');
const W = process.env.SANDBOX, BIN = process.env.WEFT_UI_BIN, SOCK = 'weft-ui';
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
const tmux = (...a) => spawnSync('tmux', ['-L', SOCK, ...a], { encoding: 'utf8' }).stdout || '';
const windows = () => tmux('list-windows', '-a', '-F', '#{window_name}').split(/\s+/).filter(w => w && w !== 'weft');
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const ctx = await b.newContext({ viewport: { width: 1500, height: 880 } });
  const p = await ctx.newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message));
  const url = fs.readFileSync(W + '/url.txt', 'utf8').trim();
  const waitFor = async (fn, t = 30000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (await fn()) return true; await sleep(200); } return false; };
  const st = id => p.locator('.nw[data-id="' + id + '"]').getAttribute('data-st');
  const busy = async ids => { for (const i of ids) if (!['busy', 'starting'].includes(await st(i))) return false; return true; };

  await p.goto(url); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400);
  await p.keyboard.press('Escape'); await sleep(200);

  // --- nothing running: the question is short, and cancelling changes nothing
  await p.click('#bQuit');
  const dlg = p.locator('#modal .win.quit');
  ok(await dlg.isVisible(), 'the quit button asks before it quits');
  ok((await dlg.innerText()).includes('No agent is running'), 'with nothing running it says so');
  ok(await p.locator('#modal .qa button').count() === 2, 'and offers just quit and cancel');
  await p.keyboard.press('Escape'); await sleep(200);
  ok(await dlg.count() === 0 || !(await p.locator('#modal').isVisible()), 'escape closes it');
  ok(await p.locator('#liveDot').innerText() === 'live', 'the canvas is still connected');

  // --- two agents at work: the question names them, cancel keeps everything
  await p.evaluate(() => {
    send('node_add', { node: { name: 'alpha', agent: 'claude', prompt: 'LONG=120 count slowly', x: 120, y: 160 }, run: true });
    send('node_add', { node: { name: 'beta', agent: 'codex', prompt: 'LONG=120 count slowly', x: 120, y: 360 }, run: true });
  });
  ok(await waitFor(async () => (await p.locator('.nw').count()) === 2, 8000), 'two agents are on the canvas');
  ok(await waitFor(() => busy(['n1', 'n2']), 60000), 'both are working (' + await st('n1') + ', ' + await st('n2') + ')');
  await p.locator('#canvas').click({ position: { x: 900, y: 600 } });
  await p.keyboard.press('q'); await sleep(250);
  const txt = (await dlg.innerText()).replace(/\s+/g, ' ');
  ok(/2 agents are running/.test(txt), 'the q key opens the question: ' + txt.slice(0, 120));
  ok(/alpha, beta are working right now|beta, alpha are working right now/.test(txt), 'it names the agents that are mid-work');
  ok(await p.locator('#modal .qa .btn.danger').innerText() === 'end all agents and quit', 'ending them is the primary choice');
  ok(await p.evaluate(() => document.activeElement === document.querySelector('#modal .qa .btn.danger')), 'and it has the focus, so enter does it');
  await p.screenshot({ path: W + '/quit-1-question.png' });
  await p.locator('#modal .qa button', { hasText: 'cancel' }).click(); await sleep(250);
  ok(await busy(['n1', 'n2']) && windows().length === 2, 'cancel leaves both agents and their windows alone');

  // --- keep them running: the daemon goes, the agents stay in tmux
  await p.keyboard.press('q'); await sleep(250);
  await p.locator('#modal .qa button', { hasText: 'keep agents running' }).click();
  ok(await waitFor(async () => await p.locator('#bye').isVisible(), 8000), 'a last screen replaces the canvas');
  const bye = (await p.locator('#bye').innerText()).replace(/\s+/g, ' ');
  ok(/2 agents keep running in tmux/.test(bye) && /weft stop/.test(bye) && /tmux -L weft-ui attach/.test(bye), 'it says what is left and how to look at it or end it: ' + bye.slice(0, 200));
  ok(await p.locator('#banner').isHidden(), 'and it does not pretend to reconnect');
  await sleep(600);
  ok(windows().length === 2, 'both agent windows are still running in tmux (' + windows() + ')');
  ok(spawnSync(BIN, ['ls'], { encoding: 'utf8', cwd: W + '/proj', env: process.env }).status !== 0, 'the daemon is gone');
  await p.screenshot({ path: W + '/quit-2-kept.png' });

  // --- weft again adopts them; this time end everything
  execFileSync(BIN, ['open', W + '/proj', '--no-open'], { env: process.env });
  await p.goto('about:blank'); await p.goto(url); await p.waitForSelector('.nw[data-id="n1"]'); await sleep(500);
  await p.keyboard.press('Escape');
  ok(await waitFor(() => busy(['n1', 'n2']), 20000), 'the next weft finds the agents still working (' + await st('n1') + ', ' + await st('n2') + ')');
  await p.locator('#canvas').click({ position: { x: 900, y: 600 } });
  await p.click('#bMore'); await sleep(150);
  ok(await p.locator('.menu .mi', { hasText: 'quit weft' }).count() === 1, 'the more menu has it too');
  await p.keyboard.press('Escape'); await sleep(150);
  await p.click('#bQuit'); await sleep(200);
  await p.keyboard.press('Enter');
  ok(await waitFor(async () => await p.locator('#bye').isVisible(), 8000), 'enter ends them: the last screen is up');
  const bye2 = (await p.locator('#bye').innerText()).replace(/\s+/g, ' ');
  ok(/2 agents ended/.test(bye2), 'it counts them: ' + bye2.slice(0, 160));
  ok(await waitFor(async () => tmux('list-sessions').trim() === '', 8000), 'no tmux session is left behind');
  ok(windows().length === 0, 'and no terminal');
  ok(spawnSync(BIN, ['ls'], { encoding: 'utf8', cwd: W + '/proj', env: process.env }).status !== 0, 'the daemon is gone');
  await p.screenshot({ path: W + '/quit-3-ended.png' });

  // --- and the canvas is the way it was, calm
  execFileSync(BIN, ['open', W + '/proj', '--no-open'], { env: process.env });
  await p.goto('about:blank'); await p.goto(url); await p.waitForSelector('.nw[data-id="n1"]'); await sleep(500);
  await p.keyboard.press('Escape');
  ok(await p.locator('.nw').count() === 2 && windows().length === 0, 'both cards are back and nothing started by itself');
  ok(!['busy', 'starting'].includes(await st('n1')), 'alpha is not running (' + await st('n1') + ')');
  ok(errs.length === 0, 'no page errors' + (errs.length ? ': ' + errs.join(' | ') : ''));
  await b.close();
  console.log(fails ? fails + ' FAILED' : 'ALL PASSED');
  process.exit(fails ? 1 : 0);
})();
