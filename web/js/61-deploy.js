/* ------------------------------------------------------------ deploy
   The step after a push that puts what was pushed to use: a command card (`n.deploy`) that runs on a
   checkout of exactly the pushed commit (engine/deploy.rs) and waits to be armed, like the push. Its
   command is the folder's: what you typed for it last, else what the daemon guessed from the folder. */
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
  const r = await req('node_add', { node: { agent: 'sh', name: 'deploy', cmd, finish: 'exit', deploy: 'pushed', draft: true, x: p.x, y: p.y }, from: fromId });
  if (!r.ok) { toast('err', r.error || 'could not add that'); return null; }
  await whenNode(r.id);
  select([r.id]); reveal(r.id);
  // no command known for this folder: say it right there
  if (!cmd) openFly(r.id, 'options');
  tip('deploy', 'Deploy: runs its command on a checkout of exactly what the push before it pushed, never on unfinished work in the folder. It is disarmed until you arm it; red goes to "failed", where a fixer can take it.');
  return r.id;
}
/* the command typed into a deploy card is the folder's from now on */
addEventListener('change', e => {
  const t = e.target, id = S.fly && S.fly.id, n = id && S.nodes.get(id);
  if (n && n.deploy && t && t.getAttribute && t.getAttribute('aria-label') === 'Command') rememberDeploy(workDir(n), t.value.trim());
}, true);
/* a deploy card says what it does in its own words */
function deployText(n, r) {
  const st = r.status;
  if (st === 'draft') return readyCards().includes(n.id) ? `ready · ${readyWhat(n, r)}` : 'disarmed · arm to deploy';
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
