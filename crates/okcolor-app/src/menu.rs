use zgui::prelude::*;

/// A dropdown occupies the right edge of the page, independent of its trigger's
/// scroll container. The palette sets its lower edge at the swatch strip.
#[component]
pub(crate) fn MenuBand(
    page: NodeRef,
    trigger: NodeRef,
    node: NodeRef,
    #[prop(optional)] swatches: Option<NodeRef>,
    children: ChildrenFn,
) -> impl IntoView {
    let body = page.observe_border_box();
    let anchor = trigger.observe_border_box();
    let edge = swatches.map(|node| node.observe_border_box());
    let items = NodeRef::new();
    let item_size = items.observe_border_size();
    let scale = use_window().scale();
    let geometry = Memo::new(move |_| {
        let Some(body) = body.get() else {
            return (0.0, 0.0, 0.0);
        };
        let scale = scale.get();
        let top = body.origin.y.0 / scale;
        let full = body.size.height.0 / scale;
        let height = edge.and_then(|s| s.get()).map_or(full, |strip| {
            (strip.origin.y.0 / scale - top).clamp(0.0, full)
        });
        let rows = item_size.get().map_or(0.0, |size| size.height.0 / scale);
        let room = (height - rows).max(0.0);
        let offset = if edge.is_some() {
            room
        } else {
            anchor.get().map_or(0.0, |button| {
                ((button.origin.y.0 + button.size.height.0 / 2.0) / scale - top - rows / 2.0)
                    .clamp(0.0, room)
            })
        };
        (top, height, offset)
    });
    let children = StoredValue::new_local(children);
    view! {Portal {
        column(class="menu-band",node_ref=node,
            style:top=move ||Some(format!("{}px",geometry.get().0)),
            style:height=move ||Some(format!("{}px",geometry.get().1))) {
            column(class="menu-items",node_ref=items,style:margin-top=move ||Some(format!("{}px",geometry.get().2))) {
                {children.get_value().view()}
            }
        }
    }}
}
