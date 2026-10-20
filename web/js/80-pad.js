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
function pathParts(dir = (S.project && S.project.dir) || '') {
  const home = (S.project && S.project.home) || '';
  const short = shortDir(dir);
  const i = short.lastIndexOf('/');
  let parent = i > 0 ? short.slice(0, i + 1) : '', name = i >= 0 && short.length > 1 ? short.slice(i + 1) : short;
  if (!name) name = short || '/';
  const segs = parent.split('/').filter(Boolean);
  if (segs.length > 4) parent = '…/' + segs.slice(-3).join('/') + '/';
  return { dir, short, parent, name, atHome: !!home && dir === home, atRoot: dir === '/' };
}
/* the line above every prompt input: which folder will an agent work in? The selected agents' (a
   click moves them), else the one new agents start in (a click picks another) */
function renderWhere() {
  const w = $('#where');
  if (!w) return;
  const ids = folderTargets();
  let d, several = 0, can = true;
  if (ids.length) {
    const ds = [...new Set(ids.map(i => workDir(S.nodes.get(i))))];
    d = ds[0]; several = ds.length > 1 ? ds.length : 0;
  } else if (S.sel.size) { d = workDir(hostOf(S.nodes.get([...S.sel][0]))); can = false; }
  else d = newDir();
  const p = pathParts(d), g = gitOf(d), away = d !== projDir();
  treeFollow(p.dir, g);   // (the git tree in the corner shows this folder too)
  const key = JSON.stringify([d, several, can, ids.length, g, p.short]);
  if (w._key === key) return;
  w._key = key;
  const tip = !can ? `${p.dir}: it works where the agent before it does`
    : ids.length ? `${several ? several + ' folders' : p.dir}\nclick or ctrl+o: move ${ids.length > 1 ? 'these agents' : 'this agent'} to another folder`
    : `${p.dir}\nclick or ctrl+o: start new agents in another folder`;
  const name = can
    ? h('button', { class: 'wname' + (away ? ' away' : ''), title: tip, 'aria-label': 'Folder: ' + p.dir, onclick: ev => {
        const r = ev.currentTarget.getBoundingClientRect(); pickFolderAt(r.left - 8, r.top - 2, true); } },
        several ? `${several} folders` : p.name)
    : h('b', { title: tip }, p.name);
  if (away && !several) name.style.setProperty('--fh', folderHue(d));
  w.replaceChildren(...nn([
    several ? null : h('span', { title: p.dir }, p.parent), name,
    !several && g.repo ? h('span', { class: 'br' }, g.branch) : null,
    !several && g.repo && g.dirty ? h('span', { class: 'dirty' }, `${g.dirty} uncommitted`) : null,
    !several && g.missing ? h('span', { class: 'dirty' }, 'folder not found') : null,
    !several && p.atHome ? h('span', { class: 'dirty' }, 'home folder') : null,
  ]));
}
// coming back from an editor or a terminal: the folder has likely changed
addEventListener('focus', () => send('proj'));
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
  renderOmni();   // what was chosen on the pad is what the bar shows now
}
function padEsc() {
  if (menuEl) return closeMenu();
  if (pad.ed) { pad.ed = null; renderPad(); return; }
  closePad();
}
const modelsAsked = new Set();
async function ensureModels(a, force) {
  const cfg = S.cfg.agents[a];
  if (!cfg || !cfg.models_cmd || (S.models[a] && !force) || modelsAsked.has(a)) return;
  modelsAsked.add(a);
  const r = await req('models', { agent: a, force: !!force });
  modelsAsked.delete(a);
  if (r && r.models && r.models.length) { S.models[a] = r.models; redrawPrompts(); }
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
  // left: which agent (claude | agy | codex), centred against the two lines on the right;
  // right: the model on the first line, the reasoning level (and yolo) on the second
  const left = [], right = [], second = [];
  if (c.lead) left.push(c.lead);
  if (c.agents) {
    const sg = seg(c.agents.map(([k, v]) => [k, k, v.label || k]), c.agent, k => c.onAgent(k));
    sg.classList.add('pagents');
    left.push(sg);
  }
  // (a queued prompt has a reasoning level only: `levels` are its choices, the rest is its agent's)
  if (cfg.model_arg && c.onModel) right.push(modelRow(c, cfg));
  const levels = c.levels || (cfg.effort_arg && (cfg.efforts || []).length && effortApplies(c.agent, c.model)
    ? [['', 'auto', 'the CLI decides'], ...cfg.efforts.map(e => [e, e === 'medium' ? 'med' : e, 'reasoning effort ' + e + ' (alt+↑/↓)'])] : null);
  if (levels) {
    const eg = seg(levels, c.effort || '', v => c.onEffort(v));
    eg.classList.add('cells');
    second.push(h('div', { class: 'peff' }, h('span', { class: 'dl' }, 'reasoning'), eg));
  }
  if (cfg.yolo_arg && c.onYolo) {
    const on = !!c.yolo;
    second.push(h('button', { class: 'pyolo', id: c.id + 'yolo', 'aria-pressed': String(on),
      title: on ? `yolo: ${c.agent} acts without asking (${cfg.yolo_arg}). Click to make it ask before it acts.` : `${c.agent} asks before it acts. Click for yolo: no permission prompts (${cfg.yolo_arg}).`,
      onclick: () => c.onYolo(!on) }, h('i'), on ? 'yolo' : 'asks first'));
  }
  if (second.length) right.push(h('div', { class: 'pline' }, ...second));
  if (!left.length && !right.length) return [];
  return [h('div', { class: 'pgrid' }, left.length ? h('div', { class: 'pleft' }, ...left) : null, right.length ? h('div', { class: 'pright' }, ...right) : null)];
}
function modelRow(c, cfg) {
  const a = c.agent, mid = c.model || '', opts = modelOptions(a);
  const pending = cfg.models_cmd && S.models[a] === undefined && !S.modelErr[a];
  if (cfg.models_cmd && S.models[a] === undefined) ensureModels(a);
  const items = [['', 'default', 'the CLI picks its own default model'],
    ...opts.map(o => [o.id, o.label, o.label !== o.id ? o.id : 'model ' + o.id])];
  if (mid && !opts.some(o => o.id === mid)) items.push([mid, mid, 'model ' + mid + ' (typed by hand)']);
  const sg = seg(items, mid, v => c.onModel(v));
  sg.classList.add('cells');
  sg.append(h('button', { class: 'other', title: 'type a model name', onclick: ev => {
    const r = ev.currentTarget.getBoundingClientRect();
    askModel(r.left, c.above ? r.top - 90 : r.bottom + 4, a, mid, v => c.onModel(v));
  } }, 'other…'));
  return h('div', { class: 'pmodels', id: c.id + 'model' }, h('span', { class: 'dl' }, 'model'), sg,
    pending ? h('span', { class: 'dl', title: 'asking its CLI for its models' }, '…') : null,
    S.modelErr[a] && !opts.length ? h('span', { class: 'dl', title: S.modelErr[a] }, 'could not list models') : null);
}
/* a model name typed by hand: where you clicked, enter keeps it */
function askModel(x, y, a, cur, onPick) {
  const cfg = S.cfg.agents[a] || {};
  askText(x, y, 'model name for ' + (cfg.label || a), cur || '', t => {
    if (t && !/^[A-Za-z0-9._:\/@+-]{1,80}$/.test(t)) return toast('err', 'model names are plain words: letters, digits and . _ : / @ + -');
    onPick(t);
  });
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
    { label: 'other\u2026', hint: 'type a model name', run: () => askModel(x, above ? y - 90 : y, a, cur, onPick) },
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
/* one step along the reasoning levels of an agent (alt+up / alt+down), or null where it has none */
function effortStep(a, model, cur, d) {
  const cfg = S.cfg.agents[a] || {}, list = ['', ...(cfg.efforts || [])];
  if (!cfg.effort_arg || !effortApplies(a, model)) return null;
  return list[Math.max(0, Math.min(list.length - 1, list.indexOf(cur || '') + d))];
}
/* the reasoning levels a queued prompt can give its agent: from the moment the agent takes it up, the agent
   thinks at that level (it is started again on its conversation if it runs at another); '' leaves it as it is.
   Null where its agent has none. */
function stepLevels(n) {
  const host = hostOf(n), cfg = S.cfg.agents[host.agent] || {};
  if (host === n || !cfg.effort_arg || !(cfg.efforts || []).length || !effortApplies(host.agent, host.model)) return null;
  return [['', 'as is', `${host.name} keeps its reasoning (${host.effort || 'auto'})`],
    ...cfg.efforts.map(e => [e, e === 'medium' ? 'med' : e, `${host.name} thinks at ${e} from this prompt on (alt+↑/↓)`])];
}
/* alt+up / alt+down on a card: its reasoning level, a queued prompt's own one included */
function cardEffort(id, d) {
  const n = S.nodes.get(id), who = hostOf(n);
  if (!n || kindOf(who) !== 'agent' || who.agent === 'sh') return;
  const v = effortStep(who.agent, who.model || '', n.effort || '', d);
  if (v !== null && v !== (n.effort || '')) patchNode(id, { effort: v }, 0);
}
function padEffort(d) {
  const a = S.lastAgent, v = effortStep(a, tuned(a, 'model'), tuned(a, 'effort'), d);
  if (v === null) return;
  setTuned(a, 'effort', v); redrawPrompts();
}
/* the prompt box is one component: the launch pad shows it big, the bar at the bottom compact. The
   text, the keys that work in it (listed in its footer) and a button that does what Enter does. */
function keyHints(rows) { return rows.map(([k, w]) => h('span', { class: 'kh' }, h('kbd', {}, k), ' ' + w)); }
/* the prompt can be expanded to edit several lines in; the choice is remembered in this browser */
let promptBig = (() => { try { return localStorage.getItem('weft.promptBig') === '1'; } catch { return false; } })();
function togglePromptBig() {
  promptBig = !promptBig;
  try { localStorage.setItem('weft.promptBig', promptBig ? '1' : '0'); } catch {}
  redrawPrompts();
}
const ICON_GROW = I('<path d="M9.5 2.5h4v4M13.5 2.5L9 7M6.5 13.5h-4v-4M2.5 13.5L7 9"/>');
const ICON_SHRINK = I('<path d="M9.5 2.5v4h4M9.5 6.5L14 2M6.5 13.5v-4h-4M6.5 9.5L2 14"/>');
function promptBox(o) {
  const ta = o.ta;
  if (!ta._boxed) {
    ta._boxed = true;
    ta.addEventListener('focus', () => { const b = ta.closest('.pbox'); if (b) b.classList.add('focus'); });
    ta.addEventListener('blur', () => { const b = ta.closest('.pbox'); if (b) b.classList.remove('focus'); });
  }
  const tip = promptBig ? 'back to a short prompt' : 'expand the prompt: more room to edit several lines';
  const cls = 'pbox chamf' + (o.compact ? ' compact' : '') + (promptBig ? ' big' : '') + (ta === document.activeElement ? ' focus' : '');
  const foot = h('div', { class: 'pfoot' }, h('span', { class: 'khs' }, ...o.hint), h('span', { class: 'sp' }),
    h('button', { class: 'xp', title: tip, 'aria-label': tip, 'aria-pressed': String(promptBig), onclick: togglePromptBig, html: promptBig ? ICON_SHRINK : ICON_GROW }),
    h('button', { class: 'btn pri', onclick: o.go }, o.label));
  // the box the prompt already sits in is kept: a textarea that is moved loses what ctrl+z would undo
  const box = ta.parentElement;
  if (box && box.id === o.id) { box.className = cls; box.lastElementChild.replaceWith(foot); return box; }
  return h('div', { id: o.id, class: cls }, ta, foot);
}
/* el's children become kids, in order, without taking out one that is already there (see promptBox) */
function keepIn(el, kids) {
  for (const c of [...el.childNodes]) if (!kids.includes(c)) c.remove();
  let at = el.firstChild;
  for (const k of kids) { if (k === at) at = at.nextSibling; else el.insertBefore(k, at); }
}
/* what is typed into the bar or the pad is kept in this browser, per board: a reload (a new build,
   a board switch) puts it back. It is saved as it is typed and when the page goes. */
const draftKey = ta => S.project && `weft-draft:${ta._draft}:${S.project.dir}`;
function keepDraft(ta, name) {
  ta._draft = name;
  ta.addEventListener('input', () => { saveDraft(ta); if (!ta._setting) ta._hi = null; });   // (typed: no longer going through the sent ones)
  addEventListener('pagehide', () => saveDraft(ta));
  loadDraft(ta);
}
function saveDraft(ta) {
  const k = draftKey(ta); if (!k) return;
  try { if (ta.value.trim()) localStorage.setItem(k, ta.value); else localStorage.removeItem(k); } catch {}
}
function loadDraft(ta) {
  const k = draftKey(ta); if (!k || ta.value) return;
  try { const v = localStorage.getItem(k); if (v) { ta.value = v; if (ta._grow) ta._grow(); } } catch {}
}
/* the whole text of a prompt replaced as an edit of its own, so ctrl+z brings back what was there
   (a sent prompt, one cleared with Esc, the one before a recalled one). Setting .value would not. */
function setPrompt(ta, text) {
  ta._setting = true;
  try {
    if (document.activeElement === ta) { ta.select(); if (document.execCommand(text ? 'insertText' : 'delete', false, text)) return; }
    ta.value = text; ta.dispatchEvent(new Event('input', { bubbles: true }));
  } finally { ta._setting = false; }
}
/* the prompts sent from the bar and the pad, per board, newest last: ↑ on the first line brings back
   the one before, ↓ on the last line goes forward again and, past the newest, to what was typed */
const SENT_MAX = 100;
const sentKey = () => S.project && 'weft-sent:' + S.project.dir;
function sentList() { try { return JSON.parse(localStorage.getItem(sentKey()) || '[]'); } catch { return []; } }
function rememberSent(text) {
  const k = sentKey(); if (!k) return;
  const l = sentList().filter(t => t !== text); l.push(text);
  try { localStorage.setItem(k, JSON.stringify(l.slice(-SENT_MAX))); } catch {}
}
function recall(ta, d) {
  const v = ta.value, a = ta.selectionStart;
  if (a !== ta.selectionEnd || (d < 0 ? v.slice(0, a) : v.slice(a)).includes('\n')) return false;
  const l = sentList();
  if (ta._hi == null) { if (d > 0) return false; ta._hi = l.length; ta._stash = v; }
  const i = ta._hi + d;
  if (i < 0 || i > l.length) return false;
  const hi = i, stash = ta._stash;
  setPrompt(ta, i === l.length ? stash : l[i]);
  ta._hi = hi; ta._stash = stash;
  const at = d < 0 ? 0 : ta.value.length; ta.setSelectionRange(at, at);
  return true;
}
const plainArrow = e => (e.key === 'ArrowUp' || e.key === 'ArrowDown') && !e.altKey && !e.ctrlKey && !e.metaKey && !e.shiftKey && !menuEl;
function shake(box) { if (box) { box.classList.remove('nope'); void box.offsetWidth; box.classList.add('nope'); } }
/* the pad and the bar show the same choices: a change made in one is drawn in both */
function redrawPrompts() { renderPad(); omni._key = null; renderOmni(); }
/* standard addenda: click to switch on, right-click to curate. While one is written the editor takes the row. */
function addendaRow() {
  if (pad.ed) return pad.ed.el || addendumEditor(pad.ed);
  const adds = S.lib.filter(x => x.kind === 'addendum');
  return h('div', { class: 'pchips' }, h('span', { class: 'dl' }, 'addenda'),
    ...adds.map(x => {
      const b = h('button', { class: 'chip', 'aria-pressed': String(addendumOn(x)), title: (x.text || '').slice(0, 400) + '\n\n(right-click: edit or remove)' }, h('i', { class: 'dot' }), x.title);
      b.addEventListener('click', () => { const m = S.prefs.addenda = S.prefs.addenda || {}; m[x.id] = !addendumOn(x); savePrefs(); redrawPrompts(); });
      b.addEventListener('contextmenu', ev => { ev.preventDefault(); addendumMenu(x, ev.clientX, ev.clientY); });
      return b;
    }),
    h('button', { class: 'chip add', title: 'Write a new standard addendum', onclick: () => { pad.ed = { id: null, title: '', text: '' }; redrawPrompts(); } }, '+ add'),
    h('button', { class: 'chip add', title: 'Standard prompts (or type / in the prompt)', onclick: ev => promptsMenu(ev.currentTarget, padOpenTa()) }, 'prompts ▾'));
}
/* the prompt that is on screen: the pad's while it is open, else the bar's */
const padOpenTa = () => (pad.open ? padTa : omniTa);
/* where the keyboard goes after the pad or the bar was drawn again: back into the addendum editor if
   that had it, else to the prompt with its selection (the pad always keeps it, the bar only if it had it) */
function editorFocus() { return pad.ed && pad.ed.el && pad.ed.el.contains(document.activeElement) ? document.activeElement : null; }
function settleKeys(ta, had, sel, edFocus, always) {
  if (edFocus) edFocus.focus();
  else if (pad.ed && pad.ed.fresh) { pad.ed.fresh = false; const f = pad.ed.el.querySelector('input'); if (f) f.focus(); }
  else if (had || (always && !pad.ed)) { ta.focus(); if (sel) try { ta.setSelectionRange(sel[0], sel[1]); } catch {} }
}
function renderPad() {
  if (!pad.open) return;
  if (!padTa) {
    padTa = h('textarea', { rows: 4, spellcheck: 'false', 'aria-label': 'What should the agent do?' });
    const grow = () => { padTa.style.height = 'auto'; padTa.style.height = Math.min(Math.max(padTa.scrollHeight, 128), innerHeight * 0.44) + 'px'; };
    padTa.addEventListener('input', grow);
    padTa.addEventListener('keydown', e => {
      if (e.isComposing) return;
      if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); padStart(); }   // plain enter is a new line
      else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); padEsc(); }
      else if (e.key === 'Tab') { e.preventDefault(); padStep(e.shiftKey ? -1 : 1); }
      else if (e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) { e.preventDefault(); padEffort(e.key === 'ArrowUp' ? 1 : -1); }
      else if (plainArrow(e) && recall(padTa, e.key === 'ArrowUp' ? -1 : 1)) e.preventDefault();
      else if (e.key === '/' && !padTa.value) { e.preventDefault(); promptsMenu(padTa, padTa); }
    });
    padTa._grow = grow;
    keepDraft(padTa, 'pad');
  }
  const had = document.activeElement === padTa;
  const sel = had ? [padTa.selectionStart, padTa.selectionEnd] : null;
  // the folder new agents start in: the project's unless another was picked here
  const D = newDir(), P = pathParts(D), g = gitOf(D), away = D !== projDir();
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
  else if (g.repo === false && !g.unknown && !g.missing) warns.push(h('div', { class: 'pwarn', style: 'border-color:var(--l3);background:none;color:var(--i2)' }, 'Not a git repository: own worktrees and commit & push nodes need one.'));
  if (away) {
    const pp = pathParts(projDir());
    warns.push(h('div', { class: 'pother' }, 'not the canvas folder ',
      h('span', { title: pp.dir }, pp.name), h('button', { class: 'btn sm dim', title: 'start new agents in the project folder again', onclick: () => setNewDir(projDir()) }, '↩ back to it')));
  }

  const where = h('div', { class: 'pwhere' },
    h('div', { class: 'pwl' }, h('div', { class: 'ppath', title: P.dir }, P.parent || ' '), padFolder(D, P, away),
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
  const box = promptBox({ id: 'pbox', ta: padTa, label: 'start', go: padStart,
    hint: keyHints([['ctrl+enter', 'start'], ['enter', 'new line'], ['tab', 'next agent'], ['/', 'prompts'], ['ctrl+o', 'folder']]) });
  padTa.placeholder = `Tell ${a} what to do in ${P.name}…`;

  const edFocus = editorFocus();
  const chipsRow = addendaRow();

  const top = padEl.scrollTop;
  const pw = padEl.firstElementChild && padEl.firstElementChild.classList.contains('pw') ? padEl.firstElementChild : h('div', { class: 'pw' });
  keepIn(pw, nn([padBoards(), where, h('div', { class: 'pctl' }, ...ctl), box, chipsRow]));
  if (pw.parentNode !== padEl) padEl.replaceChildren(pw);
  padEl.scrollTop = top;
  if (padTa._grow) padTa._grow();
  settleKeys(padTa, had, sel, edFocus, true);
}
padEl.addEventListener('pointerdown', ev => { if (ev.target === padEl && S.nodes.size) closePad(); });
padEl.addEventListener('click', ev => {
  if (pad.ed || !padTa || !ev.target.closest) return;
  if (ev.target.closest('button')) setTimeout(() => { if (pad.open && !pad.ed && !menuEl && !typingInField()) padTa.focus(); }, 0);
});
/* a one-line field (a model name, say) has the keyboard: do not take it back */
const typingInField = () => { const a = document.activeElement; return !!a && a.tagName === 'INPUT'; };
/* the standard prompts, inserted into the prompt that asked (the menu opens upward from the bar) */
function promptsMenu(anchor, ta) {
  const r = anchor.getBoundingClientRect(), above = !pad.open;
  const ps = S.lib.filter(x => x.kind === 'prompt');
  popup([{ cap: 'standard prompts' }, ...(ps.length ? ps.map(x => ({ label: x.title, hint: 'insert', run: () => {
    ta.value = x.text + (ta.value ? '\n' + ta.value : ''); ta._grow(); ta.focus(); ta.setSelectionRange(ta.value.length, ta.value.length);
  } })) : [{ cap: 'none yet: add some in the library' }])], r.left, above ? r.top - 4 : r.bottom + 4, { above });
}
function addendumMenu(x, mx, my) {
  popup([
    { label: 'edit…', run: () => { pad.ed = { id: x.id, title: x.title, text: x.text }; redrawPrompts(); } },
    { k: x.default_on ? '●' : ' ', label: 'on for new agents by default', run: () => { x.default_on = !x.default_on; delete (S.prefs.addenda || {})[x.id]; savePrefs(); saveLib(); redrawPrompts(); } },
    { sep: true },
    { label: 'remove', danger: true, run: () => { S.lib = S.lib.filter(y => y.id !== x.id); saveLib(); redrawPrompts(); } },
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
    saveLib(); pad.ed = null; redrawPrompts();
  };
  const key = e => {
    e.stopPropagation();
    if (e.key === 'Escape') { pad.ed = null; redrawPrompts(); }
    else if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); save(); }
    else if (e.key === 'Enter' && e.target === name) { e.preventDefault(); text.focus(); }
  };
  name.addEventListener('keydown', key); text.addEventListener('keydown', key);
  ed.fresh = true; // renderPad puts the keyboard in the name field as soon as the editor is on screen
  return ed.el = h('div', { class: 'aed' }, name, text,
    h('div', { class: 'row2' }, h('button', { class: 'btn sm pri', onclick: save }, ed.id ? 'save' : 'add'),
      h('button', { class: 'btn sm', onclick: () => { pad.ed = null; redrawPrompts(); } }, 'cancel'),
      h('span', { class: 'note', style: 'margin:0 0 0 auto' }, 'ctrl+enter saves')));
}
/* the folder's name, the pad's headline: a click (or ctrl+o) picks the folder new agents start in */
function padFolder(D, P, away) {
  const b = h('button', { class: 'pname' + (away ? ' away' : ''), title: `${P.dir}\nclick or ctrl+o: start new agents in another folder (a sibling repository, say)`,
    'aria-label': 'Folder: ' + P.dir, onclick: ev => {
      const r = ev.currentTarget.getBoundingClientRect();
      folderPicker(r.left, r.bottom + 6, { title: 'start new agents in', cur: D, onPick: setNewDir });
    } }, P.name);
  if (away) b.style.setProperty('--fh', folderHue(D));
  return b;
}
async function padStart() {
  const text = padTa.value.trim();
  if (!text) { shake($('#pbox')); padTa.focus(); return; }
  const a = S.lastAgent, cfg = S.cfg.agents[a] || {};
  const c = toWorld(innerWidth / 2, innerHeight / 2);
  const extra = { model: cfg.model_arg ? tuned(a, 'model') : '', effort: cfg.effort_arg ? tuned(a, 'effort') : '', addenda: addendaIds(), yolo: cfg.yolo_arg ? yoloDefault() : undefined };
  rememberSent(text); setPrompt(padTa, '');
  closePad();
  savePrefs();
  const id = await spawnAt(a, c.x - NW / 2, c.y - NH / 2, { text, draft: false, run: true, edit: false, extra, lane: true });
  if (id) setTimeout(() => reveal(id), 60);
}
