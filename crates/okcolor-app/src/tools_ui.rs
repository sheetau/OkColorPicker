use crate::{
    model::PickerModel,
    surface::ColorSwatchProps,
    ui::{PickerRefs, RecentColorsProps, SlotSwatchProps},
    workspace::{Library, Tab},
};
use okcolor_core::{
    ColorFormat, Harmony, Oklch, Ramp, apca, format_color, harmony, lightness_for_contrast,
    parse_color, ramp,
};
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

#[component]
pub(crate) fn ToolPage(model: PickerModel, refs: PickerRefs) -> impl IntoView {
    view! {
        column(class="tool-page",node_ref=refs.tool_page) {
            row(class="tool-palette") {
                box(class="swatches") {
                    if move ||model.selected.get()==0 {
                        SlotSwatch(model=model,refs=refs,index=1)
                        SlotSwatch(model=model,refs=refs,index=0)
                    } else {
                        SlotSwatch(model=model,refs=refs,index=0)
                        SlotSwatch(model=model,refs=refs,index=1)
                    }
                    control(class="swap-colors",node_ref=refs.swap,tabindex=Focus::Sequential,a11y:label="Swap main and sub colors",
                        on:pointer_down=move |_|refs.swap.focus(),on:click=move |_|model.swap()) {
                        {zgui::elements::vector().document(include_str!("../assets/swap.svg")).class("swap-icon")}
                    }
                }
                spacer()
                text(class="tool-current") {{move ||model.output(ColorFormat::Hex8)}}
            }
            if move ||model.tab.get()==Tab::Swatches {Swatches(model=model,refs=refs)}
            else { if move ||model.tab.get()==Tab::Harmonies {Harmonies(model=model,refs=refs)}
            else {Contrast(model=model,refs=refs)} }
        }
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
fn Swatches(model: PickerModel, refs: PickerRefs) -> impl IntoView {
    let selected = RwSignal::new(None::<u64>);
    let destination = RwSignal::new(None::<u64>);
    let editor = RwSignal::new(false);
    let draft = RwSignal::new(String::new());
    let add = move || {
        library_edit(model, |library| {
            let id = library.add(model.current(), destination.get_untracked())?;
            selected.set(Some(id));
            draft.set(format!("Color {id}"));
            Ok(())
        })
    };
    view! {
        column(class="swatch-library") {
            RecentColors(model=model,node=refs.recents)
            row(class="section-heading") {
                text {"Swatch Tiles"}
                control(class="tool-icon",node_ref=refs.tools[0],tabindex=Focus::Sequential,a11y:label="Save current color",on:click=move |_|add()) {"+"}
                spacer()
                control(class="tool-action",node_ref=refs.tools[1],tabindex=Focus::Sequential,a11y:label="Manage swatches and folders",on:click=move |_|editor.update(|open|*open = !*open)) {"Edit"}
            }
            box(node_ref=refs.tools[7]) {Tiles(model=model,folder=None,selected=selected,draft=draft)}
            for id in move ||model.library.get().folders.iter().map(|f|f.id).collect::<Vec<_>>(),key=|id:&u64|*id {
                column(class="folder-section") {
                    control(class="folder-heading",tabindex=Focus::Sequential,class:active=move ||destination.get()==Some(id),
                        on:click=move |_|{destination.set(Some(id));selected.set(Some(id));editor.set(true);draft.set(model.library.get_untracked().folders.iter().find(|f|f.id==id).map(|f|f.name.clone()).unwrap_or_default());}) {
                        {move ||model.library.get().folders.iter().find(|f|f.id==id).map(|f|f.name.clone()).unwrap_or_default()}
                    }
                    Tiles(model=model,folder=Some(id),selected=selected,draft=draft)
                }
            }
            if move ||model.library.get().swatches.is_empty() {text(class="tool-hint") {"Save the current color with +"}}
            if move ||editor.get() {
                column(class="library-editor") {
                    Entry(model=model,node=refs.tools[2],draft=draft,label="Swatch or folder name")
                    row(class="tool-actions") {
                        control(class="tool-action",node_ref=refs.tools[3],tabindex=Focus::Sequential,disabled=move ||selected.get().is_none(),on:click=move |_| {
                            if let Some(id)=selected.get_untracked(){library_edit(model,|library|library.rename(id,&draft.get_untracked()));}
                        }) {"Rename"}
                        control(class="tool-action",node_ref=refs.tools[4],tabindex=Focus::Sequential,on:click=move |_|library_edit(model,|library|{
                            let name=draft.get_untracked();let name=if name.trim().is_empty(){"New folder"}else{&name};
                            let id=library.create_folder(name)?;destination.set(Some(id));selected.set(Some(id));draft.set(name.into());Ok(())
                        })) {"New folder"}
                        control(class="tool-action",node_ref=refs.tools[5],tabindex=Focus::Sequential,disabled=move ||selected.get().is_none(),on:click=move |_| {
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
                        control(class="tool-action",node_ref=refs.tools[6],tabindex=Focus::Sequential,
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
    selected: RwSignal<Option<u64>>,
    draft: RwSignal<String>,
) -> impl IntoView {
    view! {
        row(class="swatch-tiles") {
            for id in move ||model.library.get().swatches.iter().filter(|s|s.folder==folder).map(|s|s.id).collect::<Vec<_>>(),key=|id:&u64|*id {
                control(class="library-tile",class:active=move ||selected.get()==Some(id),tabindex=Focus::Sequential,
                    a11y:label=move ||model.library.get().swatches.iter().find(|s|s.id==id).map(|s|s.name.clone()).unwrap_or_default(),
                    on:click=move |_|{if let Some(swatch)=model.library.get_untracked().swatches.iter().find(|s|s.id==id){model.select(swatch.color());selected.set(Some(id));draft.set(swatch.name.clone());}}) {
                    ColorSwatch(model=model,color=Memo::new(move |_|model.library.get().swatches.iter().find(|s|s.id==id).map(|s|s.color()).unwrap_or(model.current())),outline_dark=false)
                }
            }
        }
    }
}

#[component]
fn Generated(model: PickerModel, colors: Memo<Vec<Oklch>>, node: NodeRef) -> impl IntoView {
    view! {row(class="generated-colors",node_ref=node) {
        for index in move ||0..colors.get().len(),key=|index:&usize|*index {
            control(class="generated-color",tabindex=Focus::Sequential,a11y:label=move ||format!("Use {}",format_color(colors.get().get(index).copied().unwrap_or(model.current()),ColorFormat::Oklch,model.mapping.get()).map(|v|v.text).unwrap_or_default()),
                on:click=move |_|{model.select(colors.get_untracked().get(index).copied().unwrap_or(model.current()));}) {
                ColorSwatch(model=model,color=Memo::new(move |_|colors.get().get(index).copied().unwrap_or(model.current())),outline_dark=false)
            }
        }
    }}
}

#[component]
fn Harmonies(model: PickerModel, refs: PickerRefs) -> impl IntoView {
    view! {column(class="tool-sections") {
        for entry in move || [(Ramp::Shades,"Shades"),(Ramp::Tints,"Tints"),(Ramp::Tones,"Tones")],key=|entry:&(Ramp,&str)|entry.1 {
            column(class="tool-section") {
                text(class="section-heading") {{entry.1}}
                Generated(model=model,node=if entry.0==Ramp::Shades {refs.tools[7]}else{NodeRef::new()},colors=Memo::new(move |_|ramp(model.color(),entry.0,model.gamut.get()).to_vec()))
            }
        }
        column(class="tool-section") {
            row(class="section-heading") {
                text {"Harmony"} spacer()
                control(class="tool-action",node_ref=refs.tools[0],tabindex=Focus::Sequential,a11y:label="Next harmony",
                    on:click=move |_|model.tools.update(|tools|tools.harmony=(tools.harmony+1)%Harmony::ALL.len())) {{move ||Harmony::ALL[model.tools.get().harmony].label()}}
            }
            Generated(model=model,node=NodeRef::new(),colors=Memo::new(move |_|harmony(model.color(),Harmony::ALL[model.tools.get().harmony],model.gamut.get())))
        }
        text(class="tool-hint") {"Click a color to use it. Samples fit the working gamut."}
        control(class="tool-action",node_ref=refs.tools[1],tabindex=Focus::Sequential,on:click=move |_|library_edit(model,|library|library.add(model.current(),None).map(|_|()))) {"Save current to Swatches"}
    }}
}

#[component]
fn Contrast(model: PickerModel, refs: PickerRefs) -> impl IntoView {
    let background = RwSignal::new(
        format_color(
            model.tools.get_untracked().background(),
            ColorFormat::Oklch,
            model.mapping.get_untracked(),
        )
        .expect("validated OKLCH components")
        .text,
    );
    let target = RwSignal::new(model.tools.get_untracked().target_lc.to_string());
    let set_background = move |text: &str| match parse_color(text, 0.0) {
        Ok(color) if color.components()[3] == 1.0 => {
            model.tools.update(|t| t.background = color.components());
            background.set(text.into());
            model.message.set(String::new());
        }
        _ => model.message.set("Enter an opaque background color".into()),
    };
    let result = Memo::new(move |_| {
        apca(
            model.color(),
            model.tools.get().background(),
            model.mapping.get(),
        )
    });
    view! {column(class="tool-sections contrast-sections") {
        text(class="section-heading") {"APCA · selected color as text"}
        row(class="contrast-values") {
            column {text(class="tool-hint") {"On white"} text(class="lc-small") {{move ||lc(apca(model.color(),Oklch::new(1.0,0.0,0.0,1.0).unwrap(),model.mapping.get()))}}}
            column {text(class="tool-hint") {"On black"} text(class="lc-small") {{move ||lc(apca(model.color(),Oklch::new(0.0,0.0,0.0,1.0).unwrap(),model.mapping.get()))}}}
        }
        text(class="section-heading") {"Background"}
        row(class="tool-actions") {
            Entry(model=model,node=refs.tools[0],draft=background,label="APCA background color")
            control(class="tool-action",node_ref=refs.tools[1],tabindex=Focus::Sequential,on:click=move |_|set_background(&background.get_untracked())) {"Set"}
        }
        row(class="tool-actions") {
            control(class="tool-action",tabindex=Focus::Sequential,on:click=move |_|set_background("#fff")) {"White"}
            control(class="tool-action",tabindex=Focus::Sequential,on:click=move |_|set_background("#000")) {"Black"}
            control(class="tool-action",tabindex=Focus::Sequential,on:click=move |_| {
                let color=model.slots[1-usize::from(model.selected.get_untracked())].get_untracked();
                if color.components()[3]==1.0 {model.tools.update(|t|t.background=color.components());background.set(format_color(color,ColorFormat::Oklch,model.mapping.get_untracked()).unwrap().text);model.message.set(String::new());}
                else {model.message.set("The other slot must be opaque".into());}
            }) {"Other slot"}
        }
        text(class="lc-value",node_ref=refs.tools[4]) {{move ||lc(result.get())}}
        text(class="tool-hint") {{move ||match result.get(){Ok(v) if v<0.0=>"Light text on dark background",Ok(v) if v>0.0=>"Dark text on light background",Ok(_)=>"No effective text contrast",Err(_)=>"Color cannot be evaluated"}}}
        text(class="section-heading") {"Target signed Lc"}
        row(class="tool-actions") {
            Entry(model=model,node=refs.tools[2],draft=target,label="Target signed APCA Lc")
            control(class="tool-action",node_ref=refs.tools[3],tabindex=Focus::Sequential,on:click=move |_| {
                let Ok(lc)=target.get_untracked().trim().parse::<f64>() else {model.message.set("Enter a signed Lc from -110 to 110".into());return;};
                if !lc.is_finite() || !(-110.0..=110.0).contains(&lc) {model.message.set("Enter a signed Lc from -110 to 110".into());return;}
                model.tools.update(|t|t.target_lc=lc);
                match lightness_for_contrast(model.current(),model.tools.get_untracked().background(),lc,model.mapping.get_untracked()) {
                    Ok(Some(color))=>{model.select(color);}
                    Ok(None)=>model.message.set("Target unreachable with this chroma, hue and alpha".into()),
                    Err(_)=>model.message.set("Color cannot be evaluated in sRGB".into()),
                }
            }) {"Adjust L"}
        }
        text(class="tool-hint") {"Adjust L preserves C, H and alpha."}
        text(class="tool-hint") {{move ||if model.mapping.get()==okcolor_core::GamutMapping::Clip {"Evaluated in clipped sRGB"}else{"Evaluated in mapped sRGB"}}}
        text(class="tool-hint") {"APCA 0.0.98G · experimental Lc values"}
        text(class="tool-hint") {"Typography and context still require assessment."}
    }}
}
fn lc(result: Result<f64, okcolor_core::ColorError>) -> String {
    result
        .map(|value| format!("{value:+.1} Lc"))
        .unwrap_or_else(|_| "Unavailable".into())
}
