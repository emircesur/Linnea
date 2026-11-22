//! The Linnea window: the local server in this process and a webview pointed at it.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::WindowBuilder;
use wry::{WebContext, WebViewBuilder};

fn open_in_browser(url: &str) {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return;
    }
    #[cfg(windows)]
    let _ = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

/// Where the window's browser keeps what the page remembers (layout, language, presets). On Windows it lives
/// with Linnea's data, not next to the program, so an update (which replaces the program folder) keeps it; a
/// profile left next to the program by an older version moves over once.
fn profile_dir() -> Option<std::path::PathBuf> {
    #[cfg(windows)]
    {
        let dir = linnea_core::config::data_dir().join("webview");
        if !dir.exists() {
            if let Ok(exe) = std::env::current_exe() {
                let old = exe.with_file_name(format!("{}.WebView2", exe.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
                if old.is_dir() {
                    let _ = std::fs::create_dir_all(dir.parent().unwrap_or(&dir));
                    let _ = std::fs::rename(&old, &dir);
                }
            }
        }
        let _ = std::fs::create_dir_all(&dir);
        Some(dir)
    }
    #[cfg(not(windows))]
    None
}

fn main() {
    let port = match linnea_core::server::start(None) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Linnea cannot start: {e}");
            std::process::exit(1);
        }
    };
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new().with_title(if linnea_core::config::Config::load().str("ui.lang") == "tr" { "Linnea Kesit" } else { "Linnea" }).with_inner_size(LogicalSize::new(1440.0, 900.0)).with_min_inner_size(LogicalSize::new(1100.0, 700.0)).build(&event_loop).expect("window");
    let mut context = WebContext::new(profile_dir());
    // Right after an update the web view processes of the Linnea that closed may still be on their way out and
    // hold the profile: try again for a few seconds instead of closing at once.
    let mut webview = None;
    let mut why = String::new();
    for _ in 0..8 {
        match WebViewBuilder::new_with_web_context(&mut context)
            .with_url(format!("http://127.0.0.1:{port}/"))
            .with_devtools(cfg!(debug_assertions))
            // a link that wants a new window (release notes, a sign-in page) opens in the person's own browser
            .with_new_window_req_handler(|url, _| {
                open_in_browser(&url);
                wry::NewWindowResponse::Deny
            })
            .build(&window)
        {
            Ok(w) => {
                webview = Some(w);
                break;
            }
            Err(e) => {
                why = e.to_string();
                std::thread::sleep(std::time::Duration::from_millis(750));
            }
        }
    }
    let Some(_webview) = webview else {
        eprintln!("Linnea's window cannot show its page: {why}");
        std::process::exit(1);
    };
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent { event: WindowEvent::CloseRequested, .. } = event {
            *control_flow = ControlFlow::Exit;
        }
    });
}
