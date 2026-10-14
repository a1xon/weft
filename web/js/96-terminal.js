/* ------------------------------------------------------------ deleting with undo */
function snapshotNodes(ids) {
  const set = new Set(ids);
  return {
    nodes: ids.map(i => S.nodes.get(i)).filter(Boolean).map(n => JSON.parse(JSON.stringify(n))),
    edges: [...S.edges.values()].filter(e => set.has(e.from) || set.has(e.to)).map(e => ({ ...e })),
  };
}
async function restore(snap) {
  const map = new Map();
  for (const n of snap.nodes) {
    const c = { ...n }; const old = c.id; delete c.id;
    const r = await req('node_add', { node: c });
    if (r.id) map.set(old, r.id);
  }
  for (const e of snap.edges) {
    const f = map.get(e.from) || (S.nodes.has(e.from) ? e.from : null), t = map.get(e.to) || (S.nodes.has(e.to) ? e.to : null);
    if (f && t) send('edge_add', { from: f, to: t, on: e.on });
  }
  await Promise.all([...map.values()].map(whenNode));
  select([...map.values()]);
}
function deleteNodes(ids) {
  if (!ids.length) return;
  const snap = snapshotNodes(ids);
  if (!snap.nodes.length) return;
  for (const id of ids) send('node_del', { id });
  S.undo.push(() => restore(snap));
  toast('info', ids.length > 1 ? `deleted ${ids.length} agents` : `deleted "${snap.nodes[0].name}"`, { ms: 8000, action: { label: 'undo', run: undo } });
}
function delEdge(id) {
  const e = S.edges.get(id); if (!e) return;
  const snap = { ...e };
  send('edge_del', { id });
  S.undo.push(async () => { send('edge_add', { from: snap.from, to: snap.to, on: snap.on }); });
  toast('info', 'link removed', { ms: 6000, action: { label: 'undo', run: undo } });
}
function undo() { const f = S.undo.pop(); if (f) f(); else toast('info', 'nothing to undo'); }

/* ------------------------------------------------------------ focused terminal */
const focusEl = $('#focus');
let termEl = null, rowEls = [], termCols = 0;
function openFocus(id) {
  const n = S.nodes.get(id); if (!n || kindOf(n) !== 'agent') return;
  S.focus = id; S.scroll = 0; S.hist = 0; closeMenu();
  const stx = h('span', { class: 'stx' }), st = h('span', { class: 'st' });
  termEl = h('div', { id: 'term', tabindex: '0', role: 'textbox', 'aria-label': `Terminal of ${n.name}`, 'aria-multiline': 'true' });
  const badge = h('button', { class: 'btn sm pri', id: 'scrollbadge', hidden: true, onclick: () => send('scroll', { id, offset: 0 }) }, 'jump to latest');
  focusEl.replaceChildren(h('div', { class: 'win chamf' },
    h('div', { class: 'fh2' }, st, h('span', { class: 'nm' }, n.name), stx,
      h('button', { class: 'btn sm', title: 'Esc goes to the agent while you type here. Click outside or use this button to leave.', onclick: closeFocus }, 'back to canvas')),
    termEl, badge));
  focusEl.hidden = false; rowEls = []; termCols = 0;
  focusEl._refs = { st, stx, badge };
  focusEl.onpointerdown = e => { if (e.target === focusEl) closeFocus(); };
  termEl.addEventListener('keydown', onTermKey);
  termEl.addEventListener('paste', e => { e.preventDefault(); const t = e.clipboardData.getData('text'); if (t) send('text', { id: S.focus, text: t, paste: true }); });
  termEl.addEventListener('wheel', e => {
    e.preventDefault();
    const step = Math.max(1, Math.round(Math.abs(e.deltaY) / 40)) * 2;
    const off = Math.max(0, S.scroll + (e.deltaY < 0 ? step : -step));
    send('scroll', { id: S.focus, offset: Math.min(off, S.hist || off) });
  }, { passive: false });
  updateFocusHead();
  send('watch', { id });
  setTimeout(() => termEl && termEl.focus(), 30);
}
function closeFocus() {
  if (S.focus) send('watch', { id: null });
  S.focus = null; focusEl.hidden = true; focusEl.replaceChildren(); termEl = null;
}
function updateFocusHead() {
  const r = focusEl._refs; if (!r || !S.focus) return;
  const n = S.nodes.get(S.focus), rt = S.rt.get(S.focus); if (!n) return;
  const st = rt ? rt.status : 'new';
  r.st.innerHTML = ICON[st] || ICON.new; r.st.style.color = 'var(--i2)';
  r.stx.textContent = statusText(n, rt);
}
/* the terminal is as large as the window allows: by its width, by its height, up to 24px */
function fitTerm() {
  if (!termEl || !rowEls.length) return;
  const byW = (innerWidth - 80) / (termCols * 0.605), byH = (innerHeight - 140) / (rowEls.length * 1.3);
  termEl.style.fontSize = Math.max(10, Math.min(24, byW, byH)) + 'px';
  termEl.style.width = `calc(${termCols}ch + 22px)`;
  termEl.style.maxHeight = (innerHeight - 130) + 'px';
}
addEventListener('resize', fitTerm);
function onScreen(m) {
  if (m.id !== S.focus || !termEl) return;
  if (rowEls.length !== m.rows || termCols !== m.cols) {
    termCols = m.cols; termEl.replaceChildren(); rowEls = [];
    for (let i = 0; i < m.rows; i++) { const d = h('div', { class: 'r' }, ' '); rowEls.push(d); termEl.append(d); }
    fitTerm();
  }
  for (const [i, html] of m.rowsd) if (rowEls[i]) rowEls[i].innerHTML = html || ' ';
  S.scroll = m.scroll; S.hist = m.hist;
  focusEl._refs.badge.hidden = !m.scroll;
}
function onTermKey(e) {
  if (e.isComposing) return;
  const k = e.key;
  if (k === 'v' && (e.ctrlKey || e.metaKey)) return;
  if ((k === 'c' || k === 'C') && (e.ctrlKey || e.metaKey) && getSelection().toString()) return;
  if (['Shift', 'Control', 'Alt', 'Meta', 'CapsLock', 'AltGraph'].includes(k)) return;
  const names = { Enter: 'Enter', Escape: 'Escape', Backspace: 'BSpace', ArrowUp: 'Up', ArrowDown: 'Down', ArrowLeft: 'Left', ArrowRight: 'Right', Home: 'Home', End: 'End', PageUp: 'PageUp', PageDown: 'PageDown', Delete: 'DC', Insert: 'IC', ' ': 'Space' };
  let key = null, text = null;
  if (k === 'Tab') key = e.shiftKey ? 'BTab' : 'Tab';
  else if (names[k]) key = names[k];
  else if (/^F([1-9]|1[0-2])$/.test(k)) key = k;
  if (key) {
    let p = '';
    if (e.ctrlKey && key !== 'BTab') p += 'C-';
    if (e.altKey) p += 'M-';
    if (e.shiftKey && key.length > 1 && !['BTab', 'Tab', 'Enter', 'Escape', 'BSpace'].includes(key)) p += 'S-';
    if (key === 'Space' && !e.ctrlKey && !e.altKey) { text = ' '; key = null; } else key = p + key;
  } else if ([...k].length === 1) {
    if (e.ctrlKey && /^[a-zA-Z]$/.test(k)) key = (e.altKey ? 'M-' : '') + 'C-' + k.toLowerCase();
    else if (e.ctrlKey || e.metaKey) return;
    else if (e.altKey && /^[a-zA-Z0-9]$/.test(k)) key = 'M-' + k;
    else text = k;
  } else return;
  e.preventDefault();
  if (key) send('key', { id: S.focus, k: key });
  else if (text != null) send('text', { id: S.focus, text, paste: false });
}
