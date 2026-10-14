/* ------------------------------------------------------------ flyout: unfolds from the card */
const fly = $('#fly');
/* is someone typing in a field of the flyout? (then a redraw would eat their text; a click on a
   button inside it is no reason to hold the redraw back) */
const insideFly = () => {
  const a = document.activeElement;
  return !!a && a !== fly && fly.contains(a) && /^(INPUT|TEXTAREA|SELECT)$/.test(a.tagName);
};
function openFly(id, tab) { S.fly = { id, tab }; renderFly(); placeFly(); }
function closeFly() { S.fly = null; fly.hidden = true; fly.replaceChildren(); }
function placeFly() {
  if (!S.fly || fly.hidden) return;
  const n = S.nodes.get(S.fly.id); if (!n) return;
  const k = S.v.k, x = S.v.x + n.x * k, y = S.v.y + n.y * k, w = NW * k, hgt = NH * k;
  const fw = fly.offsetWidth || 480, fh = fly.offsetHeight || 300;
  let left = Math.max(10, Math.min(x, innerWidth - fw - 10));
  let top = y + hgt + 10;
  if (top + fh > innerHeight - 10) top = Math.max(50, y - fh - 10);
  fly.style.left = left + 'px'; fly.style.top = top + 'px';
}
function renderFly() {
  const d = S.fly;
  if (!d || !S.nodes.has(d.id)) { fly.hidden = true; return; }
  const n = S.nodes.get(d.id);
  fly.hidden = false;
  const tab = (t, label) => h('button', { class: 'tab' + (d.tab === t ? ' on' : ''), onclick: () => { S.fly.tab = t; renderFly(); } }, label);
  fly.replaceChildren(
    h('div', { class: 'fh' }, h('span', { class: 'ttl' }, n.name), tab('result', 'result'), tab('options', 'options'),
      h('button', { class: 'ib', 'aria-label': 'Close', html: ICON.x, onclick: closeFly })),
    h('div', { class: 'fbody', id: 'fbody' }));
  if (d.tab === 'result') renderResultTab(n); else renderOptionsTab(n);
  placeFly();
  requestAnimationFrame(placeFly);
}
function patchNode(id, patch, debounce = 350) {
  const n = S.nodes.get(id); if (!n) return;
  Object.assign(n, patch);
  if ('name' in patch) n.auto_name = false;
  renderNode(id);
  const t = (patchNode.t = patchNode.t || {});
  clearTimeout(t[id]);
  // an immediate patch must reach the daemon before whatever the caller sends next (e.g. arm)
  if (debounce === 0) send('node_set', { id, patch });
  else t[id] = setTimeout(() => send('node_set', { id, patch }), debounce);
}
function seg(options, value, onPick) {
  return h('div', { class: 'seg' }, options.map(([v, label, tip]) =>
    h('button', { 'aria-pressed': String(value === v), title: tip || '', onclick: ev => {
      for (const b of ev.currentTarget.parentNode.children) b.setAttribute('aria-pressed', 'false');
      ev.currentTarget.setAttribute('aria-pressed', 'true'); onPick(v);
    } }, label)));
}
let resultSeq = 0;
function gateResult(n, rt) {
  const id = n.id, k = kindOf(n), review = rt && rt.gate_phase === 'review';
  const b = (label, cls, fn, dis) => h('button', { class: 'btn sm ' + cls, disabled: !!dis, onclick: fn }, label);
  const shots = (rt && rt.gate_shots) || [];
  const attach = h('input', { type: 'file', accept: 'image/*', hidden: true, onchange: ev => { const f = ev.target.files[0]; if (f) uploadShot(id, f); } });
  const parts = [];
  parts.push(h('div', { class: 'sec' }, h('h3', {}, 'status'), h('div', { class: 'kvr', style: 'color:var(--i1)' }, statusText(n, rt))));
  if (k === 'shot') {
    parts.push(h('div', { class: 'sec' }, h('h3', {}, shots.length > 1 ? `screenshots (${shots.length})` : 'screenshot'),
      ...(shots.length ? shots.map(s => imgEl(s, 'bigshot')) : [h('p', { class: 'note' }, rt && rt.gate_phase === 'asking' ? 'waiting for the agent to save one\u2026' : 'none yet')]),
      rt && rt.gate_out ? h('div', { class: 'pre dim', style: 'margin-top:6px' }, rt.gate_out) : null));
  } else if (k === 'tests') {
    parts.push(h('div', { class: 'sec' }, h('h3', {}, 'suite'),
      h('div', { class: 'kvr' }, h('span', { class: 'code' }, n.cmd ? '$ ' + n.cmd : 'the agent writes and runs it')),
      h('div', { class: 'pre tail ' + (rt && rt.gate_green === false ? 'bad' : rt && rt.gate_green ? 'ok' : 'dim'), style: 'margin-top:8px' }, (rt && rt.gate_out) || '(no output yet)')));
  } else {
    parts.push(h('div', { class: 'sec' }, h('h3', {}, 'held results'), h('div', { class: 'pre' }, (rt && rt.gate) || '(nothing is waiting)')));
  }
  const actions = [];
  if (review) {
    actions.push(b(k === 'tests' && rt.gate_green === false ? 'send failure to the agent' : (k === 'gate' ? 'approve' : 'confirm'), 'pri',
      () => k === 'tests' && rt.gate_green === false ? send('gate_retry', { id, fix: true }) : send('approve', { id, ok: true })));
    if (k !== 'gate') actions.push(b('request changes\u2026', '', () => { closeFly(); askChanges(id); }));
    if (k === 'shot') actions.push(b('retake', '', () => send('gate_retry', { id })));
    if (k === 'tests') actions.push(b('run again', '', () => send('gate_retry', { id })));
    if (k === 'tests' && rt.gate_green === false) actions.push(b('continue anyway', '', () => send('approve', { id, ok: true })));
    actions.push(b('reject', 'danger', () => send('approve', { id, ok: false })));
  } else if (rt && rt.gate_phase && rt.gate_phase !== 'idle') {
    actions.push(b('continue anyway', '', () => send('approve', { id, ok: true })), b('reject', 'danger', () => send('approve', { id, ok: false })));
  }
  if (k === 'shot') actions.push(b('attach an image\u2026', '', () => attach.click()));
  parts.push(h('div', { class: 'sec' }, h('div', { class: 'row2' }, ...actions, attach),
    k === 'shot' ? h('p', { class: 'note' }, 'You can also paste an image (ctrl+v) or drop one on the card.') : null));
  return parts;
}
async function renderResultTab(n) {
  const box = $('#fbody'); const seq = ++resultSeq;
  if (isGate(kindOf(n))) { box.replaceChildren(...nn(gateResult(n, S.rt.get(n.id)))); placeFly(); return; }
  box.replaceChildren(h('div', { class: 'sec' }, h('div', { class: 'note' }, 'loading…')));
  const r = await req('result', { id: n.id });
  if (seq !== resultSeq || !S.fly || S.fly.tab !== 'result' || S.fly.id !== n.id) return;
  const rt = S.rt.get(n.id);
  const others = [...S.nodes.values()].filter(x => x.id !== n.id && kindOf(x) !== 'note');
  const steps = r.steps || [];
  const k = kindOf(n);
  box.replaceChildren(...nn([
    h('div', { class: 'sec' }, h('h3', {}, 'status'),
      h('div', { class: 'kvr', style: 'color:var(--i1)' }, statusText(n, rt)),
      rt && rt.fresh ? h('p', { class: 'note', style: 'color:var(--cy)' }, 'You started this run by chatting, so it has not been handed on. Drag ◇ to another agent, or use the button below.') : null),
    k === 'agent' ? h('div', { class: 'sec' }, h('h3', {}, 'last prompt'), h('div', { class: 'pre dim' }, r.asked || '(none yet)')) : null,
    h('div', { class: 'sec' }, h('h3', {}, 'result'),
      h('div', { class: 'pre ' + (r.status === false ? 'bad' : r.text ? 'ok' : '') }, r.text || (rt && rt.gate) || '(no result yet)'),
      h('div', { class: 'row2' },
        h('button', { class: 'btn sm', disabled: !r.text, onclick: () => { navigator.clipboard && navigator.clipboard.writeText(r.text); toast('ok', 'copied'); } }, 'copy'),
        h('button', { class: 'btn sm pri', disabled: !r.text || !others.length, onclick: ev => {
          const b = ev.currentTarget.getBoundingClientRect();
          popup([{ cap: 'hand this result to…' }, ...others.map(o => ({ label: o.name, hint: agentTag(o.agent), run: () => { send('handover', { from: n.id, to: o.id }); toast('ok', `handed to ${o.name}`); } }))], b.left, b.bottom + 4);
        } }, 'hand over to…'),
        h('button', { class: 'btn sm', disabled: !r.text, onclick: () => spawnLinked(n.id, 'next') }, 'new agent from this'))),
    steps.length > 1 ? h('div', { class: 'sec' }, h('h3', {}, `steps (${steps.length})`),
      ...steps.map((s, i) => h('div', { style: 'margin-bottom:8px' },
        h('div', { class: 'note', style: 'margin:0 0 3px' }, h('span', { class: 'stepn' }, `${i + 1}. `), (s.ok ? '✓ ' : '✕ ') + (s.prompt || '').split('\n')[0].slice(0, 90)),
        h('div', { class: 'pre ' + (s.ok ? 'ok' : 'bad'), style: 'max-height:120px' }, s.text)))) : null,
    k === 'agent' ? h('div', { class: 'sec' }, h('div', { class: 'row2' },
      h('button', { class: 'btn sm', onclick: () => openFocus(n.id) }, 'open terminal'),
      h('button', { class: 'btn sm', onclick: () => runNode(n.id) }, rt && rt.alive ? 'send prompt again' : 'run'))) : null,
  ]));
  placeFly();
}
function renderOptionsTab(n) {
  const id = n.id, box = $('#fbody'), k = kindOf(n);
  const name = h('input', { class: 'fld', value: n.name, 'aria-label': 'Name', oninput: e => patchNode(id, { name: e.target.value }) });
  const del = h('div', { class: 'sec' }, h('button', { class: 'btn sm danger', onclick: () => deleteNodes([id]) }, 'delete'));
  if (k === 'git') {
    const f = (key, label, ph) => h('div', {}, h('h3', { style: 'margin:0 0 4px;font:600 11.5px var(--display);letter-spacing:.08em;color:var(--i3)' }, label),
      h('input', { class: 'fld', value: n[key] || '', placeholder: ph, oninput: e => patchNode(id, { [key]: e.target.value }) }));
    const msg = h('textarea', { class: 'fld', value: n.message || '', rows: 2, placeholder: '{{agent}}: {{result}}', oninput: e => patchNode(id, { message: e.target.value }) });
    msg.value = n.message || '';
    const op = n.git_op || '';
    const ops = [['commit', 'commit', 'one agent commits its own work; goes on when it has'], ['push', 'push', 'pushes once every commit before it is resolved'],
      ['pr', 'open PR', 'pushes if needed, then an agent opens the pull request']];
    if (op === '') ops.push(['', 'commit & push', 'the older both-in-one node']);
    const extra = h('textarea', { class: 'fld', rows: 3, placeholder: 'e.g. base branch develop, label it agent-made, request review from the team', oninput: e => patchNode(id, { prompt: e.target.value }) });
    extra.value = n.prompt || '';
    box.replaceChildren(
      h('div', { class: 'sec' }, h('h3', {}, 'name'), name),
      h('div', { class: 'sec' }, h('h3', {}, 'this step'),
        seg(ops, op, v => { patchNode(id, { git_op: v }, 0); }),
        h('p', { class: 'note' }, op === 'commit' ? 'A commit takes the work of one agent. Give another agent its own commit and link the commits to one push: the push waits for all of them.'
          : op === 'push' ? 'A push waits until everything linked into it has resolved. A rejected push has an agent bring the branch up to date and tries once more; it never forces.'
            : op === 'pr' ? 'Steps do what the ones before them would have: a PR pushes first when nothing before it did. The agent that did the work writes the title and description.'
              : 'Each arriving result becomes one commit, then one push.')),
      h('div', { class: 'sec' }, h('div', { class: 'grid2' },
        f('branch', op === 'commit' ? 'commit on branch' : op === 'push' ? 'branch to push' : op === 'pr' ? 'branch of the PR' : 'branch', 'current branch'),
        op === 'commit' ? null : f('remote', 'remote', 'origin'))),
      op === 'pr' ? h('div', { class: 'sec' }, h('h3', {}, 'instructions for the pull request'), extra) : null,
      op === 'commit' || op === '' ? h('div', { class: 'sec' }, h('h3', {}, 'commit subject'), msg,
        h('p', { class: 'note' }, 'Placeholders: {{agent}} is the agent whose result arrived, {{result}} its first line. Empty lets the agent write its own message.')) : null,
      op !== '' ? null : h('div', { class: 'sec' }, h('h3', {}, 'when every incoming link has delivered'),
        seg([[true, 'push', 'git push -u <remote> <branch>'], [false, 'keep local', 'commit only']], n.push !== false, v => patchNode(id, { push: v }, 0))),
      h('div', { class: 'sec' }, h('div', { class: 'row2' },
        h('button', { class: 'btn sm ' + (n.draft ? 'pri' : ''), onclick: () => { n.draft ? send('arm', { id }) : patchNode(id, { draft: true }, 0); } }, n.draft ? 'arm' : 'disarm'),
        h('button', { class: 'btn sm', onclick: () => send('fire', { id }) }, op === 'commit' ? 'commit now' : op === 'push' ? 'push now' : op === 'pr' ? 'open PR now' : 'ship now'))),
      del);
    return;
  }
  if (k === 'shot' || k === 'tests') {
    const subject = h('input', { class: 'fld', value: n.subject || '', placeholder: k === 'shot' ? 'e.g. the checkout page at http://localhost:3000/checkout' : 'e.g. the payment retry logic', oninput: e => patchNode(id, { subject: e.target.value }) });
    const cmd = h('input', { class: 'fld', value: n.cmd || '', placeholder: k === 'shot' ? 'empty: the agent takes it' : 'empty: the agent runs the suite', oninput: e => patchNode(id, { cmd: e.target.value }) });
    const retries = h('input', { class: 'fld', style: 'width:70px', type: 'number', min: '0', max: '9', value: String(n.retries ?? 2), oninput: e => patchNode(id, { retries: Math.max(0, parseInt(e.target.value || '0', 10)) }) });
    box.replaceChildren(
      h('div', { class: 'sec' }, h('h3', {}, 'name'), name),
      h('div', { class: 'sec' }, h('h3', {}, k === 'shot' ? 'what to show' : 'what to cover'), subject),
      h('div', { class: 'sec' }, h('h3', {}, k === 'shot' ? 'capture command' : 'test command'), cmd,
        h('p', { class: 'note' }, k === 'shot'
          ? 'Leave it empty and the agent upstream takes the screenshot itself. With a command, weft runs it and expects the image at $WEFT_SHOT.'
          : 'Leave it empty and the agent upstream writes the tests and runs them. With a command, weft runs it, shows the output and sends failures back.')),
      k === 'tests' ? h('div', { class: 'sec' }, h('h3', {}, 'behaviour'),
        seg([[true, 'write tests first', 'ask the agent to add tests before running'], [false, 'just run', 'only run the suite']], n.write_tests !== false, v => patchNode(id, { write_tests: v }, 0)),
        h('div', { class: 'kvr', style: 'margin-top:8px' }, 'send failures back automatically', retries, 'times'),
        h('div', { style: 'height:8px' }),
        seg([[false, 'green passes by itself'], [true, 'always ask me']], !!n.confirm, v => patchNode(id, { confirm: v }, 0))) : null,
      h('p', { class: 'note' }, 'This gate is a barrier: nothing downstream starts until it is solved.'),
      del);
    return;
  }
  if (k === 'note' || k === 'gate') {
    box.replaceChildren(h('div', { class: 'sec' }, h('h3', {}, 'name'), name),
      k === 'gate' ? h('p', { class: 'note' }, 'Results that arrive wait here. Approve passes them on; reject fires only the "if it fails" links.') : h('p', { class: 'note' }, 'Link this note into any agent: its text is added to what the agent receives.'), del);
    return;
  }
  const agents = agentEntries(), inc = incoming(id), acfg = S.cfg.agents[n.agent] || {};
  const cmd = h('input', { class: 'fld', value: n.cmd || '', placeholder: (S.cfg.agents[n.agent] || {}).cmd || 'command to run', 'aria-label': 'Command', oninput: e => patchNode(id, { cmd: e.target.value }) });
  const cwd = h('input', { class: 'fld', value: n.cwd || '', placeholder: 'project folder', title: S.project.dir, 'aria-label': 'Folder', oninput: e => patchNode(id, { cwd: e.target.value }) });
  const wt = h('button', { class: 'chip', 'aria-pressed': String(!!n.worktree), title: 'own git worktree: parallel agents never collide, and commit & push merges each one', onclick: ev => {
    const v = !S.nodes.get(id).worktree; ev.currentTarget.setAttribute('aria-pressed', String(v)); patchNode(id, { worktree: v }, 0); } }, 'own git worktree');
  const adds = S.lib.filter(s => s.kind === 'addendum');
  const addChips = h('div', { class: 'chips' }, adds.length ? adds.map(a => h('button', { class: 'chip', title: a.text, 'aria-pressed': String((n.addenda || []).includes(a.id)), onclick: ev => {
    const cur = new Set(S.nodes.get(id).addenda || []); cur.has(a.id) ? cur.delete(a.id) : cur.add(a.id);
    ev.currentTarget.setAttribute('aria-pressed', String(cur.has(a.id))); patchNode(id, { addenda: [...cur] }, 0); } }, a.title)) : h('span', { class: 'note' }, 'no addenda yet'));
  const quiet = h('input', { class: 'fld', style: 'width:70px', type: 'number', min: '0.5', step: '0.5', value: n.quiet_s || '', placeholder: String(S.cfg.quiet_s), 'aria-label': 'Quiet seconds', oninput: e => patchNode(id, { quiet_s: parseFloat(e.target.value) || 0 }) });
  const quietRow = h('div', { class: 'kvr', style: 'margin-top:8px' }, 'quiet for', quiet, 'seconds');
  quietRow.hidden = n.finish !== 'idle';
  const pre = h('div', { class: 'pre dim', hidden: true });
  const loadPreview = async () => { pre.dataset.open = '1'; pre.hidden = false; const r = await req('preview', { id }); pre.textContent = r.ok ? (r.text || '(empty: the agent just opens and waits)') : r.error; };
  const attach = `tmux -L ${S.project.sock} attach -t ${S.project.session}:${id}`;
  box.replaceChildren(...nn([
    h('div', { class: 'sec' }, h('h3', {}, 'name'), name),
    h('div', { class: 'sec' }, h('h3', {}, 'runs on'),
      seg(agents.map(([kk, a]) => [kk, a.label || kk]), n.agent, v => { convertTo(id, v); }),
      h('div', { style: 'height:8px' }), cmd, h('div', { style: 'height:8px' }), cwd, h('div', { class: 'row2' }, wt)),
    h('div', { class: 'sec' }, h('h3', {}, 'watch it work'),
      ...(n.checks || []).map(c => checkRow(id, c)),
      h('div', { class: 'row2', style: 'flex-wrap:wrap' },
        h('button', { class: 'btn sm', onclick: () => attachCheck(id, 'shot') }, '+ screenshots'),
        h('button', { class: 'btn sm', onclick: () => attachCheck(id, 'tests') }, '+ tests'),
        h('button', { class: 'btn sm', onclick: () => attachCheck(id, 'eta') }, '+ ETA'),
        h('button', { class: 'btn sm', onclick: () => attachCheck(id, 'changes') }, '+ changes'),
        h('button', { class: 'btn sm', onclick: () => attachCheck(id, 'ping') }, '+ ping')),
      h('p', { class: 'note' }, 'A watch never blocks anything. What it collects shows under the card: approve it, or type a note to steer the agent right away.')),
    acfg.yolo_arg ? h('div', { class: 'sec' }, h('h3', {}, 'permissions'),
      seg([[true, 'yolo', 'acts without asking: ' + acfg.yolo_arg], [false, 'asks first', 'asks before it runs a command or edits a file']], yoloOf(n), v => patchNode(id, { yolo: v }, 0)),
      h('p', { class: 'note' }, 'Applied when the agent starts; restart it to switch now.')) : null,
    (acfg.model_arg || acfg.effort_arg) ? h('div', { class: 'sec' }, h('h3', {}, 'model and reasoning'),
      h('div', { class: 'kvr' },
        acfg.model_arg ? h('button', { class: 'pmodel', title: 'model for this agent', onclick: ev => {
          const r = ev.currentTarget.getBoundingClientRect();
          modelMenu(r.left, r.bottom + 4, n.agent, n.model || '', v => { patchNode(id, { model: v }, 0); });
        } }, h('span', { class: 'dl' }, 'model'), h('b', {}, n.model || 'default'), h('span', { class: 'car' }, '▾')) : null),
      acfg.effort_arg && (acfg.efforts || []).length && effortApplies(n.agent, n.model) ? h('div', { style: 'margin-top:8px' },
        seg([['', 'auto'], ...acfg.efforts.map(e => [e, e === 'medium' ? 'med' : e])], n.effort || '', v => patchNode(id, { effort: v }, 0))) : null,
      h('p', { class: 'note' }, 'Applied when the agent starts; restart it to switch now.')) : null,
    h('div', { class: 'sec' }, h('h3', {}, 'addenda appended to every prompt'), addChips,
      h('div', { class: 'row2' }, h('button', { class: 'btn sm', onclick: () => { if (pre.hidden) loadPreview(); else { pre.hidden = true; delete pre.dataset.open; } } }, 'preview what is sent')), pre),
    h('div', { class: 'sec' }, h('h3', {}, 'counts as finished'),
      seg([['idle', 'screen goes quiet', 'works with any TUI'], ['exit', 'process exits', 'right for commands'], ['signal', 'reports done', 'only when the agent runs weft done']],
        n.finish, v => { patchNode(id, { finish: v }, 0); quietRow.hidden = v !== 'idle'; }), quietRow,
      h('p', { class: 'note' }, 'The addendum "report completion" lets an agent say exactly when it is done and hand over a summary.')),
    inc.length ? h('div', { class: 'sec', id: 'inputs' }, h('h3', {}, 'starts when'),
      inc.length > 1 ? seg([['all', 'all inputs arrived', 'Promise.all'], ['any', 'the first arrives', 'Promise.any']], n.join || 'all', v => send('node_set', { id, patch: { join: v } })) : null,
      ...inc.map(e => { const src = S.nodes.get(e.from); const got = (S.rt.get(id) || { arrived: [] }).arrived.includes(e.id);
        const sel = h('select', { 'aria-label': 'Trigger', onchange: ev => send('edge_set', { id: e.id, on: ev.target.value }) },
          [['done', 'finishes'], ['ok', 'succeeds'], ['fail', 'fails']].map(([v, l]) => h('option', { value: v, selected: e.on === v }, l)));
        return h('div', { class: 'inrow' }, h('span', { class: 'tick' + (got ? ' on' : '') }), h('span', { class: 'nm3' }, src ? src.name : '?'), sel); })) : null,
    h('div', { class: 'sec' }, h('h3', {}, 'attach from a terminal'),
      h('div', { class: 'kvr' }, h('span', { class: 'code', title: attach }, attach),
        h('button', { class: 'btn sm', onclick: () => { navigator.clipboard && navigator.clipboard.writeText(attach); toast('ok', 'copied'); } }, 'copy'))),
    del,
  ]));
}
function checkRow(id, c) {
  const set = patch => send('check_set', { node: id, check: c.id, patch });
  const titles = { shot: 'screenshots', tests: 'tests', eta: 'ETA', changes: 'changes', ping: 'ping' };
  const gap = () => h('div', { style: 'height:6px' });
  const parts = [h('div', { class: 'kvr' }, h('b', {}, titles[c.kind] || c.kind), h('span', { style: 'flex:1' }),
    h('button', { class: 'btn sm danger', onclick: () => { send('check_del', { node: id, check: c.id }); } }, 'remove'))];
  if (c.kind === 'shot' || c.kind === 'tests') {
    const subj = h('input', { class: 'fld', value: c.subject || '', placeholder: c.kind === 'shot' ? 'what to show, e.g. the checkout page' : 'what to cover', 'aria-label': 'Subject', onchange: e => set({ subject: e.target.value }) });
    const cmd = h('input', { class: 'fld', value: c.cmd || '', 'aria-label': 'Command', placeholder: c.kind === 'shot' ? 'capture command (empty: the agent takes it); it writes $WEFT_SHOT' : 'test command, e.g. python3 -m unittest -q', onchange: e => set({ cmd: e.target.value }) });
    const every = h('select', { 'aria-label': 'How often', onchange: e => set({ every_s: parseInt(e.target.value, 10) || 0 }) },
      [[0, 'only when I ask'], [30, 'every 30 s'], [60, 'every minute'], [120, 'every 2 min'], [300, 'every 5 min'], [600, 'every 10 min']]
        .map(([v, l]) => h('option', { value: String(v), selected: (c.every_s || 0) === v }, l)));
    parts.push(gap(), subj, gap(), cmd, gap(), h('div', { class: 'kvr' }, 'ask', every, 'while it works'));
  } else if (c.kind === 'changes') {
    const every = h('select', { 'aria-label': 'How often', onchange: e => set({ every_s: parseInt(e.target.value, 10) || 0 }) },
      [[10, 'every 10 s'], [20, 'every 20 s'], [60, 'every minute'], [0, 'only when I ask']].map(([v, l]) => h('option', { value: String(v), selected: (c.every_s || 0) === v }, l)));
    parts.push(gap(), h('div', { class: 'kvr' }, 'look', every, 'while it works'),
      h('p', { class: 'note' }, 'Read from git in its folder (its worktree if it has one). Nobody is asked.'));
  } else if (c.kind === 'eta') {
    parts.push(h('p', { class: 'note' }, 'The agent is asked for an estimate when it starts and updates it at major steps. If the estimate runs out while it is still working, it is asked once more.'));
  } else if (c.kind === 'ping') {
    parts.push(h('p', { class: 'note' }, 'A desktop notification (and the tab title) when it is done, fails or needs you.'));
  }
  return h('div', { style: 'border:1px solid var(--l1);padding:8px;margin-bottom:8px' }, ...parts);
}
function updateFlyLive() {
  if (!S.fly) return;
  if (S.fly.tab === 'result' && !insideFly()) renderResultTab(S.nodes.get(S.fly.id));
  if (S.fly.tab === 'options') {
    const id = S.fly.id, r = S.rt.get(id), t = $('#inputs'); if (!t || !r) return;
    const ticks = t.querySelectorAll('.tick');
    incoming(id).forEach((e, i) => ticks[i] && ticks[i].classList.toggle('on', r.arrived.includes(e.id)));
  }
}
