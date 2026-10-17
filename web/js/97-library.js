/* ------------------------------------------------------------ the library: networks, prompts and addenda */
const modal = $('#modal');
let libSel = null, libT = 0, libTab = 'nets', netSel = null;
function openLibrary(tab) {
  if (typeof tab === 'string') libTab = tab;
  modal.hidden = false; libSel = libSel || (S.lib[0] && S.lib[0].id); renderLibrary();
}
function saveLib() { clearTimeout(libT); libT = setTimeout(() => send('lib_set', { library: S.lib }), 350); }
function onLibChanged() { if (!modal.hidden) renderLibrary(); if (S.fly && S.fly.tab === 'options' && !insideFly()) renderFly(); }
function onNetsChanged() { renderNetChips(); if (!modal.hidden && libTab === 'nets' && !modal.contains(document.activeElement)) renderLibrary(); }
function renderLibrary() {
  const ta = modal.querySelector('.me textarea'), keep = document.activeElement === ta;
  const tabs = h('div', { class: 'seg tabs', role: 'tablist' }, [['nets', 'networks'], ['text', 'prompts & addenda']].map(([k, label]) =>
    h('button', { role: 'tab', 'aria-pressed': String(libTab === k), onclick: () => { libTab = k; renderLibrary(); } }, label)));
  modal.replaceChildren(h('div', { class: 'win chamf' },
    h('div', { class: 'mh' }, h('h2', {}, 'library'), tabs, h('span', { class: 'sp' }), h('button', { class: 'ib', 'aria-label': 'Close', html: ICON.x, onclick: () => { modal.hidden = true; } })),
    libTab === 'nets' ? netsPane() : textPane()));
  if (keep) { const t = modal.querySelector('.me textarea'); if (t) t.focus(); }
  modal.onpointerdown = e => { if (e.target === modal) modal.hidden = true; };
}
/* networks: a list (drag one out onto the canvas) and what the chosen one does */
function netsPane() {
  const all = allNets(), top = new Set(topNets().map(n => n.id));
  const cur = all.find(n => n.id === netSel) || all[0];
  if (cur) netSel = cur.id;
  const item = net => {
    const b = h('button', { class: 'li net' + (net.id === netSel ? ' on' : ''), title: 'click: what it does · drag: onto the canvas' },
      netGlyph(net, 38, 16),
      h('span', { class: 'lt' }, h('b', {}, net.short), net.short !== net.title ? h('small', {}, net.title) : null),
      top.has(net.id) ? h('span', { class: 'tb', title: 'in the top bar: one of the two you use most' }, 'top') : null,
      netUses(net.id) ? h('span', { class: 'uses', title: 'how often you used it' }, netUses(net.id) + '×') : null);
    b.addEventListener('pointerdown', e => dragNet(e, net, { lift: () => { modal.hidden = true; }, click: () => { netSel = net.id; renderLibrary(); } }));
    return b;
  };
  const mine = all.filter(n => n.own), sel = [...S.sel];
  const save = sel.length && Array.isArray(S.nets) ? h('button', { class: 'btn sm dim', title: 'the selected cards and their links, as a network',
    onclick: ev => { const r = ev.currentTarget.getBoundingClientRect(); saveNetFrom(sel, r.left, r.bottom + 4); } }, `+ save the ${sel.length} selected`) : null;
  const left = h('div', { class: 'ml' },
    ...[...new Set(all.filter(n => !n.own).map(n => n.group || 'built in'))].flatMap(g =>
      [h('div', { class: 'grp' }, g), ...all.filter(n => !n.own && (n.group || 'built in') === g).map(item)]),
    h('div', { class: 'grp' }, 'yours', save),
    ...(mine.length ? mine.map(item) : [h('p', { class: 'note pad' }, Array.isArray(S.nets)
      ? 'Select cards on the canvas, right-click, "save as network". The first agent’s prompt becomes the task you type when you drop it.'
      : 'Saving your own needs a newer weft (build and install it, run weft once).')]));
  return h('div', { class: 'mb nets' }, left, h('div', { class: 'me nets' }, ...(cur ? netDetail(cur) : [])));
}
function netDetail(net) {
  const roles = netRoles(net), entries = netEntries(net);
  const order = [...net.nodes].sort((a, b) => a.x - b.x || a.y - b.y);
  const head = net.own
    ? [h('input', { class: 'fld title', value: net.title, 'aria-label': 'Name', oninput: e => editNet(net.id, { title: e.target.value }) }),
      h('textarea', { class: 'fld about', rows: 2, spellcheck: 'false', 'aria-label': 'What it is for', placeholder: 'what it is for', oninput: e => editNet(net.id, { about: e.target.value }) }, net.about)]
    : [h('h3', {}, net.title), h('p', { class: 'about' }, net.about)];
  return [
    ...head,
    h('div', { class: 'dia' }, netDiagram(net, roles)),
    h('ol', { class: 'steps' }, ...order.map(t => h('li', { class: netKind(t) }, h('b', {}, netLabel(t, roles)), h('span', {}, netDoes(t) + (entries.includes(t.key) ? ' · starts it' : ''))))),
    h('p', { class: 'note' }, 'Agents are roles: the first is the agent you work with, a second opinion is the next one (you pick them when you drop it). Dropped on an agent on the canvas, the network carries on from that agent’s work instead.'),
    h('div', { class: 'acts' },
      h('button', { class: 'btn pri', onclick: () => { modal.hidden = true; dropNet(net, null); } }, 'place it'),
      h('span', { class: 'note' }, 'or drag it from the list onto the canvas'),
      h('span', { class: 'sp' }),
      net.own ? h('button', { class: 'btn sm danger', onclick: () => {
        setNets(S.nets.filter(n => n.id !== net.id)); netSel = null; renderLibrary();
        toast('info', `removed "${net.title}"`);
      } }, 'delete') : null),
  ];
}
function editNet(id, patch) {
  const i = (S.nets || []).findIndex(n => n.id === id); if (i < 0) return;
  S.nets = S.nets.map((n, j) => (j === i ? { ...n, ...patch } : n));
  clearTimeout(editNet.t);
  editNet.t = setTimeout(() => { send('net_set', { networks: S.nets }); renderNetChips(); }, 400);
  const li = modal.querySelector('.li.net.on .lt b'); if (li && patch.title != null) li.textContent = patch.title;
}
/* standard prompts (copied in) and addenda (referenced) */
function textPane() {
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
  return h('div', { class: 'mb' },
    h('div', { class: 'ml' }, group('prompt', 'standard prompts'), group('addendum', 'addenda')),
    h('div', { class: 'me' }, ...nn(editor)));
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
  // columns follow the card width (NW 600): the same spacing as the 500 px cards had
  const impl = await mk(B, 600, 0, { prompt: P('implement') + '{{inputs}}', name: 'implement' });
  const docs = await mk(C, 600, 290, { prompt: 'Write or update the documentation for the change described below.\n\n{{inputs}}', name: 'document' });
  const gate = await mk('gate', 1200, 140, { name: 'approve' });
  const ship = await mk('git', 1800, 140, { name: 'commit & push' });
  for (const [f, t] of [[plan, impl], [plan, docs], [brief, impl], [impl, gate], [docs, gate], [gate, ship]]) send('edge_add', { from: f, to: t, on: t === ship ? 'ok' : 'done' });
  setTimeout(() => { select([plan]); fit(); }, 160);
  toast('ok', 'starter flow loaded. Describe your task in "plan", then run flow.', { ms: 10000 });
}
