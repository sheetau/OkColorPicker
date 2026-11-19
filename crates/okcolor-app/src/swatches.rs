//! Saved swatches and inline folder management.
use crate::{
    model::PickerModel,
    tiles::{ColorTileProps, Destination, Source, TileGestures},
    tooltip::Tooltips,
    ui::PickerRefs,
};
use std::time::Duration;
use zgui::{
    prelude::*,
    reactive::{RenderEffect, UnsyncCallback},
    view::{TimeoutHandle, Timers},
};

/// One delay for stationary rename presses and hovering a captured drag over the folder selector.
#[derive(Clone, Copy)]
pub(crate) struct Hold {
    clock: StoredValue<Timers, LocalStorage>,
    timer: StoredValue<Option<TimeoutHandle>, LocalStorage>,
}
impl Hold {
    pub(crate) fn new() -> Self {
        Self {
            clock: StoredValue::new_local(Timers::current().expect("picker window")),
            timer: StoredValue::new_local(None),
        }
    }
    pub(crate) fn start(self, action: impl FnOnce() + 'static) {
        self.timer
            .set_value(Some(self.clock.with_value(|clock| {
                clock.set_timeout(Duration::from_millis(650), action)
            })));
    }
    pub(crate) fn cancel(self) {
        self.timer.set_value(None);
    }
}

#[derive(Clone, Copy)]
pub(crate) struct RenameSwatch(pub UnsyncCallback<u64>);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Heading {
    Browse,
    New,
    Rename(u64),
    Delete(u64),
}

#[component]
fn NameField(
    model: PickerModel,
    node: NodeRef,
    initial: String,
    finish: UnsyncCallback<Option<String>>,
) -> impl IntoView {
    let draft = RwSignal::new(initial.clone());
    let composing = RwSignal::new(false);
    let finished = StoredValue::new(false);
    let submit = move |accept| {
        if finished.get_value() {
            return;
        }
        finished.set_value(true);
        model.editing_text.set(false);
        let name = draft.get_untracked();
        finish.run((accept && !name.trim().is_empty()).then_some(name));
    };
    let focus = RenderEffect::new(move |_| {
        node.get()?;
        node.focus();
        node.select_all();
        Some(())
    });
    on_cleanup_local(move || {
        drop(focus);
        model.editing_text.set(false);
    });
    view! {field(class="folder-input",node_ref=node,a11y:label="Swatch or folder name",
    on:focus_in=move |_|model.editing_text.set(true),on:focus_out=move |_|submit(true),
    on:ime_start=move |_|composing.set(true),on:ime_preedit=move |_|composing.set(true),
    on:ime_commit=move |_|composing.set(false),on:ime_end=move |_|composing.set(false),
    on:input=move |cx|{if let Some(v)=cx.payload().as_value(){draft.set(v.value.to_string());}},
    on:key_down=move |cx|{
        if composing.get_untracked(){return}
        let accept=match cx.key {Key::Named(NamedKey::Enter)=>true,Key::Named(NamedKey::Escape)=>false,_=>return};
        cx.prevent_default();cx.stop_propagation();submit(accept);
    }) {{initial}}}
}

#[component]
fn FolderOption(
    model: PickerModel,
    id: Option<u64>,
    selected: Memo<Option<u64>>,
    choose: UnsyncCallback<Option<u64>>,
) -> impl IntoView {
    let node = NodeRef::new();
    let gestures = TileGestures::current();
    let target = Destination::Folder(id);
    gestures.register(node, target);
    view! {control(node_ref=node,tabindex=Focus::Sequential,class:selected=move ||selected.get()==id,
        class:drag-over=move ||gestures.hovering.get()==Some(target),on:click=move |_|choose.run(id)) {
        text {{move || model.library.with(|lib| lib.folders.iter().find(|f|Some(f.id)==id).map(|f|f.name.clone())).unwrap_or_else(||"Swatch Tiles".into())}}
    }}
}

#[component]
pub(crate) fn Swatches(model: PickerModel, refs: PickerRefs) -> impl IntoView {
    let destination = RwSignal::new(None::<u64>);
    let folder = Memo::new(move |_| {
        destination.get().filter(|id| {
            model
                .library
                .with(|lib| lib.folders.iter().any(|f| f.id == *id))
        })
    });
    // Undoing folder creation also invalidates the current navigation target.
    let selection = RenderEffect::new(move |_| {
        if folder.get().is_none() && destination.get_untracked().is_some() {
            destination.set(None);
        }
    });
    on_cleanup_local(move || drop(selection));
    let heading = RwSignal::new(Heading::Browse);
    let menu = RwSignal::new(false);
    let trigger = refs.library[1];
    let popup = refs.folder_menu;
    let gestures = TileGestures::current();
    gestures.register(trigger, Destination::FolderMenu);
    crate::ui::dismiss_menu(menu, popup, trigger);
    let choose = UnsyncCallback::new(move |id| {
        destination.set(id);
        menu.set(false);
    });
    let rename = move |id| {
        menu.set(false);
        heading.set(Heading::Rename(id));
    };
    provide_context(RenameSwatch(UnsyncCallback::new(rename)));
    let remove = move |id| {
        model.edit_library(|lib| {
            lib.remove(id);
            Ok(())
        });
        destination.set(None);
        heading.set(Heading::Browse);
    };
    let confirm_escape = RenderEffect::new(move |_| {
        if !matches!(heading.get(), Heading::Delete(_)) {
            return None;
        }
        let node = refs.library[5];
        node.get()?;
        node.focus();
        Some(
            node.window_root()?
                .listen(events::KEY_DOWN, ListenerOptions::CAPTURE, move |cx| {
                    if cx.key == Key::Named(NamedKey::Escape) {
                        cx.prevent_default();
                        cx.stop_propagation();
                        heading.set(Heading::Browse);
                    }
                }),
        )
    });
    on_cleanup_local(move || drop(confirm_escape));

    let press = Hold::new();
    let pressed_at = StoredValue::new(None::<(f32, f32)>);
    let long_pressed = StoredValue::new(false);
    let dwell = Hold::new();
    let drag_menu = StoredValue::new(false);
    let over_heading = Memo::new(move |_| {
        gestures.dragging.get().is_some()
            && gestures.hovering.get() == Some(Destination::FolderMenu)
    });
    let hover = RenderEffect::new(move |_| {
        if over_heading.get() && !menu.get_untracked() {
            dwell.start(move || {
                drag_menu.set_value(true);
                menu.set(true);
            });
        } else {
            dwell.cancel();
        }
    });
    let end_drag = RenderEffect::new(move |_| {
        if gestures.dragging.get().is_none() && drag_menu.get_value() {
            drag_menu.set_value(false);
            menu.set(false);
        }
    });
    on_cleanup_local(move || {
        drop(hover);
        drop(end_drag);
    });
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
                .max(0.0)
                / scale.get();
        let above = (button.origin.y.0 - page.origin.y.0).max(0.0) / scale.get();
        let up = below < (model.library.get().folders.len() + 2) as f32 * 22.0 && above > below;
        (up, if up { above } else { below })
    });
    let finish = UnsyncCallback::new(move |name: Option<String>| {
        let mode = heading.get_untracked();
        heading.set(Heading::Browse);
        if let Some(name) = name {
            match mode {
                Heading::New => {
                    if let Some(id) = model.edit_library(|l| l.create_folder(&name)) {
                        destination.set(Some(id));
                    }
                }
                Heading::Rename(id) => {
                    model.edit_library(|l| l.rename(id, &name));
                }
                _ => {}
            }
        }
    });
    let begin_press = move |cx: &mut EventCx<'_, events::PointerDown>| {
        long_pressed.set_value(false);
        let Some(id) = folder.get_untracked() else {
            return;
        };
        match cx.button {
            Some(PointerButton::Primary) => {
                pressed_at.set_value(Some((cx.position.x.0, cx.position.y.0)));
                press.start(move || {
                    long_pressed.set_value(true);
                    rename(id);
                });
            }
            Some(PointerButton::Middle) => {
                menu.set(false);
                long_pressed.set_value(true);
                cx.prevent_default();
                if model
                    .library
                    .with_untracked(|lib| lib.swatches.iter().any(|s| s.folder == Some(id)))
                {
                    heading.set(Heading::Delete(id));
                } else {
                    remove(id);
                }
            }
            _ => {}
        }
    };
    let track_press = move |cx: &mut EventCx<'_, events::PointerMove>| {
        if pressed_at
            .get_value()
            .is_some_and(|(x, y)| (cx.position.x.0 - x).hypot(cx.position.y.0 - y) >= 4.0)
        {
            press.cancel();
        }
    };
    let initial_name = move || {
        let Heading::Rename(id) = heading.get_untracked() else {
            return String::new();
        };
        model.library.with_untracked(|lib| {
            lib.folders
                .iter()
                .find(|f| f.id == id)
                .map(|f| f.name.clone())
                .or_else(|| {
                    lib.swatches
                        .iter()
                        .find(|s| s.id == id)
                        .map(|s| s.name.clone())
                })
                .unwrap_or_default()
        })
    };
    view! {column(class="swatch-library") {
        box(class="folder-anchor") {
            if move ||heading.get()==Heading::Browse {
                control(class="folder-heading",class:up=move ||menu.get()&&placement.get().0,class:open=move ||menu.get()||over_heading.get(),
                    node_ref=Tooltips::button(trigger,"Choose swatch folder"),tabindex=Focus::Sequential,a11y:label="Choose swatch folder",
                    on:pointer_down=begin_press,on:pointer_move=track_press,
                    on:key_down=move |_|long_pressed.set_value(false),
                    on:pointer_up=move |_|{pressed_at.set_value(None);press.cancel();},
                    on:pointer_leave=move |_|press.cancel(),on:pointer_cancel=move |_|press.cancel(),
                    on:click=move |_|{if !long_pressed.get_value(){menu.update(|v| *v = !*v);}}) {
                    text {{move ||model.library.with(|l|l.folders.iter().find(|f|Some(f.id)==folder.get()).map(|f|f.name.clone())).unwrap_or_else(||"Swatch Tiles".into())}}
                    box(class="folder-chevron") {{zgui::elements::vector().document(include_str!("../assets/chevron.svg")).class("chevron")}}
                }
            } else { if move ||matches!(heading.get(),Heading::Delete(_)) {
                row(class="folder-confirm") {
                    text {"Delete?"}
                    control(node_ref=refs.library[5],tabindex=Focus::Sequential,on:click=move |_|{if let Heading::Delete(id)=heading.get_untracked(){remove(id);}}) {"Yes"}
                    control(node_ref=refs.library[6],tabindex=Focus::Sequential,on:click=move |_|heading.set(Heading::Browse)) {"No"}
                }
            } else {
                NameField(model=model,node=refs.library[2],finish=finish,initial=initial_name())
            }}
            if move ||menu.get() {
                column(class="folder-menu",node_ref=popup,class:above=move ||placement.get().0,style:max-height=move ||Some(format!("{}px",placement.get().1))) {
                    FolderOption(model=model,id=None,selected=folder,choose=choose)
                    for id in move ||model.library.with(|l|l.folders.iter().map(|f|f.id).collect::<Vec<_>>()),key=|id:&u64|*id {
                        FolderOption(model=model,id=Some(id),selected=folder,choose=choose)
                    }
                    control(class="new-folder",node_ref=refs.library[4],tabindex=Focus::Sequential,on:click=move |_|{menu.set(false);heading.set(Heading::New);}) {
                        {zgui::elements::vector().document(include_str!("../assets/add.svg")).class("add-icon")}
                        text {"New folder"}
                    }
                }
            }
        }
        if move ||heading.get()==Heading::New {
            row(class="swatch-tiles") {}
        } else {
            for current in move ||[folder.get()],key=|id:&Option<u64>|*id {
                Tiles(model=model,folder=current,refs=refs)
            }
        }
    }}
}

#[component]
fn Tiles(model: PickerModel, folder: Option<u64>, refs: PickerRefs) -> impl IntoView {
    let end = Destination::Library(folder, None);
    let gestures = TileGestures::current();
    gestures.register(refs.library[0], end);
    view! {row(class="swatch-tiles",node_ref=refs.library[7]) {
        for id in move ||model.library.with(|l|l.swatches.iter().filter(|s|s.folder==folder).map(|s|s.id).collect::<Vec<_>>()),key=|id:&u64|*id {
            ColorTile(class="library-tile",node=NodeRef::new(),source=Source::Library(id),destination=Some(Destination::Library(folder,Some(id))),
                color=Memo::new(move |_|model.library.with(|l|l.swatches.iter().find(|s|s.id==id).map(|s|s.color())).unwrap_or(model.current())),
                label=Memo::new(move |_|model.library.with(|l|l.swatches.iter().find(|s|s.id==id).map(|s|s.name.clone())).unwrap_or_default()),
                activate=UnsyncCallback::new(move |()|{if let Some(swatch)=model.library.get_untracked().swatches.iter().find(|s|s.id==id){model.select(swatch.color());}}))
        }
        control(class="tool-icon add-tile",node_ref=Tooltips::button(refs.library[0],"Save current color"),tabindex=Focus::Sequential,
            a11y:label="Save current color",on:click=move |_|{model.edit_library(|l|l.add(model.current(),folder));}) {
            {zgui::elements::vector().document(include_str!("../assets/add.svg")).class("add-icon")}
            if move ||gestures.hovering.get()==Some(end) {box(class="insertion-line") {}}
        }
    }}
}
