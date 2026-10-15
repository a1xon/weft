/* ------------------------------------------------------------ toolbar and keys */
$('#bRun').addEventListener('click', () => send('run'));
$('#bLib').addEventListener('click', () => openLibrary());
$('#bTidy').addEventListener('click', tidy);
$('#bQuit').addEventListener('click', quitWeft);
$('#bStarter').addEventListener('click', starterFlow);
$('#bMore').addEventListener('click', e => {
  const r = e.currentTarget.getBoundingClientRect();
  popup([
    { label: 'fit everything', hint: 'f', run: fit },
    { label: 'reset flow progress', run: () => send('reset') },
    { label: 'stop all agents', danger: true, run: () => { for (const [id, rr] of S.rt) if (rr.alive) send('stop', { id }); } },
    { label: 'quit weft\u2026', danger: true, hint: 'q', run: quitWeft },
    { sep: true },
    { label: 'copy tmux attach command', run: () => { const c = `tmux -L ${S.project.sock} attach -t ${S.project.session}`; navigator.clipboard && navigator.clipboard.writeText(c); toast('ok', 'copied: ' + c); } },
    { label: 'keyboard and mouse', run: shortcuts },
  ], r.right - 250, r.bottom + 6);
});
$('#zIn').addEventListener('click', () => zoomAt(innerWidth / 2, innerHeight / 2, 1.2));
$('#zOut').addEventListener('click', () => zoomAt(innerWidth / 2, innerHeight / 2, 1 / 1.2));
$('#zFit').addEventListener('click', fit);

function shortcuts() {
  const rows = [
    ['type + ctrl+enter', 'new agent, or message the selected one'], ['tab', 'switch the new agent type'],
    ['double-click canvas', 'place an agent right there'], ['click a ◇', 'add the next (or previous) step'],
    ['drag a ◇ to empty space', 'spawn the same agent there, linked'], ['  + alt', 'choose the type instead'],
    ['drag a ◇ onto an agent', 'hand its result over'], ['click a link’s chip', 'always / if ok / if fails'],
    ['double-click a link', 'insert a step into it'], ['alt-drag an agent', 'clone it'],
    ['drag a finished flow’s first card', 'the whole flow comes along'], ['  + ctrl', 'just that card'],
    ['shift+click, shift+drag', 'select several'], ['g', 'gather the selection into one agent'],
    ['right-click anything', 'what it can do'], ['ctrl+o', 'the folder new agents (or the selected) work in'], ['/', 'commands and standard prompts'], ['@', 'insert another agent’s result'],
    ['enter  r  ,', 'terminal / result / options'], ['t  f', 'tidy / fit'], ['del  ctrl+z', 'delete / undo'], ['q', 'quit weft'],
  ];
  popup(rows.map(([a, b]) => ({ label: a, hint: b, run() {} })), innerWidth / 2 - 240, 70);
}

addEventListener('keydown', e => {
  const t = e.target, tag = (t.tagName || '').toLowerCase();
  const typing = tag === 'input' || tag === 'textarea' || tag === 'select' || t.id === 'term';
  if ((e.key === 'k' || e.key === 'K') && (e.ctrlKey || e.metaKey)) { e.preventDefault(); omniTa.focus(); omniTa.value = '/'; growOmni(); omniMenu(); return; }
  if ((e.key === 'o' || e.key === 'O') && (e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && t.id !== 'term') { e.preventDefault(); folderKey(); return; }
  if (e.key === 'z' && (e.ctrlKey || e.metaKey) && !typing) { e.preventDefault(); return undo(); }
  if (SB.id && !(typing && e.key !== 'Escape')) {
    if (e.key === 'Escape') return closeShotbox();
    if (e.key === 'ArrowLeft') { e.preventDefault(); return stepShotbox(-1); }
    if (e.key === 'ArrowRight') { e.preventDefault(); return stepShotbox(1); }
  }
  if (e.key === 'Escape') {
    if (NX.id) return closeNext();
    if (NBX.net) return closeNetBox();
    if (menuEl) return closeMenu();
    if (!modal.hidden) { modal.hidden = true; return; }
    if (!focusEl.hidden && t.id !== 'term') return closeFocus();
    if (pad.open) return padEsc();
    if (S.fly && !typing) return closeFly();
    if (!typing) select([]);
    return;
  }
  if (typing || !focusEl.hidden || !modal.hidden || pad.open) return;
  const p = primary();
  if ((e.key === 'n' || e.key === 'N') && !e.ctrlKey && !e.metaKey && !e.altKey) { e.preventDefault(); return openPad(); }
  if (e.key === 'Delete' || e.key === 'Backspace') {
    if (S.selEdge) delEdge(S.selEdge); else if (S.sel.size) deleteNodes([...S.sel]);
  } else if (e.key === 'Enter' && p) { e.preventDefault(); const n = S.nodes.get(p); if (focusPlea(p)) return; if (kindOf(n) === 'agent') openFocus(p); else if (kindOf(n) === 'note') editNote(p); }
  else if ((e.key === 'q' || e.key === 'Q') && !e.ctrlKey && !e.metaKey && !e.altKey) quitWeft();
  else if (e.key === 'f' || e.key === 'F') fit();
  else if ((e.key === 'b' || e.key === 'B') && !e.ctrlKey && !e.metaKey && !e.altKey) toggleBoards();
  else if ((e.key === 't' || e.key === 'T') && !e.ctrlKey && !e.metaKey) tidy();
  else if ((e.key === 'g' || e.key === 'G') && S.sel.size > 1) gather([...S.sel]);
  else if (e.key === 'r' && p) openFly(p, 'result');
  else if (e.key === ',' && p) openFly(p, 'options');
  else if (e.key === '/') { e.preventDefault(); omniTa.focus(); }
  else if (e.key === '+' || e.key === '=') zoomAt(innerWidth / 2, innerHeight / 2, 1.2);
  else if (e.key === '-') zoomAt(innerWidth / 2, innerHeight / 2, 1 / 1.2);
  else if (e.key === 'a' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); select([...S.nodes.keys()]); }
  else if (e.key.startsWith('Arrow') && p) {
    const c = S.nodes.get(p); const dir = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] }[e.key];
    let best = null, bd = 1e9;
    for (const n of S.nodes.values()) {
      if (n.id === p) continue;
      const dx = n.x - c.x, dy = n.y - c.y, along = dx * dir[0] + dy * dir[1];
      if (along <= 10) continue;
      const d = along + Math.abs(dir[0] ? dy : dx) * 2.2;
      if (d < bd) { bd = d; best = n.id; }
    }
    if (best) { e.preventDefault(); select([best]); centerOn(best); }
  }
});
addEventListener('resize', () => { renderOmni(); placeFly(); placeNext(); placeNetBox(); });

function toast(kind, text, opts = {}) {
  const o = typeof opts === 'number' ? { ms: opts } : opts;
  const t = h('div', { class: 'toast ' + (kind || 'info'), role: 'status' }, h('span', {}, text),
    o.action ? h('button', { class: 'btn sm', onclick: () => { t.remove(); o.action.run(); } }, o.action.label) : null);
  $('#toasts').append(t);
  setTimeout(() => t.remove(), o.ms || 5200);
}
