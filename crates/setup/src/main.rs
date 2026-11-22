//! Linnea-Setup.exe: installs Linnea (Linnea Kesit in Turkish) for the current user (no admin rights), or removes it with --uninstall.
//! The program files ride inside this exe (dist/payload.zip, made by pack.py).
#![windows_subsystem = "windows"]
use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;

static PAYLOAD: &[u8] = include_bytes!("../../../dist/payload.zip");
const VERSION: &str = env!("CARGO_PKG_VERSION");
const UNINSTALL_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Linnea";
/// The app was called Kesit before: its program folder, uninstall entry, commands and MCP name are cleaned up.
const OLD_UNINSTALL_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Kesit";

fn quiet(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}
static SILENT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// `--update`: started by the app's own updater. Silent, and the app starts again at the end.
static UPDATE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// `--wait=<pid>`: the app that started the update; it closes itself, and the files are replaced once it has.
static WAIT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
fn log(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(std::env::temp_dir().join("linnea-setup.log")) {
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
    if let Ok(h) = std::env::var("LINNEA_HOME").or_else(|_| std::env::var("KESIT_HOME")) {
        return PathBuf::from(h).join("config.json");
    }
    // the settings an earlier version kept in AppData\Kesit move with the app (as the app itself does)
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    let (old, new) = (base.join("Kesit"), base.join("Linnea"));
    if !new.exists() && old.is_dir() && std::fs::rename(&old, &new).is_err() {
        return old.join("config.json");
    }
    new.join("config.json")
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
    dirs::data_local_dir().unwrap_or_else(|| PathBuf::from(".")).join("Programs").join("Linnea")
}
fn old_install_dir() -> PathBuf {
    dirs::data_local_dir().unwrap_or_else(|| PathBuf::from(".")).join("Programs").join("Kesit")
}
/// Take a bin folder off the user PATH.
fn path_remove(bin: &Path) {
    ps(&format!("$p=[Environment]::GetEnvironmentVariable('Path','User'); if ($p) {{ [Environment]::SetEnvironmentVariable('Path', (($p -split ';') | Where-Object {{ $_ -and $_ -ne '{}' }}) -join ';', 'User') }}", psq(bin)));
}
/// What an install under the old name left: its program files (never the user's work), PATH entry and uninstall entry.
fn remove_old_install() {
    let old = old_install_dir();
    if old.is_dir() {
        let _ = std::fs::remove_dir_all(&old);
        path_remove(&old.join("bin"));
    }
    let _ = quiet(Command::new("reg").args(["delete", OLD_UNINSTALL_KEY, "/f"])).output();
}
fn start_menu_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("Microsoft").join("Windows").join("Start Menu").join("Programs"))
}
/// The shortcuts for the chosen language: Start menu and desktop.
fn shortcuts() -> Vec<PathBuf> {
    let name = format!("{}.lnk", brand());
    [start_menu_dir(), dirs::desktop_dir()].into_iter().flatten().map(|d| d.join(&name)).collect()
}
/// Every name a shortcut ever had (the app was called Linnea before), so none is left behind.
fn shortcuts_all() -> Vec<PathBuf> {
    let mut v = vec![];
    for d in [start_menu_dir(), dirs::desktop_dir()].into_iter().flatten() {
        for n in ["Kesit.lnk", "Linnea.lnk", "Linnea Kesit.lnk"] {
            v.push(d.join(n));
        }
    }
    v
}

const EXES: [&str; 4] = ["linnea-app.exe", "linnea.exe", "kesit-app.exe", "kesit.exe"];
fn tasklist(filter: &str) -> String {
    quiet(Command::new("tasklist").args(["/FI", filter, "/NH", "/FO", "CSV"])).output().map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase()).unwrap_or_default()
}
fn pid_alive(pid: u32) -> bool {
    pid > 0 && tasklist(&format!("PID eq {pid}")).contains(&format!("\"{pid}\""))
}
fn any_running() -> bool {
    EXES.iter().any(|e| tasklist(&format!("IMAGENAME eq {e}")).contains(e))
}
fn sleep_ms(ms: u64) {
    std::thread::sleep(std::time::Duration::from_millis(ms));
}
/// The app that started an update closes itself: wait for it (up to 20 s) before anything is replaced.
fn wait_for_app() {
    let pid = WAIT.load(std::sync::atomic::Ordering::Relaxed);
    if pid == 0 {
        return;
    }
    for _ in 0..80 {
        if !pid_alive(pid) {
            log(&format!("app {pid} has closed"));
            return;
        }
        sleep_ms(250);
    }
    log(&format!("app {pid} is still running after 20 s"));
}
/// Stop every Linnea program, and the web view processes of its window, and wait until they are really gone:
/// a program that was just killed can hold its files for a moment, and replacing them then fails.
fn stop_running() {
    for exe in EXES {
        let _ = quiet(Command::new("taskkill").args(["/F", "/IM", exe])).output();
    }
    ps("Get-CimInstance Win32_Process -Filter \"Name='msedgewebview2.exe'\" | Where-Object { $_.CommandLine -match 'linnea' } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }");
    for _ in 0..40 {
        if !any_running() {
            break;
        }
        sleep_ms(250);
    }
    sleep_ms(500);
}
/// A file that cannot be deleted (still open somewhere) can still be renamed: move it out of the way.
fn set_aside(p: &Path) {
    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    for n in 0..50 {
        let q = p.with_file_name(format!("{name}.old{n}"));
        if !q.exists() {
            if let Err(e) = std::fs::rename(p, &q) {
                log(&format!("could not move {} aside: {e}", p.display()));
            }
            return;
        }
    }
}
/// Empty a program folder, keeping the window's browser profile (`*.WebView2`: what the app remembers).
fn clear_program_dir(d: &Path) {
    let Ok(rd) = std::fs::read_dir(d) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if e.file_name().to_string_lossy().to_lowercase().ends_with(".webview2") {
            continue;
        }
        if p.is_dir() {
            if std::fs::remove_dir_all(&p).is_err() {
                clear_program_dir(&p);
            }
        } else if std::fs::remove_file(&p).is_err() {
            set_aside(&p);
        }
    }
}
/// The program files, written one by one: a file that is still held is moved aside and written anew.
fn unpack(dir: &Path) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(PAYLOAD)).map_err(|e| format!("{}: {e}", l("the package could not be read", "paket okunamadı")))?;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = f.enclosed_name() else { continue };
        let out = dir.join(rel);
        if f.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
            continue;
        }
        if let Some(p) = out.parent() {
            std::fs::create_dir_all(p).map_err(|e| format!("{}: {e}", p.display()))?;
        }
        if out.exists() && std::fs::remove_file(&out).is_err() {
            set_aside(&out);
        }
        let mut w = std::fs::File::create(&out).map_err(|e| format!("{}: {e}", out.display()))?;
        std::io::copy(&mut f, &mut w).map_err(|e| format!("{}: {e}", out.display()))?;
    }
    Ok(())
}
/// Leftovers moved aside by an earlier install, once nothing holds them any more.
fn sweep_old(d: &Path) {
    let Ok(rd) = std::fs::read_dir(d) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if !e.file_name().to_string_lossy().to_lowercase().ends_with(".webview2") {
                sweep_old(&p);
            }
        } else if e.file_name().to_string_lossy().contains(".old") && e.file_name().to_string_lossy().rsplit(".old").next().map(|n| n.chars().all(|c| c.is_ascii_digit())).unwrap_or(false) {
            let _ = std::fs::remove_file(&p);
        }
    }
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
    wait_for_app();
    stop_running();
    remove_old_install();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{} {}: {e}", dir.display(), l("could not be created", "oluşturulamadı")))?;
    // Replace the program files (the user's work lives in Videos\Linnea and AppData, and the window's browser
    // profile is kept). A file something still holds is moved aside; a failed round stops Linnea again and
    // tries once more, five times in all.
    let mut last = String::new();
    let mut done = false;
    for round in 0..5 {
        for sub in ["bin", "library", "ui", "skill"] {
            clear_program_dir(&dir.join(sub));
        }
        match unpack(&dir) {
            Ok(()) => {
                done = true;
                break;
            }
            Err(e) => {
                log(&format!("unpacking, round {round}: {e}"));
                last = e;
                stop_running();
                sleep_ms(1000);
            }
        }
    }
    if !done {
        return Err(format!("{}: {last}", l("the files could not be unpacked", "dosyalar açılamadı")));
    }
    for sub in ["bin", "library", "ui", "skill"] {
        sweep_old(&dir.join(sub));
    }
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let setup = dir.join("Linnea-Setup.exe");
    if me != setup {
        let _ = std::fs::copy(&me, &setup);
    }
    let app = dir.join("bin").join("linnea-app.exe");
    let icon = dir.join("linnea.ico");
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
    if ask("path", l("Terminal command", "Terminal komutu"), &if turkish() { format!("Terminalden `linnea` komutunu kullanabilmek için {b} bin klasörü kullanıcı PATH'ine eklensin mi?") } else { format!("Add the {b} bin folder to your user PATH so the `linnea` command works in a terminal?") }) {
        ps(&format!(
            "$p=[Environment]::GetEnvironmentVariable('Path','User'); $b='{}'; if (-not (($p -split ';') -contains $b)) {{ [Environment]::SetEnvironmentVariable('Path', ($(if ($p) {{ $p.TrimEnd(';') + ';' }} else {{ '' }}) + $b), 'User') }}",
            psq(&bin)
        ));
    }
    let mut mcp = String::new();
    if let Some(claude) = find_claude() {
        if ask("claude", "Claude Code", &if turkish() { format!("Claude Code bu bilgisayarda bulundu. {b}, Claude Code'a MCP sunucusu olarak bağlansın ve skill'i kurulsun mu?\n\nBöylece Claude filmleri ve reels'leri {b} araçlarıyla yapar.") } else { format!("Claude Code is on this computer. Connect {b} to Claude Code as an MCP server and install its skill?\n\nClaude then makes films and reels with the {b} tools.") }) {
            let linnea = bin.join("linnea.exe");
            for name in ["linnea", "kesit"] {
                let _ = quiet(Command::new(&claude).args(["mcp", "remove", name, "-s", "user"])).output();
            }
            let ok = quiet(Command::new(&claude).args(["mcp", "add", "--scope", "user", "linnea", "--"]).arg(&linnea).arg("mcp")).output().map(|o| o.status.success()).unwrap_or(false);
            if let Some(home) = dirs::home_dir() {
                let _ = std::fs::remove_dir_all(home.join(".claude").join("skills").join("kesit"));
                let sk = home.join(".claude").join("skills").join("linnea");
                let _ = std::fs::create_dir_all(&sk);
                let _ = std::fs::copy(dir.join("skill").join("linnea").join("SKILL.md"), sk.join("SKILL.md"));
            }
            mcp = if ok { l("\nConnected to Claude Code (it shows in a new Claude Code session).", "\nClaude Code'a bağlandı (yeni bir Claude Code oturumunda görünür).").into() } else { l("\nThe Claude Code connection could not be made; use the command in Settings.", "\nClaude Code bağlantısı kurulamadı; Ayarlar ekranındaki komutu kullanın.").into() };
        }
    }
    let b = brand();
    if UPDATE.load(std::sync::atomic::Ordering::Relaxed) {
        // an update from inside the app: the skill Claude Code reads follows the new version, and the app
        // that started this comes back by itself
        if let Some(sk) = dirs::home_dir().map(|h| h.join(".claude").join("skills").join("linnea")).filter(|d| d.is_dir()) {
            let _ = std::fs::copy(dir.join("skill").join("linnea").join("SKILL.md"), sk.join("SKILL.md"));
        }
        start_app(&app, &dir);
        return Ok(());
    }
    if ask(
        "done",
        &format!("{b} {}", l("is installed", "kuruldu")),
        &if turkish() {
            format!("{b} {VERSION} kuruldu.{mcp}\n\nİlk açılışta Ayarlar > Araçlar > \"Eksikleri indir\" ile ffmpeg, yt-dlp, whisper ve modelleri indirebilirsiniz (yaklaşık 2,5 GB).\n\n{b} şimdi açılsın mı?")
        } else {
            format!("{b} {VERSION} is installed.{mcp}\n\nOn first start, Settings > Tools > \"Download missing\" fetches ffmpeg, yt-dlp, whisper and the models (about 2.5 GB).\n\nOpen {b} now?")
        },
    ) {
        start_app(&app, &dir);
    }
    Ok(())
}

/// Start the app once, on its own (not a child that goes when this installer ends).
fn start_app(app: &Path, dir: &Path) {
    let mut c = Command::new(app);
    c.current_dir(dir);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0000_0008 | 0x0000_0200);
    }
    match c.spawn() {
        Ok(_) => log("app started"),
        Err(e) => log(&format!("the app could not be started: {e}")),
    }
}

fn walk_size(p: &Path) -> u64 {
    std::fs::read_dir(p).map(|rd| rd.flatten().map(|e| if e.path().is_dir() { walk_size(&e.path()) } else { e.metadata().map(|m| m.len()).unwrap_or(0) }).sum()).unwrap_or(0)
}

fn uninstall() -> Result<(), String> {
    let dir = install_dir();
    TURKISH.store(read_config()["ui"]["lang"].as_str() == Some("tr"), std::sync::atomic::Ordering::Relaxed);
    let b = brand();
    let text = if turkish() {
        format!("{b} bu bilgisayardan kaldırılsın mı?\n\nProgram dosyaları silinir: {}\nFilmleriniz, projeleriniz ve ayarlarınız (Videolar\\Linnea, AppData\\Linnea) yerinde kalır.", dir.display())
    } else {
        format!("Remove {b} from this computer?\n\nThe program files are deleted: {}\nYour films, projects and settings (Videos\\Linnea, AppData\\Linnea) stay where they are.", dir.display())
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
    path_remove(&bin);
    if let Some(claude) = find_claude() {
        for name in ["linnea", "kesit"] {
            let _ = quiet(Command::new(&claude).args(["mcp", "remove", name, "-s", "user"])).output();
        }
    }
    // This exe lives in the folder it removes: let a short-lived shell finish the job after we exit.
    let _ = quiet(Command::new("cmd").args(["/C", &format!("ping 127.0.0.1 -n 3 > nul & rmdir /S /Q \"{}\"", dir.display())])).spawn();
    tell(&format!("{b} {}", l("was removed.", "kaldırıldı.")), false);
    Ok(())
}

fn main() {
    std::panic::set_hook(Box::new(|i| log(&format!("panic: {i}"))));
    let update = std::env::args().any(|a| a == "--update");
    UPDATE.store(update, std::sync::atomic::Ordering::Relaxed);
    if let Some(pid) = std::env::args().find_map(|a| a.strip_prefix("--wait=").and_then(|p| p.parse::<u32>().ok())) {
        WAIT.store(pid, std::sync::atomic::Ordering::Relaxed);
    }
    log(&format!("Linnea-Setup {VERSION} {:?}", std::env::args().skip(1).collect::<Vec<_>>()));
    SILENT.store(update || std::env::args().any(|a| a == "--silent" || a == "/S"), std::sync::atomic::Ordering::Relaxed);
    let un = std::env::args().any(|a| a == "--uninstall" || a == "/uninstall");
    let r = if un {
        uninstall()
    } else {
        choose_language();
        install()
    };
    if let Err(e) = r {
        if update {
            // an update the person started from the app: say what went wrong (it is not silent any more), and
            // bring Linnea back as it is
            SILENT.store(false, std::sync::atomic::Ordering::Relaxed);
            let log_file = std::env::temp_dir().join("linnea-setup.log");
            tell(&format!("{}\n{e}\n\n{}\n{}", l("The update could not finish:", "Güncelleme tamamlanamadı:"), l("Details:", "Ayrıntılar:"), log_file.display()), true);
            let dir = install_dir();
            let app = dir.join("bin").join("linnea-app.exe");
            if app.is_file() {
                start_app(&app, &dir);
            }
        } else {
            tell(&format!("{}\n{e}", l("The installation could not finish:", "Kurulum tamamlanamadı:")), true);
        }
        std::process::exit(1);
    }
}
