//! What the words don't say: how the voice rises (energy, pitch and how much the pitch moves), how much the
//! picture moves, what a vision model sees (a face pulled, a hand on the table, laughter, a graphic), who of
//! several people is speaking, and which moment of a clip makes the best hook.
use crate::analyze;
use crate::config::*;
use crate::llm;
use crate::proc::{self, Rep};
use crate::project::Project;
use crate::tools::{Ctx, Tool};
use crate::util::*;
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};

fn s(x: &str) -> String {
    x.to_string()
}

// ---------- audio and motion, per second ----------

pub fn signals(cfg: &Config, p: &Project, rep: &Rep) -> Result<Value> {
    let ff = need_tool(cfg, "ffmpeg")?;
    let dur = p.media().duration;
    let n = dur.ceil() as usize;
    let (mut rms, mut rms_max, mut pitch, mut pitch_sd) = (vec![-80.0f64; n], vec![-80.0f64; n], vec![0.0f64; n], vec![0.0f64; n]);
    if p.has("assets/audio16k.wav") {
        rep.p(0.05, "voice energy and pitch");
        let raw = proc::ffmpeg_bytes(&ff, &[s("-i"), fwd(&p.p("assets/audio16k.wav")), s("-ac"), s("1"), s("-ar"), s("8000"), s("-f"), s("s16le"), s("-")])?;
        let x: Vec<f32> = raw.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect();
        let win = 400; // 50 ms at 8 kHz
        let mut per: Vec<Vec<(f64, f64)>> = vec![vec![]; n];
        for (k, w) in x.chunks(win).enumerate() {
            let sec = (k * win / 8000).min(n.saturating_sub(1));
            let e = (w.iter().map(|v| (v * v) as f64).sum::<f64>() / w.len().max(1) as f64).sqrt();
            let db = 20.0 * e.max(1e-6).log10();
            // pitch by autocorrelation, 80..400 Hz, only on voiced windows
            let mut f0 = 0.0;
            if db > -40.0 && w.len() == win {
                let (mut best, mut lag) = (0.0f64, 0usize);
                let e0: f64 = w.iter().map(|v| (v * v) as f64).sum();
                for l in 20..100 {
                    let c: f64 = (0..win - l).map(|i| (w[i] * w[i + l]) as f64).sum();
                    if c > best {
                        best = c;
                        lag = l;
                    }
                }
                if lag > 0 && best > 0.35 * e0 {
                    f0 = 8000.0 / lag as f64;
                }
            }
            per[sec].push((db, f0));
        }
        for (i, v) in per.iter().enumerate() {
            if v.is_empty() {
                continue;
            }
            rms[i] = v.iter().map(|x| x.0).sum::<f64>() / v.len() as f64;
            rms_max[i] = v.iter().map(|x| x.0).fold(-80.0, f64::max);
            let f: Vec<f64> = v.iter().map(|x| x.1).filter(|f| *f > 0.0).collect();
            if f.len() >= 3 {
                let m = f.iter().sum::<f64>() / f.len() as f64;
                pitch[i] = m;
                pitch_sd[i] = (f.iter().map(|x| (x - m).powi(2)).sum::<f64>() / f.len() as f64).sqrt();
            }
        }
    }
    rep.p(0.5, "picture motion");
    let raw = proc::ffmpeg_bytes(&ff, &[s("-i"), fwd(&p.source()?), s("-vf"), s("fps=2,scale=64:36,format=gray"), s("-f"), s("rawvideo"), s("-")])?;
    let mut motion = vec![0.0f64; n];
    let fr: Vec<&[u8]> = raw.chunks_exact(64 * 36).collect();
    for k in 1..fr.len() {
        let d: f64 = fr[k].iter().zip(fr[k - 1]).map(|(a, b)| (*a as f64 - *b as f64).abs()).sum::<f64>() / (64.0 * 36.0);
        let sec = (k / 2).min(n.saturating_sub(1));
        motion[sec] = motion[sec].max(d);
    }
    let old = read_json_or(&p.p("build/signals.json"), json!({}));
    let r = |v: Vec<f64>| json!(v.into_iter().map(|x| (x * 10.0).round() / 10.0).collect::<Vec<_>>());
    let out = json!({"rate": 1, "rms": r(rms), "rms_max": r(rms_max), "pitch": r(pitch), "pitch_sd": r(pitch_sd), "motion": r(motion), "vision": old["vision"].clone()});
    write_json(&p.p("build/signals.json"), &out)?;
    rep.p(1.0, "cues");
    Ok(out)
}

fn series(sig: &Value, k: &str) -> Vec<f64> {
    ja(sig, k).iter().map(|v| v.as_f64().unwrap_or(f64::NAN)).collect()
}
fn z(v: &[f64]) -> Vec<f64> {
    let ok: Vec<f64> = v.iter().cloned().filter(|x| x.is_finite()).collect();
    if ok.len() < 2 {
        return vec![0.0; v.len()];
    }
    let m = ok.iter().sum::<f64>() / ok.len() as f64;
    let sd = (ok.iter().map(|x| (x - m).powi(2)).sum::<f64>() / ok.len() as f64).sqrt().max(1e-6);
    v.iter().map(|x| if x.is_finite() { (x - m) / sd } else { 0.0 }).collect()
}

/// Excitement per second: the voice rising, the pitch moving, the picture moving, what the eye model saw.
pub fn excitement(sig: &Value) -> Vec<f64> {
    let (a, b, c) = (z(&series(sig, "rms_max")), z(&series(sig, "pitch_sd")), z(&series(sig, "motion")));
    let vis = series(sig, "vision");
    let has_vis = vis.iter().any(|x| x.is_finite());
    let v = z(&vis);
    (0..a.len()).map(|i| 0.4 * a[i] + 0.3 * b.get(i).copied().unwrap_or(0.0) + 0.3 * c.get(i).copied().unwrap_or(0.0) + if has_vis { 0.6 * v.get(i).copied().unwrap_or(0.0) } else { 0.0 }).collect()
}

/// Add the emotional and visual cues to the clip scores, and re-rank.
pub fn rescore(clips: &mut Vec<Value>, sig: &Value) {
    let ex = excitement(sig);
    if ex.is_empty() {
        return;
    }
    for c in clips.iter_mut() {
        let (a, b) = (jfd(c, "a", 0.0) as usize, (jfd(c, "b", 0.0).ceil() as usize).min(ex.len()));
        if b <= a {
            continue;
        }
        let mut w: Vec<f64> = ex[a..b].to_vec();
        w.sort_by(|x, y| y.partial_cmp(x).unwrap());
        let top = w.iter().take(3).sum::<f64>() / w.len().min(3).max(1) as f64;
        let first = ex[a..(a + 3).min(b)].iter().cloned().fold(f64::MIN, f64::max);
        let emo = 100.0 / (1.0 + (-(top * 1.2)).exp());
        c["breakdown"]["emotion"] = json!(emo.round());
        let base = jfd(c, "score", 50.0);
        c["score"] = json!((0.75 * base + 0.25 * emo + if first > 1.0 { 4.0 } else { 0.0 }).round().min(99.0));
        if top > 1.2 {
            if let Some(arr) = c["why"].as_array_mut() {
                arr.push(json!("the voice or the picture peaks"));
            }
        }
    }
    clips.sort_by(|x, y| jfd(y, "score", 0.0).partial_cmp(&jfd(x, "score", 0.0)).unwrap());
}

// ---------- what a vision model sees ----------

fn t_vision(c: &Ctx, a: &Value) -> Result<Value> {
    let p = Project::open(&c.cfg, jsd(a, "project", ""))?;
    let ff = need_tool(&c.cfg, "ffmpeg")?;
    let dur = p.media().duration;
    let (from, to) = (jfd(a, "from", 0.0).max(0.0), jf(a, "to").unwrap_or(dur).min(dur));
    if to - from > 1800.0 && jb(a, "long") != Some(true) {
        bail!("that is {:.0} minutes of frames (one per second); pass long=true, or a from/to range", (to - from) / 60.0);
    }
    let sigf = p.p("build/signals.json");
    let mut sig = read_json_or(&sigf, json!({}));
    let n = dur.ceil() as usize;
    let mut vis: Vec<Value> = ja(&sig, "vision").to_vec();
    vis.resize(n, Value::Null);
    let dir = p.p("build/vision");
    std::fs::create_dir_all(&dir)?;
    let sheets = ((to - from) / 16.0).ceil() as usize;
    let mut notes = vec![];
    for k in 0..sheets {
        c.rep.check()?;
        let t0 = from + 16.0 * k as f64;
        let f = dir.join(format!("s{:06}.jpg", t0 as i64));
        proc::ffmpeg(&ff, &[s("-ss"), format!("{t0:.2}"), s("-t"), s("16"), s("-i"), fwd(&p.source()?), s("-vf"), s("fps=1,scale=256:-2,tile=4x4:padding=4"), s("-frames:v"), s("1"), s("-q:v"), s("4"), fwd(&f)], None, 0.0, &Rep::none(), "frames")?;
        let img = std::fs::read(&f)?;
        let ans = llm::complete_vision(
            &c.cfg,
            "You rate frames of a video for short-form editing. Score what would make a viewer stop scrolling: a strong facial expression, laughter, a gesture (a hand hitting the table, pointing), a sudden graphic or object, a big movement. A calm talking head scores 1-3.",
            "This sheet holds 16 frames, one per second, in reading order (left to right, top to bottom). Answer with JSON only: {\"scores\": [16 numbers 0-10], \"notes\": [{\"i\": index, \"what\": \"short\"}] for scores of 7 or more}.",
            &[img],
        )?;
        let v = llm::extract_json(&ans).ok_or_else(|| anyhow!("the model's answer could not be read"))?;
        for (i, sc) in ja(&v, "scores").iter().enumerate() {
            let sec = (t0 as usize) + i;
            if sec < n {
                vis[sec] = json!(sc.as_f64().unwrap_or(0.0));
            }
        }
        for nt in ja(&v, "notes") {
            notes.push(json!({"t": t0 + jfd(nt, "i", 0.0), "what": nt["what"]}));
        }
        c.rep.p((k + 1) as f64 / sheets as f64, &format!("vision {}/{}", k + 1, sheets));
    }
    sig["vision"] = json!(vis);
    write_json(&sigf, &sig)?;
    Ok(json!({"seconds": (to - from).round(), "moments": notes, "provider": llm::provider(&c.cfg), "note": "clips_find now uses these scores"}))
}

fn t_signals(c: &Ctx, a: &Value) -> Result<Value> {
    let p = Project::open(&c.cfg, jsd(a, "project", ""))?;
    let sig = signals(&c.cfg, &p, &c.rep)?;
    let ex = excitement(&sig);
    let mut peaks: Vec<(usize, f64)> = ex.iter().cloned().enumerate().collect();
    peaks.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    Ok(json!({"seconds": ex.len(), "peaks": peaks.iter().take(10).map(|(i, v)| json!({"t": i, "score": (v * 100.0).round() / 100.0})).collect::<Vec<_>>()}))
}

// ---------- who is speaking ----------

#[cfg(feature = "face")]
fn t_speakers(c: &Ctx, a: &Value) -> Result<Value> {
    let p = Project::open(&c.cfg, jsd(a, "project", ""))?;
    let model = find_face_model().ok_or_else(|| anyhow!("the face model is missing (Settings > Tools > download)"))?;
    let ff = need_tool(&c.cfg, "ffmpeg")?;
    let m = p.media();
    let (from, to) = (jfd(a, "from", 0.0).max(0.0), jf(a, "to").filter(|x| *x > 0.0).unwrap_or(m.duration).min(m.duration));
    let (w, fps) = (384usize, 6.0);
    let h = ((w as f64 * m.height as f64 / m.width.max(1) as f64 / 2.0).round() * 2.0) as usize;
    let mut det = rustface::create_detector(&model.to_string_lossy()).map_err(|e| anyhow!("face model: {e}"))?;
    det.set_min_face_size(24);
    det.set_score_thresh(2.0);
    det.set_pyramid_scale_factor(0.8);
    det.set_slide_window_step(4, 4);
    // audio energy per video frame
    let au = if p.has("assets/audio16k.wav") {
        proc::ffmpeg_bytes(&ff, &[s("-ss"), format!("{from:.3}"), s("-t"), format!("{:.3}", to - from), s("-i"), fwd(&p.p("assets/audio16k.wav")), s("-ac"), s("1"), s("-ar"), s("16000"), s("-f"), s("s16le"), s("-")])?
    } else {
        vec![]
    };
    let spf = (16000.0 / fps) as usize;
    let energy = |k: usize| -> f64 {
        let st = k * spf * 2;
        if st + spf * 2 > au.len() {
            return 0.0;
        }
        let x: f64 = au[st..st + spf * 2].chunks_exact(2).map(|b| (i16::from_le_bytes([b[0], b[1]]) as f64 / 32768.0).powi(2)).sum();
        (x / spf as f64).sqrt()
    };
    // tracks: (cx, cy, w, h) in px, frames seen, mouth motion per frame
    struct Tr {
        b: (f64, f64, f64, f64),
        seen: usize,
        mo: Vec<f64>,
    }
    let mut tracks: Vec<Tr> = vec![];
    let chunk: f64 = 60.0;
    let mut t0 = from;
    let mut k_all = 0usize;
    let mut prev: Option<Vec<u8>> = None;
    let mut en: Vec<f64> = vec![];
    while t0 < to {
        c.rep.check()?;
        let d = chunk.min(to - t0);
        let raw = proc::ffmpeg_bytes(&ff, &[s("-ss"), format!("{t0:.3}"), s("-t"), format!("{d:.3}"), s("-i"), fwd(&p.source()?), s("-vf"), format!("fps={fps},scale={w}:{h},format=gray"), s("-f"), s("rawvideo"), s("-")])?;
        for fr in raw.chunks_exact(w * h) {
            if k_all % 3 == 0 {
                for f in det.detect(&rustface::ImageData::new(fr, w as u32, h as u32)) {
                    let bb = f.bbox();
                    let b = (bb.x() as f64 + bb.width() as f64 / 2.0, bb.y() as f64 + bb.height() as f64 / 2.0, bb.width() as f64, bb.height() as f64);
                    match tracks.iter_mut().filter(|t| (t.b.0 - b.0).abs() < w as f64 * 0.12 && (t.b.1 - b.1).abs() < h as f64 * 0.15).min_by(|x, y| (x.b.0 - b.0).abs().partial_cmp(&(y.b.0 - b.0).abs()).unwrap()) {
                        Some(t) => {
                            t.b = (0.7 * t.b.0 + 0.3 * b.0, 0.7 * t.b.1 + 0.3 * b.1, 0.7 * t.b.2 + 0.3 * b.2, 0.7 * t.b.3 + 0.3 * b.3);
                            t.seen += 1;
                        }
                        None => tracks.push(Tr { b, seen: 1, mo: vec![0.0; k_all] }),
                    }
                }
            }
            for t in tracks.iter_mut() {
                let (cx, cy, bw, bh) = t.b;
                let (x0, x1) = ((cx - bw * 0.3).max(0.0) as usize, (cx + bw * 0.3).min(w as f64 - 1.0) as usize);
                let (y0, y1) = ((cy + bh * 0.1).max(0.0) as usize, (cy + bh * 0.55).min(h as f64 - 1.0) as usize);
                let mut sum = 0.0;
                let mut cnt = 0.0;
                if let Some(pv) = &prev {
                    for y in y0..y1 {
                        for x in x0..x1 {
                            sum += (fr[y * w + x] as f64 - pv[y * w + x] as f64).abs();
                            cnt += 1.0;
                        }
                    }
                }
                t.mo.push(if cnt > 0.0 { sum / cnt } else { 0.0 });
            }
            en.push(energy(k_all));
            prev = Some(fr.to_vec());
            k_all += 1;
        }
        t0 += d;
        c.rep.p((t0 - from) / (to - from).max(0.01), "who is speaking");
    }
    let detections = (k_all / 3).max(1);
    let keep: Vec<usize> = (0..tracks.len()).filter(|i| tracks[*i].seen as f64 > detections as f64 * 0.15).collect();
    if keep.len() < 2 {
        return Ok(json!({"faces": keep.len(), "note": "one person on screen: the normal face tracking already follows them"}));
    }
    let emax = en.iter().cloned().fold(1e-6, f64::max);
    let secs = (k_all as f64 / fps).ceil() as usize;
    let mut segs: Vec<(f64, f64, Vec<usize>)> = vec![];
    for sec in 0..secs {
        let fr0 = (sec as f64 * fps) as usize;
        let fr1 = (((sec + 1) as f64 * fps) as usize).min(k_all);
        let voice: f64 = (fr0..fr1).map(|k| en[k] / emax).sum::<f64>() / (fr1 - fr0).max(1) as f64;
        let mut sc: Vec<(usize, f64)> = keep.iter().enumerate().map(|(n, &i)| (n, (fr0..fr1).map(|k| tracks[i].mo.get(k).copied().unwrap_or(0.0) * (0.3 + en[k] / emax)).sum::<f64>())).collect();
        sc.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        let active = if voice < 0.05 {
            segs.last().map(|s| s.2.clone()).unwrap_or(vec![sc[0].0])
        } else if sc.len() > 1 && sc[1].1 > sc[0].1 * 0.7 {
            vec![sc[0].0, sc[1].0]
        } else {
            vec![sc[0].0]
        };
        let t = from + sec as f64;
        match segs.last_mut() {
            Some(l) if l.2 == active => l.1 = t + 1.0,
            _ => segs.push((t, t + 1.0, active)),
        }
    }
    // no shot shorter than 1.5 s: a short one joins its neighbour
    let mut merged: Vec<(f64, f64, Vec<usize>)> = vec![];
    for sg in segs {
        match merged.last_mut() {
            Some(l) if sg.1 - sg.0 < 1.5 || l.2 == sg.2 => l.1 = sg.1,
            _ => merged.push(sg),
        }
    }
    let faces: Vec<Value> = keep.iter().enumerate().map(|(n, &i)| { let b = tracks[i].b; json!({"id": n, "cx": b.0 / w as f64, "cy": b.1 / h as f64, "w": b.2 / w as f64, "h": b.3 / h as f64}) }).collect();
    let out = json!({"faces": faces, "segments": merged.iter().map(|(a, b, v)| json!({"t0": a, "t1": b, "active": v})).collect::<Vec<_>>()});
    write_json(&p.p("build/speakers.json"), &out)?;
    Ok(json!({"faces": keep.len(), "segments": merged.len(), "note": "set crop.mode to speaker: the frame follows whoever speaks and splits top/bottom when two react together"}))
}
#[cfg(not(feature = "face"))]
fn t_speakers(_c: &Ctx, _a: &Value) -> Result<Value> {
    bail!("this build has no face detector")
}

// ---------- the hook ----------

/// The strongest few seconds of a range: a hook phrase, a question, a number, and the voice/picture peaking.
pub fn best_hook(p: &Project, a: f64, b: f64) -> Option<(f64, f64, String)> {
    let words = p.words().ok()?.words;
    let sents: Vec<analyze::Sentence> = analyze::sentences(&words).into_iter().filter(|s| s.s >= a && s.e <= b).collect();
    if sents.len() < 2 {
        return None;
    }
    let ex = excitement(&read_json_or(&p.p("build/signals.json"), json!({})));
    let mut best: Option<(f64, usize)> = None;
    for (i, s) in sents.iter().enumerate().skip(1) {
        let low = s.text.to_lowercase();
        let mut sc = 0.0;
        if analyze::HOOKS.iter().any(|h| low.contains(h)) {
            sc += 2.0;
        }
        if s.text.contains('?') {
            sc += 1.5;
        }
        if s.text.chars().any(|c| c.is_ascii_digit()) {
            sc += 1.0;
        }
        if s.text.ends_with('!') {
            sc += 0.8;
        }
        let (x, y) = (s.s as usize, (s.e.ceil() as usize).min(ex.len()));
        if y > x {
            sc += ex[x..y].iter().cloned().fold(f64::MIN, f64::max).max(0.0);
        }
        if best.map(|b| sc > b.0).unwrap_or(true) {
            best = Some((sc, i));
        }
    }
    let s = &sents[best?.1];
    // at most ~3.5 s, ending on a word
    let mut end = s.e;
    for wd in words[s.w0..=s.w1].iter() {
        if wd.e - s.s > 3.5 {
            break;
        }
        end = wd.e;
    }
    if end - s.s < 1.0 {
        end = s.e.min(s.s + 3.5);
    }
    let text: String = words[s.w0..=s.w1].iter().filter(|w| w.e <= end + 0.01).map(|w| w.w.as_str()).collect::<Vec<_>>().join(" ");
    Some((((s.s - 0.08).max(0.0) * 100.0).round() / 100.0, ((end + 0.18) * 100.0).round() / 100.0, text))
}

/// Put a hook at the start of the reel: those seconds play first, then the clip from its beginning.
pub fn add_hook(p: &Project, rid: &str, span: Option<(f64, f64)>) -> Result<Value> {
    let mut reel = p.load_reel(rid)?;
    let mut takes: Vec<Value> = ja(&reel, "takes").iter().filter(|t| jsd(t, "note", "") != "hook").cloned().collect();
    let (ca, cb) = (takes.first().map(|t| jfd(t, "a", 0.0)).unwrap_or(0.0), takes.last().map(|t| jfd(t, "b", 0.0)).unwrap_or(p.media().duration));
    let (a, b, text) = match span {
        Some((a, b)) => (a, b, String::new()),
        None => best_hook(p, ca, cb).ok_or_else(|| anyhow!("no hook found in this reel (it needs a transcript and two sentences or more)"))?,
    };
    takes.insert(0, json!({"a": a, "b": b, "note": "hook"}));
    reel["takes"] = json!(takes);
    let mut beats: Vec<Value> = ja(&reel, "beats").iter().filter(|x| jsd(x, "note", "") != "hook").cloned().collect();
    if !text.is_empty() {
        let short: String = text.split_whitespace().take(5).collect::<Vec<_>>().join(" ").trim_end_matches([',', '.']).to_string();
        beats.insert(0, json!({"type": "title", "text": short.to_uppercase(), "at": "edit:0.05", "to": format!("edit:{:.2}", (b - a).max(1.0)), "y": 260, "size": 112, "anim": "pop", "note": "hook", "sfx": ["whoosh-cine"]}));
    }
    reel["beats"] = json!(beats);
    p.save_reel(rid, &reel)?;
    Ok(json!({"hook": {"a": a, "b": b, "text": text}, "note": "the hook plays first, then the clip from its start; edit or remove it as the first take (note: hook) and the first beat"}))
}

fn t_hook(c: &Ctx, a: &Value) -> Result<Value> {
    let p = Project::open(&c.cfg, jsd(a, "project", ""))?;
    let rid = p.rid(js(a, "reel"));
    if jb(a, "remove") == Some(true) {
        let mut reel = p.load_reel(&rid)?;
        reel["takes"] = json!(ja(&reel, "takes").iter().filter(|t| jsd(t, "note", "") != "hook").cloned().collect::<Vec<_>>());
        reel["beats"] = json!(ja(&reel, "beats").iter().filter(|x| jsd(x, "note", "") != "hook").cloned().collect::<Vec<_>>());
        p.save_reel(&rid, &reel)?;
        return Ok(json!({"removed": true}));
    }
    let span = match (jf(a, "a"), jf(a, "b")) {
        (Some(x), Some(y)) if y > x => Some((x, y)),
        _ => None,
    };
    add_hook(&p, &rid, span)
}

const P: (&str, &str, &str) = ("project!", "string", "Project");

pub const TOOLS: &[Tool] = &[
    Tool { name: "signals", desc: "Read the cues the words don't carry, per second: voice energy, pitch and how much it moves (excitement), and picture motion. clips_find uses them; returns the strongest moments.", params: &[P], long: true, app_only: false, run: t_signals },
    Tool { name: "vision_score", desc: "Let a vision model (Claude, OpenAI, or a local llava-style model) look at one frame a second and score visual action: expressions, laughter, gestures, a hand on the table, sudden graphics. clips_find uses the scores.", params: &[P, ("from", "number", "Start s"), ("to", "number", "End s"), ("long", "boolean", "Allow more than 30 minutes")], long: true, app_only: false, run: t_vision },
    Tool { name: "speakers", desc: "Active speaker detection for podcasts and interviews: tracks every face and matches mouth movement to the voice, second by second. Then crop.mode=speaker follows whoever speaks and splits the frame top/bottom when two react together.", params: &[P, ("from", "number", "Start s"), ("to", "number", "End s")], long: true, app_only: false, run: t_speakers },
    Tool { name: "hook_add", desc: "Hook: copy the strongest 2-3.5 s of the reel (a hook phrase, question, number, the voice or picture peaking) to the very start, with a big title and a whoosh; the clip then plays from its beginning. a/b pick the span by hand; remove=true takes it out.", params: &[P, ("reel", "string", "Reel id"), ("a", "number", "Start s (original)"), ("b", "number", "End s"), ("remove", "boolean", "Remove the hook")], long: false, app_only: false, run: t_hook },
];
