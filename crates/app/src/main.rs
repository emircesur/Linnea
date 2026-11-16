//! The Linnea window: the local server in this process and a webview pointed at it.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::WindowBuilder;
use wry::WebViewBuilder;

fn main() {
    let port = match kesit_core::server::start(None) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Kesit cannot start: {e}");
            std::process::exit(1);
        }
    };
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new().with_title(if kesit_core::config::Config::load().str("ui.lang") == "tr" { "Linnea Kesit" } else { "Linnea" }).with_inner_size(LogicalSize::new(1440.0, 900.0)).with_min_inner_size(LogicalSize::new(1100.0, 700.0)).build(&event_loop).expect("window");
    let _webview = WebViewBuilder::new().with_url(format!("http://127.0.0.1:{port}/")).with_devtools(cfg!(debug_assertions)).build(&window).expect("webview");
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent { event: WindowEvent::CloseRequested, .. } = event {
            *control_flow = ControlFlow::Exit;
        }
    });
}
