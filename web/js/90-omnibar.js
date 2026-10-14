/* ------------------------------------------------------------ omnibar */
const omni = $('#omni');
let omniTa = null;
function renderOmni() {
  if (!omniTa) {
    omniTa = h('textarea', { rows: 1, spellcheck: 'false', 'aria-label': 'Message or new agent' });
    omniTa.addEventListener('input', () => { growOmni(); omniMenu(); });
    omniTa.addEventListener('focus', () => { omni.classList.add('focus'); $('#omniw').classList.add('focus'); });
    omniTa.addEventListener('blur', () => { omni.classList.remove('focus'); $('#omniw').classList.remove('focus'); });
    omniTa.addEventListener('keydown', onOmniKey);
    attachMentions(omniTa, null);
    // a click on one of the choices above hands the keyboard back to the prompt
    omni.addEventListener('click', ev => {
      if (ev.target.closest && ev.target.closest('.oc button') && !menuEl) omniTa.focus();
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
  // the choices for a new agent are the saved ones, the same as on the launch pad
  const forNew = () => {
    const a = S.lastAgent;
    return agentControls({ id: 'o', agent: a, agents, above: true, model: tuned(a, 'model'), effort: tuned(a, 'effort'), yolo: yoloDefault(),
      onAgent: k => setAgent(k),
      onModel: v => { setTuned(a, 'model', v); redo(); },
      onEffort: v => setTuned(a, 'effort', v),
      onYolo: v => { S.prefs.yolo = v; savePrefs(); redo(); } });
  };
  // the choices of an agent that exists apply the next time it is started
  const forNode = n => agentControls({ id: 'o', agent: n.agent, agents: null, above: true, model: n.model || '', effort: n.effort || '', yolo: yoloOf(n),
    onModel: v => { patchNode(n.id, { model: v }, 0); redo(); },
    onEffort: v => patchNode(n.id, { effort: v }, 0),
    onYolo: v => { patchNode(n.id, { yolo: v }, 0); redo(); } });
  let chip = null, controls = [], ckey = '';
  if (ids.length === 0) {
    controls = forNew();
    ckey = JSON.stringify([S.lastAgent, tuned(S.lastAgent, 'model'), tuned(S.lastAgent, 'effort'), yoloDefault(), (S.models[S.lastAgent] || []).length]);
  } else if (ids.length > 1) {
    chip = h('span', { class: 'tgt', style: 'cursor:default' }, `→ ${ids.length} agents`);
  } else if (canHand) {
    chip = h('button', { class: 'tgt', title: handing ? 'tab: continue this agent instead' : 'tab: hand the work over to a fresh agent instead', onclick: flip },
      handing ? '⇢ hand over from ' + single.name : '↻ ' + single.name);
    if (handing) {
      controls = forNew();
      ckey = JSON.stringify(['new', S.lastAgent, tuned(S.lastAgent, 'model'), tuned(S.lastAgent, 'effort'), yoloDefault(), (S.models[S.lastAgent] || []).length]);
    } else {
      controls = forNode(single);
      ckey = JSON.stringify(['node', single.agent, single.model, single.effort, yoloOf(single), (S.models[single.agent] || []).length]);
    }
  } else if (single && kindOf(single) !== 'agent') {
    chip = h('span', { class: 'tgt', style: 'cursor:default' }, '→ ' + single.name);
    controls = forNew();
    ckey = JSON.stringify(['new', S.lastAgent, tuned(S.lastAgent, 'model'), tuned(S.lastAgent, 'effort'), yoloDefault(), (S.models[S.lastAgent] || []).length]);
  } else {
    chip = h('span', { class: 'tgt', style: 'cursor:default' }, '→ ' + (single ? single.name : '?'));
  }
  let hint, ph;
  if (handing) {
    ph = 'what should the new agent do? it takes over this work';
    hint = [h('kbd', {}, 'enter'), ' hand over ', h('kbd', {}, 'tab'), ' continue instead'];
  } else if (ids.length === 0) {
    ph = S.lastAgent === 'sh' ? 'a shell command to run…' : 'ask an agent to…';
    hint = [h('kbd', {}, 'enter'), ' run ', h('kbd', {}, 'tab'), ' agent ', h('kbd', {}, '/'), ' commands'];
  } else if (single && kindOf(single) !== 'agent') {
    ph = 'start a new agent that receives this result…';
    hint = [h('kbd', {}, 'enter'), ' create & link'];
  } else {
    const busy = ids.some(i => { const r = S.rt.get(i); return r && r.status === 'busy'; });
    ph = busy ? 'queue a prompt…' : 'message the agent…';
    hint = [h('kbd', {}, 'enter'), busy ? ' queue ' : ' send ', h('kbd', {}, 'alt+enter'), ' steer now ', ...(canHand ? [h('kbd', {}, 'tab'), ' hand over'] : [])];
  }
  // only touch the DOM when something visible changed, and never steal focus from a user who is typing
  const key = [ids.join(','), single ? single.name : '', ph, S.lastAgent, S.omniMode, ckey, agents.map(x => x[0]).join(',')].join('|');
  if (key === omni._key) return;
  omni._key = key;
  const had = document.activeElement === omniTa;
  const sel = had ? [omniTa.selectionStart, omniTa.selectionEnd] : null;
  omniTa.placeholder = ph;
  const top = chip || controls.length ? h('div', { class: 'oc' }, ...nn([chip, ...controls])) : null;
  omni.replaceChildren(...nn([top, h('div', { class: 'or' }, h('span', { class: 'prompt' }, '>_'), omniTa, h('span', { class: 'hint2' }, ...hint))]));
  if (had) { omniTa.focus(); try { omniTa.setSelectionRange(sel[0], sel[1]); } catch {} }
}
function growOmni() { omniTa.style.height = ''; omniTa.style.height = Math.min(160, omniTa.scrollHeight) + 'px'; }
async function submitOmni(mode) {
  const text = omniTa.value.trim(); if (!text) return;
  const ids = [...S.sel];
  omniTa.value = ''; growOmni(); closePalette();
  const single = ids.length === 1 ? S.nodes.get(ids[0]) : null;
  if (ids.length === 0) {
    const last = S.nodes.get(S.lastNode);
    const spot = last ? { x: last.x + nw(last) + 150, y: last.y } : (() => { const c = toWorld(innerWidth / 2 - NW / 2, innerHeight / 2 - NH); return { x: c.x, y: c.y }; })();
    const id = await spawnAt(S.lastAgent, spot.x, spot.y, { text, draft: false, run: true, edit: false });
    if (id) setTimeout(() => centerOn(id), 80);
  } else if (single && kindOf(single) === 'agent' && single.agent !== 'sh' && S.omniMode === 'hand') {
    S.omniMode = 'then';
    await takeoverFrom(single.id, S.lastAgent, text);
  } else if (single && kindOf(single) !== 'agent') {
    await spawnLinked(single.id, 'next', S.lastAgent).then(async id => { if (id) { patchNode(id, { prompt: text, draft: false }, 0); setTimeout(() => send('arm', { id }), 120); } });
  } else for (const id of ids) send('send', { id, text, mode });
}
function onOmniKey(e) {
  if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
    if (paletteOpen()) { e.preventDefault(); return palettePick(); }
    e.preventDefault(); submitOmni(e.altKey ? 'now' : (e.metaKey || e.ctrlKey) ? 'queue' : 'auto');
  } else if (e.key === 'Escape') { if (!closePalette()) { omniTa.value = ''; growOmni(); omniTa.blur(); } e.stopPropagation(); }
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
  }
}

/* command palette: "/" in the omnibar */
let pal = { items: [], i: 0 };
const paletteOpen = () => !!(menuEl && menuEl.dataset.pal);
function closePalette() { if (paletteOpen()) { closeMenu(); return true; } return false; }
function commands() {
  return [
    { label: 'run flow', hint: 'reset and run', run: () => send('run') },
    { label: 'tidy', hint: 'arrange left to right', run: tidy },
    { label: 'fit', hint: 'f', run: fit },
    { label: 'library', hint: 'prompts and addenda', run: openLibrary },
    { label: 'reset', hint: 'clear progress', run: () => send('reset') },
    { label: 'stop all agents', run: () => { for (const [id, r] of S.rt) if (r.alive) send('stop', { id }); } },
    { label: 'undo', hint: 'ctrl+z', run: undo },
    { label: 'quit', hint: 'q', run: quitWeft },
    ...TYPES.map(t => ({ label: 'new ' + t.label, hint: t.hint, run: () => { const c = toWorld(innerWidth / 2 - NW / 2, innerHeight / 2 - NH); spawnHere(t.k, c.x, c.y); } })),
    ...agentEntries().map(([k, a]) => ({ label: 'new ' + (a.label || k), hint: agentTag(k), run: () => { select([]); setAgent(k); omniTa.value = ''; omniTa.focus(); } })),
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
