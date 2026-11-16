//! Reading the recording: pauses, sentences, retakes, pause-trimmed takes, highlight clips, the face track.
use crate::config::*;
use crate::proc::{self, Rep};
use crate::project::{Project, Word};
use crate::util::*;
use anyhow::{bail, Result};
use serde_json::{json, Value};

fn s(x: impl AsRef<str>) -> String {
    x.as_ref().to_string()
}

// ---------- pauses ----------

/// Silent stretches of the voice track -> build/silence.json as [[start, end], ...].
pub fn silences(cfg: &Config, p: &Project, noise_db: f64, min_s: f64) -> Result<Vec<(f64, f64)>> {
    let cache = p.p("build/silence.json");
    if let Ok(v) = read_json(&cache) {
        if jfd(&v, "noise", 0.0) == noise_db && jfd(&v, "min", 0.0) == min_s {
            return Ok(ja(&v, "silences").iter().filter_map(|x| Some((x.get(0)?.as_f64()?, x.get(1)?.as_f64()?))).collect());
        }
    }
    let ff = need_tool(cfg, "ffmpeg")?;
    let wav = p.p("assets/audio16k.wav");
    if !wav.exists() {
        return Ok(vec![]);
    }
    let err = proc::ffmpeg(&ff, &[s("-i"), fwd(&wav), s("-af"), format!("silencedetect=noise={noise_db}dB:d={min_s}"), s("-f"), s("null"), s("-")], None, 0.0, &Rep::none(), "pauses")?;
    let mut out = vec![];
    let mut start: Option<f64> = None;
    for line in err.lines() {
        if let Some(i) = line.find("silence_start:") {
            start = line[i + 14..].trim().split_whitespace().next().and_then(|x| x.parse().ok());
        } else if let Some(i) = line.find("silence_end:") {
            let end: Option<f64> = line[i + 12..].trim().split_whitespace().next().and_then(|x| x.parse().ok());
            if let (Some(a), Some(b)) = (start.take(), end) {
                out.push((a.max(0.0), b));
            }
        }
    }
    if let Some(a) = start {
        out.push((a, p.media().duration));
    }
    write_json(&cache, &json!({"noise": noise_db, "min": min_s, "silences": out.iter().map(|(a, b)| json!([a, b])).collect::<Vec<_>>()}))?;
    Ok(out)
}

// ---------- sentences ----------

#[derive(Clone, Debug)]
pub struct Sentence {
    pub w0: usize,
    pub w1: usize, // inclusive
    pub s: f64,
    pub e: f64,
    pub text: String,
}

pub fn sentences(words: &[Word]) -> Vec<Sentence> {
    let mut out = vec![];
    let mut start = 0usize;
    for i in 0..words.len() {
        let w = &words[i];
        let last = i + 1 == words.len();
        let gap = if last { 9.0 } else { words[i + 1].s - w.e };
        let ends = w.w.ends_with(['.', '?', '!', '…']);
        if last || ends || gap > 0.7 || i - start >= 45 {
            let text = words[start..=i].iter().map(|x| x.w.as_str()).collect::<Vec<_>>().join(" ");
            out.push(Sentence { w0: start, w1: i, s: words[start].s, e: w.e, text });
            start = i + 1;
        }
    }
    out
}

fn norm_tokens(text: &str) -> Vec<String> {
    text.split_whitespace().map(|t| t.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase()).filter(|t| !t.is_empty()).collect()
}

const RESTARTS: &[&str] = &["okay again", "ok again", "one more time", "let me start over", "start over", "sorry", "tekrar", "baştan", "bir daha", "pardon", "tekrar alıyorum", "yeniden"];
const FILLERS: &[&str] = &["um", "uh", "uhm", "erm", "hmm", "ııı", "ıı", "eee", "ee", "ııı.", "şey", "yani"];

// ---------- takes ----------

fn merge_ranges(mut v: Vec<(f64, f64)>, join: f64) -> Vec<(f64, f64)> {
    v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let mut out: Vec<(f64, f64)> = vec![];
    for (a, b) in v {
        match out.last_mut() {
            Some(l) if a - l.1 <= join => l.1 = l.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

pub struct AutocutOpts {
    pub from: f64,
    pub to: f64,
    pub noise_db: f64,
    pub min_pause: f64,
    pub pad_in: f64,
    pub pad_out: f64,
    pub fillers: bool,
}

/// Takes with the pauses removed, for a single clean take (or a range of a long recording).
/// Cuts come from the audio's own silences: whisper word times hide pauses.
pub fn autocut(cfg: &Config, p: &Project, o: &AutocutOpts) -> Result<Vec<Value>> {
    let dur = p.media().duration;
    let (from, to) = (o.from.max(0.0), if o.to > 0.0 { o.to.min(dur) } else { dur });
    let sil = silences(cfg, p, o.noise_db, 0.25)?;
    let words = p.words().map(|w| w.words).unwrap_or_default();
    // Speech islands = the range minus every pause longer than min_pause.
    let mut islands = vec![];
    let mut cur = from;
    for (a, b) in sil.iter().filter(|(a, b)| b - a >= o.min_pause && *b > from && *a < to) {
        if *a > cur {
            islands.push((cur, *a));
        }
        cur = cur.max(*b);
    }
    if cur < to {
        islands.push((cur, to));
    }
    let mut takes: Vec<(f64, f64)> = vec![];
    for (a, b) in islands {
        let has_word = words.is_empty() || words.iter().any(|w| (w.s + w.e) / 2.0 >= a - 0.15 && (w.s + w.e) / 2.0 <= b + 0.15);
        if b - a < 0.2 || (!has_word && b - a < 1.2) {
            continue; // a breath, a click, room noise
        }
        takes.push(((a - o.pad_in).max(from), (b + o.pad_out).min(to)));
    }
    let mut takes = merge_ranges(takes, 0.12);
    if o.fillers && !words.is_empty() {
        let mut out = vec![];
        for (a, b) in takes {
            let mut cur = a;
            for w in words.iter().filter(|w| w.s >= a && w.e <= b) {
                let t = w.w.to_lowercase();
                let t = t.trim_matches(|c: char| !c.is_alphanumeric());
                if FILLERS[..10].contains(&t) && w.e - w.s > 0.12 {
                    if w.s - cur > 0.25 {
                        out.push((cur, w.s - 0.02));
                    }
                    cur = w.e + 0.02;
                }
            }
            if b - cur > 0.25 {
                out.push((cur, b));
            }
        }
        takes = out;
    }
    if takes.is_empty() {
        bail!("no speech found between {} and {}", fmt_t(from), fmt_t(to));
    }
    Ok(takes.into_iter().map(|(a, b)| json!({"a": (a * 1000.0).round() / 1000.0, "b": (b * 1000.0).round() / 1000.0})).collect())
}

/// For a recording with retakes: keep the later, fluent attempt of every sentence.
/// Returns (takes, decisions) where decisions explain every dropped sentence.
pub fn retakes(words: &[Word], from: f64, to: f64) -> (Vec<Value>, Vec<Value>) {
    let sents: Vec<Sentence> = sentences(words).into_iter().filter(|x| x.s >= from && (to <= 0.0 || x.e <= to)).collect();
    let toks: Vec<Vec<String>> = sents.iter().map(|x| norm_tokens(&x.text)).collect();
    let mut drop: Vec<Option<String>> = vec![None; sents.len()];
    for i in 0..sents.len() {
        let low = sents[i].text.to_lowercase();
        if toks[i].len() <= 6 && RESTARTS.iter().any(|r| low.contains(r)) {
            drop[i] = Some("restart phrase".into());
            continue;
        }
        if toks[i].is_empty() {
            drop[i] = Some("no words".into());
            continue;
        }
        for j in i + 1..sents.len().min(i + 7) {
            if sents[j].s - sents[i].e > 45.0 || toks[j].is_empty() {
                continue;
            }
            let (a, b) = (&toks[i], &toks[j]);
            let n = a.len().min(b.len());
            let prefix = a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count();
            let common = a.iter().filter(|t| b.contains(t)).count();
            let jac = common as f64 / (a.len() + b.len() - common).max(1) as f64;
            let false_start = a.len() <= 4 && prefix == a.len() && b.len() > a.len();
            if false_start || (prefix >= 3 && prefix as f64 >= 0.5 * n as f64) || (jac >= 0.6 && a.len() >= 3) {
                drop[i] = Some(format!("{} of the sentence at {}", if false_start { "false start" } else { "earlier take" }, fmt_t(sents[j].s)));
                break;
            }
        }
    }
    let mut takes: Vec<(f64, f64)> = vec![];
    let mut decisions = vec![];
    for (i, x) in sents.iter().enumerate() {
        decisions.push(json!({"at": x.s, "to": x.e, "text": x.text, "keep": drop[i].is_none(), "why": drop[i].clone().unwrap_or_default()}));
        if drop[i].is_some() {
            continue;
        }
        let a = (x.s - 0.12).max(0.0);
        let next = sents.get(i + 1).map(|n| n.s).unwrap_or(f64::MAX);
        let b = (x.e + 0.25).min(next - 0.02).max(x.e + 0.04);
        match takes.last_mut() {
            Some(l) if a - l.1 < 0.35 && drop.get(i.wrapping_sub(1)).map(|d| d.is_none()).unwrap_or(false) => l.1 = b, // one breath
            _ => takes.push((a, b)),
        }
    }
    (takes.into_iter().map(|(a, b)| json!({"a": (a * 100.0).round() / 100.0, "b": (b * 100.0).round() / 100.0})).collect(), decisions)
}

// ---------- highlight clips ----------

pub const HOOKS: &[&str] = &[
    "why", "how", "never", "nobody", "everyone", "stop", "secret", "truth", "mistake", "biggest", "worst", "best", "here's", "imagine", "most people", "the reason", "what if",
    "neden", "nasıl", "asla", "hiç", "kimse", "herkes", "sır", "gerçek", "hata", "aslında", "en büyük", "en iyi", "en kötü", "şunu", "bakın", "düşünün", "dikkat", "çoğu insan", "bu yüzden",
];

pub struct ClipOpts {
    pub count: usize,
    pub min: f64,
    pub max: f64,
}

/// Score every window of whole sentences that fits the length and return the best, least overlapping ones.
pub fn find_clips(words: &[Word], o: &ClipOpts) -> Vec<Value> {
    let sents = sentences(words);
    if sents.is_empty() {
        return vec![];
    }
    let target = (o.min + o.max) / 2.0;
    let mut cands: Vec<(f64, usize, usize, Vec<&'static str>, [f64; 4])> = vec![];
    for i in 0..sents.len() {
        let first = &sents[i];
        let low = first.text.to_lowercase();
        let ftoks = norm_tokens(&first.text);
        if ftoks.len() < 3 {
            continue;
        }
        let mut hook = 0.0;
        let mut why: Vec<&'static str> = vec![];
        if first.text.contains('?') {
            hook += 2.0;
            why.push("opens on a question");
        }
        if HOOKS.iter().any(|h| low.contains(h)) {
            hook += 1.5;
            why.push("hook phrase in the first line");
        }
        if first.text.chars().any(|c| c.is_ascii_digit()) {
            hook += 1.0;
            why.push("a number up front");
        }
        if ftoks.len() <= 12 {
            hook += 0.5;
        }
        if ftoks.iter().any(|t| ["you", "your", "sen", "siz", "senin", "sizin"].contains(&t.as_str())) {
            hook += 0.5;
            why.push("speaks to the viewer");
        }
        let lead_gap = if i == 0 { 1.0 } else { first.s - sents[i - 1].e };
        if lead_gap >= 0.4 {
            hook += 0.5;
        }
        for j in i..sents.len() {
            let dur = sents[j].e - first.s;
            if dur > o.max {
                break;
            }
            if dur < o.min {
                continue;
            }
            let last = &sents[j];
            let mut score = hook;
            let mut flow = 0.0;
            let mut w = why.clone();
            if last.text.ends_with(['.', '?', '!']) {
                score += 1.0;
                flow += 1.0;
            }
            let tail_gap = sents.get(j + 1).map(|n| n.s - last.e).unwrap_or(2.0);
            if tail_gap >= 0.6 {
                score += 0.6;
                flow += 0.6;
                w.push("ends on a pause");
            }
            let nwords = (last.w1 + 1 - first.w0) as f64;
            let wps = nwords / dur;
            if (2.0..=3.8).contains(&wps) {
                score += 1.0;
                flow += 1.0;
                w.push("steady pace");
            } else if wps < 1.3 {
                score -= 1.0;
                flow -= 1.0;
            }
            let dead = (i..j).filter(|k| sents[k + 1].s - sents[*k].e > 1.5).count() as f64;
            score -= 0.5 * dead;
            flow -= 0.5 * dead;
            let body = sents[i..=j].iter().map(|x| x.text.to_lowercase()).collect::<Vec<_>>().join(" ");
            let emph = HOOKS.iter().filter(|h| body.contains(*h)).count().min(4) as f64 * 0.25 + body.chars().filter(|c| c.is_ascii_digit()).count().min(6) as f64 * 0.1;
            score += emph;
            let fit = 0.8 * (1.0 - ((dur - target).abs() / (o.max - o.min).max(1.0)).min(1.0));
            score += fit;
            let uniq = body.split_whitespace().collect::<std::collections::HashSet<_>>().len() as f64 / body.split_whitespace().count().max(1) as f64;
            let value = emph + uniq;
            cands.push((score, i, j, w, [hook, flow + fit, value, 0.0]));
        }
    }
    cands.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    let top = cands.first().map(|c| c.0).unwrap_or(1.0).max(1.0);
    let mut picked: Vec<(f64, usize, usize, Vec<&'static str>, [f64; 4])> = vec![];
    for c in cands {
        let (a, b) = (sents[c.1].s, sents[c.2].e);
        let overlaps = picked.iter().any(|p| {
            let (pa, pb) = (sents[p.1].s, sents[p.2].e);
            let inter = (b.min(pb) - a.max(pa)).max(0.0);
            inter > 0.3 * (b - a).min(pb - pa)
        });
        if !overlaps {
            picked.push(c);
            if picked.len() >= o.count {
                break;
            }
        }
    }
    picked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    picked
        .into_iter()
        .enumerate()
        .map(|(n, (score, i, j, why, parts))| {
            let text = sents[i..=j].iter().map(|x| x.text.as_str()).collect::<Vec<_>>().join(" ");
            let title: String = sents[i].text.trim_end_matches(['.', '!', ',']).chars().take(70).collect();
            json!({
                "id": format!("c{}", n + 1), "a": ((sents[i].s - 0.15).max(0.0) * 100.0).round() / 100.0, "b": ((sents[j].e + 0.3) * 100.0).round() / 100.0,
                "dur": ((sents[j].e - sents[i].s) * 10.0).round() / 10.0,
                "score": (40.0 + 59.0 * (score / top).clamp(0.0, 1.0)).round(),
                "title": title, "hook": sents[i].text, "why": why, "text": text, "by": "heuristic",
                "breakdown": {"hook": pct(parts[0], 4.5), "flow": pct(parts[1], 3.4), "value": pct(parts[2], 2.6), "trend": null}
            })
        })
        .collect()
}

// ---------- face track ----------

/// Where the speaker's face is, twice a second: [t, cx, cy, w, h] in 0..1 of the frame.
#[cfg(feature = "face")]
pub fn face_track(cfg: &Config, p: &Project, from: f64, to: f64, rep: &Rep) -> Result<Value> {
    let Some(model) = find_face_model() else {
        bail!("the face model is missing (seeta_fd_frontal_v1.0.bin in {}). Without it reels are framed on the centre; set crop.x by hand or install the model (see `doctor`).", models_dirs()[0].display());
    };
    let ff = need_tool(cfg, "ffmpeg")?;
    let m = p.media();
    let (from, to) = (from.max(0.0), if to > 0.0 { to.min(m.duration) } else { m.duration });
    let w = 384usize;
    let h = ((w as f64 * m.height as f64 / m.width.max(1) as f64 / 2.0).round() * 2.0) as usize;
    let rate = 2.0;
    let mut det = rustface::create_detector(&model.to_string_lossy()).map_err(|e| anyhow::anyhow!("face model: {e}"))?;
    det.set_min_face_size(28);
    det.set_score_thresh(2.0);
    det.set_pyramid_scale_factor(0.8);
    det.set_slide_window_step(4, 4);
    let mut samples: Vec<Value> = read_json_or(&p.p("build/face.json"), json!({}))["samples"].as_array().cloned().unwrap_or_default();
    samples.retain(|x| x[0].as_f64().map(|t| t < from - 0.01 || t > to + 0.01).unwrap_or(false));
    // In chunks, so a long range never sits in memory at once.
    let chunk: f64 = 60.0;
    let mut t0 = from;
    while t0 < to {
        rep.check()?;
        let d = chunk.min(to - t0);
        let raw = proc::ffmpeg_bytes(
            &ff,
            &[s("-ss"), format!("{t0:.3}"), s("-t"), format!("{d:.3}"), s("-i"), fwd(&p.source()?), s("-vf"), format!("fps={rate},scale={w}:{h},format=gray"), s("-f"), s("rawvideo"), s("-")],
        )?;
        for (i, frame) in raw.chunks_exact(w * h).enumerate() {
            let t = t0 + i as f64 / rate;
            let faces = det.detect(&rustface::ImageData::new(frame, w as u32, h as u32));
            let best = faces.iter().max_by(|a, b| {
                let (aa, ab) = (a.bbox().width() as f64 * a.bbox().height() as f64, b.bbox().width() as f64 * b.bbox().height() as f64);
                aa.partial_cmp(&ab).unwrap()
            });
            if let Some(f) = best {
                let bb = f.bbox();
                let r = |x: f64| (x * 1000.0).round() / 1000.0;
                samples.push(json!([r(t), r((bb.x() as f64 + bb.width() as f64 / 2.0) / w as f64), r((bb.y() as f64 + bb.height() as f64 / 2.0) / h as f64), r(bb.width() as f64 / w as f64), r(bb.height() as f64 / h as f64)]));
            }
        }
        t0 += d;
        rep.p((t0 - from) / (to - from).max(0.01), "tracking the face");
    }
    samples.sort_by(|a, b| a[0].as_f64().partial_cmp(&b[0].as_f64()).unwrap());
    let out = json!({"rate": rate, "samples": samples});
    write_json(&p.p("build/face.json"), &out)?;
    Ok(json!({"samples": out["samples"].as_array().map(|a| a.len()).unwrap_or(0), "from": from, "to": to}))
}

#[cfg(not(feature = "face"))]
pub fn face_track(_cfg: &Config, _p: &Project, _from: f64, _to: f64, _rep: &Rep) -> Result<Value> {
    bail!("this build has no face tracker")
}

/// The median face box over a stretch of the source: (cx, cy, w, h) in 0..1.
pub fn face_at(face: &Value, a: f64, b: f64) -> Option<(f64, f64, f64, f64)> {
    let v: Vec<&Value> = ja(face, "samples").iter().filter(|x| x[0].as_f64().map(|t| t >= a - 0.3 && t <= b + 0.3).unwrap_or(false)).collect();
    if v.is_empty() {
        return None;
    }
    let col = |i: usize| median(&mut v.iter().filter_map(|x| x[i].as_f64()).collect::<Vec<_>>());
    Some((col(1)?, col(2)?, col(3)?, col(4)?))
}

fn pct(x: f64, max: f64) -> f64 {
    (100.0 * (x / max).clamp(0.0, 1.0)).round()
}
