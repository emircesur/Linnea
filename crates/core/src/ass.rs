//! The overlay layer as an ASS script that ffmpeg (libass) burns in: primitives and the caption templates.
//! Every word and box is laid out here from real font metrics and positioned on its own, so a word that
//! animates never reflows its neighbours.
use crate::fonts::{self, Font};
use crate::timeline::{EWord, Timeline};
use crate::util::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub struct Cv {
    pub w: f64,
    pub h: f64,
    /// Scale from the 1080x1920 design canvas.
    pub sc: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}
impl Rect {
    pub fn w(&self) -> f64 {
        self.x1 - self.x0
    }
    pub fn h(&self) -> f64 {
        self.y1 - self.y0
    }
    pub fn hits(&self, o: &Rect) -> bool {
        self.x0 < o.x1 && o.x0 < self.x1 && self.y0 < o.y1 && o.y0 < self.y1
    }
}

/// Something on screen for a while that captions should step around.
pub struct Obstacle {
    pub s: f64,
    pub e: f64,
    pub r: Rect,
}

pub struct Ev {
    layer: i32,
    s: f64,
    e: f64,
    text: String,
}

pub struct Doc {
    pub cv: Cv,
    pub ev: Vec<Ev>,
    pub warn: Vec<String>,
    /// Template font -> the face that stood in for it.
    pub subst: BTreeMap<String, String>,
    /// Added to every layer pushed (captions draw above beats).
    pub base: i32,
}

pub fn col(hex: &str) -> String {
    let h = hex.trim().trim_start_matches('#');
    let h = if h.len() == 3 { h.chars().flat_map(|c| [c, c]).collect::<String>() } else { h.to_string() };
    if h.len() < 6 || !h.chars().take(6).all(|c| c.is_ascii_hexdigit()) {
        return "&HFFFFFF&".into();
    }
    format!("&H{}{}{}&", &h[4..6], &h[2..4], &h[0..2]).to_uppercase()
}
/// Opacity 0..1 -> ASS alpha (00 is opaque).
pub fn alpha(op: f64) -> String {
    format!("&H{:02X}&", ((1.0 - clampf(op, 0.0, 1.0)) * 255.0).round() as u8)
}
pub fn ms(t: f64) -> i64 {
    (t * 1000.0).round() as i64
}
fn stamp(t: f64) -> String {
    let cs = (t.max(0.0) * 100.0).round() as i64;
    format!("{}:{:02}:{:02}.{:02}", cs / 360000, cs / 6000 % 60, cs / 100 % 60, cs % 100)
}
pub fn esc(t: &str) -> String {
    t.replace('\\', "/").replace('{', "(").replace('}', ")").replace(['\n', '\r'], " ")
}

/// How a run of text looks.
#[derive(Clone)]
pub struct Ts {
    pub font: Font,
    /// CSS-style em size in output pixels.
    pub px: f64,
    pub color: String,
    pub op: f64,
    pub bord: f64,
    pub bord_color: String,
    pub shad: f64,
    pub blur: f64,
    pub track: f64,
}
impl Ts {
    pub fn new(family: &str, weight: u16, italic: bool, px: f64) -> Ts {
        Ts { font: fonts::pick(family, weight, italic), px, color: "#FFFFFF".into(), op: 1.0, bord: 0.0, bord_color: "#000000".into(), shad: 0.0, blur: 0.0, track: 0.0 }
    }
    pub fn color(mut self, c: &str) -> Ts {
        self.color = c.into();
        self
    }
    pub fn outline(mut self, b: f64, s: f64) -> Ts {
        self.bord = b;
        self.shad = s;
        self
    }
    pub fn w(&self, text: &str) -> f64 {
        self.font.width(text, self.px, self.track)
    }
    /// Shrink until the text fits a width.
    pub fn fit(mut self, text: &str, max_w: f64, min_px: f64) -> Ts {
        let w = self.w(text);
        if w > max_w {
            self.px = (self.px * max_w / w).max(min_px);
        }
        self
    }
    fn tags(&self) -> String {
        format!(
            "\\fn{}\\fs{:.1}\\b{}\\i{}\\1c{}\\1a{}\\3c{}\\bord{:.1}\\shad{:.1}\\4a&H78&\\blur{:.1}\\fsp{:.1}",
            self.font.name,
            self.font.ass_size(self.px),
            if self.font.bold { 1 } else { 0 },
            if self.font.italic { 1 } else { 0 },
            col(&self.color),
            alpha(self.op),
            col(&self.bord_color),
            self.bord,
            self.shad,
            self.blur,
            self.track * self.px
        )
    }
}

pub enum Pos {
    At(f64, f64),
    /// x1 y1 -> x2 y2 between t1 and t2 (ms from the event start).
    Move(f64, f64, f64, f64, i64, i64),
}
impl Pos {
    fn tag(&self) -> String {
        match self {
            Pos::At(x, y) => format!("\\pos({x:.1},{y:.1})"),
            Pos::Move(a, b, c, d, t1, t2) => format!("\\move({a:.1},{b:.1},{c:.1},{d:.1},{t1},{t2})"),
        }
    }
}

impl Doc {
    pub fn new(w: f64, h: f64) -> Doc {
        Doc { cv: Cv { w, h, sc: (h / 1920.0).min(w / 1080.0) }, ev: vec![], warn: vec![], subst: BTreeMap::new(), base: 0 }
    }
    pub fn warn(&mut self, m: impl Into<String>) {
        let m = m.into();
        if !self.warn.contains(&m) {
            self.warn.push(m);
        }
    }
    pub fn ts(&mut self, family: &str, weight: u16, italic: bool, design_px: f64) -> Ts {
        let t = Ts::new(family, weight, italic, design_px * self.cv.sc);
        if t.font.substituted {
            self.subst.insert(family.to_string(), t.font.name.clone());
        }
        t
    }
    pub fn text(&mut self, layer: i32, s: f64, e: f64, pos: Pos, an: u8, ts: &Ts, anim: &str, text: &str) {
        if e - s < 0.02 || text.trim().is_empty() {
            return;
        }
        self.ev.push(Ev { layer: layer + self.base, s, e, text: format!("{{\\an{an}{}{}{anim}}}{}", pos.tag(), ts.tags(), esc(text)) });
    }
    /// A rounded rectangle. `wipe` anchors it on its left edge so \fscx grows it left to right.
    pub fn rect(&mut self, layer: i32, s: f64, e: f64, r: &Rect, radius: f64, color: &str, op: f64, anim: &str, wipe: bool) {
        if e - s < 0.02 || r.w() < 1.0 || r.h() < 1.0 {
            return;
        }
        let (w, h) = (r.w().round(), r.h().round());
        let rad = radius.min(w / 2.0).min(h / 2.0).max(0.0).round();
        let k = (rad * 0.5523).round();
        let path = if rad < 1.0 {
            format!("m 0 0 l {w} 0 {w} {h} 0 {h}")
        } else {
            format!(
                "m {rad} 0 l {a} 0 b {b} 0 {w} {c} {w} {rad} l {w} {d} b {w} {e2} {b} {h} {a} {h} l {rad} {h} b {c} {h} 0 {e2} 0 {d} l 0 {rad} b 0 {c} {c} 0 {rad} 0",
                a = w - rad,
                b = w - rad + k,
                c = rad - k,
                d = h - rad,
                e2 = h - rad + k
            )
        };
        let (an, x, y) = if wipe { (4, r.x0, (r.y0 + r.y1) / 2.0) } else { (5, (r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0) };
        self.ev.push(Ev { layer: layer + self.base, s, e, text: format!("{{\\an{an}\\pos({x:.1},{y:.1})\\p1\\1c{}\\1a{}\\bord0\\shad0\\blur0{anim}}}{path}{{\\p0}}", col(color), alpha(op)) });
    }
    /// Only the events whose layer is in lo..hi (the words drawn behind the speaker live at 1000+).
    pub fn render_layers(&self, lo: i32, hi: i32) -> String {
        let full = self.render();
        let mut out = String::new();
        for line in full.lines() {
            if let Some(rest) = line.strip_prefix("Dialogue: ") {
                let layer: i32 = rest.split(',').next().and_then(|x| x.trim().parse().ok()).unwrap_or(0);
                if layer < lo || layer >= hi {
                    continue;
                }
            }
            out.push_str(line);
            out.push('\n');
        }
        out
    }
    pub fn has_layers(&self, lo: i32, hi: i32) -> bool {
        self.ev.iter().any(|e| e.layer >= lo && e.layer < hi)
    }
    pub fn render(&self) -> String {
        let mut out = format!(
            "[Script Info]\nScriptType: v4.00+\nPlayResX: {}\nPlayResY: {}\nWrapStyle: 2\nScaledBorderAndShadow: yes\nYCbCr Matrix: TV.709\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: D,Arial,48,&H00FFFFFF,&H00FFFFFF,&H00000000,&H78000000,0,0,0,0,100,100,0,0,1,0,0,5,0,0,0,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n",
            self.cv.w as i64, self.cv.h as i64
        );
        for e in &self.ev {
            out.push_str(&format!("Dialogue: {},{},{},D,,0,0,0,,{}\n", e.layer, stamp(e.s), stamp(e.e), e.text));
        }
        out
    }
    /// Shift every event (used to render one frame at a given edit time) - not needed when PTS is offset instead.
    pub fn len(&self) -> usize {
        self.ev.len()
    }
}

// ---------- captions ----------

const STOP: &[&str] = &[
    "the", "a", "an", "and", "or", "but", "of", "to", "in", "on", "at", "for", "with", "is", "are", "was", "were", "be", "it", "this", "that", "you", "your", "i", "we", "they", "he", "she", "my", "so", "just", "like", "not", "do", "does", "have", "has", "what", "when", "then", "than", "them", "there", "their", "from", "about", "really", "very",
    "ve", "bir", "bu", "şu", "o", "da", "de", "ki", "mi", "mı", "mu", "mü", "için", "ile", "ama", "çok", "daha", "en", "gibi", "ne", "ben", "sen", "biz", "siz", "onlar", "var", "yok", "değil", "her", "şey", "olarak", "kadar", "sonra", "önce", "yani", "işte", "hani",
];

fn norm(t: &str) -> String {
    t.chars().filter(|c| c.is_alphanumeric() || *c == '$' || *c == '%').collect::<String>().to_lowercase()
}

struct CW {
    text: String,
    s: f64,
    e: f64,
    brk: bool,
    kw: bool,
}

pub struct CapCtx<'a> {
    pub tl: &'a Timeline,
    /// Edit-time windows where captions hide (full-screen scenes).
    pub hide: &'a [(f64, f64)],
    pub obstacles: &'a [Obstacle],
    pub face: &'a dyn Fn(f64) -> Option<Rect>,
    pub safe_top: f64,
    pub safe_bottom: f64,
    pub safe_right: f64,
}

fn prep_words(cap: &Value, ewords: &[EWord]) -> Vec<CW> {
    let fixes: Vec<(String, String)> = cap["fixes"].as_object().map(|o| o.iter().map(|(k, v)| (norm(k), v.as_str().unwrap_or("").to_string())).collect()).unwrap_or_default();
    let phrases: Vec<(Vec<String>, String)> =
        cap["phrases"].as_object().map(|o| o.iter().map(|(k, v)| (k.split_whitespace().map(norm).collect(), v.as_str().unwrap_or("").to_string())).collect()).unwrap_or_default();
    let case = jsd(cap, "case", "");
    let strip_all = jb(cap, "strip") == Some(true);
    let mut out: Vec<CW> = vec![];
    let mut i = 0;
    while i < ewords.len() {
        let mut text = ewords[i].text.clone();
        let (s, mut e) = (ewords[i].s, ewords[i].e);
        let mut used = 1;
        for (toks, rep) in &phrases {
            if !toks.is_empty() && i + toks.len() <= ewords.len() && toks.iter().enumerate().all(|(k, t)| norm(&ewords[i + k].text) == *t) {
                let tail: String = ewords[i + toks.len() - 1].text.chars().rev().take_while(|c| c.is_ascii_punctuation()).collect::<String>().chars().rev().collect();
                text = format!("{rep}{tail}");
                e = ewords[i + toks.len() - 1].e;
                used = toks.len();
                break;
            }
        }
        if used == 1 {
            if let Some((_, rep)) = fixes.iter().find(|(k, _)| *k == norm(&text)) {
                let tail: String = text.chars().rev().take_while(|c| c.is_ascii_punctuation()).collect::<String>().chars().rev().collect();
                text = format!("{rep}{tail}");
            }
        }
        let brk = text.ends_with(['.', ',', '?', '!', ';', ':', '…']);
        let mut shown = text.replace('—', "-");
        if strip_all {
            shown = shown.trim_matches(|c: char| c.is_ascii_punctuation() && c != '$' && c != '%' && c != '#').to_string();
        } else {
            shown = shown.trim_end_matches(['.', ',', ';', ':']).to_string();
        }
        shown = match case {
            "upper" => shown.to_uppercase().replace('i', "İ"),
            "lower" => shown.to_lowercase(),
            _ => shown,
        };
        if case == "upper" {
            shown = shown.replace("İ", "I").replace("ı", "I");
            // Keep the Turkish dotted capital only when the source had a Turkish letter.
            if text.contains(['ı', 'ş', 'ğ', 'ç', 'ö', 'ü', 'İ']) {
                shown = text.trim_end_matches(['.', ',', ';', ':']).chars().map(|c| match c { 'i' => 'İ', 'ı' => 'I', c => c.to_uppercase().next().unwrap_or(c) }).collect();
            }
        }
        if !shown.is_empty() {
            out.push(CW { text: shown, s, e, brk, kw: false });
        }
        i += used;
    }
    out
}

fn mark_keywords(cap: &Value, blocks: &mut [Vec<CW>]) {
    let mode = jsd(cap, "keyword", "none");
    let mut listed: Vec<String> = vec![];
    for k in ["keywords", "highlight", "tags"] {
        for v in ja(cap, k) {
            if let Some(s) = v.as_str() {
                listed.extend(s.split_whitespace().map(norm));
            }
        }
    }
    let auto = mode != "none" && cap["keywords"] != json!(false) && jb(cap, "keywordsAuto") != Some(false) && listed.is_empty();
    for b in blocks.iter_mut() {
        if !listed.is_empty() {
            for w in b.iter_mut() {
                w.kw = listed.contains(&norm(&w.text));
            }
        } else if auto && b.len() >= 2 {
            let score = |w: &CW| {
                let n = norm(&w.text);
                if STOP.contains(&n.as_str()) {
                    return -10.0;
                }
                n.chars().count() as f64 + if n.chars().any(|c| c.is_ascii_digit() || c == '$' || c == '%') { 5.0 } else { 0.0 }
            };
            let best = b.iter().enumerate().max_by(|a, c| score(a.1).partial_cmp(&score(c.1)).unwrap()).map(|(i, w)| (i, score(w)));
            if let Some((i, sc)) = best {
                if sc >= 5.0 {
                    b[i].kw = true;
                }
            }
        }
    }
}

struct Placed {
    text: String,
    cx: f64,
    cy: f64,
    w: f64,
    ts: Ts,
    kw: bool,
    s: f64,
    e: f64,
}

/// Draw the captions. Returns the layout log (one entry per block) for the QA.
pub fn captions(doc: &mut Doc, cap: &Value, ewords: &[EWord], ctx: &CapCtx) -> Vec<Value> {
    if jsd(cap, "style", "") == "none" || ewords.is_empty() {
        return vec![];
    }
    let sc = doc.cv.sc;
    let (cw_, _ch) = (doc.cv.w, doc.cv.h);
    let words = prep_words(cap, ewords);
    // ---- group into blocks ----
    let group = jfd(cap, "group", 3.0).max(1.0) as usize;
    let max_chars = jfd(cap, "maxChars", 999.0) as usize;
    let mut blocks: Vec<Vec<CW>> = vec![];
    let mut cur: Vec<CW> = vec![];
    for w in words {
        let chars: usize = cur.iter().map(|x| x.text.chars().count() + 1).sum::<usize>() + w.text.chars().count();
        let gap = cur.last().map(|l| w.s - l.e).unwrap_or(0.0);
        if !cur.is_empty() && (cur.len() >= group || gap > 0.6 || cur.last().map(|l| l.brk).unwrap_or(false) || chars > max_chars) {
            blocks.push(std::mem::take(&mut cur));
        }
        cur.push(w);
    }
    if !cur.is_empty() {
        blocks.push(cur);
    }
    mark_keywords(cap, &mut blocks);

    // ---- look ----
    let family = jsd(cap, "font", "Montserrat").to_string();
    let weight = jfd(cap, "weight", 700.0) as u16;
    let italic = jb(cap, "italic") == Some(true);
    let size = jfd(cap, "size", 60.0);
    let bg = jsd(cap, "bg", "none").to_string();
    let has_bg = bg != "none";
    let accent = jsd(cap, "accent", "#FFD23F").to_string();
    let on_accent = jsd(cap, "onAccent", "#111111").to_string();
    let accent2 = jsd(cap, "accent2", &accent).to_string();
    let base_color = jsd(cap, "color", "#FFFFFF").to_string();
    let mut base = doc.ts(&family, weight, italic, size);
    base.color = base_color.clone();
    base.track = jfd(cap, "tracking", 0.0);
    base.bord = jfd(cap, "outline", if has_bg { 0.0 } else { 3.0 }) * sc;
    base.shad = jfd(cap, "shadow", 0.0) * sc;
    base.bord_color = jsd(cap, "outlineColor", "#000000").into();
    let mode = jsd(cap, "mode", "phrase").to_string();
    let active = jsd(cap, "active", "none").to_string();
    let entry = jsd(cap, "entry", "cut").to_string();
    let kwmode = jsd(cap, "keyword", "none").to_string();
    let dim = jfd(cap, "dim", 1.0);
    let left = jsd(cap, "align", "center") == "left";
    let x_left = jfd(cap, "x", 64.0) * sc;
    let max_w = jf(cap, "maxw").map(|m| m * sc).unwrap_or(cw_ - ctx.safe_right.max(60.0 * sc) - if left { x_left } else { 60.0 * sc }).min(cw_ * 0.9);
    let y_want = jfd(cap, "y", 1200.0) * sc;
    let kw_scale = jfd(cap, "keywordScale", 1.0);
    let kw_ts = |doc: &mut Doc| -> Ts {
        let mut t = base.clone();
        match kwmode.as_str() {
            "serif" => {
                t = doc.ts(jsd(cap, "keywordFont", "Playfair Display"), 600, true, size * kw_scale);
                t.color = accent.clone();
                t.bord = base.bord;
                t.shad = base.shad;
            }
            "color" => t.color = if active == "box" { accent2.clone() } else { accent.clone() },
            "box" => {
                t.color = on_accent.clone();
                t.bord = 0.0;
                t.shad = 0.0;
            }
            _ => {}
        }
        t
    };
    let kts = kw_ts(doc);
    let lh = base.px.max(kts.px) * 1.24;
    let space = base.w(" ").max(base.px * 0.22);

    let mut log = vec![];
    let mut prev_y: Option<f64> = None;
    let n = blocks.len();
    // Block windows first, so each can end where the next begins.
    let starts: Vec<f64> = blocks.iter().map(|b| (b[0].s - 0.04).max(0.0)).collect();
    for (bi, b) in blocks.iter().enumerate() {
        let bs = starts[bi];
        let last_e = b.last().unwrap().e;
        let mut be = (last_e + 0.45).min(if bi + 1 < n { starts[bi + 1] } else { f64::MAX }).min(ctx.tl.total);
        // Hide under full-screen scenes.
        let mut skip = false;
        for (hs, he) in ctx.hide {
            if bs >= *hs - 0.01 && bs < *he {
                skip = true;
            } else if bs < *hs && be > *hs {
                be = *hs;
            }
        }
        if skip || be - bs < 0.06 {
            continue;
        }
        // ---- lay the words out in lines ----
        let mut lines: Vec<Vec<(usize, f64)>> = vec![vec![]];
        let mut line_w = 0.0;
        for (wi, w) in b.iter().enumerate() {
            let ts = if w.kw && kwmode != "none" { &kts } else { &base };
            let ww = ts.w(&w.text);
            let add = if lines.last().unwrap().is_empty() { ww } else { space + ww };
            if line_w + add > max_w && !lines.last().unwrap().is_empty() {
                lines.push(vec![]);
                line_w = ww;
            } else {
                line_w += add;
            }
            lines.last_mut().unwrap().push((wi, ww));
        }
        let widths: Vec<f64> = lines.iter().map(|l| l.iter().map(|x| x.1).sum::<f64>() + space * (l.len().saturating_sub(1)) as f64).collect();
        let block_w = widths.iter().cloned().fold(0.0, f64::max);
        let block_h = lh * lines.len() as f64;
        let pad_x = base.px * if bg == "pill" { 0.55 } else { 0.5 };
        let pad_y = base.px * if bg == "pill" { 0.12 } else { 0.34 };
        let x0 = if left { x_left } else { (cw_ - block_w) / 2.0 };
        // ---- place: wanted height, then clear of the face and of cards ----
        let mid = (bs + be) / 2.0;
        let mut y = y_want.min(ctx.safe_bottom - block_h - pad_y);
        let mut zone = "wanted";
        let rect_at = |y: f64| Rect { x0: x0 - pad_x, y0: y - pad_y, x1: x0 + block_w + pad_x, y1: y + block_h + pad_y };
        if let Some(f) = (ctx.face)(mid) {
            let guard = Rect { x0: f.x0, y0: f.y0 - 0.01 * doc.cv.h, x1: f.x1, y1: f.y1 + 0.02 * doc.cv.h };
            if rect_at(y).hits(&guard) {
                let below = guard.y1 + pad_y + 8.0 * sc;
                let above = guard.y0 - block_h - pad_y - 24.0 * sc;
                if below + block_h + pad_y <= ctx.safe_bottom {
                    y = below;
                    zone = "below-chin";
                } else if above - pad_y >= ctx.safe_top {
                    y = above;
                    zone = "above-head";
                } else {
                    y = ctx.safe_bottom - block_h - pad_y;
                    zone = "lowest";
                }
            }
        }
        for o in ctx.obstacles.iter().filter(|o| o.s < be && o.e > bs) {
            if rect_at(y).hits(&o.r) {
                let below = o.r.y1 + pad_y + 14.0 * sc;
                if below + block_h + pad_y <= ctx.safe_bottom {
                    y = below;
                    zone = "under-card";
                } else {
                    y = (o.r.y0 - block_h - pad_y - 14.0 * sc).max(ctx.safe_top);
                    zone = "over-card";
                }
            }
        }
        if let Some(p) = prev_y {
            if (p - y).abs() < 36.0 * sc && zone == "wanted" {
                y = p;
            }
        }
        prev_y = Some(y);
        let r = rect_at(y);
        log.push(json!({"s": bs, "e": be, "text": b.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" "), "zone": zone,
                        "rect": [r.x0.round(), r.y0.round(), r.x1.round(), r.y1.round()], "px": base.px.round()}));

        let mut placed: Vec<Placed> = vec![];
        for (li, line) in lines.iter().enumerate() {
            let mut x = if left { x_left } else { (cw_ - widths[li]) / 2.0 };
            let cy = y + lh * li as f64 + lh / 2.0;
            for (wi, ww) in line {
                let w = &b[*wi];
                let ts = if w.kw && kwmode != "none" { kts.clone() } else { base.clone() };
                let next_s = b.get(wi + 1).map(|n| n.s).unwrap_or(be);
                placed.push(Placed { text: w.text.clone(), cx: x + ww / 2.0, cy, w: *ww, ts, kw: w.kw, s: w.s.max(bs), e: next_s.max(w.s + 0.05).min(be) });
                x += ww + space;
            }
        }
        // ---- background ----
        let bg_color = jsd(cap, "bgColor", "#111111");
        let bg_op = jfd(cap, "bgAlpha", 0.85);
        let bg_start = if mode == "accumulate" { placed[0].s - 0.03 } else { bs };
        match bg.as_str() {
            "pill" => {
                for (li, _) in lines.iter().enumerate() {
                    let lx = if left { x_left } else { (cw_ - widths[li]) / 2.0 };
                    let ly = y + lh * li as f64;
                    doc.rect(0, bg_start, be, &Rect { x0: lx - pad_x, y0: ly + lh * 0.04, x1: lx + widths[li] + pad_x, y1: ly + lh * 0.96 }, lh, bg_color, bg_op, "\\fad(60,60)", false);
                }
            }
            "card" => doc.rect(0, bg_start, be, &r, base.px * 0.3, bg_color, bg_op, "\\fad(80,80)", false),
            "bar" => doc.rect(0, bg_start, be, &Rect { x0: 40.0 * sc, y0: r.y0, x1: cw_ - 40.0 * sc, y1: r.y1 }, 6.0 * sc, bg_color, bg_op, "\\fad(60,60)", false),
            _ => {}
        }
        // ---- the words ----
        let entry_tags = |t0: i64| -> String {
            match entry.as_str() {
                "punch" => format!("\\fscx128\\fscy128\\t({t0},{},\\fscx100\\fscy100)", t0 + 110),
                "fade" => format!("\\alpha&HFF&\\t({t0},{},\\alpha&H00&)", t0 + 140),
                "blur" => format!("\\alpha&HFF&\\blur{:.0}\\t({t0},{},\\alpha&H00&\\blur0)", 10.0 * sc, t0 + 240),
                _ => String::new(),
            }
        };
        for p in &placed {
            let (a0, a1) = (ms(p.s - bs), ms(p.e - bs));
            let pos = Pos::At(p.cx, p.cy);
            // Keyword decoration behind the word.
            let deco_s = if mode == "accumulate" { p.s } else { bs };
            if p.kw {
                let wr = Rect { x0: p.cx - p.w / 2.0, y0: p.cy - p.ts.px * 0.56, x1: p.cx + p.w / 2.0, y1: p.cy + p.ts.px * 0.56 };
                match kwmode.as_str() {
                    "box" => doc.rect(1, deco_s, be, &Rect { x0: wr.x0 - p.ts.px * 0.16, x1: wr.x1 + p.ts.px * 0.16, ..wr }, p.ts.px * 0.14, &accent, 1.0, "\\fscx0\\t(0,170,\\fscx100)", true),
                    "marker" => doc.rect(1, deco_s, be, &Rect { x0: wr.x0 - p.ts.px * 0.08, y0: p.cy - p.ts.px * 0.1, x1: wr.x1 + p.ts.px * 0.08, y1: p.cy + p.ts.px * 0.5 }, p.ts.px * 0.06, &accent, 0.92, "\\fscx0\\t(40,300,\\fscx100)", true),
                    "underline" => doc.rect(1, deco_s, be, &Rect { x0: wr.x0, y0: p.cy + p.ts.px * 0.56, x1: wr.x1, y1: p.cy + p.ts.px * 0.56 + (3.0 * sc).max(2.0) }, 0.0, &accent, 1.0, "\\fscx0\\t(60,320,\\fscx100)", true),
                    _ => {}
                }
            }
            let mut ts = p.ts.clone();
            if p.kw && kwmode == "marker" && fonts_dark(&accent) {
                ts.color = "#FFFFFF".into();
            }
            match mode.as_str() {
                "accumulate" => {
                    doc.text(2, (p.s - 0.03).max(bs), be, pos, 5, &ts, &entry_tags(0), &p.text);
                }
                "karaoke" => {
                    let mut dimmed = ts.clone();
                    dimmed.op = dim;
                    doc.text(2, bs, p.s, Pos::At(p.cx, p.cy), 5, &dimmed, "", &p.text);
                    let dur_cs = (((p.e - p.s).min(0.9)) * 100.0).round().max(4.0) as i64;
                    let fill = format!("\\1c{}\\2c{}\\2a{}\\K{dur_cs}", col(&accent), col(&ts.color), alpha(dim));
                    doc.text(2, p.s, be, pos, 5, &ts, &fill, &p.text);
                }
                "lines2" => {
                    doc.text(2, bs, be, Pos::Move(p.cx, p.cy + lh, p.cx, p.cy, 0, 160), 5, &ts, "", &p.text);
                }
                _ => {
                    let mut anim = entry_tags(0);
                    if dim < 1.0 {
                        anim.push_str(&format!("\\1a{}\\t({a0},{},\\1a&H00&)", alpha(dim), a0 + 60));
                    }
                    let base_c = col(&ts.color);
                    match active.as_str() {
                        "color" if !p.kw || kwmode == "none" => anim.push_str(&format!("\\t({a0},{},\\1c{})\\t({a1},{},\\1c{base_c})", a0 + 30, col(&accent), a1 + 30)),
                        "pop" => anim.push_str(&format!("\\t({a0},{},\\fscx113\\fscy113)\\t({},{},\\fscx100\\fscy100)", a0 + 70, a0 + 70, a0 + 210)),
                        "grow" => anim.push_str(&format!(
                            "\\t({a0},{},\\fscx120\\fscy120\\1c{}\\frz3)\\t({},{},\\frz-3)\\t({},{},\\frz0)\\t({a1},{},\\fscx100\\fscy100\\1c{base_c})",
                            a0 + 90, col(&accent), a0 + 90, a0 + 140, a0 + 140, a0 + 190, a1 + 90
                        )),
                        "box" => {
                            let wr = Rect { x0: p.cx - p.w / 2.0 - ts.px * 0.16, y0: p.cy - ts.px * 0.58, x1: p.cx + p.w / 2.0 + ts.px * 0.16, y1: p.cy + ts.px * 0.58 };
                            doc.rect(1, p.s, p.e, &wr, ts.px * 0.14, &accent, 1.0, "", false);
                            anim.push_str(&format!("\\t({a0},{a0},\\1c{}\\bord0\\shad0)\\t({a1},{a1},\\1c{base_c}\\bord{:.1}\\shad{:.1})", col(&on_accent), ts.bord, ts.shad));
                        }
                        _ => {}
                    }
                    doc.text(2, bs, be, pos, 5, &ts, &anim, &p.text);
                }
            }
        }
        // line-focus: the next line waits underneath.
        if mode == "lines2" {
            if let Some(nb) = blocks.get(bi + 1) {
                let text = nb.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ");
                let mut t = base.clone();
                t.op = 0.38;
                let t = t.fit(&text, max_w, base.px * 0.6);
                let nx = if left { x_left + t.w(&text) / 2.0 } else { cw_ / 2.0 };
                doc.text(2, bs, be, Pos::At(nx, y + block_h + lh / 2.0), 5, &t, "\\fad(120,0)", &text);
            }
        }
    }
    log
}

fn fonts_dark(hex: &str) -> bool {
    let h = hex.trim_start_matches('#');
    if h.len() < 6 {
        return false;
    }
    let c = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0) as f64 / 255.0;
    0.2126 * c(0) + 0.7152 * c(2) + 0.0722 * c(4) < 0.4
}
pub fn is_dark(hex: &str) -> bool {
    fonts_dark(hex)
}
