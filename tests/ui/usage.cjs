// Plan usage: a meter under the agents in the pad and the bar shows how much of the chosen agent's plan
// is used, for the chosen model; the CLI is asked once and not again for 15 minutes (not on a reload,
// not for a click right after, not by another page); a limit nearly used up is red.
const { chromium } = require(process.env.PLAYWRIGHT);
const fs = require('fs');
const W = process.env.SANDBOX;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let fails = 0; const ok = (c, m) => { console.log((c ? 'PASS ' : 'FAIL ') + m); if (!c) fails++; };
const asked = who => (fs.existsSync(W + '/usage-asked') ? fs.readFileSync(W + '/usage-asked', 'utf8') : '').split('\n').filter(l => l === who).length;
const iso = ms => new Date(ms).toISOString().replace(/\.\d+Z$/, 'Z');
(async () => {
  // what the CLIs say (claude in its words, agy as its JSON), the times ahead of now whenever it runs
  const berlin = (ms, o) => new Date(ms).toLocaleString('en-US', { timeZone: 'Europe/Berlin', hour: 'numeric', ...o }).replace(/ ([AP]M)/, (_, x) => x.toLowerCase());
  const at = berlin(Date.now() + 2 * 3600e3, { minute: '2-digit' }), day = berlin(Date.now() + 2 * 86400e3, { month: 'short', day: 'numeric' });
  fs.writeFileSync(W + '/usage-claude.txt', 'You are currently using your subscription to power your Claude Code usage\n\n' +
    'Current session: 34% used · resets ' + at + ' (Europe/Berlin)\nCurrent week (all models): 12% used · resets ' + day + ' (Europe/Berlin)\n' +
    'Current week (Sonnet only): 81% used · resets ' + day + ' (Europe/Berlin)\n\nWhat\'s contributing to your limits usage?\n  67%  Turn off skills\n');
  const soon = Date.now() + 2 * 3600e3, later = Date.now() + 5 * 86400e3;
  fs.writeFileSync(W + '/usage-agy.txt', JSON.stringify({ status: 'SUCCESS', command: { name: 'usage', data: { groups: [
    { name: 'Gemini Models', buckets: [{ window: 'weekly', remaining_fraction: 0.6, reset_time: iso(later) }, { window: '5h', remaining_fraction: 0.05, reset_time: iso(soon) }] },
    { name: 'Claude and GPT models', buckets: [{ window: 'weekly', remaining_fraction: 1, reset_time: iso(later) }, { window: '5h', remaining_fraction: 0.8, reset_time: iso(soon) }] }] } } }) + '\n');

  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM });
  const ctx = await b.newContext({ viewport: { width: 1500, height: 950 } });
  const p = await ctx.newPage();
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type() === 'error' && errs.push(m.text()));
  const waitFor = async (fn, t = 15000) => { const t0 = Date.now(); while (Date.now() - t0 < t) { if (await fn()) return true; await sleep(150); } return false; };
  // (without the countdown beside each bar: ut)
  const bars = sel => p.locator(sel + ' .umeter .ub').evaluateAll(l => l.map(e => [...e.childNodes].filter(c => !c.classList?.contains('ut')).map(c => c.textContent).join(' ').replace(/\s+/g, ' ').trim()));
  const url = fs.readFileSync(W + '/url.txt', 'utf8').trim();

  await p.goto(url); await p.waitForSelector('#pad:not([hidden]) textarea');
  // --- the launch pad shows it for the agent it starts (claude, its default model: the limits of all models)
  ok(await waitFor(async () => (await bars('#pad')).length === 2), 'the pad shows the plan usage of claude: ' + JSON.stringify(await bars('#pad')));
  const pad = await bars('#pad');
  ok(/^5h\s*34%$/.test(pad[0]) && /^week\s*12%$/.test(pad[1]), 'the session and the week, in percent: ' + JSON.stringify(pad));
  const left = await p.locator('#pad .umeter .ut').allInnerTexts();
  ok(left.length && left.every(t => /^\d+[dhm]$/.test(t)), 'beside each, how long until it starts over: ' + JSON.stringify(left));
  const tip = await p.locator('#pad .umeter').getAttribute('title');
  ok(/starts over in (\d+[dhm] ?)+/.test(tip) && /every 15 min/.test(tip), 'its tooltip says when each starts over and how often it is asked');
  await p.screenshot({ path: W + '/usage-1-pad.png' });
  await p.keyboard.press('Escape'); await sleep(300);
  ok((await bars('#omni')).length === 2, 'the bar shows it too');
  ok(asked('claude') === 1, 'claude was asked once (' + asked('claude') + ')');

  // --- the model chosen: a limit of its own joins
  await p.locator('#omni .pmodels button', { hasText: /^sonnet$/ }).click(); await sleep(300);
  const son = await bars('#omni');
  ok(son.length === 3 && /^sonnet\s*81%$/.test(son[2]), 'sonnet chosen: its own weekly limit is shown as well: ' + JSON.stringify(son));
  ok(await p.locator('#omni .umeter .ub.warm').count() === 1, 'one that is mostly used up is amber');

  // --- not asked again: a reload, another page, a click right after the answer
  await p.reload(); await p.waitForSelector('#pad:not([hidden]) textarea'); await p.keyboard.press('Escape'); await sleep(800);
  ok((await bars('#omni')).length === 3, 'after a reload the meter is there at once');
  const p2 = await ctx.newPage(); await p2.goto(url); await sleep(1500); await p2.close();
  await p.locator('#omni .umeter').click(); await sleep(1000);
  ok(asked('claude') === 1, 'and claude was not asked again: reload, second page, click (' + asked('claude') + ')');
  // --- a click a little later asks again, and shows the new numbers
  fs.writeFileSync(W + '/usage-claude.txt', fs.readFileSync(W + '/usage-claude.txt', 'utf8').replace('34% used', '51% used'));
  await sleep(10500); await p.locator('#omni .umeter').click();
  ok(await waitFor(async () => /^5h\s*51%$/.test((await bars('#omni'))[0])), 'a click later refreshes it: ' + JSON.stringify(await bars('#omni')));
  ok(asked('claude') === 2 && !(await p.locator('#omni .umeter.asking').count()), 'claude was asked once more, and the meter is not dimmed any more (' + asked('claude') + ')');

  // --- agy: its limits are per group of models; its default model is in the first group
  await p.locator('#omni .pagents button', { hasText: /^agy$/ }).click();
  ok(await waitFor(async () => (await bars('#omni')).length === 2), 'agy: the limits of its default model\'s group: ' + JSON.stringify(await bars('#omni')));
  const g = await bars('#omni');
  ok(/^5h\s*95%$/.test(g[0]) && /^week\s*40%$/.test(g[1]), 'what is left became what is used, the shortest window first (gemini: 95% / 40%)');
  ok(await p.locator('#omni .umeter .ub.hot').count() === 1, 'a limit almost used up is red');
  await p.screenshot({ path: W + '/usage-2-agy.png', clip: { x: 0, y: 700, width: 1500, height: 250 } });
  await p.evaluate(() => { setTuned('agy', 'model', 'claude-sonnet-4-6'); redrawPrompts(); }); await sleep(300);
  const c = await bars('#omni');
  ok(c.length === 2 && /^5h\s*20%$/.test(c[0]), 'a Claude model on agy: the other group\'s limits: ' + JSON.stringify(c));
  ok(/claude gpt/.test(await p.locator('#omni .umeter').getAttribute('title')), 'its tooltip names the group');
  ok(asked('agy') === 1, 'agy was asked once (' + asked('agy') + ')');

  // --- a CLI that cannot tell: a quiet question mark, the reason in its tooltip
  await p.locator('#omni .pagents button', { hasText: /^codex$/ }).click();
  ok(await waitFor(async () => /\?/.test(await p.locator('#omni .umeter.none').innerText().catch(() => ''))), 'codex said nothing usable: "usage ?"');
  ok(/could not be asked|did not say/.test(await p.locator('#omni .umeter').getAttribute('title')), 'and the tooltip says why');

  ok(!errs.length, 'no page errors' + (errs.length ? ': ' + errs.join(' | ') : ''));
  await b.close();
  console.log(fails ? `${fails} FAILED` : 'ALL PASS');
  process.exit(fails ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
