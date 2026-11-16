//! The local server behind the window: the UI's files, the tools over HTTP, and media with range
//! requests. It listens on 127.0.0.1 only and refuses foreign Hosts and cross-site writes, because
//! its tools can run ffmpeg on this machine.
use crate::config::{self, Config};
use crate::project::{self, Project};
use crate::tools;
use crate::util::*;
use serde_json::{json, Value};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use tiny_http::{Header, Method, Request, Response, StatusCode};

fn h(k: &str, v: &str) -> Header {
    Header::from_bytes(k.as_bytes(), v.as_bytes()).unwrap()
}

fn mime(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase().as_str() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        _ => "application/octet-stream",
    }
}

pub fn ui_dir() -> Option<PathBuf> {
    find_resource("LINNEA_UI", "ui/dist", "index.html").or_else(|| find_resource("LINNEA_UI", "ui", "index.html"))
}

fn json_resp(req: Request, status: u16, v: &Value) {
    let body = v.to_string();
    let _ = req.respond(Response::from_string(body).with_status_code(status).with_header(h("Content-Type", "application/json; charset=utf-8")).with_header(h("Cache-Control", "no-store")));
}

fn query(url: &str, key: &str) -> Option<String> {
    let q = url.split_once('?')?.1;
    for part in q.split('&') {
        if let Some((k, v)) = part.split_once('=') {
            if k == key {
                return Some(pct_decode(v));
            }
        }
    }
    None
}

fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 3 <= b.len() => {
                let hex = std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(v) => {
                        out.push(v);
                        i += 3;
                    }
                    _ => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Only files Linnea owns or a project's own source may be served.
fn allowed(cfg: &Config, p: &Path) -> bool {
    let Ok(c) = p.canonicalize() else { return false };
    let roots = [cfg.workspace(), library_dir(), config::data_dir()];
    if roots.iter().filter_map(|r| r.canonicalize().ok()).any(|r| c.starts_with(r)) {
        return true;
    }
    Project::list(cfg).iter().any(|s| {
        Project::open(cfg, jsd(s, "id", "")).ok().and_then(|pr| pr.source().ok()).and_then(|sp| sp.canonicalize().ok()).map(|sp| sp == c).unwrap_or(false)
    })
}

fn serve_file(req: Request, path: &Path, cache: bool) {
    let Ok(mut f) = std::fs::File::open(path) else {
        let _ = req.respond(Response::from_string("not found").with_status_code(404));
        return;
    };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let range = req.headers().iter().find(|x| x.field.equiv("Range")).map(|x| x.value.as_str().to_string());
    let ct = h("Content-Type", mime(path));
    let cc = h("Cache-Control", if cache { "max-age=3600" } else { "no-cache" });
    if let Some(r) = range.as_deref().and_then(|r| r.strip_prefix("bytes=")) {
        let (a, b) = r.split_once('-').unwrap_or((r, ""));
        let start: u64 = a.parse().unwrap_or(0).min(len.saturating_sub(1));
        let end: u64 = b.parse().unwrap_or(len.saturating_sub(1)).min(len.saturating_sub(1)).min(start + 8 * 1024 * 1024 - 1);
        let n = end + 1 - start;
        let _ = f.seek(SeekFrom::Start(start));
        let resp = Response::new(StatusCode(206), vec![ct, cc, h("Accept-Ranges", "bytes"), h("Content-Range", &format!("bytes {start}-{end}/{len}"))], f.take(n), Some(n as usize), None);
        let _ = req.respond(resp);
    } else {
        let resp = Response::new(StatusCode(200), vec![ct, cc, h("Accept-Ranges", "bytes")], f, Some(len as usize), None);
        let _ = req.respond(resp);
    }
}

static RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Whether this process serves the interface (and the /live/ pages the renderer loads).
pub fn running() -> bool {
    RUNNING.load(std::sync::atomic::Ordering::Relaxed)
}

fn handle(mut req: Request, port: u16) {
    let url = req.url().to_string();
    let path = url.split('?').next().unwrap_or("/").to_string();
    let header = |name: &'static str| req.headers().iter().find(|x| x.field.equiv(name)).map(|x| x.value.as_str().to_string());
    let host_ok = header("Host").map(|v| v == format!("127.0.0.1:{port}") || v == format!("localhost:{port}")).unwrap_or(false);
    let origin_ok = header("Origin").map(|o| o == format!("http://127.0.0.1:{port}") || o == format!("http://localhost:{port}")).unwrap_or(true);
    let cross = header("Sec-Fetch-Site").map(|s| s == "cross-site").unwrap_or(false);
    if !host_ok || cross || (req.method() != &Method::Get && !origin_ok) {
        let _ = req.respond(Response::from_string("forbidden").with_status_code(403));
        return;
    }
    let cfg = Config::load();
    if let Some(name) = path.strip_prefix("/api/tool/") {
        if req.method() != &Method::Post {
            return json_resp(req, 405, &json!({"ok": false, "error": "POST only"}));
        }
        // pictures arrive as data URLs (puppet assets, dropped layers, note drawings): leave room for them
        const LIMIT: u64 = 64 * 1024 * 1024;
        let mut body = String::new();
        let _ = req.as_reader().take(LIMIT + 1).read_to_string(&mut body);
        if body.len() as u64 > LIMIT {
            return json_resp(req, 200, &json!({"ok": false, "error": "the request is larger than 64 MB; use a smaller picture"}));
        }
        let args: Value = if body.trim().is_empty() {
            json!({})
        } else {
            match serde_json::from_str::<Value>(&body) {
                Ok(v) if v.is_object() => v,
                // running a tool with silently emptied arguments would report the wrong problem
                _ => return json_resp(req, 200, &json!({"ok": false, "error": "the tool arguments are not a JSON object"})),
            }
        };
        let origin = jsd(&args, "_origin", "app").to_string();
        let inline = jb(&args, "_inline") == Some(true);
        let name = name.to_string();
        let out = match tools::call(&origin, &name, &args, !inline) {
            Ok(v) => json!({"ok": true, "result": v}),
            Err(e) => json!({"ok": false, "error": e.to_string()}),
        };
        return json_resp(req, 200, &out);
    }
    if path == "/api/activity" {
        let since = query(&url, "since").and_then(|s| s.parse().ok()).unwrap_or(0);
        let focus = read_json_or(&cfg.state().join("focus.json"), Value::Null);
        return json_resp(req, 200, &json!({"events": project::read_activity(&cfg, since, 200), "focus": focus, "jobs": crate::jobs::list(), "now": now_ms()}));
    }
    if path == "/oauth/youtube" {
        let msg = match query(&url, "code") {
            Some(code) => match crate::publish::oauth_youtube(&code, port) {
                Ok(()) => "YouTube connected. You can close this tab and go back to Linnea.".to_string(),
                Err(e) => format!("YouTube sign-in failed: {e}"),
            },
            None => format!("YouTube sign-in was cancelled: {}", query(&url, "error").unwrap_or_default()),
        };
        let _ = req.respond(Response::from_string(format!("<!doctype html><meta charset=utf-8><body style=\"font:16px system-ui;padding:40px\">{msg}</body>")).with_header(h("Content-Type", "text/html; charset=utf-8")));
        return;
    }
    if let Some(rest) = path.strip_prefix("/overlay/") {
        let rest = pct_decode(rest);
        let mut it = rest.splitn(3, '/');
        let (pid, rid, rel) = (it.next().unwrap_or(""), it.next().unwrap_or("main"), it.next().unwrap_or("index.html"));
        if rel.split(|c: char| c == '/' || c == '\\').any(|x| x == "..") {
            let _ = req.respond(Response::from_string("forbidden").with_status_code(403));
            return;
        }
        return match crate::project::Project::open(&cfg, pid) {
            Ok(p) => serve_file(req, &crate::layers::dir(&p, rid).join(if rel.is_empty() { "index.html" } else { rel }), false),
            Err(_) => {
                let _ = req.respond(Response::from_string("no such project").with_status_code(404));
            }
        };
    }
    if path == "/api/film-rev" {
        let id = query(&url, "id").unwrap_or_default();
        let rev = crate::film::open(&cfg, &id).map(|f| crate::film::rev(&f)).unwrap_or(0);
        return json_resp(req, 200, &json!({"rev": rev}));
    }
    if let Some(rest) = path.strip_prefix("/live/") {
        let rest = pct_decode(rest);
        let (id, rel) = rest.split_once('/').unwrap_or((rest.as_str(), "index.html"));
        let Some(dir) = crate::film::dir_of(id) else {
            let _ = req.respond(Response::from_string("no such film").with_status_code(404));
            return;
        };
        let rel = if rel.is_empty() { "index.html" } else { rel };
        if rel.split(|c: char| c == '/' || c == '\\').any(|p| p == "..") {
            let _ = req.respond(Response::from_string("forbidden").with_status_code(403));
            return;
        }
        return serve_file(req, &dir.join(rel), false);
    }
    if path == "/api/tools" {
        let list: Vec<Value> = tools::all().iter().map(|t| json!({"name": t.name, "desc": t.desc, "schema": t.schema(), "long": t.long, "app_only": t.app_only})).collect();
        return json_resp(req, 200, &json!({"tools": list, "version": crate::VERSION, "exe": fwd(&exe_dir())}));
    }
    if path == "/file" {
        let Some(p) = query(&url, "path") else {
            let _ = req.respond(Response::from_string("no path").with_status_code(400));
            return;
        };
        let p = PathBuf::from(p);
        if !allowed(&cfg, &p) {
            let _ = req.respond(Response::from_string("forbidden").with_status_code(403));
            return;
        }
        return serve_file(req, &p, false);
    }
    // The interface.
    let Some(ui) = ui_dir() else {
        let _ = req.respond(Response::from_string("Linnea's interface files were not found (ui/dist). Build the UI or set LINNEA_UI.").with_status_code(500));
        return;
    };
    let rel = path.trim_start_matches('/');
    let mut file = if rel.is_empty() { ui.join("index.html") } else { ui.join(rel) };
    if rel.contains("..") || !file.is_file() {
        file = ui.join("index.html");
    }
    let cache = rel.starts_with("assets/");
    serve_file(req, &file, cache);
}

/// Bind and serve in background threads. Returns the port.
pub fn start(want_port: Option<u16>) -> anyhow::Result<u16> {
    let base = want_port.unwrap_or_else(|| Config::load().get("port").and_then(|p| p.as_u64()).unwrap_or(7790) as u16);
    let mut last = None;
    for port in base..base + 12 {
        match tiny_http::Server::http(("127.0.0.1", port)) {
            Ok(server) => {
                let server = std::sync::Arc::new(server);
                RUNNING.store(true, std::sync::atomic::Ordering::Relaxed);
                let _ = write_json(&config::config_dir().join("server.json"), &json!({"port": port, "pid": std::process::id()}));
                for _ in 0..8 {
                    let s = server.clone();
                    std::thread::spawn(move || {
                        while let Ok(req) = s.recv() {
                            handle(req, port);
                        }
                    });
                }
                return Ok(port);
            }
            Err(e) => last = Some(e.to_string()),
        }
    }
    anyhow::bail!("cannot open a local port near {base}: {}", last.unwrap_or_default())
}
