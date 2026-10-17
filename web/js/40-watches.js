/* ------------------------------------------------------------ watches */
const FILM_MAX = 4;
const isWatchable = n => !!n && kindOf(n) === 'agent' && n.agent !== 'step';
const hasShotWatch = n => !!n && (n.checks || []).some(c => c.kind === 'shot');
const FILM_KINDS = ['shot', 'tests', 'changes'];
const WATCH_ONLY = ['eta', 'changes', 'ping'];
const checkKindOf = (n, cid) => ((n && n.checks || []).find(c => c.id === cid) || {}).kind || '';
/* what unfolds under a card, top down: what it asks you, then what its watches collected */
function underOf(el) {
  if (!el.under) { el.under = h('div', { class: 'under' }); el.root.append(el.under); }
  return el.under;
}
function tidyUnder(el) {
  const any = !!(el.film || el.plea);
  el.root.classList.toggle('hasunder', any);
  if (!any && el.under) { el.under.remove(); el.under = null; }
}
/* needs you: the agent's question (or the dialog on its screen), readable and scrollable, with the
   answer box right under it. The answer goes into the step that asked; the card goes back to work. */
const DIALOG_OPT = /^\s*(?:[❯›>]\s*)?(\d)[.)]\s+(\S.*?)\s*$/;
function renderPlea(id, n, r) {
  const el = nodeEls.get(id); if (!el) return;
  const on = !!r && r.status === 'attention' && isWatchable(n) && (!!r.issue || r.alive);
  if (!on) {
    if (el.plea) { el.plea.remove(); el.plea = null; tidyUnder(el); }
    return;
  }
  if (!el.plea) { el.plea = mkPlea(id); underOf(el).prepend(el.plea); tidyUnder(el); }
  const p = el.plea, ask = !!r.issue;
  const rows = ask ? [] : (r.pv || []);
  const key = ask ? 'i:' + r.issue : 'd:' + rows.join('\n');
  if (p._key === key) return;
  p._key = key;
  p.dataset.mode = ask ? 'ask' : 'dialog';
  p.hd.textContent = ask ? `${n.name} asks you` : `${n.name} waits at a dialog`;
  p.row.hidden = !ask;
  if (ask) {
    p.q.className = 'pq';
    p.q.textContent = r.issue;
    p.opts.replaceChildren();
    p.tip.textContent = `enter sends it to ${n.name}, which goes on from there · shift+enter: new line`;
  } else {
    // the dialog sits at the bottom of its screen: show that end
    p.q.className = 'pq scr';
    p.q.innerHTML = rows.map(x => `<div>${x || ' '}</div>`).join('');
    p.q.scrollTop = p.q.scrollHeight;
    const seen = new Set(), picks = [];
    for (const d of p.q.children) {
      const m = DIALOG_OPT.exec(d.textContent);
      if (m && !seen.has(m[1])) { seen.add(m[1]); picks.push(m); }
    }
    const key_ = (label, k, tip) => h('button', { class: 'fbtn key', title: tip, onclick: ev => { ev.stopPropagation(); send('key', { id, k }); } }, label);
    p.opts.replaceChildren(...picks.map(([, num, label]) => h('button', { class: 'fbtn opt', title: `press ${num}`,
      onclick: ev => { ev.stopPropagation(); send('text', { id, text: num, paste: false }); } }, h('b', {}, num), label)));
    p.tip.replaceChildren(h('span', {}, picks.length ? 'pick an answer, or open the terminal' : 'answer it in the terminal'),
      h('span', { class: 'sp' }), key_('enter', 'Enter', 'confirm what is highlighted'), key_('esc', 'Escape', 'cancel the dialog'));
  }
}
function mkPlea(id) {
  const p = h('div', { class: 'plea' });
  p.hd = h('span', { class: 't' });
  p.q = h('div', { class: 'pq' });
  p.opts = h('div', { class: 'popts' });
  p.ta = h('textarea', { rows: 1, spellcheck: 'false', placeholder: 'your answer…', 'aria-label': 'Answer the agent' });
  p.go = h('button', { class: 'pgo', title: 'send the answer (enter)', 'aria-label': 'Send the answer' }, '↵');
  p.row = h('div', { class: 'prow' }, h('span', { class: 'prompt' }, '>_'), p.ta, p.go);
  p.tip = h('div', { class: 'ptip' });
  const term = h('button', { class: 'fbtn', title: 'open its terminal', onclick: ev => { ev.stopPropagation(); openFocus(id); } }, 'terminal');
  p.append(h('div', { class: 'fh2' }, p.hd, h('span', { class: 'sp' }), term), p.q, p.opts, p.row, p.tip);
  const grow = () => { p.ta.style.height = 'auto'; p.ta.style.height = Math.min(140, p.ta.scrollHeight) + 'px'; };
  const answer = () => {
    const text = p.ta.value.trim(); if (!text) return;
    send('send', { id, text, mode: 'auto' });
    p.ta.value = ''; grow();
    p.tip.textContent = 'sent';
  };
  p.ta.addEventListener('input', grow);
  p.ta.addEventListener('keydown', e => {
    e.stopPropagation();
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) { e.preventDefault(); answer(); }
    if (e.key === 'Escape') p.ta.blur();
  });
  p.go.addEventListener('click', e => { e.stopPropagation(); answer(); });
  // reading, selecting and scrolling the question is not dragging the card or zooming the canvas
  p.addEventListener('pointerdown', e => e.stopPropagation());
  p.addEventListener('dblclick', e => e.stopPropagation());
  p.q.addEventListener('wheel', e => { if (p.q.scrollHeight > p.q.clientHeight) e.stopPropagation(); });
  return p;
}
/* jump to what needs you: the card, with the question under it and the cursor in the answer */
function showPlea(id) {
  select([id]); centerOn(id);
  setTimeout(() => focusPlea(id), 80);
}
/* enter on a card that asks you goes to its answer box */
function focusPlea(id) {
  const el = nodeEls.get(id);
  if (!el || !el.plea || el.plea.dataset.mode !== 'ask') return false;
  el.plea.ta.focus();
  return true;
}
function renderFilm(id, n, r) {
  const el = nodeEls.get(id); if (!el) return;
  const checks = (n.checks || []).filter(c => FILM_KINDS.includes(c.kind));
  if (!checks.length) {
    if (el.film) { el.film.remove(); el.film = null; tidyUnder(el); }
    return;
  }
  const mine = new Set(checks.map(c => c.id));
  const feed = ((r && r.feed) || []).filter(f => mine.has(f.check)).slice().reverse();      // newest first: where the eye goes
  const busy = (r && r.checks_busy) || [];
  const key = JSON.stringify([checks, feed.map(f => [f.id, f.state, f.reply, f.ok, (f.note || '').length]), busy]);
  if (!el.film) { el.film = h('div', { class: 'film' }); underOf(el).append(el.film); tidyUnder(el); }
  if (el.film._key === key) return;
  el.film._key = key;
  // what is being typed survives a refresh
  const keep = new Map([...el.film.querySelectorAll('input[data-item]')].map(i => [i.dataset.item, { v: i.value, f: i === document.activeElement }]));
  const nShots = feed.filter(f => f.img).length;
  const kinds = [...new Set(checks.map(c => c.kind))];
  const label = kinds.length === 1
    ? ({ shot: `screenshots${nShots ? ' · ' + nShots : ''}`, tests: 'tests', changes: 'what it changed' })[kinds[0]]
    : 'watching';
  const NOW = {
    shot: ['take one', ICON.shot, c => c.cmd ? 'run the capture command now' : 'ask the agent for a screenshot now'],
    tests: ['run tests', ICON.tests, () => 'run the tests now'],
    changes: ['refresh', ICON.changes, () => 'look at the changes now'],
  };
  const btns = checks.map(c => {
    const [text, icon, tip] = NOW[c.kind];
    return h('button', {
      class: 'fbtn' + (busy.includes(c.id) ? ' busy' : ''), title: tip(c),
      onclick: ev => { ev.stopPropagation(); send('check_now', { node: id, check: c.id }); },
    }, h('i', { html: icon }), text);
  });
  const head = h('div', { class: 'fh2' }, h('span', {}, label), h('span', { class: 'sp' }), ...btns,
    h('button', { class: 'ib', title: 'watch options', html: ICON.gear, onclick: ev => { ev.stopPropagation(); select([id]); openFly(id, 'options'); } }));
  const cells = feed.slice(0, FILM_MAX).map(it => filmCell(id, it, keep, checkKindOf(n, it.check)));
  const hint = kinds.includes('shot')
    ? (checks.some(c => c.kind === 'shot' && (c.cmd || c.every_s)) ? 'the first screenshot arrives soon. You can also paste or drop one here.' : 'no screenshots yet. Ask for one, or paste / drop an image here.')
    : kinds.includes('changes') ? 'looking at what it has changed…' : 'no test run yet';
  const empty = !feed.length ? [h('div', { class: 'fempty' }, hint)] : [];
  const more = feed.length > FILM_MAX ? [h('button', { class: 'fmore', onclick: ev => { ev.stopPropagation(); openShotbox(id, feed[0].id); } }, `all ${feed.length} →`)] : [];
  el.film.replaceChildren(head, h('div', { class: 'fgrid' }, ...cells, ...empty, ...more));
  for (const inp of el.film.querySelectorAll('input[data-item]')) {
    const k = keep.get(inp.dataset.item);
    if (k) { inp.value = k.v; if (k.f) inp.focus(); inp.dispatchEvent(new Event('input')); }
  }
}
function filmCell(id, it, keep, kind) {
  const isImg = !!it.img, isChanges = kind === 'changes';
  const input = h('input', { 'data-item': String(it.id), placeholder: isImg ? 'steer the agent…' : isChanges ? 'ask or steer…' : 'what should change…', spellcheck: 'false', 'aria-label': 'Steer the agent' });
  const go = h('button', { title: 'approve (or send what you typed)', 'aria-label': 'Approve' }, '✓');
  const act = text => send('feed_act', { node: id, item: it.id, text });
  input.addEventListener('input', () => { const t = !!input.value.trim(); go.textContent = t ? '↵' : '✓'; go.classList.toggle('send', t); });
  input.addEventListener('keydown', e => {
    e.stopPropagation();
    if (e.key === 'Enter' && input.value.trim()) { e.preventDefault(); act(input.value.trim()); input.value = ''; }
    if (e.key === 'Escape') input.blur();
  });
  go.addEventListener('click', e => {
    e.stopPropagation();
    const t = input.value.trim();
    if (t) { act(t); input.value = ''; } else if (isImg || isChanges || it.ok !== false) act(''); else act('Please fix these failures.');
  });
  // a click on the picture or the text is the zoom
  const zoom = ev => { ev.stopPropagation(); openShotbox(id, it.id); };
  const mark = it.state !== 'new' ? (it.state === 'ok' ? ' · ok' : ' · sent') : '';
  let body;
  if (isImg) {
    body = h('div', { class: 'fimg', title: 'click to zoom', onclick: zoom },
      imgEl(it.img, 'fthumb', false),
      it.state !== 'new' ? h('span', { class: 'fbadge' }, it.state === 'ok' ? '✓ ok' : 'sent') : null);
  } else if (isChanges) {
    const lines = (it.note || '').split('\n');
    body = h('div', { class: 'ftxt chg', title: 'click to read it all', onclick: zoom },
      h('span', { class: 'res' }, lines[0] + mark), lines.slice(1, 7).join('\n'));
  } else {
    body = h('div', { class: 'ftxt ' + (it.ok === false ? 'bad' : 'good'), title: 'click to read it all', onclick: zoom },
      h('span', { class: 'res' }, (it.ok === false ? '✕ failed' : '✓ passed') + mark), it.note || '(no output)');
  }
  const cap = it.state === 'steered' && it.reply ? `sent: ${it.reply}` : `${age(it.at)} ago${isImg && it.note ? ' · ' + it.note : ''}`;
  return h('div', { class: 'fc ' + (it.state || 'new') }, body, h('div', { class: 'fcap', title: cap }, cap), h('div', { class: 'fin' }, input, go));
}
const CHECK_INFO = {
  shot: n => `${n.name} will show screenshots as it works. Approve or steer from the grid under its card; the camera asks for one now.`,
  tests: n => `Tests watch on ${n.name}: set the test command, then run it from the card.`,
  eta: n => `${n.name} is asked how long it will take. The countdown shows on its card and follows what it says.`,
  changes: n => `The card of ${n.name} lists what it has changed so far, from git.`,
  ping: n => `You get a notification when ${n.name} is done or needs you.`,
};
async function attachCheck(id, kind) {
  const n = S.nodes.get(id); if (!isWatchable(n)) return;
  // the permission can only be asked for in answer to a click or a drop
  if (kind === 'ping' && 'Notification' in window && Notification.permission === 'default') { try { Notification.requestPermission(); } catch {} }
  const r = await req('check_add', { node: id, kind, subject: '', cmd: '', every_s: 0 });
  if (!r.ok) return toast('err', r.error || 'could not attach that');
  select([id]);
  toast('info', CHECK_INFO[kind](n), { ms: 7000 });
  if (kind === 'tests') openFly(id, 'options');
}
function toggleCheck(id, kind) {
  const n = S.nodes.get(id); if (!n) return;
  const c = (n.checks || []).find(x => x.kind === kind);
  if (c) send('check_del', { node: id, check: c.id }); else attachCheck(id, kind);
}
/* the zoom */
const shotbox = $('#shotbox');
const SB = { id: null, item: null, draft: '', sig: '', ageEl: null };
function closeShotbox() { SB.id = null; SB.sig = ''; SB.ageEl = null; shotbox.hidden = true; shotbox.replaceChildren(); }
function openShotbox(id, item) { SB.id = id; SB.item = item; SB.draft = ''; SB.sig = ''; renderShotbox(); }
function stepShotbox(d) {
  const r = S.rt.get(SB.id); if (!r) return;
  const list = r.feed || [];
  const i = list.findIndex(f => f.id === SB.item);
  const j = Math.max(0, Math.min(list.length - 1, i + d));
  if (list[j] && list[j].id !== SB.item) { SB.item = list[j].id; SB.draft = ''; SB.sig = ''; renderShotbox(); }
}
function renderShotbox() {
  const r = S.rt.get(SB.id), n = S.nodes.get(SB.id);
  const list = (r && r.feed) || [];
  if (!r || !n || !list.length) return closeShotbox();
  let i = list.findIndex(f => f.id === SB.item); if (i < 0) i = list.length - 1;
  const it = list[i]; SB.item = it.id;
  const kindOfItem = checkKindOf(n, it.check);
  const title = `${n.name}  \u00b7  ${kindOfItem === 'changes' ? 'what it changed' : it.note || (it.img ? 'screenshot' : 'test run')}  \u00b7  `;
  // a busy agent sends runtime updates many times a second: rebuilding on each one made the
  // buttons flicker and swallowed clicks (pressed on one button, released on its replacement)
  const sig = JSON.stringify([title, it.id, it.img, it.ok, it.state, i, list.length]);
  if (sig === SB.sig && !shotbox.hidden) { if (SB.ageEl) SB.ageEl.textContent = title + age(it.at) + ' ago'; return; }
  SB.sig = sig;
  const typed = shotbox.querySelector('input');
  if (typed) SB.draft = typed.value;
  const input = h('input', { placeholder: it.img ? `steer ${n.name}: what should change in this?` : `tell ${n.name} what to fix`, spellcheck: 'false', 'aria-label': 'Steer the agent' });
  input.value = SB.draft;
  const send_ = () => { const t = input.value.trim(); send('feed_act', { node: SB.id, item: it.id, text: t || (it.img || it.ok !== false ? '' : 'Please fix these failures.') }); SB.draft = ''; input.value = ''; };
  input.addEventListener('keydown', e => { e.stopPropagation(); if (e.key === 'Enter') { e.preventDefault(); send_(); } if (e.key === 'Escape') closeShotbox(); if (e.key === 'ArrowLeft' && !input.value) stepShotbox(-1); if (e.key === 'ArrowRight' && !input.value) stepShotbox(1); });
  let main;
  if (it.img) {
    const img = imgEl(it.img, 'sbimg', false);
    img.addEventListener('click', () => img.classList.toggle('full'));
    main = h('div', { class: 'sbimgw' }, img);
  } else if (kindOfItem === 'changes') main = h('div', { class: 'sbtxt' }, it.note || '');
  else main = h('div', { class: 'sbtxt' }, (it.ok === false ? '\u2715 failed\n\n' : '\u2713 passed\n\n') + (it.note || ''));
  shotbox.replaceChildren(
    h('div', { class: 'sbh' },
      SB.ageEl = h('span', { class: 't' }, title + age(it.at) + ' ago'),
      h('span', {}, `${i + 1} / ${list.length}`),
      h('button', { class: 'btn sm', onclick: closeShotbox }, 'close')),
    h('div', { class: 'sbm' },
      h('button', { class: 'sbnav', disabled: i === 0, 'aria-label': 'Older', onclick: () => stepShotbox(-1) }, '\u2039'),
      main,
      h('button', { class: 'sbnav', disabled: i === list.length - 1, 'aria-label': 'Newer', onclick: () => stepShotbox(1) }, '\u203a')),
    h('div', { class: 'sbf' }, input,
      h('button', { class: 'btn pri', onclick: () => { const t = input.value.trim(); if (t) send_(); else { send('feed_act', { node: SB.id, item: it.id, text: '' }); } } }, 'approve / send'),
      h('button', { class: 'btn sm danger', title: 'remove this one', onclick: () => { send('feed_del', { node: SB.id, item: it.id }); } }, 'remove')));
  shotbox.hidden = false;
  if (document.activeElement === document.body || !shotbox.contains(document.activeElement)) input.focus();
}
shotbox.addEventListener('pointerdown', e => { if (e.target === shotbox) closeShotbox(); });
/* ------------------------------------------------------------ screenshots in the app */
const shotCache = new Map();
function shotURL(name) {
  if (!shotCache.has(name)) {
    shotCache.set(name, fetch('/shots/' + encodeURIComponent(name), { headers: { 'x-weft-token': token } })
      .then(r => { if (!r.ok) throw new Error(String(r.status)); return r.blob(); })
      .then(b => URL.createObjectURL(b)));
  }
  return shotCache.get(name);
}
function imgEl(name, cls, openable = true) {
  const img = h('img', { class: cls || 'bigshot', alt: 'screenshot', draggable: 'false' });
  shotURL(name).then(u => { img.src = u; }).catch(() => { img.replaceWith(h('span', { class: 'src' }, 'image not available: ' + name)); });
  if (openable) img.addEventListener('click', e => { e.stopPropagation(); window.open(img.src, '_blank'); });
  return img;
}
async function uploadShot(id, blob) {
  try {
    const isAgent = kindOf(S.nodes.get(id)) === 'agent';
    const r = await fetch('/api/shot?' + (isAgent ? 'node=' : 'gate=') + encodeURIComponent(id), {
      method: 'POST', headers: { 'x-weft-token': token, 'content-type': blob.type || 'image/png' }, body: blob });
    const j = await r.json().catch(() => ({}));
    if (!j.ok) toast('err', j.error || 'could not attach that image');
    else toast('ok', 'screenshot attached');
  } catch { toast('err', 'could not attach that image'); }
}
addEventListener('paste', e => {
  const t = e.target;
  if ((t.tagName || '').toLowerCase() === 'textarea' || (t.tagName || '').toLowerCase() === 'input' || t.id === 'term') return;
  const f = [...(e.clipboardData ? e.clipboardData.files : [])].find(x => x.type.startsWith('image/'));
  if (!f) return;
  const sel = primary();
  const sn = sel && S.nodes.get(sel);
  let target = sn && (kindOf(sn) === 'shot' || hasShotWatch(sn)) ? sel : null;
  if (!target) {
    const open = [...S.nodes.values()].filter(n => kindOf(n) === 'shot' && (S.rt.get(n.id) || {}).gate_phase && (S.rt.get(n.id) || {}).gate_phase !== 'idle');
    if (open.length === 1) target = open[0].id;
  }
  if (target) { e.preventDefault(); uploadShot(target, f); } else toast('info', 'select a screenshot gate, or an agent with a screenshot watch, then paste the image');
});
/* "request changes": a note to the agent, typed right on the card */
function askChanges(id) {
  const el = nodeEls.get(id), n = S.nodes.get(id); if (!el || el.editingNote || !n) return;
  const ta = h('textarea', { class: 'note-edit', placeholder: n.agent === 'shot' ? 'what should change in the screenshot?  \u00b7  enter sends it to the agent' : 'what should the agent change?  \u00b7  enter sends it', spellcheck: 'false' });
  el.editingNote = true;
  el.body.append(ta); ta.focus();
  let done = false;
  const fin = send_ => {
    if (done) return; done = true; el.editingNote = false;
    const v = ta.value.trim(); ta.remove(); el._ansKey = null;
    if (send_ && v) send('gate_changes', { id, text: v });
    renderNode(id);
  };
  ta.addEventListener('keydown', e => {
    e.stopPropagation();
    if (e.key === 'Escape') fin(false);
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); fin(true); }
  });
  ta.addEventListener('blur', () => fin(false));
}
/* ------------------------------------------------------------ the gate tray: drag a gate onto a link */
const TRAY = [
  ['gate', 'approve', 'you confirm before anything downstream starts'],
  ['check', 'check', "the folder's check command must pass (build, lint, tests); red goes back to the agent"],
  ['ci', 'CI', 'after a push or a PR: the CI runs of the pushed commit must pass; red goes back to the agent that did the work, which fixes and pushes'],
  ['shot', 'screenshot', 'the agent shows its work; you approve the image or steer'],
  ['tests', 'tests', 'tests are run; red goes back to the agent'],
  ['eta', 'ETA', 'the agent says how long it will take; a countdown on its card, kept up to date'],
  ['changes', 'changes', 'what it has touched so far, from git, while it works'],
  ['ping', 'ping', 'a notification when it is done or needs you'],
];
function initTray() {
  $('#tray').replaceChildren(h('span', { class: 'tl', title: 'drop on a link: it holds the flow there (a gate). Drop on an agent: it watches it work and never blocks.' }, 'checks'), ...TRAY.map(([k, label, tip]) => {
    const where = WATCH_ONLY.includes(k) ? '. Drop it on an agent.'
      : k === 'gate' || k === 'check' || k === 'ci' ? '. Drop it on a link to hold the flow there.' : '. Drop it on a link to hold the flow there, or on an agent to watch it work.';
    const b = h('button', { class: 'tchip', 'data-k': k, title: tip + where }, h('span', { class: 'ti', html: ICON[k] }), label);
    b.addEventListener('pointerdown', e => dragGate(e, k, label));
    return b;
  }));
}
function dragGate(e, k, label) {
  if (e.button !== 0) return;
  e.preventDefault(); closeMenu(); closeFly();
  const watchOnly = WATCH_ONLY.includes(k);
  const ghost = h('div', { class: 'gghost' }, label); document.body.append(ghost);
  const sx = e.clientX, sy = e.clientY; let moved = false, over = null;
  const place = ev => { ghost.style.left = ev.clientX + 14 + 'px'; ghost.style.top = ev.clientY + 12 + 'px'; };
  const probe = ev => {
    ghost.style.display = 'none';
    const t = document.elementFromPoint(ev.clientX, ev.clientY);
    ghost.style.display = '';
    const g = t && t.closest && t.closest('.ed');
    if (g && g.dataset.edge) return { edge: g.dataset.edge };
    const nd = t && t.closest && t.closest('.nw');
    if (nd) return { node: nd.dataset.id };
    return {};
  };
  const mark = id => {
    if (over === id) return;
    if (over && edgeEls.get(over)) edgeEls.get(over).g.classList.remove('drop');
    over = id;
    if (over && edgeEls.get(over)) edgeEls.get(over).g.classList.add('drop');
  };
  let overNode = null;
  const markNode = id => {
    if (overNode === id) return;
    if (overNode && nodeEls.get(overNode)) nodeEls.get(overNode).root.classList.remove('drop', 'watch');
    overNode = id;
    if (overNode && nodeEls.get(overNode)) nodeEls.get(overNode).root.classList.add('drop', 'watch');
  };
  // (an approval and a check only hold a flow: dropped on an agent they go after it)
  const watches = id => k !== 'gate' && k !== 'check' && k !== 'ci' && isWatchable(S.nodes.get(id));
  // a watch dropped on a link belongs to the agent the link comes from
  const sourceOf = edgeId => { const ed = S.edges.get(edgeId); return ed && isWatchable(S.nodes.get(ed.from)) ? ed.from : null; };
  place(e);
  const mv = ev => {
    if (Math.hypot(ev.clientX - sx, ev.clientY - sy) > 4) moved = true;
    place(ev);
    const pr = probe(ev);
    const src = pr.edge && watchOnly ? sourceOf(pr.edge) : null;
    mark(pr.edge && !watchOnly ? pr.edge : null);
    markNode(src || (pr.node && watches(pr.node) ? pr.node : null));
    ghost.textContent = pr.edge && !watchOnly ? `${label}: hold the flow here`
      : src ? `${label}: watch ${S.nodes.get(src).name}`
        : pr.node && watches(pr.node) ? `${label}: watch ${S.nodes.get(pr.node).name}` : label;
  };
  const up = ev => {
    removeEventListener('pointermove', mv);
    const t = probe(ev); mark(null); markNode(null); ghost.remove();
    if (!moved) {
      if (!watchOnly) { const c = toWorld(innerWidth / 2 - NW / 2, innerHeight / 2 - NH); spawnHere(k, c.x, c.y); return; }
      const one = primary();
      if (one && isWatchable(S.nodes.get(one))) return attachCheck(one, k);
      return toast('info', `${label} watches an agent: drag it onto one, or select one and click it.`, { ms: 5000 });
    }
    if (t.edge) {
      if (!watchOnly) return spliceInto(t.edge, k);
      const src = sourceOf(t.edge);
      return src ? attachCheck(src, k) : toast('info', `${label} watches an agent.`, { ms: 4000 });
    }
    if (t.node) {
      if (watches(t.node)) return attachCheck(t.node, k);
      const tn = S.nodes.get(t.node);
      if (k === 'gate' && isWatchable(tn)) toast('info', `An approval holds a result, so it sits after ${tn.name} now. Drop it on a link to hold the flow there.`, { ms: 6000 });
      if (!watchOnly) return spawnLinked(t.node, 'next', k);
      return toast('info', `${label} watches an agent.`, { ms: 4000 });
    }
    if (watchOnly) return toast('info', `${label} watches an agent: drop it on one.`, { ms: 4000 });
    const w = toWorld(ev.clientX, ev.clientY);
    spawnHere(k, w.x - NW / 2, w.y - 30);
  };
  addEventListener('pointermove', mv);
  addEventListener('pointerup', up, { once: true });
}
function runNode(id) {
  const n = S.nodes.get(id);
  if (n && n.draft) return send('arm', { id });
  send('fire', { id });
}
function convertTo(id, k) {
  const cur = S.nodes.get(id), patch = { agent: k, model: '', effort: '' };
  if (k === 'sh' && cur.finish === 'idle') patch.finish = 'exit';
  if (k !== 'sh' && cur.finish === 'exit' && !cur.cmd) patch.finish = 'idle';
  patchNode(id, patch, 0);
  setAgent(k);
  // a prompt that is being written keeps the keyboard and its hint
  const el = nodeEls.get(id);
  if (el && el.editing) {
    if (el.refreshEdit) el.refreshEdit();
    const ta = el.root.querySelector('.ask-edit');
    if (ta) setTimeout(() => ta.focus(), 0);
  }
}
