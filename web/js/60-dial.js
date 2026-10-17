/* ------------------------------------------------------------ the dial
   Drag a link into empty space and let go: where the line ends is where the next card goes, and a
   ring opens there (the card's outline stays under it), split into one equal slice per option (the first one to the right, the rest
   clockwise). For an agent: a prompt the SAME agent answers (its answer comes back to that card),
   a fresh agent that takes over, git. Options are data: add one to `dialFor` and it gets a slice. */
const hostOf = n => (n && n.agent === 'step' ? (S.nodes.get(n.host) || n) : n);
const GIT_VERB = { commit: 'commit', push: 'push', pr: 'open PR', '': 'commit & push' };
const gitOpLabel = n => GIT_VERB[n.git_op || ''] || 'commit & push';
const DIAL_OPT = {
  git: { key: 'git', label: 'git', hint: 'commit, push or open a PR; the agent is told what to do', run: c => gitMenu(c) },
  queue: { key: 'queue', label: 'queue a prompt', hint: 'the same agent answers it', run: c => spawnStep(c.id, c.at) },
  handoff: { key: 'handoff', label: 'hand off', hint: 'a fresh agent takes over the work', run: c => spawnHandoff(c.id, c.at) },
  agent: { key: 'agent', label: 'new agent', hint: 'receives the result', run: c => spawnAt(agentFor(c.id), c.at.x, c.at.y, { from: c.id }) },
  // after a git step the agent gets what travelled with it: commits, files, who did what, the PR
  after: { key: 'agent', label: 'next agent', hint: 'gets the commits, files and who did what', run: c => spawnAt(agentFor(c.id), c.at.x, c.at.y, { from: c.id }) },
  push: { key: 'push', label: 'push', hint: 'once every commit before it has resolved', run: c => spawnGit(c.id, c.at, 'push') },
  pr: { key: 'pr', label: 'open PR', hint: 'an agent that did the work opens it', run: c => spawnGit(c.id, c.at, 'pr') },
  // runs the folder's deploy command on exactly what was pushed (61-deploy.js)
  deploy: { key: 'deploy', label: 'deploy', hint: 'runs the deploy command on what was pushed', run: c => spawnDeploy(c.id, c.at) },
};
function dialFor(n) {
  const k = kindOf(n);
  if (k === 'git') return n.git_op === 'commit' ? [DIAL_OPT.after, DIAL_OPT.push] : n.git_op === 'pr' ? [DIAL_OPT.after] : [DIAL_OPT.after, DIAL_OPT.deploy, DIAL_OPT.pr];
  if (k === 'agent' && n.agent !== 'sh') return [DIAL_OPT.queue, DIAL_OPT.handoff, DIAL_OPT.git];
  if (k === 'agent') return [DIAL_OPT.agent, DIAL_OPT.git];
  return [DIAL_OPT.agent];
}
const dial = h('div', { id: 'dial', hidden: true });
document.body.append(dial);
// the ring: a small hole (letting go there drops the link), a wide body that fades out; past its
// edge a slice still goes on in its direction
const DIAL_IN = 26, DIAL_OUT = 170, DIAL_LBL = 110, DIAL_GAP = 3;
const DIAL_KEY = { ArrowRight: 0, ArrowDown: 90, ArrowLeft: 180, ArrowUp: 270 };
let dialOff = null;
const dialSize = n => 360 / n;
/* which slice a direction (degrees, 0 = right, clockwise) falls in */
function dialSlice(n, deg) { const s = dialSize(n); return Math.floor((((deg % 360) + 360 + s / 2) % 360) / s) % n; }
/* one slice of the ring, a constant gap to its neighbours */
function dialPath(i, n) {
  const s = dialSize(n) * Math.PI / 180, c = i * s, pt = (r, a) => `${(r * Math.cos(a)).toFixed(1)} ${(r * Math.sin(a)).toFixed(1)}`;
  const po = DIAL_GAP / DIAL_OUT, pi = DIAL_GAP / DIAL_IN, big = s - 2 * po > Math.PI ? 1 : 0;
  return `M${pt(DIAL_OUT, c - s / 2 + po)}A${DIAL_OUT} ${DIAL_OUT} 0 ${big} 1 ${pt(DIAL_OUT, c + s / 2 - po)}` +
    `L${pt(DIAL_IN, c + s / 2 - pi)}A${DIAL_IN} ${DIAL_IN} 0 0 0 ${pt(DIAL_IN, c - s / 2 + pi)}Z`;
}
function dialArc(i, n, r) {
  const s = dialSize(n) * Math.PI / 180, c = i * s, p = DIAL_GAP / r, pt = a => `${(r * Math.cos(a)).toFixed(1)} ${(r * Math.sin(a)).toFixed(1)}`;
  return `M${pt(c - s / 2 + p)}A${r} ${r} 0 ${s - 2 * p > Math.PI ? 1 : 0} 1 ${pt(c + s / 2 - p)}`;
}
function dialFade(id, color, stops) {
  const g = sv('radialGradient', { id, cx: 0, cy: 0, r: DIAL_OUT, gradientUnits: 'userSpaceOnUse' });
  for (const [off, a] of stops) g.append(sv('stop', { offset: off, 'stop-color': color, 'stop-opacity': a }));
  return g;
}
/* open the dial at (cx, cy) once the link is let go. The slice under the pointer is marked (and
   handed to `onMark`); a click anywhere outside the hole, or an arrow key and Enter, picks it; Esc,
   a click in the hole or a turn of the wheel drops the link. */
function openDial(cx, cy, opts, pick, onMark) {
  closeDial();
  const n = opts.length;
  cx = Math.max(DIAL_LBL + 90, Math.min(cx, innerWidth - DIAL_LBL - 90));
  cy = Math.max(DIAL_LBL + 40, Math.min(cy, innerHeight - DIAL_LBL - 40));
  let cur = null;
  const ring = sv('svg', { class: 'dring', width: 2 * DIAL_OUT, height: 2 * DIAL_OUT, viewBox: `${-DIAL_OUT} ${-DIAL_OUT} ${2 * DIAL_OUT} ${2 * DIAL_OUT}` });
  // a dark halo dims what lies under the ring; the slices glow on top of it, the marked one in cyan
  const k = DIAL_IN / DIAL_OUT, defs = sv('defs');
  defs.append(dialFade('dialShade', '#04060a', [[0, 0.85], [0.6, 0.6], [1, 0]]),
    dialFade('dialFill', '#8fb4dc', [[k, 0.2], [0.45, 0.09], [0.8, 0.025], [1, 0]]),
    dialFade('dialOn', '#5cc8ff', [[k, 0.6], [0.45, 0.28], [0.8, 0.07], [1, 0]]));
  ring.append(defs, sv('circle', { class: 'dshade', r: DIAL_OUT }), sv('circle', { class: 'dhole', r: DIAL_IN - 4 }), sv('circle', { class: 'dcore', r: 2.5 }));
  const segs = opts.map((o, i) => {
    const g = sv('g', { class: 'dseg', 'data-k': o.key });
    g.append(sv('path', { class: 'dbody', d: dialPath(i, n) }), sv('path', { class: 'dedge', d: dialArc(i, n, DIAL_IN + 2) }));
    ring.append(g);
    return g;
  });
  const pills = opts.map((o, i) => {
    const a = i * dialSize(n) * Math.PI / 180;
    return h('div', { class: 'dpill', 'data-k': o.key, style: `left:${Math.round(DIAL_LBL * Math.cos(a))}px;top:${Math.round(DIAL_LBL * Math.sin(a))}px` },
      h('b', {}, o.label), h('span', {}, o.hint));
  });
  dial.replaceChildren(ring, ...pills);
  dial.style.left = cx + 'px'; dial.style.top = cy + 'px'; dial.hidden = false;
  const mark = o => {
    cur = o;
    for (const el of [...segs, ...pills]) el.classList.toggle('on', !!o && el.dataset.k === o.key);
    if (onMark) onMark(o);
  };
  const at = ev => {
    const dx = ev.clientX - cx, dy = ev.clientY - cy;
    return Math.hypot(dx, dy) < DIAL_IN ? null : opts[dialSlice(n, Math.atan2(dy, dx) * 180 / Math.PI)];
  };
  const done = (o, ev) => { closeDial(); pick(o, ev); };
  const mv = ev => mark(at(ev));
  const eat = ev => { ev.preventDefault(); ev.stopPropagation(); };
  const down = ev => {
    eat(ev);
    // the click that follows this press must not reach the canvas (it would take the focus away)
    addEventListener('click', eat, { capture: true, once: true });
    setTimeout(() => removeEventListener('click', eat, true), 600);
    done(ev.button === 0 ? at(ev) : null, ev);
  };
  const wheel = () => done(null, null);
  const key = ev => {
    ev.preventDefault(); ev.stopPropagation();
    if (ev.key in DIAL_KEY) return mark(opts[dialSlice(n, DIAL_KEY[ev.key])]);
    if (ev.key === 'Enter' && cur) return done(cur, null);
    if (ev.key === 'Escape') done(null, null);
  };
  const on = [['pointermove', mv], ['pointerdown', down], ['keydown', key], ['wheel', wheel]];
  for (const [t, f] of on) addEventListener(t, f, true);
  dialOff = () => { for (const [t, f] of on) removeEventListener(t, f, true); };
  mark(null);
}
function closeDial() { dial.hidden = true; if (dialOff) { dialOff(); dialOff = null; } }
function tip(key, text, ms = 8000) {
  S.tips = S.tips || new Set();
  if (S.tips.has(key)) return;
  S.tips.add(key);
  toast('info', text, { ms });
}
/* right: a prompt for the same agent. The card is a step of that agent: when it runs the prompt
   is queued in the agent's own session, and the answer is this card's result. */
async function spawnStep(fromId, at) {
  const src = S.nodes.get(fromId), host = hostOf(src); if (!host) return null;
  const p = freeSpot(at.x, at.y);
  const node = { agent: 'step', host: host.id, x: p.x, y: p.y, draft: true, prompt: '', addenda: host.addenda || [] };
  const r = await req('node_add', { node });
  if (!r.ok) { toast('err', r.error || 'could not add that'); return null; }
  send('edge_add', { from: fromId, to: r.id });
  await whenNode(r.id);
  select([r.id]); reveal(r.id); editAsk(r.id);
  tip('step', `This prompt is asked of ${host.name} again, in its own session. The answer comes back to this card and can go on from here.`);
  return r.id;
}
/* down: a fresh agent takes over. The old one writes the note when you press enter. */
async function spawnHandoff(fromId, at) {
  const src = S.nodes.get(fromId), host = hostOf(src); if (!host) return null;
  const ag = agentFor(host), ac = S.cfg.agents[ag] || {};
  const p = freeSpot(at.x, at.y);
  const node = { agent: ag, x: p.x, y: p.y, draft: true, takeover: host.id,
    model: ac.model_arg ? tuned(ag, 'model') : '', effort: ac.effort_arg ? tuned(ag, 'effort') : '', addenda: addendaIds() };
  if (ac.yolo_arg) node.yolo = yoloOf(host);
  const r = await req('node_add', { node });
  if (!r.ok) { toast('err', r.error || 'could not add that'); return null; }
  await whenNode(r.id);
  select([r.id]); reveal(r.id); editAsk(r.id);
  tip('handoff', `A fresh ${ag} takes over from ${host.name}. When you press enter ${host.name} writes it a handover note first; the files stay as they are.`);
  return r.id;
}
/* up: git. A commit is one agent's work; the commits meet in one push; a PR comes after it.
   Each step does what the steps before it would have done, so an agent can go straight to "open PR". */
async function addGit(fromId, at, op) {
  const p = freeSpot(at.x, at.y);
  const r = await req('node_add', { node: { agent: 'git', git_op: op, x: p.x, y: p.y }, from: fromId });
  if (!r.ok) { toast('err', r.error || 'could not add that'); return null; }
  await whenNode(r.id);
  return r.id;
}
async function spawnGit(fromId, at, op) {
  const id = await addGit(fromId, at, op);
  if (!id) return null;
  select([id]); reveal(id);
  if (op === 'commit') tip('commit', 'Commit: the agent it is linked from is asked to commit its own work. Another agent gets its own commit; link the commits to one push.');
  else if (op === 'push') tip('push', 'Push: waits until every commit linked into it has resolved, then pushes. To look first, put an approve check on the link into it.');
  else tip('pr', 'Open PR: after the push, an agent that did the work writes the title and description and opens it. Chain an agent after it to review or test.');
  return id;
}
function gitMenu(c) {
  const ev = c.ev || { clientX: innerWidth / 2, clientY: innerHeight / 2 };
  popup([
    { cap: 'git: the agent is told what to do' },
    { label: 'commit', hint: 'this agent commits its own work', run: () => spawnGit(c.id, c.at, 'commit').then(c.made) },
    { label: 'push', hint: 'commit, then push when every commit is in', run: () => spawnGit(c.id, c.at, 'push').then(c.made) },
    { label: 'open PR', hint: 'commit, push, and have a pull request opened', run: () => spawnGit(c.id, c.at, 'pr').then(c.made) },
  ], ev.clientX, ev.clientY);
}
/* a link into a commit that already has an agent: that agent gets its own commit, which joins the
   same push, instead of the commit trying to be two agents' work. Returns the card the link goes
   into (that new commit, if so), null if there is no link. `skip`: a link that is going away;
   `each`: a "for each" link (src/engine/each.rs). */
async function linkNodes(from, to, on = 'done', skip = null, each = false) {
  const tn = S.nodes.get(to), fn = S.nodes.get(from);
  if (tn && fn && kindOf(tn) === 'git' && tn.git_op === 'commit' && incoming(to).some(e => e.from !== from && e.id !== skip)) {
    const after = [...S.edges.values()].filter(e => e.from === to).map(e => e.to);
    const id = await addGit(from, { x: tn.x, y: tn.y + nh(tn) + 70 }, 'commit');
    if (!id) return null;
    if (tn.branch) patchNode(id, { branch: tn.branch }, 0);
    for (const a of after) send('edge_add', { from: id, to: a });
    toast('info', `A commit takes one agent's work, so ${hostOf(fn).name} got its own commit${after.length ? ' and joins the same push' : ''}.`, { ms: 7000 });
    return id;
  }
  const r = await req('edge_add', { from, to, on, each });
  if (!r.ok) toast('warn', r.error || 'could not link that');
  return r.ok ? to : null;
}

/* grab a link and drag it: the end nearer to where it was grabbed comes loose and goes where it is
   let go, onto a card or into empty space (the dial), as drawing a new link there would. Alt keeps
   the old link too (a copy). The new link is made first, so a card that waits for all its inputs is
   never one short in between; only a commit, which takes one agent, lets go of the old one first. */
function grabLink(ev, id) {
  const e = S.edges.get(id), a = e && S.nodes.get(e.from); if (!a) return;
  const p = toWorld(ev.clientX, ev.clientY), t = targetPoint(e);
  const head = Math.hypot(p.x - t.x, p.y - t.y) <= Math.hypot(p.x - a.x - nw(a), p.y - a.y - PY);
  startConnect(ev, head ? e.from : e.to, head ? 'out' : 'in', id);
}
const dimLink = (id, on) => { const el = edgeEls.get(id); if (el) el.g.classList.toggle('moving', on); };
async function moveLink(old, from, to, copy) {
  const solo = !copy && to === old.to && kindOf(S.nodes.get(to)) === 'git' && S.nodes.get(to).git_op === 'commit';
  const had = [...S.edges.values()].some(x => x.from === from && x.to === to);
  if (solo) send('edge_del', { id: old.id });
  const made = await linkNodes(from, to, old.on, solo ? old.id : null, !!old.each);
  if (!made) { if (solo) send('edge_add', { from: old.from, to: old.to, on: old.on, each: !!old.each }); return; }
  movedLink(old, from, made, copy, solo, had && made === to);
}
/* the new link from..to is there: the old one goes, and one ctrl+z puts it back (a link that was
   there already, `had`, stays) */
function movedLink(old, from, to, copy, gone = false, had = false) {
  if (!copy && !gone) send('edge_del', { id: old.id });
  const back = () => send('edge_add', { from: old.from, to: old.to, on: old.on, each: !!old.each });
  const drop = () => { const e = !had && [...S.edges.values()].find(x => x.from === from && x.to === to); if (e) send('edge_del', { id: e.id }); };
  S.undo.push(() => { if (copy) return drop(); if (gone) { drop(); back(); } else { back(); drop(); } });
  const card = S.nodes.get(to !== old.to ? to : from);
  toast('info', `link ${copy ? 'copied' : 'moved'} to ${card ? card.name : 'the new card'}`, { ms: 6000, action: { label: 'undo', run: undo } });
}

/* the connect gesture: click a diamond = what next; drag onto an agent = link; let go in empty space = the dial */
const rubber = $('#rubber'), ghost = $('#ghost');
function agentFor(src) {
  const n0 = typeof src === 'string' ? S.nodes.get(src) : src;
  const n = hostOf(n0);
  if (n && kindOf(n) === 'agent' && (S.cfg.agents[n.agent] || n.agent === 'sh')) return n.agent;
  return S.lastAgent;
}
/* move: the id of a link whose loose end this drag is (`grabLink`) */
function startConnect(e, fromId, side, move = null) {
  if (e.button !== 0) return;
  e.preventDefault(); e.stopPropagation(); closeMenu(); closeFly();
  const n = S.nodes.get(fromId);
  const p0 = side === 'out' ? { x: n.x + nw(n), y: n.y + PY } : { x: n.x, y: n.y + PY };
  const sx = e.clientX, sy = e.clientY;
  let moved = false, over = null, last = e;
  const old = move && S.edges.get(move), loose = old ? (side === 'out' ? old.to : old.from) : null;
  // (a hand off is no link, so a link's end cannot become one)
  const opts = side === 'out' && n ? dialFor(n).filter(o => !old || o.key !== 'handoff') : [];
  const nodeAt = ev => { const t = document.elementFromPoint(ev.clientX, ev.clientY); const nd = t && t.closest && t.closest('.nw'); return nd ? nd.dataset.id : null; };
  const mv = ev => {
    last = ev;
    if (!moved && Math.hypot(ev.clientX - sx, ev.clientY - sy) < 5) return;
    if (!moved) { moved = true; rubber.style.display = 'block'; $('#app').classList.add('connecting'); if (old) dimLink(move, true); }
    const p = toWorld(ev.clientX, ev.clientY);
    const [a, b] = side === 'out' ? [p0, p] : [p, p0];
    rubber.setAttribute('d', pathBetween(a.x, a.y, b.x, b.y));
    const t = nodeAt(ev);
    if (t !== over) {
      if (over && nodeEls.get(over)) nodeEls.get(over).root.classList.remove('drop');
      over = t && t !== fromId && t !== loose ? t : null;
      if (over) nodeEls.get(over).root.classList.add('drop');
    }
    // show where a new node would land
    if (!t) {
      let label;
      if (ev.altKey) label = 'choose type…';
      else if (opts.length > 1) label = 'let go here';
      else if (opts.length) label = opts[0].label;
      else label = '+ ' + agentTag(agentFor(fromId));
      ghost.style.display = 'block';
      ghost.textContent = label;
      ghost.style.left = (side === 'out' ? p.x : p.x - NW) + 'px'; ghost.style.top = (p.y - PY) + 'px';
    } else ghost.style.display = 'none';
  };
  const up = ev => {
    removeEventListener('pointermove', mv);
    $('#app').classList.remove('connecting');
    if (over && nodeEls.get(over)) nodeEls.get(over).root.classList.remove('drop');
    const t = moved ? nodeAt(ev) : null;
    if (!(moved && !t && !ev.altKey && side === 'out' && opts.length > 1)) { rubber.style.display = 'none'; ghost.style.display = 'none'; if (old) dimLink(move, false); }
    if (!moved) { // a plain click on the diamond: what comes next?
      if (old) return selectEdge(move);
      const nn_ = S.nodes.get(fromId);
      if (side === 'out' && nn_ && kindOf(nn_) === 'agent') openNext(fromId);
      else spawnLinked(fromId, side === 'out' ? 'next' : 'prev');
      return;
    }
    const copy = !!old && (e.altKey || ev.altKey);
    const link = (f, to) => old ? moveLink(old, f, to, copy) : linkNodes(f, to);
    if (t && t !== fromId) { if (t === loose) return; if (side === 'out') link(fromId, t); else link(t, fromId); return; }
    if (t) return;
    const w = toWorld(ev.clientX, ev.clientY);
    const at = { x: side === 'out' ? w.x : w.x - NW, y: w.y - PY };
    // a link's end let go here: the card made here takes its place (and its condition)
    const made = id => {
      if (!old || !id) return;
      const [f, to] = side === 'out' ? [fromId, id] : [id, fromId];
      if (old.on !== 'done' || old.each) send('edge_add', { from: f, to, on: old.on, each: !!old.each });
      movedLink(old, f, to, copy);
    };
    const make = agent => spawnAt(agent, at.x, at.y, side === 'out' ? { from: fromId } : { to: fromId }).then(made);
    if (ev.altKey) return typeMenu(ev.clientX, ev.clientY, make);
    const run = (o, e2) => Promise.resolve(o.run({ id: fromId, at: { x: w.x, y: w.y - PY }, ev: e2 || ev, made })).then(made);
    if (side === 'out' && opts.length === 1) return run(opts[0]);
    // the line and the card's outline stay where it was let go until an option is picked
    if (side === 'out' && opts.length) return openDial(ev.clientX, ev.clientY, opts,
      (o, e2) => { rubber.style.display = 'none'; ghost.style.display = 'none'; if (old) dimLink(move, false); if (o) run(o, e2); },
      () => { ghost.textContent = ''; });
    make(agentFor(fromId));
  };
  addEventListener('pointermove', mv);
  addEventListener('pointerup', up, { once: true });
}

/* the bar for a selected link */
const edgeBar = $('#edgebar');
function selectEdge(id) {
  S.selEdge = id; S.sel = new Set();
  for (const el of nodeEls.values()) el.root.classList.remove('sel');
  for (const e of S.edges.values()) styleEdge(e);
  buildEdgeBar(); renderOmni(); renderQueue();
}
function buildEdgeBar() {
  edgeBar.replaceChildren();
  const e = S.selEdge && S.edges.get(S.selEdge);
  if (!e) { edgeBar.hidden = true; return; }
  const src = S.rt.get(e.from), s = S.nodes.get(e.from), t = S.nodes.get(e.to);
  edgeBar.className = 'chamf';
  edgeBar.style.setProperty('--c', '8px'); edgeBar.style.setProperty('--fill', '#080c12'); edgeBar.style.setProperty('--bd', 'var(--cy2)');
  edgeBar.append(
    h('div', { style: 'font:600 calc(11.5px*var(--fs)) var(--display);letter-spacing:.04em;color:var(--i3);padding:8px 10px 4px;white-space:nowrap' }, `${s ? s.name : '?'}  →  ${t ? t.name : '?'}`),
    h('div', { class: 'seg', style: 'margin:0 8px' }, [['done', '›  always'], ['ok', '✓  if it succeeds'], ['fail', '✕  if it fails']].map(([v, label]) =>
      h('button', { 'aria-pressed': String(e.on === v), onclick: () => send('edge_set', { id: e.id, on: v }) }, label))),
    S.cfg.each_max ? h('div', { style: 'display:flex;padding:6px 8px 0' }, h('button', { class: 'btn sm', 'aria-pressed': String(!!e.each),
      title: 'the next agent is copied once per job this card lists; what comes after waits for all of them',
      onclick: () => setEach(e.id, !e.each) }, '\u2200  for each job it lists')) : '',
    h('div', { style: 'padding:6px 10px;max-width:400px;font:calc(11.5px*var(--fs))/1.45 var(--mono);color:var(--i2);white-space:pre-wrap' },
      src && src.out ? 'carries: ' + (src.out.length > 140 ? src.out.slice(0, 140) + '…' : src.out) : 'carries nothing yet'),
    h('div', { style: 'display:flex;gap:6px;padding:0 8px 8px' },
      h('button', { class: 'btn sm', onclick: () => send('handover', { from: e.from, to: e.to }) }, 'hand over again'),
      h('button', { class: 'btn sm', title: 'double-click the link does this too', onclick: () => spliceInto(e.id, 'agent') }, 'insert step'),
      h('button', { class: 'btn sm danger', onclick: () => delEdge(e.id) }, 'remove')));
  edgeBar.hidden = false;
  placeEdgeBar();
}
function placeEdgeBar() {
  if (edgeBar.hidden || !S.selEdge) return;
  const el = edgeEls.get(S.selEdge); if (!el || !el.mid) return;
  edgeBar.style.position = 'absolute'; edgeBar.style.left = '0'; edgeBar.style.top = '0'; edgeBar.style.zIndex = '12';
  edgeBar.style.transformOrigin = '0 0';
  edgeBar.style.transform = `translate(${el.mid.x}px,${el.mid.y}px) scale(${1 / S.v.k}) translate(-50%,-112%)`;
}

/* ------------------------------------------------------------ selection */
function select(ids) {
  if (ids.length !== 1 || !S.sel.has(ids[0])) S.omniMode = 'then';
  S.sel = new Set(ids); S.selEdge = null;
  for (const [id, el] of nodeEls) el.root.classList.toggle('sel', S.sel.has(id));
  for (const e of S.edges.values()) styleEdge(e);
  buildEdgeBar();
  if (S.fly) { if (S.sel.size === 1 && S.sel.has(S.fly.id) === false) { S.fly.id = primary(); renderFly(); } else if (S.sel.size !== 1) closeFly(); }
  renderOmni(); renderQueue();
}
