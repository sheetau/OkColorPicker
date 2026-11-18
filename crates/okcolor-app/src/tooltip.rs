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
                                (width - (text.chars().count() as f32 * 7.0 + 12.0)).max(2.0),
                            );
                            let y = if b.origin.y.0 / zoom > 25.0 {
                                b.origin.y.0 / zoom - 22.0
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

fn hover_delay() -> Duration {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SPI_GETMOUSEHOVERTIME, SystemParametersInfoW,
        };
        let mut milliseconds = 0_u32;
        if unsafe {
            SystemParametersInfoW(SPI_GETMOUSEHOVERTIME, 0, (&raw mut milliseconds).cast(), 0)
        } != 0
        {
            return Duration::from_millis(u64::from(milliseconds));
        }
    }
    Duration::from_millis(400)
}
