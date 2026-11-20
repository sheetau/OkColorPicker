use zgui::prelude::*;

/// The dropdown ends 12 CSS pixels left of its trigger icon's center.
/// The palette sets its lower edge at the swatch strip.
#[component]
pub(crate) fn MenuBand(
    page: NodeRef,
    icon: NodeRef,
    node: NodeRef,
    #[prop(optional)] swatches: Option<NodeRef>,
    #[prop(optional)] status_menu: bool,
    children: ChildrenFn,
) -> impl IntoView {
    let body = page.observe_border_box();
    let anchor = icon.observe_border_box();
    let edge = swatches.map(|node| node.observe_border_box());
    let items = NodeRef::new();
    let item_size = items.observe_border_size();
    let scale = use_window().scale();
    let right = move || {
        let body = body.get()?;
        if status_menu {
            return Some("20px".into());
        }
        let icon = anchor.get()?;
        let center = icon.origin.x.0 + icon.size.width.0 * 0.5;
        let inset = (body.origin.x.0 + body.size.width.0 - center) / scale.get() + 12.0;
        Some(format!("{inset}px"))
    };
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
        let offset = if status_menu || edge.is_some() {
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
        box(class="menu-area",style:right=right,
            style:top=move ||Some(format!("{}px",geometry.get().0)),
            style:height=move ||Some(format!("{}px",geometry.get().1))) {
            column(class="menu-band",node_ref=node) {
                column(class="menu-items",node_ref=items,style:margin-top=move ||Some(format!("{}px",geometry.get().2))) {
                    {children.get_value().view()}
                }
            }
        }
    }}
}
