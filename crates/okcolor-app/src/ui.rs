use crate::eyedropper::EyedropperProps;
use crate::menu::MenuBandProps;
use crate::tiles::{ColorTileProps, Source, TileGestures};
use crate::tooltip::Tooltips;
use crate::updates::{UpdatePromptProps, Updates};
use crate::{
    model::{PickerModel, stepped_channel},
    surface::{ColorControlProps, SurfaceKind},
    window_frame::ResizeEdgesProps,
};
use crate::{
    swatches::SwatchesProps,
    tools_ui::{ContrastProps, HarmoniesProps},
    workspace::Tab,
};
use okcolor_core::{ColorFormat, Gamut, GamutMapping, Oklch};
use zgui::reactive::UnsyncCallback;
use zgui::{geom::CssPx, prelude::*, reactive::RenderEffect, vocab::ScrollPhase};

#[derive(Clone, Copy)]
pub(crate) struct InputExit(pub NodeRef);

/// Stable handles also let application tests exercise the actual mounted controls.
#[derive(Clone, Copy)]
pub struct PickerRefs {
    pub tool_page: NodeRef,
    pub page_content: NodeRef,
    pub settings_content: NodeRef,
    pub settings_viewport: NodeRef,
    pub settings_thumb: NodeRef,
    pub status_notice: NodeRef,
    pub eyedropper: NodeRef,
    pub tooltip: NodeRef,
    pub palette_adjust: NodeRef,
    pub adjustment_menu: NodeRef,
    pub adjustment_apply: NodeRef,
    pub adjustment_cancel: NodeRef,
    pub adjustment_rows: [NodeRef; 4],
    pub palette_dot: NodeRef,
    pub similar: NodeRef,
    pub title: NodeRef,
    pub tabs: [NodeRef; 3],
    pub library: [NodeRef; 8],
    pub folder_menu: NodeRef,
    pub format_menu: NodeRef,
    pub format_trigger: NodeRef,
    pub tools: [NodeRef; 8],
    pub group_toggles: [NodeRef; 3],
    pub apca_reset: NodeRef,
    pub current_lc: [NodeRef; 2],
    pub topmost: NodeRef,
    pub sampling: RwSignal<bool>,
    pub menu: NodeRef,
    pub gamut_toggles: [NodeRef; 2],
    pub axis_trigger: NodeRef,
    pub axis_options: [NodeRef; 3],
    pub always_snap: NodeRef,
    pub companion: NodeRef,
    pub csp_pull: NodeRef,
    pub csp_send: NodeRef,
    pub maximize: NodeRef,
    pub minimize: NodeRef,
    pub close: NodeRef,
    pub caption: RwSignal<crate::window_frame::CaptionVisual>,
    pub recents: NodeRef,
    pub swatches: [NodeRef; 2],
    pub fallback_samples: [NodeRef; 2],
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
            page_content: NodeRef::new(),
            settings_content: NodeRef::new(),
            settings_viewport: NodeRef::new(),
            settings_thumb: NodeRef::new(),
            status_notice: NodeRef::new(),
            eyedropper: NodeRef::new(),
            tooltip: NodeRef::new(),
            palette_adjust: NodeRef::new(),
            adjustment_menu: NodeRef::new(),
            adjustment_apply: NodeRef::new(),
            adjustment_cancel: NodeRef::new(),
            adjustment_rows: std::array::from_fn(|_| NodeRef::new()),
            palette_dot: NodeRef::new(),
            similar: NodeRef::new(),
            title: NodeRef::new(),
            tabs: std::array::from_fn(|_| NodeRef::new()),
            folder_menu: NodeRef::new(),
            format_menu: NodeRef::new(),
            format_trigger: NodeRef::new(),
            library: std::array::from_fn(|_| NodeRef::new()),
            group_toggles: std::array::from_fn(|_| NodeRef::new()),
            apca_reset: NodeRef::new(),
            current_lc: std::array::from_fn(|_| NodeRef::new()),
            tools: std::array::from_fn(|_| NodeRef::new()),
            topmost: NodeRef::new(),
            sampling: RwSignal::new(false),
            menu: NodeRef::new(),
            axis_trigger: NodeRef::new(),
            axis_options: std::array::from_fn(|_| NodeRef::new()),
            always_snap: NodeRef::new(),
            gamut_toggles: std::array::from_fn(|_| NodeRef::new()),
            companion: NodeRef::new(),
            csp_pull: NodeRef::new(),
            csp_send: NodeRef::new(),
            maximize: NodeRef::new(),
            minimize: NodeRef::new(),
            close: NodeRef::new(),
            caption: RwSignal::new(crate::window_frame::CaptionVisual::default()),
            recents: NodeRef::new(),
            fallback_samples: std::array::from_fn(|_| NodeRef::new()),
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
    let gestures = TileGestures::mount(
        refs.tool_page,
        refs.folder_menu,
        refs.adjustment_menu,
        model,
    );
    let app_root = NodeRef::new();
    provide_context(InputExit(app_root));
    let app_size = app_root.observe_border_size();
    let maximized = use_window().maximized();
    let scale = use_window().scale();
    let tips = crate::tooltip::Tooltips::mount();
    for (node, label) in [
        (refs.swap, "Swap colors"),
        (refs.tabs[0], "Picker · Ctrl+1"),
        (refs.tabs[1], "Harmony · Ctrl+2"),
        (refs.tabs[2], "APCA · Ctrl+3"),
    ] {
        crate::tooltip::Tooltips::bind(node, move || label.into());
    }
    let clipboard = use_clipboard();
    let settings = RwSignal::new(false);
    let updates = Updates::mount(model);
    let present_update = Memo::new(move |_| {
        updates.visible.get()
            && !model.editing_text.get()
            && !model.dragging.get()
            && gestures.dragging.get().is_none()
            && !refs.sampling.get()
    });
    Tooltips::bind(refs.menu, move || {
        if settings.get_untracked() {
            "Close settings"
        } else {
            "Settings"
        }
        .into()
    });
    #[cfg(windows)]
    if let Some(bridge) =
        use_context::<StoredValue<crate::windows_caption::CaptionBridge, LocalStorage>>()
    {
        bridge.with_value(|bridge| bridge.bind_content(refs, model));
    }
    let shortcut = move |cx: &mut EventCx<'_, events::KeyDown>| {
        if present_update.get_untracked() || refs.adjustment_menu.get_untracked().is_some() {
            return;
        }
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
            if refs.axis_options[0].window_bounds().is_some() {
                return;
            }
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
                Some("l" | "L") => Some(refs.numbers[0]),
                Some("c" | "C") => Some(refs.numbers[1]),
                Some("h" | "H") => Some(refs.numbers[2]),
                Some("a" | "A") => Some(refs.numbers[3]),
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
    let shortcuts = RenderEffect::new(move |_| {
        app_root.get()?;
        Some(app_root.window_root()?.listen(
            events::KEY_DOWN,
            ListenerOptions::CAPTURE,
            shortcut.clone(),
        ))
    });
    on_cleanup_local(move || drop(shortcuts));
    view! {
        column(class="app",class:expanded-chart=move ||maximized.get() && model.tab.get()==Tab::Picker, class:dragging=move ||gestures.dragging.get().is_some(),node_ref=app_root,tabindex=Focus::Programmatic) {
            if move ||present_update.get() {UpdatePrompt(state=updates,model=model)} else {
            Titlebar(refs=refs,settings=settings)
            if move ||settings.get() {
                Settings(model=model,refs=refs)
            } else {
                column(class="scroll-body",node_ref=refs.tool_page) {
                column(class="page-content",node_ref=refs.page_content,style:width=move ||app_size.get().map(|size|format!("{}px",(size.width.0/scale.get()-40.0).max(0.0)))) {
                    RecentColors(model=model,node=refs.recents)
                    if move ||model.tab.get()==Tab::Picker {
                        GamutPanel(model=model,refs=refs)
                    }
                    ColorPanel(model=model,refs=refs)
                    if move ||model.tab.get()==Tab::Harmonies {Harmonies(model=model,refs=refs)}
                    if move ||model.tab.get()==Tab::Apca {Contrast(model=model,refs=refs)}
                column(class="formats") {
                    CodeField(model=model,refs=refs,node=refs.codes[0],primary=true)
                    CodeField(model=model,refs=refs,node=refs.codes[1],primary=false)
                }
                    Swatches(model=model,refs=refs)
                }
                }
            }
            Statusbar(model=model,refs=refs,settings=settings)
            if move ||!refs.sampling.get() && gestures.dragging.get().is_none() && !tips.text.get().is_empty() {
                row(class="tooltip",node_ref=refs.tooltip,style:left=move ||Some(format!("{}px",tips.position.get().0)),style:top=move ||Some(format!("{}px",tips.position.get().1))) {
                    for part in move ||crate::tooltip::segments(&tips.text.get()),key=|part:&(bool,String)|part.clone() {
                        text(class:tooltip-hex=part.0) {{part.1.clone()}}
                    }
                }
            }
            ResizeEdges()
            if move ||refs.sampling.get() {
                Eyedropper(model=model,refs=refs)
            }
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
    let reset = RenderEffect::new(move |previous: Option<Vec<Oklch>>| {
        let recent = model.recent.get();
        if previous
            .as_ref()
            .is_some_and(|old| recent.first() != old.first() && recent.len() >= old.len())
        {
            node.scroll_to(
                ScrollTarget::Offset(Point::new(DevicePx(0.0), DevicePx(0.0))),
                ScrollBehavior::Instant,
            );
        }
        recent
    });
    on_cleanup_local(move || drop(reset));
    view! {
        row(class="recent-strip") {
            box(class="recent-gutter") {
                if move ||position.get().offset.x.0>0.0 {
                    control(class="recent-nav",node_ref=Tooltips::button(NodeRef::new(),"Newer recent colors"),tabindex=Focus::Sequential,a11y:label="Newer recent colors",
                        on:click=move |_|shift(-1.0)) {box(class="history-arrow") {}}
                }
            }
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
            box(class="recent-gutter") {
                if move ||{let pos=position.get(); pos.content_size.width.0-pos.scrollport.width.0-pos.offset.x.0>0.5} {
                    control(class="recent-nav",node_ref=Tooltips::button(NodeRef::new(),"Older recent colors"),tabindex=Focus::Sequential,a11y:label="Older recent colors",
                        on:click=move |_|shift(1.0)) {box(class="history-arrow redo") {}}
                }
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
    view! {
        box(class="slot-layer",class:main=index==0,class:sub=index==1) {
            ColorTile(node=refs.swatches[index],class="slot-tile",color=color,source=Source::Sample,destination=None,
                label=Memo::new(move |_|{
                    let c=color.get();let label=if index==0 {"Main color"}else{"Sub color"};
                    let hex=okcolor_core::format_color(c,ColorFormat::Hex,model.mapping.get()).expect("validated color").text;
                    match model.display.get().unavailable(c) {
                        Some(reason)=>format!("{label} {hex} · {reason}"),None=>format!("{label} {hex}")
                    }
                }),
                activate=UnsyncCallback::new(move |()|model.select_slot(index as u8)))

        }
    }
}

#[component]
fn Fallbacks(model: PickerModel, refs: PickerRefs) -> impl IntoView {
    let color = Memo::new(move |_| model.color());
    let unavailable = Memo::new(move |_| model.display.get().unavailable(color.get()).is_some());
    let distinct = Memo::new(move |_| {
        if !unavailable.get() {
            return false;
        }
        let displayed = |method| {
            let rgb = color.get().mapped_rgb(Gamut::Srgb, method).ok()?;
            Oklch::from_rgb(
                rgb.map(|v| (v * 255.0).round() / 255.0),
                Gamut::Srgb,
                1.0,
                0.0,
            )
            .ok()
        };
        displayed(GamutMapping::LocalMinde)
            .zip(displayed(GamutMapping::Clip))
            .is_some_and(|(a, b)| okcolor_core::delta_e_2000(a, b) >= 5.0)
    });
    view! {
        if move ||unavailable.get() {
            column(class="fallback-pair",class:single=move ||!distinct.get(),class:sub-fallback=move ||model.selected.get()==1) {
                FallbackSample(model=model,color=color,method=GamutMapping::LocalMinde,name="Mapped sRGB",
                    icon=include_str!("../assets/monitor.svg"),node=refs.fallback_samples[0])
                if move ||distinct.get() { FallbackSample(model=model,color=color,method=GamutMapping::Clip,name="Clipped sRGB",
                    icon=include_str!("../assets/cube.svg"),node=refs.fallback_samples[1]) }
            }
        }
    }
}

#[component]
fn FallbackSample(
    model: PickerModel,
    color: Memo<Oklch>,
    method: GamutMapping,
    name: &'static str,
    icon: &'static str,
    node: NodeRef,
) -> impl IntoView {
    let sample = Memo::new(move |_| {
        let original = color.get();
        let [_, _, h, a] = original.components();
        original
            .mapped_rgb(Gamut::Srgb, method)
            .and_then(|rgb| Oklch::from_rgb(rgb, Gamut::Srgb, a, h))
            .unwrap_or(original)
    });
    let label = Memo::new(move |_| {
        let hex = okcolor_core::format_color(sample.get(), ColorFormat::Hex, method)
            .expect("valid sRGB fallback")
            .text;
        format!("{name} {hex}")
    });
    let button = NodeRef::new();
    Tooltips::bind(button, move || label.get_untracked());
    let select = move || {
        model.select(sample.get_untracked());
    };
    view! {row(class="fallback-row") {
        control(class="fallback-button",node_ref=button,tabindex=Focus::Sequential,a11y:label=move ||label.get(),on:click=move |_|select()) {
            {zgui::elements::vector().document(icon).class("fallback-icon")}
        }
        ColorTile(node=node,class="fallback-tile",source=Source::Sample,destination=None,
            color=sample,label=label,activate=UnsyncCallback::new(move |()|select()))
    }}
}

/// Close each popup on Escape or an outside press and restore keyboard focus.
pub(crate) fn dismiss_menu(open: RwSignal<bool>, popup: NodeRef, trigger: NodeRef) {
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
    let legend = NodeRef::new();
    let legend_size = legend.observe_border_size();
    let plot_size = refs.slice.observe_border_size();
    let scale = use_window().scale();
    let near_legend = move || {
        let Some((legend, plot)) = legend_size.get().zip(plot_size.get()) else {
            return false;
        };
        plot.width.0 * (SurfaceKind::Slice.marker(model)[0].clamp(0.0, 1.0) as f32)
            < legend.width.0 + 10.0 * scale.get()
    };
    view! {
                column(class="plot-section") {
                    ColorControl(model=model,kind=SurfaceKind::Slice,node=refs.slice)
                    row(class="plot-caption") {
                        row(class="legend-group",node_ref=legend,class:right=near_legend) {
                            text(class="legend") {{move || {
                                let c=model.color();
                                if c.in_gamut(Gamut::Srgb) {"sRGB"} else if c.in_gamut(Gamut::DisplayP3) {"P3"}
                                else if c.in_gamut(Gamut::Rec2020) {"Rec.2020"} else {"Out of gamut"}
                            }}}
                        }
                        text(class="axis-c",style:left=move ||Some(format!("{}%",SurfaceKind::Slice.marker(model)[0].clamp(0.01,0.99)*100.0))) {{move ||if model.tools.get().fixed_axis==crate::workspace::FixedAxis::Hue {"C"} else {"H"}}}
                    }
                    for axis in move ||[model.tools.get().fixed_axis],key=|axis:&crate::workspace::FixedAxis|axis.view_name() {
                        ColorControl(model=model,kind=axis.bar(),node=refs.bars[axis.bar() as usize-1])
                    }
                    if move ||model.tools.get().show_alpha_slider {ColorControl(model=model,kind=SurfaceKind::Alpha,node=refs.bars[3])}
                }
    }
}

#[component]
fn ColorPanel(model: PickerModel, refs: PickerRefs) -> impl IntoView {
    let eyedropper = refs.eyedropper;
    crate::tooltip::Tooltips::bind(eyedropper, || "Eyedropper · Ctrl+Shift+C".into());
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
                            Fallbacks(model=model,refs=refs)
                            control(class="eyedropper",node_ref=eyedropper,tabindex=Focus::Sequential,a11y:label="Sample a screen color (Ctrl+Shift+C)",
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
                class:open=move ||settings.get(),a11y:label=move ||if settings.get() {"Close settings"} else {"Settings"},on:pointer_down=window.no_drag_handler(),
                on:click=move |_|settings.update(|open|*open = !*open)) {
                box(class="menu-line first") {} box(class="menu-line second") {}
                {zgui::elements::vector().document(include_str!("../assets/menu-close.svg")).class("menu-close")}
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
    use crate::workspace::FixedAxis;
    let level_window = StoredValue::new_local(use_window());
    let menu = RwSignal::new(false);
    let popup = NodeRef::new();
    let trigger = refs.axis_trigger;
    dismiss_menu(menu, popup, trigger);
    let viewport = refs.settings_viewport;
    let scroll = viewport.observe_scroll();
    let scale = use_window().scale();
    let thumb = move || {
        let pos = scroll.get();
        let page = pos.scrollport.height.0;
        let total = pos.content_size.height.0;
        if page <= 0.0 || total <= page + 0.5 {
            return None;
        }
        let size = (page * page / total).max(10.0 * scale.get()).min(page);
        Some((
            (page - size) * pos.offset.y.0 / (total - page) / scale.get(),
            size / scale.get(),
        ))
    };
    let grab = RwSignal::new(None::<(f32, f32)>);
    let set_offset = move |y: f32| {
        viewport.scroll_to(
            ScrollTarget::Offset(zgui::geom::Point::new(
                zgui::geom::DevicePx(0.0),
                zgui::geom::DevicePx(y),
            )),
            ScrollBehavior::Instant,
        )
    };
    view! {
        column(class="settings-overlay",node_ref=refs.settings_content) {
        column(class="settings-viewport",node_ref=viewport,on:wheel=move |cx|{
            let dy=cx.delta.to_pixels(CssPx(22.0)).height.0;
            set_offset(scroll.get_untracked().offset.y.0+dy*scale.get_untracked());
            cx.stop_propagation();cx.prevent_default();
        }) {
        column(class="body settings") {
            SettingToggle(label="Always on top",node=refs.topmost,checked=Memo::new(move |_|model.tools.get().always_on_top),
                toggle=UnsyncCallback::new(move |()|{
                    model.tools.update(|tools|tools.always_on_top = !tools.always_on_top);
                    level_window.with_value(|window|window.set_level(model.tools.get_untracked().window_level()));
                }))
            column(class="settings-group") {
                SettingToggle(label="Show Rec.2020",node=refs.gamut_toggles[1],checked=Memo::new(move |_|model.show_rec2020.get()),
                    toggle=UnsyncCallback::new(move |()|model.toggle_gamut(Gamut::Rec2020)))
                SettingToggle(label="Show P3",node=refs.gamut_toggles[0],checked=Memo::new(move |_|model.show_p3.get()),
                    toggle=UnsyncCallback::new(move |()|model.toggle_gamut(Gamut::DisplayP3)))
            }
            column(class="settings-group") {
            column(class="folder-anchor") {
                control(class="setting-row folder-heading",class:open=move ||menu.get(),node_ref=trigger,tabindex=Focus::Sequential,
                    a11y:label="Gamut chart view",on:click=move |_|menu.update(|v| *v = !*v)) {
                    text {"Gamut"} spacer() text {{move ||model.tools.get().fixed_axis.view_name()}}
                    box(class="folder-chevron") {{zgui::elements::vector().document(include_str!("../assets/chevron.svg")).class("chevron")}}
                }
                if move ||menu.get() {
                    MenuBand(page=viewport,trigger=trigger,node=popup) {
                        for axis in move ||[FixedAxis::Hue,FixedAxis::Lightness,FixedAxis::Chroma],key=|axis:&FixedAxis|axis.view_name() {
                            control(node_ref=refs.axis_options[axis.bar() as usize-1],class:selected=move ||model.tools.get().fixed_axis==axis,tabindex=Focus::Sequential,
                                on:click=move |_|{model.tools.update(|t|t.fixed_axis=axis);menu.set(false);trigger.focus();}) {text {{axis.view_name()}}}
                        }
                    }
                }
            }
            SettingToggle(label="Always snap to gamut boundaries",node=refs.always_snap,checked=Memo::new(move |_|model.tools.get().always_snap),
                toggle=UnsyncCallback::new(move |()|model.tools.update(|t|t.always_snap = !t.always_snap)))
            SettingToggle(label="Show alpha slider",node=NodeRef::new(),checked=Memo::new(move |_|model.tools.get().show_alpha_slider),
                toggle=UnsyncCallback::new(move |()|model.tools.update(|t|t.show_alpha_slider = !t.show_alpha_slider)))
            }
        }
        }
        if move ||thumb().is_some() {
            control(class="settings-thumb",node_ref=refs.settings_thumb,class:held=move ||grab.get().is_some(),a11y:label="Scroll settings",
                style:top=move ||thumb().map(|(y,_)|format!("{y}px")),
                style:height=move ||thumb().map(|(_,h)|format!("{h}px")),
                on:pointer_down=move |cx|{if cx.button==Some(PointerButton::Primary){
                    grab.set(Some((cx.position.y.0,scroll.get_untracked().offset.y.0)));cx.capture_pointer();}},
                on:pointer_move=move |cx|{if let Some((start,offset))=grab.get_untracked() && let Some((_,height))=thumb(){
                    let pos=scroll.get_untracked();let page=pos.scrollport.height.0;
                    let travel=page/scale.get_untracked()-height;
                    if travel>0.0 {set_offset(offset+(cx.position.y.0-start)*(pos.content_size.height.0-page)/travel);}
                }},
                on:pointer_up=move |cx|{grab.set(None);cx.release_pointer();},on:pointer_cancel=move |_|grab.set(None)) {}
        }
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
    view! {control(class="setting-row",node_ref=Tooltips::button(node,label),tabindex=Focus::Sequential,a11y:label=label,on:click=move |_|toggle.run(())) {
        text {{label}} spacer()
        box(class="setting-check",class:checked=move ||checked.get()) {{zgui::elements::vector().document(include_str!("../assets/check.svg")).class("check-icon")}}
    }}
}

fn transient_message(source: impl Fn() -> String + 'static) -> RwSignal<String> {
    let text = RwSignal::new(String::new());
    let timers = zgui::view::Timers::current().expect("picker window");
    let effect = RenderEffect::new(move |_| {
        let message = source();
        let visible = !message.is_empty();
        text.set(message);
        visible.then(|| {
            timers.set_timeout(std::time::Duration::from_secs(4), move || {
                text.set(String::new())
            })
        })
    });
    on_cleanup_local(move || drop(effect));
    text
}

#[component]
fn Statusbar(model: PickerModel, refs: PickerRefs, settings: RwSignal<bool>) -> impl IntoView {
    let notification = refs.status_notice;
    use okcolor_csp::worker::State;
    let failure = transient_message(move || match model.companion.state.get() {
        State::Failed(error) => error,
        _ => String::new(),
    });
    let hint = transient_message(move || model.companion.hint.get());
    let message = transient_message(move || model.message.get());
    let storage = transient_message(move || model.storage_message.get());
    let status = move || {
        [storage.get(), message.get(), failure.get(), hint.get()]
            .into_iter()
            .find(|s| !s.is_empty())
            .unwrap_or_else(|| model.display.get().label().into())
    };
    let failed = move || !failure.get().is_empty();
    crate::tooltip::Tooltips::bind(refs.companion, move || {
        let hint = model.companion.hint.get_untracked();
        if failed() {
            "Dismiss connection error".into()
        } else if model.companion.active() && !hint.is_empty() {
            format!("Disconnect CSP · {hint}")
        } else if model.companion.active() {
            "Disconnect CSP".into()
        } else {
            model.companion.label().into()
        }
    });
    crate::tooltip::Tooltips::bind(notification, status);
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
                class:active=move ||model.tab.get()==entry.1.0,
                on:click=move |_|{settings.set(false);switch_tab(model,refs,entry.1.0);}) {
                {zgui::elements::vector().document(entry.1.2).class("status-icon")}
            }
        }
        if move ||model.companion.can_resolve.get() {
            row(class="status-center") {
                control(node_ref=Tooltips::button(refs.csp_pull,"Use CSP color"),tabindex=Focus::Sequential,on:click=move |_|model.companion.use_csp()) {"Use CSP color"}
                control(node_ref=Tooltips::button(refs.csp_send,"Send current color to CSP"),tabindex=Focus::Sequential,on:click=move |_|model.companion.send_current()) {"Send this color"}
            }
        } else {
            text(class="status-center",node_ref=notification) {{status}}
        }
        if move ||!model.storage_message.get().is_empty() && model.can_save.get() {
            control(node_ref=Tooltips::button(NodeRef::new(),"Retry saving"),tabindex=Focus::Sequential,on:click=move |_|model.save_request.update(|r|*r=r.wrapping_add(1))) {"Retry"}
        }
        control(class="csp-connect",node_ref=refs.companion,tabindex=Focus::Sequential,
            class:warning=failed,class:linked=move ||model.companion.state.get()==State::Connected,
            a11y:label=move ||if failed() {"Dismiss connection error"} else if model.companion.active() {"Disconnect CSP"} else {model.companion.label()},
            on:click=move |_|{if failed() || model.companion.active(){model.companion.disconnect();}else{model.companion.scan();}}) {
            if move ||failed() || matches!(model.companion.state.get(),State::Scanning|State::Connecting|State::Reconnecting) {
                {zgui::elements::vector().document(include_str!("../assets/menu-close.svg")).class("status-icon")}
            } else { if move ||model.companion.state.get()==State::Connected {
                {zgui::elements::vector().document(include_str!("../assets/link.svg")).class("status-icon")}
            } else {
                {zgui::elements::vector().document(include_str!("../assets/unlinked.svg")).class("status-icon")}
            }}
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
    let exit = expect_context::<InputExit>().0;
    let bar = NodeRef::new();
    let focused = RwSignal::new(false);
    let draft = RwSignal::new(String::new());
    let dirty = RwSignal::new(false);
    let composing = RwSignal::new(false);
    let wheel_remainder = RwSignal::new((Modifiers::NONE, 0.0_f64));
    let value = [model.l, model.c, model.h, model.alpha][index];
    let display = move |value: f64| {
        let v = if index == 3 { value * 100.0 } else { value };
        let rounded = format!("{v:.4}");
        let text = rounded.trim_end_matches('0').trim_end_matches('.');
        if index == 3 {
            format!("{text}%")
        } else {
            text.to_owned()
        }
    };
    let parse = move |text: &str| {
        let text = text.trim();
        if index == 3 {
            text.trim_end_matches('%')
                .trim()
                .parse::<f64>()
                .map(|v| v / 100.0)
        } else {
            text.parse::<f64>()
        }
    };
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
        match parse(&draft.get_untracked()) {
            Ok(value) => {
                if model.set_channel(index, value) {
                    let text = display(model.current().components()[index]);
                    draft.set(text.clone());
                    number.set_value(&text);
                    dirty.set(false);
                }
            }
            Err(_) => model.message.set("Enter a finite number".into()),
        }
    };
    let adjust = move |steps: f64, modifiers: Modifiers, add_recent: bool| {
        let mut channels = model.current().components();
        if dirty.get_untracked() {
            let Ok(base) = parse(&draft.get_untracked()) else {
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
            bar.focus();
        }
        if active {
            let mut channels = model.current().components();
            channels[index] =
                stepped_channel(index, base, (f64::from(dx) / 4.0).trunc(), cx.modifiers);
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
        row(class="channel",node_ref=bar,tabindex=Focus::Programmatic,on:key_down=move |cx|{
            if cx.key==Key::Named(NamedKey::Escape) && gesture.get_untracked().is_some(){
                cancel_number();cx.release_pointer();cx.prevent_default();cx.stop_propagation();
            }
        }) {
            text(class="channel-label") {{label}}
            field(class="number",class:editing=move ||focused.get(),node_ref=number,a11y:label=label,
                on:pointer_down=move |cx| {
                    if focused.get_untracked() || !cx.primary || cx.button!=Some(PointerButton::Primary) || composing.get_untracked() { return; }
                    let mut channels=model.current().components();
                    if dirty.get_untracked() {
                        let Ok(base)=parse(&draft.get_untracked()) else { return; };
                        channels[index]=base;
                        if Oklch::new(channels[0],channels[1],channels[2],channels[3]).is_err() { return; }
                    }
                    gesture.set(Some((cx.position.x.0,channels[index],false)));
                    cx.capture_pointer();cx.prevent_default();cx.stop_propagation();
                },
                on:pointer_move=move_number,
                on:pointer_up=move |cx| {
                    if cx.button!=Some(PointerButton::Primary) { return; }
                    cx.release_pointer();
                    if let Some((_,_,active))=gesture.get_untracked() {
                        gesture.set(None);
                        if active { model.commit();bar.focus(); } else { number.focus(); number.select_all(); }
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
                    if cx.key==Key::Named(NamedKey::Enter) {commit();dirty.set(false);exit.focus();cx.prevent_default();cx.stop_propagation();}
                    if cx.key == Key::Named(NamedKey::Escape) {
                        let text = display(value.get_untracked());
                        dirty.set(false);
                        draft.set(text.clone());
                        number.set_value(&text);
                        model.message.set(String::new());
                        exit.focus();cx.stop_propagation();cx.prevent_default();
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
pub(crate) fn CopyButton(
    text: UnsyncCallback<(), String>,
    compact: bool,
    label: &'static str,
) -> impl IntoView {
    let clipboard = use_clipboard();
    let copied = RwSignal::new(false);
    let clock = zgui::view::Timers::current().expect("mounted window");
    let timer = StoredValue::new_local(None::<zgui::view::TimeoutHandle>);
    let asset = if compact {
        include_str!("../assets/copy-small.svg")
    } else {
        include_str!("../assets/copy.svg")
    };
    view! {control(class="copy",node_ref=Tooltips::button(NodeRef::new(),label),class:compact=compact,tabindex=Focus::Sequential,a11y:label=label,
        on:click=move |_|{
            clipboard.set_text(ClipboardKind::Standard,text.run(()));
            copied.set(true);
            timer.set_value(Some(clock.set_timeout(std::time::Duration::from_millis(850),move ||copied.set(false))));
        }) {
        if move ||!copied.get() {{zgui::elements::vector().document(asset).class("copy-icon")}}
        box(class="tile-check",class:visible=move ||copied.get()) {{zgui::elements::vector().document(include_str!("../assets/check.svg")).class("check-icon")}}
    }}
}

#[component]
fn CodeField(model: PickerModel, refs: PickerRefs, node: NodeRef, primary: bool) -> impl IntoView {
    let exit = expect_context::<InputExit>().0;
    let focused = RwSignal::new(false);
    let menu = RwSignal::new(false);
    let popup = refs.format_menu;
    let trigger = refs.format_trigger;
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
    let selected_format = move || {
        if primary {
            ColorFormat::Oklch
        } else {
            model.format.get_untracked()
        }
    };
    let commit = move || {
        if dirty.get_untracked() && model.input(&draft.get_untracked()) {
            let formatted = model.output_untracked(selected_format());
            draft.set(formatted.clone());
            node.set_value(&formatted);
            dirty.set(false);
        }
    };
    view! {
        row(class="code-row") {
            text(class="channel-label") {{if primary {"O"}else{"R"}}}
            row(class="code-box") {
                field(class="code",node_ref=node,on:pointer_down=move |cx|{if cx.button==Some(PointerButton::Primary) && !focused.get_untracked(){node.focus();node.select_all();cx.prevent_default();}},a11y:label=if primary {"OKLCH color"}else{"Converted color"},
                    on:focus_in=move |_|{focused.set(true);model.editing_text.set(true);node.select_all();},
                    on:focus_out=move |_|{focused.set(false);model.editing_text.set(false);},
                    on:input=move |cx|{if let Some(v)=cx.payload().as_value(){draft.set(v.value.to_string());dirty.set(true);}},
                on:ime_start=move |_|composing.set(true),
                on:ime_preedit=move |_|composing.set(true),
                on:ime_commit=move |_|composing.set(false),
                on:ime_end=move |_|composing.set(false),
                    on:change=move |_|commit(),
                    on:key_down=move |cx|{
                        if composing.get_untracked() { return; }
                        if cx.key==Key::Named(NamedKey::Enter) {commit();dirty.set(false);exit.focus();cx.prevent_default();cx.stop_propagation();}
                        if cx.key == Key::Named(NamedKey::Escape) {
                            let text = model.output_untracked(selected_format());
                            dirty.set(false);
                            draft.set(text.clone());
                            node.set_value(&text);
                            model.message.set(String::new());
                            exit.focus();cx.stop_propagation();cx.prevent_default();
                        }}) {{draft.get_untracked()}}
                if move || !primary {
                    control(class="format-trigger",class:open=move ||menu.get(),
                        node_ref=Tooltips::button(trigger,"Choose output format"),
                        tabindex=Focus::Sequential,
                        a11y:label="Choose output format",
                        on:click=move |_|menu.update(|v|*v = !*v)) {{zgui::elements::vector().document(include_str!("../assets/chevron.svg")).class("chevron")}}
                }
            }
            CopyButton(text=UnsyncCallback::new(move |()|model.output_untracked(selected_format())),compact=false,label="Copy color")
            if move ||menu.get() {
                MenuBand(page=refs.tool_page,trigger=trigger,node=popup) {
                column(on:key_down=move |cx| {
                    let direction=match cx.key {Key::Named(NamedKey::ArrowDown)=>FocusMove::Next,Key::Named(NamedKey::ArrowUp)=>FocusMove::Prev,_=>return};
                    popup.focus_move(direction);cx.prevent_default();cx.stop_propagation();
                }) {
                    for option in move || FORMATS.into_iter().enumerate(), key=|option:&(usize,(ColorFormat,&str))|option.0 {
                        control(tabindex=Focus::Sequential,
                            class:selected=move ||model.format.get()==option.1.0,
                            on:click=move |_|{model.format.set(option.1.0);menu.set(false);trigger.focus();}) {text {{option.1.1}}}
                    }
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
