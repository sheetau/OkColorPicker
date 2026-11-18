use crate::tiles::{ColorTileProps, Source, TileGestures};
use crate::{
    model::{PickerModel, stepped_channel},
    surface::{ColorControlProps, SurfaceKind},
    window_frame::ResizeEdgesProps,
};
use crate::{
    tools_ui::{ContrastProps, HarmoniesProps, SwatchesProps},
    workspace::Tab,
};
use okcolor_core::{ColorFormat, Gamut, GamutMapping, Oklch};
use zgui::reactive::UnsyncCallback;
use zgui::{geom::CssPx, prelude::*, reactive::RenderEffect, vocab::ScrollPhase};

/// Stable handles also let application tests exercise the actual mounted controls.
#[derive(Clone, Copy)]
pub struct PickerRefs {
    pub tool_page: NodeRef,
    pub title: NodeRef,
    pub tabs: [NodeRef; 3],
    pub library: [NodeRef; 8],
    pub tools: [NodeRef; 8],
    pub topmost: NodeRef,
    pub sampling: RwSignal<bool>,
    pub menu: NodeRef,
    pub gamut_toggles: [NodeRef; 2],
    pub companion: NodeRef,
    pub csp_pull: NodeRef,
    pub csp_send: NodeRef,
    pub maximize: NodeRef,
    pub minimize: NodeRef,
    pub close: NodeRef,
    pub caption: RwSignal<crate::window_frame::CaptionVisual>,
    pub recents: NodeRef,
    pub swatches: [NodeRef; 2],
    pub swap: NodeRef,
    pub slice: NodeRef,
    pub bars: [NodeRef; 4],
    pub numbers: [NodeRef; 4],
    pub codes: [NodeRef; 2],
}
impl Default for PickerRefs {
    fn default() -> Self {
        Self {
            tool_page: NodeRef::new(),
            title: NodeRef::new(),
            tabs: std::array::from_fn(|_| NodeRef::new()),
            library: std::array::from_fn(|_| NodeRef::new()),
            tools: std::array::from_fn(|_| NodeRef::new()),
            topmost: NodeRef::new(),
            sampling: RwSignal::new(false),
            menu: NodeRef::new(),
            gamut_toggles: std::array::from_fn(|_| NodeRef::new()),
            companion: NodeRef::new(),
            csp_pull: NodeRef::new(),
            csp_send: NodeRef::new(),
            maximize: NodeRef::new(),
            minimize: NodeRef::new(),
            close: NodeRef::new(),
            caption: RwSignal::new(crate::window_frame::CaptionVisual::default()),
            recents: NodeRef::new(),
            swatches: std::array::from_fn(|_| NodeRef::new()),
            swap: NodeRef::new(),
            slice: NodeRef::new(),
            bars: std::array::from_fn(|_| NodeRef::new()),
            numbers: std::array::from_fn(|_| NodeRef::new()),
            codes: std::array::from_fn(|_| NodeRef::new()),
        }
    }
}

#[component]
pub fn Picker(model: PickerModel, refs: PickerRefs) -> impl IntoView {
    provide_context(model);
    TileGestures::mount(refs.tool_page);
    let app_root = NodeRef::new();
    let app_size = app_root.observe_border_size();
    let scale = use_window().scale();
    let tips = crate::tooltip::Tooltips::mount();
    for (node, label) in [
        (refs.menu, "Settings"),
        (refs.swap, "Swap colors"),
        (refs.tabs[0], "Picker · Ctrl+1"),
        (refs.tabs[1], "Harmony · Ctrl+2"),
        (refs.tabs[2], "APCA · Ctrl+3"),
    ] {
        crate::tooltip::Tooltips::bind(node, move || label.into());
    }
    crate::tooltip::Tooltips::bind(refs.companion, move || {
        let hint = model.companion.hint.get_untracked();
        if model.companion.active() && !hint.is_empty() {
            format!("{} · {hint}", model.companion.label())
        } else {
            model.companion.label().into()
        }
    });
    let clipboard = use_clipboard();
    let settings = RwSignal::new(false);
    let shortcut = move |cx: &mut EventCx<'_, events::KeyDown>| {
        if cx.key == Key::Named(NamedKey::Escape) && refs.sampling.get_untracked() {
            refs.sampling.set(false);
            cx.prevent_default();
            return;
        }
        if cx.modifiers == (Modifiers::CONTROL | Modifiers::SHIFT)
            && cx
                .key
                .as_str()
                .is_some_and(|key| key.eq_ignore_ascii_case("c"))
        {
            refs.sampling.set(true);
            cx.prevent_default();
            cx.stop_propagation();
            return;
        }
        if cx.key == Key::Named(NamedKey::Escape) && settings.get_untracked() {
            settings.set(false);
            refs.menu.focus();
            cx.prevent_default();
            return;
        }
        if cx.modifiers == Modifiers::NONE
            && !model.editing_text.get_untracked()
            && !settings.get_untracked()
        {
            let field = match cx.key.as_str() {
                Some("o" | "O") => Some(refs.codes[0]),
                Some("r" | "R") => Some(refs.codes[1]),
                _ => None,
            };
            if let Some(field) = field {
                field.focus();
                field.select_all();
                cx.prevent_default();
                cx.stop_propagation();
                return;
            }
        }
        if !cx.modifiers.contains(Modifiers::CONTROL) {
            return;
        }
        let tab = if cx.key == Key::Named(NamedKey::Tab) {
            Some(
                model
                    .tab
                    .get_untracked()
                    .next(cx.modifiers.contains(Modifiers::SHIFT)),
            )
        } else {
            match cx.key.as_str() {
                Some("1") => Some(Tab::Picker),
                Some("2") => Some(Tab::Harmonies),
                Some("3") => Some(Tab::Apca),
                _ => None,
            }
        };
        if let Some(tab) = tab {
            settings.set(false);
            switch_tab(model, refs, tab);
            cx.prevent_default();
            cx.stop_propagation();
            return;
        }
        if model.editing_text.get_untracked() {
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
        column(class="app",node_ref=app_root,on:key_down=shortcut) {
            Titlebar(refs=refs,settings=settings)
            if move ||settings.get() {
                Settings(model=model,refs=refs)
            } else {
                column(class="scroll-body",node_ref=refs.tool_page) {
                column(class="page-content",style:width=move ||app_size.get().map(|size|format!("{}px",(size.width.0/scale.get()-40.0).max(0.0)))) {
                    RecentColors(model=model,node=refs.recents)
                    if move ||model.tab.get()==Tab::Picker {
                        GamutPanel(model=model,refs=refs)
                    }
                    ColorPanel(model=model,refs=refs)
                    if move ||model.tab.get()==Tab::Harmonies {Harmonies(model=model,refs=refs)}
                    if move ||model.tab.get()==Tab::Apca {Contrast(model=model,refs=refs)}
                column(class="formats") {
                    CodeField(model=model,node=refs.codes[0],primary=true)
                    CodeField(model=model,node=refs.codes[1],primary=false)
                }
                    Swatches(model=model,refs=refs)
                }
                }
            }
            Statusbar(model=model,refs=refs,settings=settings)
            if move ||!tips.text.get().is_empty() {
                text(class="tooltip",style:left=move ||Some(format!("{}px",tips.position.get().0)),style:top=move ||Some(format!("{}px",tips.position.get().1))) {{move ||tips.text.get()}}
            }
            ResizeEdges()
            if move ||refs.sampling.get() {
                box(class="eyedropper-overlay",a11y:label="Select a color in this window. Escape cancels.",
                    on:pointer_down=move |cx|{
                        if cx.button==Some(PointerButton::Primary) {
                            if let Some(color)=screen_color(model.h.get_untracked()) {model.select(color);}
                            else {model.message.set("Could not sample the screen pixel".into());}
                        }
                        refs.sampling.set(false);
                        cx.stop_propagation();
                        cx.prevent_default();
                    }) {}
            }
        }
    }
}

#[component]
pub(crate) fn RecentColors(model: PickerModel, node: NodeRef) -> impl IntoView {
    use zgui::geom::{CssPx, DevicePx, Point};
    let position = node.observe_scroll();
    let scale = use_window().scale();
    let shift = move |direction: f32| {
        let extent = node.scroll_position().scrollport.width.0;
        node.scroll_to(
            ScrollTarget::By(Point::new(
                DevicePx(direction * extent * 0.75),
                DevicePx(0.0),
            )),
            ScrollBehavior::Instant,
        );
    };
    let reset = RenderEffect::new(move |_| {
        model.recent.track();
        node.scroll_to(
            ScrollTarget::Offset(Point::new(DevicePx(0.0), DevicePx(0.0))),
            ScrollBehavior::Instant,
        );
    });
    on_cleanup_local(move || drop(reset));
    view! {
        box(class="recent-strip") {
            row(class="recents",node_ref=node,a11y:label="Recent colors",on:wheel=move |cx| {
                let delta=cx.delta.to_pixels(CssPx(22.0));
                let amount=if delta.width.0.abs()>delta.height.0.abs() {delta.width.0} else {delta.height.0};
                node.scroll_to(ScrollTarget::By(Point::new(DevicePx(amount*scale.get_untracked()),DevicePx(0.0))),ScrollBehavior::Instant);
                cx.prevent_default();cx.stop_propagation();
            }) {
                if move ||model.recent.get().is_empty() {text(class="empty-recent") {"Recent colors"}}
                for entry in move ||model.recent.get(), key=|entry:&Oklch|entry.components().map(f64::to_bits) {
                    ColorTile(class="recent",node=NodeRef::new(),color=Memo::new(move |_|entry),source=Source::Recent,destination=None,
                        label=Memo::new(move |_|okcolor_core::format_color(entry,ColorFormat::Hex,model.mapping.get()).expect("finite color").text),
                        activate=UnsyncCallback::new(move |()|{model.select(entry);}))

                }
            }
            if move ||position.get().content_size.width.0>position.get().scrollport.width.0 {
                control(class="recent-nav newer",tabindex=Focus::Sequential,a11y:label="Newer recent colors",
                    disabled=move ||position.get().offset.x.0<=0.0,on:click=move |_|shift(-1.0)) {box(class="history-arrow") {}}
                control(class="recent-nav older",tabindex=Focus::Sequential,a11y:label="Older recent colors",
                    disabled=move ||position.get().is_at_end_horizontally(),on:click=move |_|shift(1.0)) {box(class="history-arrow redo") {}}
            }
        }
    }
}

// This pinned zgui revision can retain stale hit order after restacking overlapping
// controls. Remount only this two-swatch branch when selection changes; ordinary
// color edits still update the existing paint handles without rebuilding the view.
#[component]
pub(crate) fn SlotSwatch(model: PickerModel, refs: PickerRefs, index: usize) -> impl IntoView {
    let color = Memo::new(move |_| model.slots[index].get());
    let unavailable =
        Memo::new(move |_| !model.display.get().wide_output && !color.get().in_gamut(Gamut::Srgb));
    let fallback = move |method| {
        let original = color.get();
        let [_, _, h, a] = original.components();
        original
            .mapped_rgb(Gamut::Srgb, method)
            .and_then(|rgb| Oklch::from_rgb(rgb, Gamut::Srgb, a, h))
            .unwrap_or(original)
    };
    view! {
        box(class="slot-layer",class:main=index==0,class:sub=index==1) {
            ColorTile(node=refs.swatches[index],class="slot-tile",color=color,source=Source::Sample,destination=None,
                label=Memo::new(move |_|if index==0 {"Main color".into()}else{"Sub color".into()}),
                activate=UnsyncCallback::new(move |()|model.select_slot(index as u8)))
            if move ||unavailable.get() && model.selected.get() as usize==index {
                column(class="fallback-pair") {
                    for entry in move ||[(0,GamutMapping::LocalMinde,"Mapped sRGB"),(1,GamutMapping::Clip,"Clipped sRGB")],key=|entry:&(u8,GamutMapping,&str)|entry.0 {
                        ColorTile(node=NodeRef::new(),class="fallback-tile",source=Source::Sample,destination=None,
                            color=Memo::new(move |_|fallback(entry.1)),label=Memo::new(move |_|entry.2.to_string()),
                            activate=UnsyncCallback::new(move |()|{model.select(fallback(entry.1));}))
                    }
                }
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn screen_color(hue: f64) -> Option<Oklch> {
    use std::ptr::null_mut;
    use windows_sys::Win32::{
        Foundation::POINT,
        Graphics::Gdi::{GetDC, GetPixel, ReleaseDC},
        UI::WindowsAndMessaging::GetCursorPos,
    };
    let mut point = POINT::default();
    // The picker window is per-monitor DPI aware, so these desktop coordinates match the DC.
    unsafe {
        if GetCursorPos(&mut point) == 0 {
            return None;
        }
        let dc = GetDC(null_mut());
        if dc.is_null() {
            return None;
        }
        let pixel = GetPixel(dc, point.x, point.y);
        ReleaseDC(null_mut(), dc);
        if pixel == u32::MAX {
            return None;
        }
        let rgb = [pixel & 255, (pixel >> 8) & 255, (pixel >> 16) & 255];
        Oklch::from_rgb(
            rgb.map(|channel| f64::from(channel) / 255.0),
            Gamut::Srgb,
            1.0,
            hue,
        )
        .ok()
    }
}

#[cfg(not(target_os = "windows"))]
fn screen_color(_: f64) -> Option<Oklch> {
    None
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
fn GamutPanel(model: PickerModel, refs: PickerRefs) -> impl IntoView {
    view! {
                column(class="plot-section") {
                    ColorControl(model=model,kind=SurfaceKind::Slice,node=refs.slice)
                    row(class="plot-caption") {
                        row(class="legend-group",class:right=move ||model.c.get()/model.max_chroma.get()<0.32) {
                            text(class="legend srgb") {"sRGB"}
                            if move ||model.show_p3.get() {text(class="legend p3") {"P3"}}
                            if move ||model.show_rec2020.get() {text(class="legend rec2020") {"Rec.2020"}}
                        }
                        text(class="axis-c",style:left=move ||Some(format!("{}%",(model.c.get()/model.max_chroma.get()).clamp(0.01,0.99)*100.0))) {"C"}
                    }
                    ColorControl(model=model,kind=SurfaceKind::Hue,node=refs.bars[2])
                    if move ||model.tools.get().show_alpha_slider {ColorControl(model=model,kind=SurfaceKind::Alpha,node=refs.bars[3])}
                }
    }
}

#[component]
fn ColorPanel(model: PickerModel, refs: PickerRefs) -> impl IntoView {
    view! {
                row(class="adjustments") {
                    column(class="preview-column") {
                        box(class="swatches") {
                            if move ||model.selected.get()==0 {
                                SlotSwatch(model=model,refs=refs,index=1)
                                SlotSwatch(model=model,refs=refs,index=0)
                            } else {
                                SlotSwatch(model=model,refs=refs,index=0)
                                SlotSwatch(model=model,refs=refs,index=1)
                            }
                            control(class="eyedropper",tabindex=Focus::Sequential,a11y:label="Sample a window color (Ctrl+Shift+C)",
                                on:click=move |_|refs.sampling.set(true)) {
                                {zgui::elements::vector().document(include_str!("../assets/eyedropper.svg")).class("eyedropper-icon")}
                            }
                            control(class="swap-colors",node_ref=refs.swap,tabindex=Focus::Sequential,a11y:label="Swap main and sub colors",
                                on:pointer_down=move |_|refs.swap.focus(),
                                on:click=move |_|model.swap()) {
                                {zgui::elements::vector().document(include_str!("../assets/swap.svg")).class("swap-icon")}
                            }
                        }
                        spacer()
                        row(class="quick-colors") {
                            for index in move ||0..3,key=|index:&usize|*index {
                                ColorTile(node=NodeRef::new(),class=if index==0{"transparent-color"}else{"preset-color"},source=Source::Sample,destination=None,
                                    color=Memo::new(move |_|{
                                        let [l,c,h,_]=model.color().components();
                                        if index==0 {Oklch::new(l,c,h,0.0)}else{Oklch::new(if index==1{0.0}else{1.0},0.0,h,1.0)}
                                            .expect("valid preset color")
                                    }),label=Memo::new(move |_|["Transparent","Black","White"][index].into()),
                                    activate=UnsyncCallback::new(move |()|{match index {
                                        0=>{model.set_channel(3,0.0);},
                                        1=>{model.input("#000");},
                                        _=>{model.input("#fff");}
                                    }}))
                            }
                        }
                    }
                    column(class="channels") {
                        Channel(model=model,index=0,label="L",number=refs.numbers[0])
                        Channel(model=model,index=1,label="C",number=refs.numbers[1])
                        Channel(model=model,index=2,label="H",number=refs.numbers[2])
                        Channel(model=model,index=3,label="A",number=refs.numbers[3])
                    }
                }
    }
}

#[component]
fn Titlebar(refs: PickerRefs, settings: RwSignal<bool>) -> impl IntoView {
    let window = use_window();
    view! {
        row(class="titlebar",on:pointer_down=window.move_drag_handler()) {
            control(class="menu-button",node_ref=refs.menu,tabindex=Focus::Sequential,
                class:open=move ||settings.get(),a11y:label="Settings",on:pointer_down=window.no_drag_handler(),
                on:click=move |_|settings.update(|open|*open = !*open)) {
                box(class="menu-line first") {} box(class="menu-line second") {}
            }
            text(class="title",node_ref=refs.title) {"OkColorPicker"}
            spacer()
            CaptionControl(index=0,node=refs.minimize,visual=refs.caption)
            CaptionControl(index=1,node=refs.maximize,visual=refs.caption)
            CaptionControl(index=2,node=refs.close,visual=refs.caption)
        }
    }
}

#[component]
fn Settings(model: PickerModel, refs: PickerRefs) -> impl IntoView {
    let level_window = StoredValue::new_local(use_window());
    view! {
        column(class="body settings") {
            SettingToggle(label="Always on top",node=refs.topmost,checked=Memo::new(move |_|model.tools.get().always_on_top),
                toggle=UnsyncCallback::new(move |()|{
                    model.tools.update(|tools|tools.always_on_top = !tools.always_on_top);
                    level_window.with_value(|window|window.set_level(model.tools.get_untracked().window_level()));
                }))
            column(class="settings-group") {
                control(class="setting-row",tabindex=Focus::Sequential,a11y:label="Toggle sRGB fallback mapping",
                    on:click=move |_|model.mapping.update(|m|*m=if *m==GamutMapping::Clip {GamutMapping::LocalMinde}else{GamutMapping::Clip})) {
                    text {"sRGB replacement"} spacer()
                    text {{move ||if model.mapping.get()==GamutMapping::Clip {"Clipped"} else {"Mapped"}}}
                }
                SettingToggle(label="Show Rec.2020",node=refs.gamut_toggles[1],checked=Memo::new(move |_|model.show_rec2020.get()),
                    toggle=UnsyncCallback::new(move |()|model.toggle_gamut(Gamut::Rec2020)))
                SettingToggle(label="Show P3",node=refs.gamut_toggles[0],checked=Memo::new(move |_|model.show_p3.get()),
                    toggle=UnsyncCallback::new(move |()|model.toggle_gamut(Gamut::DisplayP3)))
            }
            SettingToggle(label="Show alpha slider",node=NodeRef::new(),checked=Memo::new(move |_|model.tools.get().show_alpha_slider),
                toggle=UnsyncCallback::new(move |()|model.tools.update(|t|t.show_alpha_slider = !t.show_alpha_slider)))
        }
    }
}

#[component]
fn SettingToggle(
    label: &'static str,
    node: NodeRef,
    checked: Memo<bool>,
    toggle: UnsyncCallback<()>,
) -> impl IntoView {
    view! {control(class="setting-row",node_ref=node,tabindex=Focus::Sequential,a11y:label=label,on:click=move |_|toggle.run(())) {
        text {{label}} spacer()
        if move ||checked.get() {{zgui::elements::vector().document(include_str!("../assets/check.svg")).class("check-icon")}}
    }}
}

fn status_message(model: PickerModel) -> String {
    for message in [model.storage_message.get(), model.message.get()] {
        if !message.is_empty() {
            return message;
        }
    }
    if let okcolor_csp::worker::State::Failed(error) = model.companion.state.get() {
        return error;
    }
    let hint = model.companion.hint.get();
    if model.companion.active() && (hint.starts_with("Cannot") || hint.contains("exhausted")) {
        return hint;
    }
    model.display.get().label().into()
}

#[component]
fn Statusbar(model: PickerModel, refs: PickerRefs, settings: RwSignal<bool>) -> impl IntoView {
    let tabs = [
        (Tab::Picker, "Picker", include_str!("../assets/picker.svg")),
        (
            Tab::Harmonies,
            "Harmony",
            include_str!("../assets/harmonies.svg"),
        ),
        (Tab::Apca, "APCA", include_str!("../assets/apca.svg")),
    ];
    view! {row(class="status") {
        for entry in move ||tabs.into_iter().enumerate(),key=|entry:&(usize,(Tab,&str,&str))|entry.0 {
            control(class="tab-button",node_ref=refs.tabs[entry.0],tabindex=Focus::Sequential,a11y:label=entry.1.1,
                class:active=move ||model.tab.get()==entry.1.0 && !settings.get(),
                on:click=move |_|{settings.set(false);switch_tab(model,refs,entry.1.0);}) {
                {zgui::elements::vector().document(entry.1.2).class("status-icon")}
            }
        }
        if move ||model.companion.can_resolve.get() {
            row(class="status-center") {
                control(node_ref=refs.csp_pull,tabindex=Focus::Sequential,on:click=move |_|model.companion.use_csp()) {"Use CSP color"}
                control(node_ref=refs.csp_send,tabindex=Focus::Sequential,on:click=move |_|model.companion.send_current()) {"Send this color"}
            }
        } else {
            text(class="status-center") {{move ||status_message(model)}}
        }
        if move ||!model.storage_message.get().is_empty() && model.can_save.get() {
            control(tabindex=Focus::Sequential,on:click=move |_|model.save_request.update(|r|*r=r.wrapping_add(1))) {"Retry"}
        }
        control(class="csp-connect",node_ref=refs.companion,tabindex=Focus::Sequential,
            class:warning=move ||matches!(model.companion.state.get(),okcolor_csp::worker::State::Failed(_)),
            a11y:label=move ||model.companion.label(),
            on:click=move |_|{if model.companion.active(){model.companion.disconnect();}else{model.companion.scan();}}) {
            if move ||model.companion.active() {
                box(class="disconnect open") {box(class="menu-line first") {} box(class="menu-line second") {}}
            } else {
                {zgui::elements::vector().document(include_str!("../assets/link.svg")).class("status-icon")}
            }
            text {"CSP"}
        }
    }}
}

#[component]
fn CaptionControl(
    index: usize,
    node: NodeRef,
    visual: RwSignal<crate::window_frame::CaptionVisual>,
) -> impl IntoView {
    let window = use_window();
    view! {
        control(class="caption-control",node_ref=node,
            class:caption_close=index==2,
            class:caption_hover=move ||{let state=visual.get();state.enabled[index] && state.hovered==Some(index)},
            class:caption_pressed=move ||{let state=visual.get();state.enabled[index] && state.pressed==Some(index)},
            disabled=move ||!visual.get().enabled[index],
            a11y:label=move ||match index {0=>"Minimize",1 if visual.get().maximized=>"Restore",1=>"Maximize",_=>"Close"},
            on:click=move |_|match index {0=>window.minimize(),1=>window.toggle_maximized(),_=>window.close()}) {
            text(class="caption-icon") {{move ||crate::window_frame::caption_glyph(index,visual.get().maximized)}}
        }
    }
}

#[component]
fn Channel(
    model: PickerModel,
    index: usize,
    label: &'static str,
    number: NodeRef,
) -> impl IntoView {
    let bar = NodeRef::new();
    let focused = RwSignal::new(false);
    let draft = RwSignal::new(String::new());
    let dirty = RwSignal::new(false);
    let composing = RwSignal::new(false);
    let wheel_remainder = RwSignal::new((Modifiers::NONE, 0.0_f64));
    let value = [model.l, model.c, model.h, model.alpha][index];
    let display = move |value: f64| format!("{value:.4}");
    let driving = RenderEffect::new(move |_| {
        let value = value.get();
        if !focused.get() || model.dragging.get() {
            dirty.set(false);
            let text = display(value);
            draft.set(text.clone());
            number.set_value(&text);
        }
    });
    on_cleanup_local(move || drop(driving));
    let commit = move || {
        if !dirty.get_untracked() {
            return;
        }
        match draft.get_untracked().trim().parse::<f64>() {
            Ok(value) => {
                if model.set_channel(index, value) {
                    dirty.set(false);
                }
            }
            Err(_) => model.message.set("Enter a finite number".into()),
        }
    };
    let adjust = move |steps: f64, modifiers: Modifiers, add_recent: bool| {
        let mut channels = model.current().components();
        if dirty.get_untracked() {
            let Ok(base) = draft.get_untracked().trim().parse::<f64>() else {
                model.message.set("Enter a finite number".into());
                return;
            };
            channels[index] = base;
            if let Err(error) = Oklch::new(channels[0], channels[1], channels[2], channels[3]) {
                model.message.set(error.to_string());
                return;
            }
        }
        let next = stepped_channel(index, channels[index], steps, modifiers);
        if model.set_channel_with_recent(index, next, add_recent) {
            let text = display(model.current().components()[index]);
            dirty.set(false);
            draft.set(text.clone());
            number.set_value(&text);
        }
    };
    let wheel = move |cx: &mut EventCx<'_, events::Wheel>| {
        if composing.get_untracked() || model.dragging.get_untracked() {
            return;
        }
        let phase = cx.payload().as_wheel().expect("wheel payload").phase;
        let (modifiers, mut remainder) = wheel_remainder.get_untracked();
        if modifiers != cx.modifiers || phase == ScrollPhase::Started {
            remainder = 0.0;
        }
        if phase == ScrollPhase::Ended {
            wheel_remainder.set((cx.modifiers, 0.0));
        }
        let delta = cx.delta.to_pixels(CssPx(22.0));
        if delta.height.0 == 0.0 || !delta.height.0.is_finite() {
            return;
        }
        cx.prevent_default();
        cx.stop_propagation();
        if phase == ScrollPhase::Momentum {
            return;
        }
        let total = remainder - f64::from(delta.height.0) / 22.0;
        let steps = total.trunc();
        wheel_remainder.set((
            cx.modifiers,
            if phase == ScrollPhase::Ended {
                0.0
            } else {
                total - steps
            },
        ));
        if steps != 0.0 {
            adjust(steps, cx.modifiers, false);
        }
    };
    let gesture = RwSignal::new(None::<(f32, f64, bool)>);
    let move_number = move |cx: &mut EventCx<'_, events::PointerMove>| {
        let Some((start, base, mut active)) = gesture.get_untracked() else {
            return;
        };
        let dx = cx.position.x.0 - start;
        if !active && dx.abs() >= 3.0 {
            if !model.begin() {
                return;
            }
            active = true;
            number.focus();
        }
        if active {
            let mut channels = model.current().components();
            channels[index] = stepped_channel(index, base, f64::from(dx) / 4.0, cx.modifiers);
            if let Ok(color) = Oklch::new(channels[0], channels[1], channels[2], channels[3]) {
                model.preview(color);
            }
            cx.prevent_default();
            cx.stop_propagation();
        }
        gesture.set(Some((start, base, active)));
    };
    let cancel_number = move || {
        if gesture.get_untracked().is_some_and(|g| g.2) {
            model.cancel();
        }
        gesture.set(None);
        dirty.set(false);
        let text = display(model.current().components()[index]);
        draft.set(text.clone());
        number.set_value(&text);
        bar.focus();
    };
    view! {
        row(class="channel",node_ref=bar,tabindex=Focus::Programmatic) {
            text(class="channel-label") {{label}}
            field(class="number",node_ref=number,a11y:label=label,
                on:pointer_down=move |cx| {
                    if !cx.primary || cx.button!=Some(PointerButton::Primary) || composing.get_untracked() { return; }
                    let mut channels=model.current().components();
                    if dirty.get_untracked() {
                        let Ok(base)=draft.get_untracked().trim().parse::<f64>() else { return; };
                        channels[index]=base;
                        if Oklch::new(channels[0],channels[1],channels[2],channels[3]).is_err() { return; }
                    }
                    gesture.set(Some((cx.position.x.0,channels[index],false)));
                    cx.capture_pointer();cx.prevent_default();cx.stop_propagation();
                },
                on:pointer_move=move_number,
                on:pointer_up=move |cx| {
                    if cx.button!=Some(PointerButton::Primary) { return; }
                    if let Some((_,_,active))=gesture.get_untracked() {
                        gesture.set(None);cx.release_pointer();
                        if active { model.commit();bar.focus(); } else { number.focus(); }
                        cx.prevent_default();cx.stop_propagation();
                    }
                },
                on:pointer_cancel=move |_|cancel_number(),
                on:focus_in=move |_|{focused.set(true);model.editing_text.set(true);wheel_remainder.set((Modifiers::NONE,0.0));},
                on:focus_out=move |_|{focused.set(false);model.editing_text.set(false);},
                on:input=move |cx|{if let Some(v)=cx.payload().as_value(){draft.set(v.value.to_string());dirty.set(true);wheel_remainder.set((Modifiers::NONE,0.0));}},
                on:ime_start=move |_|{composing.set(true);wheel_remainder.set((Modifiers::NONE,0.0));},
                on:ime_preedit=move |_|composing.set(true),
                on:ime_commit=move |_|composing.set(false),
                on:ime_end=move |_|composing.set(false),
                on:change=move |_|commit(),
                on:wheel=wheel,
                on:key_down=move |cx|{
                    if composing.get_untracked() { return; }
                    if cx.key==Key::Named(NamedKey::Escape) && gesture.get_untracked().is_some() {
                        cancel_number();cx.release_pointer();cx.prevent_default();return;
                    }
                    wheel_remainder.set((Modifiers::NONE,0.0));
                    if matches!(cx.key, Key::Named(NamedKey::ArrowUp | NamedKey::ArrowDown)) {
                        let steps = if cx.key == Key::Named(NamedKey::ArrowUp) { 1.0 } else { -1.0 };
                        adjust(steps, cx.modifiers, true);
                        cx.prevent_default();
                        cx.stop_propagation();
                    }
                    if cx.key==Key::Named(NamedKey::Enter) {commit();cx.prevent_default();}
                    if cx.key == Key::Named(NamedKey::Escape) {
                        let text = display(value.get_untracked());
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

fn switch_tab(model: PickerModel, refs: PickerRefs, tab: Tab) {
    if model.dragging.get_untracked() {
        return;
    }
    refs.menu.focus();
    refs.tool_page.scroll_to(
        ScrollTarget::Offset(zgui::geom::Point::new(
            zgui::geom::DevicePx(0.0),
            zgui::geom::DevicePx(0.0),
        )),
        ScrollBehavior::Instant,
    );
    model.editing_text.set(false);
    model.tab.set(tab);
    model.message.set(String::new());
}
