/* ------------------------------------------------------------ deploy
   The step after a push that puts what was pushed to use: a command card (`n.deploy`) that runs on a
   checkout of exactly the pushed commit (engine/deploy.rs) as soon as the push is in (to look first, a
   check goes on the link). A new one is made as the folder's last deploy was (`deployShape`); the first
   runs the folder's command: what you typed for it last, else what the daemon guessed from the folder.
   With none known, the card waits until it is typed into its options. */
async function spawnDeploy(fromId, at) {
  const from = S.nodes.get(fromId); if (!from) return null;
  const dir = workDir(from), cmd = folderCmd('deploy', dir), p = freeSpot(at.x, at.y);
  const like = (S.prefs.deploys || {})[dir];
  const r = like ? await addDeployLike(from, like, p)
    : await req('node_add', { node: { agent: 'sh', name: 'deploy', cmd, finish: 'exit', deploy: 'pushed', draft: !cmd, x: p.x, y: p.y }, from: fromId });
  if (!r.ok) { toast('err', r.error || 'could not add that'); return null; }
  await whenNode(r.id);
  select([r.id]); reveal(r.id);
  // nothing to run known for this folder: say it right there
  if ((S.nodes.get(r.id) || {}).draft) openFly(r.id, 'options');
  tip('deploy', 'Deploy: runs its command on a checkout of exactly what the push before it pushed, never on unfinished work in the folder, as soon as the push is in. To look first, put an approve check on the link into it. Red goes to "failed", where a fixer can take it.');
  return r.id;
}
/* what a deploy card is, to make the next one like it: its agent or command, prompt, model and
   options, what watches it, and the gates on the link into it (the push's side first) */
function deployShape(n) {
  const shape = c => {
    const t = { agent: c.agent, addenda: c.addenda || [] };
    for (const f of NET_FIELDS) { const v = c[f]; if (v !== undefined && v !== null && v !== '' && !(Array.isArray(v) && !v.length)) t[f] = v; }
    // (the number weft gave it to tell it from another goes: the new card gets its own)
    if (c.auto_name) delete t.name; else if (t.name) t.name = t.name.replace(/ \d+$/, '');
    return t;
  };
  const gates = [];
  for (let at = n; gates.length < 4;) {
    const ins = [...S.edges.values()].filter(e => e.to === at.id);
    const g = ins.length === 1 && S.nodes.get(ins[0].from);
    if (!g || !isGate(kindOf(g))) break;
    gates.unshift(shape(g)); at = g;
  }
  return { card: { ...shape(n), deploy: n.deploy }, gates,
    checks: (n.checks || []).map(c => ({ kind: c.kind, subject: c.subject || '', cmd: c.cmd || '', every_s: c.every_s || 0 })) };
}
/* a deploy card that is new or was changed is what its folder's next deploy is made like (kept for 8
   folders, the newest last). The first look after the page opens fills only folders that have none. */
let deploysSeen = null;
function rememberDeploys() {
  const first = !deploysSeen, was = deploysSeen || new Map(), m = { ...(S.prefs.deploys || {}) }, had = new Set(Object.keys(m));
  deploysSeen = new Map();
  let changed = false;
  // (oldest card first: of several in one folder the newest has the last word)
  const num = id => parseInt(id.slice(1), 10) || 0;
  for (const n of [...S.nodes.values()].filter(n => n.deploy).sort((a, b) => num(a.id) - num(b.id))) {
    const s = JSON.stringify(deployShape(n)), d = workDir(n);
    deploysSeen.set(n.id, s);
    if ((first ? had.has(d) : was.get(n.id) === s) || JSON.stringify(m[d]) === s) continue;
    delete m[d]; m[d] = JSON.parse(s); changed = true;
  }
  if (!changed) return;
  // (preferences stay small: the folders deployed longest ago go first)
  for (const k of Object.keys(m)) if (Object.keys(m).length > 8 || JSON.stringify({ ...S.prefs, deploys: m }).length > 7000) delete m[k];
  S.prefs.deploys = m; savePrefs();
}
/* a deploy after `from` made like `like`: its gates, each linked after the one before, then the card
   and what watches it */
async function addDeployLike(from, like, p) {
  let prev = from.id;
  const gy = Math.max(from.y + nh(from), p.y + NH) + 40;
  for (const [i, g] of (like.gates || []).entries()) {
    const q = freeSpot((from.x + nw(from) + p.x) / 2 - NW / 2 + i * (NW + 40), gy);
    const r = await req('node_add', { node: { ...g, x: q.x, y: q.y }, from: prev });
    if (!r.ok) return r;
    prev = r.id;
  }
  const c = like.card || {}, runs = !!(c.cmd || '').trim() || (!!c.agent && c.agent !== 'sh');
  const r = await req('node_add', { node: { ...c, name: c.name || 'deploy', deploy: c.deploy || 'pushed', draft: !runs, x: p.x, y: p.y }, from: prev });
  if (r.ok) await Promise.all((like.checks || []).map(k => req('check_add', { node: r.id, ...k })));
  return r;
}
/* the command typed into a deploy card is the folder's from now on, and a card that waited for it goes
   (one that has its command, or an agent it is handed to, before the push is in starts with the push) */
function deployCmdSet(id, value) {
  const n = S.nodes.get(id); if (!n || !n.deploy) return;
  const cmd = value.trim();
  rememberFolderCmd('deploy', workDir(n), cmd);
  // (sent now, so it reaches the daemon before the arm does)
  if (cmd && n.draft) { patchNode(id, { cmd }, 0); send('arm', { id }); }
}
/* a deploy card says what it does in its own words */
function deployText(n, r) {
  const st = r.status;
  const unset = !(n.cmd || '').trim() && (!n.agent || n.agent === 'sh');
  if (st === 'draft') return unset ? 'type its command (options) to deploy' : 'waits \u00b7 arm to deploy';
  if (st === 'busy' || st === 'starting') return `deploying` + (r.since ? ` ${clock(r.since)}` : '');
  if (st === 'done') return `deployed ${age(r.since)} ago`;
  if (st === 'failed') return `deploy failed ${age(r.since)} ago`;
  if (st === 'waiting') return r.need ? `waits for the push · ${r.arrived.length}/${r.need} in` : 'waits for a push';
  return null;
}
/* in a deploy card's options, instead of "own git worktree": what it runs on */
function deployRow(id, n) {
  return h('div', { class: 'row2' }, seg([
    ['pushed', 'the pushed commit', 'a checkout of exactly what the push before it pushed, kept for the folder: never unfinished work'],
    ['folder', 'the folder as it is', 'for a script that needs files git does not track'],
  ], n.deploy, v => patchNode(id, { deploy: v }, 0)));
}
