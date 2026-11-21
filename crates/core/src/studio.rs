//! The edits a person makes without code, kept in the film's `tweaks.json` as VideoHub does:
//! `sound` and `audio.clips` (the sound layer mixer, applied by rebuilding audio/mix.wav),
//! `items2d` (2D layers drawn over the film by the linnea2d.js hook, in the preview and in renders),
//! and the Thumbnail Studio (`thumbnails/<id>/`: a laid-out brief, variants Claude makes, the final).
use crate::config::*;
use crate::film::{self, Film};
use crate::proc;
use crate::tools::{Ctx, Tool};
use crate::util::*;
use anyhow::{anyhow, bail, Result};
use base64::Engine;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn film_of(c: &Ctx, a: &Value) -> Result<Film> {
    film::open(&c.cfg, js(a, "film").or_else(|| js(a, "project")).unwrap_or(""))
}
fn tweaks_path(f: &Film) -> PathBuf {
    f.dir.join("tweaks.json")
}
pub fn tweaks(f: &Film) -> Value {
    read_json_or(&tweaks_path(f), json!({}))
}
/// Merge into tweaks.json without touching keys this call does not name.
fn save_tweaks(f: &Film, patch: &Value) -> Result<Value> {
    let mut t = tweaks(f);
    for (k, v) in patch.as_object().into_iter().flatten() {
        t[k] = v.clone();
    }
    write_json(&tweaks_path(f), &t)?;
    Ok(t)
}
fn data_url_bytes(s: &str) -> Result<Vec<u8>> {
    let b = s.split_once("base64,").map(|x| x.1).unwrap_or(s);
    Ok(base64::engine::general_purpose::STANDARD.decode(b.trim())?)
}

// ---------- sound layers ----------

const VOICE: &[&str] = &["audio/st_vo.wav", "data/vo.wav", "audio/vo.wav", "audio/voice.wav", "stems/vo.wav"];
const MUSIC: &[&str] = &["audio/music.wav", "audio/st_bed.wav", "audio/bed.wav", "stems/music.wav"];
const SFX: &[&str] = &["audio/sfx.wav", "audio/st_sfx.wav", "stems/sfx.wav"];

/// The film's sound layers: stems when it has them, else its one mix (the untouched original).
fn layers(c: &Ctx, f: &Film) -> (Vec<Value>, Option<PathBuf>) {
    let info = film::info(&c.cfg, f);
    let mix = js(&info, "audio").map(PathBuf::from);
    let mut out = vec![];
    let mut seen: Vec<PathBuf> = vec![];
    for (kind, list) in [("voice", VOICE), ("music", MUSIC), ("sfx", SFX)] {
        if let Some(p) = list.iter().map(|r| f.dir.join(r)).find(|p| p.is_file()) {
            seen.push(p.clone());
            out.push(json!({"id": kind, "kind": kind, "file": fwd(&p)}));
        }
    }
    if let Ok(rd) = std::fs::read_dir(f.dir.join("stems")) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().map(|x| x == "wav").unwrap_or(false) && !seen.contains(&p) {
                let id = p.file_stem().unwrap_or_default().to_string_lossy().to_string();
                out.push(json!({"id": id, "kind": "stem", "file": fwd(&p)}));
            }
        }
    }
    if out.is_empty() {
        if let Some(m) = &mix {
            // a mix Linnea built is not a sound of the film's own: its original is the copy kept before the
            // first build, or nothing when the film had no sound and the mix is only the clips
            let orig = orig_of(m);
            let src = if !made_here(m) { Some(m.clone()) } else if orig.is_file() { Some(orig) } else { None };
            if let Some(src) = src {
                out.push(json!({"id": "mix", "kind": "mix", "file": fwd(&src)}));
            }
        }
    }
    (out, mix)
}
fn orig_of(mix: &Path) -> PathBuf {
    mix.with_file_name(format!("{}.vhorig.wav", mix.file_stem().unwrap_or_default().to_string_lossy()))
}
fn made_of(mix: &Path) -> PathBuf {
    mix.with_file_name(format!("{}.vhmade.json", mix.file_stem().unwrap_or_default().to_string_lossy()))
}
fn stamp(p: &Path) -> Value {
    let m = std::fs::metadata(p).ok();
    let ms = m.as_ref().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0);
    json!({"len": m.map(|m| m.len()).unwrap_or(0), "modified": ms})
}
/// Whether the mix on disk is the one Linnea built last. When someone (Claude, an editor) writes a new mix,
/// that file is the film's own sound again. Films mixed before this was recorded count as built here when
/// they kept an original.
fn made_here(mix: &Path) -> bool {
    match std::fs::read_to_string(made_of(mix)).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()) {
        Some(v) => v == stamp(mix),
        None => orig_of(mix).is_file(),
    }
}

fn t_sound(c: &Ctx, a: &Value) -> Result<Value> {
    let f = film_of(c, a)?;
    let mut tw = tweaks(&f);
    if let Some(set) = a.get("set").filter(|s| s.is_object()) {
        let mut s = tw["sound"].clone();
        if !s.is_object() {
            s = json!({});
        }
        merge(&mut s, set);
        tw = save_tweaks(&f, &json!({"sound": s}))?;
    }
    let mut new_clips = a.get("clips").filter(|x| x.is_array()).cloned();
    // one clip changed by its id: {id, gain, mute, at, name}, without sending the whole list
    if let Some(one) = a.get("clip").filter(|x| x.is_object()) {
        let id = jsd(one, "id", "");
        let mut list = ja(&tw["audio"], "clips").to_vec();
        let Some(c) = list.iter_mut().find(|c| jsd(c, "id", "") == id) else { bail!("no sound clip with id {id}; hub_sound lists them") };
        for k in ["gain", "mute", "at", "name"] {
            if let Some(v) = one.get(k).filter(|v| !v.is_null()) {
                c[k] = v.clone();
            }
        }
        new_clips = Some(json!(list));
    }
    if let Some(clips) = new_clips {
        let mut au = tw["audio"].clone();
        if !au.is_object() {
            au = json!({});
        }
        // every clip keeps an id, so it can be found, muted and turned up or down on its own
        let clips: Vec<Value> = clips.as_array().cloned().unwrap_or_default().into_iter().map(|mut c| {
            if jsd(&c, "id", "").is_empty() {
                c["id"] = json!(new_id("a"));
            }
            // what hub_sound works out (length, full path) is not stored
            if let Some(o) = c.as_object_mut() {
                o.remove("dur");
                o.remove("path");
            }
            c
        }).collect();
        au["clips"] = json!(clips);
        tw = save_tweaks(&f, &json!({"audio": au}))?;
        if jb(a, "apply") == Some(true) {
            apply_sound(&c.cfg, c, &f)?;
        }
    }
    let (ls, mix) = layers(c, &f);
    // how long each clip plays, for the timeline
    let clips: Vec<Value> = ja(&tw["audio"], "clips").iter().map(|cl| {
        let mut cl = cl.clone();
        cl["path"] = json!(fwd(&f.dir.join(jsd(&cl, "file", ""))));
        if let Some(d) = film::audio_len(&c.cfg, &f.dir.join(jsd(&cl, "file", ""))) {
            cl["dur"] = json!((d * 100.0).round() / 100.0);
        }
        cl
    }).collect();
    Ok(json!({"layers": ls, "mix": mix.map(|m| fwd(&m)), "sound": tw["sound"], "clips": clips,
              "note": "sound = {levels: {layer: dB}, mute: [layer], solo: layer or clip id, removeMusic, duck: [{from, to, db}]}; clips = [{id, file, at, gain (dB), mute, name}]; clip={id, gain|mute|at|name} changes one (apply=true rebuilds the mix at once). hub_sound_apply rebuilds the mix the preview and renders use."}))
}

fn db_lin(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

pub fn apply_sound(cfg: &Config, c: &Ctx, f: &Film) -> Result<Value> {
    let ff = need_tool(cfg, "ffmpeg")?;
    let info = film::info(cfg, f);
    let dur = jfd(&info, "dur", 10.0);
    let tw = tweaks(f);
    let s = &tw["sound"];
    let (ls, mix) = layers(c, f);
    let mix = mix.unwrap_or_else(|| f.dir.join("audio").join("mix.wav"));
    if ls.is_empty() && ja(&tw["audio"], "clips").is_empty() {
        bail!("the film has no sound yet: put audio/mix.wav or stems in it, or add clips");
    }
    // Keep the untouched original once, as VideoHub does; a mix someone wrote since the last build is the
    // new original
    let orig = orig_of(&mix);
    if mix.is_file() && !made_here(&mix) {
        std::fs::copy(&mix, &orig)?;
    }
    let muted: Vec<String> = ja(s, "mute").iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect();
    let solo = jsd(s, "solo", "").to_string();
    let mut args: Vec<String> = vec![];
    let mut g: Vec<String> = vec![];
    let mut labels: Vec<String> = vec![];
    let mut n = 0;
    for l in &ls {
        let id = jsd(l, "id", "");
        let kind = jsd(l, "kind", "");
        if muted.iter().any(|m| m == id) || (!solo.is_empty() && solo != id) || (kind == "music" && jb(s, "removeMusic") == Some(true)) {
            continue;
        }
        let file = if kind == "mix" && orig.is_file() { fwd(&orig) } else { jsd(l, "file", "").to_string() };
        args.extend(["-i".into(), file]);
        let mut chain = format!("[{n}:a]aformat=sample_rates=48000:channel_layouts=stereo,volume={:.4}", db_lin(jfd(&s["levels"], id, 0.0)));
        if kind == "music" || (kind == "mix" && ls.len() == 1) {
            for d in ja(s, "duck") {
                chain.push_str(&format!(",volume=enable='between(t,{:.3},{:.3})':volume={:.4}", jfd(d, "from", 0.0), jfd(d, "to", 0.0), db_lin(jfd(d, "db", -8.0))));
            }
        }
        g.push(format!("{chain}[l{n}]"));
        labels.push(format!("[l{n}]"));
        n += 1;
    }
    for cl in ja(&tw["audio"], "clips") {
        let p = f.dir.join(jsd(cl, "file", ""));
        if !p.is_file() || jb(cl, "mute") == Some(true) || (!solo.is_empty() && solo != jsd(cl, "id", "")) {
            continue;
        }
        args.extend(["-i".into(), fwd(&p)]);
        g.push(format!("[{n}:a]aformat=sample_rates=48000:channel_layouts=stereo,volume={:.4},adelay={}:all=1[l{n}]", db_lin(jfd(cl, "gain", 0.0)), (jfd(cl, "at", 0.0) * 1000.0).round() as i64));
        labels.push(format!("[l{n}]"));
        n += 1;
    }
    if labels.is_empty() {
        args.extend(["-f".into(), "lavfi".into(), "-i".into(), "anullsrc=r=48000:cl=stereo".into()]);
        g.push(format!("[{n}:a]anull[l{n}]"));
        labels.push(format!("[l{n}]"));
    }
    g.push(format!("{}amix=inputs={}:normalize=0:duration=longest:dropout_transition=0,alimiter=limit=0.95,apad[out]", labels.concat(), labels.len()));
    std::fs::create_dir_all(mix.parent().unwrap())?;
    let tmp = mix.with_extension("tmp.wav");
    args.extend(["-filter_complex".into(), g.join(";"), "-map".into(), "[out]".into(), "-t".into(), format!("{dur:.3}"), "-c:a".into(), "pcm_s16le".into(), fwd(&tmp)]);
    proc::ffmpeg(&ff, &args, Some(&f.dir), 0.0, &proc::Rep::none(), "mix")?;
    std::fs::rename(&tmp, &mix)?;
    write_json(&made_of(&mix), &stamp(&mix))?;
    Ok(json!({"mix": fwd(&mix), "original": orig.is_file().then(|| fwd(&orig)), "layers": n, "note": "the preview and the next render use the rebuilt mix"}))
}

fn t_sound_apply(c: &Ctx, a: &Value) -> Result<Value> {
    let f = film_of(c, a)?;
    apply_sound(&c.cfg, c, &f)
}

fn t_clip_add(c: &Ctx, a: &Value) -> Result<Value> {
    let f = film_of(c, a)?;
    // a file in the film folder stays where it is; a library sound (sound=whoosh) or any other file is copied in
    let (rel, name) = if let Some(id) = js(a, "sound").filter(|s| !s.is_empty()) {
        let Some(snd) = crate::sfx::resolve(&c.cfg, id) else { bail!("no sound '{id}' in the library (`library` lists them)") };
        let name = snd.path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let dst = f.dir.join("vhaudio").join(&name);
        if !dst.exists() {
            std::fs::create_dir_all(dst.parent().unwrap())?;
            std::fs::copy(&snd.path, &dst)?;
        }
        (format!("vhaudio/{name}"), id.to_string())
    } else {
        let given = jsd(a, "file", "");
        let inside = f.dir.join(given);
        let src = if !given.is_empty() && inside.is_file() { inside } else { PathBuf::from(given) };
        if !src.is_file() {
            bail!("no such sound: {given} (give a file in the film folder, a full path, or sound=<library id>)");
        }
        let name = src.file_name().unwrap_or_default().to_string_lossy().to_string();
        match src.strip_prefix(&f.dir) {
            Ok(r) => (fwd(r), name),
            Err(_) => {
                let dst = f.dir.join("vhaudio").join(&name);
                if !dst.exists() {
                    std::fs::create_dir_all(dst.parent().unwrap())?;
                    std::fs::copy(&src, &dst)?;
                }
                (format!("vhaudio/{name}"), name)
            }
        }
    };
    let mut tw = tweaks(&f);
    let mut clips = ja(&tw["audio"], "clips").to_vec();
    let id = new_id("a");
    clips.push(json!({"id": id, "file": rel, "name": jsd(a, "name", &name), "at": jfd(a, "at", 0.0), "gain": jfd(a, "gain", 0.0)}));
    if !tw["audio"].is_object() {
        tw["audio"] = json!({});
    }
    tw["audio"]["clips"] = json!(clips);
    save_tweaks(&f, &json!({"audio": tw["audio"]}))?;
    if jb(a, "apply") == Some(true) {
        apply_sound(&c.cfg, c, &f)?;
    }
    Ok(json!({"id": id, "clips": clips}))
}

// ---------- 2D layers ----------

const HOOK: &str = "linnea2d.js";

fn hook_installed(f: &Film, page: &str) -> bool {
    std::fs::read_to_string(f.dir.join(page)).map(|s| s.contains(HOOK) || s.contains(crate::layers::OLD_HOOK)).unwrap_or(false)
}

/// Put the newest engine next to the page; a film from before the rename gets its script tag renamed too.
fn refresh_hook(f: &Film, page: &str) -> Result<()> {
    crate::layers::copy_if_changed(&library_dir().join("film-hooks").join(HOOK), &f.dir.join(HOOK))?;
    crate::layers::rename_old_hook(&f.dir, page);
    Ok(())
}

fn install_hook(cfg: &Config, f: &Film) -> Result<()> {
    let info = film::info(cfg, f);
    let page = jsd(&info, "page", "index.html").to_string();
    refresh_hook(f, &page)?;
    if !f.dir.join("tweaks.json").is_file() {
        write_json(&f.dir.join("tweaks.json"), &json!({}))?;
    }
    if !hook_installed(f, &page) {
        let p = f.dir.join(&page);
        let s = std::fs::read_to_string(&p)?;
        let tag = format!("<script src=\"{HOOK}\"></script>\n");
        let s = match s.rfind("</body>") {
            Some(i) => format!("{}{tag}{}", &s[..i], &s[i..]),
            None => format!("{s}\n{tag}"),
        };
        std::fs::write(&p, s)?;
    }
    Ok(())
}

fn t_2d(c: &Ctx, a: &Value) -> Result<Value> {
    let f = film_of(c, a)?;
    let info = film::info(&c.cfg, &f);
    if jb(a, "install") == Some(true) {
        install_hook(&c.cfg, &f)?;
    }
    if let Some(items) = a.get("items").filter(|x| x.is_array()) {
        save_tweaks(&f, &json!({"items2d": items}))?;
    }
    if let Some(e) = a.get("edits").filter(|x| x.is_object()) {
        save_tweaks(&f, &json!({"edits": e}))?;
    }
    // keep the hook current, so older films get the newest engine
    if hook_installed(&f, jsd(&info, "page", "index.html")) {
        let _ = refresh_hook(&f, jsd(&info, "page", "index.html"));
    }
    if let (Some(n), Some(data)) = (js(&a["upload"], "name"), js(&a["upload"], "data")) {
        return Ok(json!({"src": crate::layers::save_upload(&f.dir, n, data)?}));
    }
    if let Some(src) = js(a, "add_image") {
        let p = PathBuf::from(src);
        if !p.is_file() {
            bail!("no such picture: {src}");
        }
        let dir = f.dir.join("vh2d");
        std::fs::create_dir_all(&dir)?;
        let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
        std::fs::copy(&p, dir.join(&name))?;
        return Ok(json!({"src": format!("vh2d/{name}")}));
    }
    Ok(json!({"items": tweaks(&f)["items2d"].as_array().cloned().unwrap_or_default(), "edits": tweaks(&f)["edits"], "installed": hook_installed(&f, jsd(&info, "page", "index.html")),
              "size": [info["width"], info["height"]],
              "note": "items2d = [{id, label, kind: text|image|rect|ellipse|counter|marker|arrow|phone|lower|progress|puppet, t0, t1, x, y, w, h (px), rot, scale, opacity, fade, anim / animOut: none|fade|slide-up|slide-down|slide-left|slide-right|pop|zoom|spin|blur|wipe, parent, hidden, locked, keys: [{t, x?, y?, w?, h?, rot?, scale?, opacity?, ease: linear|in|out|inOut|back}], text, sub, font, weight, size, italic, align, spacing, lineHeight, upper, typing: chars|words (over reveal s), color, stroke, strokeWidth, bg, bg2 + gradAngle (gradient), radius, border, borderWidth, shadow, shadowBlur, shadowX, shadowY, blur, blend, fx, fy, fit: cover|contain, from, to, prefix, suffix, decimals, src}]. A puppet layer: {kind: 'puppet', puppet: name (puppet_link it first), params: {..}, states: [{state, t0 (seconds from the layer's t0), blend}], speed, pkeys: [{t (film s), p: {param: number}, ease}]}. `items` replaces the whole list: read it, change it, send it back. Drawn over the film by linnea2d.js in the preview and in renders."}))
}

// ---------- thumbnails ----------

fn thumbs_root(c: &Ctx, a: &Value) -> Result<PathBuf> {
    Ok(match js(a, "film").or_else(|| js(a, "project")).filter(|s| !s.is_empty()) {
        Some(_) => film_of(c, a)?.dir.join("thumbnails"),
        None => c.cfg.workspace().join("thumbnails"),
    })
}
fn thumb_dir(c: &Ctx, a: &Value) -> Result<PathBuf> {
    let id = js(a, "id").ok_or_else(|| anyhow!("`id` is the thumbnail id (hub_thumbs)"))?;
    let d = thumbs_root(c, a)?.join(id);
    if !d.join("thumb.json").is_file() {
        bail!("no thumbnail '{id}'");
    }
    Ok(d)
}

fn images_in(d: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(d)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| matches!(p.extension().and_then(|x| x.to_str()).unwrap_or("").to_lowercase().as_str(), "png" | "jpg" | "jpeg" | "webp")).map(|p| fwd(&p)).collect())
        .unwrap_or_default();
    v.sort();
    v
}

fn thumb_summary(d: &Path) -> Value {
    let t = read_json_or(&d.join("thumb.json"), json!({}));
    json!({"id": d.file_name().unwrap_or_default().to_string_lossy(), "title": t["title"], "template": t["template"], "format": t["format"], "status": t["status"],
           "variants": images_in(&d.join("variants")), "final": d.join("final.png").is_file().then(|| fwd(&d.join("final.png"))), "dir": fwd(d)})
}

fn t_thumbs(c: &Ctx, a: &Value) -> Result<Value> {
    let root = thumbs_root(c, a)?;
    let list: Vec<Value> = std::fs::read_dir(&root).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.join("thumb.json").is_file()).map(|p| thumb_summary(&p)).collect()).unwrap_or_default();
    let templates: Vec<Value> = std::fs::read_dir(library_dir().join("thumbnail-templates"))
        .map(|rd| {
            let mut v: Vec<Value> = rd.flatten().filter_map(|e| read_json(&e.path()).ok()).filter(|t| t["kind"] == json!("thumbstudio-template")).map(|t| json!({"id": t["id"], "name": t["name"], "desc": t["desc"], "format": t["format"], "tags": t["tags"], "slots": t["slots"]})).collect();
            v.sort_by(|x, y| jsd(x, "name", "").cmp(jsd(y, "name", "")));
            v
        })
        .unwrap_or_default();
    Ok(json!({"thumbnails": list, "templates": templates, "folder": fwd(&root)}))
}

fn t_thumb_new(c: &Ctx, a: &Value) -> Result<Value> {
    let root = thumbs_root(c, a)?;
    let tpl = js(a, "template").map(|t| read_json(&library_dir().join("thumbnail-templates").join(format!("{t}.json")))).transpose()?;
    let title = jsd(a, "title", "Thumbnail").to_string();
    let mut doc = tpl.clone().unwrap_or(json!({"format": jsd(a, "format", "16:9"), "layers": [], "slots": {}}));
    let mut values = json!({});
    for (k, s) in doc["slots"].as_object().cloned().unwrap_or_default() {
        values[&k] = a["slots"].get(&k).cloned().unwrap_or_else(|| s["default"].clone());
    }
    doc["kind"] = json!("thumbstudio");
    doc["title"] = json!(title);
    doc["template"] = tpl.as_ref().map(|t| t["id"].clone()).unwrap_or(Value::Null);
    doc["values"] = values;
    doc["status"] = json!("draft");
    if let Some(f) = js(a, "format") {
        doc["format"] = json!(f);
    }
    let base = slug(&title);
    let mut d = root.join(&base);
    let mut n = 2;
    while d.exists() {
        d = root.join(format!("{base}-{n}"));
        n += 1;
    }
    std::fs::create_dir_all(d.join("variants"))?;
    write_json(&d.join("thumb.json"), &doc)?;
    Ok(thumb_summary(&d))
}

fn t_thumb(c: &Ctx, a: &Value) -> Result<Value> {
    let d = thumb_dir(c, a)?;
    let p = d.join("thumb.json");
    let mut doc = read_json(&p)?;
    if let Some(set) = a.get("set").filter(|s| s.is_object()) {
        merge(&mut doc, set);
        write_json(&p, &doc)?;
    }
    for (key, file) in [("layout", "layout.png"), ("sketch", "sketch.png"), ("final", "final.png")] {
        if let Some(s) = js(a, key) {
            std::fs::write(d.join(file), data_url_bytes(s)?)?;
        }
    }
    let mut out = thumb_summary(&d);
    out["doc"] = doc;
    out["notes"] = read_json_or(&d.join("notes.json"), json!([]));
    Ok(out)
}

fn fill(s: &str, values: &Value) -> String {
    let mut out = s.to_string();
    for (k, v) in values.as_object().into_iter().flatten() {
        out = out.replace(&format!("{{{{{k}}}}}"), v.as_str().unwrap_or(""));
    }
    out
}

fn t_brief(c: &Ctx, a: &Value) -> Result<Value> {
    let d = thumb_dir(c, a)?;
    let doc = read_json(&d.join("thumb.json"))?;
    let vals = &doc["values"];
    let pos = |l: &Value| match (jf(l, "x"), jf(l, "y"), jf(l, "w"), jf(l, "h")) {
        (Some(x), Some(y), Some(w), Some(h)) => {
            let col = if x + w / 2.0 < 0.38 { "left" } else if x + w / 2.0 > 0.62 { "right" } else { "centre" };
            let row = if y + h / 2.0 < 0.38 { "top" } else if y + h / 2.0 > 0.62 { "bottom" } else { "middle" };
            format!(" at the {row} {col}, about {:.0}% of the width", w * 100.0)
        }
        _ => String::new(),
    };
    let layers: Vec<String> = ja(&doc, "layers").iter().map(|l| format!("- {} ({}): {}{}", jsd(l, "name", ""), jsd(l, "type", ""), fill(jsd(l, "desc", ""), vals), pos(l))).collect();
    let texts: Vec<String> = ja(&doc, "layers").iter().filter(|l| jsd(l, "type", "") == "text").map(|l| fill(jsd(l, "text", jsd(l, "desc", "")), vals)).collect();
    let cam = &doc["camera"];
    let light = &doc["light"];
    let prompt = format!(
        "{} thumbnail, {} format. {}. Camera: {} shot, {} angle, {}mm. Light: {} key, {}{}. Style: {}, palette {}. {}",
        jsd(&doc, "name", "A video"), jsd(&doc, "format", "16:9"),
        ja(&doc, "layers").iter().filter(|l| jsd(l, "type", "") != "text").map(|l| fill(jsd(l, "desc", ""), vals)).collect::<Vec<_>>().join("; "),
        jsd(cam, "shot", "medium"), jsd(cam, "angle", "eye"), jfd(cam, "lens", 35.0), jsd(light, "key", "neutral"), jsd(light, "time", "day"), if jb(light, "rim") == Some(true) { ", rim light" } else { "" },
        jsd(&doc["style"], "preset", "photo"), ja(&doc["style"], "palette").iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(" "),
        if jsd(&doc["text"], "policy", "overlay") == "overlay" { "Leave clean space for the title; NO text in the image (the title is added afterwards in code)." } else { "" }
    );
    let notes: Vec<Value> = read_json_or(&d.join("notes.json"), json!([])).as_array().cloned().unwrap_or_default().into_iter().filter(|n| jsd(n, "status", "open") == "open").collect();
    let refs: Vec<String> = images_in(&d.join("refs"));
    Ok(json!({
        "goal": format!("Make variants of the thumbnail \"{}\". Save each image anywhere and register it with hub_thumb_variant; the user picks one and the title text is added in Linnea.", jsd(&doc, "title", "")),
        "format": doc["format"], "slots": vals, "camera": cam, "light": light, "style": doc["style"], "layers_in_words": layers, "title_text": texts,
        "prompt_draft": prompt, "references": refs, "layout_png": d.join("layout.png").is_file().then(|| fwd(&d.join("layout.png"))),
        "sketch_png": d.join("sketch.png").is_file().then(|| fwd(&d.join("sketch.png"))), "open_variant_notes": notes, "variants": images_in(&d.join("variants")),
    }))
}

fn t_variant(c: &Ctx, a: &Value) -> Result<Value> {
    let d = thumb_dir(c, a)?;
    let src = PathBuf::from(jsd(a, "file", ""));
    if !src.is_file() {
        bail!("no such image: {}", src.display());
    }
    let ext = src.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_else(|| "png".into());
    let n = images_in(&d.join("variants")).len() + 1;
    let dst = d.join("variants").join(format!("v{n:02}.{ext}"));
    std::fs::create_dir_all(d.join("variants"))?;
    std::fs::copy(&src, &dst)?;
    Ok(json!({"variant": fwd(&dst), "note": "the user sees it in the open Thumbnail Studio now"}))
}

fn t_thumb_note(c: &Ctx, a: &Value) -> Result<Value> {
    let d = thumb_dir(c, a)?;
    let p = d.join("notes.json");
    let mut n = read_json_or(&p, json!([])).as_array().cloned().unwrap_or_default();
    if let Some(id) = js(a, "note") {
        if let Some(x) = n.iter_mut().find(|x| jsd(x, "id", "") == id) {
            for k in ["status", "reply"] {
                if let Some(v) = a.get(k) {
                    x[k] = v.clone();
                }
            }
        }
    } else {
        n.push(json!({"id": new_id("tn"), "variant": a["variant"], "text": jsd(a, "text", ""), "status": "open", "created": now_ms(), "by": c.origin}));
    }
    write_json(&p, &json!(n))?;
    Ok(json!({"notes": n}))
}

const FO: (&str, &str, &str) = ("film", "string", "Film id; without it the shared thumbnails folder");
const FR: (&str, &str, &str) = ("film!", "string", "Film id");

pub const TOOLS: &[Tool] = &[
    Tool { name: "hub_sound", desc: "The film's sound: its layers (stems: voice, music, sfx, stems/*.wav; else its mix), the mixer settings and the sound clips on its timeline (each with id, file, at, gain dB, mute, name, dur), from tweaks.json. `set` merges mixer settings {levels: {layer: dB}, mute: [layer], solo: layer or clip id, removeMusic, duck: [{from, to, db}]}; `clip` changes one clip by id {id, gain, mute, at, name}; `clips` replaces the list. apply=true rebuilds the mix right after.", params: &[FR, ("set", "object", "Mixer settings to merge"), ("clip", "object", "One clip's changes: {id, gain, mute, at, name}"), ("clips", "array", "[{id, file, at, gain, mute, name}]"), ("apply", "boolean", "Rebuild the mix after the change")], long: false, app_only: false, run: t_sound },
    Tool { name: "hub_sound_apply", desc: "Rebuild audio/mix.wav from the sound layers, levels, mutes, ducking and clips (the original is kept once as <mix>.vhorig.wav). The preview and renders use the new mix.", params: &[FR], long: false, app_only: false, run: t_sound_apply },
    Tool { name: "hub_sound_clip", desc: "Put one sound on the film's timeline as its own clip, so the user can turn it up, down or off by itself in the app: sound=<library id> (whoosh, pop, ding... `library` lists them), or file= a file in the film folder (stays there) or anywhere (copied into vhaudio/). Add every effect, sting and music piece this way instead of mixing it into audio/mix.wav yourself. apply=true rebuilds the mix at once (else hub_sound_apply).", params: &[FR, ("sound", "string", "Library sound id"), ("file", "string", "Sound file (film folder or full path)"), ("at!", "number", "Second it starts"), ("gain", "number", "dB, 0 = as it is"), ("name", "string", "Label"), ("apply", "boolean", "Rebuild the mix now")], long: false, app_only: false, run: t_clip_add },
    Tool { name: "hub_2d", desc: "2D layers over the film (tweaks.json items2d): titles, labels, pictures, boxes, counters, arrows, lower thirds and library puppets, each with a time range, keyframes and an optional parent. The linnea2d.js hook draws them in the preview and in renders; install=true adds the hook to the page. `items` replaces the list; add_image copies a picture into vh2d/.", params: &[FR, ("items", "array", "The layers"), ("install", "boolean", "Add the hook to the page"), ("add_image", "string", "Picture to copy into vh2d/"), ("edits", "object", "The user's edits of elements the film draws through KE (or of plain text): {id: {dx, dy, scale, rot, opacity, hidden, t0, t1, text, color, size, keys}}")], long: false, app_only: false, run: t_2d },
    Tool { name: "hub_thumbs", desc: "The thumbnails of a film (or the shared area) and the Thumbnail Studio templates.", params: &[FO], long: false, app_only: false, run: t_thumbs },
    Tool { name: "hub_thumb_new", desc: "Start a thumbnail from a template (or blank), filling its slots.", params: &[FO, ("title!", "string", "Title"), ("template", "string", "Template id"), ("slots", "object", "Slot values"), ("format", "string", "16:9 | 9:16")], long: false, app_only: false, run: t_thumb_new },
    Tool { name: "hub_thumb", desc: "One thumbnail: its layout document, variants, final and notes. `set` merges into thumb.json; layout / sketch / final take PNG data URLs.", params: &[FO, ("id!", "string", "Thumbnail id"), ("set", "object", "Merge into thumb.json"), ("layout", "string", "PNG data URL"), ("sketch", "string", "PNG data URL"), ("final", "string", "PNG data URL")], long: false, app_only: false, run: t_thumb },
    Tool { name: "hub_thumb_brief", desc: "The thumbnail brief in one answer: goal, format, slots, camera, light, style, the layers in words with their positions, an English prompt draft, references, the layout picture and open variant notes. Make the images, then register each with hub_thumb_variant.", params: &[FO, ("id!", "string", "Thumbnail id")], long: false, app_only: false, run: t_brief },
    Tool { name: "hub_thumb_variant", desc: "Register an image made by the agent or another tool as a variant. It is copied into variants/ and shows up in the open editor at once.", params: &[FO, ("id!", "string", "Thumbnail id"), ("file!", "string", "Image path")], long: false, app_only: false, run: t_variant },
    Tool { name: "hub_thumb_note", desc: "A note on a variant (the user's change list), or with `note` an update of one (status, reply).", params: &[FO, ("id!", "string", "Thumbnail id"), ("variant", "string", "Variant file"), ("text", "string", "Note"), ("note", "string", "Note id to update"), ("status", "string", "open | fixed"), ("reply", "string", "Reply")], long: false, app_only: false, run: t_thumb_note },
];
