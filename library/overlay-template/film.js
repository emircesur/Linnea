// The reel's layer page: drawn over the speaker's video, transparent everywhere else.
// Layers you add in Kesit (tweaks.json items2d) are drawn by kesit2d.js. Anything else can be drawn here,
// as a pure function of t (edit seconds of the reel): window.renderOut(t) must clear and redraw.
(function () {
  const cv = document.getElementById('c'), g = cv.getContext('2d');
  window.renderOut = function (t) {
    g.setTransform(1, 0, 0, 1, 0, 0);
    g.clearRect(0, 0, cv.width, cv.height);
  };
  (async () => { await document.fonts.ready; window.renderOut(0); window.READY = true; })();
})();
