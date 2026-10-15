/* ------------------------------------------------------------ the git tree */
/* Bottom right, beside the bar: the commits of the branch where you are (the folder the line above
   the bar names: the selected agent's, else the one new agents start in), drawn as a graph. Live:
   the daemon looks at the folder every few seconds and says when a branch moved, so a commit by
   any agent shows up as it lands. Its name picks another branch or every branch (only what is
   shown changes, nothing is checked out); a click on a commit unfolds its message and files. A
   daemon without the log (an older build) shows none of this. */
const GT = { dir: '', git: {}, key: '', pick: '', dirty: 0, data: null, n: 100, busy: false, again: false, off: false,
  open: true, unfold: null, info: new Map(), t: 0 };
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
async function loadTree() {
  if (GT.busy) { GT.again = true; return; }
  GT.busy = true;
  const { dir, pick, n } = GT;
  const r = await req('log', { dir, branch: pick, n });
  GT.busy = false;
  if (r && !r.ok && /not understood/.test(r.error || '')) { GT.off = true; renderTree(); return; }
  if (r && r.ok && dir === GT.dir && pick === GT.pick) {
    const old = GT.data && GT.data.dir === dir && GT.data.pick === pick && GT.data.head === r.head ? GT.data : null;
    r.dir = dir; r.pick = pick; r.fresh = new Set();
    // what came in since the last look lights up for a moment
    const seen = old && new Set(old.commits.map(c => c.h));
    if (seen) for (const c of r.commits || []) { if (seen.has(c.h)) break; r.fresh.add(c.h); }
    GT.data = r;
    renderTree();
  }
  if (GT.again) { GT.again = false; loadTree(); }
}
function setTreePick(b) {
  try { if (b) localStorage.setItem('weft-tree-branch:' + GT.dir, b); else localStorage.removeItem('weft-tree-branch:' + GT.dir); } catch {}
  GT.unfold = null; GT.n = 100;
  const d = GT.dir; GT.key = ''; GT.dir = ''; treeFollow(d, GT.git);
}
function setTreeOpen(on) {
  GT.open = on;
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
/* a branch weft cut for an agent's worktree is named after its card */
function branchCard(name) {
  const m = /^weft\/(.+)$/.exec(name);
  if (!m) return null;
  for (const n of S.nodes.values()) if ((n.cwd || '').endsWith('/' + m[1])) return n;
  return null;
}
const branchName = b => { const n = branchCard(b); return n ? `${b} · ${n.name}` : b; };
function upAndDown(d) {
  if (!d.up) return null;
  return h('span', { class: 'gab', title: `against ${d.up}: ${d.ahead} to push, ${d.behind} to pull` },
    d.ahead ? h('b', {}, `↑${d.ahead}`) : null, d.ahead && d.behind ? ' ' : null, d.behind ? h('i', {}, `↓${d.behind}`) : null,
    !d.ahead && !d.behind ? 'pushed' : null);
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
  for (const [k, path] of d) kids.push(sv('path', { class: 'k' + k, d: path }));
  for (const [x, y, k, c] of dots) {
    if (c.work) { kids.push(sv('circle', { class: 'w', cx: x, cy: y, r: 4 })); continue; }
    kids.push(sv('circle', { class: 'k' + k + (c.p.length > 1 ? ' m' : ''), cx: x, cy: y, r: c.p.length > 1 ? 3.4 : 3.6 }));
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
      + (b.cls === 'cur' ? ' (checked out)' : '') + (b.up ? `, ${b.up} is here too` : '') + (card ? `: the worktree of "${card.name}"` : '');
    return h('span', { class: 'rf ' + (card ? 'card' : b.cls), title }, card ? card.name : b.n, b.up ? h('i', {}, '⇅') : null);
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
    treeEl.inner = h('div', { class: 'gin' });
    treeEl.list = h('div', { class: 'glist' }, treeEl.inner);
    treeEl.foot = h('div', { class: 'gfoot' });
    treeEl.panel = h('div', { class: 'gpanel chamf' }, treeEl.head, treeEl.list, treeEl.foot);
    treeEl.box = h('button', { class: 'gbox', onclick: () => setTreeOpen(true) });
    for (const ev of ['pointerdown', 'wheel', 'dblclick', 'contextmenu']) treeEl.addEventListener(ev, e => e.stopPropagation());
    treeEl.append(treeEl.panel, treeEl.box);
    new ResizeObserver(placeTree).observe($('#dock'));
  }
  treeEl.classList.toggle('shut', !GT.open);
  treeEl.classList.toggle('stale', d.dir !== GT.dir);
  const name = d.pick === '*' ? 'every branch' : d.pick || d.head || `detached ${(d.at || '').slice(0, 7)}`;
  const pinned = !!d.pick && d.pick !== d.head;
  const tip = (pinned ? `showing ${d.pick === '*' ? 'every branch' : d.pick}${d.head ? `; ${d.head} is checked out` : ''}` : 'the branch that is checked out here')
    + '\nclick: show another branch (nothing is checked out)';
  const last = (d.commits || [])[0];
  treeEl.head.replaceChildren(...nn([
    h('button', { class: 'gbr' + (pinned ? ' pinned' : ''), title: tip, 'aria-label': 'Branch: ' + name, onclick: e => treeMenu(e.currentTarget) },
      h('span', { class: 'gi', html: ICON.git }), h('span', { class: 'gn' }, branchName(name))),
    upAndDown(d),
    h('span', { class: 'gdir', title: d.dir }, d.dir.slice(d.dir.lastIndexOf('/') + 1)),
    h('button', { class: 'ib', title: 'fold the tree away', 'aria-label': 'Fold the git tree', html: I('<path d="M4 8h8"/>'), onclick: () => setTreeOpen(false) }),
  ]));
  treeEl.box.title = `${d.dir}\n${name}${last ? ': ' + last.s : ''}\nclick: unfold the git tree`;
  treeEl.box.replaceChildren(...nn([h('span', { class: 'gi', html: ICON.git }), h('span', {}, branchName(name)),
    upAndDown(d), last ? h('span', { class: 'gs' }, `${ago(last.t)} · ${last.s}`) : null]));
  if (GT.open) renderRows();
  placeTree();
}
function renderRows() {
  const d = GT.data;
  const list = (d.commits || []).slice();
  // the files no one committed yet sit on top of what is checked out
  if (!d.pick && GT.dirty > 0 && d.dir === GT.dir && list.length && list[0].h === d.at) list.unshift({ h: '', p: [d.at], work: GT.dirty });
  const L = lay(list);
  const lw = L.width > 10 ? Math.max(5, 120 / L.width) : 12, x0 = 6;
  treeEl.style.setProperty('--gx', `${x0 + L.width * lw + 6}px`);
  const svg = sv('svg', { class: 'glanes' });
  const rows = list.map(c => {
    if (c.work) return h('div', { class: 'gr work' }, h('div', { class: 'gline', title: 'changed files no one has committed yet' }, h('span', { class: 'gs' }, `${c.work} uncommitted`)));
    const when = new Date(c.t * 1000);
    const row = h('div', { class: 'gr' + (GT.unfold === c.h ? ' open' : '') + (d.fresh.has(c.h) ? ' fresh' : ''), 'data-h': c.h },
      h('div', { class: 'gline', title: `${c.h.slice(0, 10)} · ${c.a} · ${when.toLocaleString()}${c.p.length > 1 ? ' · merge' : ''}\n${c.s}` },
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
  if (GT.unfold === hash && GT.open) renderRows();
}
function commitDetail(c) {
  const x = GT.info.get(c.h);
  const box = h('div', { class: 'gdet' });
  if (!x) return box.append(h('span', { class: 'gnote' }, 'reading…')), box;
  if (x.err) return box.append(h('span', { class: 'gnote' }, x.err)), box;
  const copy = () => { navigator.clipboard && navigator.clipboard.writeText(x.h); toast('ok', 'copied ' + x.h.slice(0, 10)); };
  const when = new Date(x.t * 1000), since = ago(x.t);
  box.append(...nn([
    h('div', { class: 'gdh' }, h('button', { class: 'ghash', title: 'copy the full hash', onclick: copy }, x.h.slice(0, 10)),
      h('span', { title: x.e }, x.a),
      h('span', { title: when.toLocaleString() }, `${when.toLocaleString([], { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' })} · ${since === 'now' ? 'just now' : since + ' ago'}`)),
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
