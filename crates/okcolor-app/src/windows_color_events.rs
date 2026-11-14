//! Desktop AdvancedColorInfoChanged subscription (Windows 11 build 22621+).
use std::{ffi::c_void, marker::PhantomData, rc::Rc};
use windows::{
    Foundation::TypedEventHandler,
    Graphics::Display::DisplayInformation,
    System::DispatcherQueue,
    Win32::Foundation::HWND,
    core::{GUID, HRESULT, IInspectable, IInspectable_Vtbl, Interface, Result, factory},
};

// This desktop interop interface is absent from windows 0.62.2 metadata.
// ABI and IID: Windows SDK 10.0.26100.0, windows.graphics.display.interop.h.
#[repr(transparent)]
#[derive(Clone)]
struct DisplayInformationInterop(IInspectable);

#[repr(C)]
struct InteropVtable {
    base: IInspectable_Vtbl,
    get_for_window:
        unsafe extern "system" fn(*mut c_void, HWND, *const GUID, *mut *mut c_void) -> HRESULT,
    get_for_monitor: unsafe extern "system" fn(
        *mut c_void,
        *mut c_void,
        *const GUID,
        *mut *mut c_void,
    ) -> HRESULT,
}

// SAFETY: transparent COM pointer; vtable and IID match the SDK interface.
unsafe impl Interface for DisplayInformationInterop {
    type Vtable = InteropVtable;
    const IID: GUID = GUID::from_u128(0x7449121c_382b_4705_8da7_a795ba482013);
}

pub(crate) struct ColorEvents {
    information: DisplayInformation,
    token: i64,
    _thread: PhantomData<Rc<()>>,
}

impl ColorEvents {
    /// The caller retains its own top-level HWND on the current UI thread.
    pub unsafe fn new(hwnd: HWND, invalidate: impl Fn() + Send + 'static) -> Result<Self> {
        // Fail cleanly when no runtime scope exists rather than creating a queue
        // whose shutdown would reenter a window callback during renderer drop.
        DispatcherQueue::GetForCurrentThread()?;
        let interop = factory::<DisplayInformation, DisplayInformationInterop>()?;
        let mut raw = std::ptr::null_mut();
        // SAFETY: caller retains the HWND; output IID is DisplayInformation's
        // default interface. from_abi takes ownership of the returned reference.
        let information: DisplayInformation = unsafe {
            (interop.vtable().get_for_window)(
                interop.as_raw(),
                hwnd,
                &DisplayInformation::IID,
                &mut raw,
            )
            .ok()?;
            windows::core::Type::from_abi(raw)?
        };
        let handler = TypedEventHandler::new(move |_, _| {
            invalidate();
            Ok(())
        });
        let token = information.AdvancedColorInfoChanged(&handler)?;
        Ok(Self {
            information,
            token,
            _thread: PhantomData,
        })
    }
}

impl Drop for ColorEvents {
    fn drop(&mut self) {
        if let Err(error) = self.information.RemoveAdvancedColorInfoChanged(self.token) {
            eprintln!("Cannot revoke Advanced Color notification: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::windows_dispatcher::DispatcherRuntime;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use windows::System::DispatcherQueueHandler;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    struct TestWindow(HWND);
    impl TestWindow {
        fn new() -> Self {
            // SAFETY: create a hidden top-level window owned only by this test.
            let hwnd = unsafe {
                CreateWindowExW(
                    0,
                    windows_sys::w!("STATIC"),
                    windows_sys::w!("Display events test"),
                    WS_OVERLAPPEDWINDOW,
                    0,
                    0,
                    330,
                    560,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            };
            assert!(!hwnd.is_null(), "{}", std::io::Error::last_os_error());
            Self(HWND(hwnd))
        }
    }
    impl Drop for TestWindow {
        fn drop(&mut self) {
            // SAFETY: own HWND on the creating thread, after subscription removal.
            unsafe { DestroyWindow(self.0.0) };
        }
    }

    #[test]
    #[ignore = "requires Windows 11 build 22621+ and a local display"]
    fn native_subscription_revokes_and_dispatcher_drains_without_leaking_or_swallowing_quit() {
        for _ in 0..3 {
            let runtime = DispatcherRuntime::new().unwrap();
            let queue = DispatcherQueue::GetForCurrentThread().unwrap();
            // Nested scopes must not shut down a queue owned by the outer scope.
            drop(DispatcherRuntime::new().unwrap());
            let window = TestWindow::new();
            let calls = Arc::new(AtomicUsize::new(0));
            let retained = calls.clone();
            // SAFETY: our own top-level window remains alive through revocation.
            let events = unsafe {
                ColorEvents::new(window.0, move || {
                    retained.fetch_add(1, Ordering::Relaxed);
                })
            }
            .expect("Desktop Advanced Color registration must succeed");
            let info = events.information.GetAdvancedColorInfo().unwrap();
            let white = info.SdrWhiteLevelInNits().unwrap();
            assert!(white.is_finite() && white > 0.0);
            eprintln!(
                "Native color events: kind={:?}, white={white}",
                info.CurrentAdvancedColorKind().unwrap()
            );
            assert!(Arc::strong_count(&calls) > 1);
            drop(events);
            drop(window);
            let drained = Arc::new(AtomicUsize::new(0));
            let observed = drained.clone();
            assert!(
                queue
                    .TryEnqueue(&DispatcherQueueHandler::new(move || {
                        observed.fetch_add(1, Ordering::Relaxed);
                        Ok(())
                    }))
                    .unwrap()
            );
            drop(queue);
            // SAFETY: WM_QUIT affects this test thread only, never the desktop.
            unsafe { PostQuitMessage(73) };
            drop(runtime);
            assert_eq!(drained.load(Ordering::Relaxed), 1);
            assert_eq!(
                Arc::strong_count(&calls),
                1,
                "Event handler must release its capture"
            );
            let mut message = MSG::default();
            // SAFETY: inspect only this test thread's message queue.
            unsafe {
                assert_ne!(
                    PeekMessageW(
                        &mut message,
                        std::ptr::null_mut(),
                        WM_QUIT,
                        WM_QUIT,
                        PM_REMOVE
                    ),
                    0
                );
            }
            assert_eq!(message.wParam, 73);
        }
    }
}
