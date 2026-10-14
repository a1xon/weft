/* ------------------------------------------------------------ launch pad */
const padEl = $('#pad');
const pad = { open: false, ed: null };
let padTa = null, prefT = 0, prefDirty = false;
function savePrefs() {
  prefDirty = true; clearTimeout(prefT);
  prefT = setTimeout(() => { send('pref_set', { prefs: S.prefs }); prefDirty = false; }, 250);
}
const tuned = (a, key) => ((S.prefs[key] || {})[a]) || '';
/* yolo: agents act without asking. On unless the pad (a saved choice), the config or the card says otherwise. */
const yoloDefault = () => (typeof S.prefs.yolo === 'boolean' ? S.prefs.yolo : S.cfg.yolo !== false);
const yoloOf = n => (n && typeof n.yolo === 'boolean' ? n.yolo : S.cfg.yolo !== false);
const manages = a => !!((S.cfg.agents[a] || {}).yolo_arg);
function setTuned(a, key, v) { (S.prefs[key] = S.prefs[key] || {})[a] = v; savePrefs(); }
function addendumOn(sn) { const m = S.prefs.addenda || {}; return sn.id in m ? !!m[sn.id] : !!sn.default_on; }
const addendaIds = () => S.lib.filter(x => x.kind === 'addendum' && addendumOn(x)).map(x => x.id);
const agentList = () => agentEntries().filter(([k]) => k !== 'sh');
function modelOptions(a) {
  const cfg = S.cfg.agents[a] || {}, seen = new Set(), out = [];
  for (const id of cfg.models || []) if (!seen.has(id)) { seen.add(id); out.push({ id, label: id }); }
  for (const m of S.models[a] || []) if (!seen.has(m.id)) { seen.add(m.id); out.push(m); }
  return out;
}
function tuneTag(n) {
  if (kindOf(n) !== 'agent' || n.agent === 'sh') return '';
  const m = n.model ? (n.model.length > 16 ? n.model.slice(0, 15) + '…' : n.model) : '';
  return [m, n.effort, manages(n.agent) && !yoloOf(n) ? 'asks first' : ''].filter(Boolean).map(x => ' · ' + x).join('');
}
function pathParts() {
  const dir = (S.project && S.project.dir) || '', home = (S.project && S.project.home) || '';
  const inHome = home && (dir === home || dir.startsWith(home + '/'));
  const short = inHome ? '~' + dir.slice(home.length) : dir;
  const i = short.lastIndexOf('/');
  let parent = i > 0 ? short.slice(0, i + 1) : '', name = i >= 0 && short.length > 1 ? short.slice(i + 1) : short;
  if (!name) name = short || '/';
  const segs = parent.split('/').filter(Boolean);
  if (segs.length > 4) parent = '…/' + segs.slice(-3).join('/') + '/';
  return { dir, short, parent, name, atHome: !!home && dir === home, atRoot: dir === '/' };
}
/* the line above every prompt input: which folder will an agent work in? */
function renderWhere() {
  const p = pathParts(), g = (S.project && S.project.git) || {};
  $('#where').replaceChildren(...nn([
    h('span', { title: p.dir }, p.parent), h('b', { title: p.dir }, p.name),
    g.repo ? h('span', { class: 'br' }, g.branch) : null,
    g.repo && g.dirty ? h('span', { class: 'dirty' }, `${g.dirty} uncommitted`) : null,
    p.atHome ? h('span', { class: 'dirty' }, 'home folder') : null,
  ]));
}
function openPad() {
  closeFly(); closeMenu();
  pad.open = true; padEl.hidden = false; document.body.classList.add('padopen');
  renderPad();
  send('proj');
  ensureModels(S.lastAgent);
  setTimeout(() => padTa && padTa.focus(), 0);
}
function closePad() {
  pad.open = false; pad.ed = null; padEl.hidden = true; document.body.classList.remove('padopen');
  $('#empty').hidden = S.nodes.size > 0;
  closeMenu();
}
function padEsc() {
  if (menuEl) return closeMenu();
  if (pad.ed) { pad.ed = null; renderPad(); return; }
  closePad();
}
async function ensureModels(a, force) {
  const cfg = S.cfg.agents[a];
  if (!cfg || !cfg.models_cmd || (S.models[a] && !force)) return;
  const r = await req('models', { agent: a, force: !!force });
  if (r && r.models && r.models.length) { S.models[a] = r.models; if (pad.open) renderPad(); }
}
/* one menu for the model, in the pad and in the options flyout */
/* the choices that make an agent, one click each: which one, which model, how hard it thinks and
   whether it asks first. The launch pad and the bar at the bottom are built from the same thing. */
function effortApplies(a, model) {
  const em = (S.cfg.agents[a] || {}).effort_models || [];
  const m = model || '';
  return !em.length || em.some(x => x === '' ? !m : m.includes(x));
}
function agentControls(c) {
  const cfg = S.cfg.agents[c.agent] || {};
  const out = [];
  if (c.agents) {
    const sg = seg(c.agents.map(([k, v]) => [k, k, v.label || k]), c.agent, k => c.onAgent(k));
    sg.classList.add('pagents');
    out.push(sg);
  }
  if (cfg.model_arg) {
    const mid = c.model || '';
    const lab = mid ? ((modelOptions(c.agent).find(o => o.id === mid) || {}).label || mid) : 'default';
    out.push(h('button', { class: 'pmodel', id: c.id + 'model', title: mid ? 'model ' + mid : 'the CLI picks its own default model', onclick: ev => {
      const r = ev.currentTarget.getBoundingClientRect();
      modelMenu(r.left, c.above ? r.top - 4 : r.bottom + 4, c.agent, mid, v => c.onModel(v), !!c.above);
    } }, h('span', { class: 'dl' }, 'model'), h('b', {}, lab), h('span', { class: 'car' }, '▾')));
  }
  if (cfg.effort_arg && (cfg.efforts || []).length && effortApplies(c.agent, c.model)) {
    out.push(h('div', { class: 'peff' }, h('span', { class: 'dl' }, 'reasoning'),
      seg([['', 'auto', 'the CLI decides'], ...cfg.efforts.map(e => [e, e === 'medium' ? 'med' : e, 'reasoning effort ' + e + (c.id === 'p' ? ' (alt+↑/↓)' : '')])], c.effort || '', v => c.onEffort(v))));
  }
  if (cfg.yolo_arg) {
    const on = !!c.yolo;
    out.push(h('button', { class: 'pyolo', id: c.id + 'yolo', 'aria-pressed': String(on),
      title: on ? `yolo: ${c.agent} acts without asking (${cfg.yolo_arg}). Click to make it ask before it acts.` : `${c.agent} asks before it acts. Click for yolo: no permission prompts (${cfg.yolo_arg}).`,
      onclick: () => c.onYolo(!on) }, h('i'), on ? 'yolo' : 'asks first'));
  }
  return out;
}
function modelMenu(x, y, a, cur, onPick, above) {
  const cfg = S.cfg.agents[a] || {};
  const again = () => modelMenu(x, y, a, cur, onPick, above);
  modelMenu.last = { again };
  const pending = cfg.models_cmd && S.models[a] === undefined && !S.modelErr[a];
  const items = [
    { cap: `model for ${cfg.label || a}` + (pending ? ' \u00b7 asking its CLI\u2026' : '') },
    { k: cur ? ' ' : '\u25cf', label: 'default', hint: 'the CLI decides', run: () => onPick('') },
    ...modelOptions(a).map(o => ({ k: cur === o.id ? '\u25cf' : ' ', label: o.label, hint: o.label !== o.id ? o.id : '', run: () => onPick(o.id) })),
    ...(S.modelErr[a] && !modelOptions(a).length ? [{ cap: 'could not list models: ' + S.modelErr[a] }] : []),
    { sep: true },
    { label: 'other\u2026', hint: 'type a model name', run: () => askText(x, above ? y - 90 : y, 'model name for ' + (cfg.label || a), cur || '', t => {
      if (t && !/^[A-Za-z0-9._:\/@+-]{1,80}$/.test(t)) return toast('err', 'model names are plain words: letters, digits and . _ : / @ + -');
      onPick(t);
    }) },
  ];
  popup(items, x, y, { above: !!above }).dataset.kind = 'model';
  ensureModels(a);
}
/* a one-line input where you clicked: enter keeps it, escape or clicking away drops it */
function askText(x, y, label, value, done) {
  closeMenu();
  const inp = h('input', { class: 'fld', value, placeholder: 'enter to keep', spellcheck: 'false', 'aria-label': label });
  const m = h('div', { class: 'menu', style: 'padding:8px;min-width:280px' }, h('div', { class: 'cap', style: 'padding:0 0 5px' }, label), inp);
  document.body.append(m);
  const r = m.getBoundingClientRect();
  m.style.left = Math.max(8, Math.min(x, innerWidth - r.width - 8)) + 'px';
  m.style.top = Math.max(8, Math.min(y, innerHeight - r.height - 8)) + 'px';
  inp.focus(); inp.select();
  let fin = false;
  const end = ok => { if (fin) return; fin = true; const v = inp.value.trim(); m.remove(); if (ok) done(v); if (pad.open && padTa) padTa.focus(); };
  inp.addEventListener('keydown', e => { e.stopPropagation(); if (e.key === 'Enter') end(true); if (e.key === 'Escape') end(false); });
  inp.addEventListener('blur', () => end(false));
}
function padStep(d) {
  const ks = agentList().map(x => x[0]); if (!ks.length) return;
  setAgent(ks[(ks.indexOf(S.lastAgent) + d + ks.length) % ks.length]);
}
function padEffort(d) {
  const a = S.lastAgent, cfg = S.cfg.agents[a] || {}, list = ['', ...(cfg.efforts || [])];
  if (!cfg.effort_arg || !effortApplies(a, tuned(a, 'model'))) return;
  const i = Math.max(0, Math.min(list.length - 1, list.indexOf(tuned(a, 'effort')) + d));
  setTuned(a, 'effort', list[i]); renderPad();
}
function renderPad() {
  if (!pad.open) return;
  if (!padTa) {
    padTa = h('textarea', { rows: 4, spellcheck: 'false', 'aria-label': 'What should the agent do?' });
    const grow = () => { padTa.style.height = 'auto'; padTa.style.height = Math.min(Math.max(padTa.scrollHeight, 128), innerHeight * 0.44) + 'px'; };
    padTa.addEventListener('input', grow);
    padTa.addEventListener('focus', () => $('#pbox') && $('#pbox').classList.add('focus'));
    padTa.addEventListener('blur', () => $('#pbox') && $('#pbox').classList.remove('focus'));
    padTa.addEventListener('keydown', e => {
      if (e.isComposing) return;
      if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); padStart(); }
      else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); padEsc(); }
      else if (e.key === 'Tab') { e.preventDefault(); padStep(e.shiftKey ? -1 : 1); }
      else if (e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) { e.preventDefault(); padEffort(e.key === 'ArrowUp' ? 1 : -1); }
      else if (e.key === '/' && !padTa.value) { e.preventDefault(); padPrompts(padTa); }
    });
    padTa._grow = grow;
  }
  const had = document.activeElement === padTa;
  const sel = had ? [padTa.selectionStart, padTa.selectionEnd] : null;
  const P = pathParts(), g = (S.project && S.project.git) || {};
  const agents = agentList();
  if (agents.length && !agents.some(([k]) => k === S.lastAgent)) S.lastAgent = (S.prefs.agent && S.cfg.agents[S.prefs.agent] && S.prefs.agent !== 'sh') ? S.prefs.agent : agents[0][0];
  const a = S.lastAgent, cfg = S.cfg.agents[a] || {};
  const count = S.nodes.size;

  const git = [];
  if (g.repo) {
    git.push(h('span', { class: 'br' }, h('i', { html: ICON.git }), g.branch));
    git.push(h('span', { class: g.dirty ? 'dirty' : '' }, g.dirty ? `${g.dirty} uncommitted file${g.dirty > 1 ? 's' : ''}` : 'clean'));
    if (g.last) git.push(h('span', { class: 'last', title: g.last }, 'last: ' + g.last));
  } else if (g.repo === false && !g.unknown) git.push(h('span', {}, 'not a git repository'));
  const warns = [];
  if (P.atHome) warns.push(h('div', { class: 'pwarn' }, 'This is your home folder. An agent started here can see everything in it. Quit and start weft inside a project folder if that is not what you want.'));
  else if (P.atRoot) warns.push(h('div', { class: 'pwarn' }, 'This is the filesystem root. Start weft inside a project folder.'));
  else if (g.repo === false && !g.unknown) warns.push(h('div', { class: 'pwarn', style: 'border-color:var(--l3);background:none;color:var(--i2)' }, 'Not a git repository: own worktrees and commit & push nodes need one.'));

  const where = h('div', { class: 'pwhere' },
    h('div', { class: 'pwl' }, h('div', { class: 'ppath', title: P.dir }, P.parent || ' '), h('div', { class: 'pname', title: P.dir }, P.name),
      h('div', { class: 'pgit' }, ...git), ...warns),
    count ? h('button', { class: 'btn dim pcanvas', title: 'Esc', onclick: closePad }, `canvas · ${count} agent${count > 1 ? 's' : ''}`) : null);

  // agent, model, reasoning, yolo: every choice one click, on one line
  const ctl = agentControls({
    id: 'p', agent: a, agents, model: tuned(a, 'model'), effort: tuned(a, 'effort'), yolo: yoloDefault(),
    onAgent: k => setAgent(k),
    onModel: v => { setTuned(a, 'model', v); renderPad(); },
    onEffort: v => setTuned(a, 'effort', v),
    onYolo: v => { S.prefs.yolo = v; savePrefs(); renderPad(); },
  });
  const go = h('button', { class: 'btn pri', onclick: padStart }, 'start');
  const box = h('div', { id: 'pbox', class: 'chamf' }, padTa,
    h('div', { class: 'pfoot' }, h('span', {}, h('kbd', {}, 'enter'), ' start  ', h('kbd', {}, 'shift+enter'), ' new line  ', h('kbd', {}, 'tab'), ' next agent  ', h('kbd', {}, '/'), ' prompts'),
      h('span', { class: 'sp' }), go));
  padTa.placeholder = `Tell ${a} what to do in ${P.name}…`;

  // standard addenda: click to switch on, right-click to curate
  let chipsRow;
  const edFocus = pad.ed && pad.ed.el && pad.ed.el.contains(document.activeElement) ? document.activeElement : null;
  if (pad.ed) chipsRow = pad.ed.el || addendumEditor(pad.ed);
  else {
    const adds = S.lib.filter(x => x.kind === 'addendum');
    chipsRow = h('div', { class: 'pchips' }, h('span', { class: 'dl' }, 'addenda'),
      ...adds.map(x => {
        const b = h('button', { class: 'chip', 'aria-pressed': String(addendumOn(x)), title: (x.text || '').slice(0, 400) + '\n\n(right-click: edit or remove)' }, h('i', { class: 'dot' }), x.title);
        b.addEventListener('click', () => { const m = S.prefs.addenda = S.prefs.addenda || {}; m[x.id] = !addendumOn(x); savePrefs(); renderPad(); });
        b.addEventListener('contextmenu', ev => { ev.preventDefault(); addendumMenu(x, ev.clientX, ev.clientY); });
        return b;
      }),
      h('button', { class: 'chip add', title: 'Write a new standard addendum', onclick: () => { pad.ed = { id: null, title: '', text: '' }; renderPad(); } }, '+ add'),
      h('button', { class: 'chip add', title: 'Standard prompts (or type / in the prompt)', onclick: ev => padPrompts(ev.currentTarget) }, 'prompts ▾'));
  }

  const top = padEl.scrollTop;
  padEl.replaceChildren(h('div', { class: 'pw' }, where, h('div', { class: 'pctl' }, ...ctl), box, chipsRow));
  padEl.scrollTop = top;
  if (padTa._grow) padTa._grow();
  if (edFocus) edFocus.focus();
  else if (pad.ed && pad.ed.fresh) { pad.ed.fresh = false; const f = pad.ed.el.querySelector('input'); if (f) f.focus(); }
  else if (had || !pad.ed) { padTa.focus(); if (sel) try { padTa.setSelectionRange(sel[0], sel[1]); } catch {} }
}
padEl.addEventListener('pointerdown', ev => { if (ev.target === padEl && S.nodes.size) closePad(); });
padEl.addEventListener('click', ev => {
  if (pad.ed || !padTa || !ev.target.closest) return;
  if (ev.target.closest('button')) setTimeout(() => { if (pad.open && !pad.ed && !menuEl) padTa.focus(); }, 0);
});
function padPrompts(anchor) {
  const r = anchor.getBoundingClientRect();
  const ps = S.lib.filter(x => x.kind === 'prompt');
  popup([{ cap: 'standard prompts' }, ...(ps.length ? ps.map(x => ({ label: x.title, hint: 'insert', run: () => {
    padTa.value = x.text + (padTa.value ? '\n' + padTa.value : ''); padTa._grow(); padTa.focus(); padTa.setSelectionRange(padTa.value.length, padTa.value.length);
  } })) : [{ cap: 'none yet: add some in the library' }])], r.left, r.bottom + 4);
}
function addendumMenu(x, mx, my) {
  popup([
    { label: 'edit…', run: () => { pad.ed = { id: x.id, title: x.title, text: x.text }; renderPad(); } },
    { k: x.default_on ? '●' : ' ', label: 'on for new agents by default', run: () => { x.default_on = !x.default_on; delete (S.prefs.addenda || {})[x.id]; savePrefs(); saveLib(); renderPad(); } },
    { sep: true },
    { label: 'remove', danger: true, run: () => { S.lib = S.lib.filter(y => y.id !== x.id); saveLib(); renderPad(); } },
  ], mx, my);
}
function addendumEditor(ed) {
  const name = h('input', { class: 'fld', value: ed.title, placeholder: 'short name, e.g. Run the tests', 'aria-label': 'Addendum name', spellcheck: 'false' });
  const text = h('textarea', { class: 'fld', placeholder: 'what the agent should also do or keep in mind: this is appended to every prompt', 'aria-label': 'Addendum text', spellcheck: 'false' });
  text.value = ed.text;
  const save = () => {
    const t = name.value.trim(), body = text.value.trim();
    if (!t || !body) { toast('warn', 'give it a name and a text'); return; }
    if (ed.id) { const x = S.lib.find(y => y.id === ed.id); if (x) { x.title = t; x.text = body; } }
    else {
      const id = 'add-' + Math.random().toString(36).slice(2, 7);
      S.lib.push({ id, kind: 'addendum', title: t, text: body, default_on: false });
      (S.prefs.addenda = S.prefs.addenda || {})[id] = true; savePrefs();
    }
    saveLib(); pad.ed = null; renderPad();
  };
  const key = e => {
    e.stopPropagation();
    if (e.key === 'Escape') { pad.ed = null; renderPad(); }
    else if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); save(); }
    else if (e.key === 'Enter' && e.target === name) { e.preventDefault(); text.focus(); }
  };
  name.addEventListener('keydown', key); text.addEventListener('keydown', key);
  ed.fresh = true; // renderPad puts the keyboard in the name field as soon as the editor is on screen
  return ed.el = h('div', { class: 'aed' }, name, text,
    h('div', { class: 'row2' }, h('button', { class: 'btn sm pri', onclick: save }, ed.id ? 'save' : 'add'),
      h('button', { class: 'btn sm', onclick: () => { pad.ed = null; renderPad(); } }, 'cancel'),
      h('span', { class: 'note', style: 'margin:0 0 0 auto' }, 'ctrl+enter saves')));
}
async function padStart() {
  const text = padTa.value.trim();
  if (!text) { const b = $('#pbox'); if (b) { b.classList.remove('nope'); void b.offsetWidth; b.classList.add('nope'); } padTa.focus(); return; }
  const a = S.lastAgent, cfg = S.cfg.agents[a] || {};
  const c = toWorld(innerWidth / 2, innerHeight / 2);
  const extra = { model: cfg.model_arg ? tuned(a, 'model') : '', effort: cfg.effort_arg ? tuned(a, 'effort') : '', addenda: addendaIds(), yolo: cfg.yolo_arg ? yoloDefault() : undefined };
  padTa.value = '';
  closePad();
  savePrefs();
  const id = await spawnAt(a, c.x - NW / 2, c.y - NH / 2, { text, draft: false, run: true, edit: false, extra });
  if (id) setTimeout(() => reveal(id), 60);
}
