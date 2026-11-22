// Hello, Linnea — a 10-second example film written as code.
//
// Linnea plays this page live: window.renderOut(t) draws second t on the 1080x1920 canvas, window.READY says
// the assets are in, window.VH_SCENES names the parts of the timeline, audio/mix.wav is the sound.
//
// The rule that makes it editable: every visible thing below is drawn inside its own el(...) or KE.puppet(...)
// with an English id and label. That is why each one shows in Linnea's Layers panel and on its timeline, and
// you can click it on the frame to move, resize, turn, re-type, recolour, hide or retime it, with no code.
// Your edits are kept in tweaks.json, beside this file, and survive when Claude rewrites the film.
// The progress bar at the bottom is a layer added by hand in the app (tweaks.json items2d).
(function () {
  'use strict';
  const cv = document.getElementById('c'), g = cv.getContext('2d');
  const W = cv.width, H = cv.height, DUR = 10;

  // ---- helpers ----
  const clamp = (x, a = 0, b = 1) => Math.max(a, Math.min(b, x));
  const prog = (t, a, b) => clamp((t - a) / (b - a));
  const ease = {
    out: x => 1 - Math.pow(1 - clamp(x), 3),
    inOut: x => (x = clamp(x)) < 0.5 ? 4 * x * x * x : 1 - Math.pow(-2 * x + 2, 3) / 2,
    back: x => { x = clamp(x); const c = 1.7; return 1 + (c + 1) * Math.pow(x - 1, 3) + c * Math.pow(x - 1, 2); },
  };
  const FONT = '"Segoe UI", system-ui, sans-serif';
  function text(str, x, y, { px = 96, weight = 800, color = '#fff', alpha = 1, rise = 0, family = FONT } = {}) {
    g.save(); g.globalAlpha *= alpha; g.fillStyle = color; g.font = `${weight} ${Math.round(px)}px ${family}`;
    g.textAlign = 'center'; g.textBaseline = 'middle'; g.fillText(str, x, y + rise); g.restore();
  }
  function card(x, y, w, h, r, color, alpha = 1) {
    g.save(); g.globalAlpha *= alpha; g.fillStyle = color; g.beginPath(); g.roundRect(x, y, w, h, r); g.fill(); g.restore();
  }
  // one element: what the person sees as one thing, with an id, a label and the pivot it turns and scales around
  const el = (id, t, def, draw) => window.KE ? window.KE.group(g, id, t, def, draw)
    : (t >= (def.t0 ?? -Infinity) && t < (def.t1 ?? Infinity) ? draw(def, t) : undefined);
  const puppet = (id, t, def) => window.KE && window.KE.puppet(g, id, t, def);

  const SCENES = [
    { name: 'Hello', t0: 0, t1: 3.4 },
    { name: 'Edit by hand', t0: 3.4, t1: 7 },
    { name: 'Your turn', t0: 7, t1: DUR },
  ];
  window.VH_SCENES = SCENES;

  // ---- 1. Hello: the twinflower Linnea is named after grows, the title types itself ----
  function hello(t) {
    g.fillStyle = '#f6f1e7'; g.fillRect(0, 0, W, H);   // the background may stay outside: it fills the frame
    const cx = W / 2, base = H * 0.5;
    el('flower', t, { label: 'Linnea flower', x: cx, y: base - 200, t0: 0, t1: 3.4, stem: '#5d6b4e', petal: '#e7a6b6' }, p => {
      const grow = ease.out(prog(t, 0.1, 1.1)), bloom = ease.back(prog(t, 0.9, 1.6));
      g.save(); g.lineCap = 'round'; g.lineWidth = 14; g.strokeStyle = p.stem;
      g.beginPath(); g.moveTo(cx, base); g.lineTo(cx, base - 260 * grow); g.stroke();
      for (const side of [-1, 1]) {
        if (grow < 0.6) continue;
        const k = clamp((grow - 0.6) / 0.4);
        g.beginPath(); g.moveTo(cx, base - 260);
        g.bezierCurveTo(cx + side * 30, base - 360, cx + side * 150, base - 380 * k - (1 - k) * 260, cx + side * 150 * k, base - 290);
        g.stroke();
        // a drooping bell at the end of each arch
        g.save(); g.translate(cx + side * 150 * k, base - 280); g.scale(bloom, bloom);
        g.fillStyle = p.petal; g.beginPath(); g.arc(0, 30, 52, Math.PI, 0); g.closePath(); g.fill();
        g.fillStyle = 'rgba(0,0,0,0.08)'; g.beginPath(); g.ellipse(0, 30, 52, 12, 0, 0, Math.PI); g.fill();
        g.restore();
      }
      g.restore();
    });
    el('title', t, { label: 'Title', x: cx, y: H * 0.6, t0: 0.9, t1: 3.4, text: 'Hello, Linnea', color: '#1d1d1f' },
      p => text(p.text.slice(0, Math.ceil(prog(t, 1.0, 2.2) * p.text.length)), cx, H * 0.6, { px: 118, color: p.color }));
    el('subtitle', t, { label: 'Subtitle', x: cx, y: H * 0.67, t0: 1.8, t1: 3.4, text: 'A film written as code', color: '#6f6f75' },
      p => text(p.text, cx, H * 0.67, { px: 52, weight: 500, color: p.color, alpha: ease.out(prog(t, 2.0, 2.6)) }));
  }

  // ---- 2. Edit by hand: a stick figure (a puppet from the library) points at what you can do ----
  function editByHand(t, s) {
    g.fillStyle = '#17171a'; g.fillRect(0, 0, W, H);
    puppet('presenter', t, { use: 'stickman', label: 'Presenter', x: W * 0.24, y: H * 0.7, scale: 1.1, t0: s.t0, t1: s.t1,
      states: [{ state: 'wave', t0: 0 }, { state: 'point', t0: 1.1, blend: 0.2 }] });
    const lines = ['Click anything', 'Drag, retype, retime', 'Claude keeps your edits'];
    lines.forEach((line, i) => {
      const a = s.t0 + 0.9 + i * 0.45, k = ease.back(prog(t, a, a + 0.5)), y = H * (0.3 + i * 0.12);
      el('card-' + (i + 1), t, { label: 'Card ' + (i + 1), x: W * 0.66, y, t0: s.t0, t1: s.t1, text: line, color: i === 1 ? '#17171a' : '#ffffff', bg: ['#2a2a30', '#e7a6b6', '#2a2a30'][i] }, p => {
        card(W * 0.4, y - 70, W * 0.52, 140, 28, p.bg, k);
        text(p.text, W * 0.66, y, { px: 46, weight: 700, color: p.color, alpha: k, rise: (1 - k) * 30 });
      });
    });
  }

  // ---- 3. Your turn: confetti (a puppet) behind the end title ----
  function yourTurn(t, s) {
    g.fillStyle = '#121214'; g.fillRect(0, 0, W, H);
    puppet('confetti', t, { use: 'confetti', label: 'Confetti', x: W / 2, y: H / 2, scale: H / 600, t0: s.t0, t1: s.t1, params: { count: 120 } });
    const k = ease.back(prog(t, s.t0 + 0.2, s.t0 + 0.8));
    el('end-title', t, { label: 'End title', x: W / 2, y: H * 0.46, t0: s.t0, t1: s.t1, text: 'Your turn', color: '#ffffff' }, p => {
      g.save(); g.translate(W / 2, H * 0.46); g.scale(0.6 + 0.4 * k, 0.6 + 0.4 * k); g.translate(-W / 2, -H * 0.46);
      text(p.text, W / 2, H * 0.46, { px: 140, weight: 900, color: p.color, alpha: clamp(k) });
      g.restore();
    });
    el('end-sub', t, { label: 'End line', x: W / 2, y: H * 0.54, t0: s.t0 + 0.6, t1: s.t1, text: 'Ask Claude for a film, then finish it by hand', color: '#e7a6b6' },
      p => text(p.text, W / 2, H * 0.54, { px: 44, weight: 600, color: p.color, alpha: ease.out(prog(t, s.t0 + 0.7, s.t0 + 1.3)) }));
  }

  window.renderOut = function (t) {
    t = clamp(+t || 0, 0, DUR);
    g.setTransform(1, 0, 0, 1, 0, 0);
    g.clearRect(0, 0, W, H);
    const s = SCENES.find(x => t < x.t1) || SCENES[SCENES.length - 1];
    if (s.name === 'Hello') hello(t, s); else if (s.name === 'Edit by hand') editByHand(t, s); else yourTurn(t, s);
  };

  (async () => {
    await document.fonts.ready;
    window.renderOut(0);
    window.READY = true;
  })();
})();
