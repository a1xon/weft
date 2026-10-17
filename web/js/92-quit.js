/* ------------------------------------------------------------ quit */
// Nothing running means nothing to lose, so it just quits. With agents running the question is
// what happens to them: ending them is the default, because a window left behind is the problem.
let gone = false, leaving = null;
function quitWeft() {
  const live = [...S.nodes.values()].filter(n => kindOf(n) === 'agent' && (S.rt.get(n.id) || {}).alive);
  const working = live.filter(n => ['busy', 'starting', 'attention'].includes(S.rt.get(n.id).status));
  const many = (k, w) => k + ' ' + w + (k === 1 ? '' : 's');
  const names = working.slice(0, 3).map(n => n.name).join(', ') + (working.length > 3 ? ' and ' + (working.length - 3) + ' more' : '');
  const end = h('button', { class: 'btn danger', onclick: () => leave(true, live.length) }, live.length ? 'end all agents and quit' : 'quit');
  modal.replaceChildren(h('div', { class: 'win quit chamf' },
    h('div', { class: 'mh' }, h('h2', {}, 'quit weft?'), h('button', { class: 'ib', 'aria-label': 'Close', html: ICON.x, onclick: () => { modal.hidden = true; } })),
    h('div', { class: 'qb' },
      live.length
        ? h('p', { class: 'who' }, many(live.length, 'agent') + (live.length === 1 ? ' is' : ' are') + ' running' + (working.length ? '; ' + names + (working.length === 1 ? ' is' : ' are') + ' working right now.' : '.'))
        : h('p', { class: 'who' }, 'No agent is running.'),
      h('p', { class: 'note' }, live.length
        ? 'Ending them closes their terminals. The canvas, results and worktrees stay saved; weft in this folder brings them back.'
        : 'The canvas and its results stay saved; weft in this folder brings them back.'),
      h('div', { class: 'qa' }, end,
        live.length ? h('button', { class: 'btn', onclick: () => leave(false, live.length) }, 'quit, keep agents running') : null,
        h('button', { class: 'btn dim', onclick: () => { modal.hidden = true; } }, 'cancel')))));
  modal.onpointerdown = e => { if (e.target === modal) modal.hidden = true; };
  modal.hidden = false;
  end.focus();
}
function leave(end, agents) {
  modal.hidden = true;
  if (!ws || ws.readyState !== 1) return toast('err', 'not connected to weft');
  leaving = { end, agents };
  send('quit', { end });
}
// the daemon says goodbye; its closing socket does too, in case the message did not make it
function farewell(m) {
  if (gone) return;
  gone = true;
  closeMenu(); modal.hidden = true;
  const dir = shortDir(projDir());
  const n = m.agents || 0;
  const plural = n === 1 ? '' : 's';
  const what = m.end
    ? (n ? `${n} agent${plural} ended and ${n === 1 ? 'its terminal' : 'their terminals'} closed.` : 'Nothing was left running.')
    : `${n} agent${n === 1 ? ' keeps' : 's keep'} running in tmux.`;
  const keep = !m.end && n > 0;
  $('#bye').replaceChildren(h('div', { class: 'by' },
    h('span', { class: 'word' }, 'weft', h('i')),
    h('h2', {}, 'weft has stopped'),
    h('p', {}, what + ' You can close this tab.'),
    h('p', {}, 'Run weft in ' + (dir || 'the project folder') + ' to pick up where you left off.'),
    keep ? h('p', {}, 'To look at them:') : null,
    keep ? h('code', {}, `tmux -L ${S.project.sock || 'weft'} attach -t ${S.project.session || ''}`) : null,
    keep ? h('p', {}, 'To end them later:') : null,
    keep ? h('code', {}, 'weft stop') : null));
  $('#bye').hidden = false;
  document.title = 'weft stopped';
}
