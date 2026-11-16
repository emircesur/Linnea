//! Beats: what happens on screen on a spoken word. Cards in the band under the chin, reactions beside
//! the head, kinetic type, full-screen scenes, picture and video inserts, and the sounds that go with them.
use crate::ass::*;
use crate::fonts;
use crate::timeline::{EWord, Timeline};
use crate::util::*;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Sfx {
    pub t: f64,
    pub id: String,
    pub role: String,
    pub db: f64,
    pub why: String,
}

#[derive(Clone, Debug)]
pub struct Overlay {
    pub src: PathBuf,
    pub s: f64,
    pub e: f64,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    /// 0 = keep the aspect ratio.
    pub h: f64,
    pub video: bool,
    pub inpoint: f64,
    pub audio: bool,
    pub grade: String,
}

#[derive(Default)]
pub struct BeatOut {
    pub obstacles: Vec<Obstacle>,
    pub hide: Vec<(f64, f64)>,
    pub sfx: Vec<Sfx>,
    pub overlays: Vec<Overlay>,
    pub table: Vec<Value>,
    /// Edit-time windows that need the person matte, and the cutout effects in them.
    pub behind: Vec<(f64, f64)>,
    pub cutouts: Vec<(f64, f64, Value)>,
}

fn normw(t: &str) -> String {
    t.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase()
}

struct B<'a> {
    doc: &'a mut Doc,
    tl: &'a Timeline,
    ew: &'a [EWord],
    brand: Value,
    reel: &'a Value,
    dir: &'a Path,
    face: &'a dyn Fn(f64) -> Option<Rect>,
    out: BeatOut,
    sc: f64,
    w: f64,
    h: f64,
    quiet: bool,
    restrained: bool,
}

const POP: &str = "\\fscx60\\fscy60\\t(0,110,\\fscx106\\fscy106)\\t(110,190,\\fscx100\\fscy100)\\fad(40,120)";
const SLAM: &str = "\\fscx114\\fscy114\\t(0,130,\\fscx100\\fscy100)\\fad(50,140)";
const FADE: &str = "\\fad(160,140)";

impl<'a> B<'a> {
    fn font(&self) -> String {
        jsd(&self.brand, "font", "Montserrat").to_string()
    }
    fn accent(&self) -> String {
        jsd(&self.brand, "accent", "#FFD23F").to_string()
    }
    fn on_accent(&self) -> String {
        jsd(&self.brand, "onAccent", "#111111").to_string()
    }
    fn ink(&self) -> String {
        jsd(&self.brand, "ink", "#111111").to_string()
    }
    fn at(&self, v: &Value, key: &str) -> Option<f64> {
        v.get(key).and_then(|x| self.tl.resolve(x))
    }
    /// A default sound for the beat in hand; silent when the beat overrides its sounds.
    fn snd(&mut self, t: f64, id: &str, role: &str, why: &str) {
        if self.quiet || (self.restrained && role == "ui") {
            return;
        }
        self.out.sfx.push(Sfx { t: t.max(0.0), id: id.into(), role: role.into(), db: 0.0, why: why.into() });
    }
    /// Spoken onsets (edit time) of a run of words, searched from `from` on. None where it is not said.
    fn onsets(&self, texts: &[String], from: f64) -> Vec<Option<f64>> {
        let mut i = self.ew.partition_point(|w| w.s < from - 0.4);
        texts
            .iter()
            .map(|t| {
                let want: Vec<String> = t.split_whitespace().map(normw).filter(|x| !x.is_empty()).collect();
                let first = want.first()?;
                for j in i..self.ew.len().min(i + 40) {
                    if normw(&self.ew[j].text) == *first {
                        i = j + 1;
                        return Some(self.ew[j].s);
                    }
                }
                None
            })
            .collect()
    }
    fn band_top(&self, t: f64) -> f64 {
        let want = jfd(&self.reel["layout"], "cardY", 990.0) * self.sc;
        match (self.face)(t) {
            Some(f) if f.y1 + 24.0 * self.sc > want => (f.y1 + 30.0 * self.sc).min(self.h * 0.72),
            _ => want,
        }
    }
    fn slot(&self, t: f64, size: f64) -> (f64, f64) {
        let y = jfd(&self.reel["layout"], "slotY", 240.0) * self.sc + size / 2.0;
        let right = (self.face)(t).map(|f| (f.x0 + f.x1) / 2.0 <= self.w / 2.0).unwrap_or(true);
        (if right { self.w - size / 2.0 - 70.0 * self.sc } else { size / 2.0 + 70.0 * self.sc }, y)
    }
    fn media(&self, b: &Value) -> Option<PathBuf> {
        let src = js(b, "src").map(|s| s.to_string()).or_else(|| {
            let id = js(b, "id")?;
            let d = library_dir().join("memes").join(id);
            let meta = read_json_or(&d.join("meta.json"), json!({}));
            Some(fwd(&d.join(jsd(&meta, "file", "clip.mp4"))))
        })?;
        let p = Path::new(&src);
        let p = if p.is_absolute() { p.to_path_buf() } else { self.dir.join(&src) };
        p.exists().then_some(p)
    }
    fn ground(&mut self, s: f64, e: f64, color: &str, anim: &str) {
        let r = Rect { x0: -4.0, y0: -4.0, x1: self.w + 4.0, y1: self.h + 4.0 };
        self.doc.rect(10, s, e, &r, 0.0, color, 1.0, anim, false);
    }
    fn bg_color(&mut self, v: &Value, default: &str) -> String {
        match v {
            Value::String(s) if s == "accent" => self.accent(),
            Value::String(s) if s.starts_with('#') => s.clone(),
            Value::Object(o) => {
                let look = o.get("look").and_then(|x| x.as_str()).unwrap_or("");
                self.doc.warn(format!("shader background '{look}' is drawn as a flat colour"));
                match look {
                    "warm-grain" => "#E9DFD1",
                    "mesh-calm" => "#E8EEF5",
                    "ink-warp" => "#1B2A55",
                    "dither-retro" => "#20242A",
                    _ => "#101018",
                }
                .into()
            }
            _ => default.into(),
        }
    }

    /// Lines stacked and centred on `cy`; each appears at its own time. Returns the block rectangle.
    fn stack(&mut self, layer: i32, e: f64, cy: f64, lines: &[(String, Ts, f64, &str)], gap: f64) -> Rect {
        let total: f64 = lines.iter().map(|l| l.1.px * 1.22).sum::<f64>() + gap * lines.len().saturating_sub(1) as f64;
        let mut y = cy - total / 2.0;
        let mut maxw: f64 = 0.0;
        for (text, ts, s, anim) in lines {
            let lh = ts.px * 1.22;
            self.doc.text(layer, *s, e, Pos::At(self.w / 2.0, y + lh / 2.0), 5, ts, anim, text);
            maxw = maxw.max(ts.w(text));
            y += lh + gap;
        }
        Rect { x0: (self.w - maxw) / 2.0, y0: cy - total / 2.0, x1: (self.w + maxw) / 2.0, y1: cy + total / 2.0 }
    }

    fn card(&mut self, s: f64, e: f64, r: &Rect, color: &str, op: f64, anim: &str) {
        self.doc.rect(5, s, e, r, 34.0 * self.sc, color, op, anim, false);
        self.out.obstacles.push(Obstacle { s, e, r: *r });
    }

    fn big(&mut self, b: &Value, t0: f64, t1: f64) {
        let text = jsd(b, "text", "").to_string();
        let sub = jsd(b, "sub", "").to_string();
        let sc = self.sc;
        let f = self.font();
        let ts = self.doc.ts(&f, 800, false, jfd(b, "size", 118.0)).fit(&text, 860.0 * sc, 44.0 * sc);
        let sts = self.doc.ts(&f, 600, false, 42.0).color("#C9C9C9").fit(&sub, 860.0 * sc, 24.0 * sc);
        let cw = ts.w(&text).max(sts.w(&sub)) + 110.0 * sc;
        let ch = ts.px * 1.3 + if sub.is_empty() { 0.0 } else { sts.px * 1.5 } + 36.0 * sc;
        let top = self.band_top(t0);
        let r = Rect { x0: (self.w - cw) / 2.0, y0: top, x1: (self.w + cw) / 2.0, y1: top + ch };
        let ink = self.ink();
        self.card(t0, t1, &r, &ink, 0.96, SLAM);
        let ty = top + 18.0 * sc + ts.px * 0.65;
        // A number can count up before it lands.
        let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
        if jb(b, "count") == Some(true) && !digits.is_empty() && digits.len() < 9 {
            let n: f64 = digits.parse().unwrap_or(0.0);
            let (pre, post) = (text.split(|c: char| c.is_ascii_digit()).next().unwrap_or(""), text.rsplit(|c: char| c.is_ascii_digit()).next().unwrap_or(""));
            for k in 0..7 {
                let v = (n * (k as f64 + 1.0) / 8.0).round();
                self.doc.text(6, t0 + 0.06 * k as f64, t0 + 0.06 * (k + 1) as f64, Pos::At(self.w / 2.0, ty), 5, &ts, "", &format!("{pre}{v}{post}"));
            }
            self.doc.text(6, t0 + 0.42, t1, Pos::At(self.w / 2.0, ty), 5, &ts, "\\fad(0,140)", &text);
        } else {
            self.doc.text(6, t0, t1, Pos::At(self.w / 2.0, ty), 5, &ts, SLAM, &text);
        }
        if !sub.is_empty() {
            self.doc.text(6, t0 + 0.12, t1, Pos::At(self.w / 2.0, r.y1 - 18.0 * sc - sts.px * 0.75), 5, &sts, FADE, &sub);
        }
        self.snd(t0 - 0.05, "whoosh-cine", "whoosh", "big card in");
        self.snd(t0 + 0.08, "impact-1", "impact", "big card lands");
    }

    fn chips(&mut self, b: &Value, t0: f64, t1: f64) {
        let sc = self.sc;
        let f = self.font();
        let items: Vec<Value> = ja(b, "items").to_vec();
        let (hgt, gap, padx) = (78.0 * sc, 16.0 * sc, 30.0 * sc);
        let mut rows: Vec<Vec<(String, Ts, f64, String, f64)>> = vec![vec![]];
        let mut roww = 0.0;
        for (i, it) in items.iter().enumerate() {
            let label = js(it, "text").or_else(|| js(it, "label")).unwrap_or("").to_string();
            let ts = self.doc.ts(&f, 700, false, 42.0).color(jsd(it, "fg", "#111111"));
            let cw = ts.w(&label) + 2.0 * padx;
            if roww + cw > 940.0 * sc && !rows.last().unwrap().is_empty() {
                rows.push(vec![]);
                roww = 0.0;
            }
            roww += cw + gap;
            let at = self.at(it, "at").unwrap_or(t0 + 0.24 * i as f64).max(t0);
            rows.last_mut().unwrap().push((label, ts, cw, jsd(it, "bg", "#FFFFFF").to_string(), at));
        }
        let top = self.band_top(t0);
        let mut n = 0;
        for (ri, row) in rows.iter().enumerate() {
            let total: f64 = row.iter().map(|c| c.2).sum::<f64>() + gap * row.len().saturating_sub(1) as f64;
            let mut x = (self.w - total) / 2.0;
            let y = top + ri as f64 * (hgt + gap);
            for (label, ts, cw, bg, at) in row {
                let r = Rect { x0: x, y0: y, x1: x + cw, y1: y + hgt };
                self.doc.rect(5, *at, t1, &r, hgt, bg, 1.0, POP, false);
                self.doc.text(6, *at, t1, Pos::At(x + cw / 2.0, y + hgt / 2.0), 5, ts, POP, label);
                self.snd(*at, if n % 2 == 0 { "pop-1" } else { "pop-2" }, "ui", "chip");
                n += 1;
                x += cw + gap;
            }
        }
        let r = Rect { x0: 60.0 * sc, y0: top, x1: self.w - 60.0 * sc, y1: top + rows.len() as f64 * (hgt + gap) };
        self.out.obstacles.push(Obstacle { s: t0, e: t1, r });
    }

    fn strike(&mut self, b: &Value, t0: f64, t1: f64) {
        let sc = self.sc;
        let text = jsd(b, "text", "").to_string();
        let f = self.font();
        let ts = self.doc.ts(&f, 800, false, 84.0).fit(&text, 860.0 * sc, 40.0 * sc);
        let tw = ts.w(&text);
        let top = self.band_top(t0);
        let r = Rect { x0: (self.w - tw) / 2.0 - 50.0 * sc, y0: top, x1: (self.w + tw) / 2.0 + 50.0 * sc, y1: top + ts.px * 1.3 + 30.0 * sc };
        let ink = self.ink();
        self.card(t0, t1, &r, &ink, 0.96, SLAM);
        let cy = (r.y0 + r.y1) / 2.0;
        self.doc.text(6, t0, t1, Pos::At(self.w / 2.0, cy), 5, &ts, SLAM, &text);
        let st = self.at(b, "strikeAt").unwrap_or(t0 + 0.7);
        self.doc.rect(7, st, t1, &Rect { x0: (self.w - tw) / 2.0 - 14.0 * sc, y0: cy - 5.0 * sc, x1: (self.w + tw) / 2.0 + 14.0 * sc, y1: cy + 5.0 * sc }, 5.0 * sc, "#E5484D", 1.0, "\\fscx0\\t(0,220,\\fscx100)", true);
        self.snd(st, "whoosh-1", "whoosh", "strike");
    }

    fn lines_card(&mut self, b: &Value, t0: f64, t1: f64, bullets: bool) {
        let sc = self.sc;
        let f = self.font();
        let accent = self.accent();
        let mut rows: Vec<(String, Ts, f64)> = vec![];
        if let Some(h) = js(b, "header") {
            rows.push((h.to_uppercase(), self.doc.ts(&f, 700, false, 30.0).color(&accent), t0));
        }
        for (i, l) in ja(b, "lines").iter().enumerate() {
            let text = l.as_str().map(|s| s.to_string()).unwrap_or_else(|| jsd(l, "text", "").to_string());
            let big = jb(l, "big") == Some(true);
            let at = self.at(l, "at").unwrap_or(t0 + 0.45 * i as f64).max(t0);
            let text = if bullets { format!("•  {text}") } else { text };
            let ts = self.doc.ts(&f, if big { 800 } else { 600 }, false, if big { 78.0 } else { 48.0 }).fit(&text, 790.0 * sc, 28.0 * sc);
            rows.push((text, ts, at));
            if big {
                self.snd(at, "impact-1", "impact", "the big line");
            } else if bullets {
                self.snd(at, "pop-1", "ui", "list line");
            }
        }
        let hgt: f64 = rows.iter().map(|r| r.1.px * 1.36).sum::<f64>() + 56.0 * sc;
        let top = self.band_top(t0).min(self.h * 0.78 - hgt).max(self.h * 0.3);
        let r = Rect { x0: 90.0 * sc, y0: top, x1: self.w - 90.0 * sc, y1: top + hgt };
        let ink = self.ink();
        self.card(t0, t1, &r, &ink, 0.92, FADE);
        self.doc.rect(6, t0, t1, &Rect { x0: r.x0, y0: r.y0 + 26.0 * sc, x1: r.x0 + 9.0 * sc, y1: r.y1 - 26.0 * sc }, 4.0 * sc, &accent, 1.0, FADE, false);
        let mut y = top + 28.0 * sc;
        for (text, ts, at) in &rows {
            let lh = ts.px * 1.36;
            self.doc.text(6, *at, t1, Pos::Move(r.x0 + 46.0 * sc, y + lh / 2.0 + 14.0 * sc, r.x0 + 46.0 * sc, y + lh / 2.0, 0, 180), 4, ts, "\\fad(140,120)", text);
            y += lh;
        }
        if let Some(stamp) = js(b, "stamp") {
            let st = self.at(b, "stampAt").unwrap_or(t1 - 1.0);
            let ts = self.doc.ts(&f, 900, false, 92.0).color("#E5484D").outline(5.0 * sc, 0.0);
            let mut ts = ts.fit(stamp, 700.0 * sc, 40.0 * sc);
            ts.bord_color = "#FFFFFF".into();
            self.doc.text(8, st, t1, Pos::At(self.w / 2.0, (r.y0 + r.y1) / 2.0), 5, &ts, "\\frz9\\fscx190\\fscy190\\t(0,120,\\fscx100\\fscy100)\\fad(0,120)", &stamp.to_uppercase());
            self.snd(st, "impact-1", "impact", "stamp");
            if jsd(b, "stampSfx", "") != "none" {
                self.snd(st + 0.05, "buzzer-1", "meme", "stamp");
            }
        }
    }

    fn title(&mut self, b: &Value, t0: f64, t1: f64) {
        let sc = self.sc;
        let f = jsd(b, "font", &self.font()).to_string();
        let italic = jb(b, "italic") == Some(true);
        let mut ts = self.doc.ts(&f, jfd(b, "weight", 900.0) as u16, italic, jfd(b, "size", 150.0)).color(jsd(b, "color", "#FFFFFF")).outline(5.0 * sc, 3.0 * sc);
        ts.track = jfd(b, "tracking", 0.0);
        let words: Vec<(String, f64)> = if !ja(b, "words").is_empty() {
            ja(b, "words").iter().enumerate().map(|(i, w)| (jsd(w, "text", "").to_string(), self.at(w, "at").unwrap_or(t0 + 0.3 * i as f64))).collect()
        } else {
            let texts: Vec<String> = jsd(b, "text", "").split_whitespace().map(|s| s.to_string()).collect();
            let on = self.onsets(&texts, t0);
            texts.into_iter().zip(on).enumerate().map(|(i, (t, o))| (t, o.unwrap_or(t0 + 0.3 * i as f64))).collect()
        };
        let upper = jb(b, "upper") != Some(false);
        let words: Vec<(String, f64)> = words.into_iter().map(|(t, a)| (if upper { t.to_uppercase() } else { t }, a.max(t0))).collect();
        let widest = words.iter().map(|w| ts.w(&w.0)).fold(0.0, f64::max);
        if widest > 980.0 * sc {
            ts.px *= 980.0 * sc / widest;
        }
        let space = ts.w(" ");
        let mut lines: Vec<Vec<(String, f64, f64)>> = vec![vec![]];
        let mut lw = 0.0;
        for (t, a) in words {
            let ww = ts.w(&t);
            if lw + ww > 980.0 * sc && !lines.last().unwrap().is_empty() {
                lines.push(vec![]);
                lw = 0.0;
            }
            lw += ww + space;
            lines.last_mut().unwrap().push((t, a, ww));
        }
        let anim = if jsd(b, "anim", "pop") == "blur" { format!("\\alpha&HFF&\\blur{:.0}\\t(0,260,\\alpha&H00&\\blur0)\\fad(0,120)", 14.0 * sc) } else { POP.to_string() };
        let lh = ts.px * 1.08;
        let mut y = jfd(b, "y", 300.0) * sc;
        for line in &lines {
            let total: f64 = line.iter().map(|w| w.2).sum::<f64>() + space * line.len().saturating_sub(1) as f64;
            let mut x = (self.w - total) / 2.0;
            for (t, a, ww) in line {
                self.doc.text(7, *a, t1, Pos::At(x + ww / 2.0, y + lh / 2.0), 5, &ts, &anim, t);
                x += ww + space;
            }
            y += lh;
        }
    }

    fn endcard(&mut self, b: &Value, t0: f64, t1: f64) {
        let sc = self.sc;
        let f = self.font();
        let (accent, on_accent, ink) = (self.accent(), self.on_accent(), self.ink());
        let title = jsd(b, "title", "").to_string();
        let line = jsd(b, "line", "").to_string();
        let url = jsd(b, "url", "").to_string();
        let tts = self.doc.ts(&f, 800, false, 70.0).color(&ink).fit(&title, 780.0 * sc, 36.0 * sc);
        let lts = self.doc.ts(&f, 500, false, 38.0).color("#5A5A5A").fit(&line, 780.0 * sc, 24.0 * sc);
        let uts = self.doc.ts(&f, 700, false, 36.0).color(&on_accent).fit(&url, 700.0 * sc, 22.0 * sc);
        let hgt = 70.0 * sc + tts.px * 1.3 + if line.is_empty() { 0.0 } else { lts.px * 1.6 } + if url.is_empty() { 0.0 } else { 110.0 * sc };
        let cy = self.h * 0.6;
        let r = Rect { x0: 90.0 * sc, y0: cy - hgt / 2.0, x1: self.w - 90.0 * sc, y1: cy + hgt / 2.0 };
        self.card(t0, t1, &r, "#FFFFFF", 1.0, SLAM);
        let mut y = r.y0 + 36.0 * sc + tts.px * 0.65;
        self.doc.text(6, t0, t1, Pos::At(self.w / 2.0, y), 5, &tts, SLAM, &title);
        y += tts.px * 0.65;
        if !line.is_empty() {
            y += lts.px * 0.8;
            self.doc.text(6, t0 + 0.1, t1, Pos::At(self.w / 2.0, y), 5, &lts, FADE, &line);
            y += lts.px * 0.8;
        }
        if !url.is_empty() {
            let uw = uts.w(&url) + 70.0 * sc;
            let ur = Rect { x0: (self.w - uw) / 2.0, y0: y + 22.0 * sc, x1: (self.w + uw) / 2.0, y1: y + 22.0 * sc + 70.0 * sc };
            self.doc.rect(6, t0 + 0.2, t1, &ur, 70.0 * sc, &accent, 1.0, POP, false);
            self.doc.text(7, t0 + 0.2, t1, Pos::At(self.w / 2.0, (ur.y0 + ur.y1) / 2.0), 5, &uts, POP, &url);
        }
        self.snd(t0, "whoosh-1", "whoosh", "end card");
    }

    fn textfx(&mut self, b: &Value, t0: f64, t1: f64) {
        let sc = self.sc;
        let kind = jsd(b, "kind", "headline").to_string();
        let f = jsd(b, "font", &self.font()).to_string();
        let accent = jsd(b, "accent", &self.accent()).to_string();
        let color = jsd(b, "color", "#FFFFFF").to_string();
        let exit = format!("\\fad(0,{})", ms(jfd(b, "exit", 0.35)));
        let has_bg = b.get("bg").is_some();
        if has_bg {
            let c = self.bg_color(&b["bg"].clone(), "#101014");
            self.ground(t0, t1, &c, FADE);
        }
        if has_bg || jb(b, "captions") != Some(true) {
            self.out.hide.push((t0, t1));
        }
        let cy = jf(b, "y").map(|y| y * sc).unwrap_or_else(|| if has_bg { self.h * 0.46 } else { self.band_top(t0) + 150.0 * sc });
        let outl = if has_bg { 0.0 } else { 5.0 * sc };
        match kind.as_str() {
            "hook" => {
                let zf = (jfd(b, "zoomFrom", 2.7) * 100.0) as i64;
                let pull = format!("\\fscx{zf}\\fscy{zf}\\t(140,740,\\fscx100\\fscy100){exit}");
                let mut lines: Vec<(String, Ts, f64, &str)> = vec![];
                let key = jsd(b, "key", "").to_string();
                if !key.is_empty() {
                    lines.push((key, self.doc.ts(&f, 900, false, jfd(b, "keySize", 270.0)).color(&accent).outline(outl, 0.0), t0, &pull));
                }
                let word = jsd(b, "word", "").to_string();
                let word = if jb(b, "upper") == Some(false) { word } else { word.to_uppercase() };
                if !word.is_empty() {
                    let ts = self.doc.ts(&f, 900, false, jfd(b, "wordSize", 118.0)).color(&color).outline(outl, 0.0).fit(&word, 960.0 * sc, 40.0 * sc);
                    lines.push((word.clone(), ts, t0, &pull));
                }
                if let Some(s) = js(b, "sub") {
                    lines.push((s.to_string(), self.doc.ts(&f, 600, false, jfd(b, "subSize", 54.0)).color(&color).outline(outl * 0.6, 0.0), t0 + 0.5, FADE));
                }
                if !has_bg && jb(b, "pocket") != Some(false) {
                    self.doc.rect(4, t0, t1, &Rect { x0: -4.0, y0: cy - 420.0 * sc, x1: self.w + 4.0, y1: cy + 420.0 * sc }, 0.0, "#000000", 0.42, FADE, false);
                }
                self.stack(6, t1, cy, &lines, 6.0 * sc);
                self.snd(t0, "whoosh-cine", "whoosh", "hook pulls back");
                let punch = self.at(b, "punchAt").or_else(|| self.onsets(&[word], t0)[0]).unwrap_or(t0 + 0.8);
                self.snd(punch, "impact-1", "impact", "hook word");
            }
            "spring" => {
                let text = jsd(b, "text", "").to_string();
                let text = if jb(b, "upper") == Some(false) { text } else { text.to_uppercase() };
                let ts = self.doc.ts(&f, 900, false, jfd(b, "size", 120.0)).color(&color).outline(outl, 0.0).fit(&text, 960.0 * sc, 40.0 * sc);
                let mut x = (self.w - ts.w(&text)) / 2.0;
                for (i, ch) in text.chars().enumerate() {
                    let cw = ts.w(&ch.to_string());
                    if !ch.is_whitespace() {
                        let d = (i as i64) * 45;
                        self.doc.text(6, t0, t1, Pos::Move(x + cw / 2.0, cy - 150.0 * sc, x + cw / 2.0, cy, d, d + 240), 5, &ts, &format!("\\alpha&HFF&\\fscy130\\t({d},{},\\alpha&H00&)\\t({},{},\\fscy100){exit}", d + 60, d + 240, d + 340), &ch.to_string());
                    }
                    x += cw;
                }
                if let Some(s) = js(&b["sub"], "text") {
                    let sts = self.doc.ts(&f, 600, false, jfd(&b["sub"], "size", 54.0)).color(&color).outline(outl * 0.6, 0.0);
                    self.doc.text(6, t0 + 0.4, t1, Pos::At(self.w / 2.0, cy + ts.px * 0.9), 5, &sts, FADE, s);
                }
                self.snd(t0, "whoosh-1", "whoosh", "title drops");
            }
            _ => {
                if kind != "headline" {
                    self.doc.warn(format!("textfx '{kind}' is drawn as a headline (words rise on their spoken times)"));
                }
                let src: Vec<Value> = if !ja(b, "lines").is_empty() { ja(b, "lines").to_vec() } else if !ja(b, "items").is_empty() { ja(b, "items").to_vec() } else { vec![json!(jsd(b, "text", ""))] };
                let size = jfd(b, "size", 96.0);
                let mut ly = cy - (src.len() as f64 - 1.0) * size * sc * 0.62;
                let mut clock = t0;
                let mut marked: Vec<Rect> = vec![];
                for l in &src {
                    let raw = l.as_str().map(|s| s.to_string()).unwrap_or_else(|| jsd(l, "text", "").to_string());
                    let ts = self.doc.ts(&f, jfd(b, "weight", 800.0) as u16, false, jfd(l, "size", size)).color(&color).outline(outl, 0.0);
                    let plain = raw.replace('*', "");
                    let ts = ts.fit(&plain, 940.0 * sc, 36.0 * sc);
                    let toks: Vec<String> = raw.split_whitespace().map(|s| s.to_string()).collect();
                    let on = self.onsets(&toks.iter().map(|t| t.replace('*', "")).collect::<Vec<_>>(), clock);
                    let space = ts.w(" ");
                    let mut x = (self.w - ts.w(&plain)) / 2.0;
                    let mut in_mark = false;
                    for (t, o) in toks.iter().zip(on) {
                        let starts = t.starts_with('*');
                        let ends = t.ends_with('*') && t.len() > 1;
                        if starts {
                            in_mark = true;
                        }
                        let word = t.replace('*', "");
                        let ww = ts.w(&word);
                        let at = o.unwrap_or(clock + 0.09).max(t0);
                        clock = at;
                        let mut wts = ts.clone();
                        if in_mark {
                            marked.push(Rect { x0: x, y0: ly - ts.px * 0.55, x1: x + ww, y1: ly + ts.px * 0.55 });
                            if jsd(&b["mark"], "style", "under") == "box" {
                                wts.color = self.on_accent();
                            }
                        }
                        self.doc.text(7, at, t1, Pos::Move(x + ww / 2.0, ly + 34.0 * sc, x + ww / 2.0, ly, 0, 220), 5, &wts, &format!("\\alpha&HFF&\\t(0,200,\\alpha&H00&){exit}"), &word);
                        x += ww + space;
                        if ends {
                            in_mark = false;
                        }
                    }
                    ly += ts.px * 1.24;
                }
                if let (Some(a), Some(z)) = (marked.first().cloned(), marked.last().cloned()) {
                    let mt = self.at(&b["mark"], "at").unwrap_or(clock + 0.2);
                    let r = match jsd(&b["mark"], "style", "under") {
                        "marker" | "box" => Rect { x0: a.x0 - 10.0 * sc, y0: a.y0, x1: z.x1 + 10.0 * sc, y1: z.y1 },
                        "strike" => Rect { x0: a.x0 - 8.0 * sc, y0: (a.y0 + a.y1) / 2.0 - 5.0 * sc, x1: z.x1 + 8.0 * sc, y1: (a.y0 + a.y1) / 2.0 + 5.0 * sc },
                        _ => Rect { x0: a.x0, y0: z.y1 + 2.0 * sc, x1: z.x1, y1: z.y1 + 12.0 * sc },
                    };
                    let layer = if matches!(jsd(&b["mark"], "style", "under"), "marker" | "box") { 6 } else { 8 };
                    self.doc.rect(layer, mt.max(t0), t1, &r, 5.0 * sc, &accent, 1.0, &format!("\\fscx0\\t(0,420,\\fscx100){exit}"), true);
                    self.snd(mt, "whoosh-1", "whoosh", "mark");
                }
            }
        }
    }

    fn scene(&mut self, b: &Value, t0: f64, t1: f64) {
        let sc = self.sc;
        let kind = jsd(b, "kind", "card").to_string();
        let f = self.font();
        let (accent, on_accent, ink) = (self.accent(), self.on_accent(), self.ink());
        if jb(b, "captions") != Some(true) {
            self.out.hide.push((t0, t1));
        }
        if kind == "image" {
            match self.media(b) {
                Some(src) => self.out.overlays.push(Overlay { src, s: t0, e: t1, x: 0.0, y: 0.0, w: self.w, h: self.h, video: false, inpoint: 0.0, audio: false, grade: jsd(b, "grade", "").into() }),
                None => self.doc.warn(format!("image scene at {}: the picture '{}' is missing", fmt_t(t0), jsd(b, "src", ""))),
            }
            return;
        }
        let default_bg = match kind.as_str() {
            "sentence" => jsd(&self.reel["scenes"]["sentence"], "bg", "#0F0F10").to_string(),
            "stats" | "kinetic" => jsd(&self.brand, "kineticBg", &accent).to_string(),
            "ui" | "device" => "#0B0B0F".into(),
            "word" => ink.clone(),
            _ => "#F4F2EE".into(),
        };
        let bg = self.bg_color(&b["bg"].clone(), &default_bg);
        let dark = is_dark(&bg);
        let fg = if dark { "#FFFFFF".to_string() } else { ink.clone() };
        let tin = jsd(b, "in", jsd(&self.reel["scenes"], "default_in", "blur")).to_string();
        let tout = if jsd(b, "out", "blur") == "cut" { 0 } else { 150 };
        let ganim = match tin.as_str() {
            "cut" | "glitch" => format!("\\fad(0,{tout})"),
            "expand" => format!("\\fscx0\\fscy0\\t(0,260,\\fscx100\\fscy100)\\fad(0,{tout})"),
            _ => format!("\\fad(180,{tout})"),
        };
        if tin == "wipe" {
            let r = Rect { x0: -4.0, y0: -4.0, x1: self.w + 4.0, y1: self.h + 4.0 };
            self.doc.rect(10, t0, t1, &r, 0.0, &bg, 1.0, &format!("\\fscx0\\t(0,260,\\fscx100)\\fad(0,{tout})"), true);
        } else {
            self.ground(t0, t1, &bg, &ganim);
        }
        if tin == "expand" || tin == "wipe" {
            self.snd(t0, "whoosh-1", "whoosh", "scene in");
        }
        let out = format!("\\fad(0,{tout})");
        let blur_in = format!("\\alpha&HFF&\\blur{:.0}\\t(0,280,\\alpha&H00&\\blur0){out}", 14.0 * sc);
        let cy = jf(b, "y").map(|y| y * sc).unwrap_or(self.h * 0.44);
        match kind.as_str() {
            "word" => {
                let text = jsd(b, "text", "").to_string();
                let ts = self.doc.ts(&f, 800, false, jfd(b, "size", 150.0)).color(jsd(b, "ink", &fg)).fit(&text, 940.0 * sc, 48.0 * sc);
                self.doc.text(12, t0, t1, Pos::At(self.w / 2.0, cy), 5, &ts, &blur_in, &text);
            }
            "kinetic" => {
                // One word at a time on its spoken onset; *keywords* big in the dark accent, _fillers_ small.
                let toks: Vec<String> = jsd(b, "text", "").split_whitespace().map(|s| s.to_string()).collect();
                let on = self.onsets(&toks.iter().map(|t| t.replace(['*', '_'], "")).collect::<Vec<_>>(), t0);
                let mut times: Vec<f64> = vec![];
                let mut clock = t0;
                for o in &on {
                    clock = o.unwrap_or(clock + 0.28).max(clock + 0.05);
                    times.push(clock);
                }
                let size = jfd(b, "size", 170.0);
                for (i, t) in toks.iter().enumerate() {
                    let (key, fill) = (t.starts_with('*'), t.starts_with('_'));
                    let word = t.replace(['*', '_'], "");
                    let ts = self.doc.ts(&f, if fill { 500 } else { 900 }, false, if key { size * 1.25 } else if fill { size * 0.5 } else { size });
                    let ts = ts.color(if key { jsd(&self.brand, "accentDark", "#111111") } else { &fg }).fit(&word, 960.0 * sc, 40.0 * sc);
                    let end = times.get(i + 1).cloned().unwrap_or(t1).min(t1);
                    self.doc.text(12, times[i].max(t0), end.max(times[i] + 0.06), Pos::At(self.w / 2.0, cy), 5, &ts, "\\fscx84\\fscy84\\t(0,90,\\fscx100\\fscy100)", &word);
                }
                self.snd(t0, "whoosh-1", "whoosh", "kinetic scene");
            }
            "sentence" => {
                let mut clock = t0;
                let mut y = self.h * 0.3;
                for l in ja(b, "lines") {
                    let raw = l.as_str().unwrap_or("").to_string();
                    let ts = self.doc.ts(&f, 600, false, 60.0).color(&fg).fit(&raw, 900.0 * sc, 30.0 * sc);
                    let toks: Vec<String> = raw.split_whitespace().map(|s| s.to_string()).collect();
                    let on = self.onsets(&toks, clock);
                    let space = ts.w(" ");
                    let mut x = (self.w - ts.w(&raw)) / 2.0;
                    for (t, o) in toks.iter().zip(on) {
                        clock = o.unwrap_or(clock + 0.12).max(t0);
                        let ww = ts.w(t);
                        self.doc.text(12, clock, t1, Pos::At(x + ww / 2.0, y), 5, &ts, &format!("\\alpha&HFF&\\t(0,160,\\alpha&H00&){out}"), t);
                        x += ww + space;
                    }
                    y += ts.px * 1.4;
                }
                if let Some(h) = js(&b["hero"], "text") {
                    let ht = self.at(&b["hero"], "at").unwrap_or(clock + 0.4).max(t0);
                    let ts = self.doc.ts(&f, 900, false, jfd(&b["hero"], "size", 170.0)).color(jsd(&b["hero"], "color", &accent)).fit(h, 940.0 * sc, 50.0 * sc);
                    self.doc.text(12, ht, t1, Pos::At(self.w / 2.0, self.h * 0.52), 5, &ts, &format!("\\fscx130\\fscy130\\t(0,140,\\fscx100\\fscy100){out}"), h);
                    self.snd(ht, "impact-1", "impact", "hero word");
                    if let Some(s) = js(&b["sub"], "text") {
                        let st = self.at(&b["sub"], "at").unwrap_or(ht + 0.5);
                        let sts = self.doc.ts(&f, 500, false, 46.0).color(&fg);
                        self.doc.text(12, st, t1, Pos::At(self.w / 2.0, self.h * 0.52 + ts.px * 0.95), 5, &sts, FADE, s);
                    }
                }
            }
            "stats" => {
                let badges = ja(b, "badges").to_vec();
                let gap = jfd(b, "gap", 300.0) * sc;
                let top = jfd(b, "centerY", 860.0) * sc - gap * (badges.len() as f64 - 1.0) / 2.0;
                for (i, bd) in badges.iter().enumerate() {
                    let at = self.at(bd, "at").unwrap_or(t0 + 0.5 * i as f64).max(t0);
                    let y = top + gap * i as f64;
                    let v = jsd(bd, "value", "").to_string();
                    let ts = self.doc.ts(&f, 900, false, 150.0).color(&fg).fit(&v, 900.0 * sc, 50.0 * sc);
                    self.doc.text(12, at, t1, Pos::At(self.w / 2.0, y), 5, &ts, &format!("\\fscx150\\fscy150\\t(0,150,\\fscx100\\fscy100){out}"), &v);
                    let sub = js(bd, "sub").or_else(|| js(bd, "label")).unwrap_or("").replace("**", "");
                    let sts = self.doc.ts(&f, 600, false, 40.0).color(&fg).fit(&sub, 900.0 * sc, 22.0 * sc);
                    let lt = self.at(bd, "labelAt").unwrap_or(at + 0.25);
                    self.doc.text(12, lt, t1, Pos::At(self.w / 2.0, y + ts.px * 0.78), 5, &sts, FADE, &sub);
                    self.snd(at, "whoosh-cine", "whoosh", "stat lands");
                }
            }
            "ui" | "device" => {
                let mono = jsd(&self.brand, "mono", "JetBrains Mono").to_string();
                let win = Rect { x0: 80.0 * sc, y0: cy - 380.0 * sc, x1: self.w - 80.0 * sc, y1: cy + 380.0 * sc };
                self.doc.rect(11, t0, t1, &win, 30.0 * sc, "#17171D", 1.0, &format!("\\fscy90\\t(0,180,\\fscy100)\\fad(160,{tout})"), false);
                self.doc.rect(11, t0, t1, &Rect { x0: win.x0, y0: win.y0 + 78.0 * sc, x1: win.x1, y1: win.y0 + 80.0 * sc }, 0.0, "#FFFFFF", 0.1, FADE, false);
                let lts = self.doc.ts(&mono, 500, false, 28.0).color("#9A9AA5");
                self.doc.text(12, t0, t1, Pos::At(win.x0 + 40.0 * sc, win.y0 + 40.0 * sc), 4, &lts, FADE, js(b, "label").or_else(|| js(b, "title")).unwrap_or("linnea"));
                let mut y = win.y0 + 150.0 * sc;
                if let Some(p) = js(&b["prompt"], "text") {
                    let pt = self.at(&b["prompt"], "at").unwrap_or(t0 + 0.3).max(t0);
                    let cps = jfd(&b["prompt"], "cps", 22.0);
                    let pts = self.doc.ts(&mono, 500, false, 38.0).color("#FFFFFF").fit(p, win.w() - 110.0 * sc, 20.0 * sc);
                    let mut x = win.x0 + 40.0 * sc;
                    let caret = self.doc.ts(&mono, 700, false, 38.0).color(&accent);
                    self.doc.text(12, t0, t1, Pos::At(x, y), 4, &caret, FADE, ">");
                    x += caret.w("> ");
                    for (i, ch) in p.chars().enumerate() {
                        let cw = pts.w(&ch.to_string());
                        self.doc.text(12, pt + i as f64 / cps, t1, Pos::At(x, y), 4, &pts, &out, &ch.to_string());
                        x += cw;
                    }
                    self.snd(pt, "typing-1", "ui", "prompt types");
                    y += 100.0 * sc;
                }
                for (i, l) in ja(b, "lines").iter().enumerate() {
                    let text = l.as_str().map(|s| s.to_string()).unwrap_or_else(|| jsd(l, "text", "").to_string());
                    let at = self.at(l, "at").unwrap_or(t0 + 1.2 + 0.4 * i as f64).max(t0);
                    let me = jb(l, "me") == Some(true);
                    let ts = self.doc.ts(&mono, 500, false, 34.0).color(if me { &on_accent } else { "#D8D8E0" }).fit(&text, win.w() - 150.0 * sc, 20.0 * sc);
                    if kind == "device" {
                        let bw = ts.w(&text) + 50.0 * sc;
                        let bx = if me { win.x1 - 40.0 * sc - bw } else { win.x0 + 40.0 * sc };
                        self.doc.rect(12, at, t1, &Rect { x0: bx, y0: y - 34.0 * sc, x1: bx + bw, y1: y + 34.0 * sc }, 26.0 * sc, if me { &accent } else { "#2A2A33" }, 1.0, POP, false);
                        self.doc.text(13, at, t1, Pos::At(bx + 25.0 * sc, y), 4, &ts, POP, &text);
                    } else {
                        let ck = self.doc.ts(&mono, 700, false, 34.0).color("#4ADE80");
                        self.doc.text(12, at, t1, Pos::At(win.x0 + 40.0 * sc, y), 4, &ck, FADE, "+");
                        self.doc.text(12, at, t1, Pos::At(win.x0 + 90.0 * sc, y), 4, &ts, FADE, &text);
                    }
                    self.snd(at, "pop-1", "ui", "line");
                    y += 84.0 * sc;
                }
            }
            _ => {
                if kind != "card" {
                    self.doc.warn(format!("scene '{kind}' is drawn as a card"));
                }
                let mut lines: Vec<(String, Ts, f64, &str)> = vec![];
                let src: Vec<Value> = if !ja(b, "lines").is_empty() { ja(b, "lines").to_vec() } else { ja(b, "items").to_vec() };
                for (i, l) in src.iter().enumerate() {
                    let text = l.as_str().map(|s| s.to_string()).unwrap_or_else(|| jsd(l, "text", "").to_string());
                    let at = self.at(l, "at").unwrap_or(t0 + 0.35 * i as f64).max(t0);
                    let ts = self.doc.ts(&f, 700, false, jfd(l, "size", 84.0)).color(jsd(l, "color", &fg)).fit(&text, 920.0 * sc, 36.0 * sc);
                    lines.push((text, ts, at, &blur_in));
                }
                let r = self.stack(12, t1, cy, &lines, jfd(b, "gap", 10.0) * sc);
                if let Some(p) = js(&b["pill"], "text") {
                    let pt = self.at(&b["pill"], "at").unwrap_or(t0 + 0.6).max(t0);
                    let ts = self.doc.ts(&f, 800, false, jfd(&b["pill"], "size", 64.0)).color(&on_accent);
                    let pw = ts.w(p) + 70.0 * sc;
                    let py = r.y1 + jfd(&b["pill"], "gap", 40.0) * sc + ts.px * 0.8;
                    self.doc.rect(12, pt, t1, &Rect { x0: (self.w - pw) / 2.0, y0: py - ts.px * 0.8, x1: (self.w + pw) / 2.0, y1: py + ts.px * 0.8 }, ts.px, &accent, 1.0, &format!("\\frz4{POP}"), false);
                    self.doc.text(13, pt, t1, Pos::At(self.w / 2.0, py), 5, &ts, &format!("\\frz4{POP}"), p);
                    self.snd(pt, "pop-1", "ui", "pill");
                    if b.get("cursor").is_some() {
                        self.snd(self.at(&b["cursor"], "clickAt").unwrap_or(pt + 0.6), "click-1", "ui", "cursor click");
                    }
                }
            }
        }
    }

    fn run(&mut self, b: &Value, idx: usize) {
        let kind = jsd(b, "type", "").to_string();
        let Some(t0) = self.at(b, "at").or_else(|| (kind == "endcard").then_some(self.tl.body)) else {
            self.doc.warn(format!("beat {idx} ({kind}) has no usable `at`"));
            return;
        };
        let dflt = match kind.as_str() {
            "meme" => 1.8,
            "endcard" => self.tl.total - t0,
            _ => 2.5,
        };
        let t1 = self.at(b, "to").unwrap_or(t0 + dflt).min(self.tl.total).max(t0 + 0.1);
        if t0 >= self.tl.total - 0.03 {
            self.doc.warn(format!("beat {idx} ({kind}) starts after the reel ends ({}); give the reel an `outro` or move it", fmt_t(self.tl.total)));
            return;
        }
        self.quiet = b.get("sfx").is_some();
        let n0 = self.out.sfx.len();
        let text_fields: String = ["text", "sub", "title", "line", "key", "word"].iter().filter_map(|k| js(b, k)).collect::<Vec<_>>().join(" ");
        if text_fields.contains('—') {
            self.doc.warn(format!("beat {idx} ({kind}): an em dash in on-screen text"));
        }
        match kind.as_str() {
            "big" => self.big(b, t0, t1),
            "chips" => self.chips(b, t0, t1),
            "strike" => self.strike(b, t0, t1),
            "quote" => self.lines_card(b, t0, t1, false),
            "list" => self.lines_card(b, t0, t1, true),
            "title" => self.title(b, t0, t1),
            "endcard" => self.endcard(b, t0, t1),
            "textfx" => self.textfx(b, t0, t1),
            "scene" => self.scene(b, t0, t1),
            "behind" => {
                let matte = jb(b, "front") != Some(true) && jb(self.reel, "matte") != Some(false) && crate::matte::pick(&crate::config::Config::load(), jsd(&self.reel["matte"], "model", "")).is_some();
                if jb(b, "front") != Some(true) && !matte {
                    self.doc.warn(format!("beat {idx}: a word BEHIND the speaker needs a matte model (Settings > Tools > download \"matte\"); it is drawn in front"));
                }
                if matte {
                    self.out.behind.push((t0, t1));
                    self.doc.base = 1000;
                }
                let mut t = b.clone();
                t["type"] = json!("title");
                if t.get("y").is_none() {
                    t["y"] = json!((self.face)(t0).map(|f| (f.y0 / self.sc - 40.0).max(160.0)).unwrap_or(420.0));
                }
                if t.get("size").is_none() {
                    t["size"] = json!(230);
                }
                self.title(&t, t0, t1);
                self.doc.base = 0;
                self.snd(t0, "impact-1", "impact", "big word");
            }
            "emoji" => {
                let size = jfd(b, "size", 260.0).min(280.0) * self.sc;
                let (sx, sy) = self.slot(t0, size);
                let (x, y) = (jf(b, "x").map(|v| v * self.sc).unwrap_or(sx), jf(b, "y").map(|v| v * self.sc).unwrap_or(sy));
                if let Some(src) = self.media(b) {
                    self.out.overlays.push(Overlay { src, s: t0, e: t1, x: x - size / 2.0, y: y - size / 2.0, w: size, h: 0.0, video: false, inpoint: 0.0, audio: false, grade: String::new() });
                } else if let Some(g) = js(b, "emoji") {
                    let mut ts = Ts::new("Arial", 400, false, size * 0.8);
                    ts.font = fonts::raw("Segoe UI Emoji");
                    ts.bord = 3.0 * self.sc;
                    self.doc.text(7, t0, t1, Pos::At(x, y), 5, &ts, POP, g);
                } else {
                    self.doc.warn(format!("beat {idx}: emoji '{}' is not in the library; give `emoji` a glyph or `src` a picture", jsd(b, "name", "")));
                }
                self.snd(t0, "pop-1", "ui", "emoji");
            }
            "logo" | "meme" | "broll" | "panel" => {
                let Some(src) = self.media(b) else {
                    self.doc.warn(format!("beat {idx} ({kind}): the file '{}' is missing", js(b, "src").or_else(|| js(b, "id")).unwrap_or("")));
                    return;
                };
                let ext = src.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                let video = matches!(ext.as_str(), "mp4" | "mov" | "webm" | "mkv" | "m4v" | "gif");
                let mut o = Overlay { src, s: t0, e: t1, x: 0.0, y: 0.0, w: self.w, h: self.h, video, inpoint: jfd(b, "in", 0.0), audio: false, grade: jsd(b, "grade", "").into() };
                match kind.as_str() {
                    "broll" | "panel" => {
                        if jsd(b, "mode", "") == "split" || kind == "panel" {
                            o.h = jfd(b, "h", if kind == "panel" { 768.0 } else { 960.0 }) * self.sc;
                        } else {
                            self.out.table.push(json!({"fullscreen": [t0, t1]}));
                        }
                    }
                    _ => {
                        let size = jfd(b, "w", if kind == "logo" { 220.0 } else { 420.0 }) * self.sc;
                        let (sx, sy) = self.slot(t0, size);
                        o.w = size;
                        o.h = 0.0;
                        o.x = jf(b, "x").map(|v| v * self.sc).unwrap_or(sx - size / 2.0);
                        o.y = jf(b, "y").map(|v| v * self.sc).unwrap_or(sy - size / 2.0);
                        if kind == "meme" {
                            o.audio = video && jb(b, "audio") != Some(false);
                            if !o.audio {
                                self.snd(t0, "deep-boom", "meme", "meme lands");
                            }
                        } else {
                            self.snd(t0, "pop-1", "ui", "logo");
                        }
                    }
                }
                self.out.overlays.push(o);
            }
            "cutout" => {
                if crate::matte::pick(&crate::config::Config::load(), jsd(&self.reel["matte"], "model", "")).is_some() {
                    self.out.cutouts.push((t0, t1, b.clone()));
                } else {
                    self.doc.warn(format!("beat {idx}: cutout effects need a matte model (Settings > Tools > download \"matte\"); the beat is skipped"));
                }
            }
            other => self.doc.warn(format!("beat {idx}: unknown type '{other}'")),
        }
        self.quiet = false;
        // The beat's own sound list replaces the defaults.
        match &b["sfx"] {
            Value::String(s) if s != "none" => self.out.sfx.push(Sfx { t: t0, id: s.clone(), role: String::new(), db: 0.0, why: format!("{kind} (chosen)") }),
            Value::Array(a) => {
                for (i, s) in a.iter().filter_map(|x| x.as_str()).enumerate() {
                    self.out.sfx.push(Sfx { t: t0 + 0.08 * i as f64, id: s.into(), role: String::new(), db: 0.0, why: format!("{kind} (chosen)") });
                }
            }
            _ => {}
        }
        self.out.table.push(json!({"beat": idx, "type": kind, "at": (t0 * 100.0).round() / 100.0, "to": (t1 * 100.0).round() / 100.0, "sounds": self.out.sfx.len() - n0}));
    }
}

/// Draw every beat of a resolved reel into the overlay document.
pub fn beats(doc: &mut Doc, reel: &Value, tl: &Timeline, ew: &[EWord], dir: &Path, face: &dyn Fn(f64) -> Option<Rect>) -> BeatOut {
    let (sc, w, h) = (doc.cv.sc, doc.cv.w, doc.cv.h);
    let mut b = B { doc, tl, ew, brand: reel["brand"].clone(), reel, dir, face, out: BeatOut::default(), sc, w, h, quiet: false, restrained: jsd(reel, "sfxProfile", "standard") == "restrained" };
    for (i, beat) in ja(reel, "beats").iter().enumerate() {
        b.run(beat, i);
    }
    // Hand-placed sounds.
    for s in ja(reel, "sfx") {
        if let (Some(t), Some(id)) = (b.at(s, "at"), js(s, "id")) {
            b.out.sfx.push(Sfx { t: (t - jfd(s, "lead", 0.0)).max(0.0), id: id.into(), role: String::new(), db: jfd(s, "db", 0.0), why: "placed by hand".into() });
        }
    }
    b.out
}
