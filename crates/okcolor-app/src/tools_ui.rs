use crate::tiles::{ColorTileProps, Destination, Source, TileGestures};
use crate::{model::PickerModel, ui::PickerRefs, workspace::Library};
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
    view! {
        column(class="swatch-library") {
            control(class="folder-heading",node_ref=refs.library[1],tabindex=Focus::Sequential,
                a11y:label="Choose swatch folder",on:click=move |_|menu.update(|v|*v = !*v)) {
                {move ||model.library.get().folders.iter().find(|f|Some(f.id)==destination.get()).map(|f|f.name.clone()).unwrap_or_else(||"Swatch Tiles".into())}
            }
            if move ||menu.get() {
                column(class="folder-menu") {
                    control(tabindex=Focus::Sequential,on:click=move |_|{destination.set(None);selected.set(None);menu.set(false);}) {"Swatch Tiles"}
                    for id in move ||model.library.get().folders.iter().map(|f|f.id).collect::<Vec<_>>(),key=|id:&u64|*id {
                        control(tabindex=Focus::Sequential,on:click=move |_|{destination.set(Some(id));selected.set(Some(id));menu.set(false);}) {
                            {move ||model.library.get().folders.iter().find(|f|f.id==id).map(|f|f.name.clone()).unwrap_or_default()}
                        }
                    }
                    control(tabindex=Focus::Sequential,on:click=move |_|{editor.set(true);menu.set(false);draft.set(String::new());}) {"New folder / Edit"}
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
                })) {text {"+"} if move ||gestures.hovering.get()==Some(end) {box(class="insertion-line") {}}}
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
    view! {row(class="generated-colors",node_ref=node) {
        for index in move ||0..colors.get().len(),key=|index:&usize|*index {
            ColorTile(class="generated-color",node=NodeRef::new(),source=Source::Sample,destination=None,
                color=Memo::new(move |_|colors.get().get(index).copied().unwrap_or(model.current())),
                label=Memo::new(move |_|format!("{name} {}",hex(colors.get().get(index).copied().unwrap_or(model.current())))),
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
    format!("{} {hex}", okcolor_core::color_name(color))
}

#[component]
fn RelatedGroup(model: PickerModel, refs: PickerRefs, group: usize) -> impl IntoView {
    let open = RwSignal::new(true);
    let title = ["Variations", "Similar", "Harmony"][group];
    let heading = crate::tiles::TileHeading::mount();
    view! {column(class="tool-section") {
        row(class="section-heading") {
            control(tabindex=Focus::Sequential,on:click=move |_|open.update(|v|*v = !*v)) {{move ||format!("{} {title}",if open.get(){"−"}else{"+"})}}
            spacer()
            text {{move ||heading.0.get()}}
        }
        if move ||open.get() {
            if move ||group<2 {
                for kind in move ||if group==0 {vec![Ramp::Shades,Ramp::Tints,Ramp::Tones,Ramp::Temperatures]}else{vec![Ramp::Similar]},key=|kind:&Ramp|*kind as u8 {
                    Generated(model=model,name=match kind {Ramp::Shades=>"Shades",Ramp::Tints=>"Tints",Ramp::Tones=>"Tones",Ramp::Temperatures=>"Temperature",Ramp::Similar=>"Similar"},node=if kind==Ramp::Shades {refs.tools[7]}else{NodeRef::new()},
                        colors=Memo::new(move |_|ramp_steps::<13>(model.palette_source(),kind,model.gamut.get()).to_vec()))
                }
            } else {
                row(class="harmony-combinations") {
                    for kind in move ||Harmony::ALL,key=|kind:&Harmony|*kind as u8 {
                        box(class="combination") {
                            Generated(model=model,name=kind.label(),node=NodeRef::new(),colors=Memo::new(move |_|harmony(model.palette_source(),kind,model.gamut.get())))
                        }
                    }
                }
            }
        }
    }}
}

fn generate_palette(model: PickerModel, seed: u32, count: usize) -> Vec<Oklch> {
    let source = model.palette_source_untracked();
    let [light, chroma, hue, alpha] = source.components();
    let mut state = seed;
    let mut next = || {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        f64::from(state) / f64::from(u32::MAX)
    };
    let scheme = Harmony::ALL[(next() * (Harmony::ALL.len() as f64)) as usize % Harmony::ALL.len()];
    let anchors = harmony(source, scheme, model.gamut.get_untracked());
    (0..count)
        .map(|i| {
            let anchor = anchors[i % anchors.len()].components()[2];
            let l = (light * 0.35 + 0.23 + 0.40 * next()).clamp(0.15, 0.94);
            let c = (chroma * 0.45 + 0.05 + 0.16 * next()).min(0.4);
            let h = if i == 0 {
                hue
            } else {
                anchor + (next() - 0.5) * 24.0
            };
            Oklch::new(l, c, h, alpha)
                .expect("bounded palette color")
                .reduce_chroma(model.gamut.get_untracked())
        })
        .collect()
}

#[component]
fn Palette(model: PickerModel, refs: PickerRefs) -> impl IntoView {
    let gestures = TileGestures::current();
    let heading = crate::tiles::TileHeading::mount();
    let seed = gestures.palette_seed;
    let tail = refs.tools[2];
    let end = Destination::Palette(usize::MAX);
    gestures.register(tail, end);
    let regenerate = move || {
        seed.update(|s| *s = s.wrapping_mul(1664525).wrapping_add(1013904223));
        let old = gestures.palette.get_untracked();
        let count = if old.is_empty() { 5 } else { old.len() };
        let generated = generate_palette(model, seed.get_untracked(), count);
        gestures.palette.set(
            generated
                .into_iter()
                .enumerate()
                .map(|(i, c)| {
                    old.get(i)
                        .copied()
                        .filter(|(_, locked)| *locked)
                        .unwrap_or((c, false))
                })
                .collect(),
        );
    };
    if seed.get_untracked() == 0 {
        regenerate();
    }
    view! {column(class="tool-section") {
        row(class="section-heading") {
            text {"Palettes"} spacer() text {{move ||heading.0.get()}}
            control(node_ref=refs.tools[0],tabindex=Focus::Sequential,on:click=move |_|regenerate()) {"Generate"}
        }
        row(class="palette-strip",node_ref=refs.tools[5]) {
            for i in move ||0..gestures.palette.get().len(),key=|i:&usize|*i {
                column(class="palette-cell") {
                    ColorTile(class="palette-color",node=NodeRef::new(),source=Source::Palette(i),destination=Some(Destination::Palette(i)),
                        color=Memo::new(move |_|gestures.palette.get().get(i).map(|v|v.0).unwrap_or(model.current())),
                        label=Memo::new(move |_|gestures.palette.get().get(i).map(|v|swatch_name(v.0)).unwrap_or_default()),
                        activate=UnsyncCallback::new(move |()|{if let Some(v)=gestures.palette.get_untracked().get(i){model.select(v.0);}}))
                    control(class="palette-insert",tabindex=Focus::Sequential,a11y:label="Insert palette color",
                        on:click=move |_|gestures.palette.update(|v|{if v.len()<16{v.insert(i.min(v.len()),(model.current(),false));}})) {"+"}
                    control(class="palette-lock",node_ref=if i==0{refs.tools[6]}else{NodeRef::new()},tabindex=Focus::Sequential,a11y:label="Toggle color lock",
                        class:locked=move ||gestures.palette.get().get(i).is_some_and(|v|v.1),
                        on:click=move |_|gestures.palette.update(|v|{if let Some(entry)=v.get_mut(i){entry.1 = !entry.1;}})) {{move ||if gestures.palette.get().get(i).is_some_and(|v|v.1){"Unlock"}else{"Lock"}}}
                }
            }
            control(class="palette-tail",node_ref=tail,tabindex=Focus::Sequential,a11y:label="Append palette color",
                on:click=move |_|gestures.palette.update(|v|{if v.len()<16{v.push((model.current(),false));}})) {text {"+"} if move ||gestures.hovering.get()==Some(end) {box(class="insertion-line") {}}}
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
    let commit = move || {
        if editing.get_untracked() {
            apply.run(draft.get_untracked());
        }
        editing.set(false);
        model.editing_text.set(false);
        node.set_value(&value.get_untracked());
    };
    view! {field(class="grid-field",node_ref=node,a11y:label=label,
    on:focus_in=move |_|{editing.set(true);model.editing_text.set(true);node.select_all();},
    on:input=move |cx|{if let Some(v)=cx.payload().as_value(){draft.set(v.value.to_string());}},
    on:focus_out=move |_|commit(),
    on:ime_start=move |_|composing.set(true),on:ime_preedit=move |_|composing.set(true),
    on:ime_commit=move |_|composing.set(false),on:ime_end=move |_|composing.set(false),
    on:key_down=move |cx|{if composing.get_untracked(){return;}
        if cx.key==Key::Named(NamedKey::Enter){commit();node.focus_move(FocusMove::Next);cx.prevent_default();}else if cx.key==Key::Named(NamedKey::Escape){editing.set(false);model.editing_text.set(false);node.set_value(&value.get_untracked());node.focus_move(FocusMove::Next);cx.prevent_default();}}) {{value.get_untracked()}}}
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
    let clipboard = use_clipboard();
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
            let index = (((y * scale.get_untracked() - b.origin.y.0) / b.size.height.0)
                * count as f32)
                .round()
                .clamp(0.0, count as f32) as usize;
            model.tools.update(|t| t.contrast_grid.split = index);
        }
    };
    let insert = move |i: usize| {
        model.tools.update(|t| {
            let g = &mut t.contrast_grid;
            if g.levels.len() < 16 {
                let i = i.min(g.levels.len());
                let target = *g.levels.get(i.saturating_sub(1)).unwrap_or(&60);
                g.levels.insert(i, target);
                if i < g.split {
                    g.split += 1;
                }
            }
        });
    };
    view! {column(class="tool-section") {
        row(class="section-heading contrast-heading") {
            text {"APCA"} spacer()
            control(class="tool-action",node_ref=refs.tools[0],tabindex=Focus::Sequential,on:click=move |_|model.tools.update(|t|t.contrast_grid.to_text = !t.contrast_grid.to_text)) {{move ||if grid.get().to_text{"To text"}else{"To background"}}}
            control(class="tool-action",node_ref=refs.tools[1],tabindex=Focus::Sequential,on:click=move |_|model.tools.update(|t|t.contrast_grid.p3 = !t.contrast_grid.p3)) {{move ||if grid.get().p3{"P3"}else{"sRGB"}}}
            control(class="tool-action",node_ref=refs.tools[2],tabindex=Focus::Sequential,on:click=move |_|model.tools.update(|t|t.contrast_grid.max_chroma = !t.contrast_grid.max_chroma)) {{move ||if grid.get().max_chroma{"Max C"}else{"Current C"}}}
            control(class="tool-action",tabindex=Focus::Sequential,a11y:label="Copy generated CSS colors",on:click=move |_|{
                let text=candidates.get_untracked().into_iter().enumerate().filter_map(|(i,c)|Some(format!("--color-{}: {};",(i+1)*100,format_color(c?,ColorFormat::Oklch,GamutMapping::LocalMinde).ok()?.text))).collect::<Vec<_>>().join("\n");
                clipboard.set_text(ClipboardKind::Standard,text);
            }) {"Copy"}
        }
        column(class="contrast-grid",node_ref=region) {
            for i in move ||0..grid.get().levels.len(),key=|i:&usize|*i {
                row(class="contrast-row",
                    style:background-color=move ||Some(hex(grid_reference(&model.tools.get(),i<grid.get().split))),
                    style:color=move ||Some(if grid_reference(&model.tools.get(),i<grid.get().split).components()[0]>0.6{"#161616".into()}else{"#e0e0e0".into()})) {
                    text(class="contrast-current",
                        class:outline=move ||!grid.get().to_text && (i==0 || i==grid.get().split),
                        style:border-color=move ||Some(hex(model.color())),
                        style:color=move ||Some(if grid.get().to_text{hex(grid_reference(&model.tools.get(),i<grid.get().split))}else{hex(model.color())}),
                        style:background-color=move ||(grid.get().to_text && (i==0 || i==grid.get().split)).then(||hex(model.color()))) {{move ||{
                        let t=model.tools.get();let reference=grid_reference(&t,i<t.contrast_grid.split);
                        let current=model.color();
                        let score=if t.contrast_grid.to_text{apca_in(reference,current,model.mapping.get(),gamut())}else{apca_in(current,reference,model.mapping.get(),gamut())};
                        if i==0 || i==t.contrast_grid.split {score.map(|v|format!("{:.0}",v.abs())).unwrap_or_else(|_|"—".into())}else{String::new()}
                    }}}
                    text(class="level-number") {{(i+1)*100}}
                    box(class="lc-badge",class:outline=move ||!grid.get().to_text,
                        style:background-color=move ||if grid.get().to_text{candidates.get().get(i).copied().flatten().map(hex)}else{None},
                        style:color=move ||if grid.get().to_text && candidates.get().get(i).is_some_and(Option::is_some){Some(hex(grid_reference(&model.tools.get(),i<grid.get().split)))}else{candidates.get().get(i).copied().flatten().map(hex)},
                        style:border-color=move ||candidates.get().get(i).copied().flatten().map(hex)) {
                    GridField(model=model,node=if i==0{refs.tools[3]}else{NodeRef::new()},label="Target contrast Lc",
                        value=Memo::new(move |_|grid.get().levels.get(i).map(u8::to_string).unwrap_or_default()),
                        apply=UnsyncCallback::new(move |v:String|{if let Some(lc)=v.trim().parse::<u8>().ok().filter(|v|*v<=110){
                            model.tools.update(|t|{if let Some(level)=t.contrast_grid.levels.get_mut(i){*level=lc;}});
                        }else{model.message.set("Enter a contrast from 0 to 110".into());}}))
                    }
                    if move ||candidates.get().get(i).is_some_and(Option::is_some) {
                        box(class="contrast-sample") {
                        ColorTile(node=NodeRef::new(),class="candidate-color",source=Source::Sample,destination=None,
                            color=Memo::new(move |_|candidates.get().get(i).copied().flatten().unwrap_or(model.current())),
                            label=Memo::new(move |_|candidates.get().get(i).copied().flatten().map(swatch_name).unwrap_or_default()),
                            activate=UnsyncCallback::new(move |()|{if let Some(Some(c))=candidates.get_untracked().get(i){model.select(*c);}}))
                        if move ||candidates.get().get(i).copied().flatten().is_some_and(|c|!c.in_gamut(Gamut::Srgb)){text(class="p3-badge") {"P3"}}
                        }
                    } else {text {"—"}}
                    spacer()
                    if move ||i==0 || i==grid.get().split {
                        ReferenceField(model=model,node=if i==0{refs.tools[4]}else{NodeRef::new()},dark=Memo::new(move |_|i<grid.get().split))
                    }
                    control(class="contrast-insert",tabindex=Focus::Sequential,a11y:label="Insert contrast level",on:click=move |_|insert(i)) {"+"}
                }
            }
            if move ||dragging.get() || (grid.get().split>0 && grid.get().split<grid.get().levels.len()) {
                box(class="contrast-boundary",style:top=move ||Some(format!("{}%",100.0*grid.get().split as f32/grid.get().levels.len() as f32)),
                    on:pointer_down=move |cx|{if cx.button==Some(PointerButton::Primary){dragging.set(true);cx.capture_pointer();}},
                    on:pointer_move=move |cx|{if dragging.get_untracked(){boundary(cx.position.y.0);}},
                    on:pointer_up=move |cx|{if dragging.get_untracked(){boundary(cx.position.y.0);cx.release_pointer();dragging.set(false);}},
                    on:pointer_cancel=move |_|dragging.set(false)) {}
            } else {
                control(class="restore-reference",node_ref=refs.tools[6],style:color=move ||Some(if grid_reference(&model.tools.get(),grid.get().split>0).components()[0]>0.6{"#161616".into()}else{"#e0e0e0".into()}),tabindex=Focus::Sequential,a11y:label="Restore both backgrounds",on:click=move |_|model.tools.update(|t|t.contrast_grid.split=t.contrast_grid.levels.len()/2)) {"+"}
            }
        }
        control(class="append-level",tabindex=Focus::Sequential,a11y:label="Append contrast level",on:click=move |_|insert(grid.get_untracked().levels.len())) {"+"}
    }}
}
