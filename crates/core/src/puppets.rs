//! Puppets: reusable drawings kept in a library and linked into films and reels.
//!
//! A `.ppet` is one drawing (a pencil, a phone, a character) as JSON: its native size, the parameters a user
//! may change (colour, label, length...) and a pure JS function `function (g, p, t, s, u)` that paints it in
//! its own box (0..size). A `.rppet` (rich puppet) adds `states`: named animations with a length and loop
//! flag (`idle`, `write`...); the function reads `s = {name, t, k, dur}` to know which one is playing and
//! how far. linnea2d.js draws them in films (KE.puppet in code, or a `puppet` layer placed by hand).
//!
//! The library is `<workspace>/puppets` (yours) over `library/puppets` (shipped samples). Linking copies the
//! file into the film's `puppets/` folder (so a film renders on its own) and records where it came from in
//! `puppets/links.json`, so `puppet_sync` can bring library updates in without losing local changes.
use crate::project::Project;
use crate::tools::{Ctx, Tool};
use crate::util::*;
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

const EXTS: [&str; 2] = ["rppet", "ppet"];

fn user_dir(c: &Ctx) -> PathBuf {
    c.cfg.workspace().join("puppets")
}
fn dirs(c: &Ctx) -> Vec<(PathBuf, &'static str)> {
    vec![(user_dir(c), "library"), (library_dir().join("puppets"), "sample")]
}

fn clean(name: &str) -> Result<String> {
    let n: String = name.trim().to_lowercase().chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect();
    if n.is_empty() || n.starts_with('.') {
        bail!("puppet needs a name (letters, digits, - and _)");
    }
    Ok(n)
}

/// A stable fingerprint of a puppet file (FNV-1a 64): the same on every build of the program.
fn hash(b: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for x in b {
        h ^= *x as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("f{h:016x}")
}
/// The fingerprint older versions wrote (Rust's default hasher, which may change between builds).
fn old_hash(b: &[u8]) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    b.hash(&mut h);
    format!("{:x}", h.finish())
}
/// Whether a recorded fingerprint is of these bytes, in either form.
fn same(known: &str, b: &[u8]) -> bool {
    !known.is_empty() && (known == hash(b) || known == old_hash(b))
}

/// The puppet file in a folder, whichever kind it is.
fn file_in(dir: &Path, name: &str) -> Option<PathBuf> {
    EXTS.iter().map(|e| dir.join(format!("{name}.{e}"))).find(|p| p.is_file())
}

fn find(c: &Ctx, name: &str) -> Option<(PathBuf, &'static str)> {
    dirs(c).into_iter().find_map(|(d, w)| file_in(&d, name).map(|f| (f, w)))
}

fn summary(f: &Path, v: &Value, where_: &str) -> Value {
    let keys = |k: &str| v[k].as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()).unwrap_or_default();
    json!({
        "name": f.file_stem().unwrap_or_default().to_string_lossy(),
        "kind": f.extension().unwrap_or_default().to_string_lossy(),
        "label": v["label"], "desc": v["desc"], "tags": v["tags"], "size": v["size"],
        "params": v["params"], "states": keys("states"), "version": v["version"], "where": where_, "file": fwd(f),
    })
}

/// Where a film's or a reel's puppets live.
fn target(c: &Ctx, a: &Value) -> Result<PathBuf> {
    if let Some(r) = js(a, "reel") {
        let p = Project::open(&c.cfg, jsd(a, "project", ""))?;
        let rid = p.rid(Some(r));
        return Ok(crate::layers::dir(&p, &rid).join("puppets"));
    }
    let f = crate::film::open(&c.cfg, js(a, "film").unwrap_or(""))?;
    Ok(f.dir.join("puppets"))
}

fn t_list(c: &Ctx, a: &Value) -> Result<Value> {
    let q = jsd(a, "query", "").to_lowercase();
    let mut seen = std::collections::HashSet::new();
    let mut out = vec![];
    for (d, w) in dirs(c) {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        let mut fs: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().map(|e| EXTS.contains(&e.to_string_lossy().as_ref())).unwrap_or(false)).collect();
        fs.sort();
        for f in fs {
            let name = f.file_stem().unwrap_or_default().to_string_lossy().to_string();
            if !seen.insert(name.clone()) {
                continue;
            }
            let v = read_json_or(&f, json!({}));
            let s = summary(&f, &v, w);
            if !q.is_empty() && !s.to_string().to_lowercase().contains(&q) {
                continue;
            }
            out.push(s);
        }
    }
    let mut res = json!({"puppets": out, "library": fwd(&user_dir(c))});
    if js(a, "film").is_some() || js(a, "reel").is_some() {
        let t = target(c, a)?;
        res["linked"] = read_json_or(&t.join("links.json"), json!({}));
        res["local"] = json!(std::fs::read_dir(&t).map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.ends_with("ppet")).collect::<Vec<_>>()).unwrap_or_default());
    }
    Ok(res)
}

fn has_target(a: &Value) -> bool {
    js(a, "film").is_some() || js(a, "reel").is_some()
}

fn t_get(c: &Ctx, a: &Value) -> Result<Value> {
    let name = clean(js(a, "name").unwrap_or(""))?;
    // with a film (or reel) given, its own copy wins: that is what the film draws
    let local = if has_target(a) { file_in(&target(c, a)?, &name).map(|f| (f, "film")) } else { None };
    let Some((f, w)) = local.or_else(|| find(c, &name)) else { bail!("no puppet named {name}; puppet_list shows them") };
    let v = read_json(&f)?;
    if jsd(a, "as", "json") == "js" {
        // the same puppet as a plain ES module, for using it outside Linnea
        let mut meta = v.clone();
        let draw = meta.as_object_mut().and_then(|o| o.remove("draw")).and_then(|d| d.as_str().map(String::from)).unwrap_or_else(|| "function () {}".into());
        let js = format!("// {name}: a Linnea puppet. draw(g, p, t, s, u) paints it in a {} box.\nexport const meta = {};\nexport const draw = {draw};\nexport default {{ ...meta, draw }};\n", v["size"], serde_json::to_string_pretty(&meta)?);
        return Ok(json!({"name": name, "file": fwd(&f), "where": w, "js": js}));
    }
    Ok(json!({"name": name, "file": fwd(&f), "where": w, "puppet": v}))
}

fn t_save(c: &Ctx, a: &Value) -> Result<Value> {
    let name = clean(js(a, "name").unwrap_or(""))?;
    // start from the film's local copy (promoting it to the library), then the library one, then nothing
    let empty = |v: &Value| v.as_object().map(|o| o.is_empty()).unwrap_or(true);
    let mut v = json!({});
    if has_target(a) {
        if let Some(f) = file_in(&target(c, a)?, &name) {
            v = read_json_or(&f, json!({}));
        }
    }
    if empty(&v) {
        if let Some((f, _)) = find(c, &name) {
            v = read_json_or(&f, json!({}));
        }
    }
    // from=<puppet>: a new puppet that starts as a copy of another (a variant of a sample, say)
    if let Some(src) = js(a, "from") {
        if !empty(&v) {
            bail!("a puppet named {name} already exists; pick another name for the copy");
        }
        let src = clean(src)?;
        let Some((f, _)) = find(c, &src) else { bail!("no puppet named {src} to copy from") };
        v = read_json(&f)?;
        v["version"] = json!(0);
        v["label"] = json!(name);
    }
    for k in ["label", "desc", "tags", "size", "pivot", "params", "states", "initial", "assets", "draw", "example"] {
        if let Some(x) = a.get(k).filter(|x| !x.is_null()) {
            v[k] = x.clone();
        }
    }
    let draw = jsd(&v, "draw", "").trim().to_string();
    if !(draw.starts_with("function") || draw.starts_with("async function") || draw.contains("=>")) {
        bail!("draw must be JS function source: \"function (g, p, t, s, u) {{ ... }}\"");
    }
    if v["size"].is_null() {
        v["size"] = json!([400, 400]);
    }
    let size_ok = v["size"].as_array().map(|s| s.len() == 2 && s.iter().all(|x| x.as_f64().map(|n| (1.0..=8192.0).contains(&n)).unwrap_or(false))).unwrap_or(false);
    if !size_ok {
        bail!("size must be [width, height] in px, each 1..8192");
    }
    if !(v["params"].is_null() || v["params"].is_object()) {
        bail!("params must be an object: {{key: {{type: color|number|text|bool|select, default, ...}}}}");
    }
    match &v["states"] {
        Value::Null => {}
        Value::Object(st) => {
            for (k, s) in st {
                if !s.is_object() || jfd(s, "dur", 1.0) <= 0.0 {
                    bail!("state '{k}' must be an object like {{dur: seconds above 0, loop: true|false, label}}");
                }
            }
            if let Some(i) = js(&v, "initial").filter(|i| !st.is_empty() && !st.contains_key(*i)) {
                bail!("initial '{i}' is not one of the states ({})", st.keys().cloned().collect::<Vec<_>>().join(", "));
            }
        }
        _ => bail!("states must be an object: {{name: {{dur, loop, label}}}}"),
    }
    let rich = v["states"].as_object().map(|o| !o.is_empty()).unwrap_or(false);
    let ext = if rich { "rppet" } else { "ppet" };
    v["linnea"] = json!(format!("{ext}/1"));
    v["version"] = json!(v["version"].as_u64().unwrap_or(0) + 1);
    v["updated"] = json!(now_ms());
    if v["label"].is_null() {
        v["label"] = json!(name);
    }
    let d = user_dir(c);
    std::fs::create_dir_all(&d)?;
    let f = d.join(format!("{name}.{ext}"));
    // a puppet that became rich (or plain) keeps one file
    let other = d.join(format!("{name}.{}", if rich { "ppet" } else { "rppet" }));
    if other.is_file() {
        let _ = rename_retry(&other, &other.with_extension(format!("{}.old", other.extension().unwrap_or_default().to_string_lossy())));
    }
    write_json(&f, &v)?;
    let mut res = json!({"saved": fwd(&f), "kind": ext, "version": v["version"]});
    if has_target(a) {
        // the film's copy is what was just saved (or is replaced by it on purpose): never "changed in this film"
        res["linked"] = link(c, a, &name, true)?;
    }
    Ok(res)
}

fn link(c: &Ctx, a: &Value, name: &str, force: bool) -> Result<Value> {
    let Some((src, w)) = find(c, name) else { bail!("no puppet named {name}") };
    let t = target(c, a)?;
    std::fs::create_dir_all(&t)?;
    let bytes = std::fs::read(&src)?;
    let dst = t.join(src.file_name().unwrap());
    let lp = t.join("links.json");
    let mut links = read_json_or(&lp, json!({}));
    if dst.is_file() && !force {
        let local = std::fs::read(&dst)?;
        let known = jsd(&links[name], "hash", "").to_string();
        // a copy nobody linked (written by hand into puppets/) counts as changed unless it is the library's
        let changed = if known.is_empty() { local != bytes } else { local != bytes && !same(&known, &local) };
        if changed {
            bail!("{name} was changed in this film; pass force=true to replace it with the library version, or puppet_save film=… name={name} to keep the film's version in the library");
        }
    }
    for e in EXTS {
        let o = t.join(format!("{name}.{e}"));
        if o != dst && o.is_file() {
            let _ = rename_retry(&o, &o.with_extension(format!("{}.old", o.extension().unwrap_or_default().to_string_lossy())));
        }
    }
    std::fs::write(&dst, &bytes)?;
    let v: Value = serde_json::from_slice(&bytes).unwrap_or(json!({}));
    links[name] = json!({"from": fwd(&src), "where": w, "version": v["version"], "hash": hash(&bytes), "linked": now_ms()});
    write_json(&lp, &links)?;
    // the hook reads the kind from tweaks.json instead of probing for both files
    let tp = t.parent().unwrap().join("tweaks.json");
    let mut tw = read_json_or(&tp, json!({}));
    tw["puppets"][name] = json!(dst.extension().unwrap_or_default().to_string_lossy());
    write_json(&tp, &tw)?;
    let size = v["size"].clone();
    let st = v["states"].as_object().and_then(|o| o.keys().next().cloned());
    Ok(json!({
        "file": fwd(&dst), "version": v["version"],
        "code": format!("KE.puppet(g, '{name}-1', t, {{ use: '{name}', x: W / 2, y: H / 2, scale: 1, t0: 0, t1: DUR{} }});", st.map(|s| format!(", states: [{{ state: '{s}', t0: 0 }}]")).unwrap_or_default()),
        "layer": {"kind": "puppet", "puppet": name, "w": size[0], "h": size[1]},
    }))
}

fn t_link(c: &Ctx, a: &Value) -> Result<Value> {
    let name = clean(js(a, "name").unwrap_or(""))?;
    link(c, a, &name, jb(a, "force") == Some(true))
}

fn t_sync(c: &Ctx, a: &Value) -> Result<Value> {
    let t = target(c, a)?;
    let links = read_json_or(&t.join("links.json"), json!({}));
    let check = jb(a, "check") == Some(true);
    let mut res = Map::new();
    for (name, l) in links.as_object().cloned().unwrap_or_default() {
        let Some((src, _)) = find(c, &name) else {
            res.insert(name, json!("gone from the library; the film keeps its copy"));
            continue;
        };
        let lib = std::fs::read(&src)?;
        let Some(local) = file_in(&t, &name).and_then(|f| std::fs::read(f).ok()) else {
            // the film lost its copy (deleted by hand): the library one comes back
            if check {
                res.insert(name, json!("missing in this film"));
            } else {
                link(c, a, &name, true)?;
                res.insert(name, json!("restored from the library"));
            }
            continue;
        };
        let known = jsd(&l, "hash", "");
        let (lib_ok, local_ok) = (same(known, &lib), same(known, &local));
        let state = if local == lib || (lib_ok && local_ok) {
            "current"
        } else if !local_ok && !lib_ok {
            "both changed: puppet_link force=true takes the library one, puppet_save keeps the film's"
        } else if !local_ok {
            "changed in this film"
        } else if check {
            "library has a newer version"
        } else {
            link(c, a, &name, false)?;
            "updated from the library"
        };
        res.insert(name, json!(state));
    }
    Ok(json!({"puppets": res}))
}

// ---------- trying a puppet: every state drawn at four moments, in a real browser ----------

const TRY_PAGE: &str = r#"<!doctype html><html><head><meta charset="utf-8"></head><body style="margin:0;background:#fff">
<canvas id="c" width="__W__" height="__H__"></canvas><script>
const DEF = __DEF__;
const EASE = { linear: k => k, out: k => 1 - Math.pow(1 - k, 3), in: k => k * k * k, inOut: k => k < .5 ? 4 * k * k * k : 1 - Math.pow(-2 * k + 2, 3) / 2, back: k => { const c = 1.7; return 1 + (c + 1) * Math.pow(k - 1, 3) + c * Math.pow(k - 1, 2); } };
const clamp = (x, a, b) => Math.max(a, Math.min(b, x)), rand = n => { n = Math.sin(n * 127.1 + 311.7) * 43758.5453; return n - Math.floor(n); };
const ERRS = []; window.ERRS = ERRS;
const err = m => { if (!ERRS.includes(m) && ERRS.length < 20) ERRS.push(m); };
window.renderOut = function () {};
const c = document.getElementById('c'), g = c.getContext('2d');
let fn = null;
try { fn = new Function('return (' + DEF.draw + ')')(); if (typeof fn !== 'function') { err('draw is not a function'); fn = null; } } catch (e) { err('draw does not compile: ' + e.message); }
const img = {}, loads = [];
for (const [k, s] of Object.entries(DEF.assets || {})) { if (typeof s !== 'string') continue; const i = new Image(); loads.push(new Promise(r => { i.onload = r; i.onerror = () => { err('picture ' + k + ' does not load'); r(); }; })); i.src = s; img[k] = i; }
const [PW, PH] = DEF.size || [400, 400];
const S = DEF.states || {}, names = Object.keys(S).slice(0, 6), rows = names.length ? names : [''];
const COLS = 4, CW = 300, CH = 300;
const params = () => { const p = {}; for (const [k, v] of Object.entries(DEF.params || {})) p[k] = v && typeof v === 'object' && 'default' in v ? v.default : v; return p; };
function draw() {
  g.fillStyle = '#fff'; g.fillRect(0, 0, c.width, c.height);
  rows.forEach((n, r) => {
    const info = S[n] || { dur: 2, loop: true }, dur = info.dur || 1;
    for (let i = 0; i < COLS; i++) {
      const st = dur * i / (COLS - 1) * (info.loop ? 0.999 : 1), k = info.loop ? (st / dur) % 1 : clamp(st / dur, 0, 1);
      const x0 = i * CW, y0 = r * CH;
      for (let yy = 0; yy < CH; yy += 20) for (let xx = 0; xx < CW; xx += 20) { g.fillStyle = ((xx + yy) / 20) % 2 ? '#ececec' : '#fafafa'; g.fillRect(x0 + xx, y0 + yy, 20, 20); }
      g.strokeStyle = '#bbb'; g.lineWidth = 1; g.strokeRect(x0 + .5, y0 + .5, CW - 1, CH - 1);
      const sc = Math.min((CW - 24) / PW, (CH - 44) / PH);
      g.save(); g.translate(x0 + (CW - PW * sc) / 2, y0 + 32 + (CH - 38 - PH * sc) / 2); g.scale(sc, sc);
      g.save(); g.strokeStyle = 'rgba(0,120,255,.45)'; g.lineWidth = 1 / sc; g.setLineDash([6 / sc, 4 / sc]); g.strokeRect(0, 0, PW, PH); g.restore();
      g.save();
      try { if (fn) fn(g, params(), st, { name: n, t: st, k, dur }, { ease: EASE, clamp, rand, img, W: PW, H: PH }); }
      catch (e) { err((n || 'draw') + ' at ' + st.toFixed(2) + ' s: ' + e.message); }
      g.restore(); g.restore();
      g.setTransform(1, 0, 0, 1, 0, 0); g.fillStyle = '#222'; g.font = '600 15px sans-serif'; g.textBaseline = 'top';
      g.fillText((n || '(no states)') + '   t=' + st.toFixed(2) + ' s', x0 + 8, y0 + 8);
    }
  });
}
Promise.all(loads).then(() => { draw(); window.READY = true; });
</script></body></html>"#;

fn t_try(c: &Ctx, a: &Value) -> Result<Value> {
    // start from a library puppet when one is named (its pictures come along), then the fields given
    let mut v = match js(a, "name").map(clean).transpose()? {
        Some(n) => find(c, &n).map(|(f, _)| read_json_or(&f, json!({}))).unwrap_or_else(|| json!({})),
        None => json!({}),
    };
    for k in ["size", "params", "states", "initial", "assets", "draw"] {
        if let Some(x) = a.get(k).filter(|x| !x.is_null()) {
            v[k] = x.clone();
        }
    }
    if jsd(&v, "draw", "").trim().is_empty() {
        bail!("`draw` is the puppet's function source");
    }
    let rows = v["states"].as_object().map(|o| o.len().clamp(1, 6)).unwrap_or(1);
    let (w, h) = (1200u32, 300 * rows as u32);
    let dir = crate::config::data_dir().join("tmp").join("puppet-try");
    std::fs::create_dir_all(&dir)?;
    let id = new_id("t");
    let page = dir.join(format!("{id}.html"));
    let html = TRY_PAGE.replace("__W__", &w.to_string()).replace("__H__", &h.to_string()).replace("__DEF__", &serde_json::to_string(&v)?.replace("</", "<\\/"));
    std::fs::write(&page, html)?;
    let sheet = dir.join(format!("{id}.jpg"));
    let res = (|| -> Result<Value> {
        let mut p = crate::cdp::Page::open(&c.cfg, &format!("file:///{}", fwd(&page).trim_start_matches('/')), w, h, false)?;
        p.wait_ready(std::time::Duration::from_secs(20))?;
        let errs = p.eval("window.ERRS").unwrap_or(json!([]));
        std::fs::write(&sheet, p.frame(0.0, 85, 1.0)?)?;
        let mut errors = errs.as_array().cloned().unwrap_or_default();
        errors.extend(p.errors.iter().map(|e| json!(e)));
        Ok(json!(errors))
    })();
    let _ = std::fs::remove_file(&page);
    // keep the newest few sheets only
    if let Ok(rd) = std::fs::read_dir(&dir) {
        let mut all: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        all.sort();
        let n = all.len().saturating_sub(30);
        for old in all.into_iter().take(n) {
            let _ = std::fs::remove_file(old);
        }
    }
    let errors = res?;
    Ok(json!({"sheet": fwd(&sheet), "errors": errors, "rows": v["states"].as_object().map(|o| o.keys().take(6).cloned().collect::<Vec<_>>()).unwrap_or_default(),
              "note": "rows are the states, columns four moments through each state; the dashed blue box is the puppet's box (size). Look at the picture, fix what is wrong, try again."}))
}

const MAKER: &str = "You make puppets for Linnea's puppet library: reusable drawings painted by a JavaScript function on a canvas, used in films written as code.\n\
A puppet: size [w, h] (its box in px; draw inside 0..w, 0..h with a small margin), params {key: {type: color|number|text|bool|select, default, min, max, step, options, label}} for what a person would change (colours, text, lengths, on/off), states {name: {dur (seconds), loop (bool), label}} for its motions (optional; with states it is a rich puppet), initial (a state name), and draw: the source of `function (g, p, t, s, u) { ... }`.\n\
draw: g is a CanvasRenderingContext2D already placed at the box's top-left. p holds the params' values. t is seconds since the puppet appeared. s = {name, t (seconds into the state), k (0..1 through it; loops wrap), dur}. u = {ease: {linear, in, out, inOut, back}, clamp(x, a, b), rand(n) (seeded, 0..1), img (pictures from assets), W, H (the box size)}.\n\
Rules: a pure function of t and s: no Date.now, no Math.random, nothing kept between calls. Canvas 2D only: no DOM, no fetch, no timers. save()/restore() around everything you change. Use u.W and u.H rather than fixed numbers. Draw with clear silhouettes, a consistent flat style and good contrast on both light and dark backgrounds. Moving parts (head, arms, lids, steam, wheels) go in their own save/translate/rotate/restore block around their joint, so states can turn them; ease motions with u.ease. Param labels in the person's language.\n\
A character that talks or blinks: give it params mouth {type: number, default: 0, min: 0, max: 1, label} and blink {type: number, default: 0, min: 0, max: 1, label}. Linnea drives them from the film's voice and a natural blink rhythm, so open the mouth by p.mouth and close the eyes by p.blink.\n\
Work: write it, call puppet_try (it draws every state at four moments and returns the picture and any errors), look at the picture critically (proportions, cut-off parts, motion between columns, errors), fix and try again until it is good, usually two to four tries; then puppet_save with a short lowercase name (letters, digits, -), a label in the person's language and a one-line desc in English. When changing an existing puppet, pass its name to puppet_try so its pictures come along, and save it under the same name without `assets`.\n\
Answer in the person's language, briefly: what you made, its states and its params.";

const MAKER_TOOLS: &[&str] = &["puppet_try", "puppet_save", "puppet_list", "puppet_get"];

fn t_ask(c: &Ctx, a: &Value) -> Result<Value> {
    let prompt = js(a, "prompt").filter(|s| !s.trim().is_empty()).ok_or_else(|| anyhow!("`prompt` is the puppet to make"))?;
    if crate::llm::provider(&c.cfg) == "none" {
        bail!("no language model is set up. Settings > Assistant: pick Claude, OpenAI, or a local server.");
    }
    let edit = js(a, "name").map(clean).transpose()?;
    let names: Vec<String> = ja(&t_list(c, &json!({}))?, "puppets").iter().map(|p| jsd(p, "name", "").to_string()).collect();
    let mut user = format!("Puppets already in the library (their names are taken): {}\n", names.join(", "));
    if let Some(n) = &edit {
        let (f, _) = find(c, n).ok_or_else(|| anyhow!("no puppet named {n}"))?;
        let mut v = read_json(&f)?;
        if let Some(o) = v.as_object_mut() {
            if o.contains_key("assets") {
                o.insert("assets".into(), json!("(pictures; they stay as they are)"));
            }
        }
        user.push_str(&format!("Change the existing puppet `{n}` and save it under the same name. It is now:\n{}\n", serde_json::to_string_pretty(&v)?));
    }
    user.push_str(&format!("\nThe person uses the app in {}: param labels and your answer in it.\nRequest: {prompt}", if c.cfg.str("ui.lang") == "tr" { "Turkish" } else { "English" }));
    let specs: Vec<crate::llm::ToolSpec> = MAKER_TOOLS.iter().filter_map(|n| crate::tools::all().iter().find(|t| t.name == *n)).map(|t| crate::llm::ToolSpec { name: t.name.into(), description: t.desc.into(), schema: t.schema() }).collect();
    let mut steps = 0;
    let mut tries = 0;
    let mut saved: Option<String> = None;
    let cfg = c.cfg.clone();
    let rep = c.rep.clone();
    let mut exec = |name: &str, args: &Value| -> crate::llm::ToolOut {
        steps += 1;
        if rep.cancelled() {
            return "Error: cancelled by the user".to_string().into();
        }
        if !MAKER_TOOLS.contains(&name) {
            return format!("Error: no tool '{name}' here").into();
        }
        let mut args = if args.is_object() { args.clone() } else { json!({}) };
        // the maker works in the library only, and never over a puppet it was not asked to change
        if let Some(o) = args.as_object_mut() {
            for k in ["film", "reel", "project", "from"] {
                o.remove(k);
            }
            if o.get("assets").map(|x| !x.is_object()).unwrap_or(false) {
                o.remove("assets");
            }
        }
        if name == "puppet_save" {
            match clean(jsd(&args, "name", "")) {
                Ok(n) if Some(&n) != edit.as_ref() && find(c, &n).is_some() => return format!("Error: a puppet named {n} already exists; choose another name").into(),
                Err(e) => return format!("Error: {e}").into(),
                _ => {}
            }
        }
        rep.p((steps as f64 / 20.0).min(0.95), if name == "puppet_try" { "trying" } else { name });
        match crate::tools::call_with(&cfg, "assistant", name, &args, false, rep.clone()) {
            Ok(v) => {
                let mut images = vec![];
                if name == "puppet_try" {
                    tries += 1;
                    if let Some(s) = js(&v, "sheet") {
                        if let Ok(b) = std::fs::read(s) {
                            images.push(b);
                        }
                        // the app shows the newest try while the maker works
                        rep.p((steps as f64 / 20.0 + 0.001).min(0.95), &format!("try {tries}|{s}"));
                    }
                }
                if name == "puppet_save" {
                    saved = Some(clean(jsd(&args, "name", "")).unwrap_or_default());
                }
                crate::llm::ToolOut { text: serde_json::to_string(&v).unwrap_or_default(), images }
            }
            Err(e) => format!("Error: {e}").into(),
        }
    };
    let answer = crate::llm::agent(&c.cfg, MAKER, &user, &specs, &mut exec, 20)?;
    Ok(json!({"answer": answer, "saved": saved, "tries": tries, "steps": steps}))
}

fn t_cutout(c: &Ctx, a: &Value) -> Result<Value> {
    use base64::Engine;
    let data = js(a, "data").ok_or_else(|| anyhow!("`data` is the picture as a data URL"))?;
    let (head, b64) = data.split_once("base64,").ok_or_else(|| anyhow!("`data` must be a base64 data URL"))?;
    let ext = if head.contains("png") { "png" } else if head.contains("webp") { "webp" } else { "jpg" };
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64.trim())?;
    let dir = crate::config::data_dir().join("tmp");
    std::fs::create_dir_all(&dir)?;
    let id = new_id("cut");
    let src = dir.join(format!("{id}.{ext}"));
    let out = dir.join(format!("{id}.out.png"));
    std::fs::write(&src, bytes)?;
    let r = crate::matte::cutout(&c.cfg, &src, &out);
    let _ = std::fs::remove_file(&src);
    let model = r?;
    let png = std::fs::read(&out)?;
    let _ = std::fs::remove_file(&out);
    Ok(json!({"data": format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png)), "model": model}))
}

const COMMON: (&str, &str, &str) = ("film", "string", "Film id (or project + reel for a footage reel's layer page)");

pub const TOOLS: &[Tool] = &[
    Tool {
        name: "puppet_try",
        desc: "Draw a puppet before saving it: every state (up to six) at four moments through it, in a real browser, as one picture, with any errors its draw function throws. Give the same fields as puppet_save (draw, size, params, states, assets), or name= a library puppet to start from it. Open the returned sheet to look at it.",
        params: &[("draw", "string", "JS function source"), ("size", "array", "[w, h]"), ("params", "object", "Params"), ("states", "object", "States"), ("initial", "string", "Initial state"), ("assets", "object", "Pictures"), ("name", "string", "Start from this library puppet")],
        long: false,
        app_only: false,
        run: t_try,
    },
    Tool {
        name: "puppet_ask",
        desc: "Ask the configured assistant to make a new puppet from words (or, with name, to change one): it writes the drawing, tries it in the browser, looks at it, fixes it and saves it to the library.",
        params: &[("prompt!", "string", "The puppet to make, in plain words"), ("name", "string", "Change this existing puppet instead")],
        long: true,
        app_only: true,
        run: t_ask,
    },
    Tool {
        name: "puppet_cutout",
        desc: "Cut a picture out of its background with the local matte model: returns it as a PNG data URL with transparency, for a picture puppet.",
        params: &[("data!", "string", "The picture as a data URL")],
        long: false,
        app_only: true,
        run: t_cutout,
    },
    Tool {
        name: "puppet_list",
        desc: "The puppet library: reusable drawings (.ppet) and animated ones with named states (.rppet) you or the user made before, with their parameters and states. With film (or project+reel) it also says which are linked into that film. Look here before drawing something from scratch.",
        params: &[("query", "string", "Filter by words in name, label, tags or description"), COMMON, ("project", "string", "Project"), ("reel", "string", "Reel id")],
        long: false,
        app_only: false,
        run: t_list,
    },
    Tool {
        name: "puppet_get",
        desc: "One puppet in full: its draw function source, size, params, states and assets. With film (or project+reel) it returns that film's own copy when it has one (where = film), else the library's. as=js returns it as a plain ES module to reuse outside Linnea.",
        params: &[("name!", "string", "Puppet name"), ("as", "string", "json (default) or js"), COMMON, ("project", "string", "Project"), ("reel", "string", "Reel id")],
        long: false,
        app_only: false,
        run: t_get,
    },
    Tool {
        name: "puppet_save",
        desc: "Save a drawing into the puppet library so it can be reused in any film or reel. draw = JS source of `function (g, p, t, s, u)` painting the puppet in its own box 0..size (pure of p and t; u = {ease, clamp, rand, img}). params = {key: {type: color|number|text|bool|select, default, min, max, step, options: [..] (select), label}}. states (makes it a rich puppet, .rppet) = {name: {dur, loop, label}}; draw reads s.name, s.t (seconds into the state), s.k (0..1). assets = {key: data URL or file next to it} for pictures, read as u.img[key]. pivot = [x, y] in the box that KE.puppet's x, y place (default the centre). With film given and only name, promotes the film's own puppets/<name> file to the library and re-links it. from=<other puppet> starts a new puppet as a copy. Each save bumps the version.",
        params: &[("name!", "string", "Puppet name (file name)"), ("from", "string", "Start as a copy of this puppet (name must be new)"), ("label", "string", "Human name"), ("desc", "string", "What it is and how to use it"), ("tags", "array", "Words to find it by"), ("size", "array", "[w, h] native box in px"), ("pivot", "array", "[x, y] in the box; default centre"), ("params", "object", "Changeable parameters"), ("states", "object", "Animation states (rich puppet)"), ("initial", "string", "State when none is set"), ("assets", "object", "Pictures"), ("draw", "string", "JS function source"), ("example", "string", "A usage line"), COMMON, ("project", "string", "Project"), ("reel", "string", "Reel id")],
        long: false,
        app_only: false,
        run: t_save,
    },
    Tool {
        name: "puppet_link",
        desc: "Link a library puppet into a film (or a reel's layer page): copies the file into its puppets/ folder and records the link so library updates can flow in. Returns the KE.puppet(...) line for film.js and the layer entry for items2d. Refuses to overwrite a copy the film changed unless force=true.",
        params: &[("name!", "string", "Puppet name"), COMMON, ("project", "string", "Project"), ("reel", "string", "Reel id"), ("force", "boolean", "Replace a locally changed copy")],
        long: false,
        app_only: false,
        run: t_link,
    },
    Tool {
        name: "puppet_sync",
        desc: "Bring library updates of linked puppets into a film; copies changed in the film are kept and reported, a copy the film lost is restored. check=true only reports.",
        params: &[COMMON, ("project", "string", "Project"), ("reel", "string", "Reel id"), ("check", "boolean", "Report only")],
        long: false,
        app_only: false,
        run: t_sync,
    },
];
