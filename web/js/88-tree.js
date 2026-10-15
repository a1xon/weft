/* ------------------------------------------------------------ the git tree */
/* Bottom right, beside the bar: the commits of the branch where you are (the folder the line above
   the bar names: the selected agent's, else the one new agents start in), drawn as a graph. Live:
   the daemon looks at the folder every few seconds and says when a branch moved, so a commit by
   any agent shows up as it lands. Its name picks another branch or every branch (only what is
   shown changes, nothing is checked out); a click on a commit unfolds its message and files. A
   daemon without the log (an older build) shows none of this.
   What happens to the branch (a commit, a push, a pull, what came in from the remote) is read from
   git's own record of it, whoever did it: the line under the head says the latest, and as it
   happens the tree unfolds for a moment (when it was folded) and shows it moving. The remote is
   asked every minute while this window is in front (one short request; only what moved there is
   fetched), so "↑2 to push", "↓1 to pull" and "pushed" are true of the remote now. Commits not
   pushed yet have hollow amber dots and a bar, the remote's commits that are not here yet dashed
   violet ones. */
const GT = { dir: '', git: {}, key: '', pick: '', dirty: 0, data: null, n: 100, busy: false, again: false, againRemote: false, off: false,
  open: true, unfold: null, info: new Map(), t: 0, seen: new Map(), looked: new Map(), looking: false, peeking: false, peekT: 0,
  plays: [], playUntil: 0, dots: new Map() };
/* the remote is asked this often while the window is in front (the daemon asks it at most every
   8 s for every page and board together); how long an event unfolds a folded tree; one event's turn */
const LOOK_EVERY = 60000, PEEK_MS = 6500, PLAY_MS = 950;
const treeEl = $('#gtree');
try { GT.open = localStorage.getItem('weft-tree') !== 'shut'; } catch {}
const treePick = d => { try { return localStorage.getItem('weft-tree-branch:' + d) || ''; } catch { return ''; } };

/* called with the folder the line above the bar names and what the daemon last saw of it (branch,
   uncommitted files, `refs`): look again when the folder changed or a branch there moved */
function treeFollow(d, g = {}) {
  if (GT.off || !d) return;
  GT.git = g;
  const pick = treePick(d);
  const key = JSON.stringify([d, pick, g.repo, g.branch, g.last, g.refs]);
  if (d === GT.dir && (g.dirty || 0) !== GT.dirty) { GT.dirty = g.dirty || 0; renderTree(); }
  if (key === GT.key) return;
  if (d !== GT.dir) { GT.dir = d; GT.unfold = null; GT.n = 100; GT.info.clear(); }
  GT.key = key; GT.pick = pick; GT.dirty = g.dirty || 0;
  clearTimeout(GT.t); GT.t = setTimeout(loadTree, 120);
}
/* `remote`: the daemon asks the remote first (and fetches what moved there), so it takes longer */
async function loadTree(remote) {
  if (GT.busy) { GT.again = true; if (remote) GT.againRemote = true; return; }
  GT.busy = true;
  const { dir, pick, n } = GT;
  if (remote) { GT.looked.set(dir, Date.now()); GT.looking = true; renderHead(); }
  const r = await req('log', { dir, branch: pick, n, remote: !!remote }, remote ? 45000 : 8000);
  GT.busy = false;
  if (remote) GT.looking = false;
  if (r && !r.ok && /not understood/.test(r.error || '')) { GT.off = true; renderTree(); return; }
  if (r && r.ok && dir === GT.dir && pick === GT.pick) {
    const old = GT.data && GT.data.dir === dir && GT.data.pick === pick && GT.data.head === r.head ? GT.data : null;
    r.dir = dir; r.pick = pick; r.fresh = new Set();
    // what came in since the last look lights up for a moment
    const seen = old && new Set(old.commits.map(c => c.h));
    if (seen) for (const c of r.commits || []) { if (seen.has(c.h)) break; r.fresh.add(c.h); }
    const news = newEvents(r);
    GT.data = r;
    renderTree();
    if (news.length) playEvents(news, old);
    // a folder not asked about for a while: what does its remote have?
    const R = r.remote;
    if (!remote && R && Date.now() / 1000 - R.at > LOOK_EVERY / 1000) setTimeout(lookRemote, 700);
  } else if (remote) renderHead();
  if (GT.again) { const rem = GT.againRemote; GT.again = GT.againRemote = false; loadTree(rem); }
}
/* ask the remote where its branches are (`now`: even if this window asked moments ago) */
function lookRemote(now) {
  const d = GT.data;
  if (GT.off || !d || !d.remote || d.dir !== GT.dir || GT.looking) return;
  if (now !== true && Date.now() - (GT.looked.get(GT.dir) || 0) < LOOK_EVERY / 3) return;
  loadTree(true);
}
setInterval(() => { if (document.visibilityState === 'visible' && !treeEl.hidden) lookRemote(); }, LOOK_EVERY);
addEventListener('focus', () => { const R = GT.data && GT.data.remote; if (R && Date.now() / 1000 - R.at > LOOK_EVERY / 2000) lookRemote(); });
function setTreePick(b) {
  try { if (b) localStorage.setItem('weft-tree-branch:' + GT.dir, b); else localStorage.removeItem('weft-tree-branch:' + GT.dir); } catch {}
  GT.unfold = null; GT.n = 100;
  const d = GT.dir; GT.key = ''; GT.dir = ''; treeFollow(d, GT.git);
}
function setTreeOpen(on) {
  GT.open = on;
  stopPeek();
  try { localStorage.setItem('weft-tree', on ? 'open' : 'shut'); } catch {}
  renderTree();
}

/* "3m", "5h", "2d" */
function ago(t) {
  const s = Math.max(0, Date.now() / 1000 - t);
  return s < 60 ? 'now' : s < 3600 ? `${Math.floor(s / 60)}m` : s < 86400 ? `${Math.floor(s / 3600)}h`
    : s < 86400 * 14 ? `${Math.floor(s / 86400)}d` : s < 86400 * 60 ? `${Math.floor(s / 604800)}w`
    : s < 86400 * 365 ? `${Math.floor(s / 2592000)}mo` : `${Math.floor(s / 31536000)}y`;
}
const agoWords = t => { const a = ago(t); return a === 'now' ? 'just now' : a + ' ago'; };
const counted = (n, w) => `${n} ${w}${n === 1 ? '' : 's'}`;
/* a branch weft cut for an agent's worktree is named after its card */
function branchCard(name) {
  const m = /^weft\/(.+)$/.exec(name);
  if (!m) return null;
  for (const n of S.nodes.values()) if ((n.cwd || '').endsWith('/' + m[1])) return n;
  return null;
}
const branchName = b => { const n = branchCard(b); return n ? `${b} · ${n.name}` : b; };

/* ---- the remote: how the branch stands against it, and how fresh that is. A click asks it now. */
function syncChip(d, quiet) {
  // (a daemon from before the remote was asked says only the counts: those, as of the last fetch)
  const R = d.remote === undefined && d.up ? { name: d.up.split('/')[0], at: 0, ok: true } : d.remote;
  if (!R) return null;
  if (d.remote === undefined) quiet = true;
  const outs = (d.commits || []).filter(c => c.o).length;
  const remoteBranch = d.pick && (d.branches || []).some(b => b.remote && b.name === d.pick);
  let kids, cls, says;
  if (remoteBranch) { kids = [R.name]; cls = 'sync'; says = `${d.pick} is a branch of ${R.name}`; }
  else if (R.gone) { kids = [`gone on ${R.name}`]; cls = 'gone'; says = `${d.up} was deleted on ${R.name} (its pull request merged there?)`; }
  else if (!d.up) {
    const many = outs + (d.more && outs >= (d.commits || []).length ? '+' : '');
    kids = [outs ? `↑${many} new` : `not on ${R.name}`]; cls = 'local';
    says = d.pick === '*' ? (outs ? `${counted(outs, 'commit')} on no branch of ${R.name} yet` : `every commit is on ${R.name}`)
      : `${d.head || 'this'} has no branch on ${R.name} yet${outs ? `: ${counted(outs, 'commit')} only here` : ''}`;
  } else if (d.ahead || d.behind) {
    kids = [d.ahead ? h('b', {}, `↑${d.ahead}`) : null, d.ahead && d.behind ? ' ' : null, d.behind ? h('em', {}, `↓${d.behind}`) : null,
      d.ahead && !d.behind ? ' to push' : !d.ahead ? ' to pull' : null];
    cls = d.ahead && d.behind ? 'split' : d.ahead ? 'ahead' : 'behind';
    says = [d.ahead ? `${counted(d.ahead, 'commit')} here that ${d.up} does not have (not pushed)` : '',
      d.behind ? `${counted(d.behind, 'commit')} on ${d.up} that are not here (not pulled)` : ''].filter(Boolean).join('\n');
  } else { kids = ['pushed']; cls = 'sync'; says = `${d.up} has every commit of this branch, and no other`; }
  const age = R.at ? Date.now() / 1000 - R.at : Infinity;
  const fresh = GT.looking ? 'busy' : R.at && !R.ok ? 'bad' : age < 180 ? 'live' : 'old';
  const when = fresh === 'busy' ? `asking ${R.name}…`
    : fresh === 'bad' ? `${R.name} could not be reached ${agoWords(R.at)} (${R.err}): this is as of the last fetch`
    : R.at ? `${R.name} asked ${agoWords(R.at)}${fresh === 'old' ? ': this may be out of date' : ''}` : `${R.name} not asked yet: this is as of the last fetch`;
  const title = `${says}\n${when}${quiet ? '' : `\nclick: ask ${R.name} now`}`;
  const props = { class: `gab ${cls} f-${fresh}`, title };
  if (quiet) return h('span', props, h('i', { class: 'rdot' }), ...nn(kids));
  return h('button', { ...props, 'aria-label': `${says}. ${when}`, onclick: e => { e.stopPropagation(); lookRemote(true); } }, h('i', { class: 'rdot' }), ...nn(kids));
}

/* ---- what happened: the words for one event, and its icon */
const EV_ICON = {
  commit: I('<circle cx="8" cy="8" r="2.6"/><path d="M8 1.5v3.9M8 10.6v3.9"/>'),
  push: I('<path d="M8 13.5V3M4 6.8l4-4 4 4"/>'),
  fetch: I('<path d="M8 2.5V13M4 9.2l4 4 4-4"/>'),
  merge: I('<circle cx="4.5" cy="3.4" r="1.6"/><circle cx="11.5" cy="3.4" r="1.6"/><circle cx="8" cy="12.6" r="1.6"/><path d="M4.5 5c0 3 3.5 3 3.5 6M11.5 5c0 3-3.5 3-3.5 6"/>'),
  switch: I('<path d="M2.5 5h10M10 2.5 12.5 5 10 7.5M13.5 11h-10M6 8.5 3.5 11 6 13.5"/>'),
  back: I('<path d="M3 3.5v4h4M3.5 7.4A5 5 0 1 1 4.6 11.6"/>'),
  amend: I('<path d="M3 13l1-3.2 6.6-6.6 2.2 2.2L6.2 12z"/>'),
};
const evIcon = k => EV_ICON[{ pull: 'fetch', forced: 'back', reset: 'back', rebase: 'back' }[k] || k] || EV_ICON.commit;
function eventWords(e, d) {
  const s = e.s ? `“${e.s}”` : '';
  switch (e.k) {
    case 'commit': return ['committed', s];
    case 'amend': return ['amended', s];
    case 'merge': return [e.x ? `merged ${e.x}` : 'merged', e.x ? '' : s];
    case 'pull': return [e.n ? `pulled ${counted(e.n, 'commit')}` : 'pulled', d.up ? `from ${d.up}` : ''];
    case 'rebase': return ['rebased', s];
    case 'reset': return [`reset to ${e.h.slice(0, 7)}`, s];
    case 'switch': return ['switched', e.x || ''];
    case 'push': return [e.n ? `pushed ${counted(e.n, 'commit')}` : 'pushed', `to ${e.ref}`];
    case 'fetch': return [e.n ? `${e.ref} has ${counted(e.n, 'new commit')}` : `${e.ref} moved`, 'from elsewhere'];
    case 'forced': return [`${e.ref} was rewritten`, 'force-pushed on the remote'];
  }
  return [e.k, s];
}
const evKey = e => `${e.ref || ''}|${e.t}|${e.h}|${e.k}`;
/* the events this page has not seen in this folder yet, oldest first. The first look at a folder
   sees none: what happened before you looked is not news (the line still says the latest). */
function newEvents(r) {
  const k = r.dir + '\n' + r.pick, evs = r.events || [];
  const before = GT.seen.get(k);
  GT.seen.set(k, new Set(evs.map(evKey)));
  if (!before) return [];
  const now = Date.now() / 1000;
  return evs.filter(e => !before.has(evKey(e)) && now - e.t < 180).reverse();
}
/* the line under the head: the latest event (a click shows its commit) */
function renderEvent(e) {
  const el = treeEl.evl, d = GT.data;
  if (!el) return;
  el.hidden = !e;
  if (!e) return;
  const [verb, rest] = eventWords(e, d);
  el.className = 'gev k-' + e.k;
  el.title = (d.events || []).map(x => { const [v, r] = eventWords(x, d); return `${ago(x.t).padStart(3)}  ${v} ${r}`; }).join('\n') + '\nclick: show that commit';
  el.replaceChildren(h('span', { class: 'gei', html: evIcon(e.k) }), h('b', {}, verb), h('span', { class: 'gs' }, rest),
    h('span', { class: 'ga', 'data-t': e.t }, ago(e.t)));
  el.onclick = () => showCommit(e.h);
}
function showCommit(hash) {
  const row = treeEl.inner && treeEl.inner.querySelector(`.gr[data-h="${hash}"]`);
  if (!row) return toast('', 'that commit is not in the tree shown');
  if (GT.unfold !== hash) unfoldCommit(hash);
  const r = treeEl.inner.querySelector(`.gr[data-h="${hash}"]`);
  if (r) r.scrollIntoView({ block: 'nearest' });
}

/* ---- as it happens: a folded tree unfolds for a moment (it folds again unless the pointer is on
   it; a click keeps it open), the line under the head says what happened, and it moves: a commit
   lands with a ripple, a push sends its commits up to the remote, what came in from the remote
   drops into its lane */
function playEvents(news, old) {
  if (treeEl.hidden || GT.off) return;
  // (many at once, a rebase or an agent's quick round of git: the last three; a switch is no news)
  const loud = news.filter(e => e.k !== 'switch'), list = (loud.length ? loud : news).slice(-3);
  if (loud.length) peekTree();
  if (GT.peeking || treeEl.list.scrollTop < 80) treeEl.list.scrollTop = 0;
  const still = matchMedia('(prefers-reduced-motion: reduce)').matches;
  GT.plays.forEach(clearTimeout);
  GT.playUntil = Date.now() + list.length * PLAY_MS;
  GT.plays = list.map((e, i) => setTimeout(() => playOne(e, old, still), i * PLAY_MS));
}
function playOne(e, old, still) {
  if (treeEl.hidden || !GT.data) return;
  renderEvent(e);
  const el = treeEl.evl;
  el.classList.remove('play'); void el.offsetWidth; el.classList.add('play');
  if (still || !(GT.open || GT.peeking)) return;
  const chip = () => treeEl.head.querySelector('.gab');
  if (e.k === 'push') {
    // the commits it carried: not pushed before (hollow), on the remote now
    const was = new Set(old ? old.commits.filter(c => c.o).map(c => c.h) : []);
    const went = (GT.data.commits || []).filter(c => was.has(c.h) && !c.o).map(c => c.h);
    const hs = went.length ? went : [e.h];
    hs.slice().reverse().forEach((h, i) => setTimeout(() => treeRipple(h, 'push'), i * 70));
    setTimeout(() => treeArrow(hs[0], chip(), 'up'), Math.min(hs.length, 8) * 70);
  } else if (e.k === 'fetch' || e.k === 'forced') {
    treeArrow(e.h, chip(), 'down');
    setTimeout(() => treeRipple(e.h, e.k), 560);
  } else treeRipple(e.h, e.k);
}
function treeRipple(hash, k) {
  const p = GT.dots.get(hash), svg = treeEl.inner.querySelector('svg.glanes');
  if (!p || !svg) return;
  for (const [i, r] of [[0, 4], [1, 4]]) {
    const c = sv('circle', { class: 'rip r-' + k, cx: p[0], cy: p[1], r, style: `animation-delay:${i * 180}ms` });
    svg.append(c);
    setTimeout(() => c.remove(), 1400 + i * 180);
  }
}
/* an arrow between a commit's dot and the remote chip in the head */
function treeArrow(hash, chip, way) {
  const svg = treeEl.inner.querySelector('svg.glanes'), p = GT.dots.get(hash);
  if (!chip || !svg || !chip.isConnected) return;
  const lr = treeEl.list.getBoundingClientRect(), sr = svg.getBoundingClientRect(), cr = chip.getBoundingClientRect(), tr = treeEl.getBoundingClientRect();
  // (the dot, or the edge of the list when it is scrolled out of sight)
  const dot = [p ? sr.left + p[0] : lr.left + 14, Math.max(lr.top + 6, Math.min(lr.bottom - 6, p ? sr.top + p[1] : lr.top))];
  const tip = [cr.left + 9, cr.top + cr.height / 2];
  const [a, b] = way === 'up' ? [dot, tip] : [tip, dot];
  const at = ([x, y], s) => `translate(${x - tr.left - 8}px, ${y - tr.top - 8}px) scale(${s})`;
  const el = h('i', { class: 'gfly ' + way, html: way === 'up' ? EV_ICON.push : EV_ICON.fetch });
  treeEl.append(el);
  const an = el.animate([
    { transform: at(a, .5), opacity: 0 },
    { transform: at(a, 1), opacity: 1, offset: .14 },
    { transform: at(b, 1), opacity: 1, offset: .82 },
    { transform: at(b, 1.5), opacity: 0 },
  ], { duration: 820, easing: 'cubic-bezier(.45,0,.2,1)' });
  const done = () => {
    el.remove();
    const c = treeEl.head.querySelector('.gab');
    if (c) { c.classList.remove('hit'); void c.offsetWidth; c.classList.add('hit'); }
  };
  an.onfinish = done; an.oncancel = () => el.remove();
}
function peekTree() {
  if (GT.open || treeEl.hidden) return;
  GT.peeking = true;
  treeEl.classList.remove('leaving');
  treeEl.classList.add('peek');
  renderTree();
  clearTimeout(GT.peekT);
  GT.peekT = setTimeout(endPeek, Math.max(PEEK_MS, GT.playUntil - Date.now() + 2500));
}
function endPeek() {
  if (!GT.peeking) return;
  if (treeEl.matches(':hover')) { GT.peekT = setTimeout(endPeek, 1500); return; }
  treeEl.classList.add('leaving');
  GT.peekT = setTimeout(() => { GT.peeking = false; treeEl.classList.remove('peek', 'leaving'); renderTree(); }, 220);
}
function stopPeek() {
  clearTimeout(GT.peekT);
  GT.peeking = false;
  treeEl.classList.remove('peek', 'leaving');
}

/* ---- the lanes: each commit takes the leftmost lane that waits for it (the others that wait for it
   end in it), its first parent goes on in that lane, the others join a lane that waits for them or
   open one. Lanes keep their place, and their colour: a line of work reads straight down. */
const NK = 7;
function lay(list) {
  const lanes = [], ks = [], rows = [];
  let next = 0, width = 1;
  const free = () => { const i = lanes.indexOf(null); return i < 0 ? lanes.length : i; };
  for (const c of list) {
    const before = lanes.slice(), bk = ks.slice();
    let col = c.h ? lanes.indexOf(c.h) : -1;
    if (col < 0) { col = free(); lanes[col] = null; ks[col] = c.work ? 'w' : next++ % NK; }
    const top = [], bot = [];
    before.forEach((x, i) => { if (x != null) top.push([i, x === c.h ? col : i, bk[i]]); });
    for (let i = 0; i < lanes.length; i++) if (lanes[i] === c.h) lanes[i] = null;
    // (the uncommitted files' dashed line ends here: what is checked out goes on in colour)
    if (ks[col] === 'w' && !c.work) ks[col] = next++ % NK;
    const k = ks[col];
    before.forEach((x, i) => { if (x != null && x !== c.h) bot.push([i, i, bk[i], false]); });
    c.p.forEach((p, j) => {
      let at = col;
      if (j === 0) lanes[col] = p;
      else if ((at = lanes.indexOf(p)) < 0) { at = free(); lanes[at] = p; ks[at] = next++ % NK; }
      bot.push([col, at, ks[at], true]);
    });
    while (lanes.length && lanes[lanes.length - 1] == null) { lanes.pop(); ks.pop(); }
    width = Math.max(width, lanes.length, col + 1);
    rows.push({ col, k, top, bot });
  }
  return { rows, width };
}
function drawLanes(svg, els, list, L, lw, x0) {
  const X = i => x0 + i * lw + lw / 2;
  const d = new Map();
  const add = (k, s) => d.set(k, (d.get(k) || '') + s);
  const curve = (x1, y1, x2, y2) => { const m = (y1 + y2) / 2; return `M${x1} ${y1}C${x1} ${m} ${x2} ${m} ${x2} ${y2}`; };
  const dots = [];
  els.forEach((el, r) => {
    const top = el.offsetTop, line = el.firstChild.offsetHeight, dot = top + line / 2, bot = top + el.offsetHeight;
    const row = L.rows[r], c = list[r];
    for (const [i, to, k] of row.top) add(k, i === to ? `M${X(i)} ${top}V${dot}` : curve(X(i), top, X(to), dot));
    for (const [i, to, k, fromDot] of row.bot) {
      if (!fromDot || i === to) add(k, `M${X(to)} ${dot}V${bot}`);
      else add(k, curve(X(i), dot, X(to), dot + line / 2) + `V${bot}`);
    }
    dots.push([X(row.col), dot, row.k, c]);
  });
  const kids = [];
  GT.dots.clear();
  for (const [k, path] of d) kids.push(sv('path', { class: 'k' + k, d: path }));
  for (const [x, y, k, c] of dots) {
    if (c.work) { kids.push(sv('circle', { class: 'w', cx: x, cy: y, r: 4 })); continue; }
    GT.dots.set(c.h, [x, y]);
    // (not pushed yet: hollow; on the remote and not here yet: dashed)
    const cls = 'k' + k + (c.p.length > 1 ? ' m' : '') + (c.o ? ' o' : c.i ? ' i' : '');
    kids.push(sv('circle', { class: cls, cx: x, cy: y, r: c.o || c.i ? 3.4 : c.p.length > 1 ? 3.4 : 3.6 }));
    if (c.h === GT.data.at) kids.push(sv('circle', { class: 'hd', cx: x, cy: y, r: 6.6 }));
  }
  svg.replaceChildren(...kids);
}

/* ---- where branches and tags are: "HEAD -> main", "origin/main", "tag: v1" as git prints them */
function refBadges(refs, d) {
  const remotes = new Set((d.branches || []).filter(b => b.remote).map(b => b.name.split('/')[0]));
  const out = [];
  for (let r of refs || []) {
    if (r === 'HEAD' || /\/HEAD$/.test(r)) continue;
    let cls = 'loc';
    if (r.startsWith('HEAD -> ')) { r = r.slice(8); cls = 'cur'; }
    else if (r.startsWith('tag: ')) { r = r.slice(5); cls = 'tag'; }
    else if (remotes.has(r.split('/')[0]) && r.includes('/')) cls = 'rem';
    out.push({ n: r, cls });
  }
  // origin/main where main is: one badge says both
  for (const b of out) {
    if (b.cls !== 'rem') continue;
    const mine = out.find(x => (x.cls === 'loc' || x.cls === 'cur') && x.n === b.n.slice(b.n.indexOf('/') + 1));
    if (mine) { mine.up = b.n; b.skip = true; }
  }
  const shown = out.filter(b => !b.skip);
  const els = shown.slice(0, 3).map(b => {
    const card = b.cls !== 'tag' && branchCard(b.n);
    const title = (b.cls === 'tag' ? 'tag ' : b.cls === 'rem' ? 'remote branch ' : 'branch ') + b.n
      + (b.cls === 'cur' ? ' (checked out)' : '') + (b.up ? `, ${b.up} is here too` : '') + (card ? `: the worktree of "${card.name}"` : '')
      + (b.cls === 'rem' && b.n === d.up ? `\nthe remote is here: ${d.ahead ? 'the commits above it are not pushed yet' : 'the commits above it came from there'}` : '');
    return h('span', { class: 'rf ' + (card ? 'card' : b.cls) + (b.n === d.up ? ' up' : ''), title }, card ? card.name : b.n, b.up ? h('i', {}, '⇅') : null);
  });
  if (shown.length > 3) els.push(h('span', { class: 'rf', title: shown.slice(3).map(b => b.n).join('\n') }, `+${shown.length - 3}`));
  return els;
}

/* ---- the corner */
function renderTree() {
  const d = GT.data, on = !GT.off && !!d && d.repo !== false && (d.dir === GT.dir || GT.git.repo !== false);
  treeEl.hidden = !on;
  if (!on) return;
  if (!treeEl.panel) {
    treeEl.head = h('div', { class: 'gh' });
    treeEl.evl = h('div', { class: 'gev', hidden: true });
    treeEl.inner = h('div', { class: 'gin' });
    treeEl.list = h('div', { class: 'glist' }, treeEl.inner);
    treeEl.foot = h('div', { class: 'gfoot' });
    treeEl.panel = h('div', { class: 'gpanel chamf' }, treeEl.head, treeEl.evl, treeEl.list, treeEl.foot);
    treeEl.box = h('button', { class: 'gbox', onclick: () => setTreeOpen(true) });
    // (a click in a tree that unfolded by itself keeps it open)
    treeEl.addEventListener('pointerdown', () => { if (GT.peeking) { GT.open = true; stopPeek(); try { localStorage.setItem('weft-tree', 'open'); } catch {} } }, true);
    for (const ev of ['pointerdown', 'wheel', 'dblclick', 'contextmenu']) treeEl.addEventListener(ev, e => e.stopPropagation());
    treeEl.append(treeEl.panel, treeEl.box);
    new ResizeObserver(placeTree).observe($('#dock'));
  }
  const shown = GT.open || GT.peeking;
  treeEl.classList.toggle('shut', !shown);
  treeEl.classList.toggle('stale', d.dir !== GT.dir);
  renderHead();
  if (Date.now() >= GT.playUntil) renderEvent((d.events || [])[0]);
  if (shown) renderRows();
  placeTree();
}
function renderHead() {
  const d = GT.data;
  if (!d || !treeEl.head) return;
  const name = d.pick === '*' ? 'every branch' : d.pick || d.head || `detached ${(d.at || '').slice(0, 7)}`;
  const pinned = !!d.pick && d.pick !== d.head;
  const tip = (pinned ? `showing ${d.pick === '*' ? 'every branch' : d.pick}${d.head ? `; ${d.head} is checked out` : ''}` : 'the branch that is checked out here')
    + '\nclick: show another branch (nothing is checked out)';
  const last = (d.commits || []).find(c => !c.i), ev = (d.events || [])[0];
  treeEl.head.replaceChildren(...nn([
    h('button', { class: 'gbr' + (pinned ? ' pinned' : ''), title: tip, 'aria-label': 'Branch: ' + name, onclick: e => treeMenu(e.currentTarget) },
      h('span', { class: 'gi', html: ICON.git }), h('span', { class: 'gn' }, branchName(name))),
    syncChip(d),
    h('span', { class: 'gdir', title: d.dir }, d.dir.slice(d.dir.lastIndexOf('/') + 1)),
    h('button', { class: 'ib', title: 'fold the tree away', 'aria-label': 'Fold the git tree', html: I('<path d="M4 8h8"/>'), onclick: () => setTreeOpen(false) }),
  ]));
  // folded: the branch, how it stands against the remote, and what happened last (else the last commit)
  const recent = ev && Date.now() / 1000 - ev.t < 1800 ? ev : null;
  treeEl.box.title = `${d.dir}\n${name}${last ? ': ' + last.s : ''}\nclick: unfold the git tree`;
  treeEl.box.replaceChildren(...nn([h('span', { class: 'gi', html: ICON.git }), h('span', {}, branchName(name)),
    syncChip(d, true), recent ? h('span', { class: 'gs' }, `${ago(recent.t)} · ${eventWords(recent, d).join(' ')}`)
      : last ? h('span', { class: 'gs' }, `${ago(last.t)} · ${last.s}`) : null]));
}
function renderRows() {
  const d = GT.data;
  const list = (d.commits || []).slice();
  // the files no one committed yet sit on top of what is checked out
  if (!d.pick && GT.dirty > 0 && d.dir === GT.dir && list.length) {
    const at = list.findIndex(c => c.h === d.at);
    if (at >= 0 && list.slice(0, at).every(c => c.i)) list.splice(at, 0, { h: '', p: [d.at], work: GT.dirty });
  }
  const L = lay(list);
  const lw = L.width > 10 ? Math.max(5, 120 / L.width) : 12, x0 = 6;
  treeEl.style.setProperty('--gx', `${x0 + L.width * lw + 6}px`);
  const svg = sv('svg', { class: 'glanes' });
  const rows = list.map(c => {
    if (c.work) return h('div', { class: 'gr work' }, h('div', { class: 'gline', title: 'changed files no one has committed yet' }, h('span', { class: 'gs' }, `${c.work} uncommitted`)));
    const when = new Date(c.t * 1000);
    const where = c.o ? '\nnot pushed yet' : c.i ? `\non ${d.up}, not here yet: a pull brings it` : '';
    const row = h('div', { class: 'gr' + (GT.unfold === c.h ? ' open' : '') + (d.fresh.has(c.h) ? ' fresh' : '') + (c.o ? ' out' : c.i ? ' in' : ''), 'data-h': c.h },
      h('div', { class: 'gline', title: `${c.h.slice(0, 10)} · ${c.a} · ${when.toLocaleString()}${c.p.length > 1 ? ' · merge' : ''}\n${c.s}${where}` },
        refBadges(c.r, d), h('span', { class: 'gs' }, c.s), h('span', { class: 'ga', 'data-t': c.t }, ago(c.t))));
    row.addEventListener('click', e => { if (!e.target.closest('.gdet')) unfoldCommit(c.h); });
    if (GT.unfold === c.h) row.append(commitDetail(c));
    return row;
  });
  d.fresh = new Set();
  treeEl.inner.replaceChildren(svg, ...(rows.length ? rows : [h('div', { class: 'gempty' }, 'no commits yet')]));
  if (rows.length) {
    drawLanes(svg, rows, list, L, lw, x0);
    svg.setAttribute('width', x0 + L.width * lw + 6);
    svg.setAttribute('height', treeEl.inner.offsetHeight);
  }
  treeEl.foot.replaceChildren(...nn([d.more && GT.n < 600 ? h('button', { onclick: () => { GT.n += 200; loadTree(); } }, 'older commits…') : null]));
}
async function unfoldCommit(hash) {
  GT.unfold = GT.unfold === hash ? null : hash;
  renderRows();
  if (!GT.unfold || GT.info.has(hash)) return;
  const r = await req('log', { dir: GT.dir, show: hash });
  if (GT.info.size > 60) GT.info.delete(GT.info.keys().next().value);
  GT.info.set(hash, r && r.ok ? r : { err: (r && r.error) || 'git could not show it' });
  if (GT.unfold === hash && (GT.open || GT.peeking)) renderRows();
}
function commitDetail(c) {
  const x = GT.info.get(c.h);
  const box = h('div', { class: 'gdet' });
  if (!x) return box.append(h('span', { class: 'gnote' }, 'reading…')), box;
  if (x.err) return box.append(h('span', { class: 'gnote' }, x.err)), box;
  const copy = () => { navigator.clipboard && navigator.clipboard.writeText(x.h); toast('ok', 'copied ' + x.h.slice(0, 10)); };
  const when = new Date(x.t * 1000);
  box.append(...nn([
    h('div', { class: 'gdh' }, h('button', { class: 'ghash', title: 'copy the full hash', onclick: copy }, x.h.slice(0, 10)),
      h('span', { title: x.e }, x.a),
      h('span', { title: when.toLocaleString() }, `${when.toLocaleString([], { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' })} · ${agoWords(x.t)}`),
      c.o ? h('span', { class: 'gtag out' }, 'not pushed') : c.i ? h('span', { class: 'gtag in' }, `only on ${GT.data.up}`) : null),
    h('div', { class: 'gmsg' }, x.body),
    x.files.map(f => h('div', { class: 'gf', title: f.path },
      h('b', { class: 'ad' }, f.add == null ? 'bin' : f.add ? '+' + f.add : ''), h('b', { class: 'dl' }, f.del ? '−' + f.del : ''), h('span', {}, f.path))),
    x.count > x.files.length ? h('div', { class: 'gnote' }, `+${x.count - x.files.length} more files`) : null,
    !x.count ? h('div', { class: 'gnote' }, 'no files changed') : null]));
  return box;
}
function treeMenu(btn) {
  const d = GT.data;
  if (!d) return;
  const r = btn.getBoundingClientRect();
  const it = (b, label, hint) => ({ label, hint: GT.pick === b ? 'shown' : hint || '', run: () => setTreePick(b) });
  const loc = (d.branches || []).filter(b => !b.remote && b.name !== d.head), rem = (d.branches || []).filter(b => b.remote);
  popup(nn([
    { cap: 'show the tree of' },
    it('', d.head || 'what is checked out', 'checked out'),
    it('*', 'every branch', 'all at once'),
    loc.length ? { cap: 'branches' } : null, loc.map(b => it(b.name, branchName(b.name))),
    rem.length ? { cap: 'remote' } : null, rem.map(b => it(b.name, b.name)),
  ]), r.left, r.top, { above: true });
}
/* where the open tree covers the canvas (fit and reveal keep cards out from under it), or null */
function treeBox() {
  if (treeEl.hidden || !GT.open || pad.open) return null;
  const r = treeEl.getBoundingClientRect();
  return r.width && r.height ? r : null;
}
/* beside the bar when there is room, else above it */
function placeTree() {
  if (treeEl.hidden) return;
  const bar = $('#omniw').getBoundingClientRect(), dock = $('#dock').getBoundingClientRect();
  const room = Math.floor(innerWidth - bar.right - 14 - 12);
  const beside = !bar.width || room >= 250;
  treeEl.style.setProperty('--gw', `${Math.min(440, beside && bar.width ? room : innerWidth - 28)}px`);
  treeEl.style.bottom = beside ? '' : `${Math.ceil(innerHeight - dock.top) + 10}px`;
  treeEl.style.setProperty('--gh', beside ? 'min(40vh, calc(100vh - 150px))' : 'min(30vh, calc(100vh - 260px))');
  treeEl.classList.toggle('above', !beside);
}
addEventListener('resize', placeTree);
setInterval(() => { if (!treeEl.hidden) for (const a of treeEl.querySelectorAll('.ga[data-t]')) a.textContent = ago(+a.dataset.t); }, 30000);
