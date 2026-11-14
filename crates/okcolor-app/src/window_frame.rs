//! Borderless-window resize affordances delegate the actual gesture to the OS.
use zgui::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptionVisual {
    pub hovered: Option<usize>,
    pub pressed: Option<usize>,
    pub enabled: [bool; 3],
    pub maximized: bool,
}
impl Default for CaptionVisual {
    fn default() -> Self {
        Self {
            hovered: None,
            pressed: None,
            enabled: [true; 3],
            maximized: false,
        }
    }
}

pub fn caption_glyph(index: usize, maximized: bool) -> &'static str {
    if crate::fonts::caption_font().is_none() {
        return "";
    }
    match index {
        0 => "\u{e921}",
        1 if maximized => "\u{e923}",
        1 => "\u{e922}",
        2 => "\u{e8bb}",
        _ => "",
    }
}

#[component]
pub fn ResizeEdges() -> impl IntoView {
    view! {
        Grip(edge=ResizeEdge::North,class="grip north")
        Grip(edge=ResizeEdge::South,class="grip south")
        Grip(edge=ResizeEdge::West,class="grip west")
        Grip(edge=ResizeEdge::East,class="grip east")
        Grip(edge=ResizeEdge::NorthWest,class="grip north-west")
        Grip(edge=ResizeEdge::NorthEast,class="grip north-east")
        Grip(edge=ResizeEdge::SouthWest,class="grip south-west")
        Grip(edge=ResizeEdge::SouthEast,class="grip south-east")
    }
}

#[component]
fn Grip(edge: ResizeEdge, class: &'static str) -> impl IntoView {
    let window = use_window();
    let visible = {
        let w = window.clone();
        move || !w.maximized().get() && w.fullscreen().get().is_none()
    };
    let cursor = match edge {
        ResizeEdge::North | ResizeEdge::South => CursorStyle::ResizeNorthSouth,
        ResizeEdge::East | ResizeEdge::West => CursorStyle::ResizeEastWest,
        ResizeEdge::NorthWest | ResizeEdge::SouthEast => CursorStyle::ResizeNorthWestSouthEast,
        _ => CursorStyle::ResizeNorthEastSouthWest,
    };
    view! {
        Show(when=visible) {
            box(class=class,on:pointer_down=window.resize_drag_handler(edge),
                on:pointer_enter={let w=window.clone();move |_|w.set_cursor(cursor)},
                on:pointer_leave={let w=window.clone();move |_|w.set_cursor(CursorStyle::Default)}) {}
        }
    }
}
