/* ------------------------------------------------------------ deploy
   The step after a push that puts what was pushed to use: a command card (`n.deploy`) that runs on a
   checkout of exactly the pushed commit (engine/deploy.rs) as soon as the push is in (to look first, a
   check goes on the link). Its command is the folder's: what you typed for it last, else what the
   daemon guessed from the folder; with none known, the card waits until it is typed into its options. */
function deployFor(dir) {
  const d = dir || netProj(), mine = (S.prefs.deploy || {})[d];
  return typeof mine === 'string' ? mine : (netFacts(d).deploy || '');
}
function rememberDeploy(dir, cmd) {
  const d = dir || netProj();
  if (cmd === deployFor(d)) return;
  const m = { ...(S.prefs.deploy || {}) };
  if (cmd === (netFacts(d).deploy || '')) delete m[d]; else m[d] = cmd;
  const ks = Object.keys(m); if (ks.length > 12) delete m[ks[0]];
  S.prefs.deploy = m; savePrefs();
}
async function spawnDeploy(fromId, at) {
  const from = S.nodes.get(fromId); if (!from) return null;
  const dir = netWhere(from), cmd = deployFor(dir), p = freeSpot(at.x, at.y);
  const r = await req('node_add', { node: { agent: 'sh', name: 'deploy', cmd, finish: 'exit', deploy: 'pushed', draft: !cmd, x: p.x, y: p.y }, from: fromId });
  if (!r.ok) { toast('err', r.error || 'could not add that'); return null; }
  await whenNode(r.id);
  select([r.id]); reveal(r.id);
  // no command known for this folder: say it right there
  if (!cmd) openFly(r.id, 'options');
  tip('deploy', 'Deploy: runs its command on a checkout of exactly what the push before it pushed, never on unfinished work in the folder, as soon as the push is in. To look first, put an approve check on the link into it. Red goes to "failed", where a fixer can take it.');
  return r.id;
}
/* the command typed into a deploy card is the folder's from now on, and a card that waited for it goes */
addEventListener('change', e => {
  const t = e.target, id = S.fly && S.fly.id, n = id && S.nodes.get(id);
  if (!(n && n.deploy && t && t.getAttribute && t.getAttribute('aria-label') === 'Command')) return;
  const cmd = t.value.trim();
  rememberDeploy(workDir(n), cmd);
  // (sent now, so it reaches the daemon before the arm does)
  if (cmd && n.draft) { patchNode(id, { cmd }, 0); send('arm', { id }); }
}, true);
/* a deploy card says what it does in its own words */
function deployText(n, r) {
  const st = r.status;
  if (st === 'draft') return (n.cmd || '').trim() ? 'waits \u00b7 arm to deploy' : 'type its command (options) to deploy';
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
