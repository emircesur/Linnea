//! Kesit-Setup.exe: installs Linnea (Linnea Kesit in Turkish) for the current user (no admin rights), or removes it with --uninstall.
//! The program files ride inside this exe (dist/payload.zip, made by pack.py).
#![windows_subsystem = "windows"]
use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;

static PAYLOAD: &[u8] = include_bytes!("../../../dist/payload.zip");
const VERSION: &str = env!("CARGO_PKG_VERSION");
const UNINSTALL_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Kesit";

fn quiet(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}
static SILENT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
fn log(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(std::env::temp_dir().join("kesit-setup.log")) {
        let _ = writeln!(f, "{msg}");
    }
}
// The installer speaks the language the person picks first, and the app starts in it.
static TURKISH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
fn turkish() -> bool {
    TURKISH.load(std::sync::atomic::Ordering::Relaxed)
}
/// The English or the Turkish text.
fn l<'a>(en: &'a str, tr: &'a str) -> &'a str {
    if turkish() { tr } else { en }
}
/// The product's name: Linnea, in Turkish Linnea Kesit.
fn brand() -> &'static str {
    l("Linnea", "Linnea Kesit")
}

/// A yes/no question. `key` names it: a silent install answers yes only to the install or removal itself.
fn ask(key: &str, title: &str, text: &str) -> bool {
    log(&format!("ask: {key}"));
    if SILENT.load(std::sync::atomic::Ordering::Relaxed) {
        return key == "install" || key == "uninstall";
    }
    MessageDialog::new().set_title(title).set_description(text).set_buttons(MessageButtons::YesNo).set_level(MessageLevel::Info).show() == MessageDialogResult::Yes
}
fn tell(text: &str, error: bool) {
    log(text);
    if SILENT.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    MessageDialog::new().set_title(brand()).set_description(text).set_buttons(MessageButtons::Ok).set_level(if error { MessageLevel::Error } else { MessageLevel::Info }).show();
}

/// The app's settings file (the same one the app reads).
fn config_path() -> PathBuf {
    std::env::var("KESIT_HOME").map(PathBuf::from).unwrap_or_else(|_| dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("Kesit")).join("config.json")
}
fn read_config() -> serde_json::Value {
    std::fs::read_to_string(config_path()).ok().and_then(|s| serde_json::from_str(s.trim_start_matches('\u{feff}')).ok()).filter(|v: &serde_json::Value| v.is_object()).unwrap_or_else(|| serde_json::json!({}))
}

/// Pick the language: `--lang=en|tr` or the settings for a silent run, a question otherwise. Saved as ui.lang.
fn choose_language() {
    let mut cfg = read_config();
    let saved = cfg["ui"]["lang"].as_str().map(|s| s.to_string());
    let arg = std::env::args().find_map(|a| a.strip_prefix("--lang=").map(|s| s.to_string()));
    let lang = if let Some(a) = arg {
        a
    } else if SILENT.load(std::sync::atomic::Ordering::Relaxed) {
        saved.unwrap_or_else(|| "en".into())
    } else {
        let r = MessageDialog::new()
            .set_title("Linnea")
            .set_description("Choose the language of Linnea.\nLinnea Kesit'in dilini seçin.\n\nYou can change it later in Settings. / Daha sonra Ayarlar'dan değiştirebilirsiniz.")
            .set_buttons(MessageButtons::OkCancelCustom("English".into(), "Türkçe".into()))
            .set_level(MessageLevel::Info)
            .show();
        match r {
            MessageDialogResult::Custom(s) if s == "Türkçe" => "tr".into(),
            MessageDialogResult::Cancel => "tr".into(),
            _ => "en".into(),
        }
    };
    let lang = if lang == "tr" { "tr" } else { "en" };
    TURKISH.store(lang == "tr", std::sync::atomic::Ordering::Relaxed);
    if !cfg["ui"].is_object() {
        cfg["ui"] = serde_json::json!({});
    }
    cfg["ui"]["lang"] = serde_json::json!(lang);
    if let Some(d) = config_path().parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(config_path(), serde_json::to_string_pretty(&cfg).unwrap_or_default());
}
fn ps(script: &str) -> bool {
    quiet(Command::new("powershell.exe").args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", script])).status().map(|s| s.success()).unwrap_or(false)
}
fn psq(s: &Path) -> String {
    s.to_string_lossy().replace('\'', "''")
}

fn install_dir() -> PathBuf {
    dirs::data_local_dir().unwrap_or_else(|| PathBuf::from(".")).join("Programs").join("Kesit")
}
fn start_menu_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("Microsoft").join("Windows").join("Start Menu").join("Programs"))
}
/// The shortcuts for the chosen language: Start menu and desktop.
fn shortcuts() -> Vec<PathBuf> {
    let name = format!("{}.lnk", brand());
    [start_menu_dir(), dirs::desktop_dir()].into_iter().flatten().map(|d| d.join(&name)).collect()
}
/// Every name a shortcut ever had (the app was called Kesit before), so none is left behind.
fn shortcuts_all() -> Vec<PathBuf> {
    let mut v = vec![];
    for d in [start_menu_dir(), dirs::desktop_dir()].into_iter().flatten() {
        for n in ["Kesit.lnk", "Linnea.lnk", "Linnea Kesit.lnk"] {
            v.push(d.join(n));
        }
    }
    v
}

fn stop_running() {
    for exe in ["kesit-app.exe", "kesit.exe"] {
        let _ = quiet(Command::new("taskkill").args(["/F", "/IM", exe])).output();
    }
    std::thread::sleep(std::time::Duration::from_millis(600));
}

/// Claude Code's command, if it is on this computer.
fn find_claude() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&path) {
            for n in ["claude.exe", "claude.cmd"] {
                if d.join(n).is_file() {
                    return Some(d.join(n));
                }
            }
        }
    }
    // The Claude desktop app keeps its own copy: <LOCALAPPDATA>\Claude-Data\*\claude-code\<version>\<id>\claude.exe
    let mut found: Vec<PathBuf> = vec![];
    let base = dirs::data_local_dir()?.join("Claude-Data");
    for a in std::fs::read_dir(base).ok()?.flatten() {
        let cc = a.path().join("claude-code");
        for v in std::fs::read_dir(cc).into_iter().flatten().flatten() {
            for i in std::fs::read_dir(v.path()).into_iter().flatten().flatten() {
                let p = i.path().join("claude.exe");
                if p.is_file() {
                    found.push(p);
                }
            }
        }
    }
    found.sort_by_key(|p| p.metadata().and_then(|m| m.modified()).ok());
    found.pop()
}

fn install() -> Result<(), String> {
    let dir = install_dir();
    let mb = PAYLOAD.len() / 1_000_000;
    let b = brand();
    let text = if turkish() {
        format!("{b} {VERSION} bu kullanıcı için kurulacak.\n\nKonum: {}\nBoyut: yaklaşık {} MB\n\nYönetici izni gerekmez. Devam edilsin mi?", dir.display(), mb * 2)
    } else {
        format!("{b} {VERSION} will be installed for this user.\n\nLocation: {}\nSize: about {} MB\n\nNo administrator rights are needed. Continue?", dir.display(), mb * 2)
    };
    if !ask("install", &format!("{b} {}", l("setup", "kurulumu")), &text) {
        return Ok(());
    }
    stop_running();
    // Remove the old program files, keep nothing of the user's (their work lives in Videos\Kesit and AppData).
    for sub in ["bin", "library", "ui", "skill"] {
        let _ = std::fs::remove_dir_all(dir.join(sub));
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("{} {}: {e}", dir.display(), l("could not be created", "oluşturulamadı")))?;
    let mut zip = zip::ZipArchive::new(Cursor::new(PAYLOAD)).map_err(|e| format!("{}: {e}", l("the package could not be read", "paket okunamadı")))?;
    zip.extract(&dir).map_err(|e| format!("{}: {e}", l("the files could not be unpacked", "dosyalar açılamadı")))?;
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let setup = dir.join("Kesit-Setup.exe");
    if me != setup {
        let _ = std::fs::copy(&me, &setup);
    }
    let app = dir.join("bin").join("kesit-app.exe");
    let icon = dir.join("kesit.ico");
    for old in shortcuts_all() {
        let _ = std::fs::remove_file(old);
    }
    for lnk in shortcuts() {
        ps(&format!(
            "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{}'); $s.TargetPath='{}'; $s.WorkingDirectory='{}'; $s.IconLocation='{},0'; $s.Description='{}'; $s.Save()",
            psq(&lnk), psq(&app), psq(&dir), psq(&icon), b
        ));
    }
    let size_kb: u64 = walk_size(&dir) / 1024;
    let reg = |name: &str, kind: &str, val: &str| {
        let _ = quiet(Command::new("reg").args(["add", UNINSTALL_KEY, "/v", name, "/t", kind, "/d", val, "/f"])).output();
    };
    reg("DisplayName", "REG_SZ", b);
    reg("DisplayVersion", "REG_SZ", VERSION);
    reg("Publisher", "REG_SZ", "Linnea (open source)");
    reg("InstallLocation", "REG_SZ", &dir.to_string_lossy());
    reg("DisplayIcon", "REG_SZ", &icon.to_string_lossy());
    reg("UninstallString", "REG_SZ", &format!("\"{}\" --uninstall", setup.display()));
    reg("NoModify", "REG_DWORD", "1");
    reg("NoRepair", "REG_DWORD", "1");
    reg("EstimatedSize", "REG_DWORD", &size_kb.to_string());

    let bin = dir.join("bin");
    if ask("path", l("Terminal command", "Terminal komutu"), &if turkish() { format!("Terminalden `kesit` komutunu kullanabilmek için {b} bin klasörü kullanıcı PATH'ine eklensin mi?") } else { format!("Add the {b} bin folder to your user PATH so the `kesit` command works in a terminal?") }) {
        ps(&format!(
            "$p=[Environment]::GetEnvironmentVariable('Path','User'); $b='{}'; if (-not (($p -split ';') -contains $b)) {{ [Environment]::SetEnvironmentVariable('Path', ($(if ($p) {{ $p.TrimEnd(';') + ';' }} else {{ '' }}) + $b), 'User') }}",
            psq(&bin)
        ));
    }
    let mut mcp = String::new();
    if let Some(claude) = find_claude() {
        if ask("claude", "Claude Code", &if turkish() { format!("Claude Code bu bilgisayarda bulundu. {b}, Claude Code'a MCP sunucusu olarak bağlansın ve skill'i kurulsun mu?\n\nBöylece Claude filmleri ve reels'leri {b} araçlarıyla yapar.") } else { format!("Claude Code is on this computer. Connect {b} to Claude Code as an MCP server and install its skill?\n\nClaude then makes films and reels with the {b} tools.") }) {
            let kesit = bin.join("kesit.exe");
            let _ = quiet(Command::new(&claude).args(["mcp", "remove", "kesit", "-s", "user"])).output();
            let ok = quiet(Command::new(&claude).args(["mcp", "add", "--scope", "user", "kesit", "--"]).arg(&kesit).arg("mcp")).output().map(|o| o.status.success()).unwrap_or(false);
            if let Some(home) = dirs::home_dir() {
                let sk = home.join(".claude").join("skills").join("kesit");
                let _ = std::fs::create_dir_all(&sk);
                let _ = std::fs::copy(dir.join("skill").join("kesit").join("SKILL.md"), sk.join("SKILL.md"));
            }
            mcp = if ok { l("\nConnected to Claude Code (it shows in a new Claude Code session).", "\nClaude Code'a bağlandı (yeni bir Claude Code oturumunda görünür).").into() } else { l("\nThe Claude Code connection could not be made; use the command in Settings.", "\nClaude Code bağlantısı kurulamadı; Ayarlar ekranındaki komutu kullanın.").into() };
        }
    }
    let b = brand();
    if ask(
        "done",
        &format!("{b} {}", l("is installed", "kuruldu")),
        &if turkish() {
            format!("{b} {VERSION} kuruldu.{mcp}\n\nİlk açılışta Ayarlar > Araçlar > \"Eksikleri indir\" ile ffmpeg, yt-dlp, whisper ve modelleri indirebilirsiniz (yaklaşık 2,5 GB).\n\n{b} şimdi açılsın mı?")
        } else {
            format!("{b} {VERSION} is installed.{mcp}\n\nOn first start, Settings > Tools > \"Download missing\" fetches ffmpeg, yt-dlp, whisper and the models (about 2.5 GB).\n\nOpen {b} now?")
        },
    ) {
        let _ = Command::new(&app).current_dir(&dir).spawn();
    }
    Ok(())
}

fn walk_size(p: &Path) -> u64 {
    std::fs::read_dir(p).map(|rd| rd.flatten().map(|e| if e.path().is_dir() { walk_size(&e.path()) } else { e.metadata().map(|m| m.len()).unwrap_or(0) }).sum()).unwrap_or(0)
}

fn uninstall() -> Result<(), String> {
    let dir = install_dir();
    TURKISH.store(read_config()["ui"]["lang"].as_str() == Some("tr"), std::sync::atomic::Ordering::Relaxed);
    let b = brand();
    let text = if turkish() {
        format!("{b} bu bilgisayardan kaldırılsın mı?\n\nProgram dosyaları silinir: {}\nFilmleriniz, projeleriniz ve ayarlarınız (Videolar\\Kesit, AppData\\Kesit) yerinde kalır.", dir.display())
    } else {
        format!("Remove {b} from this computer?\n\nThe program files are deleted: {}\nYour films, projects and settings (Videos\\Kesit, AppData\\Kesit) stay where they are.", dir.display())
    };
    if !ask("uninstall", &format!("{} {b}", l("Remove", "Kaldır:")), &text) {
        return Ok(());
    }
    stop_running();
    for lnk in shortcuts_all() {
        let _ = std::fs::remove_file(lnk);
    }
    let _ = quiet(Command::new("reg").args(["delete", UNINSTALL_KEY, "/f"])).output();
    let bin = dir.join("bin");
    ps(&format!("$p=[Environment]::GetEnvironmentVariable('Path','User'); if ($p) {{ [Environment]::SetEnvironmentVariable('Path', (($p -split ';') | Where-Object {{ $_ -and $_ -ne '{}' }}) -join ';', 'User') }}", psq(&bin)));
    if let Some(claude) = find_claude() {
        let _ = quiet(Command::new(&claude).args(["mcp", "remove", "kesit", "-s", "user"])).output();
    }
    // This exe lives in the folder it removes: let a short-lived shell finish the job after we exit.
    let _ = quiet(Command::new("cmd").args(["/C", &format!("ping 127.0.0.1 -n 3 > nul & rmdir /S /Q \"{}\"", dir.display())])).spawn();
    tell(&format!("{b} {}", l("was removed.", "kaldırıldı.")), false);
    Ok(())
}

fn main() {
    std::panic::set_hook(Box::new(|i| log(&format!("panic: {i}"))));
    SILENT.store(std::env::args().any(|a| a == "--silent" || a == "/S"), std::sync::atomic::Ordering::Relaxed);
    let un = std::env::args().any(|a| a == "--uninstall" || a == "/uninstall");
    let r = if un {
        uninstall()
    } else {
        choose_language();
        install()
    };
    if let Err(e) = r {
        tell(&format!("{}\n{e}", l("The installation could not finish:", "Kurulum tamamlanamadı:")), true);
        std::process::exit(1);
    }
}
