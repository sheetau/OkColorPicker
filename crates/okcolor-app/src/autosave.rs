//! A single sleeping writer coalesces pending snapshots and flushes on normal shutdown.
use crate::{
    model::PickerModel,
    persistence::{SavedState, StateStore},
};
use std::{
    cell::RefCell,
    io,
    rc::Rc,
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
};
use zgui::{prelude::*, reactive::RenderEffect};

#[derive(Default)]
struct Pending {
    latest: Option<SavedState>,
    stopping: bool,
}
type Mailbox = Arc<(Mutex<Pending>, Condvar)>;

pub struct SaveWorker {
    mailbox: Mailbox,
    thread: Option<JoinHandle<()>>,
}

impl SaveWorker {
    pub fn start(
        store: StateStore,
        initial: SavedState,
        report: impl Fn(io::Result<()>) + Send + 'static,
    ) -> io::Result<Self> {
        let mailbox = Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        let input = mailbox.clone();
        let thread = thread::Builder::new()
            .name("okcolor-state-writer".into())
            .spawn(move || {
                let mut saved = initial;
                loop {
                    let (lock, wake) = &*input;
                    let mut pending = lock.lock().unwrap();
                    while pending.latest.is_none() && !pending.stopping {
                        pending = wake.wait(pending).unwrap();
                    }
                    let Some(state) = pending.latest.take() else {
                        break;
                    };
                    drop(pending);
                    if state != saved {
                        let result = store.save(&state);
                        if result.is_ok() {
                            saved = state;
                        }
                        report(result);
                    } else {
                        report(Ok(()));
                    }
                }
            })?;
        Ok(Self {
            mailbox,
            thread: Some(thread),
        })
    }

    pub fn submit(&self, state: SavedState) {
        let (lock, wake) = &*self.mailbox;
        let mut pending = lock.lock().unwrap();
        if !pending.stopping {
            pending.latest = Some(state);
            wake.notify_one();
        }
    }

    pub fn finish(&mut self) {
        let (lock, wake) = &*self.mailbox;
        lock.lock().unwrap().stopping = true;
        wake.notify_one();
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            eprintln!("State writer stopped unexpectedly");
        }
    }
}
impl Drop for SaveWorker {
    fn drop(&mut self) {
        self.finish();
    }
}

/// The effect owns the writer; unmounting the window joins it after its final queued save.
pub fn bind(model: PickerModel, store: StateStore, initial: SavedState) {
    let status = model.storage_message;
    let writer = match SaveWorker::start(store, initial, move |result| {
        let message = match result {
            Ok(()) => String::new(),
            Err(error) => {
                eprintln!("Cannot save state: {error}");
                format!("Changes not saved: {error}")
            }
        };
        if status.try_get_untracked().is_some_and(|old| old != message) {
            status.try_set(message);
        }
    }) {
        Ok(writer) => Rc::new(RefCell::new(writer)),
        Err(error) => {
            model
                .storage_message
                .set(format!("Session only — cannot start state writer: {error}"));
            return;
        }
    };
    model.can_save.set(true);
    let submitting = writer.clone();
    let effect = RenderEffect::new(move |_| {
        model.save_request.get();
        submitting.borrow().submit(model.saved_state());
    });
    on_cleanup_local(move || {
        writer.borrow().submit(model.saved_state_untracked());
        drop(effect);
        writer.borrow_mut().finish();
    });
}
