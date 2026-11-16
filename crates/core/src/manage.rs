//! Keeping the work tidy: delete (to the Recycle Bin, so it can come back), rename and duplicate films,
//! footage projects, their clip reels and thumbnails.
use crate::film;
use crate::project::Project;
use crate::proc;
use crate::tools::{Ctx, Tool};
use crate::util::*;
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Move a file or folder to the Recycle Bin (never deleted outright).
pub fn recycle(p: &Path) -> Result<()> {
    let cp = p.canonicalize().map_err(|_| anyhow!("no such path: {}", p.display()))?;
    let kind = if cp.is_dir() { "DeleteDirectory" } else { "DeleteFile" };
    // canonicalize gives \\?\C:\... on Windows, a form the shell's Recycle Bin call refuses
    let s = cp.display().to_string();
    let plain = if let Some(r) = s.strip_prefix(r"\\?\UNC\") { format!(r"\\{r}") } else { s.strip_prefix(r"\\?\").unwrap_or(&s).to_string() };
    let script = format!(
        "Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.FileIO.FileSystem]::{kind}('{}', 'OnlyErrorDialogs', 'SendToRecycleBin')",
        plain.replace('\'', "''")
    );
    let o = proc::run(proc::cmd(Path::new("powershell.exe")).args(["-NoProfile", "-Command", &script]))?;
    if !o.ok || cp.exists() {
        bail!("could not move it to the Recycle Bin (is a file in it open?): {}", proc::err_tail(&o.stderr, 2));
    }
    Ok(())
}

/// A folder may go only when it sits inside the workspace or inside a watched container of films, and is
/// not one of those roots itself.
fn removable(c: &Ctx, dir: &Path) -> Result<()> {
    let d = dir.canonicalize().map_err(|_| anyhow!("no such folder"))?;
    let ws = c.cfg.workspace().canonicalize().unwrap_or_default();
    let mut roots = vec![ws.clone(), film::films_dir(&c.cfg).canonicalize().unwrap_or_default(), ws.join("thumbnails")];
    let mut containers = vec![];
    for r in c.cfg.get("films.roots").and_then(|x| x.as_array()).cloned().unwrap_or_default() {
        let p = PathBuf::from(r.as_str().map(|s| s.to_string()).unwrap_or_else(|| jsd(&r, "path", "").to_string()));
        let p = p.canonicalize().unwrap_or(p);
        if jsd(&r, "type", "container") == "project" {
            roots.push(p);
        } else {
            containers.push(p.clone());
            roots.push(p);
        }
    }
    if roots.iter().any(|r| *r == d) {
        bail!("this is a root folder of your work, not one item; remove it from the watched folders in Settings instead");
    }
    if d.starts_with(&ws) || containers.iter().any(|r| d.starts_with(r)) {
        return Ok(());
    }
    bail!("only items inside the workspace or a watched films folder can be deleted from here")
}

fn busy(id: &str) -> Result<()> {
    if crate::jobs::list().iter().any(|j| jsd(j, "project", "") == id && matches!(jsd(j, "status", ""), "running" | "queued")) {
        bail!("a job is still running on it; stop it in the render queue first");
    }
    Ok(())
}

fn thumbs_root(c: &Ctx, a: &Value) -> Result<PathBuf> {
    Ok(match js(a, "film").filter(|s| !s.is_empty()) {
        Some(f) => film::open(&c.cfg, f)?.dir.join("thumbnails"),
        None => c.cfg.workspace().join("thumbnails"),
    })
}

fn t_delete(c: &Ctx, a: &Value) -> Result<Value> {
    let kind = jsd(a, "kind", "");
    let id = js(a, "id").filter(|s| !s.is_empty()).ok_or_else(|| anyhow!("`id` names what to delete"))?;
    match kind {
        "film" => {
            let f = film::open(&c.cfg, id)?;
            busy(&f.id)?;
            removable(c, &f.dir)?;
            recycle(&f.dir)?;
            Ok(json!({"deleted": "film", "id": f.id, "note": "moved to the Recycle Bin; restore it from there"}))
        }
        "project" => {
            let p = Project::open(&c.cfg, id)?;
            busy(&p.id())?;
            removable(c, &p.dir)?;
            recycle(&p.dir)?;
            Ok(json!({"deleted": "project", "id": id, "note": "moved to the Recycle Bin; restore it from there"}))
        }
        "reel" => {
            let mut p = Project::open(&c.cfg, jsd(a, "project", ""))?;
            if id == "main" {
                bail!("the main edit belongs to the project; delete the project instead");
            }
            let f = p.reel_path(id);
            if !f.is_file() {
                bail!("no reel '{id}'");
            }
            recycle(&f)?;
            let overlay = p.p(&format!("overlay/{id}"));
            if overlay.is_dir() {
                let _ = recycle(&overlay);
            }
            if p.active_reel() == id {
                p.manifest["active_reel"] = json!("main");
                p.save_manifest()?;
            }
            Ok(json!({"deleted": "reel", "id": id}))
        }
        "thumb" => {
            let d = thumbs_root(c, a)?.join(id);
            if !d.join("thumb.json").is_file() {
                bail!("no thumbnail '{id}'");
            }
            recycle(&d)?;
            Ok(json!({"deleted": "thumb", "id": id}))
        }
        other => bail!("unknown kind '{other}': film, project, reel or thumb"),
    }
}

fn t_rename(c: &Ctx, a: &Value) -> Result<Value> {
    let id = js(a, "id").filter(|s| !s.is_empty()).ok_or_else(|| anyhow!("`id` names what to rename"))?;
    let title = js(a, "title").map(|s| s.trim()).filter(|s| !s.is_empty()).ok_or_else(|| anyhow!("`title` is the new name"))?;
    match jsd(a, "kind", "") {
        "film" => {
            let f = film::open(&c.cfg, id)?;
            let pj = f.dir.join(".videohub").join("project.json");
            let mut m = read_json_or(&pj, json!({}));
            m["title"] = json!(title);
            write_json(&pj, &m)?;
        }
        "project" => {
            let mut p = Project::open(&c.cfg, id)?;
            p.manifest["title"] = json!(title);
            p.save_manifest()?;
        }
        "reel" => {
            let p = Project::open(&c.cfg, jsd(a, "project", ""))?;
            let mut r = p.load_reel(id)?;
            r["title"] = json!(title);
            p.save_reel(id, &r)?;
        }
        "thumb" => {
            let f = thumbs_root(c, a)?.join(id).join("thumb.json");
            let mut m = read_json(&f).map_err(|_| anyhow!("no thumbnail '{id}'"))?;
            m["title"] = json!(title);
            write_json(&f, &m)?;
        }
        other => bail!("unknown kind '{other}': film, project, reel or thumb"),
    }
    Ok(json!({"renamed": id, "title": title}))
}

/// Copy a film's working files: code, sound, puppets, pictures and the user's edits. Renders, notes,
/// versions and captured frames stay behind, so the copy starts clean.
fn copy_film(from: &Path, to: &Path, top: bool) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)?.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if top && (matches!(name.as_str(), "render" | ".videohub" | "thumbnails" | "node_modules" | ".git") || name.starts_with("chunks")) {
            continue;
        }
        if top && p.is_file() && name.ends_with(".mp4") && name.contains("_v") {
            continue;
        }
        if p.is_dir() {
            copy_film(&p, &to.join(&name), false)?;
        } else {
            std::fs::copy(&p, to.join(&name))?;
        }
    }
    Ok(())
}

fn t_duplicate(c: &Ctx, a: &Value) -> Result<Value> {
    let f = film::open(&c.cfg, js(a, "film").unwrap_or(""))?;
    let info = film::info(&c.cfg, &f);
    let title = js(a, "title").map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| format!("{} 2", jsd(&info, "title", &f.id)));
    let root = film::films_dir(&c.cfg);
    let base = slug(&title);
    let mut dir = root.join(&base);
    let mut n = 2;
    while dir.exists() {
        dir = root.join(format!("{base}-{n}"));
        n += 1;
    }
    copy_film(&f.dir, &dir, true)?;
    let mut pj = read_json_or(&f.dir.join(".videohub").join("project.json"), json!({}));
    if !pj.is_object() {
        pj = json!({});
    }
    pj["title"] = json!(title);
    pj["status"] = json!("in-progress");
    // a film that kept its size, length and page only in the page itself still gets them written down
    for k in ["page", "fps", "dur", "width", "height"] {
        if pj["live"][k].is_null() && !info[k].is_null() {
            if !pj["live"].is_object() {
                pj["live"] = json!({});
            }
            pj["live"][k] = info[k].clone();
        }
    }
    write_json(&dir.join(".videohub").join("project.json"), &pj)?;
    let made = film::open(&c.cfg, &dir.to_string_lossy())?;
    Ok(json!({"film": made.id, "dir": fwd(&made.dir), "title": title}))
}

pub const TOOLS: &[Tool] = &[
    Tool { name: "item_delete", desc: "Move a film, a footage project, one clip reel of a project, or a thumbnail to the Recycle Bin (it can be restored from there). Only items inside the workspace or a watched films folder; never while a job runs on it.", params: &[("kind!", "string", "film | project | reel | thumb"), ("id!", "string", "Its id"), ("project", "string", "The project (kind reel)"), ("film", "string", "The film whose thumbnail it is (kind thumb; empty for shared thumbnails)")], long: false, app_only: true, run: t_delete },
    Tool { name: "item_rename", desc: "Rename a film, a footage project, a clip reel or a thumbnail (its title; folders keep their names).", params: &[("kind!", "string", "film | project | reel | thumb"), ("id!", "string", "Its id"), ("title!", "string", "The new name"), ("project", "string", "The project (kind reel)"), ("film", "string", "The film (kind thumb)")], long: false, app_only: false, run: t_rename },
    Tool { name: "film_duplicate", desc: "Copy a film into a new one: its code, sound, puppets, pictures and the user's edits; renders, notes and versions stay behind.", params: &[("film!", "string", "Film id"), ("title", "string", "Title of the copy")], long: false, app_only: false, run: t_duplicate },
];
