//! The Linnea window: the local server in this process and a webview pointed at it.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::WindowBuilder;
use wry::WebViewBuilder;

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
    let _webview = WebViewBuilder::new()
        .with_url(format!("http://127.0.0.1:{port}/"))
        .with_devtools(cfg!(debug_assertions))
        // a link that wants a new window (release notes, a sign-in page) opens in the person's own browser
        .with_new_window_req_handler(|url, _| {
            open_in_browser(&url);
            wry::NewWindowResponse::Deny
        })
        .build(&window)
        .expect("webview");
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent { event: WindowEvent::CloseRequested, .. } = event {
            *control_flow = ControlFlow::Exit;
        }
    });
}
