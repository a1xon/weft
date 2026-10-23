/* ------------------------------------------------------------ omnibar */
const omni = $('#omni');
let omniTa = null;
function renderOmni() {
  renderWhere();   // (the folder line says where what is typed here will run)
  if (!omniTa) {
    omniTa = h('textarea', { rows: 1, spellcheck: 'false', 'aria-label': 'Message or new agent' });
    omniTa._grow = growOmni;
    omniTa.addEventListener('input', () => { growOmni(); omniMenu(); });
    omniTa.addEventListener('keydown', onOmniKey);
    attachMentions(omniTa, null);
    keepDraft(omniTa, 'bar');
    // a click on one of the choices around the prompt hands the keyboard back to it
    omni.addEventListener('click', ev => {
      if (pad.ed || menuEl || typingInField() || !ev.target.closest) return;
      if (ev.target.closest('button')) omniTa.focus();
    });
  }
  const ids = [...S.sel];
  const picked = ids.length === 1 ? S.nodes.get(ids[0]) : null;
  const single = hostOf(picked);   // a queued prompt is spoken to through its agent
  // with one agent selected the bar can continue it or hand its work to a fresh one (tab)
  const canHand = !!(single && kindOf(single) === 'agent' && single.agent !== 'sh');
  const handing = canHand && S.omniMode === 'hand';
  const flip = () => { S.omniMode = S.omniMode === 'hand' ? 'then' : 'hand'; omni._key = null; renderOmni(); omniTa.focus(); };
  const agents = agentList();
  const redo = () => { omni._key = null; renderOmni(); };
  // a card the bar talks to says so on the chip, which also lets go of it (✕)
  const aim = el => h('span', { class: 'aimw' }, el,
    h('button', { class: 'tgx', title: 'let go: the bar starts a new agent again (it lets go by itself after 30 s untouched)', 'aria-label': 'let go of the selection', onclick: () => { select([]); omniTa.focus(); } }, '✕'));
  // the choices for a new agent are the saved ones, the same as on the launch pad
  const forNew = () => {
    const a = S.lastAgent;
    return agentControls({ id: 'o', agent: a, agents, lead: chip, above: true, model: tuned(a, 'model'), effort: tuned(a, 'effort'), yolo: yoloDefault(),
      onAgent: k => setAgent(k),
      onModel: v => { setTuned(a, 'model', v); redo(); },
      onEffort: v => setTuned(a, 'effort', v),
      onYolo: v => { S.prefs.yolo = v; savePrefs(); redo(); } });
  };
  // the choices of an agent that exists apply the next time it is started
  const forNode = n => agentControls({ id: 'o', agent: n.agent, agents: null, lead: chip, above: true, model: n.model || '', effort: n.effort || '', yolo: yoloOf(n),
    onModel: v => { patchNode(n.id, { model: v }, 0); redo(); },
    onEffort: v => patchNode(n.id, { effort: v }, 0),
    onYolo: v => { patchNode(n.id, { yolo: v }, 0); redo(); } });
  // a selected queued prompt shows the reasoning level it gives its agent when it is asked (merge.rs)
  const forStep = (st, host) => agentControls({ id: 'o', agent: host.agent, agents: null, lead: chip, above: true, model: host.model || '',
    effort: st.effort || '', levels: stepLevels(st), onEffort: v => { patchNode(st.id, { effort: v }, 0); redo(); } });
  let chip = null, controls = [], ckey = '', forNewAgent = false;
  if (ids.length === 0) {
    forNewAgent = true;
    controls = forNew();
    ckey = JSON.stringify([S.lastAgent, tuned(S.lastAgent, 'model'), tuned(S.lastAgent, 'effort'), yoloDefault(), (S.models[S.lastAgent] || []).length]);
  } else if (ids.length > 1) {
    chip = aim(h('span', { class: 'tgt', style: 'cursor:default' }, `→ ${ids.length} agents`));
  } else if (canHand) {
    chip = aim(h('button', { class: 'tgt', title: handing ? 'tab: continue this agent instead' : 'tab: hand the work over to a fresh agent instead', onclick: flip },
      handing ? '⇢ hand over from ' + single.name : '↻ ' + single.name));
    if (handing) {
      forNewAgent = true;
      controls = forNew();
      ckey = JSON.stringify(['new', S.lastAgent, tuned(S.lastAgent, 'model'), tuned(S.lastAgent, 'effort'), yoloDefault(), (S.models[S.lastAgent] || []).length]);
    } else if (picked !== single && stepLevels(picked)) {
      controls = forStep(picked, single);
      ckey = JSON.stringify(['step', picked.id, picked.effort, single.agent, single.model, single.effort]);
    } else {
      controls = forNode(single);
      ckey = JSON.stringify(['node', single.agent, single.model, single.effort, yoloOf(single), (S.models[single.agent] || []).length]);
    }
  } else if (single && kindOf(single) !== 'agent') {
    chip = aim(h('span', { class: 'tgt', style: 'cursor:default' }, '→ ' + single.name));
    forNewAgent = true;
    controls = forNew();
    ckey = JSON.stringify(['new', S.lastAgent, tuned(S.lastAgent, 'model'), tuned(S.lastAgent, 'effort'), yoloDefault(), (S.models[S.lastAgent] || []).length]);
  } else {
    chip = aim(h('span', { class: 'tgt', style: 'cursor:default' }, '→ ' + (single ? single.name : '?')));
  }
  // the same keys as on the launch pad (enter, shift+enter, tab, /), plus the ones a running agent adds
  let hint, ph, label;
  if (handing) {
    ph = 'what should the new agent do? it takes over this work';
    hint = [['ctrl+enter', 'hand over'], ['enter', 'new line'], ['tab', 'continue instead']]; label = 'hand over';
  } else if (ids.length === 0) {
    ph = S.lastAgent === 'sh' ? 'a shell command to run…' : 'ask an agent to…';
    hint = [['ctrl+enter', 'start'], ['enter', 'new line'], ['tab', 'next agent'], ['/', 'commands']]; label = 'start';
  } else if (single && kindOf(single) !== 'agent') {
    ph = 'start a new agent that receives this result…';
    hint = [['ctrl+enter', 'create & link'], ['enter', 'new line']]; label = 'create & link';
  } else {
    const busy = ids.some(i => { const n = hostOf(S.nodes.get(i)), r = n && S.rt.get(n.id); return !!(r && (r.status === 'busy' || r.status === 'starting')); });
    const who = single ? single.name : 'the agents';
    ph = busy ? (single && single.agent === 'sh' ? 'queue a command…' : `${who} is working: a prompt typed here waits as its own card after it…`) : `message ${who}…`;
    hint = [['ctrl+enter', busy ? 'queue' : 'send'], ['alt+enter', 'steer now'], ['enter', 'new line'], ...(canHand ? [['tab', 'hand over']] : [])];
    label = busy ? 'queue' : 'send';
  }
  // the standard addenda belong to the agent that is about to be made
  const addenda = forNewAgent && S.lastAgent !== 'sh' && !pad.open;   // (the pad shows them itself while it is open)
  const addKey = addenda ? JSON.stringify([S.lib.filter(x => x.kind === 'addendum').map(x => [x.id, x.title, addendumOn(x)]), !!pad.ed, S.lib.filter(x => x.kind === 'prompt').length]) : '';
  // only touch the DOM when something visible changed, and never steal focus from a user who is typing
  markAim();
  const key = [ids.join(','), single ? single.name : '', ph, label, S.lastAgent, S.omniMode, ckey, agents.map(x => x[0]).join(','), addKey, S.usageV].join('|');
  if (key === omni._key) return;
  omni._key = key;
  if (!addenda && !pad.open) pad.ed = null;   // an addendum being written has no room in this mode
  const had = document.activeElement === omniTa;
  const sel = had ? [omniTa.selectionStart, omniTa.selectionEnd] : null;
  const edFocus = editorFocus();
  omniTa.placeholder = ph;
  // (the target chip sits in the controls' own left column when there are controls)
  const top = chip || controls.length ? h('div', { class: 'oc' }, ...nn([controls.length ? null : chip, ...controls])) : null;
  // the launch pad's own box in its compact form, and under it the same addenda
  const box = promptBox({ id: 'obox', ta: omniTa, compact: true, label, go: () => submitOmni('auto'), hint: keyHints(hint) });
  keepIn(omni, nn([top, box, addenda ? addendaRow() : null]));
  growOmni();
  settleKeys(omniTa, had, sel, edFocus, false);
}
/* alt+up / alt+down: the reasoning level of what the bar is set up for (the next new agent, or the selected one) */
function omniEffort(d) {
  const picked = S.sel.size === 1 ? S.nodes.get([...S.sel][0]) : null;
  const one = picked ? hostOf(picked) : null;
  const mine = one && kindOf(one) === 'agent' && one.agent !== 'sh' && S.omniMode !== 'hand';
  if (mine && picked !== one) {
    // a queued prompt: the level it gives its agent when it is asked
    cardEffort(picked.id, d); omni._key = null; renderOmni();
  } else if (mine) {
    const v = effortStep(one.agent, one.model || '', one.effort || '', d);
    if (v !== null) { patchNode(one.id, { effort: v }, 0); omni._key = null; renderOmni(); }
  } else if (S.sel.size <= 1) padEffort(d);
}
/* two lines to begin with, one more for every line typed up to a cap (expanded: a fixed tall box) */
function growOmni() {
  omniTa.style.height = '';
  if (promptBig) return;
  omniTa.style.height = Math.min(Math.max(240, innerHeight * 0.3), omniTa.scrollHeight) + 'px';
}
async function submitOmni(mode) {
  const text = omniTa.value.trim(); if (!text) { shake($('#obox')); return; }
  const ids = [...S.sel];
  rememberSent(text); setPrompt(omniTa, ''); omniTa._hi = null; closePalette();
  const single = ids.length === 1 ? S.nodes.get(ids[0]) : null;
  // an open reader follows the card that answers this; a new agent is shown on the canvas
  const handing = single && kindOf(single) === 'agent' && single.agent !== 'sh' && S.omniMode === 'hand';
  if (ids.length === 0 || handing || (single && kindOf(single) !== 'agent')) readerSent(null);
  if (ids.length === 0) {
    const last = S.nodes.get(S.lastNode);
    const spot = last ? { x: last.x + nw(last) + 150, y: last.y } : (() => { const c = toWorld(innerWidth / 2 - NW / 2, innerHeight / 2 - NH); return { x: c.x, y: c.y }; })();
    const id = await spawnAt(S.lastAgent, spot.x, spot.y, { text, draft: false, run: true, edit: false, lane: true });
    if (id) setTimeout(() => centerOn(id), 80);
    else if (!omniTa.value) { omniTa.value = text; growOmni(); saveDraft(omniTa); }   // (not started: the text stays)
  } else if (single && kindOf(single) === 'agent' && single.agent !== 'sh' && S.omniMode === 'hand') {
    S.omniMode = 'then';
    await takeoverFrom(single.id, S.lastAgent, text);
  } else if (single && kindOf(single) !== 'agent') {
    await spawnLinked(single.id, 'next', S.lastAgent).then(async id => { if (id) { patchNode(id, { prompt: text, draft: false }, 0); setTimeout(() => send('arm', { id }), 120); } });
  } else for (const id of ids) {
    // a busy agent gets the prompt as a card queued after its work; steering goes in right away
    const n = hostOf(S.nodes.get(id));
    if (sendAnswers(n.id, text)) { if (ids.length === 1) readerSent(n.id, text); continue; }
    if (mode !== 'now' && busyAgent(n)) queueStep(n, text).then(sid => ids.length === 1 && sid && readerSent(sid, text));
    else { send('send', { id, text, mode }); if (ids.length === 1) readerSent(n.id, text); }
  }
}
const busyAgent = n => { const r = n && S.rt.get(n.id); return !!(r && kindOf(n) === 'agent' && n.agent !== 'sh' && (r.status === 'busy' || r.status === 'starting')); };
/* a prompt for a busy agent is a card of its own: a queued prompt (step) linked after what that
   agent still has to do, so what comes next stays in sight and can be edited, moved or deleted
   until it runs. (A link from an agent that is already done hands its result on at once.) */
async function queueStep(host, text) {
  let tail = host;
  for (const seen = new Set([host.id]); ;) {
    const next = outgoing(tail.id).map(e => S.nodes.get(e.to))
      .find(n => n && n.agent === 'step' && n.host === host.id && !seen.has(n.id) && !['done', 'failed'].includes((S.rt.get(n.id) || {}).status));
    if (!next) break;
    seen.add(next.id); tail = next;
  }
  const p = freeSpot(tail.x + nw(tail) + 150, tail.y);
  const node = { agent: 'step', host: host.id, x: p.x, y: p.y, draft: true, prompt: text, addenda: host.addenda || [] };
  const r = await req('node_add', { node });
  if (!r.ok) { toast('err', r.error || 'could not queue that'); return null; }
  send('edge_add', { from: tail.id, to: r.id });
  send('arm', { id: r.id });
  await whenNode(r.id);
  reveal(r.id);
  tip('qstep', `${host.name} is working: your prompt waits as its own card and is asked of ${host.name} when it is done. Edit, move or delete it until then.`);
  return r.id;
}
function onOmniKey(e) {
  if (e.key === 'Enter' && !e.isComposing) {
    // ctrl+enter sends (it queues by itself while the agent is busy), alt+enter steers now; a plain
    // enter is a new line. Only a command chosen from the palette takes a plain enter.
    if (paletteOpen()) { e.preventDefault(); return palettePick(); }
    if (e.ctrlKey || e.metaKey || e.altKey) { e.preventDefault(); submitOmni(e.altKey ? 'now' : 'auto'); }
  } else if (e.key === 'Escape') { if (!closePalette() && !readerBack()) { setPrompt(omniTa, ''); omniTa.blur(); } e.stopPropagation(); }
  else if (e.key === 'Tab' && !S.sel.size && !omniTa.value.startsWith('/')) {
    e.preventDefault();
    const ks = agentEntries().map(x => x[0]); const i = ks.indexOf(S.lastAgent);
    setAgent(ks[(i + (e.shiftKey ? ks.length - 1 : 1)) % ks.length]); omniTa.focus();
  } else if (e.key === 'Tab' && S.sel.size === 1 && !omniTa.value.startsWith('/')) {
    const one = S.nodes.get([...S.sel][0]);
    if (one && kindOf(one) === 'agent' && one.agent !== 'sh') {
      e.preventDefault();
      S.omniMode = S.omniMode === 'hand' ? 'then' : 'hand'; omni._key = null; renderOmni(); omniTa.focus();
    }
  } else if ((e.key === 'ArrowDown' || e.key === 'ArrowUp') && paletteOpen()) {
    e.preventDefault(); paletteMove(e.key === 'ArrowDown' ? 1 : -1);
  } else if (e.altKey && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) {
    e.preventDefault(); omniEffort(e.key === 'ArrowUp' ? 1 : -1);
  } else if (plainArrow(e) && recall(omniTa, e.key === 'ArrowUp' ? -1 : 1)) e.preventDefault();
}

/* command palette: "/" in the omnibar */
let pal = { items: [], i: 0 };
const paletteOpen = () => !!(menuEl && menuEl.dataset.pal);
function closePalette() { if (paletteOpen()) { closeMenu(); return true; } return false; }
function commands() {
  return [
    { label: 'run flow', hint: 'reset and run', run: () => send('run') },
    { label: 'tidy', hint: 'line the cards up', run: tidy },
    { label: 'fit', hint: 'f', run: fit },
    { label: 'library', hint: 'networks, prompts and addenda', run: () => openLibrary() },
    ...allNets().map(n => ({ label: 'network: ' + n.title, hint: 'place it', run: () => dropNet(n, null) })),
    { label: 'reset', hint: 'clear progress', run: () => send('reset') },
    { label: 'stop all agents', run: () => { for (const [id, r] of S.rt) if (r.alive) send('stop', { id }); } },
    { label: 'undo', hint: 'ctrl+z', run: undo },
    { label: 'quit', hint: 'q', run: quitWeft },
    ...TYPES.map(t => ({ label: 'new ' + t.label, hint: t.hint, run: () => { const c = toWorld(innerWidth / 2 - NW / 2, innerHeight / 2 - NH); spawnHere(t.k, c.x, c.y); } })),
    ...agentEntries().map(([k, a]) => ({ label: 'new ' + (a.label || k), hint: agentTag(k), run: () => { select([]); setAgent(k); omniTa.value = ''; omniTa.focus(); } })),
    ...folderCommands(),
    ...S.lib.filter(s => s.kind === 'prompt').map(s => ({ label: 'prompt: ' + s.title, hint: 'insert', run: () => { omniTa.value = s.text; growOmni(); omniTa.focus(); } })),
    ...S.lib.filter(s => s.kind === 'addendum').map(s => ({ label: 'addendum: ' + s.title, hint: 'append', run: () => { omniTa.value = (omniTa.value + '\n\n' + s.text).trim(); growOmni(); omniTa.focus(); } })),
  ];
}
function omniMenu() {
  const v = omniTa.value;
  if (!v.startsWith('/') || v.includes('\n')) { closePalette(); return; }
  const q = v.slice(1).toLowerCase();
  pal.items = commands().filter(c => c.label.toLowerCase().includes(q) || (c.hint || '').toLowerCase().includes(q)).slice(0, 10);
  pal.i = 0;
  const r = omni.getBoundingClientRect();
  const m = popup(pal.items.length ? pal.items.map(it => ({ ...it, run: () => { omniTa.value = ''; growOmni(); it.run(); } })) : [{ cap: 'no command matches' }], r.left, r.top - 6, { above: true });
  m.dataset.pal = '1';
  paletteMove(0);
}
function paletteMove(d) {
  if (!paletteOpen()) return;
  const mis = [...menuEl.querySelectorAll('.mi')];
  if (!mis.length) return;
  pal.i = (pal.i + d + mis.length) % mis.length;
  mis.forEach((m, i) => m.classList.toggle('on', i === pal.i));
}
function palettePick() { const mis = [...menuEl.querySelectorAll('.mi')]; if (mis[pal.i]) mis[pal.i].click(); }

/* @mentions inside any prompt field: insert {{from:name}} and link the agents */
function attachMentions(ta, nodeId) {
  ta.addEventListener('input', () => {
    const pos = ta.selectionStart, before = ta.value.slice(0, pos);
    const m = /(^|\s)@([\w\- ]{0,30})$/.exec(before);
    if (!m) return;
    const q = m[2].toLowerCase();
    const cands = [...S.nodes.values()].filter(n => n.id !== nodeId && n.name.toLowerCase().includes(q)).slice(0, 8);
    if (!cands.length) return;
    const r = ta.getBoundingClientRect();
    popup([{ cap: 'insert the result of…' }, ...cands.map(n => ({
      label: n.name, hint: (S.rt.get(n.id) || {}).status || 'new',
      run: () => {
        const start = pos - m[2].length - 1;
        const ins = `{{from:${n.name}}}`;
        ta.value = ta.value.slice(0, start) + ins + ta.value.slice(pos);
        ta.setSelectionRange(start + ins.length, start + ins.length);
        ta.dispatchEvent(new Event('input', { bubbles: true }));
        ta.focus();
        if (nodeId && !incoming(nodeId).some(e => e.from === n.id)) send('edge_add', { from: n.id, to: nodeId });
      },
    }))], r.left, r.bottom + 4);
  });
}

/* ------------------------------------------------------------ queue panel (above the omnibar) */
function renderQueue() {
  const el = $('#qpanel');
  const id = primary();
  const r = id && S.rt.get(id), n = id && S.nodes.get(id);
  const q = r && r.queue ? r.queue : [];
  if (!n || !q.length || kindOf(n) !== 'agent') { el.hidden = true; el.replaceChildren(); return; }
  el.hidden = false;
  let dragId = null;
  el.replaceChildren(
    h('div', { class: 'qh' }, h('b', {}, n.name), `queue · ${q.length} prompt${q.length > 1 ? 's' : ''}, sent one after another`, h('span', { class: 'sp' }),
      h('button', { class: 'btn sm dim', onclick: () => send('queue_clear', { id }) }, 'clear')),
    ...q.map((it, i) => {
      const row = h('div', { class: 'qi', draggable: 'true', title: it.text },
        h('span', { class: 'n' }, i + 1), h('span', { class: 't' }, it.text.replace(/\s+/g, ' ')),
        h('span', { class: 's' }, it.src === 'flow' ? 'from flow' : 'you'),
        h('button', { title: 'send next', onclick: () => send('queue_now', { id, qid: it.id }) }, 'next'),
        h('button', { title: 'remove', onclick: () => send('queue_del', { id, qid: it.id }) }, '✕'));
      row.addEventListener('dragstart', e => { dragId = it.id; e.dataTransfer.effectAllowed = 'move'; });
      row.addEventListener('dragover', e => { e.preventDefault(); row.classList.add('over'); });
      row.addEventListener('dragleave', () => row.classList.remove('over'));
      row.addEventListener('drop', e => { e.preventDefault(); row.classList.remove('over'); if (dragId != null && dragId !== it.id) send('queue_move', { id, qid: dragId, to: i }); });
      return row;
    }));
}

/* ------------------------------------------------------------ what the bar is aimed at */
/* The cards the bar talks to (the selection) carry a tag that says so, and the bar lets go of them
   after AIM_IDLE without anyone touching them or typing to them: a prompt typed later starts a new
   agent instead of landing on an old card by surprise. Work in progress holds it: text in the bar,
   an open flyout, terminal, reader, menu or composer, a field of the card being typed in. */
const AIM_IDLE = 30000, AIM_WARN = 10000;
let aimAt = Date.now();
function aimHeld() {
  if ((omniTa && omniTa.value.trim()) || S.fly || S.focus || menuEl || NX.id || !dial.hidden) return true;
  if (typeof RD !== 'undefined' && RD.id) return true;
  const a = document.activeElement;
  return !!(a && a !== document.body && a.closest && [...S.sel].some(id => { const el = nodeEls.get(id); return el && el.root.contains(a); }));
}
function markAim() {
  const left = AIM_IDLE - (Date.now() - aimAt);
  const soon = S.sel.size && left <= AIM_WARN ? Math.max(1, Math.ceil(left / 1000)) : 0;
  $('#omniw').classList.toggle('aimed', S.sel.size > 0);
  $('#omniw').classList.toggle('lapse', !!soon);
  for (const [id, el] of nodeEls) {
    const on = S.sel.has(id);
    if (!on && !el.aim) continue;
    if (!el.aim) { el.aim = h('div', { class: 'aimtag' }); el.root.append(el.aim); }
    el.aim.hidden = !on;
    const t = soon ? `⌨ prompt bar → here · lets go in ${soon} s` : '⌨ prompt bar → here';
    if (on && el.aim.textContent !== t) el.aim.textContent = t;
  }
}
function aimTouch() { const late = Date.now() - aimAt > AIM_IDLE - AIM_WARN; aimAt = Date.now(); if (late) markAim(); }
// touching a card the bar is aimed at, or the bar, the flyout or the terminal, counts as working with it
for (const t of ['pointerdown', 'keydown', 'wheel', 'input']) addEventListener(t, ev => {
  if (!S.sel.size) return;
  const x = ev.target && ev.target.closest ? ev.target : null;
  const card = x && x.closest('.nw');
  if (card ? S.sel.has(card.dataset.id) : t === 'keydown' || (x && x.closest('#dock, #fly, #focus, #reader'))) aimTouch();
}, true);
addEventListener('pointermove', ev => { const c = S.sel.size && ev.target.closest && ev.target.closest('.nw'); if (c && S.sel.has(c.dataset.id)) aimTouch(); }, { passive: true });
setInterval(() => {
  if (!S.sel.size) { aimAt = Date.now(); return; }
  if (aimHeld()) aimAt = Date.now();
  if (Date.now() - aimAt >= AIM_IDLE) { select([]); return; }
  markAim();
}, 1000);
