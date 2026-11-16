//! The tool registry: one set of tools behind the app, the terminal (`linnea <tool> ...`) and the MCP
//! server. The person and the agent drive the editor with the same commands.
use crate::analyze::{self, AutocutOpts, ClipOpts};
use crate::config::*;
use crate::proc::Rep;
use crate::project::{self, Project};
use crate::render::{self, Mode};
use crate::timeline::Timeline;
use crate::util::*;
use crate::{fonts, jobs, llm, media, sfx, styles};
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};

pub struct Ctx {
    pub cfg: Config,
    pub origin: String,
    pub rep: Rep,
}

pub struct Tool {
    pub name: &'static str,
    pub desc: &'static str,
    /// (name, JSON type, description); a name ending in `!` is required.
    pub params: &'static [(&'static str, &'static str, &'static str)],
    pub long: bool,
    /// Only for the app (dialogs, the assistant itself).
    pub app_only: bool,
    pub run: fn(&Ctx, &Value) -> Result<Value>,
}

impl Tool {
    pub fn schema(&self) -> Value {
        let mut props = serde_json::Map::new();
        let mut req = vec![];
        for (n, t, d) in self.params {
            let name = n.trim_end_matches('!');
            if n.ends_with('!') {
                req.push(name);
            }
            props.insert(name.into(), if *t == "any" { json!({"description": d}) } else { json!({"type": t, "description": d}) });
        }
        json!({"type": "object", "properties": props, "required": req})
    }
}

const P: (&str, &str, &str) = ("project!", "string", "Project id, title or folder (see `projects`)");
const R: (&str, &str, &str) = ("reel", "string", "Reel id inside the project; default is the project's active reel (`main` or a clip reel)");

fn proj(c: &Ctx, a: &Value) -> Result<Project> {
    Project::open(&c.cfg, jsd(a, "project", ""))
}
fn r2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

fn doctor(c: &Ctx, _a: &Value) -> Result<Value> {
    let mut tools = vec![];
    let fixes: &[(&str, &str)] = &[
        ("ffmpeg", "winget install -e --id Gyan.FFmpeg   (or unpack a build into the tools folder)"),
        ("ffprobe", "comes with ffmpeg"),
        ("yt-dlp", "winget install -e --id yt-dlp.yt-dlp   (only needed for links)"),
        ("whisper", "whisper.cpp release (whisper-cli.exe) from github.com/ggml-org/whisper.cpp/releases into the tools folder; or use an API backend"),
    ];
    for (t, fix) in fixes {
        let p = find_tool(&c.cfg, t);
        tools.push(json!({"tool": t, "ok": p.is_some(), "path": p.as_ref().map(|p| fwd(p)), "fix": if p.is_some() { "" } else { fix }}));
    }
    let wm = find_whisper_model(&c.cfg, None);
    let fm = find_face_model();
    let fonts: Vec<Value> = fonts::report().into_iter().map(|(f, ok, stand)| json!({"family": f, "installed": ok, "stand_in": if ok { String::new() } else { stand }})).collect();
    Ok(json!({
        "version": crate::VERSION, "tools": tools,
        "tools_folder": fwd(&data_dir().join("tools")), "models_folder": fwd(&data_dir().join("models")),
        "whisper_model": {"ok": wm.is_some(), "path": wm.as_ref().map(|p| fwd(p)), "fix": if wm.is_some() { "" } else { "put a ggml model (for example ggml-large-v3-turbo.bin from huggingface.co/ggerganov/whisper.cpp) in the models folder" }},
        "face_model": {"ok": fm.is_some(), "fix": if fm.is_some() { "" } else { "put seeta_fd_frontal_v1.0.bin (github.com/atomashpolskiy/rustface, model/) in the models folder; without it wide videos are framed on the centre" }},
        "encoder": media::h264_encoder(&c.cfg), "fonts": fonts, "fonts_folder": fwd(&fonts::fonts_dir()),
        "library": fwd(&library_dir()), "workspace": fwd(&c.cfg.workspace()),
        "transcribe_backend": c.cfg.str("transcribe.backend"), "assistant": llm::provider(&c.cfg),
        "keys": c.cfg.masked()["keys"],
    }))
}

fn settings(c: &Ctx, a: &Value) -> Result<Value> {
    let mut cfg = Config::load();
    if let Some(set) = a.get("set").filter(|s| s.is_object()) {
        merge(&mut cfg.0, set);
        cfg.save()?;
    }
    let _ = c;
    Ok(cfg.masked())
}

fn projects(c: &Ctx, _a: &Value) -> Result<Value> {
    Ok(json!({"workspace": fwd(&c.cfg.workspace()), "projects": Project::list(&c.cfg)}))
}

fn reel_brief(p: &Project, rid: &str) -> Value {
    let reel = p.load_reel(rid).unwrap_or(json!({}));
    let tl = Timeline::from_reel(&reel, p.media().duration.max(0.1));
    json!({"id": rid, "title": jsd(&reel, "title", ""), "seconds": r2(tl.total), "takes": ja(&reel, "takes").len(), "beats": ja(&reel, "beats").len(),
           "style": jsd(&reel, "style", "clean"), "size": [jfd(&reel, "width", 1080.0), jfd(&reel, "height", 1920.0)], "clip": reel["clip"]})
}

fn project(c: &Ctx, a: &Value) -> Result<Value> {
    let mut p = proj(c, a)?;
    if let Some(set) = a.get("set").and_then(|s| s.as_object()) {
        for k in ["title", "status", "active_reel"] {
            if let Some(v) = set.get(k) {
                p.manifest[k] = v.clone();
            }
        }
        p.save_manifest()?;
    }
    let words = p.words().ok();
    let file = |rel: &str| if p.has(rel) { json!(fwd(&p.p(rel))) } else { Value::Null };
    Ok(json!({
        "id": p.id(), "title": p.title(), "dir": fwd(&p.dir), "status": p.manifest["status"], "source": p.manifest["source"], "media": p.manifest["media"],
        "active_reel": p.active_reel(), "reels": p.reel_ids().iter().map(|r| reel_brief(&p, r)).collect::<Vec<_>>(),
        "transcript": words.as_ref().map(|w| json!({"words": w.words.len(), "lang": w.lang, "backend": w.backend})),
        "files": {"proxy": file("assets/proxy.mp4"), "thumb": file("assets/thumb.jpg"), "words": file("build/words.json"), "peaks": file("build/peaks.json"), "face": file("build/face.json"), "silence": file("build/silence.json")},
        "clips": read_json_or(&p.p("clips.json"), json!([])), "notes": p.notes(), "renders": p.renders(),
    }))
}

fn import(c: &Ctx, a: &Value) -> Result<Value> {
    let src = js(a, "source").ok_or_else(|| anyhow!("`source` is a file path or a link"))?;
    let p = media::import(&c.cfg, src, js(a, "title"), &c.rep)?;
    Ok(json!({"project": p.id(), "title": p.title(), "media": p.manifest["media"], "next": "transcribe, then clips_find (long video) or autocut / retakes (a talking-head take)"}))
}

fn transcribe(c: &Ctx, a: &Value) -> Result<Value> {
    let p = proj(c, a)?;
    let w = media::transcribe(&c.cfg, &p, js(a, "backend"), js(a, "lang"), js(a, "model"), &c.rep)?;
    Ok(json!({"words": w.words.len(), "lang": w.lang, "backend": w.backend, "seconds": w.words.last().map(|x| r2(x.e)), "next": "`words` prints the sentence table"}))
}

fn words(c: &Ctx, a: &Value) -> Result<Value> {
    let p = proj(c, a)?;
    let w = p.words()?;
    let (from, to) = (jfd(a, "from", 0.0), jfd(a, "to", f64::MAX));
    if jsd(a, "format", "sentences") == "words" {
        let v: Vec<Value> = w.words.iter().filter(|x| x.s >= from && x.e <= to).map(|x| json!([r2(x.s), r2(x.e), x.w])).collect();
        return Ok(json!({"lang": w.lang, "words": v}));
    }
    let table: Vec<String> = analyze::sentences(&w.words).iter().filter(|s| s.e >= from && s.s <= to).map(|s| format!("{:>8.2} {:>8.2}  {}", s.s, s.e, s.text)).collect();
    Ok(json!({"lang": w.lang, "note": "original-recording seconds: start, end, sentence. Cut on word times (format=words), never inside a word.", "sentences": table}))
}

fn analyze_t(c: &Ctx, a: &Value) -> Result<Value> {
    let p = proj(c, a)?;
    let sil = analyze::silences(&c.cfg, &p, jfd(a, "noise", -30.0), 0.25)?;
    let mut out = json!({"pauses": sil.len(), "pause_seconds": r2(sil.iter().map(|(a, b)| b - a).sum())});
    if jsd(a, "track", "") == "person" {
        let src = p.source()?;
        out["person"] = match crate::matte::person_track(&c.cfg, &src, jfd(a, "from", 0.0), jfd(a, "to", 0.0), &p.p("build/person.json"), &c.rep) {
            Ok(v) => v,
            Err(e) => json!(e.to_string()),
        };
    }
    if jb(a, "faces") != Some(false) {
        let long = p.media().duration > 600.0 && a.get("from").is_none();
        if long {
            out["faces"] = json!("skipped for a long source; faces are tracked per clip when `clip_make` runs, or pass from/to");
        } else {
            match analyze::face_track(&c.cfg, &p, jfd(a, "from", 0.0), jfd(a, "to", 0.0), &c.rep) {
                Ok(v) => out["faces"] = v,
                Err(e) => out["faces"] = json!(e.to_string()),
            }
        }
    }
    Ok(out)
}

fn save_takes(p: &Project, rid: &str, takes: &[Value]) -> Result<Value> {
    let mut reel = p.load_reel(rid)?;
    reel["takes"] = json!(takes);
    p.save_reel(rid, &reel)?;
    let tl = Timeline::from_reel(&reel, p.media().duration);
    Ok(json!({"takes": takes.len(), "seconds": r2(tl.total)}))
}

fn autocut(c: &Ctx, a: &Value) -> Result<Value> {
    let p = proj(c, a)?;
    let rid = p.rid(js(a, "reel"));
    let reel = p.load_reel(&rid)?;
    let clip = &reel["clip"];
    let o = AutocutOpts { from: jf(a, "from").or(jf(clip, "a")).unwrap_or(0.0), to: jf(a, "to").or(jf(clip, "b")).unwrap_or(0.0), noise_db: jfd(a, "noise", -30.0), min_pause: jfd(a, "min_pause", 0.45), pad_in: jfd(a, "pad_in", 0.1), pad_out: jfd(a, "pad_out", 0.2), fillers: jb(a, "fillers") == Some(true) };
    let takes = analyze::autocut(&c.cfg, &p, &o)?;
    let kept: f64 = takes.iter().map(|t| jfd(t, "b", 0.0) - jfd(t, "a", 0.0)).sum();
    let mut out = json!({"reel": rid, "takes": takes, "kept_seconds": r2(kept)});
    if jb(a, "apply") != Some(false) {
        out["saved"] = save_takes(&p, &rid, &takes)?;
    }
    Ok(out)
}

fn retakes(c: &Ctx, a: &Value) -> Result<Value> {
    let p = proj(c, a)?;
    let rid = p.rid(js(a, "reel"));
    let w = p.words()?;
    let (takes, decisions) = analyze::retakes(&w.words, jfd(a, "from", 0.0), jfd(a, "to", 0.0));
    let dropped: Vec<&Value> = decisions.iter().filter(|d| d["keep"] == json!(false)).collect();
    let mut out = json!({"reel": rid, "takes": takes, "dropped": dropped, "kept_sentences": decisions.len() - dropped.len()});
    if jb(a, "apply") != Some(false) {
        out["saved"] = save_takes(&p, &rid, &takes)?;
    }
    Ok(out)
}

fn clips_find(c: &Ctx, a: &Value) -> Result<Value> {
    let p = proj(c, a)?;
    let w = p.words()?;
    let o = ClipOpts { count: jfd(a, "count", 6.0) as usize, min: jfd(a, "min", 20.0), max: jfd(a, "max", 60.0) };
    let sig = read_json_or(&p.p("build/signals.json"), Value::Null);
    let mut clips = analyze::find_clips(&w.words, &ClipOpts { count: if sig.is_null() { o.count } else { o.count * 3 }, min: o.min, max: o.max });
    if !sig.is_null() {
        crate::cues::rescore(&mut clips, &sig);
        clips.truncate(o.count);
    }
    let mut note = "scored locally: hook in the first line, a clean ending, pace, length".to_string();
    if jb(a, "ai") == Some(true) {
        let sents = analyze::sentences(&w.words);
        let table: String = sents.iter().enumerate().map(|(i, s)| format!("{i}\t{:.1}\t{:.1}\t{}\n", s.s, s.e, s.text)).collect();
        let sys = "You pick the moments of a long video that work as standalone short vertical clips. A clip opens on a line that makes a stranger stop (a question, a claim, a number, a turn), holds one idea, and ends on a finished sentence. Answer with a JSON array only.";
        let user = format!(
            "Pick the best {} clips of {:.0}-{:.0} seconds. Use whole sentences. Sentence table: index, start s, end s, text.\n\n{table}\nJSON array of {{\"first\": index, \"last\": index, \"title\": short title in the video's language, \"why\": one sentence, \"score\": 0-100, \"hook\": 0-100, \"flow\": 0-100, \"value\": 0-100, \"trend\": 0-100 (how well it fits what short-form audiences share now)}}.",
            o.count, o.min, o.max
        );
        match llm::complete(&c.cfg, sys, &user).map(|t| llm::extract_json(&t)) {
            Ok(Some(Value::Array(arr))) => {
                let picked: Vec<Value> = arr
                    .iter()
                    .filter_map(|x| {
                        let (i, j) = (x["first"].as_u64()? as usize, x["last"].as_u64()? as usize);
                        let (s0, s1) = (sents.get(i)?, sents.get(j)?);
                        Some(json!({"a": r2((s0.s - 0.15).max(0.0)), "b": r2(s1.e + 0.3), "dur": r2(s1.e - s0.s), "score": x["score"], "title": x["title"], "hook": s0.text,
                                    "why": [x["why"]], "breakdown": {"hook": x["hook"], "flow": x["flow"], "value": x["value"], "trend": x["trend"]}, "text": sents[i..=j].iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" "), "by": "assistant"}))
                    })
                    .collect();
                if !picked.is_empty() {
                    clips = picked;
                    note = format!("picked by the assistant ({})", llm::provider(&c.cfg));
                }
            }
            Ok(_) => note = "the assistant's answer could not be read; local scores are shown".into(),
            Err(e) => note = format!("assistant unavailable ({e}); local scores are shown"),
        }
    }
    for (i, cl) in clips.iter_mut().enumerate() {
        cl["id"] = json!(format!("c{}", i + 1));
    }
    write_json(&p.p("clips.json"), &json!(clips))?;
    Ok(json!({"note": note, "clips": clips, "next": "clip_make turns one into a reel"}))
}

fn clip_make(c: &Ctx, a: &Value) -> Result<Value> {
    let mut p = proj(c, a)?;
    let clips = read_json_or(&p.p("clips.json"), json!([]));
    let found = js(a, "clip").and_then(|id| ja(&json!({"c": clips}), "c").iter().find(|x| jsd(x, "id", "") == id).cloned());
    let (ca, cb, title) = match (&found, jf(a, "a"), jf(a, "b")) {
        (Some(cl), _, _) => (jfd(cl, "a", 0.0), jfd(cl, "b", 0.0), jsd(cl, "title", "clip").to_string()),
        (None, Some(x), Some(y)) if y > x => (x, y, "clip".to_string()),
        _ => bail!("give `clip` (an id from clips_find) or `a` and `b` in seconds"),
    };
    let title = js(a, "title").map(|s| s.to_string()).unwrap_or(title);
    let mut rid = js(a, "clip").map(|s| s.to_string()).unwrap_or_else(|| format!("r{}", (ca as i64)));
    let mut n = 2;
    while p.reel_path(&rid).exists() {
        rid = format!("{}-{n}", rid.trim_end_matches(|ch: char| ch == '-' || ch.is_ascii_digit()).to_string() + "");
        n += 1;
        if n > 50 {
            break;
        }
    }
    let mut reel = project::default_reel(&title);
    reel["clip"] = json!({"a": ca, "b": cb});
    reel["style"] = json!(jsd(a, "style", "clean"));
    reel["takes"] = json!([{"a": ca, "b": cb}]);
    p.save_reel(&rid, &reel)?;
    let mut out = json!({"reel": rid, "title": title});
    if jb(a, "autocut") != Some(false) && p.has("assets/audio16k.wav") {
        let o = AutocutOpts { from: ca, to: cb, noise_db: -30.0, min_pause: 0.5, pad_in: 0.1, pad_out: 0.2, fillers: false };
        if let Ok(takes) = analyze::autocut(&c.cfg, &p, &o) {
            out["cut"] = save_takes(&p, &rid, &takes)?;
        }
    }
    out["faces"] = match analyze::face_track(&c.cfg, &p, ca, cb, &c.rep) {
        Ok(v) => v,
        Err(e) => json!(e.to_string()),
    };
    if jb(a, "prepend_hook") == Some(true) {
        out["hook"] = crate::cues::add_hook(&p, &rid, None).unwrap_or_else(|e| json!(e.to_string()));
    }
    p.set("active_reel", json!(rid))?;
    Ok(out)
}

fn reel_get(c: &Ctx, a: &Value) -> Result<Value> {
    let p = proj(c, a)?;
    let rid = p.rid(js(a, "reel"));
    let reel = p.load_reel(&rid)?;
    let tl = Timeline::from_reel(&reel, p.media().duration);
    Ok(json!({"id": rid, "reel": reel, "seconds": r2(tl.total), "body": r2(tl.body),
              "timeline": tl.segs.iter().map(|s| json!({"a": s.a, "b": s.b, "edit": r2(s.start), "hold": s.hold, "speed": s.speed})).collect::<Vec<_>>()}))
}

fn warnings_of(c: &Ctx, p: &Project, rid: &str) -> Value {
    match render::build(&c.cfg, p, rid, Mode::Preview, &p.p("build/none.mp4")) {
        Ok(plan) => plan.report["warnings"].clone(),
        Err(e) => json!([format!("cannot build yet: {e}")]),
    }
}

fn reel_set(c: &Ctx, a: &Value) -> Result<Value> {
    let p = proj(c, a)?;
    let rid = p.rid(js(a, "reel"));
    let mut reel = p.load_reel(&rid)?;
    if let Some(rep) = a.get("replace").filter(|r| r.is_object()) {
        reel = rep.clone();
    }
    if let Some(patch) = a.get("patch").filter(|r| r.is_object()) {
        merge(&mut reel, patch);
    }
    if !reel.is_object() {
        bail!("a reel is a JSON object");
    }
    p.save_reel(&rid, &reel)?;
    let tl = Timeline::from_reel(&reel, p.media().duration);
    Ok(json!({"reel": rid, "seconds": r2(tl.total), "warnings": if jb(a, "check") == Some(false) { json!([]) } else { warnings_of(c, &p, &rid) }}))
}

fn beat_add(c: &Ctx, a: &Value) -> Result<Value> {
    let p = proj(c, a)?;
    let rid = p.rid(js(a, "reel"));
    let mut reel = p.load_reel(&rid)?;
    let beat = a.get("beat").filter(|b| b.is_object()).ok_or_else(|| anyhow!("`beat` is an object like {{\"type\": \"big\", \"at\": 12.4, \"text\": \"$20\"}}"))?;
    let mut beats = ja(&reel, "beats").to_vec();
    match a.get("index").and_then(|i| i.as_u64()) {
        Some(i) if (i as usize) < beats.len() => beats[i as usize] = beat.clone(),
        _ => beats.push(beat.clone()),
    }
    reel["beats"] = json!(beats);
    p.save_reel(&rid, &reel)?;
    Ok(json!({"reel": rid, "beats": beats.len(), "warnings": warnings_of(c, &p, &rid)}))
}

fn beat_remove(c: &Ctx, a: &Value) -> Result<Value> {
    let p = proj(c, a)?;
    let rid = p.rid(js(a, "reel"));
    let mut reel = p.load_reel(&rid)?;
    let mut beats = ja(&reel, "beats").to_vec();
    let i = a.get("index").and_then(|i| i.as_u64()).ok_or_else(|| anyhow!("`index` is the beat's position, from 0"))? as usize;
    if i >= beats.len() {
        bail!("there are {} beats", beats.len());
    }
    beats.remove(i);
    reel["beats"] = json!(beats);
    p.save_reel(&rid, &reel)?;
    Ok(json!({"reel": rid, "beats": beats.len()}))
}

fn styles_t(_c: &Ctx, _a: &Value) -> Result<Value> {
    Ok(json!({"styles": styles::list_styles(), "captions": styles::caption_styles(),
              "note": "presets and recipe packs set the look; `captions` are the templates (captions.style). A recipe's planning rules: `recipe`."}))
}

fn style_apply(c: &Ctx, a: &Value) -> Result<Value> {
    let p = proj(c, a)?;
    let rid = p.rid(js(a, "reel"));
    let mut reel = p.load_reel(&rid)?;
    if let Some(s) = js(a, "style") {
        if styles::load_preset(s).is_none() {
            bail!("no style '{s}'. `styles` lists them.");
        }
        reel["style"] = json!(s);
        // The preset decides the caption look again; the per-recording fixes stay.
        if let Some(o) = reel["captions"].as_object_mut() {
            o.retain(|k, _| matches!(k.as_str(), "fixes" | "phrases" | "keywords" | "highlight" | "tags"));
        }
        if let Some(o) = reel.as_object_mut() {
            o.remove("brand");
        }
    }
    if let Some(cs) = js(a, "captions") {
        reel["captions"]["style"] = json!(cs);
    }
    if let Some(ac) = js(a, "accent") {
        reel["brand"]["accent"] = json!(ac);
    }
    p.save_reel(&rid, &reel)?;
    Ok(json!({"reel": rid, "style": reel["style"], "captions": styles::resolve(&reel)["_cap"]["style"], "warnings": warnings_of(c, &p, &rid)}))
}

fn build(c: &Ctx, a: &Value) -> Result<Value> {
    let p = proj(c, a)?;
    let rid = p.rid(js(a, "reel"));
    let plan = render::build(&c.cfg, &p, &rid, Mode::Preview, &p.p("build/none.mp4"))?;
    let mut r = plan.report;
    let caps: Vec<String> = ja(&r, "captions").iter().map(|x| format!("{:>7.2}  {}  [{}]", jfd(x, "s", 0.0), jsd(x, "text", ""), jsd(x, "zone", ""))).collect();
    r["captions"] = json!(caps);
    Ok(r)
}

fn snapshot(c: &Ctx, a: &Value) -> Result<Value> {
    let p = proj(c, a)?;
    let rid = p.rid(js(a, "reel"));
    let t = jfd(a, "t", 0.0);
    let f = render::snapshot(&c.cfg, &p, &rid, t)?;
    Ok(json!({"file": fwd(&f), "t": t, "note": "edit seconds. Open the file to look at the frame."}))
}

fn approval(c: &Ctx, a: &Value) -> Result<Value> {
    let p = proj(c, a)?;
    let rid = p.rid(js(a, "reel"));
    render::approval(&c.cfg, &p, &rid)
}

fn render_t(c: &Ctx, a: &Value) -> Result<Value> {
    let p = proj(c, a)?;
    let rid = p.rid(js(a, "reel"));
    let mode = if jsd(a, "mode", "preview") == "final" { Mode::Final } else { Mode::Preview };
    render::render(&c.cfg, &p, &rid, mode, &c.rep)
}

fn job(_c: &Ctx, a: &Value) -> Result<Value> {
    let id = js(a, "id").ok_or_else(|| anyhow!("`id` is the job id"))?;
    if jb(a, "cancel") == Some(true) {
        jobs::cancel(id);
    }
    let j = if jb(a, "wait") == Some(true) { jobs::wait(id, jfd(a, "seconds", 540.0) as u64) } else { jobs::get(id) };
    j.ok_or_else(|| anyhow!("no job '{id}'"))
}

fn jobs_t(_c: &Ctx, _a: &Value) -> Result<Value> {
    Ok(json!({"jobs": jobs::list()}))
}

fn notes(c: &Ctx, a: &Value) -> Result<Value> {
    let p = proj(c, a)?;
    let want = jsd(a, "status", "open");
    let v: Vec<Value> = p.notes().into_iter().filter(|n| want == "all" || jsd(n, "status", "open") == want).collect();
    Ok(json!({"notes": v, "hint": "t is edit seconds in the note's reel. After fixing: note_update with status fixed and a short reply."}))
}

fn note_add(c: &Ctx, a: &Value) -> Result<Value> {
    let p = proj(c, a)?;
    let mut n = p.notes();
    let note = json!({"id": new_id("n"), "reel": p.rid(js(a, "reel")), "t": jfd(a, "t", 0.0), "text": jsd(a, "text", ""), "status": "open", "reply": "", "created": now_ms(), "by": c.origin});
    n.push(note.clone());
    p.save_notes(&n)?;
    Ok(note)
}

fn note_update(c: &Ctx, a: &Value) -> Result<Value> {
    let p = proj(c, a)?;
    let mut n = p.notes();
    let id = jsd(a, "id", "");
    if jb(a, "delete") == Some(true) {
        n.retain(|x| jsd(x, "id", "") != id);
        p.save_notes(&n)?;
        return Ok(json!({"deleted": id}));
    }
    let note = n.iter_mut().find(|x| jsd(x, "id", "") == id).ok_or_else(|| anyhow!("no note '{id}'"))?;
    for k in ["status", "reply", "text"] {
        if let Some(v) = a.get(k) {
            note[k] = v.clone();
        }
    }
    let out = note.clone();
    p.save_notes(&n)?;
    Ok(out)
}

fn taste(_c: &Ctx, a: &Value) -> Result<Value> {
    let mut t = project::taste();
    let mut rules = ja(&t, "rules").to_vec();
    if let Some(text) = js(a, "add") {
        rules.push(json!({"id": new_id("t"), "text": text, "from": jsd(a, "from", "user"), "style": a["style"], "created": now_ms()}));
    }
    if let Some(id) = js(a, "forget") {
        rules.retain(|r| jsd(r, "id", "") != id);
    }
    t["rules"] = json!(rules);
    write_json(&project::taste_path(), &t)?;
    Ok(json!({"about": "The user's corrections across reels, in their words. A prior: the reel always wins. Read before planning an edit.", "rules": rules}))
}

fn library(c: &Ctx, a: &Value) -> Result<Value> {
    let q = jsd(a, "query", "").to_lowercase();
    let mut sounds = sfx::catalog(&c.cfg);
    if !q.is_empty() {
        sounds.retain(|s| s.to_string().to_lowercase().contains(&q));
    }
    Ok(json!({"sounds": sounds, "roles": {"ui": -14, "whoosh": -12, "impact": -8, "meme": -3},
              "note": "levels are set against the voice automatically (dB under the voice's peak, by role); never hand-tune volume. Music beds are not shipped: set music.src to your own file.",
              "fonts": fonts::report().into_iter().map(|(f, ok, s)| json!({"family": f, "installed": ok, "stand_in": s})).collect::<Vec<_>>()}))
}

fn recipe(_c: &Ctx, a: &Value) -> Result<Value> {
    let name = js(a, "name").ok_or_else(|| anyhow!("`name` is a recipe id from `styles` (kind recipe)"))?;
    let pack = styles::load_preset(name).ok_or_else(|| anyhow!("no recipe '{name}'"))?;
    let dir = library_dir().join("recipes").join(jsd(&pack, "_recipe", name));
    let read = |f: &str| std::fs::read_to_string(dir.join(f)).ok().map(|s| s.chars().take(14000).collect::<String>());
    Ok(json!({"pack": pack, "system": read("SYSTEM.md"), "readme": read("README.md"), "folder": fwd(&dir),
              "note": "SYSTEM.md is the original recipe's planning document (written for its HTML renderer). Follow its invariants and slots with Linnea's native beats; `_grammar` in the pack says which parts are native."}))
}

fn open(c: &Ctx, a: &Value) -> Result<Value> {
    let p = proj(c, a)?;
    let f = json!({"project": p.id(), "reel": p.rid(js(a, "reel")), "t": a["t"], "ts": now_ms(), "by": c.origin});
    write_json(&c.cfg.state().join("focus.json"), &f)?;
    Ok(json!({"opened": p.id(), "note": "the Linnea window shows this project now, if it is running"}))
}

fn pick_file(_c: &Ctx, a: &Value) -> Result<Value> {
    let d = rfd::FileDialog::new();
    let path = if jb(a, "folder") == Some(true) {
        d.pick_folder()
    } else {
        d.add_filter("Video", &["mp4", "mov", "mkv", "webm", "m4v", "avi"]).add_filter("All files", &["*"]).pick_file()
    };
    Ok(json!({"path": path.map(|p| fwd(&p))}))
}

fn reveal(_c: &Ctx, a: &Value) -> Result<Value> {
    let path = std::path::PathBuf::from(jsd(a, "path", ""));
    if !path.exists() {
        bail!("no such path");
    }
    #[cfg(windows)]
    {
        let arg = if path.is_file() { format!("/select,{}", path.to_string_lossy().replace('/', "\\")) } else { path.to_string_lossy().replace('/', "\\") };
        let _ = std::process::Command::new("explorer.exe").arg(arg).spawn();
    }
    Ok(json!({"ok": true}))
}

const ASSISTANT: &str = "You are the editing assistant inside Linnea, a local editor that turns talking-head recordings and long videos into short vertical reels. You work by calling tools; the person watches the result in the app.\n\
Rules of the craft:\n\
- Every time in a reel is an ORIGINAL-recording second from the transcript (`words`). Cut on word times: a = first word - 0.10..0.15, b = last word + 0.20..0.30.\n\
- Transcribe first if there is no transcript. For one clean take use `autocut`; for a recording with retakes use `retakes`; for a long video use `clips_find` then `clip_make`.\n\
- Nothing on screen says anything the speaker did not say. A meme or reaction lands ON or just after the punchline, never before it.\n\
- Restraint: one card in the band at a time, 1-2 beats per 10 seconds at most, hard cuts by default, few sounds. Leave some sentences bare.\n\
- Change the edit with `reel_set` (a JSON merge patch), `style_apply`, `beat_add`. Read the warnings each returns and fix them.\n\
- Look before you say it is done: `snapshot` a frame where you changed something, or `build` for the timeline, captions and warnings.\n\
- Render only when asked (`render`, mode preview first). Read `taste` before planning: it holds the person's standing corrections.\n\
Answer in the person's language, briefly: what you changed and what to look at.";

/// The tools the reel assistant may call: the footage editor's own. Not the film tools, not posting to
/// accounts, not downloads or settings: those stay with the person.
fn reel_tool(t: &Tool) -> bool {
    !t.app_only && !t.name.starts_with("hub_") && !t.name.starts_with("publish_") && !matches!(t.name, "import" | "settings" | "open" | "setup_tools" | "puppet_save" | "puppet_sync" | "puppet_get")
}

/// The tools the film assistant may call. hub_file is how it edits the film's code.
const FILM_TOOLS: &[&str] = &["hub_project", "hub_file", "hub_live", "hub_frame", "hub_notes", "hub_note_update", "hub_2d", "hub_sound", "hub_lipsync", "puppet_list", "puppet_get", "puppet_save", "puppet_link", "puppet_sync"];

const FILM_ASSISTANT: &str = "You are the film assistant inside Linnea, a local studio where films are written as code. The person watches the film play live from its code in the app and asks you for changes; you make them by editing the film's files with tools. The preview reloads by itself when a file changes.\n\
The contract:\n\
- index.html holds a fixed-size canvas (#c). film.js defines window.renderOut(t), which draws second t of the film as a PURE function of t: no Date.now(), no state carried between frames, no unseeded Math.random() (use the seeded rand helper). It may return a promise. window.READY = true once assets are loaded. window.VH_SCENES = [{name, t0, t1}] labels the timeline. Sound is audio/mix.wav.\n\
- Every on-screen element is drawn through el(id, t, def, draw) (KE.group), KE.text or KE.puppet with a stable id, so the person can move, resize, re-type and re-time it by hand. Their hand edits live in tweaks.json `edits`, keyed by those ids: keep the ids of existing elements, read p.text / p.color inside draw, and never write tweaks.json (hub_2d reads and changes the hand-placed layers and the edits).\n\
- Reusable drawings (objects, characters, icons, animated props) belong in the puppet library: puppet_list first; puppet_link copies one into the film and returns the KE.puppet line; puppet_save makes a new one (params for what a person would change, states for its motions).\n\
- A puppet with `mouth` and `blink` params talks and blinks by itself: the mouth follows the film's voice once audio/mouth.json exists (hub_lipsync makes it). A state change can blend in: states: [{state, t0, blend: seconds}]. A hand-placed layer can follow a path: items2d `path: [[x, y], ...]` (centre points), `pathTurn: true`, `pathEase`.\n\
How to work:\n\
1. hub_file action=read path=film.js before changing anything (long files: from / to line numbers).\n\
2. Make the smallest change that does the job with hub_file action=replace (find = exact text that occurs once). Use action=write only for a new file or a full rewrite.\n\
3. hub_live checks the page the way the renderer sees it: fix every problem, render error and console error it reports, then call it again.\n\
4. hub_frame at the seconds you changed, and look at the picture before saying it is done. If it is not right, fix it and look again.\n\
5. If the request is about the person's notes: hub_notes lists them (look at the pictures); after fixing one, hub_note_update status=fixed with a short reply.\n\
Never render and never touch anything outside this film. If the request is unclear or cannot be done, say so instead of guessing.\n\
Answer in the person's language, briefly: what you changed and at which seconds to look.";

/// The pictures a film tool's result points at, for a model that can look: a contact sheet or a frame from
/// hub_frame, the user's drawings from hub_notes.
fn film_pictures(name: &str, v: &Value) -> Vec<Vec<u8>> {
    let read = |p: Option<&str>| p.and_then(|p| std::fs::read(p).ok()).filter(|b| b.len() < 4_000_000);
    match name {
        "hub_frame" => match read(js(v, "sheet")) {
            Some(sheet) => vec![sheet],
            None => ja(v, "frames").iter().take(2).filter_map(|f| read(js(f, "file"))).collect(),
        },
        "hub_notes" => ja(v, "notes").iter().take(3).filter_map(|n| read(js(&n["images"], "crop").or_else(|| js(&n["images"], "annotated")).or_else(|| js(&n["images"], "frame")))).collect(),
        _ => vec![],
    }
}

/// The last few requests and answers, so "a bit bigger" means something on the next turn.
fn history(path: &std::path::Path) -> String {
    let h = read_json_or(path, json!([]));
    let last: Vec<&Value> = h.as_array().map(|a| a.iter().rev().take(3).collect()).unwrap_or_default();
    if last.is_empty() {
        return String::new();
    }
    let mut s = "Earlier requests in this conversation (oldest first):\n".to_string();
    for x in last.iter().rev() {
        s.push_str(&format!("- They asked: {}\n  You answered: {}\n", jsd(x, "q", ""), jsd(x, "a", "").chars().take(600).collect::<String>()));
    }
    s
}
fn remember(path: &std::path::Path, mut entry: Value) {
    let mut h = read_json_or(path, json!([])).as_array().cloned().unwrap_or_default();
    entry["ts"] = json!(now_ms());
    h.push(entry);
    let n = h.len().saturating_sub(20);
    let _ = write_json(path, &json!(h[n..]));
}

/// What the film assistant is doing, in the person's words, for the live step list in the app.
fn step_text(name: &str, a: &Value, tr: bool) -> String {
    let path = jsd(a, "path", "film.js");
    let l = |en: String, t: String| if tr { t } else { en };
    match name {
        "hub_file" => match jsd(a, "action", "read") {
            "list" => l("Looking at the files".into(), "Dosyalara bakıyor".into()),
            "write" => l(format!("Writing {path}"), format!("{path} yazılıyor")),
            "replace" => l(format!("Editing {path}"), format!("{path} düzenleniyor")),
            _ => l(format!("Reading {path}"), format!("{path} okunuyor")),
        },
        "hub_live" => l("Checking the page".into(), "Sayfayı denetliyor".into()),
        "hub_frame" => match (jf(a, "t"), ja(a, "times").len(), jf(a, "from")) {
            (Some(t), _, _) => l(format!("Looking at {t:.1} s"), format!("{t:.1}. saniyeye bakıyor")),
            (_, n, _) if n > 0 => l(format!("Looking at {n} frames"), format!("{n} kareye bakıyor")),
            (_, _, Some(x)) => l(format!("Looking at frames from {x:.1} s"), format!("{x:.1}. saniyeden başlayan karelere bakıyor")),
            _ => l("Looking at the film's frames".into(), "Filmin karelerine bakıyor".into()),
        },
        "hub_notes" => l("Reading the notes".into(), "Notları okuyor".into()),
        "hub_note_update" => l(format!("Marking a note ({})", jsd(a, "status", "")), format!("Notu işaretliyor ({})", jsd(a, "status", ""))),
        "hub_project" => l("Looking at the film".into(), "Film bilgisine bakıyor".into()),
        "hub_2d" => l("Looking at the layers".into(), "Katmanlara bakıyor".into()),
        "hub_sound" => l("Looking at the sound".into(), "Sese bakıyor".into()),
        "hub_lipsync" => l("Making the mouth movement".into(), "Ağız hareketini üretiyor".into()),
        n if n.starts_with("puppet_") => l(format!("Puppet: {}", jsd(a, "name", jsd(a, "query", "library"))), format!("Kukla: {}", jsd(a, "name", jsd(a, "query", "kütüphane")))),
        n => n.into(),
    }
}

/// The interface language, for what the assistants write to the person.
fn person_language(cfg: &Config) -> &'static str {
    if cfg.str("ui.lang") == "tr" { "Turkish" } else { "English" }
}

fn hub_ask(c: &Ctx, a: &Value) -> Result<Value> {
    let f = crate::film::open(&c.cfg, jsd(a, "film", ""))?;
    let prompt = js(a, "prompt").ok_or_else(|| anyhow!("`prompt` is what to change"))?;
    if llm::provider(&c.cfg) == "none" {
        bail!("no language model is set up. Settings > Assistant: pick Claude, OpenAI, or a local server.");
    }
    let specs: Vec<llm::ToolSpec> = FILM_TOOLS.iter().filter_map(|n| all().iter().find(|t| t.name == *n)).map(|t| llm::ToolSpec { name: t.name.into(), description: t.desc.into(), schema: t.schema() }).collect();
    let info = crate::film::info(&c.cfg, &f);
    let open_notes: Vec<Value> = crate::film::notes(&f).iter().filter(|n| matches!(jsd(n, "status", ""), "open" | "claude" | "still")).map(|n| json!({"id": n["id"], "t": n["t"], "text": n["text"]})).collect();
    let mut user = format!(
        "Film `{}` (\"{}\"): {}x{} px, {} fps, {} s. Page {}.\nScenes: {}\nFiles: {}\nOpen notes: {}\n",
        f.id, jsd(&info, "title", ""), info["width"], info["height"], info["fps"], info["dur"], jsd(&info, "page", "index.html"), info["scenes"],
        crate::film::files(&f).iter().map(|x| jsd(x, "path", "").to_string()).collect::<Vec<_>>().join(", "), json!(open_notes)
    );
    if let Some(t) = jf(a, "t") {
        user.push_str(&format!("The person is looking at second {t:.2}.\n"));
    }
    if let Some(e) = js(a, "element").filter(|e| !e.is_empty()) {
        user.push_str(&format!("They have the element with id `{e}` selected.\n"));
    }
    let hist_path = f.dir.join(".videohub").join("assistant.json");
    user.push_str(&history(&hist_path));
    user.push_str(&format!("\nThe person uses the app in {}; answer in it.\nRequest: {prompt}", person_language(&c.cfg)));
    let mut steps = 0;
    let mut changed: Vec<String> = vec![];
    let cfg = c.cfg.clone();
    let rep = c.rep.clone();
    let id = f.id.clone();
    // the moment to go back to: every file the assistant changes is backed up after it
    let since = now_ms();
    // the live step list the app shows while it works
    let live_path = f.dir.join(".videohub").join("assistant-live.json");
    let tr_ui = c.cfg.str("ui.lang") == "tr";
    let mut log: Vec<Value> = vec![];
    let _ = write_json(&live_path, &json!({"running": true, "prompt": prompt, "started": since, "steps": []}));
    let mut exec = |name: &str, args: &Value| -> llm::ToolOut {
        steps += 1;
        rep.p((steps as f64 / 30.0).min(0.95), name);
        if rep.cancelled() {
            return "Error: cancelled by the user".to_string().into();
        }
        if !FILM_TOOLS.contains(&name) {
            return format!("Error: no tool '{name}' here. Tools: {}", FILM_TOOLS.join(", ")).into();
        }
        // every call works on this film, whatever the model wrote
        let mut args = if args.is_object() { args.clone() } else { json!({}) };
        args["film"] = json!(id);
        if name == "hub_frame" && args.get("scale").is_none() {
            args["scale"] = json!(0.5);
        }
        log.push(json!({"text": step_text(name, &args, tr_ui), "tool": name}));
        let _ = write_json(&live_path, &json!({"running": true, "prompt": prompt, "started": since, "steps": log}));
        let res = call_with(&cfg, "assistant", name, &args, false, rep.clone());
        let last = log.last_mut().unwrap();
        let out = match res {
            Ok(v) => {
                if name == "hub_file" {
                    if let Some(p) = js(&v, "written").or_else(|| js(&v, "changed")) {
                        if !changed.iter().any(|x| x == p) {
                            changed.push(p.to_string());
                        }
                    }
                }
                if name == "hub_frame" {
                    last["image"] = v["sheet"].as_str().or_else(|| v["frames"][0]["file"].as_str()).map(|s| json!(s)).unwrap_or(Value::Null);
                }
                if name == "hub_live" {
                    let problems = ja(&v["checks"], "render_errors").len() + ja(&v["checks"], "console_errors").len() + usize::from(v["checks"]["ready"] != json!(true));
                    last["note"] = json!(match (problems, tr_ui) { (0, true) => "sorun yok".to_string(), (0, false) => "no problems".to_string(), (n, true) => format!("{n} sorun"), (n, false) => format!("{n} problems") });
                }
                llm::ToolOut { text: serde_json::to_string(&v).unwrap_or_default(), images: film_pictures(name, &v) }
            }
            Err(e) => {
                last["error"] = json!(e.to_string().chars().take(160).collect::<String>());
                format!("Error: {e}").into()
            }
        };
        let _ = write_json(&live_path, &json!({"running": true, "prompt": prompt, "started": since, "steps": log}));
        out
    };
    let result = llm::agent(&c.cfg, FILM_ASSISTANT, &user, &specs, &mut exec, 30);
    let _ = write_json(&live_path, &json!({"running": false, "prompt": prompt, "started": since, "steps": log}));
    let answer = result?;
    remember(&hist_path, json!({"q": prompt, "a": answer, "since": since, "changed": changed, "steps": log.len()}));
    Ok(json!({"answer": answer, "steps": steps, "changed": changed, "since": since}))
}

fn ask(c: &Ctx, a: &Value) -> Result<Value> {
    let p = proj(c, a)?;
    let rid = p.rid(js(a, "reel"));
    let prompt = js(a, "prompt").ok_or_else(|| anyhow!("`prompt` is what to do"))?;
    let specs: Vec<llm::ToolSpec> = all().iter().filter(|t| reel_tool(t)).map(|t| llm::ToolSpec { name: t.name.into(), description: t.desc.into(), schema: t.schema() }).collect();
    let state = project(c, &json!({"project": p.id()}))?;
    let user = format!(
        "Project `{}` (\"{}\"), active reel `{rid}`.\nState: {}\nTaste: {}\n\nRequest: {prompt}",
        p.id(), p.title(), json!({"media": state["media"], "transcript": state["transcript"], "reels": state["reels"], "open_notes": p.notes().iter().filter(|n| jsd(n, "status", "") == "open").collect::<Vec<_>>()}),
        project::taste()["rules"]
    );
    let user = format!("{user}\nThe person uses the app in {}; answer in it.", person_language(&c.cfg));
    let mut steps = 0;
    let cfg = c.cfg.clone();
    let rep = c.rep.clone();
    let pid = p.id().to_string();
    let mut exec = |name: &str, args: &Value| -> llm::ToolOut {
        steps += 1;
        rep.p((steps as f64 / 24.0).min(0.95), name);
        if rep.cancelled() {
            return "Error: cancelled by the user".to_string().into();
        }
        // only the tools it was given, and only on the project it was asked about
        if !all().iter().any(|t| t.name == name && reel_tool(t)) {
            return format!("Error: no tool '{name}' here").into();
        }
        let mut args = if args.is_object() { args.clone() } else { json!({}) };
        if args.get("project").is_some() {
            args["project"] = json!(pid);
        }
        match call_with(&cfg, "assistant", name, &args, false, rep.clone()) {
            Ok(v) => serde_json::to_string(&v).unwrap_or_default().into(),
            Err(e) => format!("Error: {e}").into(),
        }
    };
    let hist_path = crate::config::state_dir(&p.dir).join("assistant.json");
    let user = format!("{user}\n{}", history(&hist_path));
    let answer = llm::agent(&c.cfg, ASSISTANT, &user, &specs, &mut exec, 24)?;
    remember(&hist_path, json!({"q": prompt, "a": answer}));
    Ok(json!({"answer": answer, "steps": steps}))
}

const T: &[Tool] = &[
        Tool { name: "doctor", desc: "Check this machine: ffmpeg, yt-dlp, whisper.cpp and its model, the face model, fonts, the encoder, API keys. Each missing part comes with its fix.", params: &[], long: false, app_only: false, run: doctor },
        Tool { name: "settings", desc: "Read the settings; `set` merges changes (workspace, transcribe.backend whisper.cpp|openai|groq|elevenlabs, transcribe.lang, llm.provider none|anthropic|openai|local, llm.base_url, llm.model, keys.*, render.encoder auto|x264, ui.theme). Keys are never returned.", params: &[("set", "object", "Settings to merge")], long: false, app_only: false, run: settings },
        Tool { name: "projects", desc: "List the projects in the workspace.", params: &[], long: false, app_only: false, run: projects },
        Tool { name: "project", desc: "One project's state: source, media, transcript, reels, clip candidates, notes, renders. `set` changes title, status or active_reel.", params: &[P, ("set", "object", "title / status / active_reel")], long: false, app_only: false, run: project },
        Tool { name: "import", desc: "Start a project from a video file on this computer or from a link (YouTube and anything yt-dlp reads). Makes the preview proxy, voice track and waveform.", params: &[("source!", "string", "File path or URL"), ("title", "string", "Project title")], long: true, app_only: false, run: import },
        Tool { name: "transcribe", desc: "Word-timed transcript of the whole source. Local by default (whisper.cpp); backend openai, groq or elevenlabs sends the audio to that service.", params: &[P, ("backend", "string", "whisper.cpp | openai | groq | elevenlabs"), ("lang", "string", "Language code, or auto"), ("model", "string", "Model name or file")], long: true, app_only: false, run: transcribe },
        Tool { name: "words", desc: "The transcript as a sentence table in original-recording seconds (the take map). format=words gives every word with its times, for exact cut points.", params: &[P, ("from", "number", "Start second"), ("to", "number", "End second"), ("format", "string", "sentences | words")], long: false, app_only: false, run: words },
        Tool { name: "analyze", desc: "Find the pauses and track the speaker's face (for 9:16 framing and to keep captions and cards off the face).", params: &[P, ("faces", "boolean", "Track the face (default true)"), ("from", "number", "Start second"), ("to", "number", "End second"), ("noise", "number", "Silence threshold in dB, default -30"), ("track", "string", "person: also track the whole person with the matte model (for crop.track=person)")], long: true, app_only: false, run: analyze_t },
        Tool { name: "autocut", desc: "Takes with the pauses removed, for one clean take or a clip of a long video. Cuts on the audio's own silences. Saves them to the reel unless apply=false.", params: &[P, R, ("from", "number", "Start second"), ("to", "number", "End second"), ("min_pause", "number", "Pauses longer than this are cut, default 0.45"), ("noise", "number", "Silence threshold dB, default -30"), ("fillers", "boolean", "Also cut um / uh / ııı"), ("apply", "boolean", "Save to the reel (default true)")], long: false, app_only: false, run: autocut },
        Tool { name: "retakes", desc: "For a recording with retakes and false starts: keep the later, fluent attempt of each sentence. Returns what was dropped and why; check it against `words`.", params: &[P, R, ("from", "number", "Start second"), ("to", "number", "End second"), ("apply", "boolean", "Save to the reel (default true)")], long: false, app_only: false, run: retakes },
        Tool { name: "clips_find", desc: "Find the moments of a long video that stand alone as short clips, scored (hook, clean ending, pace). ai=true asks the configured assistant to pick instead.", params: &[P, ("count", "number", "How many, default 6"), ("min", "number", "Shortest clip in seconds, default 20"), ("max", "number", "Longest, default 60"), ("ai", "boolean", "Let the assistant pick")], long: true, app_only: false, run: clips_find },
        Tool { name: "clip_make", desc: "Turn a clip candidate (or any a..b range) into its own reel: pauses trimmed, face tracked for 9:16 framing. It becomes the active reel.", params: &[P, ("clip", "string", "Clip id from clips_find"), ("a", "number", "Start second"), ("b", "number", "End second"), ("title", "string", "Reel title"), ("style", "string", "Style preset"), ("autocut", "boolean", "Trim pauses (default true)"), ("prepend_hook", "boolean", "Copy the strongest 2-3.5 s to the start as a hook")], long: true, app_only: false, run: clip_make },
        Tool { name: "reel", desc: "Read a reel: the whole edit as JSON (takes, crop, zoom, captions, style, beats, sfx, music, look) and its timeline.", params: &[P, R], long: false, app_only: false, run: reel_get },
        Tool { name: "reel_set", desc: "Change a reel with a JSON merge patch (objects merge, null deletes, arrays replace). Examples: {\"captions\":{\"style\":\"hype\"}}, {\"takes\":[{\"a\":6.58,\"b\":10.4}]}, {\"zoom\":{\"snaps\":[[10.76,1.1]]}}, {\"crop\":{\"mode\":\"fit\"}}, {\"width\":1080,\"height\":1080}. Returns the warnings.", params: &[P, R, ("patch", "object", "Merge patch"), ("replace", "object", "A whole new reel"), ("check", "boolean", "Build and return warnings (default true)")], long: false, app_only: false, run: reel_set },
        Tool { name: "beat_add", desc: "Add a beat (or replace the one at `index`). Types: big, chips, strike, quote, list, title, endcard, emoji, logo, meme, broll, textfx (hook, headline, spring), scene (card, word, sentence, kinetic, stats, ui, device, image). `at` and `to` are original seconds; \"outro+0\" and \"end\" also work.", params: &[P, R, ("beat!", "object", "The beat"), ("index", "number", "Replace this beat")], long: false, app_only: false, run: beat_add },
        Tool { name: "beat_remove", desc: "Remove the beat at `index`.", params: &[P, R, ("index!", "number", "Beat position, from 0")], long: false, app_only: false, run: beat_remove },
        Tool { name: "styles", desc: "The looks: seven presets (clean, editorial, meme, cinematic, launch, kinetic, pop), nineteen recipe packs, and the caption templates.", params: &[], long: false, app_only: false, run: styles_t },
        Tool { name: "style_apply", desc: "Set a reel's style preset or recipe pack, and optionally its caption template and accent colour.", params: &[P, R, ("style", "string", "Preset or recipe id"), ("captions", "string", "Caption template id"), ("accent", "string", "Hex colour")], long: false, app_only: false, run: style_apply },
        Tool { name: "recipe", desc: "A recipe's house look and its planning document (invariants, content slots), to plan beats in that style.", params: &[("name!", "string", "Recipe id")], long: false, app_only: false, run: recipe },
        Tool { name: "build", desc: "Build the reel without rendering: the edit timeline, every caption with its placement, the sound table (time, sound, role, level, why) and the warnings. Fix every warning or say why it stays.", params: &[P, R], long: false, app_only: false, run: build },
        Tool { name: "snapshot", desc: "Render one exact frame of the edit (captions, cards, framing) to a JPEG. Look at it before saying an edit is done.", params: &[P, R, ("t!", "number", "Edit time in seconds")], long: false, app_only: false, run: snapshot },
        Tool { name: "approval", desc: "The approval sheet: a frame at the start, at 1.5 s, after every cut, at every beat and at the end, plus the build report. Show it before the slow render.", params: &[P, R], long: true, app_only: false, run: approval },
        Tool { name: "render", desc: "Render the reel: mode preview (720p, fast) or final (1080x1920 H.264 master). Every render is checked (streams, length, loudness, black frames, pauses, caption safe area). Only when the user asked.", params: &[P, R, ("mode", "string", "preview | final")], long: true, app_only: false, run: render_t },
        Tool { name: "job", desc: "A job's state. wait=true waits until it ends (at most about 9 minutes; call again if it has not). cancel=true stops it.", params: &[("id!", "string", "Job id"), ("wait", "boolean", "Wait for the end"), ("cancel", "boolean", "Stop it")], long: false, app_only: false, run: job },
        Tool { name: "jobs", desc: "Recent jobs across the app, the terminal and agents.", params: &[], long: false, app_only: false, run: jobs_t },
        Tool { name: "notes", desc: "The review notes the user left on frames (time, text). Default: open ones.", params: &[P, ("status", "string", "open | fixed | all")], long: false, app_only: false, run: notes },
        Tool { name: "note_add", desc: "Leave a note on a frame of a reel.", params: &[P, R, ("t!", "number", "Edit seconds"), ("text!", "string", "What is wrong")], long: false, app_only: false, run: note_add },
        Tool { name: "note_update", desc: "After fixing a note: status fixed and a short reply the user will see.", params: &[P, ("id!", "string", "Note id"), ("status", "string", "open | fixed | wontfix"), ("reply", "string", "Short reply"), ("delete", "boolean", "Remove the note")], long: false, app_only: false, run: note_update },
        Tool { name: "taste", desc: "The user's standing corrections across reels. `add` records one in their words when they correct an edit decision; `forget` removes one.", params: &[("add", "string", "The correction, in the user's words"), ("from", "string", "user | skill"), ("forget", "string", "Rule id")], long: false, app_only: false, run: taste },
        Tool { name: "library", desc: "The sound kit (ids, roles) and which template fonts are installed.", params: &[("query", "string", "Filter")], long: false, app_only: false, run: library },
        Tool { name: "open", desc: "Show a project (and reel) in the Linnea window so the user can watch and leave notes.", params: &[P, R, ("t", "number", "Edit seconds to jump to")], long: false, app_only: false, run: open },
        Tool { name: "ask", desc: "Ask the configured assistant to edit this reel in plain words.", params: &[P, R, ("prompt!", "string", "What to do")], long: true, app_only: true, run: ask },
        Tool { name: "hub_ask", desc: "Ask the configured assistant to change a film in plain words: it reads and edits the film's code, checks the page and looks at its own frames.", params: &[("film!", "string", "Film id"), ("prompt!", "string", "What to change"), ("t", "number", "The second on screen"), ("element", "string", "Id of the selected element")], long: true, app_only: true, run: hub_ask },
        Tool { name: "pick_file", desc: "Open the system file dialog.", params: &[("folder", "boolean", "Pick a folder")], long: false, app_only: true, run: pick_file },
        Tool { name: "reveal", desc: "Show a file in the file manager.", params: &[("path!", "string", "Path")], long: false, app_only: true, run: reveal },
];

/// Every tool: the film tools (hub_*, VideoHub's surface) and the footage editor's.
pub fn all() -> &'static [&'static Tool] {
    static A: std::sync::OnceLock<Vec<&'static Tool>> = std::sync::OnceLock::new();
    A.get_or_init(|| crate::film::TOOLS.iter().chain(crate::studio::TOOLS.iter()).chain(crate::extra::TOOLS.iter()).chain(crate::ai::TOOLS.iter()).chain(crate::publish::TOOLS.iter()).chain(crate::layers::TOOLS.iter()).chain(crate::cues::TOOLS.iter()).chain(crate::puppets::TOOLS.iter()).chain(crate::manage::TOOLS.iter()).chain(T.iter()).collect())
}

fn summarize(name: &str, a: &Value, v: &Value) -> String {
    let bit = |k: &str| js(a, k).map(|s| format!(" {k}={s}")).unwrap_or_default();
    let tail = match name {
        "render" => format!(" {} -> {}", jsd(a, "mode", "preview"), jsd(&v["qa"], "verdict", "")),
        "reel_set" | "beat_add" | "style_apply" => format!(" ({} warnings)", ja(v, "warnings").len()),
        "autocut" | "retakes" => format!(" {} takes", ja(v, "takes").len()),
        "clips_find" => format!(" {} clips", ja(v, "clips").len()),
        "transcribe" => format!(" {} words", v["words"]),
        _ => String::new(),
    };
    format!("{name}{}{}{tail}", bit("reel"), bit("style"))
}

pub fn call_with(cfg: &Config, origin: &str, name: &str, args: &Value, background: bool, rep: Rep) -> Result<Value> {
    let tool = all().iter().find(|t| t.name == name).ok_or_else(|| anyhow!("no tool '{name}'. Tools: {}", all().iter().filter(|t| !t.app_only).map(|t| t.name).collect::<Vec<_>>().join(", ")))?;
    for (n, _, _) in tool.params.iter().filter(|p| p.0.ends_with('!')) {
        if args.get(n.trim_end_matches('!')).map(|v| v.is_null()).unwrap_or(true) {
            bail!("`{}` is required for {name}", n.trim_end_matches('!'));
        }
    }
    let project = js(args, "project").or_else(|| js(args, "film")).unwrap_or("").to_string();
    if tool.long && background {
        let (cfg2, origin2, args2, name2, proj2) = (cfg.clone(), origin.to_string(), args.clone(), name.to_string(), project.clone());
        let run = tool.run;
        let id = jobs::spawn(name, &project, move |rep| {
            let ctx = Ctx { cfg: cfg2.clone(), origin: origin2.clone(), rep: rep.clone() };
            let res = run(&ctx, &args2);
            project::log_activity(&cfg2, &origin2, &name2, &proj2, res.is_ok(), &match &res {
                Ok(v) => summarize(&name2, &args2, v),
                Err(e) => format!("{name2} failed: {e}"),
            });
            res
        });
        return Ok(json!({"job": id, "note": "running in the background; `job` with wait=true waits for it"}));
    }
    let ctx = Ctx { cfg: cfg.clone(), origin: origin.to_string(), rep };
    let res = (tool.run)(&ctx, args);
    let quiet = matches!(name, "projects" | "project" | "jobs" | "job" | "hub_films" | "hub_project" | "hub_job" | "hub_notes" | "hub_frame" | "hub_versions" | "hub_workspace" | "hub_cleanup" | "hub_sound" | "hub_thumbs" | "hub_thumb" | "hub_thumb_brief" | "hub_2d" | "settings" | "doctor" | "reel" | "styles" | "notes" | "library" | "pick_file" | "reveal" | "words" | "taste" | "recipe" | "puppet_list" | "puppet_get") && res.is_ok() && args.get("set").is_none() && args.get("add").is_none()
        // reads that only look: a sync that only checks, a file tool that only lists or reads
        || res.is_ok() && (name == "puppet_sync" && jb(args, "check") == Some(true) || name == "hub_file" && matches!(jsd(args, "action", "read"), "list" | "read"));
    if !quiet {
        project::log_activity(cfg, origin, name, &project, res.is_ok(), &match &res {
            Ok(v) => summarize(name, args, v),
            Err(e) => format!("{name} failed: {e}"),
        });
    }
    res
}

pub fn call(origin: &str, name: &str, args: &Value, background: bool) -> Result<Value> {
    call_with(&Config::load(), origin, name, args, background, Rep::none())
}
