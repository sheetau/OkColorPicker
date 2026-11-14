use crate::{
    model::PickerModel,
    surface::{ColorControlProps, SurfaceKind},
    window_frame::ResizeEdgesProps,
};
use okcolor_core::{ColorFormat, Gamut, GamutMapping, Oklch};
use zgui::{prelude::*, reactive::RenderEffect};

/// Stable handles also let application tests exercise the actual mounted controls.
#[derive(Clone, Copy)]
pub struct PickerRefs {
    pub slice: NodeRef,
    pub bars: [NodeRef; 4],
    pub numbers: [NodeRef; 4],
    pub codes: [NodeRef; 2],
}
impl Default for PickerRefs {
    fn default() -> Self {
        Self {
            slice: NodeRef::new(),
            bars: std::array::from_fn(|_| NodeRef::new()),
            numbers: std::array::from_fn(|_| NodeRef::new()),
            codes: std::array::from_fn(|_| NodeRef::new()),
        }
    }
}

fn preview_css(color: Oklch, mapping: GamutMapping) -> String {
    let rgb = color.mapped_rgb(Gamut::Srgb, mapping).unwrap_or([0.0; 3]);
    format!(
        "rgb({:.3} {:.3} {:.3} / {})",
        rgb[0] * 255.0,
        rgb[1] * 255.0,
        rgb[2] * 255.0,
        color.components()[3]
    )
}

#[component]
pub fn Picker(model: PickerModel, refs: PickerRefs) -> impl IntoView {
    let clipboard = use_clipboard();
    let shortcut = move |cx: &mut EventCx<'_, events::KeyDown>| {
        if model.editing_text.get_untracked() || !cx.modifiers.contains(Modifiers::CONTROL) {
            return;
        }
        let Some(key) = cx.key.as_str() else {
            return;
        };
        match key.to_ascii_lowercase().as_str() {
            "c" => clipboard.set_text(
                ClipboardKind::Standard,
                model.output_untracked(model.format.get_untracked()),
            ),
            "v" => clipboard.read_text(ClipboardKind::Standard, move |text| {
                if let Some(text) = text {
                    model.input(&text);
                } else {
                    model.message.set("The clipboard contains no text".into());
                }
            }),
            "z" => {
                if cx.modifiers.contains(Modifiers::SHIFT) {
                    model.redo()
                } else {
                    model.undo()
                }
            }
            "y" => model.redo(),
            _ => return,
        }
        cx.prevent_default();
        cx.stop_propagation();
    };
    view! {
        column(class="app",on:key_down=shortcut) {
            Titlebar()
            column(class="body") {
                row(class="recents",a11y:label="Recent colors") {
                    if move || model.recent.get().is_empty() {text(class="empty-recent") {"Recent colors"}}
                    for entry in move || model.recent.get(), key=|entry:&Oklch| entry.components().map(f64::to_bits) {
                        control(class="recent",tabindex=Focus::Sequential,a11y:label="Recall recent color",
                            style:background-color=move || Some(preview_css(entry,model.mapping.get())),
                            on:click=move |_| {model.select(entry);}) {}
                    }
                }
                row(class="options") {
                    control(class="gamut",
                        class:selected=move ||model.gamut.get()==Gamut::Srgb,
                        tabindex=Focus::Sequential,
                        on:click=move |_|model.gamut.set(Gamut::Srgb)) {"sRGB"}
                    control(class="gamut",
                        class:selected=move ||model.gamut.get()==Gamut::DisplayP3,
                        tabindex=Focus::Sequential,
                        on:click=move |_|model.gamut.set(Gamut::DisplayP3)) {"P3"}
                    control(class="gamut",
                        class:selected=move ||model.gamut.get()==Gamut::Rec2020,
                        tabindex=Focus::Sequential,
                        on:click=move |_|model.gamut.set(Gamut::Rec2020)) {"2020"}
                    spacer()
                    control(class="mapping",tabindex=Focus::Sequential,a11y:label="Toggle gamut mapping",
                        on:click=move |_|model.mapping.update(|m|*m=if *m==GamutMapping::Clip {GamutMapping::LocalMinde}else{GamutMapping::Clip})) {
                        {move ||if model.mapping.get()==GamutMapping::Clip {"Clipped"} else {"Mapped"}}
                    }
                }
                column(class="plot-section") {
                    ColorControl(model=model,kind=SurfaceKind::Slice,node=refs.slice)
                    row(class="plot-caption") {
                        text {"L ↑"}
                        spacer()
                        text(class="legend srgb") {"sRGB"}
                        text(class="legend p3") {"P3"}
                        text(class="legend rec2020") {"2020"}
                        spacer()
                        text {{move ||format!("C 0–{} →",model.max_chroma.get())}}
                    }
                }
                row(class="adjustments") {
                    column(class="preview-column") {
                        box(class="swatches") {
                            control(class="previous swatch",tabindex=Focus::Sequential,a11y:label="Previous color",
                                style:background-color=move ||Some(preview_css(model.previous.get(),model.mapping.get())),
                                on:click=move |_|{model.select(model.previous.get_untracked());}) {}
                            box(class="current swatch",
                                style:background-color=move ||Some(preview_css(model.color(),model.mapping.get()))) {}
                        }
                        row(class="history-buttons") {
                            control(tabindex=Focus::Sequential,
                                a11y:label="Undo",
                                disabled=move ||!model.can_undo.get(),
                                on:click=move |_|model.undo()) {box(class="history-arrow") {}}
                            control(tabindex=Focus::Sequential,
                                a11y:label="Redo",
                                disabled=move ||!model.can_redo.get(),
                                on:click=move |_|model.redo()) {box(class="history-arrow redo") {}}
                        }
                        spacer()
                        row(class="quick-colors") {
                            control(class="transparent-color",
                                tabindex=Focus::Sequential,
                                a11y:label="Transparent",
                                on:click=move |_|{model.set_channel(3,0.0);}) {"/"}
                            control(class="black-color",
                                tabindex=Focus::Sequential,
                                a11y:label="Black",
                                on:click=move |_|{model.input("#000");}) {}
                            control(class="white-color",
                                tabindex=Focus::Sequential,
                                a11y:label="White",
                                on:click=move |_|{model.input("#fff");}) {}
                        }
                    }
                    column(class="channels") {
                        Channel(model=model,index=0,label="L",kind=SurfaceKind::Lightness,bar=refs.bars[0],number=refs.numbers[0])
                        Channel(model=model,index=1,label="C",kind=SurfaceKind::Chroma,bar=refs.bars[1],number=refs.numbers[1])
                        Channel(model=model,index=2,label="H",kind=SurfaceKind::Hue,bar=refs.bars[2],number=refs.numbers[2])
                        Channel(model=model,index=3,label="A",kind=SurfaceKind::Alpha,bar=refs.bars[3],number=refs.numbers[3])
                    }
                }
                column(class="formats") {
                    CodeField(model=model,node=refs.codes[0],primary=true)
                    CodeField(model=model,node=refs.codes[1],primary=false)
                }
                column(class="status") {
                    row {
                        text {"sRGB preview"}
                        spacer()
                        text {"CSP not connected"}
                    }
                    text(class="warning") {{move || {
                        let message=model.message.get();
                        if !message.is_empty() {message}
                        else if !model.color().in_gamut(model.gamut.get()) {"Outside working gamut — original retained".into()}
                        else if !model.color().in_gamut(Gamut::Srgb) {"Wide-gamut color shown as an sRGB preview".into()}
                        else {String::new()}
                    }}}
                }
            }
            ResizeEdges()
        }
    }
}

/// These two small menus share dismissal and focus behavior, without owning application state.
fn dismiss_menu(open: RwSignal<bool>, popup: NodeRef, trigger: NodeRef) {
    let effect = RenderEffect::new(move |_| {
        if !open.get() || popup.get().is_none() {
            return None;
        }
        let root = popup.window_root()?;
        let outside = root.listen(events::POINTER_DOWN, ListenerOptions::CAPTURE, move |cx| {
            if !popup.contains(cx.target) && !trigger.contains(cx.target) {
                open.set(false);
            }
        });
        let escape = root.listen(events::KEY_DOWN, ListenerOptions::CAPTURE, move |cx| {
            if cx.key == Key::Named(NamedKey::Escape) {
                open.set(false);
                trigger.focus();
                cx.prevent_default();
                cx.stop_propagation();
            }
        });
        Some((outside, escape))
    });
    on_cleanup_local(move || drop(effect));
}

#[component]
fn Titlebar() -> impl IntoView {
    let window = use_window();
    let help = RwSignal::new(false);
    let popup = NodeRef::new();
    let trigger = NodeRef::new();
    dismiss_menu(help, popup, trigger);
    view! {
        row(class="titlebar",on:pointer_down=window.move_drag_handler()) {
            control(class="menu-button",
                node_ref=trigger,
                tabindex=Focus::Sequential,
                a11y:label="Keyboard shortcuts",
                on:pointer_down=window.no_drag_handler(),
                on:click=move |_|help.update(|v|*v = !*v)) {"≡"}
            text(class="title") {"OkColorPicker"}
            spacer()
            control(class="window-button",
                a11y:label="Minimize",
                on:pointer_down=window.no_drag_handler(),
                on:click={let w=window.clone();move |_|w.minimize()}) {"−"}
            control(class="window-button",
                a11y:label="Maximize or restore",
                on:pointer_down=window.no_drag_handler(),
                on:click={let w=window.clone();move |_|w.toggle_maximized()}) {"□"}
            control(class="window-button close",
                a11y:label="Close",
                on:pointer_down=window.no_drag_handler(),
                on:click={let w=window.clone();move |_|w.close()}) {"×"}
            if move ||help.get() {
                column(class="help-menu",node_ref=popup,on:pointer_down=window.no_drag_handler()) {
                    text {"Ctrl C / V · Copy / paste color"}
                    text {"Ctrl Z / Shift Z · Undo / redo"}
                    text {"Arrow keys · Adjust a focused control"}
                    text {"Shift · ×10   Ctrl · ×0.1"}
                    text {"Enter · Commit   Escape · Cancel drag"}
                }
            }
        }
    }
}

#[component]
fn Channel(
    model: PickerModel,
    index: usize,
    label: &'static str,
    kind: SurfaceKind,
    bar: NodeRef,
    number: NodeRef,
) -> impl IntoView {
    let focused = RwSignal::new(false);
    let draft = RwSignal::new(String::new());
    let dirty = RwSignal::new(false);
    let composing = RwSignal::new(false);
    let value = [model.l, model.c, model.h, model.alpha][index];
    let driving = RenderEffect::new(move |_| {
        let value = value.get();
        if !focused.get() {
            dirty.set(false);
            let text = format!("{value:.3}");
            draft.set(text.clone());
            number.set_value(&text);
        }
    });
    on_cleanup_local(move || drop(driving));
    let commit = move || {
        if !dirty.get_untracked() {
            return;
        }
        match draft.get_untracked().parse::<f64>() {
            Ok(value) => {
                if model.set_channel(index, value) {
                    dirty.set(false);
                }
            }
            Err(_) => model.message.set("Enter a finite number".into()),
        }
    };
    view! {
        row(class="channel") {
            text(class="channel-label") {{label}}
            ColorControl(model=model,kind=kind,node=bar)
            field(class="number",node_ref=number,a11y:label=label,
                on:focus_in=move |_|{focused.set(true);model.editing_text.set(true);},
                on:focus_out=move |_|{focused.set(false);model.editing_text.set(false);},
                on:input=move |cx|{if let Some(v)=cx.payload().as_value(){draft.set(v.value.to_string());dirty.set(true);}},
                on:ime_start=move |_|composing.set(true),
                on:ime_preedit=move |_|composing.set(true),
                on:ime_commit=move |_|composing.set(false),
                on:ime_end=move |_|composing.set(false),
                on:change=move |_|commit(),
                on:key_down=move |cx|{
                    if composing.get_untracked() { return; }
                    if cx.key==Key::Named(NamedKey::Enter) {commit();cx.prevent_default();}
                    if cx.key == Key::Named(NamedKey::Escape) {
                        let text = format!("{:.3}", value.get_untracked());
                        dirty.set(false);
                        draft.set(text.clone());
                        number.set_value(&text);
                        model.message.set(String::new());
                        cx.prevent_default();
                    }
                }) {{draft.get_untracked()}}
        }
    }
}

const FORMATS: [(ColorFormat, &str); 11] = [
    (ColorFormat::Hex, "HEX"),
    (ColorFormat::Hex8, "HEX8"),
    (ColorFormat::Rgb, "RGB"),
    (ColorFormat::Hsl, "HSL"),
    (ColorFormat::Oklch, "OKLCH"),
    (ColorFormat::Oklab, "OKLab"),
    (ColorFormat::Lab, "Lab"),
    (ColorFormat::Lch, "LCH"),
    (ColorFormat::Color(Gamut::Srgb), "sRGB"),
    (ColorFormat::Color(Gamut::DisplayP3), "Display P3"),
    (ColorFormat::Color(Gamut::Rec2020), "Rec.2020"),
];

#[component]
fn CodeField(model: PickerModel, node: NodeRef, primary: bool) -> impl IntoView {
    let focused = RwSignal::new(false);
    let menu = RwSignal::new(false);
    let popup = NodeRef::new();
    let trigger = NodeRef::new();
    dismiss_menu(menu, popup, trigger);
    let draft = RwSignal::new(String::new());
    let dirty = RwSignal::new(false);
    let composing = RwSignal::new(false);
    let format = move || {
        if primary {
            ColorFormat::Oklch
        } else {
            model.format.get()
        }
    };
    let driving = RenderEffect::new(move |_| {
        let text = model.output(format());
        if !focused.get() {
            dirty.set(false);
            draft.set(text.clone());
            node.set_value(&text);
        }
    });
    on_cleanup_local(move || drop(driving));
    let clipboard = use_clipboard();
    let selected_format = move || {
        if primary {
            ColorFormat::Oklch
        } else {
            model.format.get_untracked()
        }
    };
    let commit = move || {
        if dirty.get_untracked() && model.input(&draft.get_untracked()) {
            dirty.set(false);
        }
    };
    view! {
        row(class="code-row") {
            text(class="channel-label") {{if primary {"O"}else{"R"}}}
            row(class="code-box") {
                field(class="code",node_ref=node,a11y:label=if primary {"OKLCH color"}else{"Converted color"},
                    on:focus_in=move |_|{focused.set(true);model.editing_text.set(true);},
                    on:focus_out=move |_|{focused.set(false);model.editing_text.set(false);},
                    on:input=move |cx|{if let Some(v)=cx.payload().as_value(){draft.set(v.value.to_string());dirty.set(true);}},
                on:ime_start=move |_|composing.set(true),
                on:ime_preedit=move |_|composing.set(true),
                on:ime_commit=move |_|composing.set(false),
                on:ime_end=move |_|composing.set(false),
                    on:change=move |_|commit(),
                    on:key_down=move |cx|{
                        if composing.get_untracked() { return; }
                        if cx.key==Key::Named(NamedKey::Enter) {commit();cx.prevent_default();}
                        if cx.key == Key::Named(NamedKey::Escape) {
                            let text = model.output_untracked(selected_format());
                            dirty.set(false);
                            draft.set(text.clone());
                            node.set_value(&text);
                            model.message.set(String::new());
                            cx.prevent_default();
                        }}) {{draft.get_untracked()}}
                if move || !primary {
                    control(class="format-trigger",
                        node_ref=trigger,
                        tabindex=Focus::Sequential,
                        a11y:label="Choose output format",
                        on:click=move |_|menu.update(|v|*v = !*v)) {box(class="chevron") {}}
                }
            }
            control(class="copy",
                tabindex=Focus::Sequential,
                a11y:label="Copy color",
                on:click=move |_|clipboard.set_text(ClipboardKind::Standard,model.output_untracked(selected_format()))) {
                box(class="copy-mark") {box(class="copy-back") {}}
            }
            if move ||menu.get() {
                column(class="format-menu",node_ref=popup,on:key_down=move |cx| {
                    let direction=match cx.key {Key::Named(NamedKey::ArrowDown)=>FocusMove::Next,Key::Named(NamedKey::ArrowUp)=>FocusMove::Prev,_=>return};
                    popup.focus_move(direction);cx.prevent_default();cx.stop_propagation();
                }) {
                    for option in move || FORMATS.into_iter().enumerate(), key=|option:&(usize,(ColorFormat,&str))|option.0 {
                        control(tabindex=Focus::Sequential,
                            class:selected=move ||model.format.get()==option.1.0,
                            on:click=move |_|{model.format.set(option.1.0);menu.set(false);trigger.focus();}) {{option.1.1}}
                    }
                }
            }
        }
    }
}

pub const STYLE: &str = include_str!("style.css");
