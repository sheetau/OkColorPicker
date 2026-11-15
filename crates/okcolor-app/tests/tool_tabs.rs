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
                click(&mut h, refs.tabs[0]);
                assert_eq!(model.tab.get_untracked(), Tab::Swatches);
                click(&mut h, refs.tools[0]);
                assert_eq!(model.library.get_untracked().swatches.len(), 1);
                click(&mut h, refs.tools[1]);
                type_text(&mut h, refs.tools[2], "Accent");
                click(&mut h, refs.tools[3]);
                assert_eq!(model.library.get_untracked().swatches[0].name, "Accent");
                type_text(&mut h, refs.tools[2], "Palette");
                click(&mut h, refs.tools[4]);
                assert_eq!(model.library.get_untracked().folders[0].name, "Palette");
                model.input("#aabbcc");
                h.settle(32);
                click(&mut h, refs.tools[7]);
                assert_eq!(model.current(), original, "tile recalls exact original");
                click(&mut h, refs.tools[6]);
                assert!(model.library.get_untracked().swatches[0].folder.is_some());
                // The active page is unmounted even when its field has focus.
                refs.tools[2].focus();
                h.settle(32);
                key(&mut h, KeyEvent::character("3"), Modifiers::CONTROL);
                assert_eq!(model.tab.get_untracked(), Tab::Harmonies);
                assert!(!model.editing_text.get_untracked());
                let strip = refs.tools[7].window_bounds().unwrap();
                let point = Point::new(
                    CssPx(strip.origin.x.0 + strip.size.width.0 * 0.5),
                    CssPx(strip.origin.y.0 + 16.0),
                );
                for action in [PointerAction::Pressed, PointerAction::Released] {
                    h.deliver_to_first(SurfaceEvent::Pointer {
                        action,
                        event: PointerEvent::mouse(point).with_button(PointerButton::Primary),
                        modifiers: Modifiers::NONE,
                        timestamp: Timestamp::ORIGIN,
                    });
                    h.settle(32);
                }
                assert!(
                    model.current().components()[0] < original.components()[0],
                    "generated shade applies through UI"
                );
                model.undo();
                h.settle(32);
                assert_eq!(model.current(), original);
                for _ in 0..6 {
                    click(&mut h, refs.tools[0]);
                }
                assert_eq!(model.tools.get_untracked().harmony, 1);
                click(&mut h, refs.tools[1]);
                assert_eq!(model.library.get_untracked().swatches.len(), 2);
                key(
                    &mut h,
                    KeyEvent::named(NamedKey::Tab, PhysicalKey::Unidentified(0)),
                    Modifiers::CONTROL,
                );
                assert_eq!(model.tab.get_untracked(), Tab::Apca);
                type_text(&mut h, refs.tools[0], "#ffffff80");
                click(&mut h, refs.tools[1]);
                assert_eq!(model.tools.get_untracked().background, [1.0, 0.0, 0.0, 1.0]);
                assert!(model.message.get_untracked().contains("opaque"));
                type_text(&mut h, refs.tools[0], "#fff");
                click(&mut h, refs.tools[1]);
                type_text(&mut h, refs.tools[2], "60");
                click(&mut h, refs.tools[3]);
                assert!(
                    (okcolor_core::apca(
                        model.current(),
                        model.tools.get_untracked().background(),
                        model.mapping.get_untracked()
                    )
                    .unwrap()
                        - 60.0)
                        .abs()
                        < 0.01
                );
                assert_eq!(
                    &model.current().components()[1..],
                    &original.components()[1..]
                );
                refs.title.focus();
                h.settle(32);
                model.undo();
                h.settle(32);
                assert_eq!(model.current(), original);
                type_text(&mut h, refs.tools[2], "110");
                click(&mut h, refs.tools[3]);
                assert_eq!(model.current(), original);
                assert!(model.message.get_untracked().contains("unreachable"));
                refs.tools[2].focus();
                h.settle(32);
                let saved = model.saved_state_untracked();
                assert_eq!(SavedState::decode(&saved.encode().unwrap()).unwrap(), saved);
                key(
                    &mut h,
                    KeyEvent::named(NamedKey::Tab, PhysicalKey::Unidentified(0)),
                    Modifiers::CONTROL | Modifiers::SHIFT,
                );
                assert_eq!(model.tab.get_untracked(), Tab::Harmonies);
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
