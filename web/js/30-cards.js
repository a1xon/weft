/* ------------------------------------------------------------ cards */
const agentEntries = () => {
  const ord = ['claude', 'agy', 'codex', 'sh'];
  const rank = k => { const i = ord.indexOf(k); return i < 0 ? 99 : i; };
  return Object.entries(S.cfg.agents).sort((a, b) => rank(a[0]) - rank(b[0]) || a[0].localeCompare(b[0]));
};
const agentTag = k => k === 'sh' || !k ? '$ shell' : k === 'note' ? 'note' : k === 'gate' ? 'gate \u00b7 approve' : k === 'shot' ? 'gate \u00b7 screenshot' : k === 'tests' ? 'gate \u00b7 tests' : k === 'git' ? 'git' : '@' + k;
const isCmd = n => n.finish === 'exit' && !!(n.cmd || '').trim();
const TYPES = [
  { k: 'note', label: 'note', hint: 'context any agent can be pointed at' },
  { k: 'gate', label: 'approve', hint: 'holds the flow until you say go' },
  { k: 'shot', label: 'screenshot', hint: 'asks the agent for a screenshot; you confirm it' },
  { k: 'check', label: 'check', hint: 'a command must pass (build, lint, tests); red goes back to the agent' },
  { k: 'tests', label: 'tests', hint: 'has tests written and run; red goes back to the agent' },
  { k: 'git:commit', label: 'commit', hint: 'one agent commits its own work' },
  { k: 'git:push', label: 'push', hint: 'pushes once every commit before it has resolved' },
  { k: 'git:pr', label: 'open PR', hint: 'an agent that did the work opens the pull request' },
];

function mkNode(n) {
  const el = {};
  el.st = h('span', { class: 'st' });
  el.nm = h('span', { class: 'nm', title: 'double-click to rename' });
  el.ag = h('button', { class: 'ag', title: 'change what runs this' });
  el.fd = h('button', { class: 'fdtag', hidden: true });
  el.gear = h('button', { class: 'ib', title: 'options ( , )', html: ICON.gear });
  el.ask = h('div', { class: 'ask', title: 'click to edit' });
  el.ans = h('div', { class: 'ans', title: 'click to read the whole result' });
  el.pvin = h('div', { class: 'pvin' });
  el.live = h('div', { class: 'live', title: 'click to open the terminal large' }, el.pvin);
  el.lA = h('button', { class: 'lt' }, 'result');
  el.lL = h('button', { class: 'lt' }, 'live');
  el.hand = h('div', { class: 'handnote', hidden: true }, 'result ready — drag ◇ to hand it on');
  el.lens = h('div', { class: 'lens' }, el.lA, el.lL);
  el.body = h('div', { class: 'body' }, el.ans, el.live, el.hand, el.lens);
  el.stx = h('span', { class: 'stx' });
  el.q = h('button', { class: 'qchip', hidden: true, title: 'queued prompts: select the agent to manage them' });
  el.bRun = h('button', { class: 'ib', html: ICON.play });
  el.bStop = h('button', { class: 'ib', title: 'stop this agent', html: ICON.stop });
  el.bOpen = h('button', { class: 'ib', title: 'open terminal (enter)', html: ICON.open });
  el.bGo = h('button', { class: 'fb go', hidden: true }, 'approve');
  el.bChg = h('button', { class: 'fb chg', hidden: true, title: 'tell the agent what to change; it tries again' }, 'changes\u2026');
  el.bNo = h('button', { class: 'fb no', hidden: true }, 'reject');
  el.bArm = h('button', { class: 'fb cy', hidden: true }, 'arm');
  el.bShip = h('button', { class: 'fb', hidden: true, title: 'commit what is there and push now' }, 'ship now');
  el.pin = h('div', { class: 'port in', title: 'input: click to add a step before this one, drag to link another agent in' });
  el.pout = h('div', { class: 'port out', title: 'what next: click to continue this agent, hand over or branch. Drag onto an agent to link, drag to empty canvas to spawn one there' });
  el.ptalk = h('div', { class: 'port talk', title: 'talk: drag onto another agent and the two coordinate directly while they work (only they hear it)' });
  el.grip = h('div', { class: 'grip', title: 'drag to resize: more of the terminal and the result. Double-click: the default size' });
  el.root = h('div', { class: 'nw', 'data-id': n.id },
    h('div', { class: 'node chamf' },
      // floats first: the name's text flows around them (right floats: first one is the rightmost)
      h('div', { class: 'hd' }, el.st, el.gear, el.ag, el.fd, el.nm), el.ask, el.body,
      h('div', { class: 'ft' }, el.stx, el.q, el.bGo, el.bChg, el.bNo, el.bArm, el.bShip, el.bRun, el.bStop, el.bOpen)),
    el.pin, el.pout, el.ptalk, el.grip);
  nodeEls.set(n.id, el);
  const id = n.id;

  el.bRun.addEventListener('click', e => { e.stopPropagation(); runNode(id); });
  el.bStop.addEventListener('click', e => { e.stopPropagation(); send('stop', { id }); });
  el.bOpen.addEventListener('click', e => { e.stopPropagation(); const nn_ = S.nodes.get(id); openFocus(nn_ && nn_.agent === 'step' ? hostOf(nn_).id : id); });
  el.bGo.addEventListener('click', e => {
    e.stopPropagation();
    const r = S.rt.get(id), n = S.nodes.get(id);
    // a red tests gate's primary button sends the failure back to the agent
    if (kindOf(n) === 'tests' && r && r.gate_green === false && r.gate_phase === 'review') send('gate_retry', { id, fix: true });
    else send('approve', { id, ok: true });
  });
  el.bChg.addEventListener('click', e => { e.stopPropagation(); askChanges(id); });
  el.bNo.addEventListener('click', e => { e.stopPropagation(); send('approve', { id, ok: false }); });
  // images can be pasted or dropped straight onto a screenshot gate
  const takesImages = () => { const nd = S.nodes.get(id); return kindOf(nd) === 'shot' || hasShotWatch(nd); };
  el.root.addEventListener('dragover', e => { if (takesImages() && [...(e.dataTransfer.items || [])].some(i => i.type.startsWith('image/'))) { e.preventDefault(); el.root.classList.add('dropfile'); } });
  el.root.addEventListener('dragleave', () => el.root.classList.remove('dropfile'));
  el.root.addEventListener('drop', e => {
    el.root.classList.remove('dropfile');
    const f = [...(e.dataTransfer.files || [])].find(x => x.type.startsWith('image/'));
    if (f && takesImages()) { e.preventDefault(); uploadShot(id, f); }
  });
  el.bArm.addEventListener('click', e => { e.stopPropagation(); send('arm', { id }); });
  el.bShip.addEventListener('click', e => { e.stopPropagation(); send('fire', { id }); });
  el.gear.addEventListener('click', e => { e.stopPropagation(); select([id]); openFly(id, 'options'); });
  el.q.addEventListener('click', e => { e.stopPropagation(); select([id]); $('#omni textarea').focus(); });
  el.lA.addEventListener('click', e => { e.stopPropagation(); setLens(id, 'ans'); });
  el.lL.addEventListener('click', e => { e.stopPropagation(); setLens(id, 'live'); });
  el.ans.addEventListener('click', e => {
    e.stopPropagation();
    const nd = S.nodes.get(id);
    if (kindOf(nd) === 'note') return editNote(id);
    if (!el.ans.classList.contains('hint') && !el.ans.classList.contains('kv')) { select([id]); openFly(id, 'result'); }
  });
  // the live output is a peek: a press and release on it without dragging opens the whole terminal, large.
  // Not a 'click': a busy agent repaints the peek's rows under the pointer, and a click whose pressed row
  // was replaced before the release is never fired (it opened only now and then)
  const openLive = () => {
    const nd = S.nodes.get(id);
    openFocus(nd && nd.agent === 'step' ? hostOf(nd).id : id);
  };
  el.live.addEventListener('click', e => e.stopPropagation());
  el.ag.addEventListener('click', e => {
    e.stopPropagation();
    const r = el.ag.getBoundingClientRect();
    if (kindOf(S.nodes.get(id)) !== 'agent' || S.nodes.get(id).agent === 'step') return openFly(id, 'options');
    popup([{ cap: 'run this step with' }, ...agentEntries().map(([k, a]) => ({ label: a.label || k, hint: k, run: () => convertTo(id, k) }))], r.left, r.bottom + 4);
  });
  el.fd.addEventListener('click', e => { e.stopPropagation(); folderTagClick(id); });
  el.nm.addEventListener('dblclick', e => { e.stopPropagation(); renameInline(id); });
  el.ask.addEventListener('click', e => { e.stopPropagation(); select([id]); editAsk(id); });
  el.root.addEventListener('dblclick', e => {
    if (e.target.closest('button, .port, .ask, .nm, .ask-edit, .nm-edit, .ans, textarea')) return;
    if (kindOf(S.nodes.get(id)) === 'agent') openFocus(id);
  });
  el.root.addEventListener('contextmenu', e => {
    if (e.target.closest('textarea, input')) return;
    e.preventDefault(); e.stopPropagation();
    if (!S.sel.has(id)) select([id]);
    S.sel.size > 1 ? multiMenu(e.clientX, e.clientY) : nodeMenu(id, e.clientX, e.clientY);
  });
  el.root.addEventListener('pointerdown', e => {
    if (e.button !== 0 || e.target.closest('button, .port, textarea, input')) return;
    e.stopPropagation(); closeMenu();
    if (e.shiftKey) { const s = new Set(S.sel); s.has(id) ? s.delete(id) : s.add(id); select([...s]); return; }
    if (!S.sel.has(id)) select([id]);
    if (e.altKey) return cloneDrag(e, id);
    const onLive = el.live.contains(e.target);
    // the first card of a finished flow brings the whole flow along (ctrl: just this card)
    const ids = S.sel.size === 1 && !(e.ctrlKey || e.metaKey) ? doneFlowFrom(id) : [...S.sel];
    dragNodes(e, ids, onLive ? moved => { if (!moved) openLive(); } : undefined);
  });
  el.pout.addEventListener('pointerdown', e => startConnect(e, id, 'out'));
  el.pin.addEventListener('pointerdown', e => startConnect(e, id, 'in'));
  el.ptalk.addEventListener('pointerdown', e => startTalk(e, id));
  el.pout.addEventListener('contextmenu', e => { e.preventDefault(); e.stopPropagation(); portMenu(id, 'out', e.clientX, e.clientY); });
  el.pin.addEventListener('contextmenu', e => { e.preventDefault(); e.stopPropagation(); portMenu(id, 'in', e.clientX, e.clientY); });
  el.grip.addEventListener('pointerdown', e => { if (e.button === 0) { e.stopPropagation(); closeMenu(); resizeNode(e, id); } });
  el.grip.addEventListener('dblclick', e => { e.stopPropagation(); sizeNode(id, 0, 0); });
  placeNode(n);
  $('#nodes').append(el.root);
  return el;
}
function placeNode(n) {
  const el = nodeEls.get(n.id);
  if (!el) return;
  el.root.style.transform = `translate(${n.x}px,${n.y}px)`;
  el.root.style.setProperty('--nw', nw(n) + 'px'); el.root.style.setProperty('--nh', nh(n) + 'px');
}
/* a card's size: dragged by its corner in 10px steps, kept in the flow (n.w, n.h; 0 = the default) */
const MIN_W = 300, MIN_H = 200, MAX_W = 1600, MAX_H = 1200;
function resizeNode(e, id) {
  const n = S.nodes.get(id), el = nodeEls.get(id); if (!n || !el) return;
  const sx = e.clientX, sy = e.clientY, w0 = nw(n), h0 = nh(n);
  const step = v => Math.round(v / 10) * 10;
  el.root.classList.add('sizing');
  const mv = ev => {
    const w = step(Math.min(MAX_W, Math.max(MIN_W, w0 + (ev.clientX - sx) / S.v.k)));
    const hh = step(Math.min(MAX_H, Math.max(MIN_H, h0 + (ev.clientY - sy) / S.v.k)));
    n.w = w === NW ? 0 : w; n.h = hh === NH ? 0 : hh;
    placeNode(n); redrawEdgesOf(id); placeEdgeBar(); placeFly();
  };
  const up = () => {
    removeEventListener('pointermove', mv); el.root.classList.remove('sizing');
    if (nw(n) !== w0 || nh(n) !== h0) sizeNode(id, n.w || 0, n.h || 0);
  };
  addEventListener('pointermove', mv);
  addEventListener('pointerup', up, { once: true });
}
function sizeNode(id, w, hh) {
  const n = S.nodes.get(id); if (!n) return;
  n.w = w; n.h = hh;
  placeNode(n); redrawEdgesOf(id); placeFly();
  send('node_set', { id, patch: { w, h: hh } });
}
/* the cards a drag of this one moves: all of its flow when it is the flow's first card (nothing
   leads into it) and every card of the flow is finished, else just this one */
function doneFlowFrom(id) {
  const n = S.nodes.get(id);
  if (!n || n.takeover || incoming(id).length) return [id];
  const nb = new Map();
  const link = (a, b) => { if (a && b && a !== b && S.nodes.has(a) && S.nodes.has(b)) { (nb.get(a) || nb.set(a, []).get(a)).push(b); (nb.get(b) || nb.set(b, []).get(b)).push(a); } };
  for (const e of S.edges.values()) link(e.from, e.to);
  for (const m of S.nodes.values()) link(m.takeover, m.id);
  const ids = [id], seen = new Set(ids);
  for (let i = 0; i < ids.length; i++) for (const m of nb.get(ids[i]) || []) if (!seen.has(m)) { seen.add(m); ids.push(m); }
  const finished = i => {
    if (kindOf(S.nodes.get(i)) === 'note') return true;
    const r = S.rt.get(i);
    return !!r && (cat(r.status) === 'end' || r.status === 'blocked');
  };
  return ids.length > 1 && ids.every(finished) ? ids : [id];
}
function dragNodes(e, ids, then) {
  const sx = e.clientX, sy = e.clientY;
  const start = new Map(ids.map(i => [i, { x: S.nodes.get(i).x, y: S.nodes.get(i).y }]));
  let moved = false;
  const mv = ev => {
    const dx = (ev.clientX - sx) / S.v.k, dy = (ev.clientY - sy) / S.v.k;
    if (!moved && Math.hypot(dx, dy) * S.v.k < 4) return;
    moved = true;
    for (const [i, p] of start) {
      const c = S.nodes.get(i);
      if (!c) continue;
      c.x = Math.round((p.x + dx) / 10) * 10; c.y = Math.round((p.y + dy) / 10) * 10;
      placeNode(c); redrawEdgesOf(i);
    }
    placeEdgeBar(); placeFly();
    archHover(ev, ids);
  };
  const up = () => {
    removeEventListener('pointermove', mv);
    if (moved && archDrop(ids)) {
      // dropped on the archive: they are being filed; what is still at work stays where it was
      for (const [i, p] of start) { const c = S.nodes.get(i); if (c) { c.x = p.x; c.y = p.y; placeNode(c); redrawEdgesOf(i); } }
      placeEdgeBar(); placeFly();
      if (then) then(moved);
      return;
    }
    if (moved) send('node_move', { moves: ids.filter(i => S.nodes.has(i)).map(i => ({ id: i, x: S.nodes.get(i).x, y: S.nodes.get(i).y })) });
    if (then) then(moved);
  };
  addEventListener('pointermove', mv);
  addEventListener('pointerup', up, { once: true });
}
/* alt-drag: the copy is what moves */
async function cloneDrag(e, id) {
  const n = S.nodes.get(id); if (!n) return;
  // the copy always lands visibly offset, never exactly on top of the original
  const c = { ...n }; delete c.id; c.name = ''; c.auto_name = false; c.x = n.x + 40; c.y = n.y + 40; c.draft = false;
  const sx = e.clientX, sy = e.clientY;
  const r = await req('node_add', { node: c });
  if (!r.id) return;
  await whenNode(r.id);
  select([r.id]);
  const nn_ = S.nodes.get(r.id);
  if (!nn_) return;
  const fake = { clientX: sx, clientY: sy };
  dragNodes(fake, [r.id]);
}

const cat = st => ({ busy: 'run', starting: 'run', attention: 'run', idle: 'run', done: 'end', failed: 'end', exited: 'end', stopped: 'end' })[st] || 'pre';
function lensOf(id, r) {
  const c = cat(r ? r.status : 'new'), o = S.lens.get(id);
  if (o && o.cat === c) return o.lens;
  return c === 'run' ? 'live' : 'ans';
}
function setLens(id, lens) {
  const r = S.rt.get(id);
  S.lens.set(id, { lens, cat: cat(r ? r.status : 'new') });
  renderNode(id);
}
const age = ms => {
  const s = Math.max(0, Math.round((Date.now() - ms) / 1000));
  return s < 60 ? `${s}s` : s < 3600 ? `${Math.floor(s / 60)}m${String(s % 60).padStart(2, '0')}s` : `${Math.floor(s / 3600)}h${Math.floor(s % 3600 / 60)}m`;
};
const clock = ms => { const s = Math.max(0, Math.round((Date.now() - ms) / 1000)); return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`; };

function statusText(n, r) {
  const k = kindOf(n);
  if (!r) return 'not started';
  if (k === 'note') return `note · ${(n.prompt || '').length} chars`;
  const st = r.status;
  if (k === 'git') {
    const verb = gitOpLabel(n), g = r.git || {};
    if (st === 'draft') return `disarmed · arm to ${verb}`;
    if (st === 'busy') {
      if (g.waiting_on) return g.stage === 'pr' ? `${g.waiting_on} is opening the PR…` : g.stage === 'rebase' ? `${g.waiting_on} is updating the branch…` : `${g.waiting_on} is committing…`;
      return n.git_op === 'commit' ? 'committing…' : 'pushing…';
    }
    if (st === 'done') return `${n.git_op === 'commit' ? 'committed' : n.git_op === 'pr' ? 'PR opened' : n.git_op === 'push' ? 'pushed' : 'shipped'} ${age(r.since)} ago`;
    if (st === 'failed') return `failed ${age(r.since)} ago`;
    return `armed · ${g.seen || 0}/${r.need || 0} in`;
  }
  if (n.agent === 'step') {
    const host = hostOf(n).name;
    if (st === 'draft') return `draft · enter asks ${host}`;
    if (st === 'busy') return `asking ${host}` + (r.since ? ` ${clock(r.since)}` : '');
    if (st === 'done') return `answered ${age(r.since)} ago`;
    if (st === 'failed') return `failed ${age(r.since)} ago`;
    if (st === 'waiting') return `waits for ${host} to finish`;
  }
  if (isGate(k)) {
    const who = (r.gate_to && r.gate_to[0]) ? r.gate_to[0] : 'the agent';
    const round = r.gate_round > 1 ? ` \u00b7 round ${r.gate_round}` : '';
    if (st === 'attention') return k === 'tests' && r.gate_green === false ? 'tests failed \u00b7 your call' : k === 'shot' ? 'confirm the screenshot' : 'waiting for you';
    if (st === 'busy') return r.gate_phase === 'running' ? `running${k === 'tests' ? ' the suite' : ''}${round}` : `asking ${who}${round}`;
    if (st === 'done') return `${k === 'gate' ? 'approved' : 'passed'} ${age(r.since)} ago`;
    if (st === 'failed') return `rejected ${age(r.since)} ago`;
    return r.need ? `holds \u00b7 ${r.arrived.length}/${r.need} in` : 'holds the flow';
  }
  if (r.bg && st === 'busy') return `waiting on background work \u00b7 ${clock(r.since)}`;
  if (r.hand_wait) return r.hand_phase === 'asking' ? `taking over \u00b7 ${r.hand_wait} is writing the note` : `taking over \u00b7 waiting for ${r.hand_wait} to finish`;
  switch (st) {
    case 'new': return 'not started';
    case 'draft': return 'draft · enter runs it';
    case 'waiting': return r.need ? `waiting · ${r.arrived.length}/${r.need} in` : 'waiting';
    case 'blocked': return `blocked by ${r.blocked || 'a failed agent'}`;
    case 'starting': return r.queue && r.queue.length ? 'starting · prompt queued' : 'starting';
    case 'ready': return 'ready';
    case 'busy': return r.waiting ? `waiting for ${r.waiting.what || 'something'}` + (r.waiting.checks ? ` \u00b7 checked ${r.waiting.checks}\u00d7` : '')
      : `working ${clock(r.since)}` + (r.runs > 1 ? ` \u00b7 run ${r.runs}` : '');
    case 'idle': return n.finish === 'idle' ? 'quiet' : 'quiet · waiting for you';
    case 'attention': return r.issue ? 'asks you' : 'needs you';
    case 'done': return `done ${age(r.since)} ago`;
    case 'failed': return `failed ${age(r.since)} ago`;
    case 'exited': return 'exited';
    case 'stopped': return 'stopped';
  }
  return st;
}

function renderNode(id) {
  const n = S.nodes.get(id); if (!n) return;
  const el = nodeEls.get(id) || mkNode(n);
  const r = S.rt.get(id);
  renderFilm(id, n, r);
  renderPlea(id, n, r);
  renderEta(id, n, r);
  renderFiling(id, n, r);
  const k = kindOf(n);
  const st = r ? r.status : 'new';
  el.root.dataset.st = st;
  el.root.dataset.kind = k;
  el.root.classList.toggle('sel', S.sel.has(id));
  el.root.classList.toggle('fresh', !!(r && r.fresh));
  const iconKey = k === 'agent' ? st : k;
  if (el._icon !== iconKey) { el._icon = iconKey; el.st.innerHTML = ICON[iconKey] || ICON.new; }
  if (el.nm.textContent !== n.name && !el.renaming) { el.nm.textContent = n.name; el.nm.title = n.name + ' · double-click to rename'; }
  const isStep = n.agent === 'step';
  el.root.dataset.step = String(isStep);
  el.ag.textContent = isStep ? '↻ same agent' : k === 'git' ? 'git · ' + gitOpLabel(n) : k === 'tests' && n.write_tests === false ? 'gate · check' : agentTag(n.agent) + tuneTag(n);
  if (isStep) el.ag.title = `answered by ${hostOf(n).name} in its own session`;
  else el.ag.title = tuneTag(n) ? `${agentTag(n.agent)}${n.model ? ' · model ' + n.model : ''}${n.effort ? ' · reasoning ' + n.effort : ''}${manages(n.agent) && !yoloOf(n) ? ' · asks before it acts' : ''} (click to change)` : 'change what runs this';
  el.stx.textContent = statusText(n, r);
  renderFolderTag(id, n, el);

  // ask: what this node is told to do (agents only)
  el.ask.hidden = k !== 'agent';
  if (k === 'agent') {
    const cmd = isCmd(n), txt = (cmd ? n.cmd : (n.prompt || '')).trim();
    const askKey = (cmd ? 'c:' : 'p:') + txt;
    if (el._askKey !== askKey && !el.editing) {
      el._askKey = askKey;
      el.ask.className = 'ask' + (txt ? '' : ' empty');
      el.ask.replaceChildren(...nn([cmd ? h('span', { class: 'p' }, '$ ') : null, txt || (cmd ? 'no command yet' : 'no prompt yet — click to write one')]));
    }
  }

  // body: kind-specific content
  const lens = k === 'agent' && !isStep ? lensOf(id, r) : 'ans';
  el.lens.hidden = k !== 'agent' || isStep;
  el.lA.classList.toggle('on', lens === 'ans');
  el.lL.classList.toggle('on', lens === 'live');
  el.ans.hidden = lens !== 'ans';
  el.live.hidden = lens !== 'live';
  el.hand.hidden = !(r && r.fresh && lens === 'ans' && k === 'agent');
  const rows = r && r.pv && r.pv.length ? r.pv : null;
  const liveKey = rows ? rows.join('\n') : 'none';
  if (liveKey !== el._liveKey) {
    el._liveKey = liveKey;
    if (rows) el.pvin.innerHTML = rows.map(x => `<div>${x || ' '}</div>`).join('');
    else el.pvin.replaceChildren(h('div', { class: 'ph' }, 'no terminal output yet'));
  }
  let key, build;
  if (k === 'note') {
    const t = n.prompt || '';
    key = 'n:' + t; build = () => ({ cls: 'ans note' + (t ? '' : ' hint'), kids: [t || 'a note: context you can point any agent at. click to write.'] });
  } else if (k === 'shot') {
    const shots = (r && r.gate_shots) || [];
    if (shots.length) {
      key = 'shot:' + shots.join(',') + st + (r.gate_out || '');
      build = () => ({ cls: 'ans kv', kids: [imgEl(shots[0], 'shotimg'), h('span', { class: 'shotcap' }, (r.gate_out || shots[0]).split('\n')[0])] });
    } else if (r && r.gate_phase === 'asking') {
      key = 'shot-ask:' + r.gate_round; build = () => ({ cls: 'ans hint', kids: [`asking ${(r.gate_to || ['the agent'])[0]} to take a screenshot${r.subject ? '' : ''}\u2026`] });
    } else if (r && r.gate_out && r.gate) {
      key = 'shot-note:' + r.gate_out; build = () => ({ cls: 'ans bad', kids: [r.gate_out, h('span', { class: 'src' }, '\npaste or drop an image here, or press retake.')] });
    } else {
      key = 'shot0:' + st; build = () => ({ cls: 'ans hint', kids: [st === 'done' ? 'confirmed.' : st === 'failed' ? 'rejected.' : n.subject ? `holds the flow until you confirm a screenshot of ${n.subject}.` : 'asks the agent upstream for a screenshot and holds the flow until you confirm it.'] });
    }
  } else if (k === 'tests') {
    const out = (r && r.gate_out) || '';
    key = 'tests:' + (n.cmd || '') + st + (r ? r.gate_phase + r.gate_round : '') + out;
    build = () => ({ cls: 'ans kv', kids: nn([
      h('span', { class: 'kvl' }, n.cmd ? h('b', {}, '$ ' + n.cmd) : n.write_tests === false ? 'the agent runs the checks' : 'the agent writes and runs the suite'),
      out ? h('span', { class: 'commit', style: 'white-space:pre-wrap;color:' + (r && r.gate_green === false ? 'var(--rd)' : 'var(--i1)') }, out.split('\n').slice(-5).join('\n'))
        : h('span', { class: 'src' }, st === 'done' ? 'passed.' : n.write_tests === false ? 'runs it and sends failures back to the agent. nothing downstream starts until it passes.'
          : 'asks for tests to be written, runs them, and sends failures back to the agent. nothing downstream starts until they pass.'),
    ]) });
  } else if (k === 'gate') {
    const held = r && r.gate;
    if (held != null && st === 'attention') { key = 'g:' + held; build = () => ({ cls: 'ans kv', kids: [h('span', { class: 'src on' }, 'holding these results for you:'), held || '(nothing arrived)'] }); }
    else { key = 'g0:' + st + (r ? r.arrived.length : 0); build = () => ({ cls: 'ans hint', kids: [st === 'done' ? 'approved. the results moved on.' : st === 'failed' ? 'rejected.' : 'a checkpoint: results that arrive wait here until you approve.'] }); }
  } else if (k === 'git') {
    const g = (r && r.git) || { log: [] };
    const op = n.git_op || '';
    const src = incoming(id).map(e => S.nodes.get(e.from)).filter(Boolean).map(x => hostOf(x).name);
    const carried = g.carried || [];
    key = 'git:' + op + n.branch + n.remote + (n.prompt || '') + src.join() + g.log.join('|') + (r ? r.status : '') + (g.pr || '') + (g.prs || []).join() + (g.commits || 0) + '/' + (g.files || 0) + carried.join();
    build = () => ({ cls: 'ans kv', kids: nn([
      op === 'commit'
        ? h('span', { class: 'kvl' }, 'commits the work of ', h('b', {}, src[0] || '(link an agent in)'), ...(n.branch ? [' on ', h('b', {}, n.branch)] : []))
        : op === 'pr'
          ? h('span', { class: 'kvl' }, 'pull request for ', h('b', {}, n.branch || 'the pushed branch'))
          : h('span', { class: 'kvl' }, 'branch ', h('b', {}, n.branch || 'current'), '  →  ', h('b', {}, (n.remote || 'origin') + (op === '' && n.push === false ? ' (no push)' : ''))),
      ...(g.log.length ? g.log.map(l => h('span', { class: 'commit' }, h('i', {}, l.startsWith('failed') ? '✕' : '●'), l))
        : [h('span', { class: 'src' }, op === 'commit' ? 'asks the agent linked into it to commit its own work. Give another agent its own commit; link the commits to one push.'
          : op === 'push' ? 'waits until every commit linked into it has resolved, then pushes.'
            : op === 'pr' ? 'once pushed, asks an agent that did the work to write and open the pull request.'
              : 'commits each result as it arrives; pushes when every incoming link has delivered.')]),
      // (one pull request per repository the work is in)
      ...((g.prs && g.prs.length ? g.prs : g.pr ? [g.pr] : []).map(u => h('a', { class: 'prlink', href: u, target: '_blank', rel: 'noopener', title: 'open the pull request' }, u))),
      r && st === 'done' && (g.commits || g.files || carried.length)
        ? h('span', { class: 'src on' }, 'passes on: ' + [g.commits ? `${g.commits} commit${g.commits > 1 ? 's' : ''}` : '', g.files ? `${g.files} file${g.files > 1 ? 's' : ''}` : '',
          carried.length ? `what ${carried.join(' and ')} did` : ''].filter(Boolean).join(' · ')) : null,
    ]) });
  } else if (r && r.status === 'blocked') {
    key = 'b:' + r.blocked; build = () => ({ cls: 'ans bad', kids: [`blocked: "${r.blocked}" failed or did not match this link.`, '\nfix it and run it again to open the junction.'] });
  } else if (r && r.issue) {
    key = 'i:' + r.issue; build = () => ({ cls: 'ans kv', kids: [h('span', { class: 'src ask' }, 'asks you \u2014 answer under the card \u2193'), r.issue] });
  } else if (r && r.out) {
    key = 'o:' + r.out + (r.ok === false ? 'F' : ''); build = () => ({ cls: 'ans ' + (r.ok === false ? 'bad' : 'ok'), kids: [r.out, h('span', { class: 'fade' })] });
  } else if (r && r.status === 'waiting') {
    const inc = incoming(id);
    key = 'w:' + inc.map(e => e.id + (r.arrived.includes(e.id) ? 1 : 0)).join();
    build = () => ({ cls: 'ans hint', kids: nn([
      h('span', { class: 'src' }, `waiting on ${n.join === 'any' ? 'the first of' : 'all of'}:`),
      ...inc.map(e => { const s = S.nodes.get(e.from); const got = r.arrived.includes(e.id);
        return h('span', { class: 'src' + (got ? ' on' : '') }, (got ? '✓ ' : '… ') + (s ? s.name : '?')); })]) });
  } else if (st === 'draft') {
    key = 'dr:'; build = () => ({ cls: 'ans hint', kids: ['a draft: write the prompt above and press enter to run it.'] });
  } else {
    key = 'h:' + st; build = () => ({ cls: 'ans hint', kids: [st === 'new' ? 'no result yet. press ▶ to run, or link another agent into this one.' : 'no result yet.'] });
  }
  if (el._ansKey !== key && !el.editingNote) {
    el._ansKey = key;
    const b = build();
    el.ans.className = b.cls;
    el.ans.replaceChildren(...nn(b.kids));
  }

  // footer controls by kind and state
  const q = r && r.queue ? r.queue.length : 0;
  el.q.hidden = q === 0 || k !== 'agent';
  el.q.textContent = `+${q} queued`;
  const alive = r && (r.alive || st === 'starting');
  el.bStop.hidden = !(k === 'agent' && alive);
  el.bRun.hidden = k !== 'agent';
  el.bOpen.hidden = k !== 'agent';
  el.bRun.title = r && r.hand_wait ? 'do not wait for the note: start with what is on disk' : st === 'draft' ? 'run it now (enter)' : alive ? 'send this prompt now' : 'start this agent and send its prompt (run)';
  const review = isGate(k) && st === 'attention';
  const anyOpen = isGate(k) && r && r.gate_phase && r.gate_phase !== 'idle' && !review;
  el.bGo.hidden = !(review || anyOpen);
  el.bGo.textContent = anyOpen ? 'continue' : k === 'tests' && r && r.gate_green === false ? 'send to agent' : k === 'gate' ? 'approve' : 'confirm';
  el.bGo.title = anyOpen ? 'stop waiting and let the flow continue' : '';
  el.bChg.hidden = !(review && (k === 'shot' || (k === 'tests' && r && r.gate_green !== false)));
  el.bNo.hidden = !(review || anyOpen);
  el.bArm.hidden = !(k === 'git' && n.draft);
  el.bShip.hidden = !(k === 'git' && !n.draft && st !== 'busy');
  el.bShip.textContent = n.git_op === 'commit' ? 'commit now' : n.git_op === 'push' ? 'push now' : n.git_op === 'pr' ? 'open PR now' : 'ship now';
  el.gear.hidden = k === 'note';
  el.ptalk.hidden = !canTalk(n);
}

function renameInline(id) {
  const el = nodeEls.get(id), n = S.nodes.get(id); if (!el) return;
  const inp = h('input', { class: 'nm-edit', value: n.name, 'aria-label': 'Name' });
  el.renaming = true;
  el.nm.replaceWith(inp); inp.focus(); inp.select();
  let done = false;
  const fin = ok => {
    if (done) return; done = true; el.renaming = false;
    const v = inp.value.trim();
    inp.replaceWith(el.nm);
    if (ok && v && v !== n.name) patchNode(id, { name: v }, 0);
  };
  inp.addEventListener('keydown', e => { if (e.key === 'Enter') fin(true); if (e.key === 'Escape') fin(false); e.stopPropagation(); });
  inp.addEventListener('blur', () => fin(true));
}
/* edit the prompt right on the card. A new draft is armed with enter, dropped with esc. */
function editAsk(id) {
  const el = nodeEls.get(id), n = S.nodes.get(id); if (!el || el.editing || !n) return;
  // a shell step edits its command, everything else its prompt; switching agent flips it live
  const isShell = () => { const c = S.nodes.get(id); return !!c && (c.agent === 'sh' || isCmd(c)); };
  const wasDraft = !!n.draft;
  const hint = () => isShell()
    ? 'a shell command  ·  enter runs it  ·  tab switches agent'
    : wasDraft ? 'what should this agent do?  ·  enter runs it  ·  tab switches agent  ·  @ mentions another agent'
      : 'what should this agent do?  ·  ctrl+enter saves and runs';
  const ta = h('textarea', { class: 'ask-edit', value: isShell() ? (n.cmd || '') : (n.prompt || ''), spellcheck: 'false', placeholder: hint() });
  el.editing = true;
  el.ask.hidden = false;
  el.ask.replaceWith(ta); ta.focus(); ta.setSelectionRange(ta.value.length, ta.value.length);
  el.refreshEdit = () => { ta.placeholder = hint(); };
  let done = false;
  const fin = (ok, arm) => {
    if (done) return; done = true; el.editing = false; el.refreshEdit = null;
    const v = ta.value;
    ta.replaceWith(el.ask);
    el._askKey = null;
    const cur = S.nodes.get(id);
    if (!cur) return;
    // escape on a brand-new, still empty draft takes it back
    if (!ok && wasDraft && !v.trim()) { send('node_del', { id }); return; }
    if (ok) {
      const field = isShell() ? 'cmd' : 'prompt';
      if (v !== (cur[field] || '')) patchNode(id, { [field]: v }, 0);
      if (arm === 'arm') send('arm', { id });
      else if (arm === 'fire') send('fire', { id });
    }
    renderNode(id);
  };
  attachMentions(ta, id);
  ta.addEventListener('keydown', e => {
    e.stopPropagation();
    if (e.key === 'Tab') {
      // switch what runs this step without leaving the keyboard
      e.preventDefault();
      const ks = agentEntries().map(x => x[0]); const cur = S.nodes.get(id);
      const i = Math.max(0, ks.indexOf(cur.agent));
      convertTo(id, ks[(i + (e.shiftKey ? ks.length - 1 : 1)) % ks.length]);
      return;
    }
    if (e.key === 'Escape') { closeMenu(); fin(false); }
    else if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      if (e.metaKey || e.ctrlKey) fin(true, 'fire');
      else if (wasDraft || (S.nodes.get(id) || {}).draft) fin(true, 'arm');
      else fin(true, null);
    }
  });
  ta.addEventListener('blur', () => setTimeout(() => { if (!menuEl && !done && document.activeElement !== ta) fin(true, null); }, 150));
}
function editNote(id) {
  const el = nodeEls.get(id), n = S.nodes.get(id); if (!el || el.editingNote || !n) return;
  const ta = h('textarea', { class: 'note-edit', value: n.prompt || '', placeholder: 'context for any agent you link this to…  ctrl+enter saves', spellcheck: 'false' });
  el.editingNote = true;
  el.body.append(ta); ta.focus(); ta.setSelectionRange(ta.value.length, ta.value.length);
  let done = false;
  const fin = ok => {
    if (done) return; done = true; el.editingNote = false;
    const v = ta.value;
    ta.remove();
    el._ansKey = null;
    if (!ok && !(n.prompt || '').trim() && !v.trim()) { send('node_del', { id }); return; }
    if (ok && v !== (S.nodes.get(id).prompt || '')) patchNode(id, { prompt: v }, 0);
    renderNode(id);
  };
  ta.addEventListener('keydown', e => {
    e.stopPropagation();
    if (e.key === 'Escape') fin(false);
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) fin(true);
  });
  ta.addEventListener('blur', () => fin(true));
}
