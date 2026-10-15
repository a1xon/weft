// Checks on an agent: screenshots collect next to the card, each with approve / steer; zoom; tests.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs');
const W = process.env.SANDBOX;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const p = await (await b.newContext({ viewport: { width: 1700, height: 900 } })).newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type() === 'error' && errs.push(m.text()));
  const st = id => p.locator('.nw[data-id="' + id + '"]').getAttribute('data-st');
  const waitSt = async (id, want, t = 40000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (want.includes(await st(id))) return true; await sleep(250); } return false; };
  const waitFor = async (fn, t = 20000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (await fn()) return true; await sleep(200); } return false; };
  const cells = id => p.locator('.nw[data-id="' + id + '"] .film .fc');
  const drag = async (from, to) => {
    await p.mouse.move(from.x, from.y); await p.mouse.down(); await p.mouse.move(to.x, to.y, { steps: 10 }); await p.mouse.up();
  };

  await p.goto(fs.readFileSync(W + '/url.txt', 'utf8').trim()); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400);
  await p.keyboard.type('Fix the slugify function in textkit/slug.py so that tests/test_slug.py passes. Keep the public API.');
  await p.keyboard.press('Control+Enter');
  await p.waitForSelector('.nw[data-id="n1"]');
  ok(await waitSt('n1', ['done']), 'the agent finishes its task');

  // --- the tray says what a check is, and a screenshot dropped on an agent becomes a watch
  const tray = await p.locator('#tray .tl').innerText();
  ok(tray === 'checks', 'the tray is called "checks": ' + tray);
  const chip = await p.locator('#tray .tchip[data-k=shot]').boundingBox();
  const card = await p.locator('.nw[data-id="n1"] .hd').boundingBox();
  await p.mouse.move(chip.x + chip.width / 2, chip.y + chip.height / 2);
  await p.mouse.down();
  await p.mouse.move(card.x + 120, card.y + 40, { steps: 12 });
  await sleep(100);
  ok(await p.locator('.nw[data-id="n1"].drop.watch').count() === 1, 'the agent lights up as a drop target while dragging over it');
  ok((await p.locator('.gghost').innerText()).includes('watch'), 'the ghost says what will happen: ' + (await p.locator('.gghost').innerText()));
  await p.mouse.up();
  await p.waitForSelector('.nw[data-id="n1"] .film');
  ok(await p.locator('.nw[data-id="n1"] .film .fempty').count() === 1, 'a screenshot area appears under the card, with a hint');
  ok(await p.locator('.nw').count() === 1, 'no gate node was created: it is a watch, not a gate');
  ok(await p.locator('#edgeG .ed').count() === 0, 'and nothing was linked');
  await p.screenshot({ path: W + '/watch-1-attached.png' });

  // --- take one now: the agent is asked once, shows it with `weft shot`
  await p.locator('.nw[data-id="n1"] .film .fbtn', { hasText: 'take one' }).click();
  ok(await waitFor(async () => (await cells('n1').count()) >= 1, 30000), 'a screenshot arrives in the grid');
  ok(await waitFor(async () => (await p.locator('.nw[data-id="n1"] .film .fthumb').first().getAttribute('src')) !== null), 'with its picture');
  ok(((await p.locator('.nw[data-id="n1"] .film .fcap').first().innerText()) || '').includes('sim:'), 'and the note the agent wrote: ' + (await p.locator('.nw[data-id="n1"] .film .fcap').first().innerText()));
  ok((await st('n1')) === 'done', 'the card stays as it was: asking for a screenshot is not a new run (' + await st('n1') + ')');

  // --- a second one, so the grid is a grid
  await p.locator('.nw[data-id="n1"] .film .fbtn', { hasText: 'take one' }).click();
  ok(await waitFor(async () => (await cells('n1').count()) >= 2, 30000), 'a second screenshot joins the first');
  await p.screenshot({ path: W + '/watch-2-grid.png' });
  const g = await p.locator('.nw[data-id="n1"] .film .fgrid').boundingBox();
  const cardBox = await p.locator('.nw[data-id="n1"] .node').boundingBox();
  ok(g.y >= cardBox.y + cardBox.height && g.width <= cardBox.width + 4, 'the grid sits under the card and is no wider than it (' + Math.round(g.width) + 'px)');

  // --- steer one: type, enter; the agent hears it
  const first = cells('n1').first();
  await first.locator('input').fill('the header is cut off, fix that');
  ok((await first.locator('.fin button').innerText()) === '↵', 'the button turns into "send" while a note is typed');
  await first.locator('input').press('Enter');
  ok(await waitFor(async () => (await cells('n1').first().getAttribute('class')).includes('steered')), 'the item is marked as sent');
  ok((await cells('n1').first().locator('.fcap').innerText()).includes('the header is cut off'), 'and shows what was said');
  await sleep(2500);
  const got = fs.readFileSync(W + '/proj/.weft/run/received-n1.log', 'utf8');
  ok(got.includes('the header is cut off, fix that') && got.includes('Please adjust that and carry on'), 'the agent received the note, right away, in its own session');

  // --- approve the other one: one click
  const second = cells('n1').nth(1);
  await second.locator('.fin button').click();
  ok(await waitFor(async () => (await cells('n1').nth(1).getAttribute('class')).includes('ok')), 'a click on the check approves it');

  // --- zoom
  await cells('n1').first().locator('.fimg').click();
  await p.waitForSelector('#shotbox:not([hidden]) img.sbimg');
  ok(true, 'clicking a thumbnail zooms it');
  const sb = await p.locator('#shotbox .sbimg').boundingBox();
  const th = await cells('n1').first().locator('.fthumb').boundingBox();
  ok(sb.width > th.width * 2, 'the zoomed picture is much larger (' + Math.round(sb.width) + ' vs ' + Math.round(th.width) + ' px)');
  ok(await p.locator('#shotbox input').evaluate(e => document.activeElement === e), 'its steering field has the keyboard');
  const counter = await p.locator('#shotbox .sbh span').nth(1).innerText();
  const [ci, cn] = counter.split(' / ');
  ok(ci === cn && Number(cn) >= 2, 'it opens on the newest of the series: ' + counter);
  await p.keyboard.press('Control+a'); // focus stays in the field; arrows step through the series
  await p.locator('#shotbox .sbnav').first().click();
  ok((await p.locator('#shotbox .sbh span').nth(1).innerText()).startsWith(String(Number(cn) - 1)), 'the arrows step to the previous one');
  await p.screenshot({ path: W + '/watch-3-zoom.png' });
  await p.keyboard.press('Escape');
  ok(await p.locator('#shotbox').isHidden(), 'escape closes the zoom');

  // --- pasting an image onto a card with a screenshot watch
  const n0 = await cells('n1').count();
  const pngB64 = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==';
  await p.locator('.nw[data-id="n1"] .hd').click();
  await p.evaluate(async b64 => {
    const bin = atob(b64); const u8 = Uint8Array.from(bin, c => c.charCodeAt(0));
    const f = new File([u8], 'pasted.png', { type: 'image/png' });
    const dt = new DataTransfer(); dt.items.add(f);
    document.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true }));
  }, pngB64);
  ok(await waitFor(async () => (await cells('n1').count()) === n0 + 1), 'a pasted image joins the grid');

  // --- tests watch: a command, run on demand, red/green with the steering field
  await p.evaluate(() => send('check_add', { node: 'n1', kind: 'tests', subject: '', cmd: 'echo "Ran 2 tests"; echo FAIL: test_wrap; exit 1', every_s: 0 }));
  await p.waitForSelector('.nw[data-id="n1"] .film .fbtn:has-text("run tests")');
  await p.locator('.nw[data-id="n1"] .film .fbtn', { hasText: 'run tests' }).click();
  ok(await waitFor(async () => (await p.locator('.nw[data-id="n1"] .film .ftxt').count()) >= 1, 20000), 'a test run shows as a result card');
  const tcell = p.locator('.nw[data-id="n1"] .film .ftxt.bad').first();
  ok((await tcell.innerText()).includes('failed') && (await tcell.innerText()).includes('FAIL: test_wrap'), 'a red run shows its output');

  // --- dropped on a link it is a gate again
  const n2 = await p.evaluate(async () => { const r = await req('node_add', { node: { agent: 'claude', x: 900, y: 120, prompt: 'Summarize.', draft: true } }); return r.id; });
  await sleep(500);
  await p.evaluate(id => send('edge_add', { from: 'n1', to: id }), n2);
  await sleep(500);
  const mid = await p.evaluate(() => { const e = [...document.querySelectorAll('#edgeG .ed')][0]; const c = e.querySelector('.chip').getBoundingClientRect(); return { x: c.x + c.width / 2 + 24, y: c.y + c.height / 2 }; });
  const chip2 = await p.locator('#tray .tchip[data-k=gate]').boundingBox();
  const nodesBefore = await p.locator('.nw').count();
  await drag({ x: chip2.x + chip2.width / 2, y: chip2.y + chip2.height / 2 }, mid);
  await sleep(700);
  ok((await p.locator('.nw').count()) === nodesBefore + 1 && await p.locator('.nw[data-kind=gate]').count() === 1, 'the same chip dropped on a link becomes a gate');

  ok(errs.length === 0, 'no page errors' + (errs.length ? ': ' + errs.slice(0, 3).join(' | ') : ''));
  await b.close();
  console.log(fails ? fails + ' FAILED' : 'WATCH OK');
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); process.exit(2); });
