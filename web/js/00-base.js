'use strict';
/* ====================================================================
   weft canvas. One WebSocket, one world <div>. Everything is manipulated
   in place: prompts are edited on the card, results unfold from the card,
   links are dragged, clicked and right-clicked. No side panels.
   ==================================================================== */
/* a card's size: the default, or what the person dragged it to (n.w, n.h) */
const NW = 500, NH = 260, PY = 21, JX = 82;
const nw = n => (n && n.w) || NW, nh = n => (n && n.h) || NH;
const $ = (s, r = document) => r.querySelector(s);
const nn = a => a.flat(9).filter(x => x != null && x !== false);
const PROPS = new Set(['value', 'checked', 'selected', 'disabled', 'hidden']);
function h(tag, props, ...kids) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(props || {})) {
    if (v == null || v === false) continue;
    if (k === 'class') e.className = v;
    else if (k.startsWith('on')) e.addEventListener(k.slice(2), v);
    else if (k === 'html') e.innerHTML = v;
    else if (PROPS.has(k)) e[k] = v;
    else if (v === true) e.setAttribute(k, '');
    else e.setAttribute(k, v);
  }
  for (const c of nn(kids)) e.append(c.nodeType ? c : document.createTextNode(c));
  return e;
}
const SVGNS = 'http://www.w3.org/2000/svg';
function sv(tag, attrs) {
  const e = document.createElementNS(SVGNS, tag);
  for (const [k, v] of Object.entries(attrs || {})) e.setAttribute(k, v);
  return e;
}
const I = (d, fill) => `<svg viewBox="0 0 16 16" fill="${fill ? 'currentColor' : 'none'}" stroke="currentColor" stroke-width="1.5" stroke-linecap="square" stroke-linejoin="miter">${d}</svg>`;
const ICON = {
  new: I('<circle cx="8" cy="8" r="5" stroke-dasharray="2 2.4"/>'),
  draft: I('<rect x="3.2" y="3.2" width="9.6" height="9.6" stroke-dasharray="2.4 2.2"/>'),
  waiting: I('<circle cx="8" cy="8" r="5" stroke-dasharray="1.6 2.2"/><path d="M8 5.2V8l2 1.4"/>'),
  blocked: I('<rect x="3.2" y="3.2" width="9.6" height="9.6"/><path d="M3.2 12.8l9.6-9.6"/>'),
  starting: I('<path d="M8 2.8A5.2 5.2 0 1 1 2.8 8"/>'),
  ready: I('<path d="M8 2.6l5.4 5.4L8 13.4 2.6 8z"/>'),
  busy: I('<path d="M8 2.8A5.2 5.2 0 1 1 2.8 8"/>'),
  idle: I('<path d="M6 4v8M10 4v8"/>'),
  attention: I('<path d="M8 2.4l6 10.6H2z"/><path d="M8 6.6v3"/><path d="M8 11.4v.1"/>'),
  done: I('<path d="M3 8.4l3.2 3.2L13 4.6"/>'),
  failed: I('<path d="M4 4l8 8M12 4l-8 8"/>'),
  exited: I('<rect x="3.6" y="3.6" width="8.8" height="8.8"/>'),
  stopped: I('<rect x="3.6" y="3.6" width="8.8" height="8.8"/><path d="M6 6l4 4"/>'),
  note: I('<path d="M3.2 2.4h6.4l3.2 3.2v8H3.2z"/><path d="M9.6 2.4v3.2h3.2"/><path d="M5.4 8.2h5.2M5.4 10.6h5.2"/>'),
  gate: I('<path d="M3 13.5V2.5M13 13.5V2.5M3 5.5h10M3 10.5h10"/>'),
  shot: I('<path d="M2.4 5h2.4l1-1.6h4.4l1 1.6h2.4v7.4H2.4z"/><circle cx="8" cy="8.6" r="2.2"/>'),
  tests: I('<path d="M6 2.4h4M6.6 2.4v4L3.2 12.6a1 1 0 0 0 .9 1.5h7.8a1 1 0 0 0 .9-1.5L9.4 6.4v-4"/><path d="M5.2 10.4h5.6"/>'),
  eta: I('<circle cx="8" cy="9" r="5"/><path d="M8 9V6M6.4 2.4h3.2"/>'),
  changes: I('<path d="M3 5h6M6 2v6"/><path d="M8 12h5"/>'),
  ping: I('<path d="M4 11V7a4 4 0 0 1 8 0v4l1 1.5H3z"/><path d="M6.5 14h3"/>'),
  git: I('<circle cx="4.4" cy="3.8" r="1.7"/><circle cx="4.4" cy="12.2" r="1.7"/><circle cx="11.6" cy="7" r="1.7"/><path d="M4.4 5.5v5M4.4 8.2c0-1.2.9-1.2 2.2-1.2h3.3"/>'),
  play: I('<path d="M4.5 3v10l8-5z"/>', true),
  stop: I('<rect x="4" y="4" width="8" height="8"/>', true),
  open: I('<path d="M9.5 2.5h4v4M13.5 2.5L8 8M12.5 9v4h-9v-9h4"/>'),
  gear: I('<circle cx="8" cy="8" r="2.2"/><path d="M8 1.8v2M8 12.2v2M1.8 8h2M12.2 8h2M3.6 3.6l1.4 1.4M11 11l1.4 1.4M12.4 3.6L11 5M5 11l-1.4 1.4"/>'),
  x: I('<path d="M4 4l8 8M12 4l-8 8"/>'),
};
