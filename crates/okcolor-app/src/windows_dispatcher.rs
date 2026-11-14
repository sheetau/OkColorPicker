//! UI-thread WinRT lifetime. Keep this scope outside the native event loop.
use std::{marker::PhantomData, rc::Rc};
use windows::{
    System::{DispatcherQueue, DispatcherQueueController},
    Win32::System::WinRT::{
        CreateDispatcherQueueController, DQTAT_COM_NONE, DQTYPE_THREAD_CURRENT,
        DispatcherQueueOptions, RO_INIT_SINGLETHREADED, RoInitialize, RoUninitialize,
    },
    core::Result,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

/// Owns only a queue created by this scope; an existing host queue is borrowed.
/// Drop after all display subscriptions and the application event loop.
pub struct DispatcherRuntime {
    controller: Option<DispatcherQueueController>,
    // COM initialization and shutdown must happen on the constructing thread.
    _thread: PhantomData<Rc<()>>,
}

impl DispatcherRuntime {
    pub fn new() -> Result<Self> {
        // SAFETY: the !Send guard balances every successful initialization.
        unsafe { RoInitialize(RO_INIT_SINGLETHREADED)? };
        let mut runtime = Self {
            controller: None,
            _thread: PhantomData,
        };
        if DispatcherQueue::GetForCurrentThread().is_err() {
            // SAFETY: attach to this thread without changing its apartment.
            runtime.controller = Some(unsafe {
                CreateDispatcherQueueController(DispatcherQueueOptions {
                    dwSize: size_of::<DispatcherQueueOptions>() as u32,
                    threadType: DQTYPE_THREAD_CURRENT,
                    apartmentType: DQTAT_COM_NONE,
                })?
            });
        }
        Ok(runtime)
    }
}

impl Drop for DispatcherRuntime {
    fn drop(&mut self) {
        if let Some(controller) = self.controller.take() {
            let mut quit = None;
            let shutdown = (|| -> Result<()> {
                let action = controller.ShutdownQueueAsync()?;
                // A current-thread queue cannot finish shutdown while its thread
                // blocks on the action. Drain its messages until it completes.
                // SAFETY: this scope owns the UI thread's queue. Preserve WM_QUIT
                // for an enclosing host loop rather than swallowing it.
                unsafe {
                    while action.Status()?.0 == 0 {
                        // AsyncStatus::Started
                        let mut message = MSG::default();
                        while PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0
                        {
                            if message.message == WM_QUIT {
                                quit = Some(message.wParam as i32);
                            } else {
                                TranslateMessage(&message);
                                DispatchMessageW(&message);
                            }
                        }
                        if action.Status()?.0 == 0 {
                            MsgWaitForMultipleObjectsEx(
                                0,
                                std::ptr::null(),
                                10,
                                QS_ALLINPUT,
                                MWMO_INPUTAVAILABLE,
                            );
                        }
                    }
                }
                action.GetResults()
            })();
            if let Some(code) = quit {
                // SAFETY: restore this thread's quit message even after an error.
                unsafe { PostQuitMessage(code) };
            }
            if let Err(error) = shutdown {
                eprintln!("Display dispatcher shutdown failed: {error}");
            }
        }
        // SAFETY: new succeeded and !Send keeps this guard on the same thread.
        unsafe { RoUninitialize() };
    }
}
