// linnea2d.js: draws the layers you edit in Linnea (tweaks.json items2d) over the page, after renderOut(t).
// The same picture in the live preview and in renders. Linnea edits them live through window.VH2D.set(items).
//
// A layer: {id, kind, t0, t1, x, y, w, h, rot, scale, opacity, fade, anim, parent, keys: [{t, x, y, w, h, rot,
// scale, opacity, ease}], ...kind fields}. Keys animate any of those properties between their times (a rig:
// a child with `parent` moves and turns with it). Kinds: text, image, rect, counter, marker, arrow, phone,
// lower (lower third), progress.
(function () {
  'use strict';
  let items = [];
  let edits = {};       // what the frame uses: the user's edits with the language's texts laid over them
  let userEdits = {};   // the user's own edits (tweaks.json `edits`)
  // Languages: tweaks.json `langs` {de: {name, texts: {elementId: text}, audio}}. ?lang=de in the page's
  // address (renders) or KE.setLang (the app) picks one; the film's own code can read window.VH_LANG.
  let langs = {};
  let lang = '';
  try { lang = new URLSearchParams(location.search).get('lang') || ''; } catch (e) { /* file:// without a query */ }
  function relang() {
    edits = Object.assign({}, userEdits);
    const tr = (lang && langs[lang] && langs[lang].texts) || {};
    for (const id of Object.keys(tr)) {
      if (typeof tr[id] === 'string' && tr[id] !== '') edits[id] = Object.assign({}, userEdits[id] || {}, { text: tr[id] });
    }
    window.VH_LANG = lang || null;
  }
  let kinds = {};   // puppet name -> 'ppet' | 'rppet' (tweaks.json puppets, kept by puppet_link)
  let MOUTH = null; // {fps, v: [0..1]}: how open a talking puppet's mouth is (audio/mouth.json, from hub_lipsync)
  const imgs = {};
  let filmReady = window.READY === true, own = false;
  try {
    const x = new XMLHttpRequest();
    x.open('GET', 'tweaks.json?' + Math.random(), false);
    x.send();
    if (x.status === 200 || (x.status === 0 && x.responseText)) { const tw = JSON.parse(x.responseText); items = tw.items2d || []; userEdits = tw.edits || {}; kinds = tw.puppets || {}; langs = tw.langs || {};
      // asked for only when hub_lipsync made it: a missing file would show up as an error in every check
      if (tw.mouth) {
        const m = new XMLHttpRequest();
        m.open('GET', tw.mouth + '?' + Math.random(), false);
        m.send();
        if (m.status === 200 || (m.status === 0 && m.responseText)) MOUTH = JSON.parse(m.responseText);
      }
    }
  } catch (e) { /* no tweaks yet */ }
  relang();
  const load = list => Promise.all(list.filter(i => i.src && !imgs[i.src]).map(i => new Promise(ok => {
    const im = new Image(); im.onload = () => { imgs[i.src] = im; ok(); }; im.onerror = () => ok(); im.src = i.src;
  })));
  let pend = 0;   // puppet pictures still loading
  Object.defineProperty(window, 'READY', { configurable: true, get: () => filmReady && own && pend === 0, set: v => { filmReady = !!v; } });
  load(items).then(() => { own = true; });

  const clamp = (x, a, b) => Math.max(a, Math.min(b, x));
  const EASE = {
    linear: k => k,
    out: k => 1 - Math.pow(1 - k, 3),
    in: k => k * k * k,
    inOut: k => k < 0.5 ? 4 * k * k * k : 1 - Math.pow(-2 * k + 2, 3) / 2,
    back: k => { const c = 1.7; return 1 + (c + 1) * Math.pow(k - 1, 3) + c * Math.pow(k - 1, 2); },
  };
  const PROPS = ['x', 'y', 'w', 'h', 'rot', 'scale', 'opacity'];
  // a point k (0..1) of the way along a polyline [[x, y], ...], by length, and the heading there
  function alongPath(P, k) {
    const L = [];
    let tot = 0;
    for (let i = 1; i < P.length; i++) { const d = Math.hypot(P[i][0] - P[i - 1][0], P[i][1] - P[i - 1][1]); L.push(d); tot += d; }
    let want = k * tot, i = 0;
    while (i < L.length - 1 && want > L[i]) { want -= L[i]; i++; }
    const a = P[i], b = P[i + 1], f = L[i] > 0 ? clamp(want / L[i], 0, 1) : 0;
    return { x: a[0] + (b[0] - a[0]) * f, y: a[1] + (b[1] - a[1]) * f, a: Math.atan2(b[1] - a[1], b[0] - a[0]) };
  }
  // The layer's state at t: base values, then its keyframes, then its parent's motion.
  function state(it, t, depth = 0) {
    const s = { x: it.x || 0, y: it.y || 0, w: it.w || 200, h: it.h || 100, rot: it.rot || 0, scale: it.scale ?? 1, opacity: it.opacity ?? 1 };
    const keys = (it.keys || []).slice().sort((a, b) => a.t - b.t);
    for (const p of PROPS) {
      const ks = keys.filter(k => k[p] !== undefined);
      if (!ks.length) continue;
      if (t <= ks[0].t) { s[p] = ks[0][p]; continue; }
      if (t >= ks[ks.length - 1].t) { s[p] = ks[ks.length - 1][p]; continue; }
      for (let i = 0; i < ks.length - 1; i++) {
        const a = ks[i], b = ks[i + 1];
        if (t >= a.t && t <= b.t) {
          const k = (EASE[b.ease || 'inOut'] || EASE.inOut)((t - a.t) / Math.max(1e-6, b.t - a.t));
          s[p] = a[p] + (b[p] - a[p]) * k;
          break;
        }
      }
    }
    // a motion path: the layer's centre travels along it from t0 to t1
    if (Array.isArray(it.path) && it.path.length >= 2) {
      const span = Math.max(0.01, (it.t1 ?? t) - (it.t0 ?? 0));
      const pt = alongPath(it.path, (EASE[it.pathEase || 'inOut'] || EASE.inOut)(clamp((t - (it.t0 || 0)) / span, 0, 1)));
      s.x = pt.x - s.w / 2; s.y = pt.y - s.h / 2;
      if (it.pathTurn) s.rot += pt.a * 180 / Math.PI;
    }
    if (it.parent && depth < 8) {
      const par = items.find(p => p.id === it.parent);
      if (par) {
        const now = state(par, t, depth + 1), base = { x: par.x || 0, y: par.y || 0, rot: par.rot || 0 };
        const pcx = now.x + now.w / 2, pcy = now.y + now.h / 2, bcx = base.x + (par.w || 200) / 2, bcy = base.y + (par.h || 100) / 2;
        const dr = (now.rot - base.rot) * Math.PI / 180;
        const ox = s.x + s.w / 2 - bcx, oy = s.y + s.h / 2 - bcy;
        const rx = ox * Math.cos(dr) - oy * Math.sin(dr), ry = ox * Math.sin(dr) + oy * Math.cos(dr);
        s.x = pcx + rx - s.w / 2; s.y = pcy + ry - s.h / 2; s.rot += now.rot - base.rot;
      }
    }
    return s;
  }
  window.VH2D_state = state;

  const target = () => [...document.querySelectorAll('canvas')].sort((a, b) => b.width * b.height - a.width * a.height)[0];
  const FONT = '"Segoe UI", system-ui, sans-serif';
  function textBlock(g, it, w, h, str, color) {
    const size = it.size || 72;
    g.font = `${it.italic ? 'italic ' : ''}${it.weight || 800} ${size}px ${it.font || FONT}`;
    g.textAlign = it.align || 'center'; g.textBaseline = 'middle';
    const tx = g.textAlign === 'left' ? (it.pad ?? size * 0.3) : g.textAlign === 'right' ? w - (it.pad ?? size * 0.3) : w / 2;
    const lines = String(str ?? '').split('\n'), lh = size * 1.15;
    lines.forEach((ln, i) => {
      const ty = h / 2 + (i - (lines.length - 1) / 2) * lh;
      if (it.stroke) { g.lineJoin = 'round'; g.lineWidth = it.strokeWidth || size * 0.12; g.strokeStyle = it.stroke; g.strokeText(ln, tx, ty); }
      g.fillStyle = color || it.color || '#fff'; g.fillText(ln, tx, ty);
    });
  }
  function draw(t) {
    const cv = target(); if (!cv) return;
    const g = cv.getContext('2d'); if (!g) return;
    inGroup++;
    try {
    for (const it of items) {
      if (t < it.t0 || t >= it.t1 || it.hidden) continue;
      const st = state(it, t);
      const fade = it.fade ?? 0.25;
      const k = fade > 0 ? clamp(Math.min((t - it.t0) / fade, (it.t1 - t) / fade), 0, 1) : 1;
      const e = 1 - Math.pow(1 - k, 3);
      const a = st.opacity * (it.anim && it.anim !== 'fade' ? Math.min(1, k * 2) : k);
      if (a <= 0) continue;
      const { x, y, w, h } = st;
      const life = clamp((t - it.t0) / Math.max(0.01, (it.reveal ?? 0.8)), 0, 1);  // 0..1 for draw-on kinds
      g.save();
      g.setTransform(1, 0, 0, 1, 0, 0);
      g.globalAlpha = a;
      g.translate(x + w / 2, y + h / 2);
      if (it.anim === 'slide-up') g.translate(0, (1 - e) * h * 0.6);
      else if (it.anim === 'slide-left') g.translate((1 - e) * w * 0.4, 0);
      else if (it.anim === 'pop') { const s = k < 1 ? 0.6 + 0.4 * e + Math.sin(e * Math.PI) * 0.08 : 1; g.scale(s, s); }
      if (st.rot) g.rotate(st.rot * Math.PI / 180);
      if (st.scale !== 1) g.scale(st.scale, st.scale);
      g.translate(-w / 2, -h / 2);
      const bg = it.bg || (it.kind === 'rect' ? (it.color || '#e9b84a') : null);
      if (bg && it.kind !== 'lower' && it.kind !== 'progress') { g.fillStyle = bg; g.beginPath(); g.roundRect(0, 0, w, h, it.radius || 0); g.fill(); }
      switch (it.kind) {
        case 'puppet': drawPuppet(g, it.puppet, it.params, t - it.t0, it.states, w, h); break;
        case 'image':
          if (imgs[it.src]) g.drawImage(imgs[it.src], 0, 0, w, h);
          break;
        case 'text':
          textBlock(g, it, w, h, it.text);
          break;
        case 'counter': {
          const v = (it.from ?? 0) + ((it.to ?? 100) - (it.from ?? 0)) * (1 - Math.pow(1 - life, 3));
          const n = (it.decimals ? v.toFixed(it.decimals) : Math.round(v).toLocaleString('tr-TR'));
          textBlock(g, it, w, h, `${it.prefix || ''}${n}${it.suffix || ''}`);
          break;
        }
        case 'marker': {
          // handwriting: the text revealed left to right as if written
          g.save(); g.beginPath(); g.rect(0, 0, w * life, h); g.clip();
          textBlock(g, { ...it, font: it.font || '"Caveat", "Segoe Print", "Ink Free", cursive', weight: it.weight || 700 }, w, h, it.text);
          g.restore();
          break;
        }
        case 'arrow': {
          const ex = w * life, ey = h * life;
          g.strokeStyle = it.color || '#e9b84a'; g.lineWidth = it.width || 10; g.lineCap = 'round';
          g.beginPath(); g.moveTo(0, 0); g.lineTo(ex, ey); g.stroke();
          if (life > 0.95) {
            const ang = Math.atan2(h, w), L = (it.width || 10) * 4;
            g.beginPath(); g.moveTo(ex, ey); g.lineTo(ex - L * Math.cos(ang - 0.45), ey - L * Math.sin(ang - 0.45));
            g.moveTo(ex, ey); g.lineTo(ex - L * Math.cos(ang + 0.45), ey - L * Math.sin(ang + 0.45)); g.stroke();
          }
          break;
        }
        case 'phone': {
          const r = w * 0.12;
          g.fillStyle = it.frame || '#0d0d0d'; g.beginPath(); g.roundRect(0, 0, w, h, r); g.fill();
          g.save(); g.beginPath(); g.roundRect(w * 0.04, w * 0.04, w * 0.92, h - w * 0.08, r * 0.8); g.clip();
          g.fillStyle = it.screen || '#f2f2f2'; g.fillRect(0, 0, w, h);
          if (imgs[it.src]) g.drawImage(imgs[it.src], w * 0.04, w * 0.04, w * 0.92, h - w * 0.08);
          if (it.text) {
            // a notification that slides in from the top
            const ny = -h * 0.15 + h * 0.2 * (1 - Math.pow(1 - life, 3));
            g.fillStyle = 'rgba(255,255,255,0.94)'; g.beginPath(); g.roundRect(w * 0.08, ny, w * 0.84, h * 0.11, w * 0.05); g.fill();
            g.fillStyle = '#111'; g.font = `600 ${Math.round(w * 0.05)}px ${FONT}`; g.textAlign = 'left'; g.textBaseline = 'middle';
            g.fillText(it.title || 'Linnea', w * 0.13, ny + h * 0.035);
            g.font = `400 ${Math.round(w * 0.045)}px ${FONT}`; g.fillText(it.text, w * 0.13, ny + h * 0.075);
          }
          g.restore();
          g.fillStyle = it.frame || '#0d0d0d'; g.beginPath(); g.roundRect(w * 0.36, w * 0.06, w * 0.28, w * 0.07, w * 0.035); g.fill();
          break;
        }
        case 'lower': {
          const bw = w * (1 - Math.pow(1 - life, 3));
          g.fillStyle = it.bg || '#121212'; g.beginPath(); g.roundRect(0, 0, bw, h, it.radius ?? 12); g.fill();
          g.fillStyle = it.accent || '#e9b84a'; g.fillRect(0, 0, Math.min(bw, h * 0.08), h);
          g.save(); g.beginPath(); g.rect(0, 0, bw, h); g.clip();
          g.textAlign = 'left'; g.textBaseline = 'middle';
          g.fillStyle = it.color || '#fff'; g.font = `800 ${Math.round(h * 0.34)}px ${it.font || FONT}`; g.fillText(it.text || '', h * 0.3, h * (it.sub ? 0.36 : 0.5));
          if (it.sub) { g.fillStyle = it.subColor || '#b0b0b0'; g.font = `500 ${Math.round(h * 0.22)}px ${it.font || FONT}`; g.fillText(it.sub, h * 0.3, h * 0.72); }
          g.restore();
          break;
        }
        case 'progress': {
          g.fillStyle = it.bg || 'rgba(255,255,255,0.15)'; g.beginPath(); g.roundRect(0, 0, w, h, h / 2); g.fill();
          g.fillStyle = it.color || '#e9b84a'; g.beginPath(); g.roundRect(0, 0, w * (it.value ?? 1) * life, h, h / 2); g.fill();
          break;
        }
      }
      g.restore();
    }
    } finally { inGroup--; }
  }

  // ======================= KE: the editable engine =======================
  // Anything a film draws through KE (or any text it draws with fillText) shows up in Linnea as an
  // element the user can select, move, resize, turn, fade, retime and re-type, without touching code.
  // The user's changes live in tweaks.json `edits` ({id: {dx, dy, scale, rot, opacity, hidden, t0, t1,
  // text, color, size, keys: [{t, dx, dy, scale, rot, opacity, ease}]}}) and are applied here, in the
  // live preview and in renders alike.
  // ---- puppets: library drawings (.ppet) and animated ones with named states (.rppet), in puppets/ ----------
  // file: {linnea: 'ppet/1', label, size: [w, h], params: {key: {type, default}}, states: {name: {dur, loop}},
  //        initial, assets: {key: src}, draw: "function (g, p, t, s, u) {...}"} painting in the box 0..size.
  const PUPPETS = {};
  const MISSING = {};   // name -> when it was last looked for and not found
  const rand =n => { n = Math.sin(n * 127.1 + 311.7) * 43758.5453; return n - Math.floor(n); };
  function puppetDef(name) {
    if (!name) return null;
    if (PUPPETS[name]) return PUPPETS[name];
    // a missing one is looked for again (it may get linked), but not on every frame: each look is two
    // failed requests, which would flood the console and slow a render to a crawl
    if (MISSING[name] && Date.now() - MISSING[name] < 2000) return null;
    let def = null;
    for (const ext of kinds[name] ? [kinds[name]] : ['rppet', 'ppet']) {
      try {
        const x = new XMLHttpRequest();
        x.open('GET', 'puppets/' + name + '.' + ext + '?' + Math.random(), false);
        x.send();
        if (x.status === 200 || (x.status === 0 && x.responseText)) { def = JSON.parse(x.responseText); def.kind = ext; break; }
      } catch (e) { /* try the other kind */ }
    }
    if (def) {
      try { def.fn = new Function('return (' + def.draw + ')')(); } catch (e) { console.error('puppet ' + name + ': ' + e.message); def.fn = null; }
      if (typeof def.fn !== 'function') def.fn = null;
      [def.w, def.h] = def.size || [400, 400];
      def.img = {};
      for (const [k, src] of Object.entries(def.assets || {})) {
        if (typeof src !== 'string' || !src) continue;
        pend++;
        const im = new Image();
        let done = false;
        im.onload = im.onerror = () => { if (!done) { done = true; pend--; settled(); } };
        im.src = /^(data:|blob:|https?:|\/)/.test(src) ? src : 'puppets/' + src;
        def.img[k] = im;
      }
    }
    if (!def) MISSING[name] = Date.now();
    return (PUPPETS[name] = def);
  }
  // Pictures of a puppet first drawn mid-film load after that frame was painted: whoever asked for the frame
  // (the preview, a render) waits here and gets it drawn again, so no frame goes out with a hole in it.
  let waiters = [];
  function settled() { if (pend === 0 && waiters.length) { const w = waiters; waiters = []; w.forEach(f => f()); } }
  const whenSettled = () => new Promise(ok => { if (pend === 0) ok(); else { waiters.push(ok); setTimeout(ok, 8000); } });
  const complained = new Set();
  function complain(name, e) {
    const key = name + ': ' + (e && e.message || e);
    if (complained.has(key)) return;
    complained.add(key);
    console.error('puppet ' + key);
  }
  // which state plays at puppet time t: states = [{state, t0, blend}] in the puppet's own seconds. With a
  // blend (seconds; the state's own default otherwise) the one before fades out under it: s.prev, s.mix.
  function stateAt(def, states, t) {
    const S = def.states || {};
    let cur = { state: def.initial || Object.keys(S)[0] || 'idle', t0: 0 }, prev = null;
    for (const s of (states || []).slice().sort((a, b) => a.t0 - b.t0)) if (t >= s.t0) { prev = cur; cur = s; }
    const one = c => {
      const info = S[c.state] || {}, dur = info.dur || 1, st = Math.max(0, t - (c.t0 || 0));
      return { name: c.state, t: st, k: info.loop ? (st / dur) % 1 : clamp(st / dur, 0, 1), dur };
    };
    const s = one(cur);
    const blend = cur.blend ?? (S[cur.state] || {}).blend ?? 0;
    if (prev && blend > 0 && s.t < blend) { s.prev = one(prev); s.mix = s.t / blend; }
    return s;
  }
  // talking and blinking, as pure functions of the film's time
  function mouthAt(t) {
    if (!MOUTH || !MOUTH.v || !MOUTH.v.length) return 0;
    const f = Math.max(0, t) * (MOUTH.fps || 30), i = Math.floor(f), a = MOUTH.v[i] || 0, b = MOUTH.v[i + 1] || 0;
    return a + (b - a) * (f - i);
  }
  function blinkAt(t, seed) {
    const P = 4, n = Math.floor(t / P), o = 0.5 + rand(seed * 13.7 + n) * (P - 1.2), d = Math.abs(t - (n * P + o));
    return d < 0.09 ? 1 - d / 0.09 : 0;
  }
  const seedOf = s => { let h = 7; for (const ch of String(s)) h = (h * 31 + ch.charCodeAt(0)) % 9973; return h; };
  function puppetParams(def, over) {
    const p = {};
    for (const [k, v] of Object.entries(def.params || {})) p[k] = v && typeof v === 'object' && 'default' in v ? v.default : v;
    return Object.assign(p, over || {});
  }
  function drawPuppet(g, name, params, t, states, w, h) {
    const def = puppetDef(name);
    if (!def || !def.fn) {
      g.save(); g.strokeStyle = '#7fb0ff'; g.setLineDash([12, 8]); g.lineWidth = 4; g.strokeRect(0, 0, w || 200, h || 200);
      g.fillStyle = '#7fb0ff'; g.font = '600 28px ' + FONT; g.textAlign = 'center'; g.textBaseline = 'middle';
      g.fillText((def ? 'error in ' : 'missing ') + name, (w || 200) / 2, (h || 200) / 2); g.restore();
      return;
    }
    g.save();
    if (w && h) g.scale(w / def.w, h / def.h);
    // the puppet's whole box counts as painted, so its selection box in Linnea never shrinks to a detail
    if (curAcc) devBox(g, 0, 0, def.w, def.h, curAcc);
    // a puppet with mouth / blink params talks with the film's voice and blinks, unless set by hand
    const pp = puppetParams(def, params), dp = def.params || {};
    if ('mouth' in dp && !(params && params.mouth !== undefined)) pp.mouth = mouthAt(curT);
    if ('blink' in dp && !(params && params.blink !== undefined)) pp.blink = blinkAt(curT, seedOf(name));
    const U = { ease: EASE, clamp, rand, img: def.img, W: def.w, H: def.h };
    const one = (st, alpha) => {
      g.save();
      if (alpha !== undefined) g.globalAlpha *= alpha;
      try { def.fn(g, pp, t, st, U); } catch (e) { complain(name, e); }
      g.restore();
    };
    const s = stateAt(def, states, t);
    // a blend without the see-through dip of two half-faded drawings: the outgoing state stays solid while
    // the new one comes up over it, then the new one is solid while the old one fades away on top
    if (s.prev) {
      if (s.mix < 0.5) { one(s.prev); one(s, s.mix * 2); } else { one(s); one(s.prev, (1 - s.mix) * 2); }
    } else one(s);
    g.restore();
  }
  window.KE_PUPPET = name => { const d = puppetDef(name); return d && { label: d.label, kind: d.kind, size: d.size, params: d.params || {}, states: d.states || {}, initial: d.initial, broken: !d.fn }; };
  // everything linked into the film loads before READY (the first frame that needs it must not wait)
  for (const name of Object.keys(kinds)) puppetDef(name);
  for (const it of items) if (it.kind === 'puppet') puppetDef(it.puppet);

  const BOXES = [];
  window.KE_BOXES = BOXES;
  let inGroup = 0;
  const NUM = ['dx', 'dy', 'scale', 'rot', 'opacity'];
  function editsAt(id, t) {
    const e = Object.assign({}, edits[id] || {});
    const keys = (e.keys || []).slice().sort((a, b) => a.t - b.t);
    for (const p of NUM) {
      const ks = keys.filter(k => k[p] !== undefined);
      if (!ks.length) continue;
      if (t <= ks[0].t) { e[p] = ks[0][p]; continue; }
      if (t >= ks[ks.length - 1].t) { e[p] = ks[ks.length - 1][p]; continue; }
      for (let i = 0; i < ks.length - 1; i++) {
        const a = ks[i], b = ks[i + 1];
        if (t >= a.t && t <= b.t) { const k = (EASE[b.ease || 'inOut'] || EASE.inOut)((t - a.t) / Math.max(1e-6, b.t - a.t)); e[p] = a[p] + (b[p] - a[p]) * k; break; }
      }
    }
    return e;
  }
  function devBox(g, x, y, w, h, acc) {
    const m = g.getTransform();
    for (const [px, py] of [[x, y], [x + w, y], [x, y + h], [x + w, y + h]]) {
      const X = m.a * px + m.c * py + m.e, Y = m.b * px + m.d * py + m.f;
      acc.x0 = Math.min(acc.x0, X); acc.y0 = Math.min(acc.y0, Y); acc.x1 = Math.max(acc.x1, X); acc.y1 = Math.max(acc.y1, Y);
    }
  }
  function textBox(g, text, x, y, acc) {
    const w = g.measureText(String(text)).width;
    const size = +((g.font.match(/(\d+(?:\.\d+)?)px/) || [0, 16])[1]);
    const al = g.textAlign, bl = g.textBaseline;
    const x0 = al === 'center' ? x - w / 2 : (al === 'right' || al === 'end') ? x - w : x;
    const y0 = bl === 'middle' ? y - size * 0.6 : bl === 'top' || bl === 'hanging' ? y : bl === 'bottom' || bl === 'ideographic' ? y - size * 1.1 : y - size * 0.85;
    devBox(g, x0, y0, w, size * 1.2, acc);
  }
  // Measure what a draw function paints, in canvas pixels.
  // mute: measure only, paint nothing (a hidden element keeps a clickable box in Linnea, never a pixel)
  const PAINTS = new Set(['fillText', 'strokeText', 'fillRect', 'strokeRect', 'drawImage']);
  let curAcc = null;   // the box being measured, for drawings that know their own extent (puppets)
  function track(g, fn, mute) {
    const acc = { x0: Infinity, y0: Infinity, x1: -Infinity, y1: -Infinity };
    const outer = curAcc;
    const saved = {};
    const wrapM = (name, measure) => { saved[name] = g[name]; g[name] = function () { try { measure.apply(null, arguments); } catch (e) { } return mute && PAINTS.has(name) ? undefined : saved[name].apply(g, arguments); }; };
    if (mute) for (const n of ['fill', 'stroke', 'clearRect', 'putImageData']) { saved[n] = g[n]; g[n] = function () { }; }
    wrapM('fillText', (t, x, y) => textBox(g, t, x, y, acc));
    wrapM('strokeText', (t, x, y) => textBox(g, t, x, y, acc));
    wrapM('fillRect', (x, y, w, h) => devBox(g, x, y, w, h, acc));
    wrapM('strokeRect', (x, y, w, h) => devBox(g, x, y, w, h, acc));
    wrapM('rect', (x, y, w, h) => devBox(g, x, y, w, h, acc));
    wrapM('roundRect', (x, y, w, h) => devBox(g, x, y, w, h, acc));
    wrapM('arc', (x, y, r) => devBox(g, x - r, y - r, 2 * r, 2 * r, acc));
    wrapM('ellipse', (x, y, rx, ry) => devBox(g, x - rx, y - ry, 2 * rx, 2 * ry, acc));
    // paths drawn point by point (characters, icons): every point counts
    wrapM('moveTo', (x, y) => devBox(g, x, y, 0, 0, acc));
    wrapM('lineTo', (x, y) => devBox(g, x, y, 0, 0, acc));
    wrapM('quadraticCurveTo', (cx, cy, x, y) => devBox(g, x, y, 0, 0, acc));
    wrapM('bezierCurveTo', (a, b, c, d, x, y) => devBox(g, x, y, 0, 0, acc));
    wrapM('arcTo', (a, b, x, y) => devBox(g, x, y, 0, 0, acc));
    wrapM('drawImage', function (img) {
      const a = arguments;
      if (a.length >= 9) devBox(g, a[5], a[6], a[7], a[8], acc);
      else if (a.length >= 5) devBox(g, a[1], a[2], a[3], a[4], acc);
      else devBox(g, a[1], a[2], img.width || 0, img.height || 0, acc);
    });
    inGroup++;
    curAcc = acc;
    try { fn(); } finally { curAcc = outer; inGroup--; for (const k in saved) g[k] = saved[k]; }
    if (outer && isFinite(acc.x0)) {   // a group inside a group grows its parent's box too
      outer.x0 = Math.min(outer.x0, acc.x0); outer.y0 = Math.min(outer.y0, acc.y0); outer.x1 = Math.max(outer.x1, acc.x1); outer.y1 = Math.max(outer.y1, acc.y1);
    }
    return isFinite(acc.x0) ? acc : null;
  }
  const KE = {
    /** An editable group: everything `draw(p, tt)` paints moves, scales, turns and retimes together.
     *  def: {label, t0, t1, x, y (the pivot), ...any props the drawing reads from p}. tt is the time to
     *  animate with (shifted when the user retimes the group). Call it every frame; KE hides it outside t0..t1. */
    group(g, id, t, def, draw) {
      const e = editsAt(id, t);
      const t0 = e.t0 ?? def.t0 ?? -Infinity, t1 = e.t1 ?? def.t1 ?? Infinity;
      if (t < t0 || t >= t1) return;
      const tt = (def.t0 !== undefined && e.t0 !== undefined) ? t - e.t0 + def.t0 : t;
      const p = Object.assign({}, def, e);
      const ox = def.ox ?? def.x ?? 0, oy = def.oy ?? def.y ?? 0;
      g.save();
      g.translate(ox + (e.dx || 0), oy + (e.dy || 0));
      if (e.rot) g.rotate(e.rot * Math.PI / 180);
      const s = e.scale ?? 1;
      if (s !== 1) g.scale(s, s);
      g.translate(-ox, -oy);
      if (e.opacity !== undefined) g.globalAlpha *= e.opacity;
      const b = track(g, () => draw(p, tt), !!e.hidden);
      g.restore();
      if (b) BOXES.push({ id, label: def.label || p.text || id, x: b.x0, y: b.y0, w: b.x1 - b.x0, h: b.y1 - b.y0, t0: def.t0, t1: def.t1, kind: p.text !== undefined ? 'text' : 'group', text: p.text, src: def.text, edited: !!userEdits[id], hidden: !!e.hidden });
    },
    /** A library puppet, editable like any element (move, size, turn, retime) plus its params and states.
     *  def: {use: 'pencil', x, y (centre), scale, t0, t1, params: {...}, states: [{state, t0}] (puppet seconds
     *  from t0), label}. The user's edits may change params and states too. */
    puppet(g, id, t, def) {
      const pd = puppetDef(def.use);
      const e = edits[id] || {};
      const sc = def.scale ?? 1, w = (pd ? pd.w : 200) * sc, h = (pd ? pd.h : 200) * sc;
      // x, y place the puppet's pivot: the centre of its box unless the puppet names another point
      const pv = pd && Array.isArray(pd.pivot) && pd.pivot.length === 2 ? pd.pivot : null;
      const px = pv ? pv[0] * sc : w / 2, py = pv ? pv[1] * sc : h / 2;
      const x = def.x ?? 0, y = def.y ?? 0;
      const params = Object.assign({}, def.params, e.params), states = e.states || def.states;
      let pt = null;   // the puppet's own clock at this frame, as it was drawn
      KE.group(g, id, t, Object.assign({ label: (pd && pd.label) || def.use, x, y }, def), (p, tt) => {
        pt = tt - (def.t0 || 0);
        g.save(); g.translate(x - px, y - py);
        drawPuppet(g, def.use, params, pt, states, w, h);
        g.restore();
      });
      const b = BOXES[BOXES.length - 1];
      if (b && b.id === id && pt !== null) { b.kind = 'puppet'; b.puppet = def.use; b.params = params; b.states = states || []; b.pt = pt; }
    },
    /** An editable text with an optional box behind it. def: {text, x, y, size, weight, font, color,
     *  align, stroke, strokeWidth, shadow, bg, border, borderWidth, pad, radius, t0, t1, label}. */
    text(g, id, t, def) {
      KE.group(g, id, t, def, p => {
        const size = p.size || 64;
        g.save();
        g.font = `${p.italic ? 'italic ' : ''}${p.weight || 800} ${size}px ${p.font || FONT}`;
        g.textAlign = p.align || 'center'; g.textBaseline = 'middle';
        const lines = String(p.text ?? '').split('\n'), lh = size * (p.lineHeight || 1.15);
        const w = Math.max(...lines.map(l => g.measureText(l).width)), h = lh * lines.length;
        const pad = p.pad ?? (p.bg || p.border ? size * 0.4 : 0);
        const bx = p.align === 'left' ? p.x - pad : p.align === 'right' ? p.x - w - pad : p.x - w / 2 - pad;
        if (p.bg || p.border) {
          g.beginPath(); g.roundRect(bx, p.y - h / 2 - pad, w + 2 * pad, h + 2 * pad, p.radius ?? 8);
          if (p.bg) { g.fillStyle = p.bg; g.fill(); }
          if (p.border) { g.strokeStyle = p.border; g.lineWidth = p.borderWidth || 4; g.stroke(); }
        }
        if (p.shadow) { g.shadowColor = p.shadow; g.shadowBlur = size * 0.25; }
        lines.forEach((ln, i) => {
          const y = p.y + (i - (lines.length - 1) / 2) * lh;
          if (p.stroke) { g.lineJoin = 'round'; g.lineWidth = p.strokeWidth || size * 0.12; g.strokeStyle = p.stroke; g.strokeText(ln, p.x, y); }
          g.fillStyle = p.color || '#fff'; g.fillText(ln, p.x, y);
        });
        g.restore();
      });
    },
    /** One editable value: KE.v('title', 'color', '#fff') returns the user's choice or the default. */
    v(id, key, def) { const e = edits[id]; return e && e[key] !== undefined ? e[key] : def; },
    /** How open a talking mouth is at film second t (0..1, from audio/mouth.json; 0 without it). */
    mouth: t => mouthAt(t),
    /** 1 while the eyes are shut in a natural blink rhythm (seed: any number, one per character). */
    blink: (t, seed) => blinkAt(t, seed ?? 1),
    setEdits(e) { userEdits = e || {}; relang(); },
    getEdits() { return userEdits; },
    /** Show the film in a language from tweaks.json `langs` ('' = the original). `texts` replaces that
     *  language's texts without a reload (the app passes them while you translate). */
    setLang(code, texts) { lang = code || ''; if (lang && texts) langs[lang] = Object.assign({}, langs[lang], { texts }); relang(); },
    get lang() { return lang; },
  };
  window.KE = KE;

  // Text drawn without KE is still editable: each fillText on the film's canvas becomes an element
  // keyed by its text (move, scale, turn, hide, re-type, recolour). Its box behind it does not move.
  (function () {
    const P = CanvasRenderingContext2D.prototype, ft = P.fillText, st = P.strokeText;
    const main = c => c === target();
    function auto(orig, isFill) {
      return function (text, x, y, mw) {
        if (inGroup || !main(this.canvas)) return orig.apply(this, arguments);
        const id = 'txt:' + String(text).slice(0, 48);
        const e = edits[id] ? editsAt(id, curT) : null;
        if (e && ((e.t0 !== undefined && curT < e.t0) || (e.t1 !== undefined && curT >= e.t1))) return;
        const txt = e && e.text !== undefined ? e.text : text;
        this.save();
        if (e) {
          this.translate(x + (e.dx || 0), y + (e.dy || 0));
          if (e.rot) this.rotate(e.rot * Math.PI / 180);
          if (e.scale && e.scale !== 1) this.scale(e.scale, e.scale);
          this.translate(-x, -y);
          if (e.opacity !== undefined) this.globalAlpha *= e.opacity;
          if (isFill && e.color) this.fillStyle = e.color;
        }
        // hidden: not drawn, still measured below so Linnea can show where it is
        const r = e && e.hidden ? undefined : mw === undefined ? orig.call(this, txt, x, y) : orig.call(this, txt, x, y, mw);
        if (isFill) {
          const acc = { x0: Infinity, y0: Infinity, x1: -Infinity, y1: -Infinity };
          textBox(this, txt, x, y, acc);
          if (isFinite(acc.x0) && !BOXES.some(b => b.id === id)) BOXES.push({ id, label: String(text), x: acc.x0, y: acc.y0, w: acc.x1 - acc.x0, h: acc.y1 - acc.y0, kind: 'auto', text: txt, src: String(text), edited: !!userEdits[id], hidden: !!(e && e.hidden) });
        }
        this.restore();
        return r;
      };
    }
    P.fillText = auto(ft, true);
    P.strokeText = auto(st, false);
  })();
  let curT = 0;

  let wrapped = null;
  function wrap() {
    const orig = window.renderOut;
    if (typeof orig !== 'function' || orig === wrapped) return;
    wrapped = function (t, again) {
      BOXES.length = 0; curT = t;
      const r = orig(t);
      // a puppet first met in this frame may still be loading its pictures: wait, then draw the frame again
      const fin = () => { draw(t); if (pend > 0 && (again || 0) < 3) return whenSettled().then(() => wrapped(t, (again || 0) + 1)); };
      if (r && r.then) return r.then(fin);
      return fin() || r;
    };
    window.renderOut = wrapped;
  }
  wrap();
  let tries = 0; const iv = setInterval(() => { wrap(); if (++tries > 100) clearInterval(iv); }, 50);
  window.VH2D = { get: () => items, set: list => { items = list || []; return load(items); } };
})();
