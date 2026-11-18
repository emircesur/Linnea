//! Films written as code, on VideoHub's project contract (docs/PROJECT-CONTRACT.md, MIT):
//! a page with `window.renderOut(t)` + `window.READY`, `window.VH_SCENES`, `.videohub/` for notes, versions
//! and settings, `render/chunks/c_NNN.mp4` chunks and `<base>_vN.mp4` versions. Claude writes the film,
//! Linnea plays it live from its code, the user leaves notes on frames, Claude fixes them, and the user
//! renders when ready (only the chunks that changed, if they like).
use crate::cdp::Page;
use crate::config::*;
use crate::media::h264_encoder;
use crate::proc::{self, Rep};
use crate::tools::{Ctx, Tool};
use crate::util::*;
use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

const SKIP: &[&str] = &["render", "out", "snaps", "node_modules", ".videohub", "tmp", "profiles", "vh2d", "vhaudio", ".git", "tools", "fonts", "videos", "thumbnails"];
// puppet files count as code: a film redraws when one of its puppets changes
const CODE: &[&str] = &["html", "htm", "js", "mjs", "css", "json", "svg", "glsl", "frag", "vert", "txt", "ppet", "rppet"];
const VIDEO: &[&str] = &["mp4", "mov", "webm", "mkv"];

pub struct Film {
    pub id: String,
    pub dir: PathBuf,
}

fn ext(p: &Path) -> String {
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}
fn skipped(name: &str) -> bool {
    name.starts_with('.') || name.starts_with('_') || name.starts_with("chunks") || SKIP.contains(&name)
}

pub fn films_dir(cfg: &Config) -> PathBuf {
    cfg.workspace().join("films")
}

/// Container roots (every sub-folder is a film) and single-film roots, from settings.
fn roots(cfg: &Config) -> Vec<(PathBuf, bool)> {
    let mut v = vec![(films_dir(cfg), true)];
    for r in cfg.get("films.roots").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
        let p = PathBuf::from(r.as_str().map(|s| s.to_string()).unwrap_or_else(|| jsd(&r, "path", "").to_string()));
        v.push((p, jsd(&r, "type", "container") != "project"));
    }
    v
}

fn is_film(d: &Path) -> bool {
    if d.join(".videohub").join("notes.json").exists() || d.join(".videohub").join("project.json").exists() || d.join("videohub-project.json").exists() {
        return true;
    }
    for f in ["index.html", "render.js", "render.mjs", "render.py", "make.mjs", "STATE.md"] {
        if d.join(f).exists() {
            return true;
        }
    }
    std::fs::read_dir(d).map(|rd| rd.flatten().any(|e| VIDEO.contains(&ext(&e.path()).as_str()))).unwrap_or(false)
}

fn index() -> &'static Mutex<HashMap<String, PathBuf>> {
    static I: OnceLock<Mutex<HashMap<String, PathBuf>>> = OnceLock::new();
    I.get_or_init(|| Mutex::new(HashMap::new()))
}

fn scan(cfg: &Config) -> Vec<Film> {
    let mut out: Vec<Film> = vec![];
    for (root, container) in roots(cfg) {
        let dirs: Vec<PathBuf> = if container {
            std::fs::read_dir(&root).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir() && !skipped(&p.file_name().unwrap_or_default().to_string_lossy())).collect()).unwrap_or_default()
        } else {
            vec![root.clone()]
        };
        for d in dirs {
            if !is_film(&d) {
                continue;
            }
            let base = slug(&d.file_name().unwrap_or_default().to_string_lossy());
            let mut id = base.clone();
            let mut n = 2;
            while out.iter().any(|f| f.id == id) {
                id = format!("{base}-{n}");
                n += 1;
            }
            out.push(Film { id, dir: d });
        }
    }
    let mut ix = index().lock().unwrap();
    ix.clear();
    for f in &out {
        ix.insert(f.id.clone(), f.dir.clone());
    }
    out
}

pub fn open(cfg: &Config, key: &str) -> Result<Film> {
    if key.is_empty() {
        bail!("no film given; `hub_films` lists them");
    }
    let p = Path::new(key);
    if p.is_absolute() && p.is_dir() {
        if let Some(f) = scan(cfg).into_iter().find(|f| f.dir.canonicalize().ok() == p.canonicalize().ok()) {
            return Ok(f);
        }
        return Ok(Film { id: slug(&p.file_name().unwrap_or_default().to_string_lossy()), dir: p.to_path_buf() });
    }
    if let Some(d) = index().lock().unwrap().get(key).cloned() {
        return Ok(Film { id: key.into(), dir: d });
    }
    let all = scan(cfg);
    let k = key.to_lowercase();
    all.into_iter().find(|f| f.id == key || title_of(f).to_lowercase() == k).ok_or_else(|| anyhow!("film '{key}' not found. `hub_films` lists them; a folder outside {} can be added in settings (films.roots)", films_dir(cfg).display()))
}

pub fn dir_of(id: &str) -> Option<PathBuf> {
    let hit = index().lock().unwrap().get(id).cloned();
    hit.or_else(|| {
        scan(&Config::load());
        index().lock().unwrap().get(id).cloned()
    })
}

// ---------- the contract ----------

pub fn meta(f: &Film) -> Value {
    let mut m = read_json_or(&f.dir.join("videohub-project.json"), json!({}));
    merge(&mut m, &read_json_or(&f.dir.join(".videohub").join("project.json"), json!({})));
    m
}
fn title_of(f: &Film) -> String {
    let m = meta(f);
    js(&m, "title").map(|s| s.to_string()).unwrap_or_else(|| f.dir.file_name().unwrap_or_default().to_string_lossy().to_string())
}
fn vh(f: &Film, rel: &str) -> PathBuf {
    f.dir.join(".videohub").join(rel)
}

fn find_in(dir: &Path, depth: u32, pick: &dyn Fn(&Path) -> bool) -> Option<PathBuf> {
    let rd = std::fs::read_dir(dir).ok()?;
    let mut sub = vec![];
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if depth > 0 && !skipped(&e.file_name().to_string_lossy()) {
                sub.push(p);
            }
        } else if pick(&p) {
            return Some(p);
        }
    }
    sub.into_iter().find_map(|d| find_in(&d, depth - 1, pick))
}

pub fn audio_len(cfg: &Config, p: &Path) -> Option<f64> {
    proc::probe(&find_tool(cfg, "ffprobe")?, p).ok().map(|m| m.duration)
}

pub fn info(cfg: &Config, f: &Film) -> Value {
    let m = meta(f);
    let live = &m["live"];
    let page = js(live, "page").map(|s| s.to_string()).or_else(|| js(&m["render"]["env"], "PAGE").map(|s| s.to_string())).unwrap_or_else(|| "index.html".into());
    let html = std::fs::read_to_string(f.dir.join(&page)).unwrap_or_default();
    // size: live -> data.json -> the largest <canvas width height> in the page -> 1920x1080
    let data = read_json_or(&f.dir.join("data.json"), json!({}));
    let mut size = (jf(live, "width").or(jf(&data, "width")), jf(live, "height").or(jf(&data, "height")));
    if size.0.is_none() {
        let mut best = 0.0;
        for tag in html.split("<canvas").skip(1) {
            let tag = tag.split('>').next().unwrap_or("");
            let attr = |k: &str| tag.split(&format!("{k}=")).nth(1).and_then(|r| r.trim_start_matches(['"', '\'']).split(|c: char| !c.is_ascii_digit()).next()?.parse::<f64>().ok());
            if let (Some(w), Some(h)) = (attr("width"), attr("height")) {
                if w * h > best {
                    best = w * h;
                    size = (Some(w), Some(h));
                }
            }
        }
    }
    let (w, h) = (size.0.unwrap_or(1920.0), size.1.unwrap_or(1080.0));
    let fps = jf(live, "fps").or_else(|| js(&m["render"]["env"], "FPS").and_then(|s| s.parse().ok())).or(jf(&m["render"], "fps")).unwrap_or(30.0);
    let audio = js(live, "audio").map(|a| f.dir.join(a)).filter(|p| p.is_file()).or_else(|| {
        ["audio/mix.wav", "audio/mix.mp3", "audio/final.wav", "audio/master.wav", "mix.wav", "audio/mix.m4a"].iter().map(|a| f.dir.join(a)).find(|p| p.is_file())
    });
    let dur = jf(live, "dur")
        .or_else(|| js(&m["render"]["env"], "DUR").and_then(|s| s.parse().ok()))
        .or(jf(&m["render"]["env"], "DUR"))
        .or_else(|| audio.as_ref().and_then(|a| audio_len(cfg, a)))
        .unwrap_or(60.0);
    let words = js(&m["transcript"], "file").map(|p| f.dir.join(p)).filter(|p| p.is_file()).or_else(|| find_in(&f.dir, 3, &|p| p.file_name().map(|n| n.to_string_lossy().starts_with("words") && ext(p) == "json").unwrap_or(false)));
    let ch = (2.0 * fps).round().max(1.0);
    json!({
        "id": f.id, "title": title_of(f), "dir": fwd(&f.dir), "page": page, "url": format!("/live/{}/{}", f.id, page),
        "width": w, "height": h, "fps": fps, "dur": dur, "frames": (dur * fps).round(), "chunk_frames": ch, "chunks": ((dur * fps) / ch).ceil(),
        "audio": audio.map(|a| fwd(&a)), "words": words.map(|w| fwd(&w)), "status": m["status"], "scenes": read_json_or(&vh(f, "scenes.json"), json!([])),
        "has_page": f.dir.join(&page).is_file(), "dur_known": jf(live, "dur").is_some() || m["render"]["env"]["DUR"].is_string() || m["render"]["env"]["DUR"].is_number(),
    })
}

/// Newest change to the film's code (ms). The preview reloads when it moves.
pub fn rev(f: &Film) -> u64 {
    fn walk(d: &Path, depth: u32, best: &mut u64, n: &mut u32) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            *n += 1;
            if *n > 4000 {
                return;
            }
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                if depth > 0 && !skipped(&name) {
                    walk(&p, depth - 1, best, n);
                }
            } else if CODE.contains(&ext(&p).as_str()) && name != "tweaks.json" {
                if let Ok(t) = e.metadata().and_then(|m| m.modified()) {
                    let ms = t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
                    *best = (*best).max(ms);
                }
            }
        }
    }
    let mut best = 0;
    let mut n = 0;
    walk(&f.dir, 4, &mut best, &mut n);
    best
}

fn words_at(info: &Value, t: f64) -> String {
    let Some(p) = js(info, "words") else { return String::new() };
    let v = read_json_or(Path::new(p), json!([]));
    let arr = v.as_array().cloned().or_else(|| v["words"].as_array().cloned()).unwrap_or_default();
    let mut out = vec![];
    for w in arr {
        let (s, e, txt) = if let Some(a) = w.as_array() {
            (a.first().and_then(|x| x.as_f64()).unwrap_or(-1.0), a.get(1).and_then(|x| x.as_f64()).unwrap_or(-1.0), a.get(2).and_then(|x| x.as_str()).unwrap_or("").to_string())
        } else {
            (jf(&w, "s").or(jf(&w, "start")).unwrap_or(-1.0), jf(&w, "e").or(jf(&w, "end")).unwrap_or(-1.0), js(&w, "w").or(js(&w, "word")).or(js(&w, "text")).unwrap_or("").to_string())
        };
        if e >= t - 1.5 && s <= t + 1.5 {
            out.push(txt);
        }
    }
    out.join(" ")
}

// ---------- frames ----------

fn snap_page() -> &'static Mutex<Option<(String, Page)>> {
    static P: OnceLock<Mutex<Option<(String, Page)>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(None))
}

/// Close the browser kept open for frames. A one-shot process (the terminal) calls this before it exits:
/// a static is never dropped, and the browser would outlive it (holding the terminal's output open).
pub fn close_pages() {
    if let Ok(mut s) = snap_page().lock() {
        *s = None;
    }
}

pub fn page_url(cfg: &Config, f: &Film, info: &Value) -> String {
    let port = read_json_or(&config_dir().join("server.json"), json!({}))["port"].as_u64().filter(|_| crate::server::running()).unwrap_or(0);
    if port > 0 {
        format!("http://127.0.0.1:{port}{}", jsd(info, "url", ""))
    } else {
        // No server in this process (the terminal): open the file straight from disk.
        let _ = cfg;
        format!("file:///{}", fwd(&f.dir.join(jsd(info, "page", "index.html"))).trim_start_matches('/'))
    }
}

/// Exact frames of the film at these seconds, as JPEG files under .videohub/snaps.
pub fn frames(cfg: &Config, f: &Film, times: &[f64], scale: f64) -> Result<Vec<PathBuf>> {
    let info = info(cfg, f);
    let (w, h) = (jfd(&info, "width", 1920.0) as u32, jfd(&info, "height", 1080.0) as u32);
    let key = format!("{}@{}x{}#{}", f.id, w, h, rev(f));
    let mut slot = snap_page().lock().unwrap_or_else(|e| e.into_inner());
    if slot.as_ref().map(|(k, _)| *k != key).unwrap_or(true) {
        *slot = None;
        let mut p = Page::open(cfg, &page_url(cfg, f, &info), w, h, false)?;
        p.wait_ready(Duration::from_secs(45))?;
        *slot = Some((key, p));
    }
    let page = &mut slot.as_mut().unwrap().1;
    let dir = vh(f, "snaps");
    std::fs::create_dir_all(&dir)?;
    let mut out = vec![];
    let fps = jfd(&info, "fps", 30.0);
    for &t in times {
        let t = clampf((t * fps).round() / fps, 0.0, jfd(&info, "dur", 60.0));
        let img = match page.frame(t, 90, scale) {
            Ok(i) => i,
            Err(e) => {
                *slot = None;
                return Err(e);
            }
        };
        let p = dir.join(format!("t{:08.3}.jpg", t));
        std::fs::write(&p, img)?;
        out.push(p);
    }
    Ok(out)
}

fn sheet(cfg: &Config, files: &[PathBuf], out: &Path) -> Result<()> {
    let ff = need_tool(cfg, "ffmpeg")?;
    let tmp = out.with_extension("d");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;
    for (i, f) in files.iter().enumerate() {
        std::fs::copy(f, tmp.join(format!("{i:03}.jpg")))?;
    }
    let cols = (files.len() as f64).sqrt().ceil().max(1.0) as usize;
    let rows = files.len().div_ceil(cols);
    let r = proc::run(proc::cmd(&ff).args(["-hide_banner", "-y", "-v", "error", "-i"]).arg(tmp.join("%03d.jpg")).args(["-vf", &format!("scale=480:-2,tile={cols}x{rows}:padding=6:color=0x111111"), "-frames:v", "1", "-q:v", "3"]).arg(out));
    let _ = std::fs::remove_dir_all(&tmp);
    match r {
        Ok(o) if o.ok => Ok(()),
        Ok(o) => bail!("contact sheet: {}", proc::err_tail(&o.stderr, 3)),
        Err(e) => Err(e),
    }
}

// ---------- notes ----------

pub fn notes(f: &Film) -> Vec<Value> {
    read_json_or(&vh(f, "notes.json"), json!([])).as_array().cloned().unwrap_or_default()
}
fn save_notes(f: &Film, n: &[Value]) -> Result<()> {
    write_json(&vh(f, "notes.json"), &json!(n))
}

fn chunks_for(info: &Value, a: f64, b: f64) -> Vec<usize> {
    let fps = jfd(info, "fps", 30.0);
    let ch = jfd(info, "chunk_frames", 60.0);
    let last = (jfd(info, "chunks", 1.0) as usize).max(1) - 1;
    let c0 = (((a - 0.5).max(0.0) * fps) / ch).floor() as usize;
    let c1 = (((b + 0.5) * fps) / ch).floor() as usize;
    (c0.min(last)..=c1.min(last)).collect()
}

/// A note on a frame: the exact frame, the user's drawing over it, and a close-up of the drawing.
pub fn add_note(cfg: &Config, f: &Film, a: &Value, by: &str) -> Result<Value> {
    let info = info(cfg, f);
    let fps = jfd(&info, "fps", 30.0);
    let t = (jfd(a, "t", 0.0) * fps).round() / fps;
    let t1 = jf(a, "t1").filter(|x| *x > t);
    let id = new_id("n");
    let dir = vh(f, &format!("notes/{id}"));
    std::fs::create_dir_all(&dir)?;
    let mut images = json!({});
    // A note on a rendered version takes its frame from that video; a live note, from the page.
    let grabbed: Result<PathBuf> = match js(a, "video") {
        Some(v) => {
            let out = dir.join("frame.jpg");
            let ff = need_tool(cfg, "ffmpeg")?;
            let o = proc::run(proc::cmd(&ff).args(["-hide_banner", "-y", "-v", "error", "-ss", &format!("{t:.3}"), "-i", v, "-frames:v", "1", "-q:v", "3"]).arg(&out))?;
            if o.ok { Ok(out) } else { Err(anyhow!("frame from the video: {}", proc::err_tail(&o.stderr, 2))) }
        }
        None => frames(cfg, f, &[t], 1.0).map(|v| v[0].clone()),
    };
    if let Ok(fr0) = grabbed {
        let frame = dir.join("frame.jpg");
        if fr0 != frame {
            std::fs::copy(&fr0, &frame)?;
        }
        images["frame"] = json!(fwd(&frame));
        if let Some(d) = js(a, "drawing").and_then(|d| d.split_once("base64,").map(|x| x.1.to_string())) {
            let png = dir.join("drawing.png");
            std::fs::write(&png, base64::engine::general_purpose::STANDARD.decode(d.trim())?)?;
            let ff = need_tool(cfg, "ffmpeg")?;
            let (w, h) = (jfd(&info, "width", 1920.0), jfd(&info, "height", 1080.0));
            let ann = dir.join("annotated.jpg");
            let o = proc::run(proc::cmd(&ff).args(["-hide_banner", "-y", "-v", "error", "-i"]).arg(&frame).arg("-i").arg(&png).args(["-filter_complex", &format!("[1]scale={w}:{h}[d];[0][d]overlay=0:0"), "-q:v", "3"]).arg(&ann))?;
            if o.ok {
                images["annotated"] = json!(fwd(&ann));
                if let Some(b) = a["box"].as_array().filter(|b| b.len() == 4) {
                    let v: Vec<f64> = b.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect();
                    let pad = 0.08 * w.max(h);
                    let cw = (v[2] - v[0] + 2.0 * pad).max(0.3 * w.min(h)).min(w);
                    let chh = (v[3] - v[1] + 2.0 * pad).max(0.3 * w.min(h)).min(h);
                    let cx = clampf((v[0] + v[2]) / 2.0 - cw / 2.0, 0.0, w - cw);
                    let cy = clampf((v[1] + v[3]) / 2.0 - chh / 2.0, 0.0, h - chh);
                    let crop = dir.join("crop.jpg");
                    let o = proc::run(proc::cmd(&ff).args(["-hide_banner", "-y", "-v", "error", "-i"]).arg(&ann).args(["-vf", &format!("crop={}:{}:{}:{}", cw as i64, chh as i64, cx as i64, cy as i64), "-q:v", "3"]).arg(&crop))?;
                    if o.ok {
                        images["crop"] = json!(fwd(&crop));
                    }
                }
            }
        }
    }
    let note = json!({
        "id": id, "t": t, "t1": t1, "frame": (t * fps).round(), "text": jsd(a, "text", ""), "status": "open", "reply": "", "by": by,
        "created": now_ms(), "chunks": chunks_for(&info, t, t1.unwrap_or(t)), "narration": words_at(&info, t), "images": images, "box": a["box"],
    });
    let mut n = notes(f);
    n.push(note.clone());
    save_notes(f, &n)?;
    Ok(note)
}

// ---------- versions and render ----------

pub fn versions(f: &Film) -> Vec<Value> {
    read_json_or(&vh(f, "versions.json"), json!([])).as_array().cloned().unwrap_or_default()
}

fn base_name(f: &Film) -> String {
    // a language version's file carries its code: the name comes from the latest original one
    let latest = versions(f).iter().rev().find(|v| v["lang"].is_null()).and_then(|v| js(v, "file").map(|s| s.to_string()));
    let b = latest.as_deref().and_then(|p| Path::new(p).file_stem().map(|s| s.to_string_lossy().to_string())).map(|s| {
        let s = s.replace("_preview", "").replace("_web", "");
        match s.rfind("_v") {
            Some(i) if s[i + 2..].chars().all(|c| c.is_ascii_digit()) && i + 2 < s.len() => s[..i].to_string(),
            _ => s,
        }
    });
    b.unwrap_or_else(|| slug(&f.dir.file_name().unwrap_or_default().to_string_lossy()))
}

fn civil(ms: u64) -> String {
    let days = (ms / 86_400_000) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

fn register(cfg: &Config, f: &Film, file: &Path, kind: &str, mode: &str, changed: Value, chunks: Value, job: &str) -> Result<Value> {
    let mut vs = versions(f);
    let v = vs.iter().filter_map(|x| x["v"].as_u64()).max().unwrap_or(0) + 1;
    let mut n = notes(f);
    let fixed: Vec<String> = n.iter().filter(|x| jsd(x, "status", "") == "fixed" && x.get("version").map(|v| v.is_null()).unwrap_or(true)).map(|x| jsd(x, "id", "").to_string()).collect();
    if kind == "final" {
        for x in n.iter_mut() {
            if fixed.contains(&jsd(x, "id", "").to_string()) {
                x["version"] = json!(v);
            }
        }
        save_notes(f, &n)?;
    }
    let entry = json!({"v": v, "file": fwd(file), "kind": kind, "created": now_ms(), "job": job, "mode": mode, "parent": vs.last().map(|x| x["v"].clone()), "changedRanges": changed, "chunks": chunks, "notes": fixed});
    vs.push(entry.clone());
    write_json(&vh(f, "versions.json"), &json!(vs))?;
    if kind == "final" {
        let day = civil(now_ms());
        let out = cfg.workspace().join("outputs").join(&day);
        if std::fs::create_dir_all(&out).is_ok() {
            let _ = std::fs::hard_link(file, out.join(format!("{day} {} v{v}.mp4", title_of(f).replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], " "))));
        }
    }
    Ok(entry)
}

fn encode_args(cfg: &Config, preview: bool, enc: &str) -> Vec<String> {
    match enc {
        "amf" => return ["-c:v", "h264_amf", "-quality", "quality", "-rc", "cqp", "-qp_i", if preview { "27" } else { "19" }, "-qp_p", if preview { "29" } else { "21" }].iter().map(|s| s.to_string()).collect(),
        "qsv" => return ["-c:v", "h264_qsv", "-global_quality", if preview { "27" } else { "19" }].iter().map(|s| s.to_string()).collect(),
        "x264" => return ["-c:v", "libx264", "-preset", if preview { "veryfast" } else { "medium" }, "-crf", if preview { "25" } else { "17" }].iter().map(|s| s.to_string()).collect(),
        _ => {}
    }
    let nv = enc == "nvenc" || h264_encoder(cfg) == "h264_nvenc";
    let a: &[&str] = match (nv, preview) {
        (true, false) => &["-c:v", "h264_nvenc", "-preset", "p5", "-rc", "vbr", "-cq", "19", "-b:v", "0", "-profile:v", "high"],
        (true, true) => &["-c:v", "h264_nvenc", "-preset", "p3", "-rc", "vbr", "-cq", "27", "-b:v", "0"],
        (false, false) => &["-c:v", "libx264", "-preset", "medium", "-crf", "17", "-profile:v", "high"],
        (false, true) => &["-c:v", "libx264", "-preset", "veryfast", "-crf", "25"],
    };
    a.iter().map(|s| s.to_string()).collect()
}

/// Render the film. mode: full | partial (only the chunks the fixed notes, `seconds` or `chunks` touch) | preview.
pub fn render(cfg: &Config, f: &Film, a: &Value, rep: &Rep) -> Result<Value> {
    let ff = need_tool(cfg, "ffmpeg")?;
    let info = info(cfg, f);
    if !jb(&info, "has_page").unwrap_or(false) {
        bail!("the film has no page ({}) to render", jsd(&info, "page", ""));
    }
    let mode = jsd(a, "mode", "full").to_string();
    let preview = mode == "preview";
    // a language renders on its own: its own chunks (a partial render compares it with its last version),
    // its own sound when it has one, and a file named with its code
    let lang = match js(a, "lang").filter(|l| !l.is_empty()) {
        Some(l) => {
            let l = crate::langs::clean(l)?;
            if !crate::langs::langs(f).contains_key(&l) {
                bail!("the film has no language {l}; hub_lang lists them");
            }
            l
        }
        None => String::new(),
    };
    let (w, h, fps, dur) = (jfd(&info, "width", 1920.0) as u32, jfd(&info, "height", 1080.0) as u32, jfd(&info, "fps", 30.0), jfd(&info, "dur", 10.0));
    let ch = jfd(&info, "chunk_frames", 60.0) as usize;
    let total = (dur * fps).round() as usize;
    let nchunks = total.div_ceil(ch).max(1);
    let rdir = if lang.is_empty() { f.dir.join("render") } else { f.dir.join("render").join(format!("lang-{lang}")) };
    let cdir = rdir.join(if preview { "preview_chunks" } else { "chunks" });
    let man = json!({"fps": fps, "chunkFrames": ch, "chunkSec": ch as f64 / fps, "width": w, "height": h, "dur": dur, "family": "A", "renderer": "linnea"});
    let prev_v = versions(f).iter().filter_map(|x| x["v"].as_u64()).max().unwrap_or(0);
    let mut changed: Vec<usize> = vec![];
    std::fs::create_dir_all(&rdir)?;
    if mode == "partial" {
        let old = read_json_or(&rdir.join("manifest.json"), Value::Null);
        if old.is_null() || jfd(&old, "chunkFrames", 0.0) as usize != ch || jfd(&old, "width", 0.0) as u32 != w || jfd(&old, "height", 0.0) as u32 != h || jfd(&old, "fps", 0.0) != fps {
            bail!("a partial render needs one full render with the same size, fps and length first");
        }
        let mut want: Vec<usize> = ja(a, "chunks").iter().filter_map(|c| c.as_u64().map(|x| x as usize)).collect();
        for s in ja(a, "seconds") {
            let (x, y) = match s.as_array() {
                Some(r) => (r.first().and_then(|v| v.as_f64()).unwrap_or(0.0), r.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0)),
                None => (s.as_f64().unwrap_or(0.0), s.as_f64().unwrap_or(0.0)),
            };
            want.extend(chunks_for(&info, x, y.max(x)));
        }
        let ids: Vec<String> = ja(a, "notes").iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect();
        for n in notes(f) {
            let pick = if ids.is_empty() { jsd(&n, "status", "") == "fixed" && n.get("version").map(|v| v.is_null()).unwrap_or(true) && a.get("chunks").is_none() && a.get("seconds").is_none() } else { ids.contains(&jsd(&n, "id", "").to_string()) };
            if pick {
                want.extend(chunks_for(&info, jfd(&n, "t", 0.0), jf(&n, "t1").unwrap_or(jfd(&n, "t", 0.0))));
            }
        }
        want.sort();
        want.dedup();
        if want.is_empty() {
            bail!("nothing to re-render: no fixed notes since the last version, and no `seconds` or `chunks` given. Use a full render after code changes without notes");
        }
        let keep = rdir.join(format!("chunks_v{prev_v}"));
        std::fs::create_dir_all(&keep)?;
        for c in &want {
            let p = cdir.join(format!("c_{c:03}.mp4"));
            if p.exists() {
                let _ = crate::util::rename_retry(&p, keep.join(format!("c_{c:03}.mp4")));
            }
        }
        changed = want;
    } else if cdir.exists() {
        if preview {
            std::fs::remove_dir_all(&cdir)?;
        } else {
            let keep = rdir.join(format!("chunks_v{prev_v}"));
            let _ = std::fs::remove_dir_all(&keep);
            if crate::util::rename_retry(&cdir, &keep).is_err() {
                let _ = std::fs::remove_dir_all(&cdir);
            }
        }
    }
    // a folder Windows is still deleting (a player or the last render held a file in it) refuses for a moment
    let mut made = std::fs::create_dir_all(&cdir);
    for _ in 0..5 {
        if made.is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_secs(2));
        made = std::fs::create_dir_all(&cdir);
    }
    made.with_context(|| format!("cannot create {} (is a video from it open in a player, or does Windows' controlled folder access block Linnea?)", cdir.display()))?;
    let todo: Vec<usize> = (0..nchunks).filter(|c| !cdir.join(format!("c_{c:03}.mp4")).exists()).collect();
    let todo_frames: usize = todo.iter().map(|c| (total - c * ch).min(ch)).sum();
    let workers = (jfd(a, "workers", cfg.get("render.workers").and_then(|v| v.as_f64()).unwrap_or(2.0)) as usize).clamp(1, 6).min(todo.len().max(1));
    // from the start: progress reads in order, and a render cut short leaves the film's beginning done
    let queue = Arc::new(Mutex::new(todo.iter().copied().collect::<std::collections::VecDeque<usize>>()));
    let done = Arc::new(AtomicUsize::new(0));
    let fail: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let url = crate::langs::url_for(&page_url(cfg, f, &info), &lang);
    let scale = jf(a, "scale").unwrap_or(if preview { 0.5 } else { 1.0 }).clamp(0.25, 1.0);
    let enc = jsd(a, "encoder", "auto").to_string();
    let quality = if preview { 80 } else { jfd(a, "quality", 94.0) as u32 };
    rep.p(0.0, &format!("{}/{} chunks to render", todo.len(), nchunks));
    std::thread::scope(|s| {
        for wi in 0..workers {
            let (queue, done, fail, url, ff, cdir, rep) = (queue.clone(), done.clone(), fail.clone(), url.clone(), ff.clone(), cdir.clone(), rep.clone());
            let cfg = cfg.clone();
            let enc = enc.clone();
            s.spawn(move || {
                // One chunk: frames from the page into ffmpeg. On any error ffmpeg is stopped and waited for, so
                // no half-written chunk stays locked (a locked file is what "Access is denied" came from).
                let chunk = |page: &mut Page, c: usize, written: &mut usize| -> Result<bool> {
                    let n = (total - c * ch).min(ch);
                    let out = cdir.join(format!("c_{c:03}.mp4"));
                    let tmp = cdir.join(format!("c_{c:03}.tmp.mp4"));
                    let mut cmd = proc::cmd_low(&ff);
                    cmd.args(["-hide_banner", "-y", "-v", "error", "-f", "image2pipe", "-framerate", &format!("{fps}"), "-c:v", if quality > 0 { "mjpeg" } else { "png" }, "-i", "-"]);
                    cmd.args(["-frames:v", &n.to_string()]).args(encode_args(&cfg, preview, &enc)).args(["-pix_fmt", "yuv420p", "-r", &format!("{fps}"), "-an", "-movflags", "+faststart"]).arg(&tmp);
                    let mut child = cmd.stdin(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).spawn().context("starting ffmpeg")?;
                    let mut stdin = child.stdin.take().unwrap();
                    let mut feed = || -> Result<bool> {
                        for k in 0..n {
                            if rep.cancelled() {
                                return Ok(false);
                            }
                            let t = (c * ch + k) as f64 / fps;
                            let img = page.frame(t, quality, scale).map_err(|e| anyhow!("frame {} ({:.2} s): {e}", c * ch + k, t))?;
                            stdin.write_all(&img).context("ffmpeg stopped taking frames")?;
                            *written += 1;
                            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                            if d % 4 == 0 {
                                rep.p(0.9 * d as f64 / todo_frames.max(1) as f64, &format!("[w{wi}] chunk {c}"));
                            }
                        }
                        Ok(true)
                    };
                    let fed = feed();
                    drop(stdin);
                    if !matches!(fed, Ok(true)) {
                        let _ = child.kill();
                        let _ = child.wait();
                        let _ = std::fs::remove_file(&tmp);
                        return fed;
                    }
                    let o = child.wait_with_output()?;
                    if !o.status.success() {
                        let _ = std::fs::remove_file(&tmp);
                        bail!("encoding chunk {c}: {}", proc::err_tail(&String::from_utf8_lossy(&o.stderr), 4));
                    }
                    crate::util::rename_retry(&tmp, &out).with_context(|| format!("saving {}", out.display()))?;
                    Ok(true)
                };
                let run = || -> Result<()> {
                    let open = || -> Result<Page> {
                        let mut p = Page::open(&cfg, &url, w, h, true)?;
                        p.wait_ready(Duration::from_secs(120))?;
                        Ok(p)
                    };
                    let mut page = open()?;
                    loop {
                        if rep.cancelled() || fail.lock().unwrap().is_some() {
                            return Ok(());
                        }
                        let Some(c) = queue.lock().unwrap().pop_front() else { return Ok(()) };
                        // a browser that stops answering (it happens on long renders) is replaced and the chunk
                        // is drawn again, twice at most, before the render gives up
                        let mut tries = 0;
                        loop {
                            let mut written = 0;
                            match chunk(&mut page, c, &mut written) {
                                Ok(true) => break,
                                Ok(false) => return Ok(()),
                                Err(e) => {
                                    done.fetch_sub(written, Ordering::Relaxed);
                                    tries += 1;
                                    if tries > 2 {
                                        return Err(e.context(format!("chunk {c} failed 3 times")));
                                    }
                                    rep.p(0.9 * done.load(Ordering::Relaxed) as f64 / todo_frames.max(1) as f64, &format!("[w{wi}] chunk {c}: retry {tries}"));
                                    drop(page);
                                    std::thread::sleep(Duration::from_secs(2));
                                    page = open()?;
                                }
                            }
                        }
                    }
                };
                if let Err(e) = run() {
                    *fail.lock().unwrap() = Some(format!("{e:#}"));
                }
            });
        }
    });
    rep.check()?;
    if let Some(e) = fail.lock().unwrap().take() {
        bail!(e);
    }
    // Join, then lay the sound under it.
    rep.p(0.92, "joining");
    let list: String = (0..nchunks).map(|c| format!("file '{}/c_{c:03}.mp4'\n", cdir.file_name().unwrap().to_string_lossy())).collect();
    write_text(&rdir.join("list.txt"), &list)?;
    let video = rdir.join(if preview { "preview.mp4" } else { "video.mp4" });
    proc::ffmpeg(&ff, &["-f".into(), "concat".into(), "-safe".into(), "0".into(), "-i".into(), "list.txt".into(), "-c".into(), "copy".into(), fwd(&video)], Some(&rdir), 0.0, &Rep::none(), "joining chunks")?;
    if !preview {
        write_json(&rdir.join("manifest.json"), &man)?;
    }
    rep.p(0.96, "sound");
    let v = versions(f).iter().filter_map(|x| x["v"].as_u64()).max().unwrap_or(0) + 1;
    let tag = if lang.is_empty() { String::new() } else { format!("_{lang}") };
    let out = f.dir.join(format!("{}{tag}{}_v{v}.mp4", base_name(f), if preview { "_preview" } else { "" }));
    let lang_audio = if lang.is_empty() { None } else { crate::langs::audio(f, &lang) };
    let mux: Vec<String> = ja(&meta(f)["render"], "mux").iter().filter_map(|x| x.as_str().map(|s| s.replace("{video}", &fwd(&video)).replace("{out}", &fwd(&out)))).collect();
    if let Some(au) = lang_audio {
        proc::ffmpeg(&ff, &["-i".into(), fwd(&video), "-i".into(), fwd(&au), "-map".into(), "0:v".into(), "-map".into(), "1:a".into(), "-c:v".into(), "copy".into(), "-c:a".into(), "aac".into(), "-b:a".into(), "192k".into(), "-shortest".into(), "-movflags".into(), "+faststart".into(), fwd(&out)], None, 0.0, &Rep::none(), "sound")?;
    } else if !mux.is_empty() {
        let o = proc::run(proc::cmd(&ff).args(&mux).current_dir(&f.dir))?;
        if !o.ok {
            bail!("render.mux failed: {}", proc::err_tail(&o.stderr, 4));
        }
    } else if let Some(au) = js(&info, "audio") {
        proc::ffmpeg(&ff, &["-i".into(), fwd(&video), "-i".into(), au.into(), "-map".into(), "0:v".into(), "-map".into(), "1:a".into(), "-c:v".into(), "copy".into(), "-c:a".into(), "aac".into(), "-b:a".into(), "192k".into(), "-shortest".into(), "-movflags".into(), "+faststart".into(), fwd(&out)], None, 0.0, &Rep::none(), "sound")?;
    } else {
        std::fs::copy(&video, &out)?;
    }
    let ranges: Vec<Value> = {
        let src: Vec<usize> = if mode == "partial" { changed.clone() } else { (0..nchunks).collect() };
        let mut r: Vec<(f64, f64)> = vec![];
        for c in src {
            let (x, y) = ((c * ch) as f64 / fps, (((c + 1) * ch).min(total)) as f64 / fps);
            match r.last_mut() {
                Some(l) if (l.1 - x).abs() < 1e-6 => l.1 = y,
                _ => r.push((x, y)),
            }
        }
        r.into_iter().map(|(x, y)| json!([(x * 1000.0).round() / 1000.0, (y * 1000.0).round() / 1000.0])).collect()
    };
    let mut entry = register(cfg, f, &out, if preview { "preview" } else { "final" }, &mode, json!(ranges), json!(if mode == "partial" { json!(changed) } else { Value::Null }), jsd(a, "_job", ""))?;
    if !lang.is_empty() {
        entry["lang"] = json!(lang);
        let mut vs = versions(f);
        if let Some(last) = vs.iter_mut().rev().find(|x| x["v"] == entry["v"]) {
            last["lang"] = json!(lang);
        }
        write_json(&vh(f, "versions.json"), &json!(vs))?;
    }
    rep.p(1.0, "done");
    Ok(json!({"file": fwd(&out), "version": entry["v"], "mode": mode, "chunks_rendered": todo.len(), "of": nchunks, "changedRanges": ranges}))
}

// ---------- new films ----------

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)?.flatten() {
        let p = e.path();
        if p.is_dir() {
            copy_dir(&p, &to.join(e.file_name()))?;
        } else {
            std::fs::copy(&p, to.join(e.file_name()))?;
        }
    }
    Ok(())
}

pub fn new_film(cfg: &Config, a: &Value) -> Result<Value> {
    let title = jsd(a, "title", "New film").to_string();
    let example = jsd(a, "example", "");
    let root = films_dir(cfg);
    std::fs::create_dir_all(&root)?;
    let base = slug(&title);
    let mut dir = root.join(&base);
    let mut n = 2;
    while dir.exists() {
        dir = root.join(format!("{base}-{n}"));
        n += 1;
    }
    let (w, h, fps, dur) = (jfd(a, "width", 1080.0), jfd(a, "height", 1920.0), jfd(a, "fps", 30.0), jfd(a, "dur", 10.0));
    if !(16.0..=8192.0).contains(&w) || !(16.0..=8192.0).contains(&h) {
        bail!("width and height are in pixels, 16..8192 (got {w} x {h})");
    }
    if !(1.0..=120.0).contains(&fps) {
        bail!("fps is 1..120 (got {fps})");
    }
    if !(dur.is_finite() && dur >= 0.5 && dur <= 3.0 * 3600.0) {
        bail!("dur is the film's length in seconds, 0.5 or more (got {dur})");
    }
    // even sizes: H.264 with yuv420p cannot encode an odd width or height
    let (w, h) = ((w as u32) & !1, (h as u32) & !1);
    if !example.is_empty() {
        if example.contains(['/', '\\']) || example.contains("..") {
            bail!("no example '{example}'");
        }
        let src = library_dir().join("film-examples").join(example);
        if !src.is_dir() {
            bail!("no example '{example}'");
        }
        copy_dir(&src, &dir)?;
    } else {
        copy_dir(&library_dir().join("film-template"), &dir)?;
        // the title lands in an HTML <title>, a // comment and a '...' string: each wants its own escaping
        let one_line: String = title.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
        let html_title = one_line.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        let js_title = one_line.replace('\\', "\\\\").replace('\'', "\\'").replace("</", "<\\/");
        for (file, t) in [("index.html", &html_title), ("film.js", &js_title)] {
            let p = dir.join(file);
            let s = std::fs::read_to_string(&p)?.replace("__W__", &w.to_string()).replace("__H__", &h.to_string()).replace("__DUR__", &format!("{dur}")).replace("__FPS__", &format!("{fps}")).replace("__TITLE__", t);
            std::fs::write(&p, s)?;
        }
        // the linnea2d.js hook reads tweaks.json; a missing one shows up as a load error in checks
        write_json(&dir.join("tweaks.json"), &json!({}))?;
        let pj = json!({"title": title, "status": "in-progress", "live": {"page": "index.html", "fps": fps, "dur": dur, "width": w, "height": h}, "render": {"env": {"DUR": format!("{dur}")}}});
        write_json(&dir.join(".videohub").join("project.json"), &pj)?;
    }
    let films = scan(cfg);
    let f = films.into_iter().find(|f| f.dir == dir).ok_or_else(|| anyhow!("the new film was not found after creating it"))?;
    Ok(json!({"film": f.id, "dir": fwd(&f.dir), "files": ["index.html", "film.js"], "next": "edit film.js (window.renderOut(t) draws second t), then hub_live to check it and show it to the user"}))
}

// ---------- tools ----------

fn film(c: &Ctx, a: &Value) -> Result<Film> {
    open(&c.cfg, js(a, "film").or_else(|| js(a, "project")).unwrap_or(""))
}

fn t_workspace(c: &Ctx, _a: &Value) -> Result<Value> {
    let ws = c.cfg.workspace();
    Ok(json!({"workspace": fwd(&ws), "films": fwd(&films_dir(&c.cfg)), "outputs": fwd(&ws.join("outputs")), "note": "make every new film in the films folder, one folder per film (hub_new_film does it); never write into outputs"}))
}

fn t_films(c: &Ctx, _a: &Value) -> Result<Value> {
    let list: Vec<Value> = scan(&c.cfg)
        .iter()
        .map(|f| {
            let n = notes(f);
            let vs = versions(f);
            let i = info(&c.cfg, f);
            let cover = Some(()).and_then(|_| {
                let d = vh(f, "snaps");
                std::fs::read_dir(&d).ok().and_then(|rd| rd.flatten().map(|e| e.path()).max_by_key(|p| p.metadata().and_then(|m| m.modified()).ok())).map(|p| fwd(&p))
            });
            json!({"id": f.id, "title": i["title"], "dir": i["dir"], "status": i["status"], "width": i["width"], "height": i["height"], "dur": i["dur"],
                   "open_notes": n.iter().filter(|x| matches!(jsd(x, "status", ""), "open" | "claude" | "still")).count(), "fixed_notes": n.iter().filter(|x| jsd(x, "status", "") == "fixed").count(),
                   "versions": vs.len(), "latest": vs.last().map(|v| v["file"].clone()), "cover": cover, "rev": rev(f)})
        })
        .collect();
    Ok(json!({"films": list, "folder": fwd(&films_dir(&c.cfg))}))
}

fn t_new(c: &Ctx, a: &Value) -> Result<Value> {
    new_film(&c.cfg, a)
}

fn t_project(c: &Ctx, a: &Value) -> Result<Value> {
    let f = film(c, a)?;
    if let Some(set) = a.get("set").filter(|s| s.is_object()) {
        let p = vh(&f, "project.json");
        let mut m = read_json_or(&p, json!({}));
        merge(&mut m, set);
        write_json(&p, &m)?;
    }
    let mut i = info(&c.cfg, &f);
    i["project"] = read_json_or(&vh(&f, "project.json"), json!({}));
    i["versions"] = json!(versions(&f));
    i["notes"] = json!({"open": notes(&f).iter().filter(|x| matches!(jsd(x, "status", ""), "open" | "claude" | "still")).count(), "all": notes(&f).len()});
    Ok(i)
}

/// Check the page headless the way the renderer will see it, then show it in the Linnea window.
fn t_live(c: &Ctx, a: &Value) -> Result<Value> {
    let f = film(c, a)?;
    let mut i = info(&c.cfg, &f);
    if !jb(&i, "has_page").unwrap_or(false) {
        bail!("no page {} in {}. A live film needs a page with window.renderOut(t) and window.READY (hub_new_film makes one)", jsd(&i, "page", ""), f.dir.display());
    }
    let (w, h) = (jfd(&i, "width", 1920.0) as u32, jfd(&i, "height", 1080.0) as u32);
    let mut checks = json!({});
    match Page::open(&c.cfg, &page_url(&c.cfg, &f, &i), w, h, false) {
        Ok(mut p) => {
            let ready = p.wait_ready(Duration::from_secs(30));
            checks["ready"] = json!(ready.is_ok());
            if let Err(e) = &ready {
                checks["problem"] = json!(e.to_string());
            }
            let probe = p.eval("(() => { const cs = [...document.querySelectorAll('canvas')].map(c => [c.width, c.height]); let s = window.VH_SCENES; if (typeof s === 'function') s = s(); return {renderOut: typeof window.renderOut, canvases: cs, scenes: Array.isArray(s) ? s : null, title: document.title, hook: !!window.KE, edits: window.KE ? Object.keys(window.KE.getEdits() || {}).length : null}; })()").unwrap_or(json!({}));
            checks["page"] = probe.clone();
            if probe["scenes"].is_array() {
                let _ = write_json(&vh(&f, "scenes.json"), &probe["scenes"]);
                i["scenes"] = probe["scenes"].clone();
            }
            if ready.is_ok() {
                let dur = jfd(&i, "dur", 10.0);
                let mut bad = vec![];
                for t in [0.0, dur * 0.25, dur * 0.5, dur * 0.75, (dur - 1.0 / jfd(&i, "fps", 30.0)).max(0.0)] {
                    if let Err(e) = p.eval(&format!("(async () => {{ const r = window.renderOut({t:.4}); if (r && r.then) await r; return 1; }})()")) {
                        bad.push(format!("renderOut({t:.2}): {e}"));
                    }
                }
                checks["render_errors"] = json!(bad);
            }
            checks["console_errors"] = json!(p.errors);
        }
        Err(e) => checks["problem"] = json!(format!("headless check not possible: {e}")),
    }
    // a fresh frame becomes the film's cover in the list
    if checks["ready"] == json!(true) {
        let d = jfd(&i, "dur", 10.0);
        let _ = frames(&c.cfg, &f, &[d * 0.3], 0.5);
    }
    write_json(&c.cfg.state().join("focus.json"), &json!({"kind": "film", "project": f.id, "t": a["t"], "ts": now_ms(), "by": c.origin}))?;
    // puppets the film made for itself go to the library, so the next film can use them
    let mut harvested = Value::Null;
    if c.cfg.get("puppets.autosave") != Some(&json!(false)) {
        if let Ok(h) = crate::puppets::harvest(c, &json!({"film": f.id}), false) {
            if !ja(&h, "saved").is_empty() {
                harvested = h["saved"].clone();
            }
        }
    }
    let mut next = vec![];
    if !jb(&i, "dur_known").unwrap_or(false) {
        next.push(format!("the length is a guess ({} s): save it with hub_project set={{live:{{dur:N}}}}", jfd(&i, "dur", 0.0)));
    }
    if checks["ready"] != json!(true) || !ja(&checks, "render_errors").is_empty() || !ja(&checks, "console_errors").is_empty() {
        next.push("fix the errors above, then call hub_live again".into());
    } else {
        next.push("the user is watching it in Linnea now; stop here and wait for their notes (hub_notes)".into());
    }
    let mut out = json!({"film": f.id, "info": i, "checks": checks, "next": next});
    if !harvested.is_null() {
        out["puppets_saved_to_library"] = harvested;
    }
    Ok(out)
}

fn t_frame(c: &Ctx, a: &Value) -> Result<Value> {
    let f = film(c, a)?;
    let i = info(&c.cfg, &f);
    let mut times: Vec<f64> = ja(a, "times").iter().filter_map(|x| x.as_f64()).collect();
    if let Some(t) = jf(a, "t") {
        times.push(t);
    }
    if let (Some(x), Some(y)) = (jf(a, "from"), jf(a, "to")) {
        let step = jfd(a, "step", ((y - x) / 8.0).max(1.0 / jfd(&i, "fps", 30.0)));
        let mut t = x;
        while t <= y + 1e-6 && times.len() < 48 {
            times.push(t);
            t += step;
        }
    }
    if times.is_empty() {
        let d = jfd(&i, "dur", 10.0);
        times = (0..9).map(|k| d * k as f64 / 8.0 * 0.999).collect();
    }
    times.truncate(48);
    let files = frames(&c.cfg, &f, &times, jfd(a, "scale", 1.0))?;
    let mut out = json!({"frames": times.iter().zip(&files).map(|(t, p)| json!({"t": t, "file": fwd(p)})).collect::<Vec<_>>(), "note": "open the files with Read to look at them"});
    if files.len() > 1 {
        let s = vh(&f, &format!("snaps/sheet-{}.jpg", now_ms()));
        if sheet(&c.cfg, &files, &s).is_ok() {
            out["sheet"] = json!(fwd(&s));
        }
    }
    Ok(out)
}

fn t_notes(c: &Ctx, a: &Value) -> Result<Value> {
    let f = film(c, a)?;
    let want = jsd(a, "status", "open");
    let list: Vec<Value> = notes(&f)
        .into_iter()
        .filter(|n| match want {
            "all" => true,
            "open" => matches!(jsd(n, "status", ""), "open" | "claude" | "still"),
            s => jsd(n, "status", "") == s,
        })
        .collect();
    let mut all = notes(&f);
    let mut touched = false;
    if want == "open" && c.origin == "agent" {
        for n in all.iter_mut() {
            if jsd(n, "status", "") == "open" {
                n["status"] = json!("claude");
                touched = true;
            }
        }
        if touched {
            save_notes(&f, &all)?;
        }
    }
    Ok(json!({"film": f.id, "notes": list, "how": "t is the second, frame the frame number, chunks the render chunks it touches. Read images.crop first (the drawing close up), then images.annotated / images.frame. After fixing: hub_note_update status=fixed with a short reply. hub_frame t=… shows the frame as it is now."}))
}

fn t_note_add(c: &Ctx, a: &Value) -> Result<Value> {
    let f = film(c, a)?;
    add_note(&c.cfg, &f, a, &c.origin)
}

fn t_note_update(c: &Ctx, a: &Value) -> Result<Value> {
    let f = film(c, a)?;
    let mut n = notes(&f);
    let id = jsd(a, "id", "");
    if jb(a, "delete") == Some(true) {
        n.retain(|x| jsd(x, "id", "") != id);
        save_notes(&f, &n)?;
        let _ = std::fs::remove_dir_all(vh(&f, &format!("notes/{id}")));
        return Ok(json!({"deleted": id}));
    }
    let note = n.iter_mut().find(|x| jsd(x, "id", "") == id).ok_or_else(|| anyhow!("no note '{id}'"))?;
    for k in ["status", "reply", "text"] {
        if let Some(v) = a.get(k) {
            note[k] = v.clone();
        }
    }
    if jsd(a, "status", "") == "fixed" {
        note["fixed_at"] = json!(now_ms());
        note["version"] = Value::Null;
    }
    let out = note.clone();
    save_notes(&f, &n)?;
    Ok(out)
}

fn t_render(c: &Ctx, a: &Value) -> Result<Value> {
    if c.origin == "agent" && c.cfg.get("render.agent").and_then(|v| v.as_bool()) != Some(true) {
        bail!("rendering is the user's step: they press Render in Linnea when the film is ready. Tell them it is ready for review instead. (Settings render.agent=true lets agents render.)");
    }
    let f = film(c, a)?;
    render(&c.cfg, &f, a, &c.rep)
}

fn t_register(c: &Ctx, a: &Value) -> Result<Value> {
    let f = film(c, a)?;
    let file = PathBuf::from(jsd(a, "file", ""));
    if !file.is_file() {
        bail!("no such video: {}", file.display());
    }
    register(&c.cfg, &f, &file, jsd(a, "kind", "final"), "outside", a.get("changed").cloned().unwrap_or(Value::Null), Value::Null, "")
}

fn t_versions(c: &Ctx, a: &Value) -> Result<Value> {
    let f = film(c, a)?;
    Ok(json!({"versions": versions(&f)}))
}

fn t_cleanup(c: &Ctx, a: &Value) -> Result<Value> {
    let f = film(c, a)?;
    fn size(p: &Path) -> u64 {
        if p.is_file() {
            return p.metadata().map(|m| m.len()).unwrap_or(0);
        }
        std::fs::read_dir(p).map(|rd| rd.flatten().map(|e| size(&e.path())).sum()).unwrap_or(0)
    }
    let mut items = vec![];
    if let Ok(rd) = std::fs::read_dir(f.dir.join("render")) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n.starts_with("chunks_v") || n == "preview_chunks" || n == "preview.mp4" {
                items.push(json!({"path": fwd(&e.path()), "bytes": size(&e.path()), "why": if n.starts_with("chunks_v") { "chunks of an older version, kept for comparing" } else { "preview render parts" }}));
            }
        }
    }
    let snaps = vh(&f, "snaps");
    if snaps.exists() {
        items.push(json!({"path": fwd(&snaps), "bytes": size(&snaps), "why": "frames captured for checks and the agent"}));
    }
    Ok(json!({"items": items, "note": "a list only: the user decides what goes"}))
}

// ---------- the film's own files (for the assistant inside the app) ----------

/// A path inside the film folder that code may read or write: no way out of the folder, text files only.
fn film_path(f: &Film, rel: &str, write: bool) -> Result<PathBuf> {
    let rel = rel.trim().replace('\\', "/");
    let rel = rel.trim_start_matches("./");
    if rel.is_empty() {
        bail!("`path` is a file inside the film folder, like film.js");
    }
    let parts: Vec<&str> = rel.split('/').collect();
    if rel.starts_with('/') || rel.contains(':') || parts.iter().any(|p| p.is_empty() || *p == "..") {
        bail!("`path` stays inside the film folder: {rel} does not");
    }
    let p = f.dir.join(rel);
    let e = ext(&p);
    if !(CODE.contains(&e.as_str()) || e == "md") {
        bail!("only text files can be read or written here ({}); {rel} is not one", CODE.join(", "));
    }
    if write {
        let name = parts[parts.len() - 1];
        if parts.len() > 1 && skipped(parts[0]) && parts[0] != "puppets" {
            bail!("{} holds Linnea's own data (notes, versions, renders); it is not written by hand", parts[0]);
        }
        match name {
            "linnea2d.js" | "kesit2d.js" => bail!("linnea2d.js is Linnea's engine and is replaced on every update; put the change in film.js"),
            "tweaks.json" => bail!("tweaks.json holds the user's own edits and layers; change them with hub_2d"),
            "links.json" if parts.len() > 1 => bail!("puppets/links.json is kept by puppet_link and puppet_sync"),
            _ => {}
        }
    }
    Ok(p)
}

/// Keep the file as it is now, so a change can be undone. A file that does not exist yet leaves an
/// `.absent` marker: undoing then removes it again.
fn backup(f: &Film, p: &Path) {
    let dir = vh(f, "backups");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let name = p.strip_prefix(&f.dir).map(|r| fwd(r).replace('/', "__")).unwrap_or_else(|_| "file".into());
    if p.is_file() {
        let _ = std::fs::copy(p, dir.join(format!("{}.{name}", now_ms())));
    } else {
        let _ = std::fs::write(dir.join(format!("{}.{name}.absent", now_ms())), b"");
    }
    // keep the newest sixty
    if let Ok(rd) = std::fs::read_dir(&dir) {
        let mut all: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        all.sort();
        let n = all.len().saturating_sub(60);
        for old in all.into_iter().take(n) {
            let _ = std::fs::remove_file(old);
        }
    }
}

/// The film's code as a list of files with their sizes.
pub fn files(f: &Film) -> Vec<Value> {
    fn walk(root: &Path, d: &Path, depth: u32, out: &mut Vec<Value>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        let mut es: Vec<_> = rd.flatten().collect();
        es.sort_by_key(|e| e.file_name());
        for e in es {
            if out.len() >= 300 {
                return;
            }
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                if depth > 0 && (!skipped(&name) || name == "puppets") {
                    walk(root, &p, depth - 1, out);
                }
            } else if CODE.contains(&ext(&p).as_str()) || ext(&p) == "md" || ["png", "jpg", "jpeg", "webp", "wav", "mp3", "mp4", "woff2", "ttf"].contains(&ext(&p).as_str()) {
                out.push(json!({"path": p.strip_prefix(root).map(fwd).unwrap_or_default(), "bytes": e.metadata().map(|m| m.len()).unwrap_or(0)}));
            }
        }
    }
    let mut out = vec![];
    walk(&f.dir, &f.dir, 3, &mut out);
    out
}

fn t_file(c: &Ctx, a: &Value) -> Result<Value> {
    let f = film(c, a)?;
    let action = jsd(a, "action", "read");
    if action == "list" {
        return Ok(json!({"film": f.id, "dir": fwd(&f.dir), "files": files(&f)}));
    }
    let rel = jsd(a, "path", "");
    match action {
        "read" => {
            let p = film_path(&f, rel, false)?;
            let s = std::fs::read_to_string(&p).map_err(|e| anyhow!("cannot read {rel}: {e}"))?;
            let lines: Vec<&str> = s.lines().collect();
            let from = (jfd(a, "from", 1.0).max(1.0) as usize).min(lines.len().max(1));
            let to = (jfd(a, "to", lines.len() as f64) as usize).clamp(from, lines.len().max(from));
            let mut text = lines[from - 1..to.min(lines.len())].join("\n");
            let cut = text.chars().count() > 60_000;
            if cut {
                text = text.chars().take(60_000).collect();
            }
            Ok(json!({"path": rel, "lines": lines.len(), "from": from, "to": to, "content": text, "truncated": cut, "note": if cut { "long file: read it in parts with from / to (line numbers)" } else { "" }}))
        }
        "write" => {
            let p = film_path(&f, rel, true)?;
            let content = js(a, "content").ok_or_else(|| anyhow!("`content` is the whole new text of {rel}"))?;
            backup(&f, &p);
            write_text(&p, content)?;
            Ok(json!({"written": rel, "bytes": content.len(), "next": "hub_live checks the page; hub_frame shows the frames you changed"}))
        }
        "replace" => {
            let p = film_path(&f, rel, true)?;
            let find = js(a, "find").filter(|s| !s.is_empty()).ok_or_else(|| anyhow!("`find` is the exact text to replace"))?;
            let with = js(a, "with").ok_or_else(|| anyhow!("`with` is the text that takes its place"))?;
            let s = std::fs::read_to_string(&p).map_err(|e| anyhow!("cannot read {rel}: {e}"))?;
            // files saved on Windows have CRLF; the model writes LF
            let (s, find, with) = if s.contains("\r\n") && !find.contains("\r\n") { (s.replace("\r\n", "\n"), find.to_string(), with.to_string()) } else { (s, find.to_string(), with.to_string()) };
            let n = s.matches(&find).count();
            if n == 0 {
                bail!("`find` is not in {rel}; read the file again and copy the text exactly, with its spaces");
            }
            if n > 1 && jb(a, "all") != Some(true) {
                bail!("`find` is in {rel} {n} times; add lines around it until it is unique, or pass all=true");
            }
            backup(&f, &p);
            write_text(&p, &s.replace(&find, &with))?;
            Ok(json!({"changed": rel, "places": n, "next": "hub_live checks the page; hub_frame shows the frames you changed"}))
        }
        other => bail!("unknown action '{other}': list, read, write or replace"),
    }
}

/// Every file changed since `since` (ms), with the backup that holds it as it was then (None: it did not exist).
fn checkpoint(f: &Film, since: u64) -> Vec<(String, Option<PathBuf>)> {
    let mut first: std::collections::BTreeMap<String, (u64, Option<PathBuf>)> = Default::default();
    for e in std::fs::read_dir(vh(f, "backups")).into_iter().flatten().flatten() {
        let n = e.file_name().to_string_lossy().to_string();
        let Some((ms, rest)) = n.split_once('.') else { continue };
        let Ok(ms) = ms.parse::<u64>() else { continue };
        if ms < since {
            continue;
        }
        let (rel, held) = match rest.strip_suffix(".absent") {
            Some(r) => (r.replace("__", "/"), None),
            None => (rest.replace("__", "/"), Some(e.path())),
        };
        if first.get(&rel).map(|x| ms < x.0).unwrap_or(true) {
            first.insert(rel, (ms, held));
        }
    }
    first.into_iter().map(|(k, (_, p))| (k, p)).collect()
}

fn t_restore(c: &Ctx, a: &Value) -> Result<Value> {
    let f = film(c, a)?;
    let since = jf(a, "since").ok_or_else(|| anyhow!("`since` is the moment (ms) to go back to, as hub_ask returned it"))? as u64;
    let files = checkpoint(&f, since);
    if files.is_empty() {
        bail!("no file of this film changed since then");
    }
    let cap = |s: String| if s.chars().count() > 200_000 { s.chars().take(200_000).collect() } else { s };
    match jsd(a, "action", "diff") {
        "diff" => Ok(json!({"files": files.iter().map(|(rel, held)| json!({
            "path": rel,
            "before": cap(held.as_ref().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default()),
            "after": cap(std::fs::read_to_string(f.dir.join(rel)).unwrap_or_default()),
            "existed": held.is_some(),
        })).collect::<Vec<_>>()})),
        "undo" => {
            let mut done = vec![];
            for (rel, held) in &files {
                let p = film_path(&f, rel, true)?;
                backup(&f, &p);
                match held {
                    Some(b) => {
                        std::fs::copy(b, &p).with_context(|| format!("restoring {rel}"))?;
                    }
                    None => {
                        let _ = std::fs::remove_file(&p);
                    }
                }
                done.push(rel.clone());
            }
            // the assistant's history shows which requests were taken back
            let hp = vh(&f, "assistant.json");
            if let Some(mut h) = read_json_or(&hp, json!([])).as_array().cloned() {
                for e in h.iter_mut() {
                    if jfd(e, "since", 0.0) as u64 >= since {
                        e["undone"] = json!(true);
                    }
                }
                let _ = write_json(&hp, &json!(h));
            }
            Ok(json!({"restored": done, "note": "the files are back as they were; the preview reloads"}))
        }
        other => bail!("unknown action '{other}': diff or undo"),
    }
}

// ---------- the mouth follows the voice ----------

/// audio/mouth.json: how open a talking puppet's mouth is, 30 times a second, from the film's voice (its
/// voice stem when it has one, else the mix). linnea2d.js reads it for every puppet with a `mouth` param.
pub fn lipsync(cfg: &Config, f: &Film) -> Result<Value> {
    let ff = need_tool(cfg, "ffmpeg")?;
    let info = info(cfg, f);
    let voice = ["audio/st_vo.wav", "data/vo.wav", "audio/vo.wav", "audio/voice.wav", "stems/vo.wav"].iter().map(|r| f.dir.join(r)).find(|p| p.is_file());
    let src = voice.or_else(|| js(&info, "audio").map(PathBuf::from)).ok_or_else(|| anyhow!("the film has no sound yet (audio/mix.wav)"))?;
    let o = proc::cmd(&ff)
        .args(["-hide_banner", "-v", "error", "-i"])
        .arg(&src)
        .args(["-ac", "1", "-ar", "8000", "-f", "s16le", "-"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()?;
    if !o.status.success() {
        bail!("reading the sound: {}", proc::err_tail(&String::from_utf8_lossy(&o.stderr), 3));
    }
    let s: Vec<f64> = o.stdout.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f64 / 32768.0).collect();
    let fps = 30.0;
    let n = ((s.len() as f64 / 8000.0) * fps).ceil() as usize;
    let mut rms = vec![0.0; n];
    for (i, r) in rms.iter_mut().enumerate() {
        let a = (i as f64 * 8000.0 / fps) as usize;
        let b = (((i + 1) as f64 * 8000.0 / fps) as usize).min(s.len());
        if b > a {
            *r = (s[a..b].iter().map(|x| x * x).sum::<f64>() / (b - a) as f64).sqrt();
        }
    }
    let mut sorted = rms.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p95 = sorted.get((sorted.len() as f64 * 0.95) as usize).copied().unwrap_or(0.0).max(1e-4);
    let floor = p95 * 0.12;
    let mut prev = 0.0;
    let v: Vec<f64> = rms
        .iter()
        .map(|r| {
            let x = ((r - floor) / (p95 - floor)).clamp(0.0, 1.0);
            // opens at once, closes over a few frames: lips do not snap shut between syllables
            let y = if x > prev { x } else { (prev * 0.55).max(x) };
            prev = y;
            (y * 100.0).round() / 100.0
        })
        .collect();
    let from = src.strip_prefix(&f.dir).map(fwd).unwrap_or_else(|_| fwd(&src));
    // linnea2d.js asks for the file only when tweaks.json names it (then a film without one shows no error)
    let tp = f.dir.join("tweaks.json");
    let mut tw = read_json_or(&tp, json!({}));
    tw["mouth"] = json!("audio/mouth.json");
    write_json(&tp, &tw)?;
    write_json(&f.dir.join("audio").join("mouth.json"), &json!({"fps": fps, "from": from, "v": v}))?;
    Ok(json!({"file": "audio/mouth.json", "frames": n, "from": from, "note": "puppets with a `mouth` param now open their mouth with this sound (KE.mouth(t) in code); run again after the sound changes"}))
}

fn t_lipsync(c: &Ctx, a: &Value) -> Result<Value> {
    let f = film(c, a)?;
    lipsync(&c.cfg, &f)
}

fn t_job(_c: &Ctx, a: &Value) -> Result<Value> {
    let id = js(a, "id").ok_or_else(|| anyhow!("`id` is the job id"))?;
    if jb(a, "cancel") == Some(true) {
        crate::jobs::cancel(id);
    }
    let j = if jb(a, "wait") == Some(true) { crate::jobs::wait(id, jfd(a, "seconds", 540.0) as u64) } else { crate::jobs::get(id) };
    j.ok_or_else(|| anyhow!("no job '{id}'"))
}

const F: (&str, &str, &str) = ("film!", "string", "Film id, title or folder (see hub_films)");

pub const TOOLS: &[Tool] = &[
    Tool { name: "hub_workspace", desc: "Where this PC keeps its video work: the workspace, the films folder (make every new film there, one folder per film) and the outputs folder (finished renders; never write there).", params: &[], long: false, app_only: false, run: t_workspace },
    Tool { name: "hub_films", desc: "List the films: title, size, length, open notes, versions.", params: &[], long: false, app_only: false, run: t_films },
    Tool { name: "hub_new_film", desc: "Start a new film written as code from the template: index.html with a fixed-size canvas and film.js where window.renderOut(t) draws second t. Default 1080x1920, 30 fps, 10 s. example=hello-film copies VideoHub's example film instead.", params: &[("title!", "string", "Film title"), ("width", "number", "Width px"), ("height", "number", "Height px"), ("fps", "number", "Frames per second"), ("dur", "number", "Length in seconds"), ("example", "string", "hello-film")], long: false, app_only: false, run: t_new },
    Tool { name: "hub_project", desc: "A film's contract as Linnea reads it (page, size, fps, length, sound, words, scenes, chunks), its versions and note counts. `set` merges into .videohub/project.json (title, status, live {page, fps, dur, width, height, audio}, render {env {DUR}, mux, workers}, transcript {file, offset}).", params: &[F, ("set", "object", "Keys to merge")], long: false, app_only: false, run: t_project },
    Tool { name: "hub_live", desc: "Open the film in Linnea's live preview WITHOUT RENDERING: it plays from its code, the user watches and leaves notes, and the preview reloads itself when the code changes. First it loads the page headless and reports problems: READY never set, renderOut missing or throwing, console errors, the canvas size. Call it as soon as a new film (or a big change) plays, then stop and wait for the user's notes.", params: &[F, ("t", "number", "Second to show")], long: false, app_only: false, run: t_live },
    Tool { name: "hub_frame", desc: "Exact frames of the film as JPEGs, drawn by its own renderOut: `t` one second, `times` a list, or `from`/`to`/`step` a range (with a contact sheet). Use it to look at your work before handing it over, and after every fix.", params: &[F, ("t", "number", "Second"), ("times", "array", "Seconds"), ("from", "number", "Range start"), ("to", "number", "Range end"), ("step", "number", "Seconds between frames"), ("scale", "number", "0.25..1, default 1")], long: false, app_only: false, run: t_frame },
    Tool { name: "hub_notes", desc: "The notes the user left on frames in Linnea: time, frame, range, the chunks it touches, text, narration at that moment, and the pictures (images.crop = the drawing close up, images.annotated = the frame with the drawing, images.frame = the frame). Open the pictures with Read. Default: open notes (they are marked as with Claude).", params: &[F, ("status", "string", "open | fixed | done | all")], long: false, app_only: false, run: t_notes },
    Tool { name: "hub_note_update", desc: "Save a note's status and a short reply the user sees. After fixing: status \"fixed\" + reply. Others: open, wontfix, done.", params: &[F, ("id!", "string", "Note id"), ("status", "string", "fixed | open | wontfix | done"), ("reply", "string", "Short reply"), ("delete", "boolean", "Remove the note")], long: false, app_only: false, run: t_note_update },
    Tool { name: "hub_note_add", desc: "Leave a note on a frame (the user does this in the app; an agent may record its own open question).", params: &[F, ("t!", "number", "Second"), ("t1", "number", "End of a range"), ("text!", "string", "What is wrong"), ("drawing", "string", "PNG data URL at video size"), ("box", "array", "[x0,y0,x1,y1] of the drawing"), ("video", "string", "A rendered version: take the frame from it")], long: false, app_only: false, run: t_note_add },
    Tool { name: "hub_render", desc: "Render the film in Linnea's queue (one render at a time, low priority). mode full, partial (only the chunks the fixed notes, `seconds` or `chunks` touch; needs one full render first) or preview (half size). Renders are the USER's step in the app; an agent gets a refusal unless the user allowed it in settings.", params: &[F, ("mode", "string", "full | partial | preview"), ("notes", "array", "Note ids (partial)"), ("seconds", "array", "[[from, to], ...] (partial)"), ("chunks", "array", "Chunk numbers (partial)"), ("workers", "number", "Browser workers, default 2"), ("encoder", "string", "auto | nvenc | amf | qsv | x264"), ("scale", "number", "Size 0.25..1"), ("lang", "string", "Render this language (hub_lang) as its own file")], long: true, app_only: false, run: t_render },
    Tool { name: "hub_job", desc: "The state of a render job. wait=true waits until it ends (at most ~9 min; call again if it has not). cancel=true stops it.", params: &[("id!", "string", "Job id"), ("wait", "boolean", "Wait"), ("cancel", "boolean", "Stop it")], long: false, app_only: false, run: t_job },
    Tool { name: "hub_register_version", desc: "Register a video made outside Linnea's queue as this film's new version (with the seconds that changed).", params: &[F, ("file!", "string", "Video path"), ("changed", "array", "[[from, to], ...]"), ("kind", "string", "final | preview")], long: false, app_only: false, run: t_register },
    Tool { name: "hub_versions", desc: "The film's rendered versions with the ranges each one changed.", params: &[F], long: false, app_only: false, run: t_versions },
    Tool { name: "hub_file", desc: "The film's own code files, for the assistant inside the app: action list (the files), read (path; from / to line numbers for a part), write (path, content: the whole file) or replace (path, find: exact text that occurs once, with: its replacement; all=true for every occurrence). Paths stay inside the film folder; every overwrite keeps a backup under .videohub/backups. linnea2d.js and tweaks.json are not written here (hub_2d changes layers and edits).", params: &[F, ("action", "string", "list | read | write | replace"), ("path", "string", "File inside the film folder, e.g. film.js"), ("content", "string", "write: the whole new text"), ("find", "string", "replace: the exact text to find"), ("with", "string", "replace: the text that takes its place"), ("all", "boolean", "replace: every occurrence"), ("from", "number", "read: first line"), ("to", "number", "read: last line")], long: false, app_only: true, run: t_file },
    Tool { name: "hub_restore", desc: "Go back to how the film's files were at a moment: action diff shows every file changed since `since` (ms, as hub_ask returns it) before and after; action undo puts them back (undo is itself undoable).", params: &[F, ("since!", "number", "Moment in ms"), ("action", "string", "diff | undo")], long: false, app_only: true, run: t_restore },
    Tool { name: "hub_lipsync", desc: "Make audio/mouth.json from the film's voice (its voice stem, else audio/mix.wav): every puppet with a `mouth` param then opens its mouth with the sound, and KE.mouth(t) gives the same 0..1 in code. Run again after the sound changes.", params: &[F], long: false, app_only: false, run: t_lipsync },
    Tool { name: "hub_cleanup", desc: "List the files of this film that can safely go (old chunks, previews, captured frames). A list only: the user decides.", params: &[F], long: false, app_only: false, run: t_cleanup },
];
