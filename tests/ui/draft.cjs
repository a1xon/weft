// What is typed in the bar or on the pad and not sent yet survives a reload (a new build, say): it
// is kept per board in this browser; a sent prompt is not. ↑/↓ go through the prompts sent before
// (as in a shell), and ctrl+z brings back a prompt that was sent or cleared.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs');
const W = process.env.SANDBOX;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const p = await (await b.newContext({ viewport: { width: 1600, height: 900 } })).newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type() === 'error' && errs.push(m.text()));
  const waitFor = async (fn, t = 15000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (await fn()) return true; await sleep(200); } return false; };
  const bar = () => p.locator('#omni textarea').inputValue();
  const pad = () => p.locator('#pad textarea').inputValue();
  const reload = async sel => { await p.reload(); await p.waitForSelector(sel); await p.waitForFunction(() => loaded); await sleep(300); };

  await p.goto(fs.readFileSync(W + '/url.txt', 'utf8').trim()); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400);
  // --- the pad
  await p.keyboard.type('A long prompt on the pad,\nwith a second line.');
  await reload('#pad:not([hidden]) textarea');
  ok((await pad()) === 'A long prompt on the pad,\nwith a second line.', 'the pad has its text back after a reload: ' + JSON.stringify(await pad()));
  await p.locator('#pad textarea').focus(); await p.keyboard.press('Control+Enter');
  await p.waitForSelector('.nw[data-id="n1"]');
  await reload('.nw[data-id="n1"]');
  ok(await p.evaluate(() => !localStorage.getItem('weft-draft:pad:' + S.project.dir)), 'a prompt that was started is not kept');

  // --- the bar
  await p.evaluate(() => select([]));
  await p.locator('#omni textarea').click();
  await p.keyboard.type('Half a thought for the bar');
  await reload('#omni textarea');
  ok((await bar()) === 'Half a thought for the bar', 'the bar has its text back after a reload: ' + JSON.stringify(await bar()));
  ok(await p.evaluate(() => omniTa.offsetHeight > 0), 'and it is shown');
  // a reload because the daemon is another build (the one that took the text before)
  await p.locator('#omni textarea').click(); await p.keyboard.press('End'); await p.keyboard.type(', and more');
  await p.evaluate(() => { document.querySelector('meta[name=weft-build]').content = 'other'; ws.close(); });
  await sleep(1500); await p.waitForSelector('#omni textarea'); await p.waitForFunction(() => loaded); await sleep(300);
  ok((await bar()) === 'Half a thought for the bar, and more', 'after a reload for a new build too: ' + JSON.stringify(await bar()));
  // it belongs to this board only
  ok(await p.evaluate(() => Object.keys(localStorage).filter(k => k.startsWith('weft-draft:bar:')).join() === 'weft-draft:bar:' + S.project.dir), 'kept under this board');
  // sent: gone, after a reload as well
  await p.locator('#omni textarea').focus(); await p.keyboard.press('Control+Enter');
  ok(await waitFor(async () => (await p.evaluate(() => S.nodes.size)) === 2), 'the bar starts its agent');
  ok((await bar()) === '', 'and is empty');
  await reload('#omni textarea');
  ok((await bar()) === '', 'it stays empty after a reload');

  // --- ctrl+z: the prompt just sent comes back, so does one cleared with Esc
  await p.locator('#omni textarea').focus(); await p.keyboard.press('Control+z');
  ok((await bar()) === '', 'after a reload there is nothing to undo');
  await p.keyboard.type('a second thought');
  await p.keyboard.press('Escape'); ok((await bar()) === '', 'Esc clears the bar');
  await p.locator('#omni textarea').focus(); await p.keyboard.press('Control+z');
  ok((await bar()) === 'a second thought', 'ctrl+z brings it back: ' + JSON.stringify(await bar()));
  ok(await p.evaluate(() => S.nodes.size === 2), 'and undoes nothing on the canvas');
  await p.keyboard.press('Control+Enter');
  ok(await waitFor(async () => (await p.evaluate(() => S.nodes.size)) === 3), 'sent: a third agent');
  await p.keyboard.press('Control+z');
  ok((await bar()) === 'a second thought', 'ctrl+z after sending brings the sent prompt back: ' + JSON.stringify(await bar()));
  await p.keyboard.press('Control+Shift+z'); ok(await waitFor(async () => (await bar()) === '', 3000), 'and ctrl+shift+z takes it away again: ' + JSON.stringify(await bar()));

  // --- ↑ / ↓: the prompts sent before, newest first, the pad's too; what was typed waits at the bottom
  await p.keyboard.type('typed, not sent\nsecond line');
  await p.keyboard.press('ArrowUp');
  ok((await bar()) === 'typed, not sent\nsecond line', '↑ below the first line moves the caret, as always');
  await p.keyboard.press('ArrowUp');
  ok((await bar()) === 'a second thought', '↑ on the first line brings back the last prompt sent: ' + JSON.stringify(await bar()));
  await p.keyboard.press('ArrowUp');
  ok((await bar()) === 'Half a thought for the bar, and more', 'and the one before: ' + JSON.stringify(await bar()));
  await p.keyboard.press('ArrowUp');
  ok((await bar()) === 'A long prompt on the pad,\nwith a second line.', 'the pad\'s prompt is among them: ' + JSON.stringify(await bar()));
  await p.keyboard.press('ArrowUp');
  ok((await bar()) === 'A long prompt on the pad,\nwith a second line.', 'the oldest one stays');
  for (let i = 0; i < 3; i++) await p.keyboard.press('ArrowDown');   // (the caret goes to its last line first)
  ok((await bar()) === 'a second thought', '↓ goes forward again: ' + JSON.stringify(await bar()));
  await p.keyboard.press('ArrowDown');
  ok((await bar()) === 'typed, not sent\nsecond line', 'past the newest, the text that was being typed is back: ' + JSON.stringify(await bar()));
  await p.keyboard.press('ArrowUp'); await p.keyboard.press('ArrowUp');
  await p.keyboard.press('Control+z');
  ok((await bar()) === 'typed, not sent\nsecond line', 'ctrl+z puts back what a recalled prompt replaced: ' + JSON.stringify(await bar()));
  await reload('#omni textarea');
  ok(await p.evaluate(() => JSON.parse(localStorage.getItem('weft-sent:' + S.project.dir)).length === 3), 'the sent ones are kept per board, each once');

  ok(!errs.length, 'no page errors' + (errs.length ? ': ' + errs.join(' | ') : ''));
  await b.close();
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
