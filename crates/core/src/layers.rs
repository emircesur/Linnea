//! Layers over a footage reel: a transparent page per reel (`overlay/<reel>/`), the same layer system the
//! films use (tweaks.json items2d: text, pictures, counters, handwriting, arrows, phones, lower thirds,
//! keyframes and rigs), edited live in the editor and rendered as a transparent video over the picture.
//! Claude can also write any animation into the page's film.js.
use crate::cdp::Page;
use crate::config::{need_tool, Config};
use crate::proc::{self, Rep};
use crate::project::Project;
use crate::timeline::Timeline;
use crate::tools::{Ctx, Tool};
use crate::util::*;
use anyhow::{bail, Result};
use base64::Engine;
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn dir(p: &Project, rid: &str) -> PathBuf {
    p.p(&format!("overlay/{rid}"))
}

/// Make the reel's layer page if it is missing; keep its hook current.
pub fn ensure_page(p: &Project, rid: &str, w: u32, h: u32) -> Result<PathBuf> {
    let d = dir(p, rid);
    std::fs::create_dir_all(&d)?;
    let tpl = library_dir().join("overlay-template");
    let page = d.join("index.html");
    let size_ok = std::fs::read_to_string(&page).map(|s| s.contains(&format!("width=\"{w}\" height=\"{h}\""))).unwrap_or(false);
    if !size_ok {
        let s = std::fs::read_to_string(tpl.join("index.html"))?.replace("__W__", &w.to_string()).replace("__H__", &h.to_string());
        std::fs::write(&page, s)?;
    }
    if !d.join("film.js").exists() {
        std::fs::copy(tpl.join("film.js"), d.join("film.js"))?;
    }
    copy_if_changed(&library_dir().join("film-hooks").join("linnea2d.js"), &d.join("linnea2d.js"))?;
    rename_old_hook(&d, "index.html");
    if !d.join("tweaks.json").is_file() {
        write_json(&d.join("tweaks.json"), &json!({}))?;
    }
    Ok(d)
}

/// Copy a file only when the copy differs. Rewriting the same engine on every save would give it a new
/// modification time, and the preview reloads (and rescans the film) whenever a code file changes.
pub fn copy_if_changed(src: &Path, dst: &Path) -> Result<()> {
    let new = std::fs::read(src)?;
    if std::fs::read(dst).map(|old| old != new).unwrap_or(true) {
        std::fs::write(dst, new)?;
    }
    Ok(())
}

/// The engine's name before the rename.
pub const OLD_HOOK: &str = "kesit2d.js";

/// A page made before the rename loads kesit2d.js: point it at linnea2d.js and drop the old copy.
pub fn rename_old_hook(d: &Path, page: &str) {
    let p = d.join(page);
    if let Ok(s) = std::fs::read_to_string(&p) {
        if s.contains(OLD_HOOK) && std::fs::write(&p, s.replace(OLD_HOOK, "linnea2d.js")).is_ok() {
            let _ = std::fs::remove_file(d.join(OLD_HOOK));
        }
    }
}

pub fn items(d: &Path) -> Vec<Value> {
    read_json_or(&d.join("tweaks.json"), json!({}))["items2d"].as_array().cloned().unwrap_or_default()
}

/// A dropped file (data URL) saved next to the page; returns its path relative to the page.
pub fn save_upload(d: &Path, name: &str, data: &str) -> Result<String> {
    let b = data.split_once("base64,").map(|x| x.1).unwrap_or(data);
    let bytes = base64::engine::general_purpose::STANDARD.decode(b.trim())?;
    let safe: String = name.chars().map(|c| if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' }).collect();
    std::fs::create_dir_all(d.join("vh2d"))?;
    std::fs::write(d.join("vh2d").join(&safe), bytes)?;
    Ok(format!("vh2d/{safe}"))
}

fn custom_js(d: &Path) -> bool {
    let tpl = std::fs::read_to_string(library_dir().join("overlay-template").join("film.js")).unwrap_or_default();
    std::fs::read_to_string(d.join("film.js")).map(|s| s != tpl).unwrap_or(false)
}

/// Whether the reel has anything to draw over it, and the signature of what it draws.
fn signature(p: &Project, rid: &str, reel: &Value, tl: &Timeline) -> Option<String> {
    use std::hash::{Hash, Hasher};
    let d = dir(p, rid);
    let it = items(&d);
    if it.is_empty() && !custom_js(&d) {
        return None;
    }
    let mut hs = std::collections::hash_map::DefaultHasher::new();
    for f in ["film.js", "tweaks.json", "index.html", "linnea2d.js"] {
        std::fs::read(d.join(f)).unwrap_or_default().hash(&mut hs);
    }
    if let Ok(rd) = std::fs::read_dir(d.join("vh2d")) {
        for e in rd.flatten() {
            e.file_name().hash(&mut hs);
            e.metadata().map(|m| m.len()).unwrap_or(0).hash(&mut hs);
        }
    }
    format!("{}x{}@{}:{:.3}", jfd(reel, "width", 1080.0), jfd(reel, "height", 1920.0), tl.fps, tl.total).hash(&mut hs);
    Some(format!("{:x}", hs.finish()))
}

fn url(d: &Path) -> String {
    format!("file:///{}", fwd(&d.join("index.html")).trim_start_matches('/'))
}

/// The cached layer video for the reel as it is now, if it was rendered.
pub fn cached(p: &Project, rid: &str, reel: &Value, tl: &Timeline) -> Option<(PathBuf, bool)> {
    let sig = signature(p, rid, reel, tl)?;
    let f = p.p(&format!("build/overlay/{rid}-{sig}.mov"));
    Some((f.clone(), f.is_file()))
}

/// Render the layer page to a transparent video (QuickTime Animation with alpha), cached.
pub fn ensure_video(cfg: &Config, p: &Project, rid: &str, reel: &Value, tl: &Timeline, rep: &Rep) -> Result<Option<PathBuf>> {
    let Some((f, ok)) = cached(p, rid, reel, tl) else { return Ok(None) };
    if ok {
        return Ok(Some(f));
    }
    let (w, h) = (jfd(reel, "width", 1080.0) as u32, jfd(reel, "height", 1920.0) as u32);
    let d = ensure_page(p, rid, w, h)?;
    let ff = need_tool(cfg, "ffmpeg")?;
    let mut page = Page::open(cfg, &url(&d), w, h, true)?;
    page.transparent()?;
    page.wait_ready(Duration::from_secs(60))?;
    std::fs::create_dir_all(f.parent().unwrap())?;
    let tmp = f.with_extension("part.mov");
    let n = (tl.total * tl.fps).round().max(1.0) as usize;
    let mut ch = proc::cmd_low(&ff)
        .args(["-hide_banner", "-y", "-v", "error", "-f", "image2pipe", "-framerate", &format!("{}", tl.fps), "-c:v", "png", "-i", "-", "-c:v", "qtrle", "-pix_fmt", "argb"])
        .arg(&tmp)
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    let mut si = ch.stdin.take().unwrap();
    for k in 0..n {
        rep.check()?;
        let png = page.frame(k as f64 / tl.fps, 0, 1.0)?;
        si.write_all(&png)?;
        if k % 10 == 0 {
            rep.p(k as f64 / n as f64, "layers");
        }
    }
    drop(si);
    let o = ch.wait_with_output()?;
    if !o.status.success() {
        bail!("layer video: {}", proc::err_tail(&String::from_utf8_lossy(&o.stderr), 3));
    }
    crate::util::rename_retry(&tmp, &f)?;
    // keep only the current one
    if let Ok(rd) = std::fs::read_dir(f.parent().unwrap()) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n.starts_with(&format!("{rid}-")) && e.path() != f {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    Ok(Some(f))
}

/// One transparent frame of the layers at edit time t (for exact snapshots).
pub fn frame(cfg: &Config, p: &Project, rid: &str, reel: &Value, tl: &Timeline, t: f64) -> Result<Option<PathBuf>> {
    if signature(p, rid, reel, tl).is_none() {
        return Ok(None);
    }
    let (w, h) = (jfd(reel, "width", 1080.0) as u32, jfd(reel, "height", 1920.0) as u32);
    let d = ensure_page(p, rid, w, h)?;
    let mut page = Page::open(cfg, &url(&d), w, h, false)?;
    page.transparent()?;
    page.wait_ready(Duration::from_secs(30))?;
    let png = page.frame(t, 0, 1.0)?;
    let f = p.p("build/overlay/frame.png");
    std::fs::create_dir_all(f.parent().unwrap())?;
    std::fs::write(&f, png)?;
    Ok(Some(f))
}

fn t_layers(c: &Ctx, a: &Value) -> Result<Value> {
    let p = Project::open(&c.cfg, jsd(a, "project", ""))?;
    let rid = p.rid(js(a, "reel"));
    let reel = p.load_reel(&rid)?;
    let d = ensure_page(&p, &rid, jfd(&reel, "width", 1080.0) as u32, jfd(&reel, "height", 1920.0) as u32)?;
    if let Some(it) = a.get("items").filter(|x| x.is_array()) {
        let mut tw = read_json_or(&d.join("tweaks.json"), json!({}));
        tw["items2d"] = it.clone();
        write_json(&d.join("tweaks.json"), &tw)?;
    }
    if let Some(e) = a.get("edits").filter(|x| x.is_object()) {
        let mut tw = read_json_or(&d.join("tweaks.json"), json!({}));
        tw["edits"] = e.clone();
        write_json(&d.join("tweaks.json"), &tw)?;
    }
    if let (Some(n), Some(data)) = (js(&a["upload"], "name"), js(&a["upload"], "data")) {
        return Ok(json!({"src": save_upload(&d, n, data)?}));
    }
    if let Some(src) = js(a, "add_image") {
        let f = PathBuf::from(src);
        if !f.is_file() {
            bail!("no such picture: {src}");
        }
        std::fs::create_dir_all(d.join("vh2d"))?;
        let name = f.file_name().unwrap_or_default().to_string_lossy().to_string();
        std::fs::copy(&f, d.join("vh2d").join(&name))?;
        return Ok(json!({"src": format!("vh2d/{name}")}));
    }
    Ok(json!({
        "items": items(&d), "edits": read_json_or(&d.join("tweaks.json"), json!({}))["edits"], "url": format!("/overlay/{}/{}/index.html", p.id(), rid), "dir": fwd(&d), "size": [reel["width"], reel["height"]],
        "note": "items = [{id, kind: text|image|rect|counter|marker|arrow|phone|lower|progress|puppet, t0, t1 (EDIT seconds), x, y, w, h (px), rot, scale, opacity, fade, anim, parent, keys: [{t, x, y, w, h, rot, scale, opacity, ease: linear|in|out|inOut|back}], text, sub, size, weight, color, bg, stroke, radius, from, to, prefix, suffix, src}]. A puppet layer: {kind: 'puppet', puppet: name (puppet_link project=Ã¢â‚¬Â¦ reel=Ã¢â‚¬Â¦ name=Ã¢â‚¬Â¦ first), params, states: [{state, t0}]}. Custom animation: edit film.js in `dir` (renderOut(t) on a transparent canvas)."
    }))
}

pub const TOOLS: &[Tool] = &[Tool {
    name: "reel_layers",
    desc: "Layers over a footage reel, edited by hand in the editor or here: titles, pictures, counters, handwriting, arrows, a phone mockup, lower thirds, progress bars; each can be keyframed (keys) and rigged to a parent. Times are EDIT seconds. Also returns the layer page folder, whose film.js can hold any custom animation. Rendered as a transparent layer on top of the reel.",
    params: &[("project!", "string", "Project"), ("reel", "string", "Reel id"), ("items", "array", "Replace the layer list"), ("add_image", "string", "Copy a picture in (returns its src)"), ("upload", "object", "{name, data (data URL)}: a dropped file")],
    long: false,
    app_only: false,
    run: t_layers,
}];

