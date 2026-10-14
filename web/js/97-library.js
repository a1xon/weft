/* ------------------------------------------------------------ prompt library */
const modal = $('#modal');
let libSel = null, libT = 0;
function openLibrary() { modal.hidden = false; libSel = libSel || (S.lib[0] && S.lib[0].id); renderLibrary(); }
function saveLib() { clearTimeout(libT); libT = setTimeout(() => send('lib_set', { library: S.lib }), 350); }
function onLibChanged() { if (!modal.hidden) renderLibrary(); if (S.fly && S.fly.tab === 'options' && !insideFly()) renderFly(); }
function renderLibrary() {
  const cur = S.lib.find(s => s.id === libSel) || S.lib[0];
  if (cur) libSel = cur.id;
  const group = (kind, title) => [
    h('div', { class: 'grp' }, title, h('button', { class: 'btn sm dim', onclick: () => {
      const s = { id: kind + '-' + Math.random().toString(36).slice(2, 7), kind, title: 'new ' + (kind === 'prompt' ? 'prompt' : 'addendum'), text: '', default_on: false };
      S.lib.push(s); libSel = s.id; saveLib(); renderLibrary(); } }, '+ new')),
    ...S.lib.filter(s => s.kind === kind).map(s => h('button', { class: 'li' + (s.id === libSel ? ' on' : ''), onclick: () => { libSel = s.id; renderLibrary(); } }, s.title)),
  ];
  const editor = cur ? [
    h('input', { class: 'fld', value: cur.title, 'aria-label': 'Title', oninput: e => { cur.title = e.target.value; saveLib(); } }),
    (() => { const t = h('textarea', { class: 'fld', spellcheck: 'false', 'aria-label': 'Text', oninput: e => { cur.text = e.target.value; saveLib(); } }); t.value = cur.text; return t; })(),
    cur.kind === 'addendum'
      ? h('label', { class: 'chk' }, h('input', { type: 'checkbox', checked: cur.default_on, onchange: e => { cur.default_on = e.target.checked; saveLib(); } }), 'switch on for new agents')
      : h('span', { class: 'note' }, 'Standard prompts are copied into an agent when you insert them (type / in a prompt), so you can edit them there.'),
    cur.kind === 'addendum' ? h('span', { class: 'note' }, 'Addenda are referenced, not copied: editing one here changes every agent that uses it. Variables: {{weft}}, {{node}}, {{cwd}}.') : null,
    h('div', {}, h('button', { class: 'btn sm danger', onclick: () => {
      S.lib = S.lib.filter(s => s.id !== cur.id);
      for (const n of S.nodes.values()) if ((n.addenda || []).includes(cur.id)) send('node_set', { id: n.id, patch: { addenda: n.addenda.filter(a => a !== cur.id) } });
      libSel = null; saveLib(); renderLibrary(); } }, 'delete')),
  ] : [h('p', { class: 'note' }, 'create a standard prompt or addendum with + new')];
  const ta = modal.querySelector('textarea'), keep = document.activeElement === ta;
  modal.replaceChildren(h('div', { class: 'win chamf' },
    h('div', { class: 'mh' }, h('h2', {}, 'prompt library'), h('button', { class: 'ib', 'aria-label': 'Close', html: ICON.x, onclick: () => { modal.hidden = true; } })),
    h('div', { class: 'mb' },
      h('div', { class: 'ml' }, group('prompt', 'standard prompts'), group('addendum', 'addenda')),
      h('div', { class: 'me' }, ...nn(editor)))));
  if (keep) { const t = modal.querySelector('textarea'); if (t) t.focus(); }
  modal.onpointerdown = e => { if (e.target === modal) modal.hidden = true; };
}

/* ------------------------------------------------------------ starter flow */
async function starterFlow() {
  const P = id => (S.lib.find(s => s.id === id) || { text: '' }).text;
  const keys = Object.keys(S.cfg.agents);
  const pick = (k, fb) => keys.includes(k) ? k : fb;
  const A = pick('claude', keys[0]), B = pick('codex', A), C = pick('agy', A);
  const mk = (agent, x, y, node) => req('node_add', { node: { agent, x, y, ...node } }).then(r => r.id);
  const brief = await mk('note', 0, 330, { prompt: 'House rules: keep the public API stable, prefer small functions, no new dependencies.', name: 'house rules' });
  const plan = await mk(A, 0, 40, { prompt: P('plan') + '<describe your task here>', name: 'plan' });
  const impl = await mk(B, 500, 0, { prompt: P('implement') + '{{inputs}}', name: 'implement' });
  const docs = await mk(C, 500, 290, { prompt: 'Write or update the documentation for the change described below.\n\n{{inputs}}', name: 'document' });
  const gate = await mk('gate', 1000, 140, { name: 'approve' });
  const ship = await mk('git', 1500, 140, { name: 'commit & push' });
  for (const [f, t] of [[plan, impl], [plan, docs], [brief, impl], [impl, gate], [docs, gate], [gate, ship]]) send('edge_add', { from: f, to: t, on: t === ship ? 'ok' : 'done' });
  setTimeout(() => { select([plan]); fit(); }, 160);
  toast('ok', 'starter flow loaded. Describe your task in "plan", then run flow. The push stays disarmed until you arm it.', { ms: 10000 });
}
