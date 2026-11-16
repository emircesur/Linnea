//! Takes -> the edit timeline. Every time in a reel is an ORIGINAL-recording second; this maps it to edit time.
use crate::project::Word;
use crate::util::*;
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct Seg {
    pub a: f64,
    pub b: f64,
    pub hold: f64,
    /// Where this take starts in the edit.
    pub start: f64,
    /// Playback speed (1 = as recorded; 2 = twice as fast).
    pub speed: f64,
}
impl Seg {
    /// Length in the source.
    pub fn dur(&self) -> f64 {
        self.b - self.a
    }
    /// Length in the edit.
    pub fn len(&self) -> f64 {
        (self.b - self.a) / self.speed
    }
}

#[derive(Clone, Debug)]
pub struct EWord {
    pub text: String,
    pub s: f64,
    pub e: f64,
    pub seg: usize,
    pub orig: f64,
}

#[derive(Clone, Debug)]
pub struct Timeline {
    pub segs: Vec<Seg>,
    pub fps: f64,
    pub outro: f64,
    /// End of the last take (with its hold): where the outro starts.
    pub body: f64,
    pub total: f64,
}

impl Timeline {
    pub fn from_reel(reel: &Value, src_dur: f64) -> Timeline {
        let fps = jfd(reel, "fps", 30.0).max(1.0);
        let grid = |t: f64| (t * fps).round() / fps;
        let mut segs = vec![];
        let mut cur = 0.0;
        for t in ja(reel, "takes") {
            let a = grid(clampf(jfd(t, "a", 0.0), 0.0, src_dur));
            let b = grid(clampf(jfd(t, "b", src_dur), 0.0, src_dur));
            if b - a < 1.5 / fps {
                continue;
            }
            let hold = grid(jfd(t, "hold", 0.0).max(0.0));
            let speed = clampf(jfd(t, "speed", 1.0), 0.25, 4.0);
            segs.push(Seg { a, b, hold, start: cur, speed });
            cur += (b - a) / speed + hold;
        }
        if segs.is_empty() {
            let b = grid(src_dur - 0.5 / fps).max(1.0 / fps);
            segs.push(Seg { a: 0.0, b, hold: 0.0, start: 0.0, speed: 1.0 });
            cur = b;
        }
        let outro = grid(jfd(reel, "outro", 0.0).max(0.0));
        Timeline { segs, fps, outro, body: cur, total: cur + outro }
    }

    /// Edit time of an original second, if it survived the cut.
    pub fn to_edit(&self, t: f64) -> Option<f64> {
        self.segs.iter().find(|s| t >= s.a - 1e-6 && t <= s.b + 1e-6).map(|s| s.start + (t - s.a) / s.speed)
    }
    /// Like `to_edit`, but a time inside removed material snaps to the next kept moment.
    pub fn to_edit_snap(&self, t: f64) -> f64 {
        if let Some(e) = self.to_edit(t) {
            return e;
        }
        let mut best: Option<&Seg> = None;
        for s in &self.segs {
            if s.a >= t && best.map(|b| s.a < b.a).unwrap_or(true) {
                best = Some(s);
            }
        }
        best.map(|s| s.start).unwrap_or(self.body)
    }
    /// A reel time spec: a number (original seconds), "end", "outro+N", "hold:<take>+<s>", "edit:<s>".
    pub fn resolve(&self, spec: &Value) -> Option<f64> {
        if let Some(n) = spec.as_f64() {
            return Some(self.to_edit_snap(n));
        }
        let s = spec.as_str()?.trim();
        if s == "end" {
            return Some((self.total - 1.0 / self.fps).max(0.0));
        }
        if let Some(n) = s.strip_prefix("outro") {
            let n: f64 = n.trim_start_matches('+').parse().unwrap_or(0.0);
            return Some(self.body + n);
        }
        if let Some(n) = s.strip_prefix("edit:") {
            return n.parse().ok();
        }
        if let Some(r) = s.strip_prefix("hold:") {
            let mut it = r.split('+');
            let i: usize = it.next()?.parse().ok()?;
            let off: f64 = it.next().and_then(|x| x.parse().ok()).unwrap_or(0.0);
            let seg = self.segs.get(i)?;
            return Some(seg.start + seg.len() + off);
        }
        s.parse::<f64>().ok().map(|n| self.to_edit_snap(n))
    }
    /// The take and original second playing at an edit time.
    pub fn to_orig(&self, e: f64) -> (usize, f64) {
        for (i, s) in self.segs.iter().enumerate() {
            if e < s.start + s.len() + s.hold || i == self.segs.len() - 1 {
                return (i, clampf(s.a + (e - s.start) * s.speed, s.a, s.b - 0.5 / self.fps));
            }
        }
        (0, 0.0)
    }
    /// The words that survive the cut, in edit order. A word belongs to the take holding its midpoint.
    pub fn edit_words(&self, words: &[Word]) -> Vec<EWord> {
        let mut out = vec![];
        for (i, s) in self.segs.iter().enumerate() {
            let from = words.partition_point(|w| (w.s + w.e) / 2.0 < s.a);
            for w in &words[from..] {
                let mid = (w.s + w.e) / 2.0;
                if mid > s.b {
                    break;
                }
                let ws = clampf(w.s, s.a, s.b);
                let we = clampf(w.e, s.a, s.b);
                out.push(EWord { text: w.w.trim().to_string(), s: s.start + (ws - s.a) / s.speed, e: s.start + (we - s.a) / s.speed, seg: i, orig: w.s });
            }
        }
        out
    }
    pub fn cuts(&self) -> Vec<f64> {
        self.segs.iter().skip(1).map(|s| s.start).collect()
    }
}
