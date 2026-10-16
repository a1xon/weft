// weft marks, in the page's own world: what only the page's scripts can see. Which component
// rendered an element and where it is written (React, Vue, Svelte, Angular, Astro, inspector
// plugins), and what went wrong on the page (console errors, uncaught errors, failed loads).
// On dev hosts (localhost, 127.0.0.1, *.localhost) this runs from the start so earlier errors are
// kept too; anywhere else it arrives with the picker. The picker asks with a `weft:probe` event
// on the element and gets the answer back as a `weft:probed` event (both synchronous).
(() => {
  const K = Symbol.for('weft.probe');
  if (window[K]) return;
  window[K] = true;

  // ------------------------------------------------------------ what went wrong on the page
  const log = [];
  const str = a => {
    if (a instanceof Error) return a.stack && a.stack.includes(a.message) ? a.stack : `${a.name}: ${a.message}\n${a.stack || ''}`;
    if (typeof a === 'string') return a;
    if (a instanceof Element) return '<' + a.localName + (a.id ? '#' + a.id : '') + '>';
    try { return JSON.stringify(a).slice(0, 300); } catch { return String(a); }
  };
  // console.error('%s failed: %o', x, y) the way the console shows it
  const fmt = args => {
    if (typeof args[0] !== 'string' || !/%[sdifoOc]/.test(args[0])) return args.map(str).join(' ');
    let i = 1;
    const head = args[0].replace(/%[sdifoOc]/g, t => (i >= args.length ? t : t === '%c' ? (i++, '') : str(args[i++])));
    return [head, ...args.slice(i).map(str)].join(' ');
  };
  const note = (k, text) => {
    const m = String(text).replace(/\n\s*\n/g, '\n').slice(0, 900);
    const last = log[log.length - 1];
    if (last && last.k === k && last.m === m) { last.n++; last.t = Date.now(); return; }
    log.push({ k, m, n: 1, t: Date.now() });
    if (log.length > 40) log.shift();
  };
  for (const k of ['error', 'warn']) {
    const orig = console[k];
    console[k] = function (...a) { try { note(k, fmt(a)); } catch {} return orig.apply(this, a); };
  }
  addEventListener('error', e => {
    const t = e.target;
    if (t && t !== window && t.localName) note('load', `${t.localName} did not load: ${t.currentSrc || t.src || t.href || ''}`);
    else note('uncaught', e.error ? str(e.error) : `${e.message} (${e.filename}:${e.lineno}:${e.colno})`);
  }, true);
  addEventListener('unhandledrejection', e => note('rejection', 'unhandled rejection: ' + str(e.reason)));

  // requests that came back with an error status (the browser keeps them; nothing is wrapped)
  const failed = () => performance.getEntriesByType('resource')
    .filter(r => r.responseStatus >= 400)
    .slice(-12)
    .map(r => ({ status: r.responseStatus, url: r.name, kind: r.initiatorType }));

  // ----------------------------------------------------------- which component, written where
  const page = location.origin;
  // a frame of a stack in the page's own code, as a path: http://localhost:5173/src/App.tsx?t=1:12:5 -> src/App.tsx:12
  const frame = line => {
    const m = /\(?((?:https?|file|webpack-internal|rsc):\/\/[^\s)]+?):(\d+):(\d+)\)?\s*$/.exec(line);
    if (!m) return null;
    let u = m[1];
    if (/node_modules|\/react-dom|\/react\.|jsx-dev-runtime|\/chunk-|\/deps\//.test(u)) return null;
    if (u.startsWith(page)) u = u.slice(page.length + 1);
    u = u.replace(/^webpack-internal:\/\/\/(\([^)]*\)\/)?\.?\/?/, '').replace(/^rsc:\/\/React\/\w+\//, '').replace(/\?.*$/, '');
    return `${decodeURI(u)}:${m[2]}`;
  };
  const fromStack = s => {
    if (!s) return '';
    const lines = String(s.stack || s).split('\n').slice(1);
    for (const l of lines) { const f = frame(l); if (f) return f; }
    return '';
  };
  const reactName = t => {
    if (!t || (typeof t !== 'function' && typeof t !== 'object')) return '';
    return t.displayName || t.name
      || (t.render && (t.render.displayName || t.render.name))
      || (t.type && (t.type.displayName || t.type.name)) || '';
  };
  const reactSrc = f => (f._debugSource ? `${f._debugSource.fileName}:${f._debugSource.lineNumber}` : fromStack(f._debugStack));
  function react(el) {
    for (let n = el; n; n = n.parentElement) {
      const key = Object.keys(n).find(k => k.startsWith('__reactFiber$') || k.startsWith('__reactInternalInstance$'));
      if (!key) continue;
      const host = n[key];
      const chain = [];
      for (let f = host.return; f && chain.length < 7; f = f.return) {
        const name = reactName(f.type);
        if (!name || /^(Fragment|StrictMode|Suspense|Profiler)$/.test(name)) continue;
        chain.push({ name, src: reactSrc(f) });
      }
      // where the element itself is written: the JSX in the component that rendered it
      return { fw: 'react', at: n === el ? reactSrc(host) : '', chain };
    }
    return null;
  }
  function vue(el) {
    for (let n = el; n; n = n.parentElement) {
      let c = n.__vueParentComponent;
      if (c) {
        const chain = [];
        for (; c && chain.length < 7; c = c.parent) {
          const t = c.type || {};
          const file = t.__file || '';
          const name = t.name || t.__name || (file ? file.split('/').pop().replace(/\.vue$/, '') : '');
          if (name || file) chain.push({ name: name || '?', src: file });
        }
        return { fw: 'vue', at: '', chain };
      }
      let v = n.__vue__;
      if (v) {
        const chain = [];
        for (; v && chain.length < 7; v = v.$parent) {
          const o = v.$options || {};
          const name = o.name || o._componentTag || '';
          if (name || o.__file) chain.push({ name: name || '?', src: o.__file || '' });
        }
        return { fw: 'vue 2', at: '', chain };
      }
    }
    return null;
  }
  function svelte(el) {
    const chain = [];
    for (let n = el; n && chain.length < 7; n = n.parentElement) {
      const m = n.__svelte_meta;
      const loc = m && m.loc;
      if (!loc || !loc.file) continue;
      // (Svelte 4 counts lines from 0 and has `char`; Svelte 5 counts from 1)
      const src = loc.line != null ? `${loc.file}:${loc.line + (loc.char != null ? 1 : 0)}` : loc.file;
      if (!chain.length || chain[chain.length - 1].src.split(':')[0] !== loc.file) {
        chain.push({ name: loc.file.split('/').pop().replace(/\.svelte$/, ''), src });
      }
    }
    return chain.length ? { fw: 'svelte', at: chain[0].src, chain } : null;
  }
  function angular(el) {
    const ng = window.ng;
    if (!ng || typeof ng.getOwningComponent !== 'function') return null;
    const chain = [];
    let c = ng.getComponent && ng.getComponent(el) || ng.getOwningComponent(el);
    for (let guard = 0; c && chain.length < 7 && guard < 20; guard++) {
      chain.push({ name: c.constructor && c.constructor.name || '?', src: '' });
      const hostEl = ng.getHostElement && ng.getHostElement(c);
      c = hostEl && hostEl.parentElement ? ng.getOwningComponent(hostEl.parentElement) : null;
      if (c && chain.some(x => x.name === (c.constructor && c.constructor.name))) break;
    }
    return chain.length ? { fw: 'angular', at: '', chain } : null;
  }

  // what dev servers and inspector plugins write onto elements
  const SOURCE_ATTRS = ['data-astro-source-file', 'data-astro-source-loc', 'data-insp-path', 'data-v-inspector',
    'data-source', 'data-source-file', 'data-source-loc', 'data-component', 'data-component-name', 'data-testid', 'data-test-id', 'data-cy', 'data-qa'];
  function attrs(el) {
    const out = {};
    for (let n = el, depth = 0; n && n.getAttribute && depth < 12; n = n.parentElement, depth++) {
      for (const a of SOURCE_ATTRS) {
        const v = n.getAttribute(a);
        if (v != null && !(a in out)) out[a] = depth ? `${v} (on <${n.localName}> ${depth} up)` : v;
      }
    }
    if (out['data-astro-source-file'] && out['data-astro-source-loc'] && !/ up\)$/.test(out['data-astro-source-file'])) {
      out.at = `${out['data-astro-source-file']}:${String(out['data-astro-source-loc']).split(':')[0]}`;
    }
    return out;
  }

  function probe(el) {
    let comp = null;
    for (const f of [react, vue, svelte, angular]) {
      try { comp = f(el); } catch { comp = null; }
      if (comp) break;
    }
    const a = attrs(el);
    if (a.at && comp && !comp.at) comp.at = a.at;
    if (a.at && !comp) comp = { fw: 'astro', at: a.at, chain: [] };
    delete a.at;
    return { comp, attrs: a };
  }

  addEventListener('weft:probe', e => {
    let out;
    try { out = probe(e.target); } catch (err) { out = { error: String(err) }; }
    document.dispatchEvent(new CustomEvent('weft:probed', { detail: JSON.stringify(out) }));
  }, true);
  addEventListener('weft:log', () => {
    let out;
    try { out = { log: log.slice(-15), failed: failed() }; } catch (err) { out = { log: [], failed: [], error: String(err) }; }
    document.dispatchEvent(new CustomEvent('weft:logged', { detail: JSON.stringify(out) }));
  }, true);
})();
