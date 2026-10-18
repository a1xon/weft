/* ------------------------------------------------------------ reader: a result, full size
   An agent's answer read the way its terminal showed it: monospace, its own line breaks and
   indents, tables kept whole (a terminal's box table as drawn, a markdown table from `weft done`
   as a table), long lines wrapped under their own indent. A click on a card's answer, r, or the
   flyout's result tab opens it; Esc or a click beside it closes it. Gates keep their flyout.
   The prompt bar stays over it, aimed at the card: what is read is answered right there. */
const readerEl = $('#reader'), dockEl = $('#dock');
const RD = { id: null, seq: 0, key: '', text: '', qs: [], sent: null, z: (() => { try { return +localStorage.getItem('weft.readerZoom') || 1; } catch { return 1; } })() };
// the reader's text ends above the bar, however tall the bar has grown
new ResizeObserver(() => readerEl.style.setProperty('--dockh', dockEl.offsetHeight + 'px')).observe(dockEl);
const dockOver = on => dockEl.classList.toggle('over', !!on);

function openReader(id) {
  const n = S.nodes.get(id); if (!n) return;
  closeMenu(); closeFly();
  if (!(S.sel.size === 1 && S.sel.has(id))) select([id]);   // the bar over it talks to this card
  RD.id = id; RD.key = ''; RD.text = ''; RD.qs = []; RD.sent = null;
  const st = h('span', { class: 'st' }), stx = h('span', { class: 'stx' });
  const qs = h('button', { class: 'btn sm rq', hidden: true, title: 'questions this answer asks you: jump to the next one', onclick: nextQuestion });
  const skip = h('button', { class: 'btn sm rskip', hidden: true, title: 'nothing to answer: the report is the result as it stands, and goes on',
    onclick: () => { send('skip', { id }); omniTa.value = ''; growOmni(); } }, 'skip all');
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
    h('div', { class: 'fh2' }, st, h('span', { class: 'nm' }, n.name), stx, qs, skip, cli,
      h('span', { class: 'rz' }, b('A−', 'smaller text (-)', () => zoomReader(-1)), b('A+', 'larger text (+)', () => zoomReader(1))),
      copy, pass, next, term, b('back to canvas', 'Esc', closeReader)),
    body));
  readerEl._refs = { st, stx, body, copy, pass, next, qs, skip };
  readerEl.style.setProperty('--rz', RD.z);
  readerEl.hidden = false;
  readerEl.onpointerdown = e => { if (e.target === readerEl) closeReader(); };
  dockOver(true);
  if (omniTa && !omniTa._rd) { omniTa._rd = true; omniTa.addEventListener('input', () => RD.id && markAnswered()); }
  updateReaderLive(true);
  setTimeout(() => readerEl._refs && readerEl._refs.body.focus(), 30);
}
function closeReader() { RD.id = null; dockOver(false); readerEl.hidden = true; readerEl.replaceChildren(); readerEl._refs = null; }
/* what the bar sent while the reader was open: the reader follows the card that answers it, a new
   agent is shown on the canvas */
function readerSent(id, text) {
  if (!RD.id) return;
  if (!id) return closeReader();
  if (id !== RD.id) return openReader(id);
  RD.sent = new Set(RD.qs.filter(q => answerIn(q, text)).map(q => q.text));
  markAnswered();
}
/* Esc in the bar goes back to reading and keeps what was typed */
function readerBack() {
  if (!RD.id || !readerEl._refs) return false;
  readerEl._refs.body.focus();
  return true;
}
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
  // typing answers: Enter or a letter goes to the bar (the letter lands in it, under what is there)
  else if (RD.id && (e.key === 'Enter' || /^\p{L}$/u.test(e.key))) {
    if (e.key === 'Enter') e.preventDefault();
    else if (omniTa.value.trim() && !omniTa.value.endsWith('\n')) { omniTa.value = omniTa.value.trimEnd() + '\n\n'; growOmni(); }
    omniTa.focus(); omniTa.setSelectionRange(omniTa.value.length, omniTa.value.length);
  }
}
/* the head follows every update; the text is fetched again only when the result changed */
async function updateReaderLive(force) {
  const r0 = readerEl._refs, id = RD.id; if (!r0 || !id) return;
  const n = S.nodes.get(id); if (!n) return closeReader();
  const rt = S.rt.get(id);
  r0.st.innerHTML = ICON[rt ? rt.status : 'new'] || ICON.new; r0.st.style.color = 'var(--i2)';
  r0.stx.textContent = statusText(n, rt);
  const busy = !!(rt && (rt.status === 'busy' || rt.status === 'starting'));
  const key = rt ? [rt.out_n, rt.ok, rt.steps, rt.asked, busy, rt.issue, rt.report, rt.out && rt.out.slice(0, 200)].join('\u0001') : '';
  if (!force && key === RD.key) return;
  RD.key = key;
  const seq = ++RD.seq;
  const r = await req('result', { id });
  if (seq !== RD.seq || RD.id !== id || !readerEl._refs) return;
  const { body, copy, pass, next } = readerEl._refs;
  const was = RD.text;
  RD.text = r.text || '';
  if (RD.text !== was) RD.sent = null;   // a new answer asks its own questions
  copy.disabled = pass.disabled = next.disabled = !RD.text;
  const steps = (r.steps || []).slice(0, -1);
  const keep = body.scrollTop;
  // a report that waits for answers is the text; what an ISSUE asks stands above the answer before it
  const rt2 = S.rt.get(id) || {}, issue = rt2.issue && !rt2.report ? rt2.issue : '';
  const blocks = rt2.report ? askBlocks(rt2.asks, RD.text) : [], asked = issue ? askBlocks(rt2.asks, issue) : [];
  RD.qs = [...asked, ...blocks].flatMap(b => b.qs);
  body.replaceChildren(...nn([
    issue ? h('div', { class: 'rissue' }, h('h3', {}, `${n.name} asks you`), h('div', { class: 'cli' }, cliView(issue, asked))) : null,
    busy && RD.text ? h('p', { class: 'note rwork' }, `${n.name} is working: this is its answer from before, the next one shows here when it is in.`) : null,
    r.asked ? h('details', { class: 'rask' }, h('summary', {}, h('span', { class: 'rk' }, 'asked '), r.asked.split('\n').find(l => l.trim()) || ''),
      h('div', { class: 'cli dim' }, cliView(r.asked))) : null,
    h('div', { class: 'cli ' + (r.status === false ? 'bad' : RD.text ? 'ok' : 'dim') }, RD.text ? cliView(RD.text, blocks) : '(no result yet)'),
    steps.length ? h('div', { class: 'rsteps' }, h('h3', {}, `earlier steps (${steps.length})`),
      steps.map((s, i) => h('details', {}, h('summary', {}, h('span', { class: 'stepn' }, `${i + 1}. `), (s.ok ? '✓ ' : '✕ ') + ((s.prompt || '').split('\n').find(l => l.trim()) || '').slice(0, 120)),
        h('div', { class: 'cli ' + (s.ok ? 'ok' : 'bad') }, cliView(s.text))))) : null,
  ]));
  markAnswered();
  if (!force) body.scrollTop = RD.text !== was ? 0 : keep;
}

/* ---- what an agent asks you
   weft finds the questions and decisions an answer leaves to you (src/asks.rs) and holds the card
   while they are open; they come with it (`r.asks`). Marked where they stand (a violet line beside
   them), never popped up, each with quick answers: yes / no or the options it gave, "do it" for a
   decision it left to you, "you decide", "skip", and "answer…" to write your own. A pick writes
   "> question" and the answer into a text box (the bar over the reader, the box under the card) as
   plain text that can still be edited; nothing is sent before you send it. Sending only skips, or
   "skip all", lets the report go on as it is. */
const ASK_DO = 'yes, do it', ASK_DECIDE = 'you decide: do what you think is best', ASK_SKIP = 'skip: leave this as it is for now';

/* the questions of `rt` grouped by the lines of `text` they stand on: [{from, to, ind, qs}] */
function askBlocks(asks, text) {
  const lines = String(text || '').replace(/\r/g, '').split('\n'), at = new Map();
  for (const a of asks || []) {
    if (a.from >= lines.length) continue;
    if (!at.has(a.from)) at.set(a.from, { from: a.from, to: Math.min(a.to, lines.length - 1), ind: lines[a.from].length - lines[a.from].trimStart().length, qs: [] });
    at.get(a.from).qs.push({ ...a });
  }
  return [...at.values()];
}
/* the quick answers of one question, written into the text box `ta`; `after` runs once one is picked */
function askChips(q, ta, after) {
  const c = (label, ans, title, cls = '') => h('button', { class: 'qc ' + cls, title, 'data-a': ans == null ? '' : ans,
    onclick: ev => { ev.stopPropagation(); answerQ(ta, q, ans); after && after(); } }, label);
  return [
    ...(q.opts && q.opts.length ? q.opts.map(o => c(o.label, o.answer, 'answer: ' + o.answer))
      : q.yn ? [c('yes', 'yes', 'answer: yes'), c('no', 'no', 'answer: no')]
      : q.kind === 'decision' ? [c('do it', ASK_DO, 'answer: ' + ASK_DO)] : []),
    q.kind === 'need' ? null : c('you decide', ASK_DECIDE, 'answer: ' + ASK_DECIDE, 'qm'),
    c('skip', ASK_SKIP, 'no answer to this one: ' + ASK_SKIP, 'qm qs'),
    c('answer…', null, 'write your own answer to it', 'qm qq'),
  ].filter(Boolean);
}
/* where a question's answer sits in a text: after "> question", up to the next blank line */
function answerAt(q, v) {
  const quote = '> ' + q.text, at = v.indexOf(quote);
  if (at < 0 || (at + quote.length < v.length && v[at + quote.length] !== '\n')) return null;
  const s = Math.min(v.length, at + quote.length + 1), e = v.indexOf('\n\n', s);
  return { at, s, e: e < 0 ? v.length : e };
}
const answerIn = (q, v) => { const a = answerAt(q, v || ''); return a ? v.slice(a.s, a.e).trim() : ''; };
function answerQ(ta, q, ans) {
  const v = ta.value, a = answerAt(q, v);
  let out, caret;
  if (a) {
    const keep = ans == null ? v.slice(a.s, a.e) : ans;
    out = v.slice(0, a.at) + '> ' + q.text + '\n' + keep + v.slice(a.e);
    caret = a.at + q.text.length + 3 + keep.length;
  } else {
    out = (v.trimEnd() ? v.trimEnd() + '\n\n' : '') + '> ' + q.text + '\n' + (ans || '');
    caret = out.length;
  }
  ta.value = out;
  ta.dispatchEvent(new Event('input'));
  ta.focus(); ta.setSelectionRange(caret, caret);
}
/* the chips follow the text box: the answer picked is lit, an answered question dims */
function markAsks(qs, v) {
  for (const q of qs) {
    const ans = answerIn(q, v);
    q.done = !!ans || !!q.sent;
    if (q.bar) for (const c of q.bar.querySelectorAll('.qc')) c.classList.toggle('on', !!ans && c.dataset.a === ans);
  }
  for (const q of qs) if (q.box) q.box.classList.toggle('qd', qs.every(x => x.box !== q.box || x.done));
}
/* answers that only skip have nothing for the agent: a report that waits for them goes on as it is */
function sendAnswers(id, text) {
  const r = S.rt.get(id), blocks = String(text || '').trim().split(/\n{2,}/);
  const skips = blocks.every(b => { const l = b.split('\n'); return l.length === 2 && l[0].startsWith('> ') && l[1].trim() === ASK_SKIP; });
  if (!(r && r.report && skips)) return false;
  send('skip', { id });
  return true;
}

/* a question's block in the reader, with its quick answers at the end of its last line (one row each when it asks several) */
function qBlock(blk, rows) {
  const el = h('div', { class: 'qb' }, rows);
  for (const q of blk.qs) q.box = el;
  if (blk.qs.length === 1) rows[rows.length - 1].append(qChips(blk.qs[0], null));
  else el.append(...blk.qs.map(q => qChips(q, blk.ind)));
  return el;
}
function qChips(q, row) {   // row: the indent of a row of its own, or null for the end of the line
  const kids = askChips(q, omniTa, markAnswered);
  q.bar = row == null ? h('span', { class: 'qa' }, kids)
    : h('div', { class: 'qa row', style: `padding-left:${row}ch` }, h('span', { class: 'qt', title: q.text }, q.text), kids);
  return q.bar;
}
/* the reader's marks follow the bar: answered there (or sent already), or still open */
function markAnswered() {
  const r = readerEl._refs; if (!r || !r.qs) return;
  for (const q of RD.qs) q.sent = !!(RD.sent && RD.sent.has(q.text));
  markAsks(RD.qs, omniTa ? omniTa.value : '');
  const seen = new Set();
  let open = 0;
  for (const q of RD.qs) if (!seen.has(q.text)) { seen.add(q.text); if (!q.done) open++; }
  const all = seen.size, sentAll = RD.sent && [...seen].every(t => RD.sent.has(t));
  const rt = S.rt.get(RD.id);
  r.qs.hidden = !all;
  r.skip.hidden = !(all && rt && rt.report && !sentAll);
  r.qs.textContent = !open ? (sentAll ? '✓ answers sent' : `✓ ${all} answered · ctrl+enter sends`) : open === all ? `? ${all} question${all > 1 ? 's' : ''} for you` : `? ${open} of ${all} open`;
}
/* the header's question count jumps to the next open one */
function nextQuestion() {
  const r = readerEl._refs; if (!r || !RD.qs.length) return;
  const open = RD.qs.filter(q => !q.done), list = open.length ? open : RD.qs;
  const y = q => q.box.getBoundingClientRect().top - r.body.getBoundingClientRect().top + r.body.scrollTop;
  const q = list.find(x => y(x) > r.body.scrollTop + 90) || list[0];
  r.body.scrollTo({ top: Math.max(0, y(q) - 80), behavior: matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth' });
  q.box.classList.remove('flash'); void q.box.offsetWidth; q.box.classList.add('flash');
}

/* ---- text as a terminal lays it out */
const BOX_ROW = /^[┌│├└┃┏┣┗╭╰║╔╠╚]/;
const MD_RULE = /^\s*\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)*\|?\s*$/;
function cliView(text, asking = []) {
  const lines = String(text || '').replace(/\r/g, '').split('\n'), out = [];
  const qat = new Map(asking.map(b => [b.from, b]));
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
    const q = qat.get(i);
    if (q) { // a question for the person: its lines together, marked, with its quick answers
      const rows = [];
      for (; i <= q.to; i++) rows.push(cliLine(lines[i]));
      i--; out.push(qBlock(q, rows));
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
