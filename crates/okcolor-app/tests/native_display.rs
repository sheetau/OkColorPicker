//! Read the display configuration for a private hidden window, without changing OS settings.
#![cfg(target_os = "windows")]
use okcolor_app::windows_display::{ColorMode, query};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    platform::windows::EventLoopBuilderExtWindows,
    window::{Window, WindowId},
};

#[derive(Default)]
struct CheckDisplay(bool);
impl ApplicationHandler for CheckDisplay {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("OkColorPicker display query test")
                    .with_visible(false),
            )
            .unwrap();
        let display = query(&window).expect("The local display configuration must be readable");
        eprintln!("Display query: {display:?}");
        assert!(display.bits_per_channel > 0 && display.bits_per_channel <= 32);
        match display.mode {
            ColorMode::Sdr | ColorMode::WideColor => assert_eq!(display.white_scale(), Some(1.0)),
            ColorMode::Hdr => assert!(
                display
                    .white_scale()
                    .is_some_and(|v| v > 0.0 && v.is_finite())
            ),
            ColorMode::AdvancedUnknown => assert_eq!(display.white_scale(), None),
        }
        self.0 = true;
        event_loop.exit();
    }
    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

#[test]
#[ignore = "requires a local Windows display configuration; remote sessions may deny access"]
fn current_monitor_capabilities_are_read_without_changing_display_state() {
    let event_loop = EventLoop::builder().with_any_thread(true).build().unwrap();
    let mut app = CheckDisplay::default();
    event_loop.run_app(&mut app).unwrap();
    assert!(app.0);
}
