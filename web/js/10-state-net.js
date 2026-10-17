/* ------------------------------------------------------------ state */
const S = {
  project: {}, cfg: { agents: {} }, lib: [],
  nodes: new Map(), edges: new Map(), rt: new Map(),
  sel: new Set(), selEdge: null, focus: null, scroll: 0,
  fly: null, lens: new Map(), rid: 0, acks: new Map(),
  v: { x: 90, y: 110, k: 1 }, lastAgent: 'claude', lastNode: null, undo: [],
  prefs: {}, models: {}, modelErr: {}, omniMode: 'then',
  dirs: {}, sibs: null,   // other folders agents work in: their branch and uncommitted files; the project's neighbours
};
const nodeEls = new Map(), edgeEls = new Map(), juncEls = new Map();
let ws = null, token = null, loaded = false;
const primary = () => (S.sel.size === 1 ? [...S.sel][0] : null);
const KINDS = ['note', 'gate', 'shot', 'tests', 'git'];
const kindOf = n => (n && KINDS.includes(n.agent)) ? n.agent : 'agent';
const isGate = k => k === 'gate' || k === 'shot' || k === 'tests';
const incoming = id => [...S.edges.values()].filter(e => e.to === id);
const outgoing = id => [...S.edges.values()].filter(e => e.from === id);

/* ------------------------------------------------------------ network */
function connect() {
  const proto = location.protocol === 'https:' ? 'wss' : 'ws';
  ws = new WebSocket(`${proto}://${location.host}/ws?t=${encodeURIComponent(token)}`);
  ws.onopen = () => { $('#banner').hidden = true; const d = $('#liveDot'); d.className = 'live-dot'; d.textContent = 'live'; };
  ws.onmessage = e => onMsg(JSON.parse(e.data));
  ws.onclose = () => { if (gone) return; if (leaving) return farewell(leaving); $('#banner').hidden = false; const d = $('#liveDot'); d.className = 'live-dot off'; d.textContent = 'offline'; setTimeout(connect, 1000); };
}
function send(op, args = {}) { if (ws && ws.readyState === 1) ws.send(JSON.stringify({ op, ...args })); }
function req(op, args = {}, wait = 8000) {
  return new Promise(res => {
    const rid = ++S.rid;
    S.acks.set(rid, res);
    send(op, { ...args, rid });
    setTimeout(() => { if (S.acks.delete(rid)) res({ ok: false, error: 'timeout' }); }, wait);
  });
}
function onMsg(m) {
  switch (m.t) {
    case 'state': loadState(m); break;
    case 'flow': applyFlow(m.nodes, m.edges); applyTalks(m.talks); break;
    case 'talk': onTalk(m); break;
    case 'rt': queueRt(m); break;
    case 'scr': onScreen(m); break;
    case 'lib': S.lib = m.library; onLibChanged(); break;
    case 'nets': S.nets = m.networks; onNetsChanged(); break;
    case 'prefs': if (!prefDirty) { S.prefs = m.prefs || {}; redrawPrompts(); renderNetChips(); } break;
    case 'proj':
      if (!m.dir || m.dir === S.project.dir) S.project.git = m.git;
      if (m.dir) S.dirs[m.dir] = m.git;
      folderChanged();
      break;
    case 'models': {
      S.models[m.agent] = m.models || []; S.modelErr[m.agent] = m.err || null;
      if (menuEl && menuEl.dataset.kind === 'model' && modelMenu.last) modelMenu.last.again();
      redrawPrompts();
      break;
    }
    case 'toast': if (!(netBuilding && m.k === 'info' && / is now asked of /.test(m.m))) toast(m.k, m.m); break;
    // a card made from the browser (weft marks): in view when the person comes back, unless they are here working
    case 'reveal': whenNode(m.id).then(() => { if (!document.hasFocus()) reveal(m.id); }); break;
    // (a daemon making way for a new build: the socket closes and the page reconnects to the new one)
    case 'bye': if (!m.replace) farewell(m); break;
    case 'archive': onArchive(m); break;
    case 'boards': onBoards(m.list); break;
    case 'board': onBoardName(m.name); break;
    case 'idle': S.idle = m.since || null; refreshFiling(); break;
    case 'hold': S.hold = !!m.on; renderHold(); refreshFiling(); break;
    case 'hand': bead(m.edge, m.ok); break;
    case 'ack': { const f = S.acks.get(m.rid); if (f) { S.acks.delete(m.rid); f(m.r); } break; }
  }
}
function loadState(m) {
  // the daemon was replaced by another build (a new install): fetch the matching page
  const meta = document.querySelector('meta[name=weft-build]');
  const mine = meta ? meta.content : '';
  if (m.build && mine && mine !== m.build) { location.reload(); return; }
  S.project = m.project; S.cfg = m.cfg; S.lib = m.library;
  S.idle = m.idle || null;
  S.hold = !!m.hold;
  S.nets = m.networks;   // (undefined from a daemon that keeps none: the built-in networks still work)
  S.prefs = m.cfg.prefs && typeof m.cfg.prefs === 'object' ? m.cfg.prefs : {};
  S.models = m.models || {};
  S.dirs = m.project.dirs || {};
  document.title = `${m.project.name} – weft`;
  BD.on = Array.isArray(m.boards);
  onBoards(m.boards || []);
  S.rt.clear();
  for (const r of m.rt) S.rt.set(r.id, r);
  if (!loaded) {
    loaded = true;
    try {
      const v = JSON.parse(localStorage.getItem('weft-view:' + m.project.dir) || 'null');
      if (v && isFinite(v.k)) S.v = v;
      S.lastAgent = localStorage.getItem('weft-agent') || S.lastAgent;
    } catch {}
    // what you used last wins, wherever you used it
    if (S.prefs.agent && S.cfg.agents[S.prefs.agent] && S.prefs.agent !== 'sh') S.lastAgent = S.prefs.agent;
  }
  applyFlow(m.flow.nodes, m.flow.edges);
  S.talkLog = m.talk || []; S.talkOn = Array.isArray(m.talk);
  S.fixOn = !!m.fix;
  applyTalks(m.flow.talks);
  for (const id of S.nodes.keys()) renderRt(id);
  onArchive({ groups: m.archive });
  applyView();
  renderTicker(); renderOmni(); renderWhere(); renderNetChips();
  applyBootAt();
  watchDir(newDir());   // the folder new agents start in, if it is not the project's
  if (S.focus) send('watch', { id: S.focus });
  // the first thing you see: where you are, and a prompt waiting for you; unless agents are at work,
  // then it is the board (the bar at the bottom still takes a prompt)
  if (!padShown) { padShown = true; if (!boardActive()) openPad(); }
}
let padShown = false;
const boardActive = () => [...S.rt.values()].some(r => ['busy', 'starting', 'attention'].includes(r.status));
