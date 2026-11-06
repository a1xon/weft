// Whose the uncommitted files are: two agents at work at the same time each change a file and say so
// in their reports, so each card counts its own file, not the other's (a notch under the card's bottom
// edge, what unfolds under the card below it); a file no card changed is
// "with no card" on the git tree's uncommitted row, whose click lists the files by card; a file's
// click shows its diff; "commit with an agent" starts a card told exactly those files; a card's count
// opens the list at its files; a commit takes a file off its card.
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
  const PROJ = W + '/proj';
  const git = (...a) => execFileSync('git', ['-C', PROJ, ...a], { encoding: 'utf8' });
  const chip = id => p.locator('.nw[data-id="' + id + '"] .ucn');
  const chipText = async id => (await chip(id).count()) && await chip(id).isVisible() ? (await chip(id).innerText()).replace(/\s+/g, ' ').trim() : '';
  const workRow = p.locator('#gtree .gr.work');

  await p.goto(fs.readFileSync(W + '/url.txt', 'utf8').trim()); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400);
  await p.keyboard.press('Escape'); await sleep(200);

  // --- two agents at work at the same time, one a while longer: each card's snapshots see both files
  const a = await add({ agent: 'claude', name: 'slugger', prompt: 'Fix the slugify function in textkit/slug.py. LONG=9', x: 120, y: 80 });
  const c = await add({ agent: 'codex', name: 'wrapper', prompt: 'Fix the wrap function in textkit/wrap.py.', x: 120, y: 480 });
  await p.evaluate(id => send('arm', { id }), a);
  await sleep(1500);
  await p.evaluate(id => send('arm', { id }), c);
  ok(await waitSt(c, ['done'], 90000), 'the short one finishes (' + await st(c) + ')');
  ok(await waitSt(a, ['done'], 90000), 'the long one finishes (' + await st(a) + ')');
  const files = id => p.evaluate(i => (OWN.cards.get(i) || { files: [] }).files.map(f => f.p + (f.unsure ? '?' : '')).sort().join(','), id);
  ok(await waitFor(async () => (await files(a)) === 'textkit/slug.py' && (await files(c)) === 'textkit/wrap.py', 20000),
    `each card has the file it changed and said so, not the other's (${await files(a)} | ${await files(c)})`);
  ok(await waitFor(async () => (await chipText(a)) === '1 uncommitted' && (await chipText(c)) === '1 uncommitted', 8000),
    `the cards say so: "${await chipText(a)}", "${await chipText(c)}"`);
  ok(/textkit\/slug\.py/.test(await chip(a).getAttribute('title') || ''), 'the count names its file when hovered');
  const edge = await p.evaluate(id => { const r = nodeEls.get(id).root.querySelector('.node').getBoundingClientRect(), u = nodeEls.get(id).uc.getBoundingClientRect();
    return { top: u.top - r.bottom, inside: u.left > r.left && u.right < r.right, h: u.height }; }, a);
  ok(Math.abs(edge.top) <= 2 && edge.inside && edge.h > 10, 'it is a notch hanging from the card\'s bottom edge, within its width (' + JSON.stringify(edge) + ')');
  // something that unfolds under the card (a question) starts under the notch
  const under = await p.evaluate(id => { const card = nodeEls.get(id); underOf(card).append(h('div', { class: 'plea', id: 'probe' }, 'a question'));
    const u = nodeEls.get(id).uc.getBoundingClientRect(), q = document.getElementById('probe').getBoundingClientRect(); const gap = q.top - u.bottom;
    document.getElementById('probe').remove(); tidyUnder(card); return gap; }, a);
  ok(under >= 4, 'what unfolds under the card starts below the notch (' + under + 'px)');

  // --- a file no card changed
  fs.mkdirSync(PROJ + '/notes', { recursive: true });
  fs.writeFileSync(PROJ + '/notes/leftover.md', '# left over\n\nfrom an earlier session\n');
  await p.evaluate(() => send('proj'));
  ok(await waitFor(async () => (await workRow.count()) === 1 && /3 uncommitted/.test(await workRow.innerText()) && /1 with no card/.test(await workRow.innerText()), 20000),
    'the tree\'s uncommitted row says how many no card has: "' + ((await workRow.count()) ? (await workRow.innerText()).replace(/\s+/g, ' ') : '') + '"');
  ok((await chipText(a)) === '1 uncommitted', 'and the cards keep their own count');

  // --- the list, by card
  await workRow.locator('.gline').click();
  ok(await waitFor(async () => (await p.locator('#gtree .gwork .gwg').count()) === 3, 5000), 'a click lists the files in three groups (' + await p.locator('#gtree .gwork .gwg').count() + ')');
  const left = p.locator('#gtree .gwg.left');
  ok(/no card/i.test(await left.innerText()) && /notes\/leftover\.md/.test(await left.innerText()), 'what no card has comes first: "' + (await left.innerText()).replace(/\s+/g, ' ') + '"');
  ok(/slugger/.test(await p.locator('#gtree .gwg').nth(1).innerText()) || /slugger/.test(await p.locator('#gtree .gwg').nth(2).innerText()), 'each card\'s group is named after the card');
  await p.screenshot({ path: W + '/whose-1-list.png' });

  // --- a file's diff
  await left.locator('.gwf').first().click();
  ok(await waitFor(async () => (await p.locator('#reader .dv .da').count()) > 0, 10000), 'a click on a file shows what is uncommitted in it');
  ok(/leftover\.md/.test(await p.locator('#reader .fh2').innerText()) && /no card/.test(await p.locator('#reader .fh2').innerText()), 'named, with whose it is');
  await p.keyboard.press('Escape'); await sleep(200);
  if (!(await p.locator('#reader').isHidden())) await p.locator('#reader button', { hasText: 'back to canvas' }).click();

  // --- commit with an agent: a card told exactly those files, and to stage only them
  const before = await p.evaluate(() => [...S.nodes.keys()]);
  await p.locator('#gtree .gwgo').click();
  const fresh = await waitFor(async () => (await p.evaluate(b => [...S.nodes.keys()].filter(k => !b.includes(k)).length, before)) === 1, 10000);
  ok(fresh, 'a click on "commit with an agent" starts one card');
  const nid = await p.evaluate(b => [...S.nodes.keys()].find(k => !b.includes(k)), before);
  const prompt = nid ? await p.evaluate(i => S.nodes.get(i).prompt, nid) : '';
  ok(/notes\/leftover\.md/.test(prompt) && !/textkit\/slug\.py/.test(prompt), 'it is told the file no card has, and not the cards\' files');
  ok(/never `git add -A`/.test(prompt) && /Do not delete, revert/.test(prompt), 'and to stage only those, by name, and to delete nothing');
  ok(nid && await p.evaluate(i => S.sel.has(i), nid), 'it is selected');
  await p.locator('#gtree .gwgo').click();
  await sleep(1500);
  ok((await p.evaluate(b => [...S.nodes.keys()].filter(k => !b.includes(k)).length, before)) === 1, 'a second click while it works starts no second one');
  if (nid) { await p.evaluate(i => send('stop', { id: i }), nid); await sleep(800); await p.evaluate(i => send('node_del', { id: i }), nid); }

  // --- a card's count opens the list at its files
  await workRow.locator('.gline').click();   // fold it
  await sleep(300);
  await p.evaluate(i => { select([]); reveal(i); }, c); await sleep(700);
  await chip(c).click();
  ok(await waitFor(async () => (await p.locator('#gtree .gwg.focus').count()) === 1, 6000), 'a click on a card\'s count unfolds the list at its group');
  ok(/wrapper/.test(await p.locator('#gtree .gwg.focus').innerText()), 'that card\'s');
  ok(await p.evaluate(i => S.sel.has(i) && S.sel.size === 1, c), 'and selects the card');

  // --- committed: no one's any more
  git('add', '--', 'textkit/slug.py'); git('commit', '-qm', 'slug fixed');
  await p.evaluate(() => send('proj'));
  ok(await waitFor(async () => (await chipText(a)) === '', 20000), 'a commit takes the file off its card (' + (await chipText(a) || 'no count') + ')');
  ok((await chipText(c)) === '1 uncommitted', 'the other card keeps its own');
  await p.screenshot({ path: W + '/whose-2-after.png' });

  ok(!errs.length, 'no page errors' + (errs.length ? ': ' + errs.join(' | ') : ''));
  await b.close();
  console.log(fails ? `${fails} FAILED` : 'all passed');
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
