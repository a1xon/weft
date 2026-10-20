// weft marks: the browser extension in a real Chromium (headless, extensions on) reaching the
// daemon through native messaging. A dev page served from the board's folder is marked: the card
// lands on that board, started, with the note, the element, its component, the page's console
// errors and failed requests, and the screenshots saved; the next mark on the page goes to the
// same agent. A PDF in the board's folder opens in weft's own viewer, where a paragraph is marked:
// the card says the file, the page and the text. Needs a full Chromium (chrome-headless-shell has no extensions): CHROME, else the one
// Playwright keeps beside the headless shell.
const { chromium } = require(process.env.PLAYWRIGHT);
const { spawn, spawnSync } = require('child_process');
const fs = require('fs');
const path = require('path');
const W = process.env.SANDBOX, BIN = process.env.WEFT_UI_BIN;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
const ID = 'jbabimhfbehpngcbcpdllclojiebcomn';

function fullChrome() {
  if (process.env.CHROME) return process.env.CHROME;
  const cache = path.join(process.env.HOME, '.cache/ms-playwright');
  const dirs = fs.existsSync(cache) ? fs.readdirSync(cache).filter(d => /^chromium-\d+$/.test(d)).sort().reverse() : [];
  for (const d of dirs) { const p = path.join(cache, d, 'chrome-linux64/chrome'); if (fs.existsSync(p)) return p; }
  return '';
}
const flow = () => { try { return JSON.parse(fs.readFileSync(W + '/proj/.weft/flow.json', 'utf8')); } catch { return { nodes: [] }; } };
// what the simulated agent was sent (it logs every prompt)
const received = id => { try { return fs.readFileSync(`${W}/proj/.weft/run/received-${id}.log`, 'utf8'); } catch { return ''; } };

(async () => {
  const chrome = fullChrome();
  if (!chrome) { console.log('SKIP marks: no full Chromium (set CHROME)'); return; }

  // --- `weft extension`: the files, and the host told to every browser found
  fs.mkdirSync(W + '/cfg/BraveSoftware/Brave-Browser', { recursive: true });
  const env = { ...process.env, XDG_DATA_HOME: W + '/data' };
  const inst = spawnSync(BIN, ['extension'], { encoding: 'utf8', env });
  const ext = W + '/data/weft/extension';
  ok(inst.status === 0 && /connected to Brave/.test(inst.stdout), 'weft extension connects Brave: ' + inst.stdout.split('\n')[0]);
  const host = JSON.parse(fs.readFileSync(W + '/cfg/BraveSoftware/Brave-Browser/NativeMessagingHosts/weft.browser.json', 'utf8'));
  ok(host.path === fs.realpathSync(BIN) && host.allowed_origins[0] === `chrome-extension://${ID}/`, 'the host is this weft, for this extension only');
  ok(['manifest.json', 'background.js', 'mark.js', 'probe.js', 'icons/128.png'].every(f => fs.existsSync(ext + '/' + f)), 'the extension files are in place');
  // the test browser: Brave finds the host where `weft extension` told it; Chrome for Testing looks
  // in its profile. And (only here: no toolbar click in a headless browser) the page access a click
  // on the weft button would grant.
  for (const d of /brave/.test(chrome) ? [] : [W + '/profile']) {
    fs.mkdirSync(d + '/NativeMessagingHosts', { recursive: true });
    fs.writeFileSync(d + '/NativeMessagingHosts/weft.browser.json', JSON.stringify(host));
  }
  const extT = W + '/ext-test';
  fs.cpSync(ext, extT, { recursive: true });
  const man = JSON.parse(fs.readFileSync(extT + '/manifest.json', 'utf8'));
  man.host_permissions = ['<all_urls>'];
  fs.writeFileSync(extT + '/manifest.json', JSON.stringify(man));

  // --- a dev server in the board's folder, its page made by "React" (a fiber as React dev builds leave it)
  fs.mkdirSync(W + '/proj/web', { recursive: true });
  fs.writeFileSync(W + '/proj/web/index.html', `<!doctype html><meta charset="utf-8"><title>Pricing · Shop</title>
<style>body{font:16px sans-serif;margin:40px;background:#fff} .hero{padding:30px;background:#eef} .cta{padding:12px 24px;font-size:22px}</style>
<main><section class="hero"><h1>Plans</h1><div class="actions"><button id="buy" class="cta primary">Get started</button><button class="cta">Later</button></div></section>
<p id="fine">Prices without tax.</p></main>
<script>
  const hero = { type: function Hero() {}, _debugSource: { fileName: 'src/Hero.jsx', lineNumber: 4 },
                 return: { type: function App() {}, _debugSource: { fileName: 'src/main.jsx', lineNumber: 9 }, return: null } };
  document.querySelector('#buy')['__reactFiber$x1'] = { type: 'button', _debugSource: { fileName: 'src/Hero.jsx', lineNumber: 12 }, return: hero };
  console.error('Warning: Each child in a list should have a unique %s prop.', 'key');
  fetch('/api/prices').catch(() => {});
</script>`);
  // (MARKS_OLD_DAEMON=<an older weft>: the board runs a daemon that does not know marks yet; the
  // first mark has it replaced by this build, started from the host the browser ran)
  if (process.env.MARKS_OLD_DAEMON) {
    spawnSync(BIN, ['stop', W + '/proj'], { env: process.env });
    await sleep(500);
    spawnSync(process.env.MARKS_OLD_DAEMON, ['open', W + '/proj', '--no-open'], { env: process.env, encoding: 'utf8' });
    const rec = fs.readdirSync(W + '/state/weft').find(f => /^[0-9a-f]{8}\.json$/.test(f));
    ok(!!rec, 'an older daemon runs the board: ' + (spawnSync(process.env.MARKS_OLD_DAEMON, ['--version'], { encoding: 'utf8' }).stdout || '').trim());
  }
  const port = 20000 + Math.floor(Math.random() * 20000);
  const srv = spawn('python3', ['-m', 'http.server', String(port), '--bind', '127.0.0.1'], { cwd: W + '/proj/web', stdio: 'ignore' });
  await sleep(800);

  const ctx = await chromium.launchPersistentContext(W + '/profile', {
    executablePath: chrome, headless: true, viewport: { width: 1280, height: 800 },
    args: [`--disable-extensions-except=${extT}`, `--load-extension=${extT}`],
  });
  try {
    let sw = ctx.serviceWorkers()[0] || await ctx.waitForEvent('serviceworker', { timeout: 10000 });
    ok(sw.url().startsWith(`chrome-extension://${ID}/`), 'the extension loads with its fixed id');
    // the board's canvas open in a tab behind
    const canvas = await ctx.newPage();
    await canvas.goto(fs.readFileSync(W + '/url.txt', 'utf8').trim());
    await canvas.waitForSelector('#pad:not([hidden]) textarea'); await sleep(300);
    await canvas.keyboard.press('Escape');
    const p = await ctx.newPage();
    const perrs = []; p.on('pageerror', e => perrs.push(e.message));
    await p.goto(`http://127.0.0.1:${port}/`);
    await sleep(500);
    const pick = () => sw.evaluate(async () => { const [t] = await chrome.tabs.query({ active: true, lastFocusedWindow: true }); await start(t); });

    // --- the weft button: hover shows what it is, a click marks it
    await pick(); await sleep(300);
    const buy = await p.locator('#buy').boundingBox();
    await p.mouse.move(buy.x + 20, buy.y + 10); await sleep(150);
    await p.mouse.move(buy.x + 30, buy.y + 12); await sleep(200);
    await p.screenshot({ path: W + '/marks-1-hover.png' });
    let clicked = false; await p.exposeFunction('pageSawClick', () => { clicked = true; });
    await p.evaluate(() => document.querySelector('#buy').addEventListener('click', () => window.pageSawClick()));
    await p.mouse.click(buy.x + 30, buy.y + 12);
    await sleep(1500);   // screenshot + the board list from weft
    ok(!clicked, 'the click marks the button; the page never sees it');
    await p.keyboard.type('The button is too big; make it the size of the "Later" button.');
    await sleep(200);
    await p.screenshot({ path: W + '/marks-2-box.png' });
    ok(await p.evaluate(() => document.activeElement && document.activeElement.localName === 'weft-mark'), 'the note box has the keyboard');
    await p.keyboard.press('Control+Enter');

    // --- a card on the board, started, with everything the agent needs
    let card = null;
    for (let i = 0; i < 50 && !card; i++) { await sleep(200); card = flow().nodes.find(n => /too big/.test(n.prompt || '')); }
    ok(!!card, 'a card is on the board: ' + (card ? card.id + ' ' + card.name : 'none'));
    if (card) {
      const t = card.prompt;
      ok(t.startsWith('The button is too big'), 'the note comes first');
      ok(t.includes(`Page: http://127.0.0.1:${port}/ "Pricing · Shop"`), 'with the page and its title');
      ok(t.includes('### button#buy.cta.primary'), 'the element');
      ok(t.includes('Written in: src/Hero.jsx:12 (react)'), 'where it is written');
      ok(t.includes('<Hero> src/Hero.jsx:4 ← <App> src/main.jsx:9'), 'the components around it');
      ok(/Selector: #buy\n/.test(t) && t.includes('Text: "Get started"'), 'its selector and text');
      ok(/Box: \d+×\d+ at \d+,\d+ in the viewport · display:inline-block; padding:12px 24px; font:22px/.test(t), 'its box and styles');
      ok(t.includes('<button id="buy" class="cta primary">Get started</button>'), 'its html');
      ok(/- error: Warning: Each child in a list should have a unique key prop\./.test(t), 'the console error (formatted)');
      ok(/- 404 fetch http:\/\/127\.0\.0\.1:\d+\/api\/prices/.test(t), 'the failed request');
      const shots = [...t.matchAll(/(\/\S+\.png) \(/g)].map(m => m[1]);
      ok(shots.length === 2 && shots.every(f => f.startsWith(W + '/proj/.weft/shots/') && fs.existsSync(f)), 'two screenshots saved in the board: ' + shots.map(f => path.basename(f)).join(', '));
      const png = shots[0] && fs.readFileSync(shots[0]);
      ok(png && png.readUInt32BE(16) > 100 && png.readUInt32BE(16) < 600, 'the close-up is about the button (' + (png && png.readUInt32BE(16)) + ' px wide)');
      ok(!card.draft, 'it was started');
      ok(card.cwd === '', 'it works in the board folder (the server runs in its repository)');
      // the agent got it
      let got = false;
      for (let i = 0; i < 60 && !got; i++) { await sleep(300); got = /too big[\s\S]*Written in: src\/Hero\.jsx:12/.test(received(card.id)); }
      ok(got, 'the agent was given the prompt');
    }
    await p.screenshot({ path: W + '/marks-3-sent.png' });
    if (card) {
      // (a canvas whose daemon was just replaced reloads first; a page from a build before that said
      // goodbye instead, and the person reloads it once)
      if (process.env.MARKS_OLD_DAEMON) { await sleep(1500); await canvas.reload(); }
      let seen = '';
      for (let i = 0; i < 40 && seen !== 'in view'; i++) {
        seen = await canvas.evaluate(id => {
          const el = document.querySelector('.nw[data-id="' + id + '"]'); if (!el) return 'no card';
          const r = el.getBoundingClientRect();
          return r.right > 0 && r.bottom > 0 && r.left < innerWidth && r.top < innerHeight ? 'in view' : 'off screen';
        }, card.id).catch(() => 'reloading');
        if (seen !== 'in view') await sleep(250);
      }
      ok(seen === 'in view', 'the canvas behind shows the new card (' + seen + ')');
    }

    // --- what the probe reads in the page's own world, for the shapes other frameworks leave
    const probed = await p.evaluate(port => {
      const mk = (html, setup) => { const d = document.createElement('div'); d.innerHTML = html; document.body.appendChild(d); const el = d.firstElementChild; setup(el); return el; };
      const ask = el => { let got = null; const h = e => { got = JSON.parse(e.detail); }; document.addEventListener('weft:probed', h, { once: true }); el.dispatchEvent(new CustomEvent('weft:probe')); return got; };
      const out = {};
      // React 19: no _debugSource, a stack captured where the JSX was written
      out.react19 = ask(mk('<a>x</a>', el => {
        const stack = 'Error: react-stack-top-frame\n    at exports.jsxDEV (http://127.0.0.1:' + port + '/node_modules/.vite/deps/react_jsx-dev-runtime.js?v=9:250:30)\n'
          + '    at Nav (http://127.0.0.1:' + port + '/src/components/Nav.tsx?t=171:21:7)\n    at renderWithHooks (http://127.0.0.1:' + port + '/node_modules/.vite/deps/react-dom_client.js:5:1)';
        el['__reactFiber$q'] = { type: 'a', _debugStack: { stack }, return: { type: { displayName: 'Nav' }, _debugStack: { stack: stack.replace('Nav.tsx?t=171:21:7', 'Layout.tsx:8:3') }, return: null } };
      }));
      out.vue = ask(mk('<b>y</b>', el => { el.__vueParentComponent = { type: { __name: 'PriceCard', __file: '/w/shop/src/components/PriceCard.vue' }, parent: { type: { name: 'App', __file: '/w/shop/src/App.vue' }, parent: null } }; }));
      out.svelte = ask(mk('<i>z</i>', el => { el.__svelte_meta = { loc: { file: 'src/lib/Cart.svelte', line: 10, column: 2, char: 300 } }; }));
      out.astro = ask(mk('<span data-astro-source-file="/w/shop/src/pages/index.astro" data-astro-source-loc="31:5" data-testid="hero-cta">a</span>', () => {}));
      out.none = ask(mk('<em>plain</em>', () => {}));
      return out;
    }, port);
    ok(probed.react19.comp && probed.react19.comp.at === 'src/components/Nav.tsx:21' && probed.react19.comp.chain[0].src === 'src/components/Layout.tsx:8',
      'React 19: written in, from the dev stack (bundle frames skipped): ' + JSON.stringify(probed.react19.comp));
    ok(probed.vue.comp && probed.vue.comp.fw === 'vue' && probed.vue.comp.chain.map(c => c.name + ' ' + c.src).join(', ') === 'PriceCard /w/shop/src/components/PriceCard.vue, App /w/shop/src/App.vue', 'Vue: the components and their files');
    ok(probed.svelte.comp && probed.svelte.comp.at === 'src/lib/Cart.svelte:11', 'Svelte: the file and line (from 0 in Svelte 4)');
    ok(probed.astro.comp && probed.astro.comp.at === '/w/shop/src/pages/index.astro:31' && probed.astro.attrs['data-testid'] === 'hero-cta', 'Astro: the source attributes, and a test id');
    ok(probed.none.comp === null, 'a plain element has no component');

    // --- the next mark on this page: an area, to the same agent
    await sleep(6500);   // the box closes by itself after a send
    await pick(); await sleep(300);
    const fine = await p.locator('#fine').boundingBox();
    await p.mouse.move(fine.x - 4, fine.y - 4);
    await p.mouse.down(); await p.mouse.move(fine.x + 80, fine.y + 10, { steps: 4 }); await p.mouse.move(fine.x + 160, fine.y + fine.height + 4, { steps: 4 });
    await p.mouse.up();
    await sleep(1500);
    await p.keyboard.type('And say "including tax" here.');
    await p.keyboard.press('Control+Enter');
    let second = null;
    for (let i = 0; i < 50 && !second; i++) {
      await sleep(200);
      second = flow().nodes.find(n => /including tax/.test(n.prompt || ''));
      if (!second && card && /including tax/.test(received(card.id))) second = { typed: true };
    }
    ok(!!second, 'the next mark goes to the same agent (' + (second && second.typed ? 'typed into it' : second ? 'queued as ' + second.id + ' after it' : 'nowhere') + ')');
    const got2 = card ? received(card.id) : '';
    ok(/### an area in p#fine\n/.test(got2) && /Text: "Prices without tax\."/.test(got2), 'the area is about the paragraph in it');
    ok(!/weft-mark/.test(got2), 'weft\'s own layer is not in the page\'s html');
    if (second && !second.typed) ok(second.host === (card && card.id), 'a queued prompt of that agent');
    ok(perrs.length === 0, 'no page errors: ' + perrs.join(' | '));

    // --- a PDF in the board's folder (printed from a page, as a PDF often is): the weft button opens it
    // in weft's viewer (the browser's lets no extension in) and the picker starts there
    await sleep(6500);
    fs.mkdirSync(W + '/proj/docs', { recursive: true });
    const mk = await ctx.newPage();
    await mk.setContent(`<title>Offer</title><style>body{font:14px Georgia;margin:0} h1{font-size:28px} p{width:520px;line-height:1.5}
      .cols{columns:2;column-gap:2.5em;width:600px} .cols p{width:auto}</style>
      <h1>Offer for the new shop</h1>
      <p>Payment is due within 14 days of the invoice date. Late payments carry interest at the statutory rate, and reminders cost five euros each.</p>
      <p>Delivery takes three weeks from the order. Parts that are out of stock are sent as soon as they come in, without extra cost.</p>
      <div class="cols"><p>Left column text that runs on for a while so that it wraps onto several lines in its column.</p>
      <p>Right column text that also runs on for a while so that it wraps onto several lines.</p></div>
      <div style="height:1100px"></div><p>The warranty is two years on all parts.</p>`);
    await mk.pdf({ path: W + '/proj/docs/offer.pdf', format: 'A4' });
    await mk.close();
    const d = await ctx.newPage();
    const derrs = []; d.on('pageerror', e => derrs.push(e.message));
    await d.goto('file://' + W + '/proj/docs/offer.pdf');
    await sleep(800);
    await pick();
    await d.waitForURL(/^chrome-extension:\/\/[a-z]+\/viewer\.html\?file=/, { timeout: 10000 }).catch(() => {});
    ok(d.url().startsWith(`chrome-extension://${ID}/viewer.html?file=` + encodeURIComponent('file://' + W + '/proj/docs/offer.pdf')), 'the weft button on a PDF opens it in weft\'s viewer: ' + d.url().slice(0, 90));
    await d.waitForSelector('.pg .para', { timeout: 10000 }).catch(() => {});
    await sleep(500);
    const shown = await d.evaluate(() => ({
      pages: document.querySelectorAll('.pg').length, title: document.title, drawn: document.querySelector('.pg canvas').width,
      paras: [...document.querySelectorAll('.pg[data-n="1"] .para')].map(e => globalThis.__weftDoc.describe(e, e.getBoundingClientRect(), false).text),
      picking: !!document.querySelector('weft-mark'),
    }));
    ok(shown.pages === 2 && shown.drawn > 500 && shown.title === 'Offer · weft', `the pages are drawn (${shown.pages}, ${shown.drawn} px wide), the tab says the PDF's title: ${shown.title}`);
    ok(shown.picking, 'the picker is on as soon as the pages are there');
    ok(shown.paras[1] === 'Payment is due within 14 days of the invoice date. Late payments carry interest at the\nstatutory rate, and reminders cost five euros each.',
      'the text is laid out in lines and paragraphs: ' + JSON.stringify(shown.paras[1]));
    ok(shown.paras.some(t => t.startsWith('Left column text')) && shown.paras.some(t => t.startsWith('Right column text')) && !shown.paras.some(t => /Left[^]*Right|Right[^]*Left/.test(t)),
      'two columns are two paragraphs: ' + JSON.stringify(shown.paras.slice(3)));
    // an area over the end of one paragraph and the start of the next: only the text inside it
    const area = await d.evaluate(() => {
      const ps = document.querySelectorAll('.pg[data-n="1"] .para'), a = ps[1].getBoundingClientRect(), b = ps[2].getBoundingClientRect();
      return globalThis.__weftDoc.describe(document.body, new DOMRect(a.left - 2, a.top + a.height / 2, a.width + 4, b.top + b.height / 3 - a.top - a.height / 2), true);
    });
    ok(area.label === 'page 1 · area' && /^statutory rate.*\nDelivery takes three weeks/.test(area.text) && !/Payment/.test(area.text), 'an area holds the text inside it: ' + JSON.stringify(area.text));
    // a click marks a line, ↑ its paragraph
    const ln = await d.evaluate(() => { const r = document.querySelectorAll('.pg[data-n="1"] .para')[2].querySelector('.ln').getBoundingClientRect(); return { x: r.left + 40, y: r.top + r.height / 2 }; });
    await d.mouse.move(ln.x, ln.y); await sleep(100); await d.mouse.move(ln.x + 10, ln.y); await sleep(200);
    await d.screenshot({ path: W + '/marks-4-pdf-line.png' });
    await d.keyboard.press('ArrowUp'); await sleep(200);
    await d.screenshot({ path: W + '/marks-5-pdf-paragraph.png' });
    await d.mouse.click(ln.x + 10, ln.y);
    await sleep(1500);
    await d.keyboard.type('Say four weeks here.');
    await d.screenshot({ path: W + '/marks-6-pdf-box.png' });
    await d.keyboard.press('Control+Enter');
    let pc = null;
    for (let i = 0; i < 50 && !pc; i++) { await sleep(200); pc = flow().nodes.find(n => /four weeks/.test(n.prompt || '')); }
    ok(!!pc, 'the mark in the PDF is a card on the board that has the file\'s folder: ' + (pc ? pc.id : 'none'));
    if (pc) {
      const t = pc.prompt;
      ok(t.startsWith('Say four weeks here.\n\n## What I marked in the PDF'), 'the note first, then the PDF');
      ok(t.includes(`PDF: ${W}/proj/docs/offer.pdf "Offer" (2 pages)`), 'the file on this machine, its title and pages');
      ok(/### page 1 · paragraph\nWhere: page 1, \d+×\d+ pt at \d+,\d+ from the top left of the page \(the page is 596×843 pt\)/.test(t), 'the paragraph and where on the page it is');
      ok(t.includes('```text\nDelivery takes three weeks from the order. Parts that are out of stock are sent as soon as they\ncome in, without extra cost.\n```'), 'its text');
      ok(t.includes('change the source and make the PDF again'), 'a PDF made from a source is changed in the source');
      const shots = [...t.matchAll(/(\/\S+\.png) \(/g)].map(m => m[1]);
      ok(shots.length === 2 && shots.every(f => fs.existsSync(f)), 'its screenshots saved, drawn by the viewer: ' + shots.map(f => path.basename(f)).join(', '));
      const png = shots[0] && fs.readFileSync(shots[0]);
      ok(png && png.readUInt32BE(16) > 400 && png.readUInt32BE(16) < 1000, 'the close-up is about the paragraph (' + (png && png.readUInt32BE(16)) + ' px wide)');
      ok(pc.cwd === '' && !pc.draft, 'started, in the board folder');
    }
    // the weft button again, on the viewer itself: the picker comes back there
    await sleep(6500);
    ok(!(await d.evaluate(() => !!document.querySelector('weft-mark'))), 'the box closed after the send');
    await pick(); await sleep(500);
    ok(await d.evaluate(() => !!document.querySelector('weft-mark')), 'the weft button on the viewer marks again');
    ok(derrs.length === 0, 'no errors in the viewer: ' + derrs.join(' | '));
  } finally {
    await ctx.close();
    srv.kill();
  }
  console.log(fails ? `${fails} FAILED` : 'all passed');
  process.exitCode = fails ? 1 : 0;
})().catch(e => { console.error(e); process.exit(1); });
