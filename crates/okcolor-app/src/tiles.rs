//! Pointer gestures shared by color samples, library tiles and generated palettes.
use crate::{model::PickerModel, surface::ColorSwatchProps};
use okcolor_core::{Oklch, format_color};
use std::time::Duration;
use zgui::{prelude::*, reactive::UnsyncCallback};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Source {
    Sample,
    Recent,
    Library(u64),
    Palette(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Destination {
    Library(Option<u64>, Option<u64>),
    Palette(usize),
}

#[derive(Clone, Copy)]
pub(crate) struct TileGestures {
    page: NodeRef,
    zones: StoredValue<Vec<(NodeRef, Destination)>, LocalStorage>,
    pub hovering: RwSignal<Option<Destination>>,
    pub dragging: RwSignal<Option<NodeRef>, LocalStorage>,
    pub palette: RwSignal<Vec<(Oklch, bool)>>,
}

impl TileGestures {
    pub fn mount(page: NodeRef) -> Self {
        let gestures = Self {
            page,
            zones: StoredValue::new_local(Vec::new()),
            hovering: RwSignal::new(None),
            dragging: RwSignal::new_local(None),
            palette: RwSignal::new(Vec::new()),
        };
        provide_context(gestures);
        gestures
    }

    pub fn current() -> Self {
        use_context().expect("color tiles belong to a picker")
    }

    pub fn register(self, node: NodeRef, destination: Destination) {
        self.zones
            .update_value(|zones| zones.push((node, destination)));
        on_cleanup_local(move || {
            self.zones
                .update_value(|zones| zones.retain(|(zone, _)| *zone != node))
        });
    }

    fn hit(self, x: f32, y: f32) -> Option<Destination> {
        let visible = self.page.window_bounds()?;
        if y < visible.origin.y.0 || y >= visible.origin.y.0 + visible.size.height.0 {
            return None;
        }
        self.zones.with_value(|zones| {
            zones
                .iter()
                .rev()
                .filter_map(|(node, dest)| {
                    let b = node.window_bounds()?;
                    let mut destination = *dest;
                    if let Destination::Palette(i) = destination
                        && i != usize::MAX
                        && x >= b.origin.x.0 + b.size.width.0 * 0.5
                    {
                        destination = Destination::Palette(
                            if i + 1 == self.palette.with_untracked(Vec::len) {
                                usize::MAX
                            } else {
                                i + 1
                            },
                        );
                    }
                    (x >= b.origin.x.0 - 1.0
                        && y >= b.origin.y.0
                        && x < b.origin.x.0 + b.size.width.0 + 1.0
                        && y < b.origin.y.0 + b.size.height.0)
                        .then_some((b.size.width.0 * b.size.height.0, destination))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, dest)| dest)
        })
    }

    fn drop_at(self, model: PickerModel, source: Source, color: Oklch, destination: Destination) {
        match destination {
            Destination::Library(folder, before) => {
                let mut library = model.library.get_untracked();
                let result = match source {
                    Source::Library(id) => library.move_swatch(id, folder).map(|()| id),
                    _ => library.add(color, folder),
                };
                match result {
                    Ok(id) => {
                        if before != Some(id)
                            && let Some(from) =
                                library.swatches.iter().position(|entry| entry.id == id)
                        {
                            let tile = library.swatches.remove(from);
                            let to = library
                                .swatches
                                .iter()
                                .position(|entry| Some(entry.id) == before)
                                .unwrap_or(library.swatches.len());
                            library.swatches.insert(to, tile);
                        }
                        model.library.set(library);
                    }
                    Err(error) => model.message.set(error.into()),
                }
            }
            Destination::Palette(index) => self.palette.update(|colors| {
                let mut to = index.min(colors.len());
                let entry = if let Source::Palette(from) = source {
                    if from >= colors.len() {
                        return;
                    }
                    if from < to {
                        to -= 1;
                    }
                    colors.remove(from)
                } else {
                    if colors.len() >= 10 {
                        return;
                    }
                    (color, false)
                };
                colors.insert(to, entry);
            }),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct TileHeading(pub RwSignal<String>);

impl TileHeading {
    pub fn mount() -> Self {
        let heading = Self(RwSignal::new(String::new()));
        provide_context(heading);
        heading
    }
}

pub(crate) fn contrast_ink(color: Oklch) -> &'static str {
    let rgb = color
        .mapped_rgb(
            okcolor_core::Gamut::Srgb,
            okcolor_core::GamutMapping::LocalMinde,
        )
        .unwrap_or([0.0; 3]);
    let linear = rgb.map(|c| {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    });
    if linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722 > 0.179 {
        "#000000"
    } else {
        "#ffffff"
    }
}

#[component]
pub(crate) fn ColorTile(
    node: NodeRef,
    class: &'static str,
    color: Memo<Oklch>,
    source: Source,
    label: Memo<String>,
    destination: Option<Destination>,
    activate: UnsyncCallback<()>,
) -> impl IntoView {
    let model: PickerModel = use_context().expect("mounted picker");
    let shared = TileGestures::current();
    if let Some(dest) = destination {
        shared.register(node, dest);
    }
    let heading = use_context::<TileHeading>();
    let clipboard = use_clipboard();
    let scale = use_window().scale();
    let origin = RwSignal::new(None::<(f32, f32)>);
    let suppress_click = RwSignal::new(false);
    let copied = RwSignal::new(false);
    crate::tooltip::Tooltips::bind(node, move || {
        let text = label.get_untracked();
        if matches!(source, Source::Palette(_)) {
            format!("{text} · Shift-click to lock or unlock")
        } else {
            text
        }
    });
    let timers = StoredValue::new_local(zgui::view::Timers::current().expect("picker window"));
    let copy_timer = StoredValue::new_local(None::<zgui::view::TimeoutHandle>);
    let scroll_timer = StoredValue::new_local(None::<zgui::view::IntervalHandle>);
    let cursor = RwSignal::new((0.0f32, 0.0f32));
    let cancel = move || {
        scroll_timer.set_value(None);
        origin.set(None);
        if shared.dragging.get_untracked() == Some(node) {
            shared.dragging.set(None);
            shared.hovering.set(None);
        }
    };
    on_cleanup_local(cancel);
    let press = move |cx: &mut EventCx<'_, events::PointerDown>| {
        suppress_click.set(cx.button != Some(PointerButton::Primary));
        match cx.button {
            Some(PointerButton::Primary) => {
                origin.set(Some((cx.position.x.0, cx.position.y.0)));
                cx.capture_pointer();
            }
            Some(PointerButton::Secondary) => {
                if let Ok(formatted) = format_color(
                    color.get_untracked(),
                    model.format.get_untracked(),
                    model.mapping.get_untracked(),
                ) {
                    clipboard.set_text(ClipboardKind::Standard, formatted.text);
                    copied.set(true);
                    copy_timer.set_value(Some(timers.with_value(|timers| {
                        timers.set_timeout(Duration::from_millis(850), move || copied.set(false))
                    })));
                }
            }
            Some(PointerButton::Middle) => match source {
                Source::Recent => model
                    .recent
                    .update(|recent| recent.retain(|entry| *entry != color.get_untracked())),
                Source::Library(id) => model.library.update(|library| library.remove(id)),
                Source::Palette(i) => shared.palette.update(|colors| {
                    if i < colors.len() && colors.len() > 2 {
                        colors.remove(i);
                    }
                }),
                Source::Sample => {}
            },
            _ => {}
        }
    };
    let drag = move |cx: &mut EventCx<'_, events::PointerMove>| {
        if let Some((x, y)) = origin.get_untracked() {
            if shared.dragging.get_untracked() != Some(node)
                && (cx.position.x.0 - x).hypot(cx.position.y.0 - y) >= 4.0
            {
                shared.dragging.set(Some(node));
                suppress_click.set(true);
                scroll_timer.set_value(Some(timers.with_value(|timers| {
                    timers.set_interval(Duration::from_millis(32), move || {
                        let Some(b) = shared.page.window_bounds() else {
                            return;
                        };
                        let (x, y) = cursor.get_untracked();
                        let edge = 20.0 * scale.get_untracked();
                        let step = if y < b.origin.y.0 + edge {
                            -edge
                        } else if y > b.origin.y.0 + b.size.height.0 - edge {
                            edge
                        } else {
                            0.0
                        };
                        if step != 0.0 {
                            use zgui::geom::{DevicePx, Point};
                            shared.page.scroll_to(
                                ScrollTarget::By(Point::new(DevicePx(0.0), DevicePx(step))),
                                ScrollBehavior::Instant,
                            );
                            shared.hovering.set(shared.hit(x, y));
                        }
                    })
                })));
            }
            if shared.dragging.get_untracked() == Some(node) {
                let (px, py) = (
                    cx.position.x.0 * scale.get_untracked(),
                    cx.position.y.0 * scale.get_untracked(),
                );
                cursor.set((px, py));
                shared.hovering.set(shared.hit(px, py));
            }
        }
    };
    let release = move |cx: &mut EventCx<'_, events::PointerUp>| {
        if cx.button != Some(PointerButton::Primary) {
            return;
        }
        let target = shared.hit(
            cx.position.x.0 * scale.get_untracked(),
            cx.position.y.0 * scale.get_untracked(),
        );
        let dropping = shared.dragging.get_untracked() == Some(node);
        cx.release_pointer();
        cancel();
        if dropping && let Some(dest) = target {
            shared.drop_at(model, source, color.get_untracked(), dest);
        }
    };
    view! {control(class=class,node_ref=node,tabindex=Focus::Sequential,a11y:label=move ||label.get(),
        class:dark=move ||class=="preset-color" && color.get().components()[0]<0.5,
        class:dragged=move ||shared.dragging.get()==Some(node),
        style:outline-color=move ||(shared.dragging.get()==Some(node)).then(||{
            let l=color.get().components()[0];
            let neutral=Oklch::new((l+if l<0.5 {1.0}else{0.0})*0.5,0.0,0.0,1.0).expect("bounded lightness");
            okcolor_core::format_color(neutral,okcolor_core::ColorFormat::Hex,okcolor_core::GamutMapping::Clip).expect("neutral color").text
        }),
        on:pointer_enter=move |_|{
            if let Some(heading)=heading{heading.0.set(label.get_untracked());}
        },
        on:pointer_leave=move |_|{if let Some(heading)=heading{heading.0.set(String::new());}},
        on:pointer_down=press,
        on:pointer_move=drag,
        on:pointer_up=release,
        on:pointer_cancel=move |_|cancel(),
        on:click=move |cx|{
            if !suppress_click.get_untracked(){
                if let Source::Palette(index)=source && cx.modifiers.contains(Modifiers::SHIFT) {
                    shared.palette.update(|colors|{if let Some(entry)=colors.get_mut(index){entry.1 = !entry.1;}});
                }else{activate.run(());}
            }
            suppress_click.set(false);
        }) {
            ColorSwatch(model=model,color=color,outline_dark=class!="transparent-color" && class!="preset-color" && class!="generated-color")
            if move ||destination.is_some() && shared.hovering.get()==destination {box(class="insertion-line") {}}
            if move ||class=="slot-tile" && !model.display.get().wide_output && !color.get().in_gamut(okcolor_core::Gamut::Srgb) {
                {zgui::elements::vector().document(include_str!("../assets/unavailable.svg")).class("unavailable-slot")}
            }
            if move ||class=="transparent-color" {
                {zgui::elements::vector().document(include_str!("../assets/transparent.svg")).class("transparent-mark")}
            }
            box(class="tile-check",style:color=move ||Some(contrast_ink(color.get()).to_owned()),class:visible=move ||copied.get()) {
                {zgui::elements::vector().document(include_str!("../assets/check.svg")).class("check-icon")}
            }
    }}
}
