//! Styles: presets (brand, captions, sound, look), the caption templates, and the recipe style packs.
use crate::util::*;
use serde_json::{json, Value};

/// The twelve caption templates as parameter sets. A reel's `captions` block overrides any field.
/// Sizes are CSS-style pixels on a 1080x1920 canvas.
pub fn caption_template(name: &str) -> Value {
    match name {
        "house" => json!({"font": "Arial", "weight": 700, "size": 56, "color": "#FFFFFF", "outline": 5, "mode": "phrase", "active": "pop", "y": 1300, "group": 3}),
        "pill" => json!({"font": "Geist", "weight": 600, "size": 60, "color": "#FFFFFF", "mode": "phrase", "active": "color", "bg": "pill", "bgColor": "#111111", "bgAlpha": 0.82, "y": 1280, "group": 3}),
        "clean" => json!({"font": "brand", "weight": 800, "size": "brand", "color": "#FFFFFF", "outline": 3, "shadow": 2, "mode": "phrase", "active": "color", "y": 1200, "group": 3}),
        "editorial" => json!({"font": "brand", "weight": 700, "size": "brand", "case": "lower", "color": "#FFFFFF", "shadow": 3, "outline": 1.5, "mode": "accumulate", "entry": "blur", "keyword": "box", "keywordsAuto": false, "y": 1180, "group": 3}),
        "mini" => json!({"font": "Inter", "weight": 600, "size": 40, "color": "#FFFFFF", "outline": 2, "mode": "phrase", "active": "none", "y": 1360, "group": 5}),
        "chest" => json!({"font": "Inter", "weight": 600, "size": 38, "case": "lower", "color": "#FFFFFF", "shadow": 3, "outline": 1, "mode": "phrase", "active": "none", "strip": true, "y": 1305, "group": 6, "maxChars": 30}),
        "hype" => json!({"font": "Montserrat", "weight": 900, "size": 92, "case": "upper", "color": "#FFFFFF", "outline": 6, "mode": "phrase", "entry": "punch", "active": "box", "keyword": "color", "accent2": "#FFE14D", "y": 1160, "group": 3, "maxChars": 16}),
        "explosive" => json!({"font": "Unbounded", "weight": 800, "size": 64, "case": "upper", "color": "#FFFFFF", "outline": 5, "mode": "phrase", "dim": 0.45, "active": "grow", "keyword": "color", "y": 1200, "group": 3}),
        "line-focus" => json!({"font": "Inter", "weight": 700, "size": 60, "color": "#FFFFFF", "shadow": 3, "outline": 1.5, "mode": "lines2", "active": "none", "y": 1150, "group": 5}),
        "neo-minimal" => json!({"font": "Inter", "weight": 500, "size": 44, "case": "lower", "tracking": 0.08, "color": "#FFFFFF", "shadow": 2, "outline": 1, "mode": "accumulate", "entry": "blur", "keyword": "underline", "y": 1240, "group": 4}),
        "karaoke-fill" => json!({"font": "Montserrat", "weight": 800, "size": 66, "color": "#FFFFFF", "outline": 4, "mode": "karaoke", "dim": 0.5, "y": 1200, "group": 4}),
        "highlighter" => json!({"font": "Inter", "weight": 800, "size": 64, "color": "#FFFFFF", "outline": 3, "shadow": 2, "mode": "accumulate", "entry": "fade", "keyword": "marker", "y": 1200, "group": 4}),
        _ => caption_template("clean"),
    }
}

pub fn caption_styles() -> Vec<Value> {
    let rows: &[(&str, &str, &str)] = &[
        ("clean", "Clean", "Bold mixed-case words, the spoken word in the accent"),
        ("house", "House", "White with a black outline, the spoken word pops"),
        ("pill", "Pill", "Words on a dark rounded pill"),
        ("editorial", "Editorial", "Lowercase lines that resolve out of blur, accent boxes on chosen words"),
        ("hype", "Hype", "Big uppercase words punch in, the spoken word sits in a colour box"),
        ("explosive", "Explosive", "The phrase waits dimmed, each spoken word grows hot"),
        ("line-focus", "Line focus", "A lyrics view: this line bright, the next one waiting"),
        ("neo-minimal", "Neo minimal", "Small spaced lowercase, a hairline under the key word"),
        ("karaoke-fill", "Karaoke fill", "A colour fill sweeps through each word as it is spoken"),
        ("highlighter", "Highlighter", "Words appear as spoken, a marker swipe on the key word"),
        ("mini", "Mini", "Small captions low on the frame"),
        ("chest", "Chest", "Small lowercase phrases on the chest, cut in on the first word"),
        ("none", "None", "No captions (the recording already has them)"),
    ];
    rows.iter().map(|(id, label, about)| json!({"id": id, "label": label, "about": about})).collect()
}

pub fn load_preset(name: &str) -> Option<Value> {
    let lib = library_dir().join("styles");
    for p in [lib.join(format!("{name}.json")), lib.join("recipes").join(format!("{name}.json")), lib.join("user").join(format!("{name}.json"))] {
        if let Ok(v) = read_json(&p) {
            return Some(v);
        }
    }
    None
}

pub fn list_styles() -> Vec<Value> {
    let lib = library_dir().join("styles");
    let mut out = vec![];
    for (sub, kind) in [("", "preset"), ("recipes", "recipe"), ("user", "saved")] {
        let Ok(rd) = std::fs::read_dir(lib.join(sub)) else { continue };
        let mut names: Vec<String> = rd.flatten().filter_map(|e| e.file_name().to_string_lossy().strip_suffix(".json").map(|s| s.to_string())).collect();
        names.sort();
        for n in names {
            let Some(v) = load_preset(&n) else { continue };
            out.push(json!({
                "id": n, "kind": kind,
                "name": js(&v, "_name").map(|s| s.to_string()).unwrap_or_else(|| { let mut c = n.chars(); c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default() }),
                "about": jsd(&v, "_about", ""), "grammar": jsd(&v, "_grammar", ""),
                "accent": jsd(&v["brand"], "accent", "#FFD23F"), "ink": jsd(&v["brand"], "ink", "#111111"), "font": jsd(&v["brand"], "font", "Montserrat"),
                "captions": jsd(&v["captions"], "style", "clean"), "sfx": jsd(&v, "sfxProfile", "standard"),
                "music": !v["music"].is_null(), "recipe": v["_recipe"],
            }));
        }
    }
    out
}

fn luminance(hex: &str) -> f64 {
    let h = hex.trim_start_matches('#');
    if h.len() < 6 {
        return 0.0;
    }
    let c = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0) as f64 / 255.0;
    0.2126 * c(0) + 0.7152 * c(2) + 0.0722 * c(4)
}

/// The reel with its preset merged underneath and the caption parameters worked out (`_cap`).
pub fn resolve(reel: &Value) -> Value {
    let style = jsd(reel, "style", "clean");
    let mut r = load_preset(style).unwrap_or_else(|| json!({}));
    if let Some(o) = r.as_object_mut() {
        o.retain(|k, _| !k.starts_with('_'));
    }
    merge(&mut r, reel);
    let mut brand = json!({"accent": "#FFD23F", "accentDark": "#E0B400", "ink": "#111111", "capSize": 60, "font": "Montserrat"});
    merge(&mut brand, &r["brand"].clone());
    if brand.get("onAccent").is_none() {
        brand["onAccent"] = json!(if luminance(jsd(&brand, "accent", "#000000")) > 0.55 { "#111111" } else { "#FFFFFF" });
    }
    let caps = r["captions"].clone();
    let name = jsd(&caps, "style", "clean").to_string();
    let mut cap = caption_template(&name);
    if jsd(&cap, "font", "") == "brand" {
        cap["font"] = brand["font"].clone();
    }
    if cap["size"] == json!("brand") {
        cap["size"] = brand["capSize"].clone();
    }
    cap["accent"] = brand["accent"].clone();
    cap["onAccent"] = brand["onAccent"].clone();
    if caps.is_object() {
        merge(&mut cap, &caps);
    }
    // Upstream spellings.
    if jb(&cap, "upper") == Some(true) {
        cap["case"] = json!("upper");
    }
    if let Some(h) = js(&cap, "highlight") {
        if h.starts_with('#') {
            cap["accent"] = json!(h);
            if jsd(&cap, "active", "") == "pop" {
                cap["active"] = json!("color");
            }
        }
    }
    if jsd(&cap, "accent", "") == "brand" {
        cap["accent"] = brand["accent"].clone();
    }
    if jsd(&cap, "keywordStyle", "") == "serif" {
        cap["keyword"] = json!("serif");
        if cap.get("keywordFont").is_none() {
            cap["keywordFont"] = json!(jsd(&brand, "serif", "Playfair Display"));
        }
    }
    cap["style"] = json!(name);
    r["brand"] = brand;
    r["_cap"] = cap;
    r
}
