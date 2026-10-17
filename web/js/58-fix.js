/* ------------------------------------------------------------ what failed, fixed (engine/fix.rs)
   A click on the top bar's "N failed" (or "fix it" on a failed card): a fresh agent, a reasoning level
   above the ones that failed, finds out why, fixes the cause and runs the cards again. While it works
   on them the chip and the cards say so, and a click shows it. */
function fixerOf(id) {
  for (const n of S.nodes.values()) {
    const st = (S.rt.get(n.id) || {}).status;
    if ((n.fixes || []).includes(id) && st && !['done', 'failed', 'stopped', 'exited'].includes(st)) return n;
  }
  return null;
}
async function fixFailed(ids = []) {
  const r = await req('fix', { ids });
  if (!r.ok) { toast('warn', r.error || 'no agent could be started to fix it'); return; }
  await whenNode(r.id);
  select([r.id]); reveal(r.id);
}
function showFixer(f) { select([f.id]); reveal(f.id); }
/* the chip in the top bar */
function failedChip(count) {
  if (!S.fixOn) return h('span', { class: 'tk rd' }, h('b', {}, count), ' failed');
  const failed = [...S.rt.entries()].filter(([id, r]) => r.status === 'failed' && kindOf(S.nodes.get(id)) !== 'note').map(([id]) => id);
  const fixers = failed.map(fixerOf);
  const all = fixers.length > 0 && fixers.every(Boolean);
  return h('span', {
    class: 'tk rd fx' + (all ? ' on' : ''),
    title: all ? `${fixers[0].id} is fixing it: show it` : 'fix it: a stronger agent finds out why, fixes the cause and runs it again',
    onclick: () => all ? showFixer(fixers[0]) : fixFailed(),
  }, h('b', {}, count), ' failed', h('i', {}, all ? ' · fixing' : ' · fix it'));
}
/* the top of a failed card's menu */
function fixItems(id) {
  const f = fixerOf(id);
  return [
    { cap: 'it failed' },
    f ? { label: 'show ' + f.name, hint: 'it is fixing this', run: () => showFixer(f) }
      : { label: 'fix it', hint: 'a stronger agent finds out why', run: () => fixFailed([id]) },
    { label: 'run it again', hint: 'with the input it had', run: () => req('retry', { id }).then(r => { if (!r.ok) toast('warn', r.error || 'it cannot run again'); }) },
    { sep: true },
  ];
}
