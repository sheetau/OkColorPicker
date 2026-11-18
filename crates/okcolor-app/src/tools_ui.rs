use crate::tiles::{ColorTileProps, Destination, Source, TileGestures};
use crate::{
    model::PickerModel,
    ui::{CopyButtonProps, PickerRefs},
    workspace::Library,
};
use okcolor_core::{
    ColorFormat, Gamut, GamutMapping, Harmony, Oklch, Ramp, apca_in, contrast_sample, format_color,
    harmony, parse_color, ramp_steps,
};
use zgui::reactive::UnsyncCallback;
use zgui::{prelude::*, reactive::RenderEffect};

fn library_edit(model: PickerModel, f: impl FnOnce(&mut Library) -> Result<(), &'static str>) {
    let mut next = model.library.get_untracked();
    match f(&mut next) {
        Ok(()) => {
            model.library.set(next);
            model.message.set(String::new());
        }
        Err(message) => model.message.set(message.into()),
    }
}

/// Drafts stay local; buttons explicitly apply them. Unmounting cannot leave text focus latched.
#[component]
fn Entry(
    model: PickerModel,
    node: NodeRef,
    draft: RwSignal<String>,
    label: &'static str,
) -> impl IntoView {
    let focused = RwSignal::new(false);
    let effect = RenderEffect::new(move |_| {
        let text = draft.get();
        if !focused.get() {
            node.set_value(&text);
        }
    });
    on_cleanup_local(move || {
        drop(effect);
        model.editing_text.set(false);
    });
    view! {field(class="tool-entry",node_ref=node,a11y:label=label,
    on:focus_in=move |_|{focused.set(true);model.editing_text.set(true);},on:focus_out=move |_|{focused.set(false);model.editing_text.set(false);},
    on:input=move |cx|{if let Some(value)=cx.payload().as_value(){draft.set(value.value.to_string());}}) {{draft.get_untracked()}}}
}

#[component]
pub(crate) fn Swatches(model: PickerModel, refs: PickerRefs) -> impl IntoView {
    let selected = RwSignal::new(None::<u64>);
    let destination = RwSignal::new(None::<u64>);
    let editor = RwSignal::new(false);
    let draft = RwSignal::new(String::new());
    let menu = RwSignal::new(false);
    let trigger = refs.library[1];
    let popup = refs.folder_menu;
    crate::ui::dismiss_menu(menu, popup, trigger);
    let scroll = refs.tool_page.observe_scroll();
    let size = refs.tool_page.observe_border_size();
    let scale = use_window().scale();
    let placement = Memo::new(move |_| {
        scroll.get();
        size.get();
        if !menu.get() {
            return (false, 0.0);
        }
        let (Some(button), Some(page)) = (trigger.window_bounds(), refs.tool_page.window_bounds())
        else {
            return (false, 0.0);
        };
        let below =
            (page.origin.y.0 + page.size.height.0 - button.origin.y.0 - button.size.height.0)
                .max(0.0);
        let above = (button.origin.y.0 - page.origin.y.0).max(0.0);
        let above = above / scale.get();
        let below = below / scale.get();
        let height = (model.library.get().folders.len() + 2) as f32 * 22.0 + 10.0;
        let up = below < height && above > below;
        (up, if up { above } else { below })
    });
    view! {
        column(class="swatch-library") {
            box(class="folder-anchor") {
                control(class="folder-heading",node_ref=refs.library[1],tabindex=Focus::Sequential,
                    a11y:label="Choose swatch folder",on:click=move |_|menu.update(|v|*v = !*v)) {
                    text {{move ||model.library.get().folders.iter().find(|f|Some(f.id)==destination.get()).map(|f|f.name.clone()).unwrap_or_else(||"Swatch Tiles".into())}}
                    spacer() box(class="folder-chevron") {box(class="chevron") {}}
                }
                if move ||menu.get() {
                    column(class="folder-menu",node_ref=popup,class:above=move ||placement.get().0,style:max-height=move ||Some(format!("{}px",placement.get().1))) {
                        control(tabindex=Focus::Sequential,on:click=move |_|{destination.set(None);selected.set(None);menu.set(false);}) {"Swatch Tiles"}
                        for id in move ||model.library.get().folders.iter().map(|f|f.id).collect::<Vec<_>>(),key=|id:&u64|*id {
                            control(tabindex=Focus::Sequential,on:click=move |_|{destination.set(Some(id));selected.set(Some(id));menu.set(false);}) {
                                {move ||model.library.get().folders.iter().find(|f|f.id==id).map(|f|f.name.clone()).unwrap_or_default()}
                            }
                        }
                        control(tabindex=Focus::Sequential,on:click=move |_|{editor.set(true);menu.set(false);draft.set(String::new());}) {"New folder / Edit"}
                    }
                }
            }
            for folder in move ||[destination.get()],key=|folder:&Option<u64>|*folder {
                Tiles(model=model,folder=folder,refs=refs,selected=selected,draft=draft)
            }
            if move ||editor.get() {
                column(class="library-editor") {
                    Entry(model=model,node=refs.library[2],draft=draft,label="Swatch or folder name")
                    row(class="tool-actions") {
                        control(class="tool-action",node_ref=refs.library[3],tabindex=Focus::Sequential,disabled=move ||selected.get().is_none(),on:click=move |_| {
                            if let Some(id)=selected.get_untracked(){library_edit(model,|library|library.rename(id,&draft.get_untracked()));}
                        }) {"Rename"}
                        control(class="tool-action",node_ref=refs.library[4],tabindex=Focus::Sequential,on:click=move |_|library_edit(model,|library|{
                            let name=draft.get_untracked();let name=if name.trim().is_empty(){"New folder"}else{&name};
                            let id=library.create_folder(name)?;destination.set(Some(id));selected.set(Some(id));draft.set(name.into());Ok(())
                        })) {"New folder"}
                        control(class="tool-action",node_ref=refs.library[5],tabindex=Focus::Sequential,disabled=move ||selected.get().is_none(),on:click=move |_| {
                            if let Some(id)=selected.get_untracked(){library_edit(model,|library|{library.remove(id);Ok(())});selected.set(None);if destination.get_untracked()==Some(id){destination.set(None);}}
                        }) {"Remove"}
                    }
                    text(class="tool-hint") {"Destination · select a tile, then Move"}
                    row(class="tool-actions wrap") {
                        control(class="tool-action",class:active=move ||destination.get().is_none(),tabindex=Focus::Sequential,on:click=move |_|destination.set(None)) {"Unfiled"}
                        for id in move ||model.library.get().folders.iter().map(|f|f.id).collect::<Vec<_>>(),key=|id:&u64|*id {
                            control(class="tool-action",class:active=move ||destination.get()==Some(id),tabindex=Focus::Sequential,on:click=move |_|destination.set(Some(id))) {
                                {move ||model.library.get().folders.iter().find(|f|f.id==id).map(|f|f.name.clone()).unwrap_or_default()}
                            }
                        }
                    }
                    row(class="tool-actions") {
                        control(class="tool-action",node_ref=refs.library[6],tabindex=Focus::Sequential,
                            disabled=move ||!model.library.get().swatches.iter().any(|s|Some(s.id)==selected.get()),
                            on:click=move |_|{if let Some(id)=selected.get_untracked(){library_edit(model,|library|library.move_swatch(id,destination.get_untracked()));}}) {"Move"}
                        control(class="tool-action",tabindex=Focus::Sequential,on:click=move |_|editor.set(false)) {"Close editor"}
                    }
                    text(class="tool-hint") {"Removing a folder keeps its colors unfiled."}
                }
            }
        }
    }
}

#[component]
fn Tiles(
    model: PickerModel,
    folder: Option<u64>,
    refs: PickerRefs,
    selected: RwSignal<Option<u64>>,
    draft: RwSignal<String>,
) -> impl IntoView {
    let end = Destination::Library(folder, None);
    let gestures = TileGestures::current();
    gestures.register(refs.library[0], end);
    view! {
        row(class="swatch-tiles",node_ref=refs.library[7]) {
            for id in move ||model.library.get().swatches.iter().filter(|s|s.folder==folder).map(|s|s.id).collect::<Vec<_>>(),key=|id:&u64|*id {
                ColorTile(class="library-tile",node=NodeRef::new(),source=Source::Library(id),destination=Some(Destination::Library(folder,Some(id))),
                    color=Memo::new(move |_|model.library.get().swatches.iter().find(|s|s.id==id).map(|s|s.color()).unwrap_or(model.current())),
                    label=Memo::new(move |_|model.library.get().swatches.iter().find(|s|s.id==id).map(|s|s.name.clone()).unwrap_or_default()),
                    activate=UnsyncCallback::new(move |()|{if let Some(swatch)=model.library.get_untracked().swatches.iter().find(|s|s.id==id){model.select(swatch.color());selected.set(Some(id));draft.set(swatch.name.clone());}}))
            }
            control(class="tool-icon add-tile",node_ref=refs.library[0],tabindex=Focus::Sequential,
                a11y:label="Save current color",on:click=move |_|library_edit(model,|library|{
                    let id=library.add(model.current(),folder)?;selected.set(Some(id));draft.set(format!("Color {id}"));Ok(())
                })) {{zgui::elements::vector().document(include_str!("../assets/add.svg")).class("add-icon")} if move ||gestures.hovering.get()==Some(end) {box(class="insertion-line") {}}}
        }
    }
}

#[component]
fn Generated(
    model: PickerModel,
    name: &'static str,
    colors: Memo<Vec<Oklch>>,
    node: NodeRef,
) -> impl IntoView {
    view! {row(class="generated-colors",class:similar=name=="Similar",node_ref=node) {
        for index in move ||0..colors.get().len(),key=|index:&usize|*index {
            ColorTile(class="generated-color",node=NodeRef::new(),source=Source::Sample,destination=None,
                color=Memo::new(move |_|colors.get().get(index).copied().unwrap_or(model.current())),
                label=Memo::new(move |_|{let c=colors.get().get(index).copied().unwrap_or(model.current());if name=="Similar"{swatch_name(c)}else{format!("{name} {}",hex(c))}}),
                activate=UnsyncCallback::new(move |()|{if let Some(color)=colors.get_untracked().get(index){model.select(*color);}}))

        }
    }}
}

fn swatch_name(color: Oklch) -> String {
    let hex = format_color(
        color,
        ColorFormat::Hex,
        okcolor_core::GamutMapping::LocalMinde,
    )
    .map_or_else(|_| String::new(), |value| value.text);
    let mut rest = okcolor_core::color_name(color);
    let mut name = String::new();
    // CSS spells compound keywords without spaces, e.g. lightsteelblue.
    const WORDS: &[&str] = &[
        "medium",
        "light",
        "dark",
        "pale",
        "deep",
        "alice",
        "antique",
        "blanched",
        "blue",
        "burly",
        "cadet",
        "cornflower",
        "corn",
        "dodger",
        "floral",
        "forest",
        "ghost",
        "green",
        "honey",
        "hot",
        "indian",
        "lavender",
        "lawn",
        "lemon",
        "lime",
        "midnight",
        "mint",
        "misty",
        "navajo",
        "old",
        "olive",
        "orange",
        "papaya",
        "peach",
        "powder",
        "rebecca",
        "rosy",
        "royal",
        "saddle",
        "sandy",
        "sea",
        "sky",
        "slate",
        "spring",
        "steel",
        "white",
        "yellow",
    ];
    while !rest.is_empty() {
        let word = WORDS
            .iter()
            .copied()
            .find(|part| rest.starts_with(part))
            .unwrap_or(rest);
        if !name.is_empty() {
            name.push(' ');
        }
        name.push_str(&word[..1].to_ascii_uppercase());
        name.push_str(&word[1..]);
        rest = &rest[word.len()..];
    }
    format!("{name} {hex}")
}

#[component]
fn HoverLabel(heading: crate::tiles::TileHeading) -> impl IntoView {
    let parts = Memo::new(move |_| {
        let label = heading.0.get();
        label
            .rsplit_once(' ')
            .map(|(name, value)| (name.to_owned(), value.to_owned()))
            .unwrap_or((label, String::new()))
    });
    view! {row(class="hover-label"){text {{move ||parts.get().0}} text(class="hover-value") {{move ||parts.get().1}}}}
}

#[component]
fn RelatedGroup(model: PickerModel, refs: PickerRefs, group: usize) -> impl IntoView {
    let open = RwSignal::new(true);
    let title = ["Variations", "Similar", "Harmony"][group];
    let heading = crate::tiles::TileHeading::mount();
    view! {column(class="tool-section") {
        row(class="section-heading variation-heading") {
            control(tabindex=Focus::Sequential,on:click=move |_|open.update(|v|*v = !*v)) {
                if move ||open.get() {{zgui::elements::vector().document(include_str!("../assets/minus.svg")).class("fold-icon")}}
                else {{zgui::elements::vector().document(include_str!("../assets/add.svg")).class("fold-icon")}}
                text {{title}}
            }
            spacer()
            HoverLabel(heading=heading)
        }
        if move ||open.get() {
            if move ||group<2 {
                for kind in move ||if group==0 {vec![Ramp::Shades,Ramp::Tints,Ramp::Tones,Ramp::Temperatures]}else{vec![Ramp::Similar]},key=|kind:&Ramp|*kind as u8 {
                    Generated(model=model,name=match kind {Ramp::Shades=>"Shades",Ramp::Tints=>"Tints",Ramp::Tones=>"Tones",Ramp::Temperatures=>"Temperature",Ramp::Similar=>"Similar"},node=if kind==Ramp::Shades {refs.tools[7]}else{NodeRef::new()},
                        colors=Memo::new(move |_|if kind==Ramp::Similar {okcolor_core::similar_named(model.palette_source())} else {ramp_steps::<13>(model.palette_source(),kind,model.gamut.get()).to_vec()}))
                }
            } else {
                row(class="harmony-combinations") {
                    for kind in move ||Harmony::ALL,key=|kind:&Harmony|*kind as u8 {
                        box(class="combination",style:width=match kind {Harmony::Complementary=>"40px",Harmony::Tetradic|Harmony::Square=>"80px",_=>"60px"}) {
                            Generated(model=model,name=kind.label(),node=NodeRef::new(),colors=Memo::new(move |_|harmony(model.palette_source(),kind,model.gamut.get())))
                        }
                    }
                }
            }
        }
    }}
}

fn generate_palette(model: PickerModel, seed: u32, old: &[(Oklch, bool)]) -> Vec<(Oklch, bool)> {
    let mut state = seed;
    let mut next = || {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        f64::from(state) / (f64::from(u32::MAX) + 1.0)
    };
    let gamut = model.gamut.get_untracked();
    let locks: Vec<_> = old
        .iter()
        .filter(|(_, locked)| *locked)
        .map(|(c, _)| *c)
        .collect();
    let bases = if locks.is_empty() {
        vec![
            Oklch::new(
                0.42 + 0.38 * next(),
                0.06 + 0.14 * next(),
                360.0 * next(),
                1.0,
            )
            .expect("bounded palette base"),
        ]
    } else {
        locks
    };
    let scheme = Harmony::ALL[(next() * Harmony::ALL.len() as f64) as usize];
    let anchors: Vec<_> = bases
        .iter()
        .flat_map(|source| harmony(*source, scheme, gamut))
        .collect();
    let count = if old.is_empty() { 5 } else { old.len() };
    (0..count)
        .map(|i| {
            if let Some(entry) = old.get(i).filter(|(_, locked)| *locked) {
                return *entry;
            }
            let anchor = anchors[(next() * anchors.len() as f64) as usize];
            let [l, c, h, a] = anchor.components();
            let tone = (i as f64 + next()) / count as f64;
            let color = Oklch::new(
                (0.3 * l + 0.08 + 0.64 * tone).clamp(0.1, 0.96),
                c * (0.45 + 0.85 * next()),
                h + (next() - 0.5) * 18.0,
                a,
            )
            .expect("bounded palette color");
            (color.reduce_chroma(gamut), false)
        })
        .collect()
}

const MAX_PALETTE: usize = 10;

fn insert_palette_colors(
    colors: &mut Vec<(Oklch, bool)>,
    index: usize,
    requested: usize,
    edge: Oklch,
) {
    let count = requested.min(MAX_PALETTE.saturating_sub(colors.len()));
    let index = index.min(colors.len());
    let left = index.checked_sub(1).map_or(edge, |i| colors[i].0);
    let right = colors.get(index).map_or(edge, |entry| entry.0);
    let [l, c, h, a] = left.components();
    let [r, k, j, b] = right.components();
    let delta = (j - h + 180.0).rem_euclid(360.0) - 180.0;
    let ramp = (1..=count).map(|step| {
        let t = step as f64 / (count + 1) as f64;
        let color = Oklch::new(
            l + (r - l) * t,
            c + (k - c) * t,
            h + delta * t,
            a + (b - a) * t,
        )
        .expect("interpolated palette color");
        (color, false)
    });
    colors.splice(index..index, ramp);
}

#[component]
fn PaletteInsert(model: PickerModel, index: usize, node: NodeRef) -> impl IntoView {
    let gestures = TileGestures::current();
    let tail = index == usize::MAX;
    let count = RwSignal::new(1usize);
    let timer = StoredValue::new_local(None::<zgui::view::IntervalHandle>);
    let clock = StoredValue::new_local(zgui::view::Timers::current().expect("picker window"));
    view! {control(class=if tail {"palette-tail"}else{"palette-insert"},node_ref=node,tabindex=Focus::Sequential,
        a11y:label=if tail {"Append palette color"}else{"Insert palette colors"},
        on:pointer_down=move |cx|{
            if cx.button!=Some(PointerButton::Primary){return;}
            count.set(1);
            timer.set_value(Some(clock.with_value(|clock|clock.set_interval(std::time::Duration::from_millis(450),move ||{
                let max=MAX_PALETTE.saturating_sub(gestures.palette.get_untracked().len()).max(1);
                count.update(|n|*n=(*n%max)+1);
            }))));
        },
        on:pointer_up=move |_|timer.set_value(None),
        on:pointer_leave=move |_|{timer.set_value(None);count.set(1);},
        on:pointer_cancel=move |_|{timer.set_value(None);count.set(1);},
        on:click=move |_|{
            gestures.palette.update(|v|insert_palette_colors(v,index,count.get_untracked(),model.palette_source_untracked()));
            count.set(1);
        }) {
        if move ||count.get()==1 {{zgui::elements::vector().document(include_str!("../assets/add.svg")).class("add-icon")}}
        else {text(class="insert-count") {{move ||count.get().to_string()}}}
        if move ||tail && gestures.hovering.get()==Some(Destination::Palette(usize::MAX)) {box(class="insertion-line") {}}
    }}
}

#[component]
fn Palette(model: PickerModel, refs: PickerRefs) -> impl IntoView {
    let gestures = TileGestures::current();
    let heading = crate::tiles::TileHeading::mount();

    let tail = refs.tools[2];
    let end = Destination::Palette(usize::MAX);
    gestures.register(tail, end);
    let regenerate = move || {
        use std::hash::{BuildHasher, RandomState};
        // RandomState supplies independent session entropy without a platform RNG dependency.
        let entropy = RandomState::new().hash_one(()) as u32;
        gestures.palette.set(generate_palette(
            model,
            entropy,
            &gestures.palette.get_untracked(),
        ));
    };
    if gestures.palette.get_untracked().is_empty() {
        regenerate();
    }
    let shortcut = RenderEffect::new(move |_| {
        refs.tools[0].get()?;
        Some(refs.tools[0].window_root()?.listen(
            events::KEY_DOWN,
            ListenerOptions::CAPTURE,
            move |cx| {
                if cx.modifiers == Modifiers::NONE
                    && !model.editing_text.get_untracked()
                    && (cx.key == Key::Named(NamedKey::Space) || cx.key.as_str() == Some(" "))
                {
                    regenerate();
                    cx.prevent_default();
                    cx.stop_propagation();
                }
            },
        ))
    });
    on_cleanup_local(move || drop(shortcut));
    view! {column(class="tool-section palette-section") {
        row(class="section-heading") {
            text {"Palettes"} spacer() HoverLabel(heading=heading)
            control(node_ref=refs.tools[0],tabindex=Focus::Sequential,class="generate-button",a11y:label="Generate palette",on:click=move |_|regenerate()) {{zgui::elements::vector().document(include_str!("../assets/generate.svg")).class("generate-icon")}}
        }
        row(class="palette-strip",node_ref=refs.tools[5]) {
            for i in move ||0..gestures.palette.get().len(),key=|i:&usize|*i {
                column(class="palette-cell") {
                    ColorTile(class="palette-color",node=NodeRef::new(),source=Source::Palette(i),destination=Some(Destination::Palette(i)),
                        color=Memo::new(move |_|gestures.palette.get().get(i).map(|v|v.0).unwrap_or(model.current())),
                        label=Memo::new(move |_|gestures.palette.get().get(i).map(|v|swatch_name(v.0)).unwrap_or_default()),
                        activate=UnsyncCallback::new(move |()|{if let Some(v)=gestures.palette.get_untracked().get(i){model.select(v.0);}}))
                    PaletteInsert(model=model,index=i,node=NodeRef::new())
                    control(class="palette-lock",style:color=move ||gestures.palette.get().get(i).map(|v|crate::tiles::contrast_ink(v.0).to_owned()),node_ref=if i==0{refs.tools[6]}else{NodeRef::new()},tabindex=Focus::Sequential,a11y:label="Toggle color lock",
                        class:locked=move ||gestures.palette.get().get(i).is_some_and(|v|v.1),
                        on:click=move |_|gestures.palette.update(|v|{if let Some(entry)=v.get_mut(i){entry.1 = !entry.1;}})) {box(class="lock-dot") {}}
                }
            }
            PaletteInsert(model=model,index=usize::MAX,node=tail)
        }
    }}
}

#[component]
pub(crate) fn Harmonies(model: PickerModel, refs: PickerRefs) -> impl IntoView {
    view! {column(class="tool-sections") {
        for index in move ||0..3,key=|i:&usize|*i {RelatedGroup(model=model,refs=refs,group=index)}
        Palette(model=model,refs=refs)
    }}
}

fn grid_reference(settings: &crate::workspace::ToolSettings, dark: bool) -> Oklch {
    if !dark {
        return settings.background();
    }
    let [l, c, h, a] = settings.contrast_grid.dark_reference;
    Oklch::new(l, c, h, a).expect("validated contrast reference")
}

fn label_ink(preferred: Oklch, background: Oklch, gamut: Gamut) -> String {
    if apca_in(preferred, background, GamutMapping::Clip, gamut).is_ok_and(|lc| lc.abs() >= 45.0) {
        hex(preferred)
    } else {
        crate::tiles::contrast_ink(background).to_owned()
    }
}

fn hex(color: Oklch) -> String {
    format_color(color, ColorFormat::Hex, GamutMapping::LocalMinde)
        .map(|v| v.text)
        .unwrap_or_default()
}

#[component]
fn GridField(
    model: PickerModel,
    node: NodeRef,
    label: &'static str,
    value: Memo<String>,
    apply: UnsyncCallback<String>,
) -> impl IntoView {
    let editing = RwSignal::new(false);
    let composing = RwSignal::new(false);
    let draft = RwSignal::new(String::new());
    let refresh = RenderEffect::new(move |_| {
        let text = value.get();
        if !editing.get() {
            node.set_value(&text);
            draft.set(text);
        }
    });
    on_cleanup_local(move || drop(refresh));
    let dirty = RwSignal::new(false);
    let reset = move || {
        dirty.set(false);
        let formatted = value.get_untracked();
        draft.set(formatted.clone());
        node.set_value(&formatted);
    };
    let commit = move || {
        if dirty.get_untracked() {
            apply.run(draft.get_untracked());
            reset();
        }
    };
    view! {field(class="grid-field",node_ref=node,a11y:label=label,
    on:pointer_down=move |cx|{
        if cx.button==Some(PointerButton::Primary) && !editing.get_untracked() {
            node.focus();node.select_all();cx.prevent_default();
        }
    },
    on:focus_in=move |_|{editing.set(true);model.editing_text.set(true);node.select_all();},
    on:input=move |cx|{if let Some(v)=cx.payload().as_value(){draft.set(v.value.to_string());dirty.set(true);}},
    on:focus_out=move |_|{commit();editing.set(false);model.editing_text.set(false);},
    on:ime_start=move |_|composing.set(true),on:ime_preedit=move |_|composing.set(true),
    on:ime_commit=move |_|composing.set(false),on:ime_end=move |_|composing.set(false),
    on:key_down=move |cx|{
        if composing.get_untracked(){return;}
        match cx.key {
            Key::Named(NamedKey::Enter)=>commit(),
            Key::Named(NamedKey::Escape)=>reset(),
            _=>return,
        }
        node.select_all();cx.prevent_default();cx.stop_propagation();
    }) {{value.get_untracked()}}}
}

#[component]
fn ReferenceField(model: PickerModel, dark: Memo<bool>, node: NodeRef) -> impl IntoView {
    view! {GridField(model=model,node=node,label="Reference HEX",value=Memo::new(move |_|hex(grid_reference(&model.tools.get(),dark.get()))),
    apply=UnsyncCallback::new(move |value:String|{
        match parse_color(value.trim(),0.0) {
            Ok(color) if color.components()[3]==1.0 => model.tools.update(|t|if dark.get_untracked(){t.contrast_grid.dark_reference=color.components();}else{t.background=color.components();}),
            _=> model.message.set("Enter an opaque reference color".into()),
        }
    }))}
}

#[component]
pub(crate) fn Contrast(model: PickerModel, refs: PickerRefs) -> impl IntoView {
    let heading = crate::tiles::TileHeading::mount();
    let grid = Memo::new(move |_| model.tools.get().contrast_grid);
    let gamut = move || {
        if grid.get().p3 {
            Gamut::DisplayP3
        } else {
            Gamut::Srgb
        }
    };
    let candidates = Memo::new(move |_| {
        let t = model.tools.get();
        let g = &t.contrast_grid;
        g.levels
            .iter()
            .enumerate()
            .map(|(i, lc)| {
                contrast_sample(
                    model.color(),
                    grid_reference(&t, i < g.split),
                    f64::from(*lc),
                    gamut(),
                    g.to_text,
                    g.max_chroma,
                )
            })
            .collect::<Vec<_>>()
    });
    let region = refs.tools[5];
    let dragging = RwSignal::new(false);
    let scale = use_window().scale();
    let boundary = move |y: f32| {
        if let Some(b) = region.window_bounds() {
            let count = grid.get_untracked().levels.len();
            let local = (y * scale.get_untracked() - b.origin.y.0) / scale.get_untracked();
            let index = ((local - 20.0) / 40.0).round().clamp(0.0, count as f32) as usize;
            model.tools.update(|t| t.contrast_grid.split = index);
        }
    };
    let insert = move |i: usize| model.tools.update(|t| t.contrast_grid.insert(i));
    let origin = move |dark: bool| {
        let g = grid.get();
        if dark { 0 } else { g.split }
    };
    let count = move |dark: bool| {
        let g = grid.get();
        if dark {
            g.split
        } else {
            g.levels.len() - g.split
        }
    };
    let hue = Memo::new(move |_| {
        const NAMES: [&str; 36] = [
            "Flamingo",
            "Pink",
            "Red",
            "Coral",
            "Mahogany",
            "Apricot",
            "Bronze",
            "Orange",
            "Amber",
            "Mustard",
            "Yellow",
            "Turmeric",
            "Pear",
            "Avocado",
            "Lime",
            "Green",
            "Emerald",
            "Aquamarine",
            "Turquoise",
            "Cyan",
            "Electric blue",
            "Pelorus",
            "Sky",
            "Cerulean",
            "Steel",
            "Azure",
            "Blue",
            "Neon",
            "Iris",
            "Lavender",
            "Violet",
            "Purple",
            "Fuchsia",
            "Orchid",
            "Byzantium",
            "Mulberry",
        ];
        NAMES[((model.h.get() / 10.0).round() as usize) % 36]
    });
    let css = move || {
        let g = grid.get_untracked();
        let name = hue.get_untracked().to_ascii_lowercase().replace(' ', "-");
        candidates
            .get_untracked()
            .into_iter()
            .enumerate()
            .filter_map(|(i, c)| {
                let value = format_color(c?, ColorFormat::Oklch, GamutMapping::LocalMinde)
                    .ok()?
                    .text;
                Some(format!("--{name}-{}: {value};", g.level_name(i)))
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    view! {column(class="tool-section") {
        row(class="section-heading contrast-heading") {
            text {"APCA"} spacer() HoverLabel(heading=heading)
            control(class="tool-action",node_ref=refs.tools[0],tabindex=Focus::Sequential,on:click=move |_|model.tools.update(|t|t.contrast_grid.to_text = !t.contrast_grid.to_text)) {{move ||if grid.get().to_text{"To text"}else{"To bg"}}}
            control(class="tool-action",node_ref=refs.tools[1],tabindex=Focus::Sequential,on:click=move |_|model.tools.update(|t|t.contrast_grid.p3 = !t.contrast_grid.p3)) {{move ||if grid.get().p3{"P3"}else{"sRGB"}}}
            control(class="tool-action",node_ref=refs.tools[2],tabindex=Focus::Sequential,on:click=move |_|model.tools.update(|t|t.contrast_grid.max_chroma = !t.contrast_grid.max_chroma)) {{move ||if grid.get().max_chroma{"Max C"}else{"Current C"}}}
            CopyButton(text=UnsyncCallback::new(move |()|css()),compact=true,label="Copy generated CSS colors")
        }
        column(class="contrast-grid",node_ref=region) {
            box(class="grid-padding",style:background-color=move ||Some(hex(grid_reference(&model.tools.get(),grid.get().split>0)))) {}
            for i in move ||0..grid.get().levels.len(),key=|i:&usize|*i {
                row(class="contrast-row",style:background-color=move ||Some(hex(grid_reference(&model.tools.get(),i<grid.get().split))),
                    style:color=move ||Some(crate::tiles::contrast_ink(grid_reference(&model.tools.get(),i<grid.get().split)).into()),
                    on:pointer_down=move |cx|{if cx.button==Some(PointerButton::Middle){model.tools.update(|t|t.contrast_grid.remove(i));cx.prevent_default();cx.stop_propagation();}}) {
                    box(class="current-spacer") {}
                    row(class="level-info") {
                        text(class="level-number") {{move ||grid.get().level_name(i)}}
                        box(class="lc-badge",class:outline=move ||!grid.get().to_text,
                            style:background-color=move ||if grid.get().to_text{candidates.get().get(i).copied().flatten().map(hex)}else{None},
                            style:color=move ||{
                                let reference=grid_reference(&model.tools.get(),i<grid.get().split);
                                let candidate=candidates.get().get(i).copied().flatten();
                                Some(candidate.map_or_else(||crate::tiles::contrast_ink(reference).to_owned(),|c|{
                                    if grid.get().to_text {label_ink(reference,c,gamut())} else {label_ink(c,reference,gamut())}
                                }))
                            },
                            style:border-color=move ||candidates.get().get(i).copied().flatten().map(hex)) {
                            GridField(model=model,node=if i==0{refs.tools[3]}else{NodeRef::new()},label="Target contrast Lc",
                                value=Memo::new(move |_|grid.get().levels.get(i).map(u8::to_string).unwrap_or_default()),
                                apply=UnsyncCallback::new(move |v:String|{if let Some(lc)=v.trim().parse::<u8>().ok().filter(|v|*v<=110){
                                    model.tools.update(|t|{if let Some(level)=t.contrast_grid.levels.get_mut(i){*level=lc;}});
                                }else{model.message.set("Enter a contrast from 0 to 110".into());}}))
                        }
                    }
                    box(class="contrast-sample") {
                        if move ||candidates.get().get(i).is_some_and(Option::is_some) {
                            ColorTile(node=NodeRef::new(),class="candidate-color",source=Source::Sample,destination=None,
                                color=Memo::new(move |_|candidates.get().get(i).copied().flatten().unwrap_or(model.current())),
                                label=Memo::new(move |_|format!("{}-{} {}",hue.get(),grid.get().level_name(i),candidates.get().get(i).copied().flatten().map(hex).unwrap_or_default())),
                                activate=UnsyncCallback::new(move |()|{if let Some(Some(c))=candidates.get_untracked().get(i){model.select(*c);}}))
                            if move ||candidates.get().get(i).copied().flatten().is_some_and(|c|!c.in_gamut(Gamut::Srgb)){
                                text(class="p3-badge",style:color=move ||candidates.get().get(i).copied().flatten().map(|c|crate::tiles::contrast_ink(c).into())) {"P3"}
                            }
                        } else {text {"—"}}
                    }
                    control(class="contrast-insert",tabindex=Focus::Sequential,a11y:label="Insert contrast level",on:click=move |_|insert(i)) {{zgui::elements::vector().document(include_str!("../assets/add.svg")).class("add-icon")}}
                }
            }
            box(class="grid-padding",style:background-color=move ||Some(hex(grid_reference(&model.tools.get(),grid.get().split==grid.get().levels.len())))) {}
            for dark in move ||[true,false].into_iter().filter(|v|count(*v)>0).collect::<Vec<_>>(),key=|dark:&bool|*dark {
                text(class="contrast-current",class:outline=move ||!grid.get().to_text,
                    style:top=move ||Some(format!("{}px",20+40*origin(dark)+20*(count(dark).saturating_sub(1)))),
                    style:border-color=move ||Some(hex(model.color())),
                    style:color=move ||{
                        let reference=grid_reference(&model.tools.get(),dark);
                        Some(if grid.get().to_text {label_ink(reference,model.color(),gamut())} else {label_ink(model.color(),reference,gamut())})
                    },
                    style:background-color=move ||grid.get().to_text.then(||hex(model.color()))) {{move ||{
                    let reference=grid_reference(&model.tools.get(),dark);
                    let current=model.color();
                    let score=if grid.get().to_text{apca_in(reference,current,model.mapping.get(),gamut())}else{apca_in(current,reference,model.mapping.get(),gamut())};
                    score.map(|v|format!("{:.0}",v.abs())).unwrap_or_else(|_|"—".into())
                }}}
                box(class="reference-field",style:top=move ||{
                    let g=grid.get();let top=if g.split==0{10}else if dark {20+40*g.split-22}else{20+40*g.split+10};
                    Some(format!("{top}px"))},style:color=move ||Some(crate::tiles::contrast_ink(grid_reference(&model.tools.get(),dark)).into())) {
                    ReferenceField(model=model,node=if dark{refs.tools[4]}else{refs.tools[7]},dark=Memo::new(move |_|dark))
                }
            }
            if move ||dragging.get() || (grid.get().split>0 && grid.get().split<grid.get().levels.len()) {
                box(class="contrast-boundary",style:top=move ||Some(format!("{}px",20+40*grid.get().split)),
                    on:pointer_down=move |cx|{if cx.button==Some(PointerButton::Primary){dragging.set(true);cx.capture_pointer();}},
                    on:pointer_move=move |cx|{if dragging.get_untracked(){boundary(cx.position.y.0);}},
                    on:pointer_up=move |cx|{if dragging.get_untracked(){boundary(cx.position.y.0);cx.release_pointer();dragging.set(false);}},
                    on:pointer_cancel=move |_|dragging.set(false)) {}
            } else {
                control(class="restore-reference",class:at-top=move ||grid.get().split>0,node_ref=refs.tools[6],style:color=move ||Some(crate::tiles::contrast_ink(grid_reference(&model.tools.get(),grid.get().split>0)).into()),tabindex=Focus::Sequential,a11y:label="Restore both backgrounds",on:click=move |_|model.tools.update(|t|{if t.contrast_grid.levels.len()==1{t.contrast_grid.insert(1);}t.contrast_grid.split=t.contrast_grid.levels.len()/2;})) {{zgui::elements::vector().document(include_str!("../assets/add.svg")).class("add-icon")}}
            }
            control(class="contrast-insert append-level",tabindex=Focus::Sequential,a11y:label="Append contrast level",on:click=move |_|insert(grid.get_untracked().levels.len())) {{zgui::elements::vector().document(include_str!("../assets/add.svg")).class("add-icon")}}
        }
    }}
}
