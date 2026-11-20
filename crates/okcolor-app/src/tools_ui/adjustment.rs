use super::{PickerModel, PickerRefs};
use okcolor_core::adjust_palette_color;
use zgui::{prelude::*, reactive::RenderEffect};

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
    let cancel = move || {
        if let Some((index, _, original)) = drag.get_value() {
            drag.set_value(None);
            offsets.update(|v| v[index] = original);
            model.finish_palette_drag(false);
            expected.set_value(model.palette.get_untracked());
        }
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
    on_cleanup_local(move || {
        drop(observe);
        cancel();
    });

    let scrub = move |x: f32, modifiers: Modifiers| {
        let Some((index, start, original)) = drag.get_value() else {
            return;
        };
        let limit = if index == 0 { 180.0 } else { 100.0 };
        let speed = if modifiers.contains(Modifiers::SHIFT) {
            10.0
        } else if modifiers.contains(Modifiers::CONTROL) {
            0.1
        } else {
            1.0
        };
        let next =
            (f32::from(original) + ((x - start) * speed / 4.0).trunc()).clamp(-limit, limit) as i16;
        let mut values = offsets.get_untracked();
        if values[index] != next {
            values[index] = next;
            offsets.set(values);
            let palette: Vec<_> = base.with_value(|base| {
                base.iter()
                    .map(|&(source, locked)| {
                        (
                            if locked {
                                source
                            } else {
                                adjust_palette_color(source, gamut, values)
                            },
                            locked,
                        )
                    })
                    .collect()
            });
            expected.set_value(palette.clone());
            model.preview_palette(palette);
        }
    };
    // A row can disappear on Escape before the pointer is released. Listen at
    // the window so the drag can cross rows without leaving capture on a removed node.
    let root = refs
        .palette_adjust
        .window_root()
        .expect("mounted palette trigger");
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
            model.finish_palette_drag(true);
            cx.prevent_default();
            cx.stop_propagation();
        }
    });
    let cancelled = root.listen(
        events::POINTER_CANCEL,
        ListenerOptions::CAPTURE,
        move |_| cancel(),
    );
    on_cleanup_local(move || drop((moved, released, cancelled)));
    const LABELS: [&str; 4] = ["Hue", "Saturation", "Brightness", "Temperature"];
    view! {column(class="folder-menu above",node_ref=refs.adjustment_menu) {
        for i in move ||0..LABELS.len(),key=|i:&usize|*i {
            control(class="adjustment-row",node_ref=refs.adjustment_rows[i],a11y:label=LABELS[i],
                on:pointer_down=move |cx|{
                    if cx.primary && cx.button==Some(PointerButton::Primary) && model.begin() {
                        drag.set_value(Some((i,cx.position.x.0,offsets.get_untracked()[i])));
                        cx.prevent_default();
                    }
                }) {
                text(class="adjustment-name") {{LABELS[i]}}
                text(class="adjustment-value") {{move ||{
                    let value=offsets.get()[i];
                    let unit=if i==0 {""}else{"%"};
                    if value>0 {format!("+{value}{unit}")}else{format!("{value}{unit}")}
                }}}
            }
        }
    }}
}
