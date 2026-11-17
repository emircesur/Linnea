//! Films in more than one language. A film keeps its own text; each language is a set of replacement
//! texts for the elements it draws (by element id, the same ids the user edits), plus an optional sound
//! track, in tweaks.json `langs`:
//!
//!   {"de": {"name": "Deutsch", "texts": {"hook-title": "Hier anfangen", "txt:Start here": "..."}, "audio": "audio/mix.de.wav"}}
//!
//! linnea2d.js lays the texts over the frame when the page is opened with ?lang=de (renders) or the app
//! switches with KE.setLang. `hub_render lang=de` renders that version as its own file; the layout check
//! finds the texts that a translation pushed off the frame or made much wider.
use crate::cdp::Page;
use crate::film::{self, Film};
use crate::tools::{Ctx, Tool};
use crate::util::*;
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};
use std::time::Duration;

/// Every text element the page draws (its original text and when it shows) and every box that leaves the
/// frame, drawn every `step` seconds.
const SCAN_JS: &str = r#"(async (dur, step, W, H) => {
  const seen = {}, over = {};
  for (let t = 0; t <= dur + 1e-6; t += step) {
    try { const r = window.renderOut(t); if (r && r.then) await r; } catch (e) { continue; }
    for (const b of (window.KE_BOXES || [])) {
      if (b.hidden) continue;
      const src = b.src !== undefined ? b.src : (b.kind === 'auto' ? b.label : undefined);
      const x = seen[b.id];
      if (x) { x.t1 = Math.min(dur, t + step); x.w = Math.max(x.w, b.w); }
      else if (typeof src === 'string' && src.trim()) seen[b.id] = { id: b.id, text: src, shown: b.text, label: b.label, t0: t, t1: Math.min(dur, t + step), w: b.w };
      const sides = [];
      if (b.x < -2) sides.push('left');
      if (b.y < -2) sides.push('top');
      if (b.x + b.w > W + 2) sides.push('right');
      if (b.y + b.h > H + 2) sides.push('bottom');
      if (sides.length && !over[b.id]) over[b.id] = { id: b.id, text: b.text, t: Math.round(t * 100) / 100, sides, box: [b.x, b.y, b.w, b.h].map(Math.round) };
    }
  }
  return { texts: Object.values(seen), overflow: over };
})"#;

fn tweaks_path(f: &Film) -> std::path::PathBuf {
    f.dir.join("tweaks.json")
}
pub fn langs(f: &Film) -> Map<String, Value> {
    read_json_or(&tweaks_path(f), json!({}))["langs"].as_object().cloned().unwrap_or_default()
}
fn save_langs(f: &Film, l: Map<String, Value>) -> Result<()> {
    let mut tw = read_json_or(&tweaks_path(f), json!({}));
    tw["langs"] = Value::Object(l);
    write_json(&tweaks_path(f), &tw)
}

/// A language code as a file name and URL part: letters, digits and - only (de, pt-BR, zh-Hans).
pub fn clean(code: &str) -> Result<String> {
    let c = code.trim();
    if c.is_empty() || c.len() > 16 || !c.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-') {
        bail!("a language is a short code such as de, fr, pt-BR or zh-Hans");
    }
    Ok(c.to_string())
}

/// The language's sound track, when it has one that exists.
pub fn audio(f: &Film, lang: &str) -> Option<std::path::PathBuf> {
    js(&langs(f).get(lang).cloned().unwrap_or(Value::Null), "audio").map(|a| f.dir.join(a)).filter(|p| p.is_file())
}

/// The page's address for a language ('' = the original).
pub fn url_for(url: &str, lang: &str) -> String {
    if lang.is_empty() {
        url.to_string()
    } else {
        format!("{url}{}lang={lang}", if url.contains('?') { '&' } else { '?' })
    }
}

/// Draw the film headless (in one language) and collect its texts and the boxes that leave the frame.
fn scan(c: &Ctx, f: &Film, lang: &str) -> Result<Value> {
    let info = film::info(&c.cfg, f);
    let (w, h, dur) = (jfd(&info, "width", 1920.0), jfd(&info, "height", 1080.0), jfd(&info, "dur", 10.0));
    let step = (dur / 400.0).max(0.5);
    let mut page = Page::open(&c.cfg, &url_for(&film::page_url(&c.cfg, f, &info), lang), w as u32, h as u32, true)?;
    page.wait_ready(Duration::from_secs(60))?;
    if page.eval("typeof window.KE_BOXES").unwrap_or(Value::Null) != json!("object") {
        bail!("the page does not load linnea2d.js, so its texts cannot be found: hub_2d install=true adds it");
    }
    page.eval(&format!("{SCAN_JS}({dur}, {step}, {w}, {h})"))
}

/// What a language still lacks: the source texts with no translation.
fn missing(texts: &[Value], tr: &Map<String, Value>) -> Vec<Value> {
    texts.iter().filter(|t| tr.get(jsd(t, "id", "")).and_then(|x| x.as_str()).map(|s| s.is_empty()).unwrap_or(true)).cloned().collect()
}

fn source(texts: &Value) -> Vec<Value> {
    ja(texts, "texts").iter().map(|t| json!({"id": t["id"], "text": t["text"], "t0": t["t0"], "t1": t["t1"]})).collect()
}

fn t_lang(c: &Ctx, a: &Value) -> Result<Value> {
    let f = film::open(&c.cfg, js(a, "film").unwrap_or(""))?;
    let mut all = langs(&f);
    match jsd(a, "action", "list") {
        "list" => {}
        "set" => {
            let code = clean(js(a, "lang").unwrap_or(""))?;
            let mut l = all.get(&code).cloned().unwrap_or_else(|| json!({}));
            if let Some(n) = js(a, "name") {
                l["name"] = json!(n);
            }
            if let Some(au) = js(a, "audio") {
                if !au.is_empty() && !f.dir.join(au).is_file() {
                    bail!("no sound file {au} in the film folder (give it relative to the folder, e.g. audio/mix.{code}.wav)");
                }
                l["audio"] = if au.is_empty() { Value::Null } else { json!(au) };
            }
            if let Some(t) = a.get("texts").and_then(|t| t.as_object()) {
                let mut cur = l["texts"].as_object().cloned().unwrap_or_default();
                for (k, v) in t {
                    match v.as_str() {
                        Some(s) if !s.is_empty() => {
                            cur.insert(k.clone(), json!(s));
                        }
                        _ => {
                            cur.remove(k);
                        }
                    }
                }
                l["texts"] = Value::Object(cur);
            }
            all.insert(code, l);
            save_langs(&f, all.clone())?;
        }
        "remove" => {
            let code = clean(js(a, "lang").unwrap_or(""))?;
            all.remove(&code);
            save_langs(&f, all.clone())?;
        }
        x => bail!("unknown action {x}: list, set or remove (hub_lang_scan finds texts, translates and checks)"),
    }
    let out: Vec<Value> = all.iter().map(|(k, v)| json!({"lang": k, "name": v["name"], "texts": v["texts"], "audio": v["audio"], "count": v["texts"].as_object().map(|o| o.len()).unwrap_or(0)})).collect();
    Ok(json!({"langs": out, "note": "hub_lang_scan action=texts lists the film's texts; render a language with hub_render lang=<code>"}))
}

const TRANSLATE: &str = "You translate the on-screen texts of a short video. You get a JSON object {id: text} and the target language. Answer with one JSON object {id: translation} with exactly the same ids and nothing else. Keep each translation about as long as the original (it has to fit the same place on screen), keep line breaks, numbers, emoji, names and brand names, and match the tone. Never add quotes or explanations.";

fn t_lang_scan(c: &Ctx, a: &Value) -> Result<Value> {
    let f = film::open(&c.cfg, js(a, "film").unwrap_or(""))?;
    let action = jsd(a, "action", "texts");
    c.rep.p(0.05, "drawing the film");
    let orig = scan(c, &f, "")?;
    let texts = source(&orig);
    match action {
        "texts" => {
            let all = langs(&f);
            let todo: Map<String, Value> = all.iter().map(|(k, v)| (k.clone(), json!(missing(&texts, &v["texts"].as_object().cloned().unwrap_or_default()).len()))).collect();
            Ok(json!({"texts": texts, "missing": todo, "note": "translate with hub_lang action=set lang=<code> texts={id: text} (ids from here), or hub_lang_scan action=translate with the built-in assistant"}))
        }
        "translate" => {
            let code = clean(js(a, "lang").unwrap_or(""))?;
            let mut all = langs(&f);
            let mut l = all.get(&code).cloned().unwrap_or_else(|| json!({}));
            let mut cur = l["texts"].as_object().cloned().unwrap_or_default();
            let redo = jb(a, "redo") == Some(true);
            let want: Map<String, Value> = texts.iter().filter(|t| redo || !cur.contains_key(jsd(t, "id", ""))).map(|t| (jsd(t, "id", "").to_string(), t["text"].clone())).collect();
            if want.is_empty() {
                return Ok(json!({"lang": code, "translated": 0, "note": "every text already has a translation (redo=true translates them again)"}));
            }
            let name = js(a, "name").or_else(|| js(&l, "name")).unwrap_or(&code).to_string();
            c.rep.p(0.5, &format!("translating {} texts", want.len()));
            let answer = crate::llm::complete(&c.cfg, TRANSLATE, &format!("Target language: {name} ({code})\n\n{}", Value::Object(want.clone())))?;
            let got = crate::llm::extract_json(&answer).and_then(|v| v.as_object().cloned()).ok_or_else(|| anyhow!("the assistant's answer was not a JSON object"))?;
            let mut n = 0;
            for (k, v) in got {
                if let (true, Some(s)) = (want.contains_key(&k), v.as_str()) {
                    cur.insert(k, json!(s));
                    n += 1;
                }
            }
            l["texts"] = Value::Object(cur);
            if l["name"].is_null() {
                l["name"] = json!(name);
            }
            all.insert(code.clone(), l);
            save_langs(&f, all)?;
            c.rep.p(1.0, "done");
            Ok(json!({"lang": code, "translated": n, "of": want.len()}))
        }
        "check" => {
            let code = clean(js(a, "lang").unwrap_or(""))?;
            if !langs(&f).contains_key(&code) {
                bail!("the film has no language {code}; hub_lang lists them");
            }
            c.rep.p(0.5, &format!("drawing the film in {code}"));
            let tr = scan(c, &f, &code)?;
            // only what the translation broke: a box the original keeps inside the frame, or one that grew
            let before = orig["overflow"].as_object().cloned().unwrap_or_default();
            let widths: std::collections::HashMap<String, f64> = ja(&orig, "texts").iter().map(|t| (jsd(t, "id", "").to_string(), jfd(t, "w", 0.0))).collect();
            let mut problems = vec![];
            for (id, o) in tr["overflow"].as_object().cloned().unwrap_or_default() {
                if !before.contains_key(&id) {
                    problems.push(json!({"id": id, "text": o["text"], "t": o["t"], "problem": format!("runs off the frame ({})", ja(&o, "sides").iter().filter_map(|s| s.as_str()).collect::<Vec<_>>().join(", "))}));
                }
            }
            for t in ja(&tr, "texts") {
                let id = jsd(t, "id", "");
                let (w0, w1) = (widths.get(id).copied().unwrap_or(0.0), jfd(t, "w", 0.0));
                if w0 > 0.0 && w1 > w0 * 1.3 && !problems.iter().any(|p| jsd(p, "id", "") == id) {
                    problems.push(json!({"id": id, "text": t["shown"], "t": t["t0"], "problem": format!("{}% wider than the original", ((w1 / w0 - 1.0) * 100.0).round())}));
                }
            }
            let left = missing(&texts, &langs(&f).get(&code).and_then(|l| l["texts"].as_object().cloned()).unwrap_or_default());
            c.rep.p(1.0, "done");
            Ok(json!({"lang": code, "problems": problems, "untranslated": left, "ok": problems.is_empty() && left.is_empty()}))
        }
        x => bail!("unknown action {x}: texts, translate or check"),
    }
}

pub const TOOLS: &[Tool] = &[
    Tool { name: "hub_lang", desc: "A film's languages (tweaks.json langs): each is a set of replacement texts for the elements it draws, by element id, and an optional sound track. action list (default), set (lang, name, texts {id: text}, merged; an empty text removes one; audio: a file in the film folder) or remove (lang). The app previews each language; hub_render lang=<code> renders it as its own file.", params: &[("film!", "string", "Film id"), ("action", "string", "list | set | remove"), ("lang", "string", "Language code: de, fr, pt-BR..."), ("name", "string", "Its name: Deutsch"), ("texts", "object", "{element id: translated text}"), ("audio", "string", "This language's sound, relative to the film folder")], long: false, app_only: false, run: t_lang },
    Tool { name: "hub_lang_scan", desc: "Draw the film headless to work on its languages. action texts: every text the film shows (element id, original text, when it shows) and how many each language still lacks. action translate (lang, name): the built-in assistant translates the missing ones (redo=true: all). action check (lang): the texts the translation pushed off the frame or made much wider, and the ones still untranslated.", params: &[("film!", "string", "Film id"), ("action", "string", "texts | translate | check"), ("lang", "string", "Language code"), ("name", "string", "Language name for the assistant"), ("redo", "boolean", "Translate every text again")], long: true, app_only: false, run: t_lang_scan },
];
