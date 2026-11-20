//! Pointer gestures shared by color samples, library tiles and generated palettes.
use crate::{model::PickerModel, surface::ColorSwatchProps};
use okcolor_core::{Oklch, format_color};
use std::time::Duration;
use zgui::{prelude::*, reactive::UnsyncCallback};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Source {
    Slot(usize),
    Sample,
    Recent,
    Library(u64),
    Palette(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Destination {
    Slot(usize),
    FolderMenu,
    Folder(Option<u64>),
    Library(Option<u64>, Option<u64>),
    Palette(usize),
}

#[derive(Clone, Copy)]
pub(crate) struct TileGestures {
    page: NodeRef,
    folder_menu: NodeRef,
    adjustment_menu: NodeRef,
    zones: StoredValue<Vec<(NodeRef, Destination)>, LocalStorage>,
    pub hovering: RwSignal<Option<Destination>>,
    pub dragging: RwSignal<Option<NodeRef>, LocalStorage>,
    pub palette: RwSignal<Vec<(Oklch, bool)>>,
}

impl TileGestures {
    pub fn mount(
        page: NodeRef,
        folder_menu: NodeRef,
        adjustment_menu: NodeRef,
        model: PickerModel,
    ) -> Self {
        let gestures = Self {
            page,
            folder_menu,
            adjustment_menu,
            zones: StoredValue::new_local(Vec::new()),
            hovering: RwSignal::new(None),
            dragging: RwSignal::new_local(None),
            palette: model.palette,
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
            // A remount can register the same stable node before its previous owner is disposed.
            self.zones.update_value(|zones| {
                if let Some(i) = zones.iter().position(|entry| *entry == (node, destination)) {
                    zones.remove(i);
                }
            })
        });
    }

    fn hit(self, x: f32, y: f32, source: Source) -> Option<Destination> {
        let visible = self.page.window_bounds()?;
        if y < visible.origin.y.0 || y >= visible.origin.y.0 + visible.size.height.0 {
            return None;
        }
        let in_menu = self.folder_menu.window_bounds().is_some_and(|b| {
            x >= b.origin.x.0
                && x < b.origin.x.0 + b.size.width.0
                && y >= b.origin.y.0
                && y < b.origin.y.0 + b.size.height.0
        });
        let adjusting = self.adjustment_menu.get_untracked().is_some();
        self.zones.with_value(|zones| {
            zones
                .iter()
                .rev()
                .filter_map(|(node, dest)| {
                    // Clip scrolled options and keep tiles under the open menu out of hit testing.
                    if matches!(dest, Destination::Folder(_)) != in_menu
                        || (adjusting && matches!(dest, Destination::Palette(_)))
                    {
                        return None;
                    }
                    if matches!(dest, Destination::Slot(_)) && !matches!(source, Source::Slot(_)) {
                        return None;
                    }
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

    pub(crate) fn palette_boundary(self, index: usize) -> String {
        self.palette.with(|colors| {
            let black = Oklch::new(0.0, 0.0, 0.0, 1.0).expect("black");
            let i = index.min(colors.len());
            let left = i.checked_sub(1).map_or(black, |j| colors[j].0);
            let right = colors.get(i).map_or(black, |entry| entry.0);
            boundary_ink(left, right)
        })
    }

    fn drop_at(self, model: PickerModel, source: Source, color: Oklch, destination: Destination) {
        let destination = match destination {
            Destination::Folder(folder) => Destination::Library(folder, None),
            other => other,
        };
        match destination {
            Destination::Slot(to) => {
                if let Source::Slot(from) = source {
                    model.copy_slot(from, to);
                }
            }
            Destination::FolderMenu | Destination::Folder(_) => {}
            Destination::Library(folder, before) => {
                model.edit_library(|library| {
                    let id = match source {
                        Source::Library(id) => {
                            library.move_swatch(id, folder)?;
                            id
                        }
                        _ => library.add(color, folder)?,
                    };
                    if before != Some(id)
                        && let Some(from) = library.swatches.iter().position(|s| s.id == id)
                    {
                        let swatch = library.swatches.remove(from);
                        let to = library
                            .swatches
                            .iter()
                            .position(|s| Some(s.id) == before)
                            .unwrap_or(library.swatches.len());
                        library.swatches.insert(to, swatch);
                    }
                    Ok(())
                });
            }
            Destination::Palette(index) => model.edit_palette(|colors| {
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

fn relative_luminance(rgb: [f64; 3]) -> f64 {
    let linear = rgb.map(linear_channel);
    linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722
}

fn linear_channel(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub(crate) fn contrast_ink(color: Oklch) -> &'static str {
    let rgb = color
        .mapped_rgb(
            okcolor_core::Gamut::Srgb,
            okcolor_core::GamutMapping::LocalMinde,
        )
        .unwrap_or([0.0; 3]);
    if relative_luminance(rgb) > 0.179 {
        "#000000"
    } else {
        "#ffffff"
    }
}

/// Choose the neutral ink with the largest minimum contrast to both adjacent fills.
pub(crate) fn boundary_ink(first: Oklch, second: Oklch) -> String {
    let surfaces = [first, second].map(|color| {
        relative_luminance(
            color
                .mapped_rgb(
                    okcolor_core::Gamut::Srgb,
                    okcolor_core::GamutMapping::LocalMinde,
                )
                .unwrap_or([0.0; 3]),
        ) + 0.05
    });
    let contrast = |v: u8| {
        let gray = linear_channel(f64::from(v) / 255.0) + 0.05;
        surfaces
            .into_iter()
            .map(|y| gray.max(y) / gray.min(y))
            .fold(f64::INFINITY, f64::min)
    };
    let gray = (0..=255)
        .max_by(|a, b| contrast(*a).total_cmp(&contrast(*b)))
        .unwrap();
    format!("#{gray:02x}{gray:02x}{gray:02x}")
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
        let mut text = label.get_untracked();
        if (matches!(source, Source::Library(_))
            || class == "preset-color"
            || (class == "transparent-color" && !model.supports_none()))
            && let Ok(hex) = format_color(
                color.get_untracked(),
                okcolor_core::ColorFormat::Hex,
                model.mapping.get_untracked(),
            )
        {
            text.push(' ');
            text.push_str(&hex.text);
        }
        if matches!(source, Source::Palette(_)) {
            format!("{text} · Shift-click to lock or unlock")
        } else {
            text
        }
    });
    let timers = StoredValue::new_local(zgui::view::Timers::current().expect("picker window"));
    let rename = use_context::<crate::swatches::RenameSwatch>();
    let hold = crate::swatches::Hold::new();
    let copy_timer = StoredValue::new_local(None::<zgui::view::TimeoutHandle>);
    let cancel = move || {
        hold.cancel();
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
                if let (Source::Library(id), Some(rename)) = (source, rename) {
                    hold.start(move || {
                        suppress_click.set(true);
                        origin.set(None);
                        rename.begin.run(id);
                    });
                }
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
                Source::Library(id) => {
                    model.edit_library(|library| {
                        library.remove(id);
                        Ok(())
                    });
                }
                Source::Palette(i) => model.edit_palette(|colors| {
                    if i < colors.len() && colors.len() > 2 {
                        colors.remove(i);
                    }
                }),
                Source::Sample | Source::Slot(_) => {}
            },
            _ => {}
        }
    };
    let drag = move |cx: &mut EventCx<'_, events::PointerMove>| {
        if let Some((x, y)) = origin.get_untracked() {
            if shared.dragging.get_untracked() != Some(node)
                && (cx.position.x.0 - x).hypot(cx.position.y.0 - y) >= 4.0
            {
                hold.cancel();
                shared.dragging.set(Some(node));
                suppress_click.set(true);
            }
            if shared.dragging.get_untracked() == Some(node) {
                let (px, py) = (
                    cx.position.x.0 * scale.get_untracked(),
                    cx.position.y.0 * scale.get_untracked(),
                );
                shared.hovering.set(shared.hit(px, py, source));
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
            source,
        );
        let dropping = shared.dragging.get_untracked() == Some(node);
        cx.release_pointer();
        cancel();
        if dropping && let Some(dest) = target {
            shared.drop_at(model, source, color.get_untracked(), dest);
        }
    };
    let unavailable = Memo::new(
        move |_| matches!(source, Source::Slot(i) if model.slot_is_none(i) || model.display.get().unavailable(color.get()).is_some()),
    );
    let renaming = move || matches!(source, Source::Library(id) if rename.is_some_and(|state| state.active.get() == Some(id)));
    let ink = move || boundary_ink(color.get(), Oklch::new(0.0, 0.0, 0.0, 1.0).expect("black"));
    let fill = Memo::new(move |_| {
        if class == "slot-tile" {
            PickerModel::preview_source(color.get(), model.display.get())
        } else {
            color.get()
        }
    });
    let outlined = move || {
        shared.dragging.get() == Some(node)
            || matches!(destination, Some(Destination::Slot(_)) if destination == shared.hovering.get())
    };
    view! {control(class=class,node_ref=node,tabindex=Focus::Sequential,a11y:label=move ||label.get(),
        class:dragged=outlined,
        class:renaming=renaming,
        style:outline-color=move ||(outlined() || renaming()).then(&ink),
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
                    model.edit_palette(|colors|{if let Some(entry)=colors.get_mut(index){entry.1 = !entry.1;}});
                }else{activate.run(());}
            }
            suppress_click.set(false);
        }) {
            ColorSwatch(model=model,color=fill,outline_dark=class!="transparent-color")
            if move ||destination.is_some() && !matches!(destination,Some(Destination::Slot(_))) && shared.hovering.get()==destination {box(class="insertion-line",style:background-color=move ||match destination { Some(Destination::Palette(i))=>Some(shared.palette_boundary(i)), _=>None }) {}}
            if move ||unavailable.get() {
                box(class="unavailable-slot") {{zgui::elements::vector().document(include_str!("../assets/unavailable.svg")).class("unavailable-cross")}}
            }
            if move ||class=="transparent-color" {
                {zgui::elements::vector().document(include_str!("../assets/transparent.svg")).class("transparent-mark")}
            }
            box(class="tile-check",style:color=move ||Some(if unavailable.get() { "#ffffff" } else { contrast_ink(color.get()) }.to_owned()),class:visible=move ||copied.get()) {
                {zgui::elements::vector().document(include_str!("../assets/check.svg")).class("check-icon")}
            }
    }}
}
