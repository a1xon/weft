// dev: the page follows edits to its folder (src/dev.rs, web/dev.js). Not part of weft proper.
// run.sh serves a copy of web/ from the sandbox; this edits that copy the way a person or an agent would.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs'); const W = process.env.SANDBOX, WEB = process.env.WEFT_WEB_DIR;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
const append = (f, text) => fs.appendFileSync(WEB + '/' + f, text);
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const p = await (await b.newContext({ viewport: { width: 1500, height: 880 } })).newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message));
  const waitFor = async (fn, t = 10000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { try { if (await fn()) return Date.now() - t0; } catch {} await sleep(50); } return false; };
  const ev = f => p.evaluate(f);
  const original = { boot: fs.readFileSync(WEB + '/js/99-boot.js', 'utf8'), links: fs.readFileSync(WEB + '/js/50-links.js', 'utf8') };

  await p.goto(fs.readFileSync(W + '/url.txt', 'utf8').trim()); await p.waitForSelector('#pad:not([hidden]) textarea'); await sleep(400);
  await p.keyboard.press('Escape'); await sleep(200);
  ok(await p.locator('#devtag').count() === 0, 'no tag while the page is up to date (the corner is the git tree\'s)');
  await ev(() => send('node_add', { node: { name: 'probe', agent: 'claude', prompt: 'say hi', x: 200, y: 200 } }));
  await p.waitForSelector('.nw[data-id="n1"]');
  await ev(() => { window.__same = 1; });

  // --- a style sheet: swapped in place, nothing reloads
  append('css/96-quit.css', '\n#top{outline:3px solid rgb(1, 2, 3)}\n');
  const t1 = await waitFor(() => ev(() => getComputedStyle(document.querySelector('#top')).outlineColor === 'rgb(1, 2, 3)'));
  ok(t1 !== false, `an edited style sheet shows without a reload (${t1} ms)`);
  ok(await ev(() => window.__same === 1), 'the page was not reloaded for it');

  // --- a script: the page reloads and puts back what was open
  await ev(() => { select(['n1']); openFly('n1'); });
  await sleep(200);
  ok(await p.locator('#fly').isVisible(), '(a flyout is open on the card)');
  append('js/99-boot.js', '\nwindow.__probe = 42;\n');
  const t2 = await waitFor(() => ev(() => window.__probe === 42));
  ok(t2 !== false, `an edited script reloads the page (${t2} ms)`);
  ok(await ev(() => window.__same === undefined), 'it is a new page');
  await waitFor(() => p.locator('#fly').isVisible(), 3000);
  ok(await p.locator('#pad').isHidden(), 'the launch pad does not jump up after a reload you did not ask for');
  ok(await ev(() => S.sel.has('n1')) && await p.locator('#fly').isVisible(), 'the selection and the open flyout come back');
  ok((await p.locator('#top').evaluate(e => getComputedStyle(e).outlineColor)) === 'rgb(1, 2, 3)', 'and the style edit is part of the new page');

  // --- not while you type: the reload waits until your text is gone
  await p.locator('#omni textarea').click();
  await p.keyboard.type('a draft I have not sent');
  append('js/99-boot.js', '\nwindow.__probe = 43;\n');
  await sleep(1500);
  ok(await ev(() => window.__probe === 42), 'a script edit does not reload the page under your fingers');
  ok(await p.locator('#devtag').count() === 1 && /reloads when you are done/.test(await p.locator('#devtag').innerText()), 'a tag says it is waiting');
  await p.keyboard.press('Control+a'); await p.keyboard.press('Backspace');
  const t3 = await waitFor(() => ev(() => window.__probe === 43), 5000);
  ok(t3 !== false, `once the text is gone it reloads (${t3} ms)`);

  // --- a broken script: the bar names the file and line; the fix reloads by itself
  await ev(() => { select(['n1']); openFly('n1'); });
  await sleep(200);
  const lines = original.links.split('\n');
  const at = lines.length + 1;
  fs.writeFileSync(WEB + '/js/50-links.js', original.links + '\nconst broken = ;\n');
  ok(await waitFor(() => p.locator('#deverr').isVisible(), 8000) !== false, 'a script that does not parse shows the error bar');
  const bar = await p.locator('#deverr').innerText().catch(() => '');
  ok(bar.includes('js/50-links.js:' + at), `it names the file and the line: ${bar.replace(/\s+/g, ' ').slice(0, 120)} (want js/50-links.js:${at})`);
  fs.writeFileSync(WEB + '/js/50-links.js', original.links);
  ok(await waitFor(() => ev(() => typeof loaded !== 'undefined' && loaded && S.nodes.has('n1')), 8000) !== false, 'fixing it reloads a working page');
  ok(await p.locator('#deverr').count() === 0, 'without the bar');
  await waitFor(() => p.locator('#fly').isVisible(), 3000);
  ok(await p.locator('#fly').isVisible() && await p.locator('#pad').isHidden(), 'what was open before it broke comes back');

  // --- an error at run time: same, with the line inside its file
  append('js/99-boot.js', '\nsetTimeout(function devBoom() { notDefinedAnywhere.x; }, 200);\n');
  const want = fs.readFileSync(WEB + '/js/99-boot.js', 'utf8').split('\n').findIndex(l => l.includes('devBoom')) + 1;
  ok(await waitFor(() => p.locator('#deverr').isVisible(), 8000) !== false, 'an error at run time shows the bar');
  const bar2 = await p.locator('#deverr').innerText().catch(() => '');
  ok(bar2.includes('js/99-boot.js:' + want) && bar2.includes('notDefinedAnywhere'), `with file, line and message: ${bar2.replace(/\s+/g, ' ').slice(0, 140)}`);
  await p.locator('#deverr button').click();
  ok(await p.locator('#deverr').count() === 0, 'and it can be dismissed');
  fs.writeFileSync(WEB + '/js/99-boot.js', original.boot);
  await waitFor(() => ev(() => window.__probe === undefined && loaded), 8000);

  // --- a new file is part of the next page
  fs.writeFileSync(WEB + '/js/99-zz-dev-probe.js', 'window.__fresh = "yes";\n');
  ok(await waitFor(() => ev(() => window.__fresh === 'yes'), 8000) !== false, 'a script file added while working is loaded');
  fs.unlinkSync(WEB + '/js/99-zz-dev-probe.js');
  await waitFor(() => ev(() => window.__fresh === undefined && loaded), 8000);

  const own = errs.filter(e => !/notDefinedAnywhere|Unexpected token|broken/.test(e));
  ok(own.length === 0, 'no other page errors: ' + own.join(' | ').slice(0, 300));
  await p.screenshot({ path: W + '/dev-1.png' });
  await b.close();
  console.log(fails ? `${fails} FAILED` : 'all passed');
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
