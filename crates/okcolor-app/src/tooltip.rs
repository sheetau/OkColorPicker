use std::time::Duration;
use zgui::{
    prelude::*,
    reactive::RenderEffect,
    view::{TimeoutHandle, Timers},
};

#[derive(Clone, Copy)]
pub(crate) struct Tooltips {
    active: RwSignal<Option<NodeRef>, LocalStorage>,
    pub text: RwSignal<String>,
    pub position: RwSignal<(f32, f32)>,
    timer: StoredValue<Option<TimeoutHandle>, LocalStorage>,
    clock: StoredValue<Timers, LocalStorage>,
    delay: Duration,
}

impl Tooltips {
    pub fn mount() -> Self {
        let tips = Self {
            active: RwSignal::new_local(None),
            text: RwSignal::new(String::new()),
            position: RwSignal::new((0.0, 0.0)),
            timer: StoredValue::new_local(None),
            clock: StoredValue::new_local(Timers::current().expect("picker window")),
            delay: hover_delay(),
        };
        provide_context(tips);
        tips
    }

    pub fn bind(node: NodeRef, label: impl Fn() -> String + Copy + 'static) {
        let tips = use_context::<Self>().expect("mounted picker");
        let scale = use_window().scale();
        let effect = RenderEffect::new(move |_| {
            node.get()?;
            let enter = node.listen(
                events::POINTER_ENTER,
                ListenerOptions::default(),
                move |_| {
                    tips.active.set(Some(node));
                    tips.text.set(String::new());
                    tips.timer.set_value(Some(tips.clock.with_value(|clock| {
                        clock.set_timeout(tips.delay, move || {
                            let Some(b) = node.window_bounds() else {
                                return;
                            };
                            let Some(window) = node.window_root().and_then(|r| r.window_bounds())
                            else {
                                return;
                            };
                            let zoom = scale.get_untracked();
                            let width = window.size.width.0 / zoom;
                            let text = label();
                            let x = (b.origin.x.0 / zoom).clamp(
                                2.0,
                                (width - (text.chars().count() as f32 * 5.0 + 12.0)).max(2.0),
                            );
                            let lines = ((text.chars().count() as f32 * 5.0)
                                / (width - 16.0).max(1.0))
                            .ceil()
                            .max(1.0);
                            let height = 10.0 + lines * 12.0;
                            let y = if b.origin.y.0 / zoom > height {
                                b.origin.y.0 / zoom - height
                            } else {
                                (b.origin.y.0 + b.size.height.0) / zoom + 3.0
                            };
                            tips.position.set((x, y));
                            tips.text.set(text);
                        })
                    })));
                },
            );
            let leave = node.listen(
                events::POINTER_LEAVE,
                ListenerOptions::default(),
                move |_| tips.dismiss(node),
            );
            let press = node.listen(
                events::POINTER_DOWN,
                ListenerOptions::default(),
                move |_| tips.dismiss(node),
            );
            Some((enter, leave, press))
        });
        on_cleanup_local(move || {
            drop(effect);
            tips.dismiss(node);
        });
    }

    fn dismiss(self, node: NodeRef) {
        if self.active.try_get_untracked() == Some(Some(node)) {
            self.timer.set_value(None);
            self.active.set(None);
            self.text.set(String::new());
        }
    }
}

pub(crate) fn segments(label: &str) -> Vec<(bool, String)> {
    let hex = label.split_whitespace().find(|s| {
        s.strip_prefix('#').is_some_and(|s| {
            matches!(s.len(), 3 | 4 | 6 | 8) && s.bytes().all(|b| b.is_ascii_hexdigit())
        })
    });
    let Some(hex) = hex else {
        return vec![(false, label.to_owned())];
    };
    let (before, after) = label.split_once(hex).expect("matched substring");
    [(false, before.trim()), (true, hex), (false, after.trim())]
        .into_iter()
        .filter(|(_, s)| !s.is_empty())
        .map(|(mono, s)| (mono, s.to_owned()))
        .collect()
}

fn hover_delay() -> Duration {
    #[cfg(windows)]
    {
        // The initial Win32 tooltip delay uses the double-click time.
        let ms = unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime() };
        Duration::from_millis(u64::from(ms.max(500)))
    }
    #[cfg(not(windows))]
    Duration::from_millis(500)
}
