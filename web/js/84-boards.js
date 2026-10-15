/* boards: every folder weft was started in is a board, with a daemon of its own. The launch pad
   lists them above its folder, and the board's name in the top bar unfolds the same list (B): what
   each board is doing, a badge when another one needs you. A click switches this window to that
   board (its daemon wakes if it sleeps), ctrl+click opens it in a window of its own, "+ new board"
   makes one in a folder you pick. The daemon sends the list (`boards`) whenever it changes; one
   from before boards sends none, and the page then shows none of this. */
const BD = { on: false, list: [], busy: '', menu: null };
let bootAt = '';   // `weft` started in a folder of this board: new agents start there (99-boot.js)

function onBoards(list) {
  BD.list = Array.isArray(list) ? list : [];
  const me = BD.list.find(b => b.me);
  if (me) onBoardName(me.name);
  renderBoardChip();
  drawPadBoards();
  if (BD.menu && menuEl === BD.menu) drawBoardMenu();
}
function onBoardName(name) {
  if (!S.project || !name || S.project.name === name) return;
  S.project.name = name;
  document.title = `${name} – weft`;
  renderBoardChip();
}
/* `weft` was started in a folder inside this board's: new agents start there */
function applyBootAt() {
  const d = bootAt; bootAt = '';
  if (d && d !== newDir()) setNewDir(d);
}

/* names: a board's own, with its parent folder's where two boards share one */
function boardLabel(b) {
  const twin = BD.list.some(o => o !== b && o.name === b.name);
  return twin ? baseName(parentOf(b.dir).replace(/\/$/, '')) + '/' + b.name : b.name;
}
function boardStats(b) {
  const pl = (n, one, many) => `${n} ${n === 1 ? one : many}`;
  if (!b.up) return [h('span', { class: 'bdz' }, b.cards ? `asleep · ${pl(b.cards, 'card', 'cards')}` : 'asleep')];
  const out = [h('span', {}, b.agents ? pl(b.agents, 'agent', 'agents') : 'no agents running')];
  if (b.working) out.push(h('span', { class: 'bdw' }, `${b.working} working`));
  if (b.needs) out.push(h('span', { class: 'bdv' }, `${b.needs} need${b.needs === 1 ? 's' : ''} you`));
  if (b.agents && !b.working && !b.needs) out.push(h('span', { class: 'bdz' }, 'idle'));
  return out.flatMap((x, i) => i ? [h('span', { class: 'bdsep' }, '·'), x] : [x]);
}
function boardRow(b, o = {}) {
  const busy = BD.busy === b.dir;
  const cls = ['bdrow', b.me ? 'me' : '', b.up ? 'up' : 'asleep', b.needs ? 'needs' : '', busy ? 'busy' : ''].filter(Boolean).join(' ');
  const row = h('button', { class: cls, 'data-dir': b.dir,
    title: b.me ? `${b.dir}\nthe board you are on${o.menu ? ' · double-click the name to rename it' : ''}`
      : `${b.dir}\nclick: switch to it · ctrl+click: a window of its own`,
    onclick: ev => { if (!b.me) openBoard(b.dir, ev); } },
    h('span', { class: 'bdd' }),
    h('span', { class: 'bdn' }, boardLabel(b)),
    h('span', { class: 'bds' }, ...(busy ? [h('span', { class: 'bdw' }, b.up ? 'opening…' : 'waking up…')] : boardStats(b))),
    !b.me && !b.up && o.menu ? h('span', { class: 'bdx', role: 'button', title: 'leave it off the list (its cards stay in its folder)',
      onclick: ev => { ev.stopPropagation(); forgetBoard(b); } }, '×') : null);
  row.addEventListener('auxclick', ev => { if (ev.button === 1 && !b.me) { ev.preventDefault(); openBoard(b.dir, ev); } });
  if (b.me && o.menu) row.querySelector('.bdn').addEventListener('dblclick', ev => { ev.stopPropagation(); renameBoard(row); });
  return row;
}
const newBoardBtn = () => h('button', { class: 'bdrow bdnew', title: 'a board of its own for another folder',
  onclick: ev => newBoard(ev.currentTarget) }, h('span', { class: 'bdd' }, '+'), h('span', { class: 'bdn' }, 'new board'));

/* the launch pad: the boards above the folder (only the way to a new one while there is just one) */
const padBoardsEl = h('div', { class: 'pboards' });
function padBoards() { drawPadBoards(); return padBoardsEl; }
function drawPadBoards() {
  padBoardsEl.hidden = !BD.on;
  const many = BD.list.length > 1;
  padBoardsEl.classList.toggle('one', !many);
  padBoardsEl.replaceChildren(...(many ? BD.list.map(b => boardRow(b)) : []), newBoardBtn());
}

/* the top bar: the board's name, its folder, and a badge when another board needs you */
function renderBoardChip() {
  const el = $('#projName');
  if (!el || !S.project) return;
  const dir = S.project.dir || '', hm = S.project.home || '';
  const path = hm && (dir === hm || dir.startsWith(hm + '/')) ? '~' + dir.slice(hm.length) : dir;
  if (!BD.on) { el.textContent = path; el.classList.remove('bdon'); return; }
  el.classList.add('bdon');
  const named = S.project.name && S.project.name !== baseName(dir);
  el.replaceChildren(h('b', {}, S.project.name || baseName(dir)), h('span', { class: 'bdpath' }, named ? path : parentOf(path)));
  el.title = `${dir}\nyour boards (B)`;
  el.setAttribute('role', 'button'); el.tabIndex = 0;
  const waiting = BD.list.filter(b => !b.me && b.needs);
  let badge = $('#bdBadge');
  if (!badge) { badge = h('button', { id: 'bdBadge', class: 'tk vi' }); el.after(badge); badge.addEventListener('click', ev => openBoard(badge.dataset.dir, ev)); }
  badge.hidden = !waiting.length;
  if (waiting.length) {
    badge.dataset.dir = waiting[0].dir;
    badge.replaceChildren(waiting.length === 1 ? h('b', {}, boardLabel(waiting[0])) : h('b', {}, `${waiting.length} boards`), waiting.length === 1 ? ' needs you' : ' need you');
    badge.title = waiting.map(b => `${boardLabel(b)}: ${b.needs} need${b.needs === 1 ? 's' : ''} you`).join('\n') + '\nclick: switch to it';
  }
}
$('#projName').addEventListener('click', () => toggleBoards());
$('#projName').addEventListener('keydown', ev => { if (ev.key === 'Enter' || ev.key === ' ') { ev.preventDefault(); toggleBoards(); } });

/* the list under the name */
function toggleBoards() {
  if (!BD.on) return;
  if (BD.menu && menuEl === BD.menu) return closeMenu();
  closeMenu();
  const m = h('div', { class: 'menu bdmenu', role: 'dialog', 'aria-label': 'Boards' });
  const r = $('#projName').getBoundingClientRect();
  m.style.left = Math.max(8, r.left - 10) + 'px'; m.style.top = (r.bottom + 9) + 'px';
  document.body.append(m);
  menuEl = BD.menu = m;
  const outside = ev => { if (!m.contains(ev.target) && !$('#projName').contains(ev.target)) closeMenu(); };
  setTimeout(() => {
    if (menuEl !== m) return;
    addEventListener('pointerdown', outside, true);
    menuOff = () => { removeEventListener('pointerdown', outside, true); BD.menu = null; };
  }, 0);
  m.addEventListener('keydown', ev => {
    const rows = [...m.querySelectorAll('.bdrow')], at = rows.indexOf(document.activeElement);
    if (ev.key === 'ArrowDown' || ev.key === 'ArrowUp') { ev.preventDefault(); rows[(at + (ev.key === 'ArrowDown' ? 1 : rows.length - 1) + rows.length) % rows.length].focus(); }
    else if (ev.key === 'Escape') { ev.preventDefault(); ev.stopPropagation(); closeMenu(); }
  });
  drawBoardMenu();
  send('proj');   // (and the daemon looks at the boards again for an open page anyway)
  const first = m.querySelector('.bdrow:not(.me)') || m.querySelector('.bdrow');
  if (first) first.focus();
}
function drawBoardMenu() {
  const m = BD.menu;
  if (!m) return;
  const had = document.activeElement && m.contains(document.activeElement) ? document.activeElement.dataset.dir || 'new' : null;
  if (m.querySelector('input')) return;   // (renaming: the list waits)
  m.replaceChildren(h('div', { class: 'cap' }, 'boards'), ...BD.list.map(b => boardRow(b, { menu: true })), h('div', { class: 'sep' }), newBoardBtn(),
    h('div', { class: 'fkeys' }, ...keyHints([['↑↓', 'choose'], ['enter', 'switch'], ['ctrl+click', 'own window']])));
  if (had) { const b = had === 'new' ? m.querySelector('.bdnew') : [...m.querySelectorAll('.bdrow')].find(x => x.dataset.dir === had); if (b) b.focus(); }
}

/* switch this window to a board (ctrl / middle click: a window of its own); it wakes up if it sleeps */
function openBoard(dir, ev) {
  if (!dir || BD.busy) return;
  const win = !!(ev && (ev.ctrlKey || ev.metaKey || ev.shiftKey || ev.button === 1));
  BD.busy = dir; drawPadBoards(); drawBoardMenu();
  const done = () => { BD.busy = ''; drawPadBoards(); drawBoardMenu(); };
  // (a board that sleeps takes a moment to come up: longer than an ordinary request may)
  new Promise(res => {
    const rid = ++S.rid;
    S.acks.set(rid, res);
    send('board_open', { dir, window: win, rid });
    setTimeout(() => { if (S.acks.delete(rid)) res({ ok: false, error: 'it did not come up in time' }); }, 20000);
  }).then(r => {
    if (!r || !r.ok) { done(); toast('err', `${baseName(dir)}: ${(r && r.error) || 'could not open it'}`); return; }
    if (win) { done(); closeMenu(); toast('info', `${baseName(dir)} opens in a window of its own`); return; }
    location.href = r.url;
  });
}
function newBoard(anchor) {
  const r = anchor.getBoundingClientRect();
  folderPicker(r.left, r.bottom + 6, { title: 'a new board in', cur: '', refocus: false, onPick: d => openBoard(d) });
}
function forgetBoard(b) {
  req('board_forget', { dir: b.dir }).then(r => {
    if (!r || !r.ok) return toast('err', (r && r.error) || 'could not');
    BD.list = BD.list.filter(x => x.dir !== b.dir);
    onBoards(BD.list);
    toast('info', `${boardLabel(b)} is off the list; its cards are still in ${b.dir}`);
  });
}
function renameBoard(row) {
  // (a field cannot sit in a button: the row makes way for one)
  const inp = h('input', { class: 'fld bdin', value: S.project.name || '', spellcheck: 'false', 'aria-label': 'Board name', placeholder: baseName(projDir()) });
  row.replaceWith(h('div', { class: 'bdrow me renaming' }, h('span', { class: 'bdd' }), inp, h('span', { class: 'bds' }, 'enter: keep it · empty: the folder\'s name')));
  inp.focus(); inp.select();
  let gone = false;
  const end = save => {
    if (gone) return; gone = true;
    const v = inp.value.trim();
    if (save) req('board_name', { name: v }).then(r => { if (r && r.ok) { onBoardName(r.name); BD.list.forEach(b => { if (b.me) b.name = r.name; }); } drawBoardMenu(); });
    else drawBoardMenu();
  };
  inp.addEventListener('keydown', ev => {
    ev.stopPropagation();
    if (ev.key === 'Enter') { ev.preventDefault(); inp.blur(); end(true); }
    else if (ev.key === 'Escape') { ev.preventDefault(); end(false); }
  });
  inp.addEventListener('blur', () => end(true));
}
