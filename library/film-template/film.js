// __TITLE__ — a film written as code, for Linnea.
//
//   window.renderOut(t)  draws second t on the fixed-size canvas #c (__W__x__H__). It may return a promise
//                        (for example to seek a <video>); Linnea waits for it, in the preview and in renders.
//   window.READY = true  once every asset is loaded (images, fonts, videos).
//   window.VH_SCENES     [{name, t0, t1}] — shown on Linnea's timeline, and in every note.
//   audio/mix.wav        the film's sound; the preview plays it and renders mux it.
//
// RULE 1, never skipped: every visible thing (title, text, card, shape, drawing, character, animal, prop) is
// drawn inside its own el(id, t, {label, ...}, draw) below (or KE.text / KE.puppet), with a readable English id
// and label. Anything drawn with raw canvas calls outside el() shows in the preview but NOT in Linnea's Layers
// panel or timeline, so the person cannot select, move or retime it; hub_live lists it under
// checks.not_editable. Only a background that fills the frame may stay outside.
//
// RULE 2: the picture is a pure function of t. No Date.now(), no state carried from frame to frame,
// no unseeded Math.random() (use rand(seed)). Then seeking, frame stepping, notes and partial re-renders
// (only the 2-second chunks a fix touched) all give the same picture.
(function () {
  'use strict';
  const cv = document.getElementById('c'), g = cv.getContext('2d');
  const W = cv.width, H = cv.height, DUR = __DUR__, FPS = __FPS__;

  // ---- helpers -------------------------------------------------------------------------------------
  const clamp = (x, a = 0, b = 1) => Math.max(a, Math.min(b, x));
  const lerp = (a, b, k) => a + (b - a) * k;
  const prog = (t, a, b) => clamp((t - a) / (b - a));                       // 0..1 between seconds a and b
  const ease = { out: x => 1 - Math.pow(1 - clamp(x), 3), inOut: x => (x = clamp(x)) < 0.5 ? 4 * x * x * x : 1 - Math.pow(-2 * x + 2, 3) / 2,
                 back: x => { x = clamp(x); const c = 1.7; return 1 + (c + 1) * Math.pow(x - 1, 3) + c * Math.pow(x - 1, 2); } };
  const rand = s => { s = Math.sin(s * 127.1 + 311.7) * 43758.5453; return s - Math.floor(s); };   // seeded, 0..1
  const font = (px, weight = 700, family = '"Segoe UI", system-ui, sans-serif') => `${weight} ${Math.round(px)}px ${family}`;
  function text(str, x, y, { px = 96, weight = 800, color = '#fff', align = 'center', alpha = 1, rise = 0, family } = {}) {
    g.save(); g.globalAlpha = alpha; g.fillStyle = color; g.font = font(px, weight, family); g.textAlign = align; g.textBaseline = 'middle';
    g.fillText(str, x, y + rise); g.restore();
  }
  function rect(x, y, w, h, r, color, alpha = 1) {
    g.save(); g.globalAlpha = alpha; g.fillStyle = color; g.beginPath(); g.roundRect(x, y, w, h, r); g.fill(); g.restore();
  }
  // Editable elements. Draw every title, card, character and object through el(): the user can then click it
  // in Linnea and move, resize, turn, re-type, recolour, hide or retime it without touching this code
  // (linnea2d.js, KE). def = {label, x, y (pivot), t0, t1, text, ...}; draw(p, tt) reads p (def + the
  // user's edits, so p.text / p.color are what the user typed) and animates with tt.
  const el = (id, t, def, draw) => window.KE ? window.KE.group(g, id, t, def, draw)
    : (t >= (def.t0 ?? -Infinity) && t < (def.t1 ?? Infinity) ? draw(def, t) : undefined);
  // Reusable drawings come from the puppet library (puppet_list, then puppet_link copies one into puppets/):
  //   window.KE.puppet(g, 'pencil-1', t, { use: 'pencil', x: W / 2, y: H / 2, scale: 1, t0: 1, t1: 6,
  //                                        params: { body: '#f2c14e' }, states: [{ state: 'write', t0: 1.5 }] });
  // x, y is where its centre goes; state times count from the puppet's own t0. The user can move it, change
  // its params and re-time its states in Linnea like any other element.
  // Pictures and footage: load before READY. A <video> is seeked to the exact time each frame.
  const loadImage = src => new Promise((ok, no) => { const i = new Image(); i.onload = () => ok(i); i.onerror = () => no(new Error('cannot load ' + src)); i.src = src; });
  const loadVideo = src => new Promise((ok, no) => { const v = document.createElement('video'); v.muted = true; v.preload = 'auto'; v.src = src;
    v.onloadeddata = () => ok(v); v.onerror = () => no(new Error('cannot load ' + src)); });
  const seek = (v, t) => new Promise(ok => { const tt = clamp(t, 0, v.duration - 0.001); if (Math.abs(v.currentTime - tt) < 0.0005) return ok();
    v.onseeked = () => ok(); v.currentTime = tt; });
  function cover(img, iw, ih, x = 0, y = 0, w = W, h = H, zoom = 1) {        // draw like CSS object-fit: cover
    const s = Math.max(w / iw, h / ih) * zoom, dw = iw * s, dh = ih * s;
    g.drawImage(img, x + (w - dw) / 2, y + (h - dh) / 2, dw, dh);
  }

  // ---- the film ------------------------------------------------------------------------------------
  // PLACEHOLDERS: the three scenes below only show how el() works. Replace them with the film's own scenes and
  // delete every placeholder element you do not use (hub_live lists the ones left under checks.leftovers), so
  // the person's Layers panel holds only what the film really shows.
  const SCENES = [
    { name: 'Hook', t0: 0, t1: DUR * 0.45 },
    { name: 'Point', t0: DUR * 0.45, t1: DUR * 0.8 },
    { name: 'End', t0: DUR * 0.8, t1: DUR },
  ];
  window.VH_SCENES = SCENES;
  // const clip = await loadVideo('footage/talk.mp4');   (inside the loader below)

  function hook(t) {
    g.fillStyle = '#121212'; g.fillRect(0, 0, W, H);
    const k = ease.out(prog(t, 0.1, 0.9));
    el('hook-rule', t, { label: 'Line', x: W / 2, y: H * 0.36 }, () => rect(W * 0.08, H * 0.36, W * 0.84 * k, 10, 5, '#e9b84a'));
    el('hook-title', t, { label: 'Title', x: W / 2, y: H * 0.44, text: 'Start here', color: '#fff' },
      p => text(p.text, W / 2, H * 0.44, { px: W * 0.11, color: p.color, alpha: k, rise: (1 - k) * 40 }));
    el('hook-sub', t, { label: 'Subtitle', x: W / 2, y: H * 0.52, text: 'edit film.js, the preview reloads', color: '#a4a4a4' },
      p => text(p.text, W / 2, H * 0.52, { px: W * 0.04, weight: 500, color: p.color, alpha: ease.out(prog(t, 0.6, 1.4)) }));
  }
  function point(t, s) {
    g.fillStyle = '#f3f1ec'; g.fillRect(0, 0, W, H);
    for (let i = 0; i < 3; i++) {
      const k = ease.back(prog(t, s.t0 + 0.15 + i * 0.25, s.t0 + 0.65 + i * 0.25));
      const y = H * (0.385 + i * 0.12);
      el('card-' + i, t, { label: 'Card ' + (i + 1), x: W / 2, y, text: ['One idea', 'per scene', 'on the word'][i], color: i === 1 ? '#1a1a1a' : '#fff' }, p => {
        rect(W * 0.12, H * (0.34 + i * 0.12), W * 0.76, H * 0.09, 24, ['#1a1a1a', '#e9b84a', '#1a1a1a'][i], k);
        text(p.text, W / 2, y, { px: W * 0.065, color: p.color, alpha: k });
      });
    }
  }
  function end(t, s) {
    g.fillStyle = '#121212'; g.fillRect(0, 0, W, H);
    const k = ease.inOut(prog(t, s.t0, s.t0 + 0.6));
    el('end-title', t, { label: 'End title', x: W / 2, y: H / 2, text: '__TITLE__', color: '#fff' }, p => text(p.text, W / 2, H / 2, { px: W * 0.08, color: p.color, alpha: k }));
  }

  window.renderOut = function (t) {
    t = clamp(+t || 0, 0, DUR);
    g.setTransform(1, 0, 0, 1, 0, 0);
    g.clearRect(0, 0, W, H);
    const s = SCENES.find(x => t < x.t1) || SCENES[SCENES.length - 1];
    if (s.name === 'Hook') hook(t, s); else if (s.name === 'Point') point(t, s); else end(t, s);
    // async example: return (async () => { await seek(clip, t - s.t0); cover(clip, clip.videoWidth, clip.videoHeight); })();
  };

  (async () => {
    await document.fonts.ready;
    window.renderOut(0);
    window.READY = true;
  })();
})();
