//! Native backend policy tests use only windows created and kept hidden by this test.
#![cfg(target_os = "windows")]
use raw_window_handle::RawWindowHandle;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows};
use zgui::platform::{
    AppHandler, Decorations, PlatformCx, SurfaceAttributes, SurfaceEvent, SurfaceId, WakeReason,
    WindowLevel,
};
use zgui_platform_winit::{UserEvent, WinitApp};

struct CheckWindows(bool);
impl AppHandler for CheckWindows {
    fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
        let mut attributes = SurfaceAttributes::new("OkColorPicker native window test");
        attributes.transparent = true;
        attributes.decorations = Decorations::NoTitleBar;
        let surface = cx.create_surface(&attributes).unwrap();
        let gpu = surface.clone().gpu_shared().unwrap();
        let handle = gpu.window_handle().unwrap();
        let RawWindowHandle::Win32(native) = handle.as_raw() else {
            panic!("Not a Win32 window")
        };
        let hwnd = native.hwnd.get() as _;
        let verify = |resizable: bool| {
            // SAFETY: the surface and borrowed handle retain this test-owned HWND.
            unsafe {
                let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
                assert_ne!(ex & WS_EX_NOREDIRECTIONBITMAP, 0);
                assert_eq!(ex & WS_EX_LAYERED, 0);
                assert_eq!(IsWindowVisible(hwnd), 0);
                let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
                assert_eq!(style & WS_THICKFRAME != 0, resizable);
                assert_eq!(
                    style & (WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX),
                    WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX
                );
            }
        };
        verify(true);
        // These calls force winit to rebuild native styles from its own state.
        surface.set_resizable(false);
        verify(false);
        surface.set_resizable(true);
        verify(true);
        surface.set_window_level(WindowLevel::AlwaysOnTop);
        verify(true);
        surface.set_window_level(WindowLevel::Normal);
        surface.set_decorations(Decorations::Full);
        verify(true);
        surface.set_decorations(Decorations::NoTitleBar);
        verify(true);
        self.0 = true;
        cx.destroy_surface(surface.id());
        cx.request_exit();
    }

    fn surface_event(&mut self, _: &dyn PlatformCx, _: SurfaceId, _: SurfaceEvent) {}
    fn wake(&mut self, _: &dyn PlatformCx, _: WakeReason) {}
}

#[test]
fn composition_style_is_created_and_retained_by_the_native_backend() {
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .with_any_thread(true)
        .build()
        .unwrap();
    let mut app = WinitApp::new(&event_loop, CheckWindows(false));
    event_loop.run_app(&mut app).unwrap();
    assert!(
        app.handler().0,
        "The native backend did not create a window"
    );
}
