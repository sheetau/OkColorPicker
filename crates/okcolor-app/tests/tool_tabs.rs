//! Mounted tool workflows, using an isolated profile and headless input dispatch.
use okcolor_app::{
    fonts,
    model::PickerModel,
    persistence::SavedState,
    ui::{PickerProps, PickerRefs, STYLE},
    workspace::Tab,
};
use std::{
    cell::Cell,
    rc::Rc,
    sync::{Arc, Mutex},
    time::Duration,
};
use zgui::{
    geom::{CssPx, Point},
    platform::{Surface, SurfaceEvent},
    prelude::*,
    render::{RenderTarget, Renderer},
    vocab::{KeyEvent, KeyState, PointerAction, PointerEvent, Timestamp},
};
use zgui_platform_headless::Harness;
use zgui_render_wgpu::{Builder, wgpu};
mod support;
use support::Capture;

fn click(h: &mut Harness<impl zgui::platform::AppHandler>, node: NodeRef) {
    let b = node.window_bounds().expect("mounted control");
    assert!(
        b.size.width.0 > 0.0 && b.size.height.0 > 0.0,
        "control must be laid out"
    );
    let p = Point::new(
        CssPx(b.origin.x.0 + b.size.width.0.min(20.0) * 0.5),
        CssPx(b.origin.y.0 + b.size.height.0.min(20.0) * 0.5),
    );
    for action in [
        PointerAction::Moved,
        PointerAction::Pressed,
        PointerAction::Released,
    ] {
        h.deliver_to_first(SurfaceEvent::Pointer {
            action,
            event: PointerEvent::mouse(p).with_button(PointerButton::Primary),
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::ORIGIN,
        });
        h.settle(32);
    }
}
fn tile_pointer(
    h: &mut Harness<impl zgui::platform::AppHandler>,
    action: PointerAction,
    button: PointerButton,
    p: Point<CssPx, zgui::geom::Css>,
) {
    h.deliver_to_first(SurfaceEvent::Pointer {
        action,
        event: PointerEvent::mouse(p).with_button(button),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    h.settle(32);
}
fn key(h: &mut Harness<impl zgui::platform::AppHandler>, event: KeyEvent, modifiers: Modifiers) {
    h.deliver_to_first(SurfaceEvent::Key {
        state: KeyState::Pressed,
        event,
        modifiers,
        timestamp: Timestamp::ORIGIN,
    });
    h.settle(32);
}
fn type_text(h: &mut Harness<impl zgui::platform::AppHandler>, node: NodeRef, text: &str) {
    node.focus();
    h.settle(32);
    key(h, KeyEvent::character("a"), Modifiers::CONTROL);
    key(h, KeyEvent::character(text), Modifiers::NONE);
}

#[test]
#[ignore = "requires a graphics device; run with --ignored"]
fn mounted_tabs_edit_recall_adjust_and_restore() {
    let profile = tempfile::tempdir().unwrap();
    let profile_path = profile.path().to_path_buf();
    let mut store = Some(okcolor_app::persistence::StateStore::open(&profile_path).unwrap());
    let state = Rc::new(Cell::new(None));
    let built = state.clone();
    let frames = Arc::new(Mutex::new(Vec::new()));
    let capture = frames.clone();
    let factory = move |_: &Arc<dyn Surface>, target: RenderTarget| {
        let mut renderer = Builder::new()
            .offscreen(target, wgpu::TextureFormat::Rgba8Unorm, false)
            .map_err(zgui::runtime::AppError::GpuUnavailable)?;
        zgui_render_vector_vello::attach(&mut renderer, target.size);
        Ok(Box::new(Capture {
            renderer,
            frames: capture.clone(),
        }) as Box<dyn Renderer>)
    };
    app()
        .with_size(330.0, 560.0)
        .with_fonts(fonts::load())
        .with_stylesheet(STYLE)
        .with_renderer(Box::new(factory))
        .run_on(
            |handler| {
                let mut h = Harness::new(handler);
                h.settle(64);
                let (model, refs): (PickerModel, PickerRefs) = state.get().unwrap();
                let original = model.current();
                refs.menu.focus();
                h.settle(32);
                key(
                    &mut h,
                    KeyEvent::character("C"),
                    Modifiers::CONTROL | Modifiers::SHIFT,
                );
                assert!(refs.sampling.get_untracked());
                key(
                    &mut h,
                    KeyEvent::named(NamedKey::Escape, PhysicalKey::Unidentified(0)),
                    Modifiers::NONE,
                );
                assert!(!refs.sampling.get_untracked());
                assert_eq!(model.current(), original);
                click(&mut h, refs.tabs[0]);
                assert_eq!(model.tab.get_untracked(), Tab::Picker);
                click(&mut h, refs.tabs[1]);
                assert_eq!(model.tab.get_untracked(), Tab::Harmonies);
                let rest = refs.tool_page;
                rest.scroll_to(
                    ScrollTarget::By(Point::new(
                        zgui::geom::DevicePx(0.0),
                        zgui::geom::DevicePx(10000.0),
                    )),
                    ScrollBehavior::Instant,
                );
                h.settle(64);
                let strip = refs.tools[5].window_bounds().unwrap();
                let palette_start = Point::new(
                    CssPx(strip.origin.x.0 + 23.0),
                    CssPx(strip.origin.y.0 + 8.0),
                );
                let tail = refs.tools[2].window_bounds().unwrap();
                let palette_end = Point::new(CssPx(tail.origin.x.0 + 4.0), palette_start.y);
                let last_color = Point::new(CssPx(tail.origin.x.0 - 22.0), palette_start.y);
                let primary = PointerButton::Primary;
                tile_pointer(&mut h, PointerAction::Pressed, primary, palette_start);
                tile_pointer(&mut h, PointerAction::Released, primary, palette_start);
                let locked = model.current();
                assert_ne!(locked, original);
                click(&mut h, refs.tools[6]);
                click(&mut h, refs.tools[0]);
                model.select(original);
                tile_pointer(&mut h, PointerAction::Pressed, primary, palette_start);
                tile_pointer(&mut h, PointerAction::Released, primary, palette_start);
                assert_eq!(
                    model.current(),
                    locked,
                    "generation preserves the locked color"
                );
                tile_pointer(&mut h, PointerAction::Pressed, primary, palette_start);
                tile_pointer(&mut h, PointerAction::Moved, primary, palette_end);
                tile_pointer(&mut h, PointerAction::Released, primary, palette_end);
                model.select(original);
                tile_pointer(&mut h, PointerAction::Pressed, primary, last_color);
                tile_pointer(&mut h, PointerAction::Released, primary, last_color);
                assert_eq!(
                    model.current(),
                    locked,
                    "drop on the trailing target appends"
                );
                model.select(original);
                h.settle(32);
                click(&mut h, refs.library[0]);
                assert_eq!(model.library.get_untracked().swatches[0].color(), original);
                let first_id = model.library.get_untracked().swatches[0].id;
                model.input("#d03264");
                h.settle(32);
                click(&mut h, refs.library[0]);
                let second = model.current();
                let b = refs.library[7].window_bounds().unwrap();
                let first = Point::new(CssPx(b.origin.x.0 + 10.0), CssPx(b.origin.y.0 + 10.0));
                let next = Point::new(CssPx(b.origin.x.0 + 33.0), first.y);
                // Drag a saved tile before its neighbor, preserving the current color and IDs.
                tile_pointer(&mut h, PointerAction::Pressed, PointerButton::Primary, next);
                tile_pointer(&mut h, PointerAction::Moved, PointerButton::Primary, first);
                tile_pointer(
                    &mut h,
                    PointerAction::Released,
                    PointerButton::Primary,
                    first,
                );
                assert_eq!(model.library.get_untracked().swatches[0].color(), second);
                assert_eq!(model.library.get_untracked().swatches[1].id, first_id);
                tile_pointer(
                    &mut h,
                    PointerAction::Pressed,
                    PointerButton::Secondary,
                    next,
                );
                tile_pointer(
                    &mut h,
                    PointerAction::Released,
                    PointerButton::Secondary,
                    next,
                );
                assert_eq!(model.current(), second);
                refs.menu.focus();
                key(&mut h, KeyEvent::character("v"), Modifiers::CONTROL);
                assert_eq!(
                    model.output_untracked(okcolor_core::ColorFormat::Hex),
                    "#4e83a3"
                );
                tile_pointer(&mut h, PointerAction::Pressed, PointerButton::Middle, first);
                tile_pointer(
                    &mut h,
                    PointerAction::Released,
                    PointerButton::Middle,
                    first,
                );
                assert_eq!(model.library.get_untracked().swatches.len(), 1);
                model.select(original);
                refs.menu.focus();
                h.settle(32);
                assert!(!model.editing_text.get_untracked());
                key(&mut h, KeyEvent::character("r"), Modifiers::NONE);
                assert!(
                    model.editing_text.get_untracked(),
                    "R should focus the output field"
                );
                key(&mut h, KeyEvent::character("e0e0"), Modifiers::NONE);
                assert_eq!(refs.codes[1].text_content(), "e0e0", "shortcut selects all");
                key(
                    &mut h,
                    KeyEvent::named(NamedKey::Enter, PhysicalKey::Unidentified(0)),
                    Modifiers::NONE,
                );
                assert_eq!(
                    model.output_untracked(okcolor_core::ColorFormat::Hex),
                    "#00e0e0"
                );
                assert_eq!(
                    refs.codes[1].text_content(),
                    "#00e0e0",
                    "Enter replaces the focused draft"
                );
                model.select(original);
                key(&mut h, KeyEvent::character("3"), Modifiers::CONTROL);
                assert_eq!(model.tab.get_untracked(), Tab::Apca);
                refs.menu.focus();
                h.settle(32);
                key(&mut h, KeyEvent::character("o"), Modifiers::NONE);
                key(&mut h, KeyEvent::character("red"), Modifiers::NONE);
                assert_eq!(refs.codes[0].text_content(), "red");
                key(
                    &mut h,
                    KeyEvent::named(NamedKey::Escape, PhysicalKey::Unidentified(0)),
                    Modifiers::NONE,
                );
                assert_eq!(model.current(), original);
                let config = model.tools.get_untracked().contrast_grid;
                click(&mut h, refs.tools[0]);
                assert_ne!(
                    model.tools.get_untracked().contrast_grid.to_text,
                    config.to_text
                );
                click(&mut h, refs.tools[1]);
                assert_ne!(model.tools.get_untracked().contrast_grid.p3, config.p3);
                type_text(&mut h, refs.tools[3], "65");
                key(
                    &mut h,
                    KeyEvent::named(NamedKey::Enter, PhysicalKey::Unidentified(0)),
                    Modifiers::NONE,
                );
                assert_eq!(model.tools.get_untracked().contrast_grid.levels[0], 65);
                assert_eq!(model.current(), original);
                let grid = refs.tools[5].window_bounds().unwrap();
                let divider = Point::new(
                    CssPx(grid.origin.x.0 + 10.0),
                    CssPx(grid.origin.y.0 + grid.size.height.0 / 2.0 - 1.0),
                );
                let edge = Point::new(divider.x, CssPx(grid.origin.y.0));
                tile_pointer(&mut h, PointerAction::Pressed, primary, divider);
                tile_pointer(&mut h, PointerAction::Moved, primary, edge);
                tile_pointer(&mut h, PointerAction::Released, primary, edge);
                assert_eq!(model.tools.get_untracked().contrast_grid.split, 0);
                click(&mut h, refs.tools[6]);
                assert_eq!(model.tools.get_untracked().contrast_grid.split, 2);
                let saved = model.saved_state_untracked();
                assert_eq!(SavedState::decode(&saved.encode().unwrap()).unwrap(), saved);
                if let Ok(directory) = std::env::var("OKCOLOR_TOOLS_CAPTURE_DIR") {
                    // Synthetic color fixtures make density, clipping and all three pages reviewable.
                    model.library.update(|library| {
                        for index in 0..35 {
                            library
                                .add(
                                    okcolor_core::Oklch::new(
                                        0.35 + f64::from(index % 5) * 0.08,
                                        0.08,
                                        f64::from(index) * 30.0,
                                        1.0,
                                    )
                                    .unwrap(),
                                    None,
                                )
                                .unwrap();
                        }
                    });
                    for (index, name) in [(0, "swatches"), (1, "harmonies"), (2, "apca")] {
                        click(&mut h, refs.tabs[index]);
                        // Clear the tab hover, including its delayed native-style tooltip.
                        tile_pointer(
                            &mut h,
                            PointerAction::Moved,
                            primary,
                            Point::new(CssPx(145.0), CssPx(15.0)),
                        );
                        h.run_for(Duration::from_secs(1), Duration::from_millis(100));
                        h.settle(64);
                        assert!(refs.tool_page.window_bounds().unwrap().size.height.0 > 100.0);
                        std::fs::write(
                            std::path::Path::new(&directory).join(format!("{name}.rgba")),
                            frames.lock().unwrap().last().unwrap().bytes(),
                        )
                        .unwrap();
                        assert_eq!(
                            h.run_for(Duration::from_secs(1), Duration::from_millis(100)),
                            0,
                            "idle tool page must settle"
                        );
                    }
                }
                // A full library remains scrollable and the titlebar stays fixed.
                click(&mut h, refs.tabs[0]);
                model.library.update(|library| {
                    for _ in 0..200 {
                        library.add(original, None).unwrap();
                    }
                });
                h.settle(32);
                refs.tool_page.scroll_to(
                    ScrollTarget::By(zgui::geom::Point::new(
                        zgui::geom::DevicePx(0.0),
                        zgui::geom::DevicePx(10000.0),
                    )),
                    ScrollBehavior::Instant,
                );
                h.settle(32);
                assert!(refs.tool_page.scroll_position().offset.y.0 > 0.0);
                assert!(refs.tool_page.scroll_position().is_at_end_vertically());
                assert!(refs.title.window_bounds().unwrap().origin.y.0 < 32.0);
                click(&mut h, refs.title);
                assert_eq!(model.tab.get_untracked(), Tab::Picker);
                assert!(refs.slice.window_bounds().is_some());
                let expected = model.saved_state_untracked();
                h.shut_down();
                assert_eq!(
                    okcolor_app::persistence::StateStore::open(&profile_path)
                        .unwrap()
                        .load()
                        .unwrap(),
                    expected,
                    "shutdown persists tools and library"
                );
                Ok(())
            },
            move || {
                let model = PickerModel::default();
                okcolor_app::autosave::bind(model, store.take().unwrap(), SavedState::default());
                let refs = PickerRefs::default();
                built.set(Some((model, refs)));
                view! {Picker(model=model,refs=refs)}
            },
        )
        .expect("mounted tool tabs");
}
