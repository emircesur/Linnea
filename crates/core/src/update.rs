//! Updating Linnea from its GitHub releases. `app_update` looks for a newer release (at most every few
//! hours unless asked) and installs it: on Windows the new Linnea-Setup.exe runs silently, waits for this app to
//! close itself, and starts it again, on Linux the archive's install.sh replaces the program, on macOS the disk image opens for the
//! drag to Applications. The user's films, projects and settings are never touched.
use crate::config::{config_dir, data_dir};
use crate::tools::{Ctx, Tool};
use crate::util::*;
use anyhow::{bail, Result};
use serde_json::{json, Value};

const REPO: &str = "emircesur/Linnea";
const EVERY_MS: u64 = 6 * 3600 * 1000;

/// 0.1.12 > 0.1.3, and a leading v is ignored.
fn newer(a: &str, b: &str) -> bool {
    let parts = |s: &str| s.trim_start_matches('v').split(['.', '-']).map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    let (x, y) = (parts(a), parts(b));
    for i in 0..x.len().max(y.len()) {
        let (p, q) = (x.get(i).copied().unwrap_or(0), y.get(i).copied().unwrap_or(0));
        if p != q {
            return p > q;
        }
    }
    false
}

/// The download for this computer among a release's files.
fn pick_asset(assets: &[Value]) -> Option<Value> {
    let want = |n: &str| -> bool {
        let n = n.to_lowercase();
        if cfg!(windows) {
            n.ends_with(".exe") && n.contains("setup")
        } else if cfg!(target_os = "macos") {
            n.ends_with(".dmg") && n.contains(if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" })
        } else {
            n.ends_with(".tar.gz") && n.contains("linux")
        }
    };
    assets.iter().find(|a| want(jsd(a, "name", ""))).cloned()
}

fn cache_path() -> std::path::PathBuf {
    config_dir().join("update.json")
}

fn check(force: bool) -> Result<Value> {
    let cached = read_json_or(&cache_path(), Value::Null);
    if !force && !cached.is_null() && now_ms().saturating_sub(cached["checked"].as_u64().unwrap_or(0)) < EVERY_MS {
        return Ok(with_state(cached));
    }
    let (status, v) = crate::net::get_json(&format!("https://api.github.com/repos/{REPO}/releases/latest"), &[("User-Agent", "linnea".into()), ("Accept", "application/vnd.github+json".into())], 20)?;
    if status == 404 {
        bail!("no release is published on GitHub yet");
    }
    if status != 200 {
        bail!("GitHub answered {status} while looking for updates");
    }
    let tag = jsd(&v, "tag_name", "").to_string();
    let asset = pick_asset(&ja(&v, "assets"));
    let out = json!({
        "checked": now_ms(), "latest": tag.trim_start_matches('v'), "tag": tag, "page": v["html_url"],
        "notes": jsd(&v, "body", "").chars().take(4000).collect::<String>(), "published": v["published_at"],
        "asset": asset.as_ref().map(|a| json!({"name": a["name"], "url": a["browser_download_url"], "size": a["size"]})),
    });
    let _ = write_json(&cache_path(), &out);
    Ok(with_state(out))
}

fn with_state(mut v: Value) -> Value {
    v["current"] = json!(crate::VERSION);
    v["available"] = json!(newer(jsd(&v, "latest", "0"), crate::VERSION));
    v
}

fn install(c: &Ctx) -> Result<Value> {
    let info = check(true)?;
    if info["available"] != json!(true) {
        return Ok(json!({"installed": false, "note": format!("Linnea {} is the newest version", crate::VERSION)}));
    }
    let Some(asset) = info.get("asset").filter(|a| !a.is_null()) else {
        bail!("the release {} has no download for this computer; get it from {}", jsd(&info, "tag", ""), jsd(&info, "page", ""));
    };
    let name = jsd(asset, "name", "linnea-update");
    let file = data_dir().join("updates").join(name);
    crate::extra::download(jsd(asset, "url", ""), &file, &c.rep, name)?;
    c.rep.p(1.0, "starting the installer");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // The installer waits for this app to be gone, replaces the program files and starts it again. It runs
        // detached, in its own process group and outside any job this app was started in, so it outlives the app.
        let pid = std::process::id();
        let spawn = |flags: u32| std::process::Command::new(&file).args(["--silent", "--update", &format!("--wait={pid}")]).creation_flags(flags).spawn();
        const DETACHED: u32 = 0x0000_0008;
        const NEW_GROUP: u32 = 0x0000_0200;
        const BREAKAWAY: u32 = 0x0100_0000;
        spawn(DETACHED | NEW_GROUP | BREAKAWAY).or_else(|_| spawn(DETACHED | NEW_GROUP))?;
        // The app steps aside by itself once the window has had the answer, so no file the installer replaces is
        // still held by a running Linnea (killing it mid-way is what left half-replaced program folders).
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(1500));
            std::process::exit(0);
        });
        return Ok(json!({"installed": true, "restarting": true, "version": info["latest"]}));
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(&file).spawn()?;
        return Ok(json!({"installed": false, "opened": fwd(&file), "version": info["latest"], "note": "drag Linnea to Applications in the window that opened, then start it again"}));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let dir = data_dir().join("updates").join(format!("linnea-{}", jsd(&info, "latest", "new")));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        let o = std::process::Command::new("tar").arg("-xzf").arg(&file).arg("-C").arg(&dir).output()?;
        if !o.status.success() {
            bail!("unpacking {name}: {}", String::from_utf8_lossy(&o.stderr));
        }
        let script = std::fs::read_dir(&dir)?.flatten().map(|e| e.path().join("install.sh")).find(|p| p.is_file());
        let Some(script) = script else { bail!("{name} has no install.sh") };
        let o = std::process::Command::new("bash").arg(&script).stdin(std::process::Stdio::null()).output()?;
        if !o.status.success() {
            bail!("install.sh: {}", String::from_utf8_lossy(&o.stderr));
        }
        Ok(json!({"installed": true, "restart": true, "version": info["latest"], "note": "close Linnea and start it again to use the new version"}))
    }
}

fn t_check(_c: &Ctx, a: &Value) -> Result<Value> {
    check(jb(a, "force") == Some(true))
}
fn t_install(c: &Ctx, _a: &Value) -> Result<Value> {
    install(c)
}

pub const TOOLS: &[Tool] = &[
    Tool { name: "app_update", desc: "Whether a newer Linnea is on GitHub: the newest version, whether it is newer than this one, and its notes. force=true asks GitHub now instead of using the answer of the last few hours.", params: &[("force", "boolean", "Ask GitHub now")], long: false, app_only: true, run: t_check },
    Tool { name: "app_update_install", desc: "Download the newest Linnea from GitHub and install it (Windows starts the app again by itself; on macOS the disk image opens).", params: &[], long: true, app_only: true, run: t_install },
];

#[cfg(test)]
mod tests {
    #[test]
    fn versions_compare() {
        assert!(super::newer("0.1.10", "0.1.9"));
        assert!(super::newer("v0.2.0", "0.1.3"));
        assert!(!super::newer("0.1.3", "0.1.3"));
        assert!(!super::newer("0.1.2", "0.1.3"));
    }
}
