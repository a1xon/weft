/* ------------------------------------------------------------ rewind: what each step changed, and undo (engine/rewind.rs)
   Every step an agent takes is snapshotted before and after (in a shadow repository under .weft/cp/:
   the folder, its index and its branches never change). A step that changed files is a dot above the
   card's footer; a click shows its diff full size, with "undo this step" and "rewind to before it".
   Undo puts back only the files that step changed; a file changed after it is left and named. A
   rewind is a dot itself, so it can be undone too. */
const cpsOf = r => (r && Array.isArray(r.cps) ? r.cps : []);
function renderCps(id, n, r) {
  const el = nodeEls.get(id); if (!el) return;
  // (a first step still running is no row yet: every working card would carry one)
  const cps = cpsOf(r).some(c => !c.open) ? cpsOf(r) : [];
  if (!cps.length) { if (el.cps) { el.cps.remove(); el.cps = null; } return; }
  if (!el.cps) {
    el.cps = h('div', { class: 'cps', title: 'what each step changed: click a dot for its diff and to undo it' });
    el.root.querySelector('.node').insertBefore(el.cps, el.root.querySelector('.node .ft'));
  }
  const key = JSON.stringify(cps);
  if (el.cps._key === key) return;
  el.cps._key = key;
  el.cps.replaceChildren(h('span', { class: 'cpl' }, 'steps'), ...cps.map(c => {
    const tip = c.rewind ? `${c.what}: ${c.files} file${c.files === 1 ? '' : 's'} put back${c.undone ? ' (undone)' : ''}`
      : c.open ? `step ${c.n}, running: ${c.what || 'its prompt'}`
        : `step ${c.n}: ${c.what || 'its prompt'}\n${c.files} file${c.files === 1 ? '' : 's'}: ${c.names.join(', ')}${c.files > c.names.length ? ', …' : ''}${c.undone ? '\n(undone)' : ''}`;
    return h('button', { class: 'cpd' + (c.open ? ' open' : '') + (c.undone ? ' undone' : '') + (c.rewind ? ' rw' : ''), 'data-n': String(c.n), title: tip,
      onclick: ev => { ev.stopPropagation(); openStep(id, c.n); } }, c.rewind ? '↺' : '');
  }));
}
/* a step's diff, full size, in the reader's window */
async function openStep(id, num) {
  const n = S.nodes.get(id); if (!n) return;
  const c = cpsOf(S.rt.get(id)).find(x => x.n === num); if (!c) return;
  closeMenu(); closeFly();
  RD.id = null;
  const b = (label, title, fn, cls = '') => h('button', { class: 'btn sm ' + cls, title, onclick: fn }, label);
  const body = h('div', { class: 'rbody diff', tabindex: '0', 'aria-label': `What step ${num} of ${n.name} changed` }, h('div', { class: 'note' }, 'loading…'));
  const cps = cpsOf(S.rt.get(id)), done = cps.filter(x => !x.open);
  const last = done.length ? done[done.length - 1].n : num;
  const undo = (from, to, what) => req('rewind', { id, from, to }).then(r => {
    if (!r.ok) return toast('warn', r.error || 'it cannot be undone now');
    toast('info', what + '…'); closeReader();
  });
  const acts = c.open ? [h('span', { class: 'src' }, 'still running: what it changed so far')]
    : c.undone ? [h('span', { class: 'src' }, 'undone')]
      : c.rewind ? [b('undo this rewind', 'put back what the rewind took away', () => undo(num, num, 'putting it back'), 'cy')]
        : [b('undo this step', 'its files go back to how they were before it; files changed since are left as they are', () => undo(num, num, 'undoing it'), 'cy'),
          ...(num !== last ? [b('rewind to before it', `undo this step and every one after it (to step ${last})`, () => undo(num, last, 'rewinding'))] : [])];
  readerEl.replaceChildren(h('div', { class: 'win chamf' },
    h('div', { class: 'fh2' }, h('span', { class: 'nm' }, n.name),
      h('span', { class: 'stx' }, c.rewind ? c.what : `step ${num} · ${c.what || 'its prompt'}`),
      h('span', { class: 'cpf' }, `${c.files || ''} ${c.files ? (c.files === 1 ? 'file' : 'files') : ''}`),
      h('span', { class: 'rz' }, b('A−', 'smaller text (-)', () => zoomReader(-1)), b('A+', 'larger text (+)', () => zoomReader(1))),
      ...acts, b('back to canvas', 'Esc', closeReader)),
    body));
  readerEl._refs = { body };
  readerEl.style.setProperty('--rz', RD.z);
  readerEl.hidden = false;
  readerEl.onpointerdown = e => { if (e.target === readerEl) closeReader(); };
  const r = await req('cp_diff', { id, n: num });
  if (readerEl.hidden || readerEl._refs?.body !== body) return;
  if (!r.ok) { body.replaceChildren(h('div', { class: 'note' }, r.error || 'it cannot be shown')); return; }
  body.replaceChildren(diffView(r.diff || ''));
  setTimeout(() => body.focus(), 30);
}
/* a unified diff, coloured the way a terminal colours it */
function diffView(text) {
  const out = h('div', { class: 'dv' });
  for (const l of text.split('\n')) {
    const cls = /^diff --git /.test(l) ? 'df' : /^(\+\+\+|---) /.test(l) ? 'dh' : /^@@/.test(l) ? 'dk' : /^\+/.test(l) ? 'da' : /^-/.test(l) ? 'dd'
      : /^(index |new file|deleted file|similarity|rename |old mode|new mode)/.test(l) ? 'dm' : / \| +\d+ [+-]*$| files? changed/.test(l) ? 'ds' : '';
    out.append(h('div', { class: 'dl ' + cls }, l || ' '));
  }
  return out;
}
