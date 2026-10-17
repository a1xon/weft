/* ------------------------------------------------------------ menus */
let menuEl = null, menuOff = null;
function closeMenu() {
  if (menuEl) { menuEl.remove(); menuEl = null; }
  if (menuOff) { menuOff(); menuOff = null; }
}
function popup(items, x, y, opts = {}) {
  closeMenu();
  const m = h('div', { class: 'menu', role: 'menu' }, items.map(it =>
    it.sep ? h('div', { class: 'sep' }) : it.cap ? h('div', { class: 'cap' }, it.cap)
      : h('button', { class: 'mi' + (it.danger ? ' danger' : ''), role: 'menuitem', onclick: () => { closeMenu(); it.run(); } },
          it.k ? h('span', { class: 'k' }, it.k) : null, it.label, it.hint ? h('span', { class: 'hint' }, it.hint) : null)));
  document.body.append(m);
  const r = m.getBoundingClientRect();
  let top = opts.above ? y - r.height - 8 : y;
  top = Math.max(8, Math.min(top, innerHeight - r.height - 8));
  m.style.left = Math.max(8, Math.min(x, innerWidth - r.width - 8)) + 'px'; m.style.top = top + 'px';
  menuEl = m;
  // a click outside closes this menu; the listener goes away with the menu, so a menu that was
  // rebuilt in place (its list arrived) is not closed by the one it replaced
  const outside = ev => { if (!m.contains(ev.target)) closeMenu(); };
  setTimeout(() => {
    if (menuEl !== m) return;
    addEventListener('pointerdown', outside, true);
    menuOff = () => removeEventListener('pointerdown', outside, true);
  }, 0);
  return m;
}
function typeItems(onPick, withKinds = true) {
  return [
    ...agentEntries().map(([k, a]) => ({ label: a.label || k, hint: agentTag(k), run: () => onPick(k) })),
    ...(withKinds ? [{ sep: true }, ...TYPES.map(t => ({ label: t.label, hint: t.hint, run: () => onPick(t.k) }))] : []),
  ];
}
function typeMenu(x, y, onPick) { popup([{ cap: 'place here' }, ...typeItems(onPick)], x, y); }
function canvasMenu(x, y) {
  const w = toWorld(x, y), at = [w.x - NW / 2, w.y - 30];
  popup([
    { cap: 'place here' },
    ...typeItems(k => spawnHere(k, at[0], at[1])),
    { sep: true },
    { label: 'run flow', run: () => send('run') },
    { label: 'tidy up', hint: 't', run: tidy },
    { label: 'fit everything', hint: 'f', run: fit },
    { label: 'library', hint: 'networks, prompts', run: () => openLibrary() },
    { label: 'undo', hint: 'ctrl+z', run: undo },
  ], x, y);
}
function portMenu(id, side, x, y) {
  popup([{ cap: side === 'out' ? 'add a step after this' : 'add a step before this' },
    ...typeItems(k => spawnLinked(id, side === 'out' ? 'next' : 'prev', k))], x, y);
}
function nodeMenu(id, x, y) {
  const n = S.nodes.get(id), r = S.rt.get(id), k = kindOf(n);
  const items = [];
  if (r && r.status === 'failed' && S.fixOn) items.push(...fixItems(id));
  if (k === 'agent' && n.agent === 'step') {
    const host = hostOf(n);
    items.push(
      { label: 'ask again', hint: 'send this prompt to ' + host.name + ' once more', run: () => runNode(id) },
      { label: 'open ' + host.name + "'s terminal", run: () => openFocus(host.id) },
      { sep: true },
      { label: 'hand over to a fresh agent', run: () => spawnHandoff(id, { x: n.x + nw(n) + 150, y: n.y }) },
      { label: 'commit what ' + host.name + ' did', run: () => spawnGit(id, { x: n.x + nw(n) + 150, y: n.y }, 'commit') },
    );
  } else if (k === 'agent') {
    items.push(
      { label: 'open terminal', hint: 'enter', run: () => openFocus(id) },
      { label: 'read the result', hint: 'r', run: () => openFly(id, 'result') },
      { label: 'options', hint: ',', run: () => openFly(id, 'options') },
      { cap: 'watch it' },
      { k: hasShotWatch(n) ? '\u25cf' : ' ', label: hasShotWatch(n) ? 'screenshots: ask for one now' : 'screenshots', run: () => hasShotWatch(n) ? send('check_now', { node: id, check: n.checks.find(c => c.kind === 'shot').id }) : attachCheck(id, 'shot') },
      { k: hasEta(n) ? '\u25cf' : ' ', label: 'ETA countdown', hint: hasEta(n) ? 'remove' : '', run: () => toggleCheck(id, 'eta') },
      { k: (n.checks || []).some(c => c.kind === 'changes') ? '\u25cf' : ' ', label: 'what it changed', hint: (n.checks || []).some(c => c.kind === 'changes') ? 'remove' : '', run: () => toggleCheck(id, 'changes') },
      { k: hasPing(n) ? '\u25cf' : ' ', label: 'ping me when it is done', hint: hasPing(n) ? 'remove' : '', run: () => toggleCheck(id, 'ping') },
      { sep: true },
      { label: n.draft ? 'run it' : r && r.alive ? 'send prompt now' : 'run', run: () => runNode(id) },
      { label: 'restart agent', run: () => send('restart', { id }) },
      { label: 'stop', run: () => send('stop', { id }) },
      { sep: true },
      { cap: 'and then\u2026' },
      { label: 'then: queue a prompt on this agent', hint: 'click \u25c7', run: () => openNext(id, 'then') },
      { label: 'hand over to a fresh agent', hint: 'new context, same files', run: () => openNext(id, 'hand') },
      { label: 'branch: a new agent gets the result', run: () => openNext(id, 'branch') },
      { sep: true },
      { label: 'fork (parallel copy)', hint: 'same inputs', run: () => fork(id) },
      { label: 'clone', hint: 'alt-drag', run: () => cloneAt(id) },
      { sep: true },
      { label: 'mark done', run: () => send('done', { id, ok: true, text: '' }) },
      { label: 'mark failed', run: () => send('done', { id, ok: false, text: '' }) },
      { label: 'keep result as a note', run: () => keepAsNote(id) },
      { label: 'forget waiting inputs', run: () => send('clear_inputs', { id }) },
      { sep: true },
      { cap: 'convert to' },
      ...agentEntries().map(([kk, a]) => ({ k: n.agent === kk ? '●' : ' ', label: a.label || kk, run: () => convertTo(id, kk) })),
    );
  } else if (isGate(k)) {
    items.push(
      { label: k === 'gate' ? 'approve' : 'confirm', run: () => send('approve', { id, ok: true }) },
      ...(k !== 'gate' ? [{ label: 'request changes\u2026', run: () => askChanges(id) },
        { label: k === 'shot' ? 'retake the screenshot' : 'run it again', run: () => send('gate_retry', { id }) }] : []),
      ...(k === 'tests' ? [{ label: 'send the failure to the agent', run: () => send('gate_retry', { id, fix: true }) }] : []),
      { label: 'reject', run: () => send('approve', { id, ok: false }) },
      { sep: true },
      { label: 'options', hint: ',', run: () => openFly(id, 'options') },
      { label: 'add next step', run: () => spawnLinked(id, 'next') },
    );
  } else if (k === 'git') {
    const verb = gitOpLabel(n);
    items.push(
      { label: n.draft ? `arm: ${verb} as results arrive` : 'disarm', run: () => n.draft ? send('arm', { id }) : patchNode(id, { draft: true }, 0) },
      { label: n.git_op === 'commit' ? 'commit now' : n.git_op === 'push' ? 'push now' : n.git_op === 'pr' ? 'open PR now' : 'ship now', hint: 'with what is there', run: () => send('fire', { id }) },
      { label: 'options', hint: ',', run: () => openFly(id, 'options') },
      { sep: true },
      { label: 'add next step', run: () => spawnLinked(id, 'next') },
    );
  } else {
    items.push(
      { label: 'edit text', run: () => editNote(id) },
      { label: 'add an agent that uses it', run: () => spawnLinked(id, 'next') },
    );
  }
  items.push({ sep: true }, ...archItems(id), { label: 'delete', danger: true, hint: 'del', run: () => deleteNodes([id]) });
  popup(items, x, y);
}
function multiMenu(x, y) {
  const ids = [...S.sel];
  popup([
    { cap: `${ids.length} selected` },
    { label: 'gather into one agent', hint: 'g', run: () => gather(ids) },
    { label: 'commit each, then push', hint: 'one commit per agent, one push', run: () => shipThese(ids) },
    { label: 'save as network\u2026', hint: 'reuse these cards and links', run: () => saveNetFrom(ids, x, y) },
    ...talkMulti(ids),
    { sep: true },
    { label: 'run all', run: () => ids.forEach(i => runNode(i)) },
    { label: 'stop all', run: () => ids.forEach(i => send('stop', { id: i })) },
    { label: 'tidy up', run: tidy },
    { sep: true },
    ...archMulti(ids),
    { label: `delete ${ids.length}`, danger: true, hint: 'del', run: () => deleteNodes(ids) },
  ], x, y);
}
function edgeMenu(id, x, y) {
  const e = S.edges.get(id); if (!e) return;
  popup([
    { cap: 'this link fires' },
    { k: e.on === 'done' ? '●' : ' ', label: 'always, when the source finishes', run: () => send('edge_set', { id, on: 'done' }) },
    { k: e.on === 'ok' ? '●' : ' ', label: 'only if the source succeeds', run: () => send('edge_set', { id, on: 'ok' }) },
    { k: e.on === 'fail' ? '●' : ' ', label: 'only if the source fails', run: () => send('edge_set', { id, on: 'fail' }) },
    { sep: true },
    { label: 'hand the result over again', run: () => send('handover', { from: e.from, to: e.to }) },
    { label: 'insert a step here', hint: 'double-click', run: () => spliceInto(id, 'agent') },
    { cap: 'put a gate on this link' },
    { label: 'approve', hint: 'you confirm first', run: () => spliceInto(id, 'gate') },
    { label: 'check', hint: 'a command must pass', run: () => spliceInto(id, 'check') },
    { label: 'screenshot', hint: 'agent shows its work', run: () => spliceInto(id, 'shot') },
    { label: 'tests', hint: 'written, run, fixed', run: () => spliceInto(id, 'tests') },
    { sep: true },
    { label: 'remove link', danger: true, hint: 'del', run: () => delEdge(id) },
  ], x, y);
}

/* ------------------------------------------------------------ making things */
/* where a new card fits without overlapping another (and without covering a junction).
   lane: a card that starts working now keeps its row free to the right of it (no card that runs
   or has run there), so what is queued after it lines up next to it */
const filmH = id => { const el = nodeEls.get(id); return el && el.under ? el.under.offsetHeight + 12 : 0; };
function freeSpot(x, y, lane) {
  const box = n => ({ x: n.x - JX - 20, y: n.y - 14, w: nw(n) + JX + 40, h: nh(n) + 28 + filmH(n.id) });
  const boxes = [...S.nodes.values()].map(box);
  const started = lane ? [...S.nodes.values()].filter(n => cat((S.rt.get(n.id) || {}).status) !== 'pre').map(box) : [];
  const overlaps = (cx, cy) => boxes.some(b => cx - JX < b.x + b.w && cx + NW > b.x && cy < b.y + b.h && cy + NH > b.y)
    || started.some(b => b.x > cx && cy < b.y + b.h && cy + NH > b.y);
  const snap = v => Math.round(v / 10) * 10;
  x = snap(x); y = snap(y);
  if (!overlaps(x, y)) return { x, y };
  // the nearest spot that is free: the card is pushed just past an edge of a card in the way
  const xs = new Set([x]), ys = new Set([y]);
  for (const b of boxes) {
    xs.add(Math.ceil((b.x + b.w + JX) / 10) * 10); xs.add(Math.floor((b.x - NW) / 10) * 10);
    ys.add(Math.ceil((b.y + b.h) / 10) * 10); ys.add(Math.floor((b.y - NH) / 10) * 10);
  }
  const cands = [];
  for (const cx of xs) for (const cy of ys) cands.push([cx, cy, (cx - x) ** 2 + (cy - y) ** 2]);
  cands.sort((a, b) => a[2] - b[2]);
  for (const [cx, cy] of cands) if (!overlaps(cx, cy)) return { x: cx, y: cy };
  return { x, y: snap(Math.max(...boxes.map(b => b.y + b.h))) };
}
function setAgent(k) {
  if (!S.cfg.agents[k]) return;
  S.lastAgent = k; S.prefs.agent = k; savePrefs();
  try { localStorage.setItem('weft-agent', k); } catch {}
  renderOmni();
  if (pad.open) { renderPad(); ensureModels(k); }
}

/* every node made by gesture is a draft: nothing runs until you press enter */
/* "git:commit", "git:push" and "git:pr" are the git node with an op */
function splitKind(k) {
  if (k === 'check') return ['tests', checkGate()];
  const m = /^git:(commit|push|pr)$/.exec(k || '');
  return m ? ['git', { git_op: m[1] }] : [k, {}];
}
async function spawnAt(agent0, x, y, opts = {}) {
  const [agent, kindFields] = splitKind(agent0);
  const p = freeSpot(x, y, opts.lane);
  const k = KINDS.includes(agent) ? agent : 'agent';
  const node = { agent, x: p.x, y: p.y, ...kindFields };
  if (k === 'agent') node.draft = opts.draft !== false;
  if (agent === 'sh') node.finish = 'exit';
  if (k === 'agent' && agent !== 'sh') {
    const ac = S.cfg.agents[agent] || {}, ex = opts.extra || {};
    node.model = ex.model ?? (ac.model_arg ? tuned(agent, 'model') : '');
    node.effort = ex.effort ?? (ac.effort_arg ? tuned(agent, 'effort') : '');
    node.addenda = ex.addenda ?? addendaIds();
    if (ac.yolo_arg) node.yolo = ex.yolo ?? yoloDefault();
  }
  if (opts.text) { if (agent === 'sh') node.cmd = opts.text; else node.prompt = opts.text; }
  // where it works: the folder chosen for new agents (linked after a card, the daemon takes that card's)
  if (k === 'agent' && agent !== 'step' && !opts.from) {
    const d = opts.extra && 'cwd' in opts.extra ? opts.extra.cwd : cwdField(newDir());
    if (d) node.cwd = d;
  }
  const r = await req('node_add', { node, from: opts.from || null, run: !!opts.run });
  if (!r.ok) { toast('err', r.error || 'could not add that'); return null; }
  if (opts.to) send('edge_add', { from: r.id, to: opts.to });
  if (k === 'agent') setAgent(agent);
  S.lastNode = r.id;
  await whenNode(r.id);
  select([r.id]);
  reveal(r.id);
  if (opts.edit !== false) {
    if (k === 'agent') editAsk(r.id);
    else if (k === 'note') editNote(r.id);
    else if (k === 'git') toast('info', kindFields.git_op === 'commit' ? 'commit: link the agent whose work it commits. Another agent gets its own commit; link them all to one push.'
      : kindFields.git_op === 'pr' ? 'open PR: link the push before it, then press arm. An agent that did the work writes and opens it.'
        : 'push: link commits (or agents) into it, then press arm. It waits for all of them.', { ms: 7000 });
    else if (k === 'shot') toast('info', 'screenshot gate: it asks the agent upstream, shows the image here and waits for you.', { ms: 7000 });
    else if (k === 'tests' && kindFields.write_tests === false) toast('info', kindFields.cmd ? `check: weft runs \`${kindFields.cmd}\`; red goes back to the agent. Change it in options.` : 'check: the agent runs the checks; red goes back to it. Set a command in options.', { ms: 8000 });
    else if (k === 'tests') toast('info', 'tests gate: the agent writes and runs tests. Set a command in options to run the suite yourself.', { ms: 8000 });
  }
  return r.id;
}
const spawnHere = (agent, x, y) => spawnAt(agent, x, y, {});
function spawnLinked(fromId, dir, agent) {
  const src = S.nodes.get(fromId); if (!src) return;
  const ag = agent || agentFor(src);
  const x = dir === 'next' ? src.x + nw(src) + 150 : src.x - NW - 150;
  return spawnAt(ag, x, src.y, dir === 'next' ? { from: fromId } : { to: fromId });
}
async function cloneAt(id) {
  const n = S.nodes.get(id); if (!n) return;
  const c = { ...n }; delete c.id; c.name = ''; c.auto_name = false; c.x = n.x + 40; c.y = n.y + nh(n) + 30;
  const r = await req('node_add', { node: c });
  if (r.id) { await whenNode(r.id); select([r.id]); }
}
/* links arrive one by one, and a finished card hands its result over the moment it is linked: a
   card that gets several links back waits as a draft until all are in, then starts once with all */
const holdsLinks = (n, links) => !n.takeover && links > 1 && ['agent', 'step'].includes(kindOf(n));
/* a parallel copy that receives the same inputs */
async function fork(id) {
  const n = S.nodes.get(id); if (!n) return;
  const inc = incoming(id), hold = holdsLinks(n, inc.length);
  const c = { ...n }; delete c.id; c.name = ''; c.auto_name = false; c.x = n.x; c.y = n.y + nh(n) + 40; c.draft = hold;
  const p = freeSpot(c.x, c.y); c.x = p.x; c.y = p.y;
  const r = await req('node_add', { node: c });
  if (!r.id) return;
  for (const e of inc) send('edge_add', { from: e.from, to: r.id, on: e.on });
  if (hold) send('arm', { id: r.id });
  await whenNode(r.id); select([r.id]);
}
async function keepAsNote(id) {
  const r = await req('result', { id });
  if (!r.text) return toast('info', 'no result to keep yet');
  const n = S.nodes.get(id);
  const nid = await spawnAt('note', n.x, n.y + nh(n) + 40, { text: r.text, edit: false });
  if (nid) toast('ok', 'saved as a note you can link into any agent');
}
/* splice a new step into a link: A -> B becomes A -> N -> B */
async function spliceInto(edgeId, agent) {
  const e = S.edges.get(edgeId); if (!e) return;
  const a = S.nodes.get(e.from), b = S.nodes.get(e.to); if (!a || !b) return;
  const [ag, kindFields] = splitKind(agent === 'agent' ? agentFor(a) : agent);
  const at = { x: (a.x + nw(a) + b.x) / 2 - NW / 2, y: Math.max(a.y + nh(a), b.y + nh(b)) + 40 };
  const p = freeSpot(at.x, at.y);
  const node = { agent: ag, x: p.x, y: p.y, ...kindFields };
  if (kindOf({ agent: ag }) === 'agent') node.draft = true;
  const r = await req('node_add', { node });
  if (isGate(ag)) toast('info', `${(TYPES.find(t => t.k === agent) || TYPES.find(t => t.k === ag) || {}).label || ag} gate spliced in: nothing past it starts until it is solved.`, { ms: 5000 });
  if (!r.id) return;
  send('edge_del', { id: edgeId });
  send('edge_add', { from: e.from, to: r.id });
  send('edge_add', { from: r.id, to: e.to, on: e.on });
  await whenNode(r.id);
  select([r.id]);
  if (kindOf({ agent: ag }) === 'agent') editAsk(r.id);
}
/* several results into one agent: a visible Promise.all */
async function gather(ids) {
  const ns = ids.map(i => S.nodes.get(i)).filter(Boolean); if (ns.length < 2) return;
  const x = Math.max(...ns.map(n => n.x + nw(n))) + 160, y = Math.round(ns.reduce((s, n) => s + n.y, 0) / ns.length);
  const nid = await spawnAt(agentFor(ns[0]), x, y, { edit: true });
  if (!nid) return;
  for (const n of ns) send('edge_add', { from: n.id, to: nid });
}
async function shipThese(ids) {
  const ns = ids.map(i => S.nodes.get(i)).filter(n => n && kindOf(n) === 'agent'); if (!ns.length) return;
  const x = Math.max(...ns.map(n => n.x + nw(n))) + 150, y = Math.round(ns.reduce((t, n) => t + n.y, 0) / ns.length);
  // one commit per agent, and the commits meet in one push
  const commits = [];
  for (const n of ns) { const c = await addGit(n.id, { x, y: n.y }, 'commit'); if (c) commits.push(c); }
  if (!commits.length) return;
  const push = await addGit(null, { x: x + NW + 150, y }, 'push');
  if (!push) return;
  for (const c of commits) send('edge_add', { from: c, to: push });
  select([push]); reveal(push);
  tip('push', 'One commit per agent; the push waits until every commit has resolved. It is disarmed until you arm it.');
}
