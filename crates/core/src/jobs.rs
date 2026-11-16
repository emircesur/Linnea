//! Long work (imports, transcripts, renders) runs as jobs: one at a time per process, with progress,
//! cancellation, and a status file so the app sees jobs an agent started from another process.
use crate::config::Config;
use crate::proc::Rep;
use crate::util::*;
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

struct Entry {
    state: Value,
    cancel: Arc<AtomicBool>,
}

struct Jobs {
    map: Mutex<HashMap<String, Entry>>,
    serial: Mutex<()>,
}

fn jobs() -> &'static Jobs {
    static J: OnceLock<Jobs> = OnceLock::new();
    J.get_or_init(|| Jobs { map: Mutex::new(HashMap::new()), serial: Mutex::new(()) })
}

fn dir() -> PathBuf {
    Config::load().state().join("jobs")
}

fn update(id: &str, f: impl FnOnce(&mut Value)) {
    let mut m = jobs().map.lock().unwrap();
    if let Some(e) = m.get_mut(id) {
        f(&mut e.state);
        e.state["updated"] = json!(now_ms());
        let _ = write_json(&dir().join(format!("{id}.json")), &e.state);
    }
}

pub fn spawn(kind: &str, project: &str, f: impl FnOnce(&Rep) -> Result<Value> + Send + 'static) -> String {
    let id = new_id("j");
    let cancel = Arc::new(AtomicBool::new(false));
    let state = json!({"id": id, "kind": kind, "project": project, "status": "queued", "progress": 0.0, "message": "", "created": now_ms(), "updated": now_ms(), "pid": std::process::id()});
    jobs().map.lock().unwrap().insert(id.clone(), Entry { state: state.clone(), cancel: cancel.clone() });
    let _ = write_json(&dir().join(format!("{id}.json")), &state);
    let jid = id.clone();
    let kind_s = kind.to_string();
    std::thread::spawn(move || {
        let _lock = jobs().serial.lock().unwrap_or_else(|e| e.into_inner());
        if cancel.load(Ordering::Relaxed) {
            update(&jid, |s| s["status"] = json!("cancelled"));
            return;
        }
        // One render at a time on this computer, also across Linnea windows, the terminal and agents.
        let renders = ["render", "hub_render", "render_batch", "heygen_translate"].contains(&kind_s.as_str());
        let lock = dir().parent().map(|d| d.join("render.lock"));
        if renders {
            if let Some(l) = &lock {
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        update(&jid, |s| s["status"] = json!("cancelled"));
                        return;
                    }
                    let stale = std::fs::metadata(l).and_then(|m| m.modified()).map(|t| t.elapsed().map(|e| e.as_secs() > 20).unwrap_or(true)).unwrap_or(true);
                    if !l.exists() || stale {
                        let _ = std::fs::write(l, std::process::id().to_string());
                        break;
                    }
                    update(&jid, |s| s["message"] = json!("waiting for another render"));
                    std::thread::sleep(std::time::Duration::from_secs(2));
                }
            }
        }
        update(&jid, |s| s["status"] = json!("running"));
        let pid = jid.clone();
        let last = Arc::new(Mutex::new((0.0f64, 0u64)));
        let rep = Rep::new(
            Arc::new(move |frac, msg| {
                let mut l = last.lock().unwrap();
                let now = now_ms();
                if (frac - l.0).abs() >= 0.01 || now - l.1 > 800 {
                    *l = (frac, now);
                    update(&pid, |s| {
                        s["progress"] = json!((frac * 1000.0).round() / 1000.0);
                        s["message"] = json!(msg);
                    });
                }
            }),
            cancel.clone(),
        );
        // Heartbeat, so another process can tell a live job from one whose process died.
        let alive = Arc::new(AtomicBool::new(true));
        let (a2, hid) = (alive.clone(), jid.clone());
        let lock2 = if renders { lock.clone() } else { None };
        std::thread::spawn(move || {
            while a2.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_secs(3));
                if a2.load(Ordering::Relaxed) {
                    update(&hid, |_| {});
                    if let Some(l) = &lock2 {
                        let _ = std::fs::write(l, std::process::id().to_string());
                    }
                }
            }
        });
        let res = f(&rep);
        alive.store(false, Ordering::Relaxed);
        if renders {
            if let Some(l) = &lock {
                let _ = std::fs::remove_file(l);
            }
            notify(if res.is_ok() { "Render bitti" } else { "Render durdu" }, &match &res {
                Ok(v) => v["file"].as_str().or_else(|| v["renders"][0]["file"].as_str()).unwrap_or("").rsplit('/').next().unwrap_or("").to_string(),
                Err(e) => e.to_string().chars().take(120).collect(),
            });
        }
        update(&jid, |s| match res {
            Ok(v) => {
                s["status"] = json!("done");
                s["progress"] = json!(1.0);
                s["result"] = v;
            }
            Err(e) => {
                let msg = e.to_string();
                s["status"] = json!(if msg == "cancelled" { "cancelled" } else { "failed" });
                s["error"] = json!(msg);
            }
        });
    });
    id
}

pub fn get(id: &str) -> Option<Value> {
    if let Some(e) = jobs().map.lock().unwrap().get(id) {
        return Some(e.state.clone());
    }
    let mut v = read_json(&dir().join(format!("{id}.json"))).ok()?;
    if matches!(jsd(&v, "status", ""), "running" | "queued") && now_ms().saturating_sub(jfd(&v, "updated", 0.0) as u64) > 15_000 {
        v["status"] = json!("lost");
        v["error"] = json!("the process that ran this job is gone");
    }
    Some(v)
}

pub fn list() -> Vec<Value> {
    let mut ids: Vec<String> = jobs().map.lock().unwrap().keys().cloned().collect();
    if let Ok(rd) = std::fs::read_dir(dir()) {
        for e in rd.flatten() {
            if let Some(id) = e.file_name().to_string_lossy().strip_suffix(".json") {
                if !ids.iter().any(|x| x == id) {
                    ids.push(id.to_string());
                }
            }
        }
    }
    let day = 24 * 3600 * 1000;
    let mut v: Vec<Value> = ids.iter().filter_map(|i| get(i)).filter(|j| now_ms().saturating_sub(jfd(j, "created", 0.0) as u64) < day).collect();
    v.sort_by(|a, b| jfd(b, "created", 0.0).partial_cmp(&jfd(a, "created", 0.0)).unwrap());
    v.truncate(40);
    v
}

pub fn cancel(id: &str) -> bool {
    match jobs().map.lock().unwrap().get(id) {
        Some(e) => {
            e.cancel.store(true, Ordering::Relaxed);
            true
        }
        None => false,
    }
}

/// Wait for a job to end, at most `secs`. Returns its state either way.
pub fn wait(id: &str, secs: u64) -> Option<Value> {
    let until = now_ms() + secs * 1000;
    loop {
        let j = get(id)?;
        if !matches!(jsd(&j, "status", ""), "running" | "queued") || now_ms() >= until {
            return Some(j);
        }
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
}

/// A system notification, when Linnea or another program is in front.
fn notify(title: &str, text: &str) {
    #[cfg(windows)]
    {
        let esc = |s: &str| s.replace('\'', "''").replace('<', " ").replace('>', " ").replace('&', "+");
        let script = format!(
            "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null; $x = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02); $t = $x.GetElementsByTagName('text'); $t.Item(0).AppendChild($x.CreateTextNode('{}')) > $null; $t.Item(1).AppendChild($x.CreateTextNode('{}')) > $null; [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('Linnea').Show([Windows.UI.Notifications.ToastNotification]::new($x))",
            esc(title), esc(text)
        );
        let _ = crate::proc::cmd(std::path::Path::new("powershell.exe")).args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script]).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let q = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!("display notification \"{}\" with title \"{}\"", q(text), q(title));
        let _ = std::process::Command::new("osascript").args(["-e", &script]).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("notify-send").args(["-a", "Linnea", title, text]).spawn();
    }
}
