/* ------------------------------------------------------------ reader: a result, full size
   An agent's answer read the way its terminal showed it: monospace, its own line breaks and
   indents, tables kept whole (a terminal's box table as drawn, a markdown table from `weft done`
   as a table), long lines wrapped under their own indent. A click on a card's answer, r, or the
   flyout's result tab opens it; Esc or a click beside it closes it. Gates keep their flyout. */
const readerEl = $('#reader');
const RD = { id: null, seq: 0, key: '', text: '', z: (() => { try { return +localStorage.getItem('weft.readerZoom') || 1; } catch { return 1; } })() };

function openReader(id) {
  const n = S.nodes.get(id); if (!n) return;
  closeMenu(); closeFly();
  RD.id = id; RD.key = ''; RD.text = '';
  const st = h('span', { class: 'st' }), stx = h('span', { class: 'stx' });
  const body = h('div', { class: 'rbody', tabindex: '0', 'aria-label': `Result of ${n.name}` }, h('div', { class: 'note' }, 'loading…'));
  const b = (label, title, fn, cls = '') => h('button', { class: 'btn sm ' + cls, title, onclick: fn }, label);
  const copy = b('copy', 'copy the whole result', () => { if (!RD.text) return; navigator.clipboard && navigator.clipboard.writeText(RD.text); toast('ok', 'copied'); });
  const pass = b('hand over to…', 'a fresh agent continues from this result', ev => {
    const others = [...S.nodes.values()].filter(x => x.id !== id && kindOf(x) !== 'note');
    if (!others.length) return toast('info', 'no other agent on the canvas');
    const r = ev.currentTarget.getBoundingClientRect();
    popup([{ cap: 'hand this result to…' }, ...others.map(o => ({ label: o.name, hint: agentTag(o.agent), run: () => { send('handover', { from: id, to: o.id }); toast('ok', `handed to ${o.name}`); } }))], r.left, r.bottom + 4);
  });
  const next = b('new agent from this', 'start an agent that receives this result', () => { closeReader(); spawnLinked(id, 'next'); });
  const term = kindOf(n) === 'agent' ? b('terminal', 'the agent’s terminal (Esc comes back here)', () => openFocus(n.agent === 'step' ? hostOf(n).id : id)) : null;
  const cli = h('button', { class: 'rcli', title: 'the same text in your terminal: click to copy the command', onclick: () => {
    navigator.clipboard && navigator.clipboard.writeText(`weft result ${id}`); toast('ok', `copied: weft result ${id}`);
  } }, `$ weft result ${id}`);
  readerEl.replaceChildren(h('div', { class: 'win chamf' },
    h('div', { class: 'fh2' }, st, h('span', { class: 'nm' }, n.name), stx, cli,
      h('span', { class: 'rz' }, b('A−', 'smaller text (-)', () => zoomReader(-1)), b('A+', 'larger text (+)', () => zoomReader(1))),
      copy, pass, next, term, b('back to canvas', 'Esc', closeReader)),
    body));
  readerEl._refs = { st, stx, body, copy, pass, next };
  readerEl.style.setProperty('--rz', RD.z);
  readerEl.hidden = false;
  readerEl.onpointerdown = e => { if (e.target === readerEl) closeReader(); };
  updateReaderLive(true);
  setTimeout(() => readerEl._refs && readerEl._refs.body.focus(), 30);
}
function closeReader() { RD.id = null; readerEl.hidden = true; readerEl.replaceChildren(); readerEl._refs = null; }
function zoomReader(d) {
  RD.z = d ? Math.max(0.7, Math.min(2.2, Math.round((RD.z + d * 0.1) * 10) / 10)) : 1;
  readerEl.style.setProperty('--rz', RD.z);
  try { localStorage.setItem('weft.readerZoom', String(RD.z)); } catch {}
}
function readerKey(e) {
  if (e.ctrlKey || e.metaKey || e.altKey) return;
  if (e.key === '+' || e.key === '=') { e.preventDefault(); zoomReader(1); }
  else if (e.key === '-') { e.preventDefault(); zoomReader(-1); }
  else if (e.key === '0') { e.preventDefault(); zoomReader(0); }
}
/* the head follows every update; the text is fetched again only when the result changed */
async function updateReaderLive(force) {
  const r0 = readerEl._refs, id = RD.id; if (!r0 || !id) return;
  const n = S.nodes.get(id); if (!n) return closeReader();
  const rt = S.rt.get(id);
  r0.st.innerHTML = ICON[rt ? rt.status : 'new'] || ICON.new; r0.st.style.color = 'var(--i2)';
  r0.stx.textContent = statusText(n, rt);
  const key = rt ? [rt.out_n, rt.ok, rt.steps, rt.asked, rt.fresh, rt.out && rt.out.slice(0, 200)].join('\u0001') : '';
  if (!force && key === RD.key) return;
  RD.key = key;
  const seq = ++RD.seq;
  const r = await req('result', { id });
  if (seq !== RD.seq || RD.id !== id || !readerEl._refs) return;
  const { body, copy, pass, next } = readerEl._refs;
  RD.text = r.text || '';
  copy.disabled = pass.disabled = next.disabled = !RD.text;
  const steps = (r.steps || []).slice(0, -1);
  const keep = body.scrollTop;
  body.replaceChildren(...nn([
    rt && rt.fresh ? h('p', { class: 'note rfresh' }, 'You started this run by chatting, so it has not been handed on: hand it over, or drag the card\u2019s \u25c7 to another agent.') : null,
    r.asked ? h('details', { class: 'rask' }, h('summary', {}, h('span', { class: 'rk' }, 'asked '), r.asked.split('\n').find(l => l.trim()) || ''),
      h('div', { class: 'cli dim' }, cliView(r.asked))) : null,
    h('div', { class: 'cli ' + (r.status === false ? 'bad' : RD.text ? 'ok' : 'dim') }, RD.text ? cliView(RD.text) : '(no result yet)'),
    steps.length ? h('div', { class: 'rsteps' }, h('h3', {}, `earlier steps (${steps.length})`),
      steps.map((s, i) => h('details', {}, h('summary', {}, h('span', { class: 'stepn' }, `${i + 1}. `), (s.ok ? '✓ ' : '✕ ') + ((s.prompt || '').split('\n').find(l => l.trim()) || '').slice(0, 120)),
        h('div', { class: 'cli ' + (s.ok ? 'ok' : 'bad') }, cliView(s.text))))) : null,
  ]));
  if (!force) body.scrollTop = keep;
}

/* ---- text as a terminal lays it out */
const BOX_ROW = /^[┌│├└┃┏┣┗╭╰║╔╠╚]/;
const MD_RULE = /^\s*\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)*\|?\s*$/;
function cliView(text) {
  const lines = String(text || '').replace(/\r/g, '').split('\n'), out = [];
  for (let i = 0; i < lines.length; i++) {
    const t = lines[i].trim();
    const fence = t.match(/^(```|~~~)/);
    if (fence) { // code: as written, never wrapped
      const rows = [];
      for (i++; i < lines.length && !lines[i].trim().startsWith(fence[1]); i++) rows.push(lines[i]);
      out.push(h('div', { class: 'cb' }, rows.join('\n') || ' '));
      continue;
    }
    if (t.startsWith('|') && MD_RULE.test(lines[i + 1] || '')) { // a markdown table
      const rows = [];
      for (; i < lines.length && lines[i].trim().startsWith('|'); i++) rows.push(lines[i]);
      i--; out.push(mdTable(rows));
      continue;
    }
    if (BOX_ROW.test(t)) { // a table the terminal drew: its rows stay together and unwrapped
      const rows = [];
      for (; i < lines.length && BOX_ROW.test(lines[i].trim()); i++) rows.push(lines[i]);
      i--; out.push(h('div', { class: 'bx' }, rows.join('\n')));
      continue;
    }
    out.push(cliLine(lines[i]));
  }
  return out;
}
/* one line: wrapped under its own indent (and under the text of a list item, not its bullet) */
function cliLine(l) {
  if (!l.trim()) return h('div', { class: 'cl' }, ' ');
  const ind = l.length - l.trimStart().length, rest = l.slice(ind);
  const head = rest.match(/^#{1,6}\s+(.*)$/);
  if (head) return h('div', { class: 'cl hx', style: `padding-left:${ind}ch` }, inlineMd(head[1].replace(/\s+#+\s*$/, '')));
  const bullet = rest.match(/^([-*+•]|\d{1,3}[.)])\s+/), mark = bullet ? bullet[0].length : 0;
  return h('div', { class: 'cl', style: `padding-left:${ind + mark}ch;text-indent:-${mark}ch` }, inlineMd(rest));
}
/* **bold**, `code` and links; everything else stays as written */
function inlineMd(s) {
  const out = [], re = /(`[^`\n]+`)|(\*\*[^*\n]+\*\*)|(https?:\/\/[^\s<>()\[\]`]+[^\s<>()\[\]`.,;:!?'"])/g;
  let at = 0, m;
  while ((m = re.exec(s))) {
    if (m.index > at) out.push(s.slice(at, m.index));
    if (m[1]) out.push(h('span', { class: 'ic' }, m[1].slice(1, -1)));
    else if (m[2]) out.push(h('b', {}, m[2].slice(2, -2)));
    else out.push(h('a', { href: m[3], target: '_blank', rel: 'noopener' }, m[3]));
    at = re.lastIndex;
  }
  if (at < s.length) out.push(s.slice(at));
  return out;
}
function mdTable(rows) {
  const cells = r => r.trim().replace(/^\|/, '').replace(/(^|[^\\])\|\s*$/, '$1').split(/(?<!\\)\|/).map(c => c.trim().replace(/\\\|/g, '|'));
  const align = cells(rows[1]).map(c => c.endsWith(':') ? (c.startsWith(':') ? 'center' : 'right') : 'left');
  const row = (r, tag) => h('tr', {}, cells(r).map((c, k) => h(tag, { style: `text-align:${align[k] || 'left'}` }, inlineMd(c))));
  return h('div', { class: 'mtw' }, h('table', { class: 'mt' }, h('thead', {}, row(rows[0], 'th')), h('tbody', {}, rows.slice(2).map(r => row(r, 'td')))));
}
