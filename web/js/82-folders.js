/* ------------------------------------------------------------ folders */
/* An agent can work in another folder than the project's (a sibling repository, say). New agents
   start in the folder chosen on the launch pad (remembered per project); cards are moved from their
   folder tag, the folder line above the bar, or ctrl+o. A card stores nothing for the project folder. */
const projDir = () => (S.project && S.project.dir) || '';
function absDir(s) {
  let d = (s || '').trim();
  if (!d) return projDir();
  const home = (S.project && S.project.home) || '';
  if (d === '~' || d.startsWith('~/')) d = home + d.slice(1);
  if (!d.startsWith('/')) d = projDir() + '/' + d;
  const out = [];
  for (const p of d.split('/')) { if (!p || p === '.') continue; if (p === '..') out.pop(); else out.push(p); }
  return '/' + out.join('/');
}
function shortDir(d) {
  const home = (S.project && S.project.home) || '';
  return home && (d === home || d.startsWith(home + '/')) ? '~' + d.slice(home.length) : d;
}
const baseName = d => d.slice(d.lastIndexOf('/') + 1) || '/';
const parentOf = d => d.slice(0, d.lastIndexOf('/') + 1);
/* (an agent that took over in a worktree weft made works on a branch of the project's repository) */
const weftWorktree = d => /\/weft\/worktrees\//.test(d);
/* where a card works: its own folder; a queued prompt in its agent's, a gate or git step in the
   folder of the agent whose work it handles (the daemon's `node_dir` decides the same way) */
function workDir(n) {
  if (!n) return projDir();
  if ((n.cwd || '').trim()) return absDir(n.cwd);
  if (kindOf(n) === 'note' || (kindOf(n) === 'agent' && n.agent !== 'step')) return projDir();
  const seen = new Set(), todo = [n.id];
  while (todo.length) {
    const at = todo.shift();
    if (seen.has(at)) continue;
    seen.add(at);
    const m = S.nodes.get(at);
    if (!m || kindOf(m) === 'note') continue;
    if (kindOf(m) === 'agent' && m.agent !== 'step') {
      const d = (m.cwd || '').trim() ? absDir(m.cwd) : projDir();
      return weftWorktree(d) ? projDir() : d;
    }
    if (m.agent === 'step' && m.host) { todo.unshift(m.host); continue; }
    for (const e of incoming(at)) todo.push(e.from);
  }
  return projDir();
}
/* every folder a card's work is in: a push or pull request after work in several repositories
   works in each of them (the daemon's `git_repos`) */
function workDirs(n) {
  if (!n || kindOf(n) !== 'git' || (n.cwd || '').trim()) return [workDir(n)];
  const out = [], seen = new Set(), todo = [n.id];
  while (todo.length) {
    const at = todo.shift();
    if (seen.has(at)) continue;
    seen.add(at);
    const m = S.nodes.get(at);
    if (!m || kindOf(m) === 'note') continue;
    if (kindOf(m) === 'agent' && m.agent !== 'step') {
      const d = workDir(m), w = weftWorktree(d) ? projDir() : d;
      if (!out.includes(w)) out.push(w);
      continue;
    }
    if (m.agent === 'step' && m.host) { todo.unshift(m.host); continue; }
    for (const e of incoming(at)) todo.push(e.from);
  }
  return out.length ? out : [workDir(n)];
}
/* branch and uncommitted files of a folder, as the daemon last saw them */
const gitOf = d => (d === projDir() ? (S.project && S.project.git) : S.dirs[d]) || {};
/* one colour per folder, so cards of one repository are seen together (none for the project's) */
const FOLDER_HUES = [330, 95, 15, 240, 55, 172, 300, 130];
function folderHue(d) {
  let x = 2166136261;
  for (let i = 0; i < d.length; i++) x = Math.imul(x ^ d.charCodeAt(i), 16777619);
  return FOLDER_HUES[(x >>> 0) % FOLDER_HUES.length];
}

/* the folder new agents start in: the launch pad's choice for this project */
function newDir() {
  const d = (S.prefs.here || {})[projDir()];
  return d && !gitOf(d).missing ? d : projDir();
}
const cwdField = d => (d === projDir() ? '' : d);
function rememberDir(d) {
  if (!d || d === projDir()) return;
  S.prefs.dirs = [d, ...(S.prefs.dirs || []).filter(x => x !== d)].slice(0, 8);
}
/* the daemon looks at a folder's branch and uncommitted files from now on */
function watchDir(d) {
  if (!d || d === projDir()) return;
  req('proj', { dir: d }).then(r => {
    if (r && r.git && typeof r.git === 'object' && !S.dirs[d]) { S.dirs[d] = r.git; folderChanged(); }
  });
}
function folderChanged() { renderWhere(); if (pad.open) renderPad(); }
function setNewDir(d) {
  const here = { ...(S.prefs.here || {}) };
  delete here[projDir()];
  if (d !== projDir()) here[projDir()] = d;
  const ks = Object.keys(here);
  if (ks.length > 16) delete here[ks[0]];
  S.prefs.here = here;
  rememberDir(d); savePrefs(); watchDir(d);
  redrawPrompts(); renderWhere();
}
/* move cards to another folder: an agent works there from its next start */
function moveToDir(ids, d) {
  const live = [];
  for (const id of ids) {
    const n = S.nodes.get(id);
    if (!n || workDir(n) === d) continue;
    // (a git step or gate with no folder of its own follows its agent: pinning it needs the path)
    patchNode(id, { cwd: kindOf(n) === 'agent' ? cwdField(d) : d }, 0);
    if ((S.rt.get(id) || {}).alive) live.push(id);
  }
  rememberDir(d); savePrefs(); watchDir(d);
  renderWhere(); omni._key = null; renderOmni();
  if (!live.length) return;
  const who = live.length > 1 ? `${live.length} running agents` : S.nodes.get(live[0]).name;
  toast('info', `${who} move${live.length > 1 ? '' : 's'} to ${baseName(d)} at the next start: the running session stays where it is.`,
    { ms: 9000, action: { label: 'restart there', run: () => live.forEach(id => send('restart', { id })) } });
}

/* the folder picker: a field to filter the known folders or type a path, the folders under it.
   ↑/↓ choose, enter takes it, tab goes into a folder, esc drops it */
function folderPicker(x, y, o) {
  closeMenu();
  const inp = h('input', { class: 'fld', spellcheck: 'false', autocomplete: 'off', placeholder: 'filter, or a path: ~/…  ../…  /…', 'aria-label': 'Folder' });
  const list = h('div', { class: 'fpl', role: 'listbox' });
  const m = h('div', { class: 'menu fpick', role: 'dialog', 'aria-label': o.title },
    h('div', { class: 'cap' }, o.title), inp, list,
    h('div', { class: 'fkeys' }, ...keyHints([['↑↓', 'choose'], ['enter', 'take it'], ['tab', 'go into']])));
  document.body.append(m);
  menuEl = m;
  const outside = ev => { if (!m.contains(ev.target)) closeMenu(); };
  setTimeout(() => {
    if (menuEl !== m) return;
    addEventListener('pointerdown', outside, true);
    menuOff = () => removeEventListener('pointerdown', outside, true);
  }, 0);
  let rows = [], at = 0, seq = 0, sibs = S.sibs || [], pending = null;
  const place = () => {
    const r = m.getBoundingClientRect();
    const top = o.above ? y - r.height - 8 : y;
    m.style.left = Math.max(8, Math.min(x, innerWidth - r.width - 8)) + 'px';
    m.style.top = Math.max(8, Math.min(top, innerHeight - r.height - 8)) + 'px';
  };
  const known = () => {
    const out = [], seen = new Set();
    const add = (path, tag) => { if (path && !seen.has(path)) { seen.add(path); out.push({ path, tag }); } };
    add(projDir(), 'project');
    for (const n of S.nodes.values()) if (kindOf(n) === 'agent' && (n.cwd || '').trim() && !weftWorktree(absDir(n.cwd))) add(absDir(n.cwd), 'has agents');
    for (const d of S.prefs.dirs || []) add(d, 'used before');
    for (const s of sibs) add(s.path, s.repo ? 'git' : '');
    return out;
  };
  const pathy = v => /^[~./]/.test(v) || v.includes('/');
  const draw = () => {
    at = Math.max(0, Math.min(at, rows.length - 1));
    list.replaceChildren(...(rows.length ? rows.map((r, i) => {
      const b = h('button', { class: 'mi' + (i === at ? ' on' : ''), role: 'option', 'aria-selected': String(i === at), title: r.path, onclick: () => pick(r.path) },
        h('span', { class: 'k' }, r.path === o.cur ? '●' : ' '),
        // (a folder agents work in shows the colour its cards wear)
        r.tag === 'has agents' ? h('span', { class: 'fsq', style: `--fh:${folderHue(r.path)}` }) : null,
        h('span', { class: 'fname' }, baseName(r.path)),
        h('span', { class: 'fpar' }, shortDir(parentOf(r.path))),
        r.tag ? h('span', { class: 'hint' }, r.tag) : null);
      b.addEventListener('pointermove', () => { if (at !== i) { at = i; mark(); } });
      return b;
    }) : [h('div', { class: 'cap' }, inp.value.trim() ? 'no folder matches' : 'no folders')]));
    place();
  };
  const mark = () => [...list.children].forEach((b, i) => { b.classList.toggle('on', i === at); b.setAttribute('aria-selected', String(i === at)); });
  const ask = (args, merge) => {
    const mine = ++seq;
    const done = req('dirs', args).then(r => {
      if (mine !== seq || menuEl !== m) return;
      pending = null;
      const got = (r && r.dirs) || [];
      const have = new Set(merge.map(x => x.path));
      rows = [...merge, ...got.filter(d => !have.has(d.path)).map(d => ({ path: d.path, tag: d.repo ? 'git' : '' }))];
      draw();
    });
    pending = done;
  };
  const refresh = () => {
    const v = inp.value.trim();
    at = 0;
    if (!v) { seq++; pending = null; rows = known(); draw(); return; }
    if (pathy(v)) {
      // what the typed path names so far, completed by the daemon (it can see the disk); until it
      // answers, what is listed and still fits stays
      const full = absDir(v) + (v.endsWith('/') && v !== '/' ? '/' : '');
      rows = rows.filter(r => r.path.startsWith(full) && r.path !== full.replace(/\/$/, ''));
      draw();
      ask({ q: full }, []);
      return;
    }
    // a word: the known folders that contain it, then the project's neighbours that begin with it
    const w = v.toLowerCase();
    rows = known().filter(r => baseName(r.path).toLowerCase().includes(w) || shortDir(r.path).toLowerCase().includes(w));
    draw();
    ask({ like: v }, rows);
  };
  const pick = d => {
    closeMenu();
    o.onPick(absDir(d));
    const ta = padOpenTa();
    if (ta && o.refocus !== false) ta.focus();
  };
  inp.addEventListener('input', refresh);
  inp.addEventListener('keydown', e => {
    e.stopPropagation();
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); if (rows.length) { at = (at + (e.key === 'ArrowDown' ? 1 : rows.length - 1)) % rows.length; mark(); } }
    else if (e.key === 'Enter') {
      e.preventDefault();
      // (typed faster than the disk answered: take what it lists for all of it)
      const v = inp.value.trim(), now = () => { if (rows[at]) pick(rows[at].path); else if (v && pathy(v)) pick(v); };
      if (pending) pending.then(() => { if (menuEl === m) now(); }); else now();
    } else if (e.key === 'Tab') {
      e.preventDefault();
      const go = () => { if (rows[at]) { inp.value = shortDir(rows[at].path) + '/'; refresh(); } };
      if (pending) pending.then(() => { if (menuEl === m) go(); }); else go();
    } else if (e.key === 'Escape') { e.preventDefault(); closeMenu(); const ta = padOpenTa(); if (ta) ta.focus(); }
  });
  rows = known(); draw(); inp.focus();
  // the project's neighbours, asked again each time (a folder may have appeared)
  req('dirs', {}).then(r => {
    if (!r || !r.ok || menuEl !== m) return;
    S.sibs = sibs = r.dirs || [];
    if (!inp.value.trim()) { rows = known(); draw(); }
  });
  return m;
}

/* what the folder line and ctrl+o are about: the selected agents, else the next new one */
function folderTargets() {
  const out = [];
  for (const id of S.sel) {
    const n = hostOf(S.nodes.get(id));
    if (n && kindOf(n) === 'agent' && n.agent !== 'step' && !out.includes(n.id)) out.push(n.id);
  }
  return out;
}
function pickFolderAt(x, y, above) {
  const ids = folderTargets();
  if (S.sel.size && !ids.length) return;   // a git step, a note: they work where their agent does
  if (ids.length) {
    const dirs = [...new Set(ids.map(i => workDir(S.nodes.get(i))))];
    const who = ids.length > 1 ? `${ids.length} agents` : S.nodes.get(ids[0]).name;
    return folderPicker(x, y, { above, title: `move ${who} to`, cur: dirs.length === 1 ? dirs[0] : '', onPick: d => moveToDir(ids, d) });
  }
  folderPicker(x, y, { above, title: 'start new agents in', cur: newDir(), onPick: setNewDir });
}
/* ctrl+o: the folder picker under the launch pad's folder, or above the bar */
function folderKey() {
  if (pad.open) {
    const b = $('.pname', padEl);
    if (!b) return;
    const r = b.getBoundingClientRect();
    return folderPicker(r.left, r.bottom + 6, { title: 'start new agents in', cur: newDir(), onPick: setNewDir });
  }
  const w = $('#where').getBoundingClientRect();
  pickFolderAt(w.left + 8, w.top - 2, true);
}
/* a card's folder tag. Only while the cards on the canvas work in more than one folder: then every
   card says where it works, the project folder too; all in one folder (any one), no card needs it */
const FD = { many: false };
function folderSpread() {
  const dirs = new Set();
  for (const n of S.nodes.values()) if (kindOf(n) !== 'note') for (const d of workDirs(n)) dirs.add(d);
  FD.many = dirs.size > 1;
}
function renderFolderTag(id, n, el) {
  const k = kindOf(n);
  const ds = FD.many && k !== 'note' ? workDirs(n) : [];
  const key = ds.join('|');
  if (el._fd === key) return;
  el._fd = key;
  el.fd.hidden = !ds.length;
  if (!ds.length) return;
  // (work in several repositories: their names, and no one folder's colour)
  if (ds.length === 1) el.fd.style.setProperty('--fh', folderHue(ds[0])); else el.fd.style.removeProperty('--fh');
  el.fd.classList.toggle('several', ds.length > 1);
  el.fd.textContent = ds.map(baseName).join(' + ');
  el.fd.title = `works in ${ds.map(shortDir).join(' and ')}` + (n.agent === 'step' ? ', in its agent\'s session (click to move the agent)'
    : k === 'agent' ? ' (click to move it)' : (n.cwd || '').trim() ? '' : ds.length > 1 ? ': one push and one pull request each' : ', where its agent does');
}
function folderTagClick(id) {
  const el = nodeEls.get(id), n = hostOf(S.nodes.get(id));   // (a queued prompt runs in its agent's session)
  if (!n || !el) return;
  const r = el.fd.getBoundingClientRect();
  // (a git step or gate picked a folder of its own no longer follows its agent)
  const title = kindOf(n) === 'agent' ? `move ${n.name} to` : `${n.name} works in (instead of its agent's)`;
  folderPicker(r.left, r.bottom + 4, { title, cur: workDir(n), refocus: false, onPick: d => moveToDir([n.id], d) });
}
/* "/crawler" in the bar: work in that folder (the selected agents move, else new agents start there) */
function folderCommands() {
  const seen = new Set([projDir()]), out = [projDir()];
  for (const d of [...(S.prefs.dirs || []), ...(S.sibs || []).map(s => s.path)]) if (!seen.has(d)) { seen.add(d); out.push(d); }
  return out.slice(0, 14).map(d => ({ label: 'folder ' + baseName(d), hint: shortDir(parentOf(d)), run: () => {
    const ids = folderTargets();
    if (ids.length) moveToDir(ids, d); else setNewDir(d);
  } }));
}
