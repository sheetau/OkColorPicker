use crate::tiles::{ColorTileProps, Destination, Source, TileGestures};
use crate::tooltip::Tooltips;
use crate::{
    model::PickerModel,
    ui::{CopyButtonProps, PickerRefs},
};
use okcolor_core::{
    ColorFormat, Gamut, GamutMapping, Harmony, Oklch, Variation, apca_in, closest_current_contrast,
    closest_grid_sample, format_color, harmony, parse_color, variations,
};
use zgui::reactive::UnsyncCallback;
use zgui::{prelude::*, reactive::RenderEffect};

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
                label=Memo::new(move |_|{let c=colors.get().get(index).copied().unwrap_or(model.current());format!("{name} {}",hex(c))}),
                activate=UnsyncCallback::new(move |()|{if let Some(color)=colors.get_untracked().get(index){model.select(*color);}}))

        }
    }}
}

#[component]
fn Similar(model: PickerModel, node: NodeRef) -> impl IntoView {
    let hits = Memo::new(move |_| okcolor_core::nearest_named(model.palette_source(), 6));
    view! {row(class="generated-colors similar",node_ref=node){
        for hit in move ||hits.get(),key=|hit:&okcolor_core::NamedColor|hit.name {
            ColorTile(class="generated-color",node=NodeRef::new(),source=Source::Sample,destination=None,
                color=Memo::new(move |_|hit.color),label=Memo::new(move |_|format!("{} {}",hit.name,hex(hit.color))),
                activate=UnsyncCallback::new(move |()|{model.select(hit.color);}))
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
    let open = Memo::new(move |_| !model.tools.get().folded_groups[group]);
    let title = ["Variations", "Similar", "Harmony"][group];
    let heading = crate::tiles::TileHeading::mount();
    view! {column(class="tool-section") {
        row(class="section-heading variation-heading") {
            control(node_ref=Tooltips::button(refs.group_toggles[group],title),tabindex=Focus::Sequential,on:click=move |_|model.tools.update(|t|t.folded_groups[group] = !t.folded_groups[group])) {
                if move ||open.get() {{zgui::elements::vector().document(include_str!("../assets/minus.svg")).class("fold-icon")}}
                else {box(class="fold-icon") {box(class="fold-dot") {}}}
                text {{title}}
            }
            spacer()
            HoverLabel(heading=heading)
        }
        if move ||open.get() {
            if move ||group==0 {
                for entry in move ||[(Variation::Lightness,"Shades / Tints"),(Variation::Chroma,"Tones"),(Variation::Hue,"Hue"),(Variation::Temperature,"Temperature")],key=|entry:&(Variation,&str)|entry.0 as u8 {
                    Generated(model=model,name=entry.1,node=if entry.0==Variation::Lightness {refs.tools[7]}else{NodeRef::new()},
                        colors=Memo::new(move |_|variations(model.palette_source(),entry.0,model.gamut.get()).to_vec()))
                }
            }
            if move ||group==1 {
                Similar(model=model,node=refs.similar)
            }
            if move ||group==2 {
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

const MAX_PALETTE: usize = 10;

fn insert_palette_colors(
    colors: &mut Vec<(Oklch, bool)>,
    index: usize,
    requested: usize,
    edge: Oklch,
    gamut: Gamut,
) {
    let count = requested.min(MAX_PALETTE.saturating_sub(colors.len()));
    let index = index.min(colors.len());
    let left = index.checked_sub(1).map_or(edge, |i| colors[i].0);
    let right = colors.get(index).map_or(edge, |entry| entry.0);
    let gamut = if left.in_gamut(Gamut::Srgb) && right.in_gamut(Gamut::Srgb) {
        Gamut::Srgb
    } else {
        gamut
    };
    let endpoints = [left, right].map(|c| c.mapped_rgb(gamut, GamutMapping::LocalMinde));
    let [Ok(left_rgb), Ok(right_rgb)] = endpoints else {
        return;
    };
    let [_, _, h, a] = left.components();
    let b = right.components()[3];
    let ramp = (1..=count).map(|step| {
        let t = step as f64 / (count + 1) as f64;
        let rgb = std::array::from_fn(|k| {
            if gamut == Gamut::Srgb {
                let lo = (left_rgb[k] * 255.0).round();
                let hi = (right_rgb[k] * 255.0).round();
                (lo + (hi - lo) * t).round() / 255.0
            } else {
                left_rgb[k] + (right_rgb[k] - left_rgb[k]) * t
            }
        });
        let color =
            Oklch::from_rgb(rgb, gamut, a + (b - a) * t, h).expect("bounded palette gradient");
        (color, false)
    });
    colors.splice(index..index, ramp);
}

#[component]
fn PaletteInsert(
    model: PickerModel,
    index: usize,
    node: NodeRef,
    hot: RwSignal<Option<usize>>,
) -> impl IntoView {
    let dragging_tile = TileGestures::current().dragging;
    let tail = index == usize::MAX;
    const PIXELS_PER_COLOR: f32 = 12.0;
    let origin_y = RwSignal::new(None::<f32>);
    let count = RwSignal::new(0usize);
    let scrubbed = RwSignal::new(false);
    let insert = move |requested| {
        model.edit_palette(|colors| {
            insert_palette_colors(
                colors,
                index,
                requested,
                model.palette_source_untracked(),
                model.gamut.get_untracked(),
            )
        });
    };
    let update_count = move |y: f32| {
        if let Some(start) = origin_y.get_untracked() {
            let steps = ((start - y) / PIXELS_PER_COLOR).floor() as usize;
            let available = MAX_PALETTE.saturating_sub(model.palette.get_untracked().len());
            if steps > 0 {
                scrubbed.set(true);
            }
            count.set(steps.min(available));
        }
    };
    Tooltips::bind(node, move || {
        if origin_y.get_untracked().is_some() {
            String::new()
        } else if tail {
            "Append palette color · drag up for more".into()
        } else {
            "Insert palette colors · drag up for more".into()
        }
    });
    view! {control(class=if tail {"palette-tail"}else{"palette-insert"},
        class:tile-drag=move ||dragging_tile.get().is_some(),
        class:visible=move ||origin_y.get().is_some() || hot.get()==Some(index),
        style:left=move ||(!tail).then(||if index==0 {"0px".into()} else {format!("{}%",100.0*index as f32/model.palette.get().len() as f32)}),
        class:interior=!tail&&index>0,node_ref=node,tabindex=Focus::Sequential,
        a11y:label=if tail {"Append palette color"}else{"Insert palette colors"},
        on:pointer_down=move |cx|{
            if cx.button==Some(PointerButton::Primary) {
                scrubbed.set(false);
                origin_y.set(Some(cx.position.y.0));
                cx.capture_pointer();
            }
        },
        on:pointer_move=move |cx|{
            if origin_y.get_untracked().is_some() {
                update_count(cx.position.y.0);
                cx.stop_propagation();
            }
        },
        on:pointer_up=move |cx|{
            if cx.button==Some(PointerButton::Primary) && origin_y.get_untracked().is_some() {
                update_count(cx.position.y.0);
                let requested=count.get_untracked();
                origin_y.set(None);
                count.set(0);
                cx.release_pointer();
                if scrubbed.get_untracked() && requested>0 { insert(requested); }
            }
        },
        on:pointer_cancel=move |_|{origin_y.set(None);count.set(0);scrubbed.set(true);},
        on:key_down=move |_|scrubbed.set(false),
        on:click=move |_|{if !scrubbed.get_untracked(){insert(1);} scrubbed.set(false);}) {
        if move ||count.get()==0 {{zgui::elements::vector().document(include_str!("../assets/add.svg")).class("add-icon")}}
        else {text(class="insert-count") {{move ||count.get().to_string()}}}
    }}
}

#[component]
fn Palette(model: PickerModel, refs: PickerRefs) -> impl IntoView {
    let gestures = TileGestures::current();
    let heading = crate::tiles::TileHeading::mount();

    let hot = RwSignal::new(None::<usize>);
    let scale = use_window().scale();
    let palette_hover = move |cx: &mut EventCx<'_, events::PointerMove>| {
        let Some(b) = refs.tools[5].window_bounds() else {
            return;
        };
        let scale = scale.get_untracked();
        let width = b.size.width.0 / scale;
        let x = cx.position.x.0 - b.origin.x.0 / scale;
        let count = model.palette.get_untracked().len();
        let cell = width / count as f32;
        let index = (x / cell).round() as usize;
        let next = if x < 21.0 {
            Some(0)
        } else if x >= width - 21.0 {
            Some(usize::MAX)
        } else if index < count && (x - index as f32 * cell).abs() <= 10.5 {
            Some(index)
        } else {
            None
        };
        hot.set(next);
    };
    let tail = refs.tools[2];
    let end = Destination::Palette(usize::MAX);
    gestures.register(tail, end);
    let regenerate = move || model.generate_palette();
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
            control(node_ref=Tooltips::button(refs.tools[0],"Generate palette · Space"),tabindex=Focus::Sequential,class="generate-button",a11y:label="Generate palette",on:click=move |_|regenerate()) {{zgui::elements::vector().document(include_str!("../assets/generate.svg")).class("generate-icon")}}
        }
        row(class="palette-strip",node_ref=refs.tools[5],on:pointer_move=palette_hover,on:pointer_leave=move |_|hot.set(None)) {
            for i in move ||0..model.palette.get().len(),key=|i:&usize|*i {
                column(class="palette-cell",node_ref=if i==0{refs.tools[6]}else{NodeRef::new()}) {
                    ColorTile(class="palette-color",node=NodeRef::new(),source=Source::Palette(i),destination=Some(Destination::Palette(i)),
                        color=Memo::new(move |_|model.palette.get().get(i).map(|v|v.0).unwrap_or(model.current())),
                        label=Memo::new(move |_|model.palette.get().get(i).map(|v|swatch_name(v.0)).unwrap_or_default()),
                        activate=UnsyncCallback::new(move |()|{if let Some(v)=model.palette.get_untracked().get(i){model.select(v.0);}}))
                    if move ||model.palette.get().get(i).is_some_and(|entry|entry.1) {
                        box(class="palette-lock",node_ref=if i==0{refs.palette_dot}else{NodeRef::new()},style:background-color=move ||model.palette.get().get(i).map(|v|crate::tiles::contrast_ink(v.0).to_owned())) {}
                    }
                }
            }
            for i in move ||0..model.palette.get().len(),key=|i:&usize|*i {
                PaletteInsert(model=model,index=i,node=NodeRef::new(),hot=hot)
            }
            PaletteInsert(model=model,index=usize::MAX,node=tail,hot=hot)
            if move ||gestures.hovering.get()==Some(end) {box(class="palette-end-line",style:background-color=move ||Some(gestures.palette_boundary(usize::MAX))) {}}
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

// An explicit sign selects a polarity. A bare number follows the field's current direction.
fn parse_lc(text: &str, default_positive: bool) -> Option<(f64, Option<bool>)> {
    let text = text.trim();
    let value = text.parse::<f64>().ok().filter(|v| v.is_finite())?.round();
    let explicit = text
        .starts_with(['-', '+'])
        .then_some(!text.starts_with('-'));
    let target = if explicit.is_some() || default_positive {
        value
    } else {
        -value
    };
    Some((target, explicit))
}

fn current_lc(
    color: Oklch,
    reference: Oklch,
    to_text: bool,
    gamut: Gamut,
    mapping: GamutMapping,
) -> Option<f64> {
    if to_text {
        apca_in(reference, color, mapping, gamut)
    } else {
        apca_in(color, reference, mapping, gamut)
    }
    .ok()
}

// Keep hue, chroma and alpha. Monotone lightness search also clamps requests
// beyond the attainable range and picks the nearer side of the APCA dead zone.
fn adjust_current_lc(
    model: PickerModel,
    reference: Oklch,
    to_text: bool,
    gamut: Gamut,
    value: String,
) {
    let source = model.current();
    let mapping = model.mapping.get_untracked();
    let Some(score) = current_lc(source, reference, to_text, gamut, mapping) else {
        model
            .message
            .set("A contrast background must be opaque".into());
        return;
    };
    let positive = if score == 0.0 {
        (reference.components()[0] <= 0.6) == to_text
    } else {
        score > 0.0
    };
    let Some((target, _)) = parse_lc(&value, positive) else {
        model.message.set("Enter a finite contrast value".into());
        return;
    };
    model.message.set(String::new());
    if score.round() == target {
        return;
    }
    if let Some((color, _)) =
        closest_current_contrast(source, reference, target, gamut, to_text, mapping)
    {
        model.select(color);
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
    let exit = expect_context::<crate::ui::InputExit>().0;
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
            Key::Named(NamedKey::Enter)=>{commit();exit.focus();},
            Key::Named(NamedKey::Escape)=>{reset();exit.focus();},
            _=>return,
        }
        cx.prevent_default();cx.stop_propagation();
    }) {{value.get_untracked()}}}
}

#[component]
fn ReferenceField(model: PickerModel, dark: Memo<bool>, node: NodeRef) -> impl IntoView {
    view! {GridField(model=model,node=node,label="Reference HEX",value=Memo::new(move |_|hex(grid_reference(&model.tools.get(),dark.get()))),
    apply=UnsyncCallback::new(move |value:String|{
        match parse_color(value.trim(),0.0) {
            Ok(color) if color.components()[3]==1.0 => model.edit_contrast(|t|if dark.get_untracked(){t.contrast_grid.dark_reference=color.components();}else{t.background=color.components();}),
            _=> model.message.set("Enter an opaque reference color".into()),
        }
    }))}
}

#[component]
pub(crate) fn Contrast(model: PickerModel, refs: PickerRefs) -> impl IntoView {
    let preview_split = RwSignal::new(None::<usize>);
    let grid = Memo::new(move |_| {
        let mut grid = model.tools.get().contrast_grid;
        if let Some(split) = preview_split.get() {
            grid.split = split.min(grid.levels.len());
        }
        grid
    });
    let gamut = Memo::new(move |_| {
        if grid.get().p3 {
            Gamut::DisplayP3
        } else {
            Gamut::Srgb
        }
    });
    let samples = Memo::new(move |_| {
        let t = model.tools.get();
        let g = grid.get();
        g.levels
            .iter()
            .enumerate()
            .map(|(i, _)| {
                let reference = grid_reference(&t, i < g.split);
                closest_grid_sample(
                    model.color(),
                    reference,
                    g.signed_target(i, reference),
                    gamut.get(),
                    g.to_text,
                    g.max_chroma,
                )
            })
            .collect::<Vec<_>>()
    });
    let candidates = Memo::new(move |_| {
        samples.with(|s| {
            s.iter()
                .map(|item| item.map(|(color, _)| color))
                .collect::<Vec<_>>()
        })
    });
    let region = refs.tools[5];
    let dragging = RwSignal::new(false);
    let scale = use_window().scale();
    let hot = RwSignal::new(None::<usize>);
    let contrast_hover = move |cx: &mut EventCx<'_, events::PointerMove>| {
        let Some(b) = region.window_bounds() else {
            return;
        };
        let scale = scale.get_untracked();
        let x = cx.position.x.0 - b.origin.x.0 / scale;
        let y = cx.position.y.0 - b.origin.y.0 / scale - 20.0;
        let level = (y / 40.0).round();
        hot.set(
            ((146.0..186.0).contains(&x)
                && (y - 40.0 * level).abs() <= 11.0
                && level >= 0.0
                && level <= grid.get_untracked().levels.len() as f32)
                .then_some(level as usize),
        );
    };
    let boundary = move |y: f32| {
        if let Some(b) = region.window_bounds() {
            let count = grid.get_untracked().levels.len();
            let local = (y * scale.get_untracked() - b.origin.y.0) / scale.get_untracked();
            let index = ((local - 20.0) / 40.0).round().clamp(0.0, count as f32) as usize;
            preview_split.set(Some(index));
        }
    };
    let insert = move |i: usize| model.edit_contrast(|t| t.contrast_grid.insert(i));
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
            text {"APCA"} spacer()
            control(class="tool-action",node_ref=Tooltips::button(refs.tools[0],"Switch text and background roles"),tabindex=Focus::Sequential,on:click=move |_|model.edit_contrast(|t|t.contrast_grid.to_text = !t.contrast_grid.to_text)) {{move ||if grid.get().to_text{"To text"}else{"To bg"}}}
            control(class="tool-action",node_ref=Tooltips::button(refs.tools[1],"Switch contrast gamut"),tabindex=Focus::Sequential,on:click=move |_|model.edit_contrast(|t|t.contrast_grid.p3 = !t.contrast_grid.p3)) {{move ||if grid.get().p3{"P3"}else{"sRGB"}}}
            control(class="tool-action",node_ref=Tooltips::button(refs.tools[2],"Switch chroma limit"),tabindex=Focus::Sequential,on:click=move |_|model.edit_contrast(|t|t.contrast_grid.max_chroma = !t.contrast_grid.max_chroma)) {{move ||if grid.get().max_chroma{"Max C"}else{"Current C"}}}
            row(class="contrast-actions") {
                control(class="generate-button",node_ref=Tooltips::button(refs.apca_reset,"Reset APCA grid"),tabindex=Focus::Sequential,a11y:label="Reset APCA grid",
                    on:click=move |_|model.edit_contrast(|t|{
                        let defaults=crate::workspace::ToolSettings::default();
                        t.contrast_grid=defaults.contrast_grid;
                        t.background=defaults.background;
                    })) {{zgui::elements::vector().document(include_str!("../assets/generate.svg")).class("generate-icon")}}
                CopyButton(text=UnsyncCallback::new(move |()|css()),compact=true,label="Copy generated CSS colors")
            }
        }
        column(class="contrast-grid",node_ref=region,on:pointer_move=contrast_hover,on:pointer_leave=move |_|hot.set(None)) {
            box(class="grid-padding",style:background-color=move ||Some(hex(grid_reference(&model.tools.get(),grid.get().split>0)))) {}
            for i in move ||0..grid.get().levels.len(),key=|i:&usize|*i {
                row(class="contrast-row",style:background-color=move ||Some(hex(grid_reference(&model.tools.get(),i<grid.get().split))),
                    style:color=move ||Some(crate::tiles::contrast_ink(grid_reference(&model.tools.get(),i<grid.get().split)).into()),
                    on:pointer_down=move |cx|{if cx.button==Some(PointerButton::Middle){model.edit_contrast(|t|t.contrast_grid.remove(i));cx.prevent_default();cx.stop_propagation();}}) {
                    box(class="current-spacer") {}
                    row(class="level-info") {
                        text(class="level-number") {{move ||grid.get().level_name(i)}}
                        box(class="lc-badge",class:outline=move ||!grid.get().to_text,
                            style:background-color=move ||if grid.get().to_text{candidates.get().get(i).copied().flatten().map(hex)}else{None},
                            style:color=move ||{
                                let reference=grid_reference(&model.tools.get(),i<grid.get().split);
                                let candidate=candidates.get().get(i).copied().flatten();
                                Some(candidate.map_or_else(||crate::tiles::contrast_ink(reference).to_owned(),|c|{
                                    if grid.get().to_text {hex(reference)} else {hex(c)}
                                }))
                            },
                            style:border-color=move ||candidates.get().get(i).copied().flatten().map(hex)) {
                            GridField(model=model,node=if i==0{refs.tools[3]}else{NodeRef::new()},label="Target contrast Lc",
                                value=Memo::new(move |_|samples.with(|s|s.get(i).copied().flatten().map(|(_,score)|format!("{:.0}",score.abs())).unwrap_or_default())),
                                apply=UnsyncCallback::new(move |v:String|{
                                    let settings=model.tools.get_untracked();
                                    let grid=&settings.contrast_grid;
                                    let reference=grid_reference(&settings,i<grid.split);
                                    let positive=okcolor_core::default_grid_target(reference,grid.to_text,1.0)>0.0;
                                    let Some((lc,explicit))=parse_lc(&v,positive) else {model.message.set("Enter a finite contrast value".into());return};
                                    let gamut=if grid.p3 {Gamut::DisplayP3}else{Gamut::Srgb};
                                    if let Some((_,score))=closest_grid_sample(model.current(),reference,lc,gamut,grid.to_text,grid.max_chroma) {
                                        model.edit_contrast(|t|t.contrast_grid.set_target(i,score,explicit));
                                        model.message.set(String::new());
                                    }
                                }))
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
                }
            }
            box(class="grid-padding",style:background-color=move ||Some(hex(grid_reference(&model.tools.get(),grid.get().split==grid.get().levels.len())))) {}
            for dark in move ||[true,false].into_iter().filter(|v|count(*v)>0).collect::<Vec<_>>(),key=|dark:&bool|*dark {
                box(class="contrast-current",class:outline=move ||!grid.get().to_text,
                    style:top=move ||Some(format!("{}px",20+40*origin(dark)+20*(count(dark).saturating_sub(1)))),
                    style:border-color=move ||Some(hex(model.color())),
                    style:color=move ||{
                        let reference=grid_reference(&model.tools.get(),dark);
                        Some(if grid.get().to_text {hex(reference)} else {hex(model.color())})
                    },
                    style:background-color=move ||grid.get().to_text.then(||hex(model.color()))) {
                    GridField(model=model,node=refs.current_lc[usize::from(!dark)],label="Current color contrast Lc",
                        value=Memo::new(move |_|current_lc(model.color(),grid_reference(&model.tools.get(),dark),grid.get().to_text,gamut.get(),model.mapping.get()).map_or_else(||"—".into(),|v|format!("{:.0}",v.abs()))),
                        apply=UnsyncCallback::new(move |v|adjust_current_lc(model,grid_reference(&model.tools.get_untracked(),dark),grid.get_untracked().to_text,gamut.get_untracked(),v)))
                }
                box(class="reference-field",style:top=move ||{
                    let g=grid.get();let top=if g.split==0{40+40*g.levels.len()-26}else if g.split==g.levels.len(){10}else if dark {20+40*g.split-26}else{20+40*g.split+10};
                    Some(format!("{top}px"))},style:color=move ||Some(crate::tiles::contrast_ink(grid_reference(&model.tools.get(),dark)).into())) {
                    ReferenceField(model=model,node=if dark{refs.tools[4]}else{refs.tools[7]},dark=Memo::new(move |_|dark))
                }
            }
            if move ||dragging.get() || (grid.get().split>0 && grid.get().split<grid.get().levels.len()) {
                box(class="contrast-boundary",style:color=move ||{let t=model.tools.get();Some(crate::tiles::boundary_ink(grid_reference(&t,true),grid_reference(&t,false)))},class:active=move ||dragging.get(),style:top=move ||Some(format!("{}px",20+40*grid.get().split)),
                    on:pointer_down=move |cx|{if cx.button==Some(PointerButton::Primary){dragging.set(true);cx.capture_pointer();}},
                    on:pointer_move=move |cx|{if dragging.get_untracked(){boundary(cx.position.y.0);}},
                    on:pointer_up=move |cx|{if dragging.get_untracked(){boundary(cx.position.y.0);cx.release_pointer();if let Some(split)=preview_split.get_untracked(){model.edit_contrast(|t|t.contrast_grid.split=split);}preview_split.set(None);dragging.set(false);}},
                    on:pointer_cancel=move |_|{preview_split.set(None);dragging.set(false);}) {box(class="divider-line") {}}
            } else {
                control(class="restore-reference",class:at-top=move ||grid.get().split==0,node_ref=Tooltips::button(refs.tools[6],"Restore both backgrounds"),style:color=move ||Some(crate::tiles::contrast_ink(grid_reference(&model.tools.get(),grid.get().split>0)).into()),tabindex=Focus::Sequential,a11y:label="Restore both backgrounds",on:click=move |_|model.edit_contrast(|t|{if t.contrast_grid.levels.len()==1{t.contrast_grid.insert(1);}t.contrast_grid.split=t.contrast_grid.levels.len()/2;})) {{zgui::elements::vector().document(include_str!("../assets/add.svg")).class("add-icon")}}
            }
            for i in move ||0..=grid.get().levels.len(),key=|i:&usize|*i {
                control(class="contrast-insert",class:visible=move ||hot.get()==Some(i),style:top=move ||Some(format!("{}px",9+i*40)),
                    node_ref=Tooltips::button(NodeRef::new(),"Insert contrast level"),tabindex=Focus::Sequential,a11y:label="Insert contrast level",
                    on:click=move |_|insert(i)) {{zgui::elements::vector().document(include_str!("../assets/add.svg")).class("add-icon")}}
            }
        }
    }}
}

#[cfg(test)]
mod palette_tests {
    use super::*;

    #[test]
    fn inserted_palette_colors_match_encoded_rgb_midpoints() {
        for (left, right, middle) in [
            ("#000000", "#ffffff", "#808080"),
            ("#ff0000", "#0000ff", "#800080"),
        ] {
            let left = parse_color(left, 0.0).unwrap();
            let right = parse_color(right, 0.0).unwrap();
            let mut palette = vec![(left, true), (right, false)];
            insert_palette_colors(&mut palette, 1, 1, left, Gamut::Srgb);
            assert_eq!(hex(palette[1].0), middle);
            assert_eq!((palette[0], palette[2]), ((left, true), (right, false)));
        }
    }
}
