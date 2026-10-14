/* ------------------------------------------------------------ "and then\u2026" */
/* Three ways to go on, cheapest first:
   then      queue a prompt on the SAME agent: its context and memory stay, links fire when its queue is done
   hand over a FRESH agent takes over the work: the old one writes a note, the files stay where they are
   branch    a NEW agent runs alongside and receives the result (a link) */
const nextbar = $('#nextbar');
const NX = { id: null, mode: 'then', agent: null };
const NX_MODES = [
  ['then', 'then', 'the same agent goes on: its context and memory stay'],
  ['hand', 'hand over', 'a fresh agent takes over the work, with a note from this one'],
  ['branch', 'branch', 'a new agent runs alongside and receives the result'],
];
function closeNext() { NX.id = null; nextbar.hidden = true; nextbar.replaceChildren(); }
function openNext(id, mode) {
  const n = S.nodes.get(id); if (!n || kindOf(n) !== 'agent') return;
  closeMenu(); closeFly();
  const keep = nextbar.querySelector('textarea');
  NX.id = id;
  // an agent that has not produced anything yet has nothing to continue: the natural next step is a branch
  const ran = (S.rt.get(id) || {}).runs > 0;
  NX.mode = mode || (n.agent === 'sh' || !ran ? 'branch' : 'then');
  const ag = agentList();
  NX.agent = n.agent !== 'sh' && S.cfg.agents[n.agent] ? n.agent : (ag.some(([k]) => k === S.lastAgent) ? S.lastAgent : (ag[0] || [S.lastAgent])[0]);
  renderNext(keep ? keep.value : '');
}
function renderNext(text) {
  const n = S.nodes.get(NX.id); if (!n) return closeNext();
  const r = S.rt.get(NX.id) || {};
  const ta = h('textarea', { rows: 2, spellcheck: 'false', 'aria-label': 'What next?' });
  ta.value = text || '';
  const modes = n.agent === 'sh' ? NX_MODES.filter(m => m[0] !== 'then') : NX_MODES;
  const seg_ = h('div', { class: 'seg' }, modes.map(([v, label, tip]) =>
    h('button', { 'aria-pressed': String(NX.mode === v), title: tip, onclick: () => { NX.mode = v; renderNext(ta.value); } }, label)));
  const busy = r.status === 'busy' || r.status === 'starting';
  const explain = {
    then: busy ? `${n.name} is working. This waits in its queue; its links fire after the last queued step.`
      : `${n.name} keeps its context and runs this next. Use it for follow-ups on the same work.`,
    hand: `A fresh ${NX.agent} takes over. ${n.name} writes a short handover note first; the files stay as they are.${busy ? ` It starts when ${n.name} has finished its step.` : ''}`,
    branch: `A new ${NX.agent} starts alongside and receives ${n.name}'s result (a link).`,
  }[NX.mode];
  const ph = {
    then: `what should ${n.name} do next?`,
    hand: 'what should the new agent do? (empty: carry on where it stopped)',
    branch: 'what should the new agent do with the result?',
  }[NX.mode];
  ta.placeholder = ph;
  const agents = agentList();
  const pick = NX.mode === 'then' ? null : h('div', { class: 'seg' }, agents.map(([k, c]) =>
    h('button', { 'aria-pressed': String(NX.agent === k), title: c.label || k, onclick: () => { NX.agent = k; renderNext(ta.value); } }, k)));
  const go = h('button', { class: 'btn sm pri', onclick: submitNext }, NX.mode === 'then' ? (busy ? 'queue' : 'send') : NX.mode === 'hand' ? 'hand over' : 'create');
  nextbar.replaceChildren(
    h('div', { class: 'nh' }, h('span', { class: 'nt' }, `after ${n.name}`), seg_),
    ta,
    h('p', { class: 'nx' }, explain),
    h('div', { class: 'nf' }, ...nn([pick, h('span', { class: 'sp' }), h('kbd', {}, 'enter'), go])));
  ta.addEventListener('keydown', e => {
    e.stopPropagation();
    if (e.isComposing) return;
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); submitNext(); }
    else if (e.key === 'Escape') { e.preventDefault(); closeNext(); }
    else if (e.key === 'Tab') {
      e.preventDefault();
      const order = modes.map(m => m[0]); const i = order.indexOf(NX.mode);
      NX.mode = order[(i + (e.shiftKey ? order.length - 1 : 1)) % order.length]; renderNext(ta.value);
    }
  });
  ta.addEventListener('input', () => { ta.style.height = 'auto'; ta.style.height = Math.min(210, Math.max(62, ta.scrollHeight)) + 'px'; });
  nextbar.hidden = false;
  placeNext();
  ta.focus();
  ta.setSelectionRange(ta.value.length, ta.value.length);
}
function placeNext() {
  const el = nextbar.hidden || !NX.id ? null : nodeEls.get(NX.id);
  if (!el) return;
  const c = el.root.getBoundingClientRect(), w = nextbar.offsetWidth || 410, hgt = nextbar.offsetHeight || 190;
  let left = c.right + 22, top = c.top;
  if (left + w > innerWidth - 12) { left = Math.max(12, Math.min(c.left, innerWidth - w - 12)); top = c.bottom + 10; }
  if (top + hgt > innerHeight - 70) top = Math.max(50, innerHeight - hgt - 70);
  nextbar.style.left = left + 'px'; nextbar.style.top = top + 'px';
}
addEventListener('pointerdown', ev => { if (NX.id && !nextbar.contains(ev.target) && !(ev.target.closest && ev.target.closest('.port.out'))) closeNext(); }, true);
function submitNext() {
  const n = S.nodes.get(NX.id); if (!n) return closeNext();
  const ta = nextbar.querySelector('textarea'), text = ta.value.trim();
  const { id, mode, agent } = NX;
  if (mode === 'then' && !text) { ta.classList.remove('nope'); void ta.offsetWidth; ta.classList.add('nope'); return; }
  closeNext();
  if (mode === 'then') {
    select([id]);
    send('send', { id, text, mode: 'auto' });
    const r = S.rt.get(id);
    if (r && (r.status === 'busy' || r.status === 'starting')) toast('info', `queued on ${n.name}: it goes out after the current step`, { ms: 3500 });
  } else if (mode === 'hand') {
    takeoverFrom(id, agent, text);
  } else {
    spawnAt(agent, n.x + NW + 150, n.y, { from: id, text, draft: !text, run: false, edit: !text });
  }
}
/* a fresh agent takes over: the engine asks the old one for a note, composes the handover and starts it */
async function takeoverFrom(id, agent, text) {
  const n = S.nodes.get(id); if (!n) return null;
  if (agent && S.cfg.agents[agent] && agent !== 'sh') setAgent(agent);
  const ag = agent && S.cfg.agents[agent] ? agent : agentFor(n);
  const p = freeSpot(n.x + NW + 150, n.y);
  const ac = S.cfg.agents[ag] || {};
  const node = { agent: ag, x: p.x, y: p.y, model: ac.model_arg ? tuned(ag, 'model') : '', effort: ac.effort_arg ? tuned(ag, 'effort') : '', addenda: addendaIds() };
  if (ac.yolo_arg) node.yolo = yoloOf(n);
  const r = await req('takeover', { from: id, node, text });
  if (!r.ok) { toast('err', r.error || 'could not hand over'); return null; }
  await whenNode(r.id);
  select([r.id]); reveal(r.id);
  return r.id;
}
