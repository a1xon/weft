// Boards: every folder weft was started in is a board. The pad lists them, the board's name in the
// top bar unfolds the same list, another board that needs you shows a badge; a click switches the
// window over (a sleeping board wakes up), `weft` in a folder of a board opens that board.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs'); const cp = require('child_process'); const http = require('http');
const W = process.env.SANDBOX, BIN = process.env.WEFT_UI_BIN;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
const until = async (f, ms = 12000) => { const t = Date.now(); while (Date.now() - t < ms) { try { if (await f()) return true; } catch {} await sleep(250); } return false; };
// a board whose agents are at work opens on the canvas, not the pad: open it the way N does
const padUp = async pg => { await pg.waitForFunction(() => padShown); await pg.evaluate(() => pad.open || openPad()); await pg.waitForSelector('#pad:not([hidden]) textarea'); };
const STATES = W + '/state/weft';
const record = dir => fs.readdirSync(STATES).filter(f => f.endsWith('.json') && !f.endsWith('.live.json'))
  .map(f => ({ f, ...JSON.parse(fs.readFileSync(STATES + '/' + f, 'utf8')) })).find(i => i.dir === dir);
const api = (dir, body) => new Promise((res, rej) => {
  const i = record(dir), data = JSON.stringify(body);
  const r = http.request({ host: '127.0.0.1', port: i.port, path: '/api/cmd', method: 'POST',
    headers: { 'X-Weft-Token': i.token, 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(data) } },
  s => { let b = ''; s.on('data', d => b += d); s.on('end', () => { try { res(JSON.parse(b)); } catch (e) { rej(e); } }); });
  r.on('error', rej); r.end(data);
});
const ENV = { ...process.env }; for (const k of ['WEFT_URL', 'WEFT_TOKEN', 'WEFT_PROJECT', 'WEFT_NODE']) delete ENV[k];
const weft = (args, cwd) => cp.execFileSync(BIN, args, { cwd: cwd || W, encoding: 'utf8', env: ENV });

(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const ctx = await b.newContext({ viewport: { width: 1500, height: 900 } });
  const p = await ctx.newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type() === 'error' && errs.push(m.text()));
  const url = fs.readFileSync(W + '/url.txt', 'utf8').trim();
  const rows = pg => pg.locator('.pboards .bdrow:not(.bdnew)').allInnerTexts();
  await p.goto(url); await p.waitForSelector('#pad:not([hidden]) textarea');
  await sleep(600);

  // --- one board: no list, only the way to another one
  ok((await rows(p)).length === 0, 'with one board the pad lists none');
  ok(await p.locator('.pboards .bdnew').isVisible(), 'but offers a new board');
  ok((await p.locator('#projName b').innerText()) === 'proj', 'the top bar names the board');

  // --- a second board, made from a terminal
  const other = W + '/other';
  cp.execSync(`cp -r ${W}/proj ${other} && rm -rf ${other}/.weft && mkdir -p ${other}/sub`);
  const made = weft(['new', other, '--no-open']);
  ok(/127\.0\.0\.1/.test(made), '`weft new` starts a board of its own');
  ok(await until(async () => (await rows(p)).length === 2), 'the pad lists both boards now: ' + JSON.stringify(await rows(p)));
  const r0 = await rows(p);
  ok(/^proj/.test(r0[0]) && /no agents running/.test(r0[0]), 'this board first: ' + r0[0]);
  ok(/^other/.test(r0[1]) && /no agents running/.test(r0[1]), 'then the other: ' + r0[1]);
  ok(await p.locator('.pboards .bdrow.me').count() === 1 && (await p.locator('.pboards .bdrow.me').innerText()).startsWith('proj'), 'the board you are on is marked');

  // --- an agent there needs you: the row says so, the top bar shows a badge
  ok((await api(other, { op: 'node_add', node: { agent: 'claude', name: 'asker', prompt: 'ASK_QUESTION fix the slug', x: 100, y: 100 }, run: true })).ok, 'an agent starts on the other board');
  ok(await until(async () => /1 needs you/.test((await rows(p))[1])), 'its row says it needs you: ' + (await rows(p))[1]);
  ok(/1 agent/.test((await rows(p))[1]), 'and counts its agent');
  ok(await until(async () => (await p.locator('#bdBadge').isVisible()) && /other needs you/.test(await p.locator('#bdBadge').innerText())), 'the top bar shows "other needs you"');
  await p.screenshot({ path: W + '/boards-1-pad.png' });

  // --- the same list under the board's name, on the canvas too
  await p.keyboard.press('Escape'); await sleep(250);
  await p.keyboard.press('b'); await sleep(300);
  ok(await p.locator('.bdmenu').isVisible(), 'B unfolds the board list under the name');
  ok(await p.locator('.bdmenu .bdrow:not(.me):not(.bdnew)').evaluate(e => document.activeElement === e), 'with the other board ready to take');
  await p.screenshot({ path: W + '/boards-2-menu.png' });
  await p.keyboard.press('Escape'); await sleep(200);
  ok(!(await p.locator('.bdmenu').count()), 'Esc folds it away');

  // --- `weft` in a folder of that board opens it, with new agents starting there
  const sub = weft(['--no-open'], other + '/sub');
  ok(sub.includes('weft  ' + other + '\n') && sub.includes('new agents start in ' + other + '/sub'), '`weft` in other/sub opens the other board: ' + sub.split('\n')[0]);
  ok(!!record(other) && !record(other + '/sub'), 'and makes no board of its own');
  const p2 = await ctx.newPage();
  await p2.goto(sub.trim().split('\n').pop().trim()); await p2.waitForFunction(() => padShown);
  ok(await p2.locator('#pad').isHidden(), 'an agent there needs you: the page opens on its canvas, not the pad');
  await padUp(p2);
  ok(await until(async () => (await p2.locator('#pad .pname').innerText()).trim() === 'sub'), 'that page starts new agents in sub: ' + await p2.locator('#pad .pname').innerText());
  ok(!/at=/.test(await p2.evaluate(() => location.href)), 'and the address forgets it again');
  await p2.close();

  // --- a click switches this window over (and what you chose here comes along: the settings are everyone's)
  await p.keyboard.press('n'); await sleep(300);
  await p.locator('#pad .pagents button', { hasText: /^codex$/ }).click();
  await sleep(1200);
  await p.locator('.pboards .bdrow:not(.me):not(.bdnew)').click();
  ok(await until(() => p.evaluate(() => location.port).then(x => x === String(record(other).port))), 'a click on the other board switches the window to it');
  await padUp(p);
  ok(await until(async () => /^other/.test((await rows(p))[0] || '') && /^proj/.test((await rows(p))[1] || '')), 'there it is first, the board you came from second: ' + JSON.stringify(await rows(p)));
  ok(await until(async () => (await p.title()).startsWith('other')), 'the window is titled after it');
  ok((await p.locator('#pad .pagents button[aria-pressed="true"]').innerText()) === 'codex', 'the agent chosen on the other board is chosen here too');

  // --- renaming: double-click the name in the list
  await p.keyboard.press('Escape'); await sleep(200);
  await p.locator('#projName').click(); await sleep(250);
  await p.locator('.bdmenu .bdrow.me .bdn').dblclick();
  await p.locator('.bdmenu .bdin').fill('crawler work');
  await p.keyboard.press('Enter');
  ok(await until(async () => (await p.locator('#projName b').innerText()) === 'crawler work'), 'a board can be renamed: ' + await p.locator('#projName b').innerText());
  ok(JSON.parse(fs.readFileSync(other + '/.weft/flow.json', 'utf8')).name === 'crawler work', 'the name is kept with its cards');
  await p.keyboard.press('Escape');

  // --- back on proj: the other board sleeps, and wakes on a click
  await p.goto(url); await p.waitForSelector('#pad:not([hidden]) textarea');
  ok(await until(async () => /^crawler work/.test((await rows(p))[1] || '')), 'the other boards see the new name: ' + (await rows(p))[1]);
  weft(['stop', other]);
  ok(await until(async () => /asleep · 1 card/.test((await rows(p))[1] || '')), 'a board that was quit sleeps, its cards counted: ' + (await rows(p))[1]);
  ok(await until(async () => !(await p.locator('#bdBadge').isVisible())), 'and the badge goes: nobody waits on a sleeping board');
  await p.screenshot({ path: W + '/boards-3-asleep.png' });
  const t0 = Date.now();
  await p.locator('.pboards .bdrow.asleep').click();
  ok(await until(() => p.evaluate(() => location.port).then(x => x === String(record(other).port)), 15000), `a click wakes it and switches over (${Date.now() - t0} ms)`);
  await padUp(p);
  ok(await until(async () => (await p.locator('.nw').count()) === 1), 'with its card');

  // --- leaving a sleeping board off the list
  await p.goto(url); await p.waitForSelector('#pad:not([hidden]) textarea');
  weft(['stop', other]);
  ok(await until(async () => /asleep/.test((await rows(p))[1] || '')), 'asleep again');
  await p.keyboard.press('Escape'); await sleep(200);
  await p.keyboard.press('b'); await sleep(300);
  await p.locator('.bdmenu .bdrow.asleep').hover();
  await p.locator('.bdmenu .bdrow.asleep .bdx').click();
  ok(await until(async () => (await p.locator('.bdmenu .bdrow:not(.bdnew)').count()) === 1), 'its × leaves it off the list');
  ok(!record(other) && fs.existsSync(other + '/.weft/flow.json'), 'its record goes, its cards stay in its folder');

  ok(errs.length === 0, 'no page errors' + (errs.length ? ': ' + errs.join(' | ') : ''));
  await b.close();
  console.log(fails ? `${fails} FAILED` : 'all passed');
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
