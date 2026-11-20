use super::{PickerModel, PickerRefs};
use crate::menu::MenuBandProps;
use okcolor_core::adjust_palette_color;
use zgui::{prelude::*, reactive::RenderEffect};

const LABELS: [&str; 4] = ["L", "C", "H", "T"];

#[component]
pub(super) fn AdjustmentMenu(
    model: PickerModel,
    refs: PickerRefs,
    open: RwSignal<bool>,
) -> impl IntoView {
    let base = StoredValue::new(model.palette.get_untracked());
    let expected = StoredValue::new(base.get_value());
    let gamut = model.gamut.get_untracked();
    let offsets = RwSignal::new([0i16; 4]);
    let drag = StoredValue::new(None::<(usize, f32, i16)>);
    let editing = StoredValue::new(false);
    let finish = move |apply| {
        if editing.get_value() {
            editing.set_value(false);
            model.finish_palette_adjustment(apply);
        }
    };
    let close = move |apply| {
        finish(apply);
        open.set(false);
        refs.palette_adjust.focus();
    };
    let observe = RenderEffect::new(move |_| {
        if model
            .palette
            .with(|palette| expected.with_value(|own| palette != own))
            || model.gamut.get() != gamut
        {
            open.set(false);
        }
    });

    let scrub = move |x: f32, modifiers: Modifiers| {
        let Some((index, start, original)) = drag.get_value() else {
            return;
        };
        let limit = if index == 2 { 360.0 } else { 100.0 };
        let mut speed = 1.0;
        if modifiers.contains(Modifiers::SHIFT) {
            speed *= 10.0;
        }
        if modifiers.contains(Modifiers::CONTROL) {
            speed *= 0.1;
        }
        let next =
            (f32::from(original) + ((x - start) * speed / 4.0).trunc()).clamp(-limit, limit) as i16;
        let mut values = offsets.get_untracked();
        if values[index] == next {
            return;
        }
        if !editing.get_value() {
            if !model.begin() {
                return;
            }
            editing.set_value(true);
        }
        values[index] = next;
        offsets.set(values);
        let palette = base.with_value(|base| {
            base.iter()
                .map(|&(source, locked)| {
                    let color = if locked {
                        source
                    } else {
                        adjust_palette_color(source, gamut, values)
                    };
                    (color, locked)
                })
                .collect::<Vec<_>>()
        });
        expected.set_value(palette.clone());
        model.preview_palette(palette);
        if values == [0; 4] {
            finish(false);
        }
    };
    let root = refs
        .palette_adjust
        .window_root()
        .expect("mounted palette trigger");
    // Win32 captures the physical mouse for a button press. Capture the zgui
    // root as well so an off-window release still has a live dispatch target.
    let pressed = root.listen(events::POINTER_DOWN, ListenerOptions::CAPTURE, move |cx| {
        if let Some(index) = refs
            .adjustment_rows
            .iter()
            .position(|row| row.contains(cx.target))
        {
            if cx.primary && cx.button == Some(PointerButton::Primary) {
                drag.set_value(Some((
                    index,
                    cx.position.x.0,
                    offsets.get_untracked()[index],
                )));
                cx.capture_pointer();
                cx.prevent_default();
            }
        } else if !refs.adjustment_menu.contains(cx.target) {
            if editing.get_value() {
                cx.prevent_default();
                cx.stop_propagation();
            } else if !refs.palette_adjust.contains(cx.target) {
                open.set(false);
            }
        }
    });
    let clicked = root.listen(events::CLICK, ListenerOptions::CAPTURE, move |cx| {
        if (editing.get_value() && !refs.adjustment_menu.contains(cx.target))
            || refs.tools[5].contains(cx.target)
            || refs.tools[0].contains(cx.target)
        {
            cx.prevent_default();
            cx.stop_propagation();
        }
    });
    let moved = root.listen(events::POINTER_MOVE, ListenerOptions::CAPTURE, move |cx| {
        if cx.primary && drag.get_value().is_some() {
            scrub(cx.position.x.0, cx.modifiers);
            cx.stop_propagation();
        }
    });
    let released = root.listen(events::POINTER_UP, ListenerOptions::CAPTURE, move |cx| {
        if cx.button == Some(PointerButton::Primary) && drag.get_value().is_some() {
            scrub(cx.position.x.0, cx.modifiers);
            drag.set_value(None);
            cx.release_pointer();
            cx.prevent_default();
            cx.stop_propagation();
        }
    });
    let cancelled = root.listen(
        events::POINTER_CANCEL,
        ListenerOptions::CAPTURE,
        move |cx| {
            drag.set_value(None);
            cx.release_pointer();
        },
    );
    let key = root.listen(events::KEY_DOWN, ListenerOptions::CAPTURE, move |cx| {
        if cx.key == Key::Named(NamedKey::Escape) {
            if drag.get_value().is_some() {
                drag.set_value(None);
                cx.release_pointer();
            }
            close(false);
            cx.prevent_default();
            cx.stop_propagation();
        }
    });
    on_cleanup_local(move || {
        drop((observe, pressed, clicked, moved, released, cancelled, key));
        finish(false);
    });
    view! {MenuBand(page=refs.tool_page,trigger=refs.palette_adjust,node=refs.adjustment_menu,swatches=refs.tools[5]) {
        for i in move ||0..LABELS.len(),key=|i:&usize|*i {
            control(class="adjustment-row",node_ref=refs.adjustment_rows[i],a11y:label=LABELS[i]) {
                text(class="adjustment-name") {{LABELS[i]}}
                text(class="adjustment-value") {{move ||{
                    let value=offsets.get()[i];
                    if value>0 {format!("+{value}")}else{value.to_string()}
                }}}
            }
        }
        control(class="adjustment-action",node_ref=refs.adjustment_apply,tabindex=Focus::Sequential,on:click=move |_|close(true)) {text(class="adjustment-caption") {"Apply"}}
        control(class="adjustment-action",node_ref=refs.adjustment_cancel,tabindex=Focus::Sequential,on:click=move |_|close(false)) {text(class="adjustment-caption") {"Cancel"}}
    }}
}
