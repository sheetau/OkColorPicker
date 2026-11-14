//! Mounted UI contracts. These exercise actual event dispatch and GPU presentation.
use okcolor_app::{
    autosave, fonts,
    model::PickerModel,
    persistence::{SavedState, StartupState, StateStore},
    ui::{PickerProps, PickerRefs, STYLE},
};
use std::{
    cell::Cell,
    rc::Rc,
    sync::{Arc, Mutex},
    time::Duration,
};
use zgui::{
    geom::{CssPx, DevicePx, Point, Size},
    platform::{PlatformCx, Surface, SurfaceEvent},
    prelude::*,
    render::{RenderTarget, Renderer},
    vocab::{
        ImeEvent, KeyEvent, KeyState, PhysicalKey, PointerAction, PointerEvent, PointerId,
        PointerKind, ScrollDelta, ScrollPhase, Timestamp, WheelEvent,
    },
};
use zgui_platform_headless::Harness;
use zgui_render_wgpu::{Builder, wgpu};
mod support;
use support::Capture;

#[test]
#[ignore = "requires a graphics device; run with --ignored"]
fn mounted_picker_draws_and_drag_is_one_undo() {
    let profile = tempfile::tempdir().unwrap();
    let profile_path = profile.path().to_path_buf();
    let mut startup = StartupState::open(Ok(profile_path.clone()));
    let frames = Arc::new(Mutex::new(Vec::new()));
    let capture = frames.clone();
    let state = Rc::new(Cell::new(None));
    let built = state.clone();
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
        .with_transparent(true)
        .with_fonts(fonts::load())
        .with_stylesheet(STYLE)
        .with_renderer(Box::new(factory))
        .run_on(
            |handler| {
                let mut harness = Harness::new(handler);
                harness.settle(64);
                let (model, refs): (PickerModel, PickerRefs) = state.get().expect("mounted model");
                let before = model.current();
                let saved_before_status = model.saved_state_untracked();
                for status in [
                    okcolor_csp::worker::State::Scanning,
                    okcolor_csp::worker::State::Connected,
                    okcolor_csp::worker::State::Reconnecting,
                    okcolor_csp::worker::State::Idle,
                ] {
                    model.companion.state.set(status);
                    harness.settle(64);
                    let bounds = refs.companion.window_bounds().expect("CSP control");
                    assert!(bounds.origin.x.0 >= 0.0 && bounds.origin.x.0 + bounds.size.width.0 <= 330.0,
                        "CSP status clipped horizontally: {bounds:?}");
                    assert!(bounds.origin.y.0 + bounds.size.height.0 <= 560.0,
                        "CSP status clipped vertically: {bounds:?}");
                    assert_eq!(model.saved_state_untracked(), saved_before_status);
                }
                model.companion.state.set(okcolor_csp::worker::State::Connected);
                model.companion.hint.set("CSP color not confirmed; local color retained".into());
                model.companion.can_resolve.set(true);
                harness.settle(64);
                for node in [refs.csp_pull,refs.csp_send] {
                    let bounds=node.window_bounds().expect("CSP resolution control");
                    assert!(bounds.origin.x.0>=0.0 && bounds.origin.x.0+bounds.size.width.0<=330.0,
                        "CSP resolution control clipped horizontally: {bounds:?}");
                    assert!(bounds.origin.y.0+bounds.size.height.0<=560.0,
                        "CSP resolution control clipped vertically: {bounds:?}");
                }
                if let Ok(path)=std::env::var("OKCOLOR_TEST_CSP_RGBA") {
                    let frames=frames.lock().unwrap();
                    std::fs::write(path,frames.last().unwrap().bytes()).unwrap();
                }
                model.companion.state.set(okcolor_csp::worker::State::Idle);
                model.companion.hint.set(String::new());model.companion.can_resolve.set(false);
                harness.settle(64);
                let bounds = refs.bars[0].window_bounds().expect("lightness bar");
                assert!(bounds.size.width.0 > 70.0, "bar bounds {bounds:?}");
                assert!(
                    bounds.origin.y.0 > 200.0 && bounds.origin.y.0 < 400.0,
                    "bar bounds {bounds:?}"
                );
                for node in refs.codes.into_iter().chain(refs.numbers) {
                    let bounds = node.window_bounds().expect("field bounds");
                    assert!(
                        bounds.origin.y.0 + bounds.size.height.0 <= 560.0,
                        "field clipped: {bounds:?}"
                    );
                }
                if let Ok(path) = std::env::var("OKCOLOR_TEST_RGBA") {
                    let frames = frames.lock().unwrap();
                    let image = frames.last().expect("UI frame");
                    assert_eq!((image.size().width, image.size().height), (330, 560));
                    assert!(!image.is_bgra());
                    std::fs::write(path, image.bytes()).expect("write optional preview");
                }
                let pointer = |action, fraction| SurfaceEvent::Pointer {
                    action,
                    event: PointerEvent::mouse(Point::new(
                        CssPx(bounds.origin.x.0 + bounds.size.width.0 * fraction),
                        CssPx(bounds.origin.y.0 + bounds.size.height.0 * 0.5),
                    ))
                    .with_button(PointerButton::Primary),
                    modifiers: Modifiers::NONE,
                    timestamp: Timestamp::ORIGIN,
                };
                harness.deliver_to_first(pointer(PointerAction::Pressed, 0.3));
                harness.settle(32);
                for fraction in [0.4, 0.5, 0.6, 0.7] {
                    harness.deliver_to_first(pointer(PointerAction::Moved, fraction));
                    harness.settle(32);
                }
                assert!(
                    model.recent.get_untracked().is_empty(),
                    "drag must not enter recents yet"
                );
                harness.deliver_to_first(pointer(PointerAction::Released, 0.7));
                harness.settle(32);
                assert!(
                    (model.current().components()[0] - 0.7).abs() < 1e-5,
                    "drag did not reach L=0.7: {:?}",
                    model.current()
                );
                assert_eq!(model.recent.get_untracked().len(), 1);
                model.undo();
                harness.settle(32);
                assert_eq!(model.current(), before);
                assert!(!model.can_undo.get_untracked());
                // Focus and type through the platform path, not through the model setter.
                refs.numbers[0].focus();
                harness.settle(32);
                let key = |event, modifiers| SurfaceEvent::Key {
                    state: KeyState::Pressed,
                    event,
                    modifiers,
                    timestamp: Timestamp::ORIGIN,
                };
                harness.deliver_to_first(key(KeyEvent::character("a"), Modifiers::CONTROL));
                harness.settle(32);
                for c in "0.625".chars() {
                    harness
                        .deliver_to_first(key(KeyEvent::character(c.to_string()), Modifiers::NONE));
                    harness.settle(32);
                }
                harness.deliver_to_first(key(
                    KeyEvent::named(NamedKey::Enter, PhysicalKey::Unidentified(0)),
                    Modifiers::NONE,
                ));
                harness.settle(32);
                assert!(
                    (model.current().components()[0] - 0.625).abs() < 1e-12,
                    "number field did not commit: {:?}",
                    model.current()
                );
                refs.slice.focus();
                harness.settle(32);
                let precise = model.current();
                refs.numbers[1].focus();
                harness.settle(32);
                harness.deliver_to_first(key(KeyEvent::character("a"), Modifiers::CONTROL));
                harness.deliver_to_first(key(KeyEvent::character("invalid"), Modifiers::NONE));
                harness.deliver_to_first(key(
                    KeyEvent::named(NamedKey::Enter, PhysicalKey::Unidentified(0)),
                    Modifiers::NONE,
                ));
                harness.settle(32);
                assert_eq!(model.current(), precise, "invalid number changed color");
                assert!(!model.message.get_untracked().is_empty());
                harness.deliver_to_first(key(
                    KeyEvent::named(NamedKey::Escape, PhysicalKey::Unidentified(0)),
                    Modifiers::NONE,
                ));
                refs.slice.focus();
                harness.settle(32);
                assert_eq!(
                    model.current(),
                    precise,
                    "Escape/blur quantized untouched chroma"
                );

                // Clipboard text inside a field remains a draft; Enter commits any supported format.
                harness
                    .platform()
                    .clipboard()
                    .write(
                        ClipboardKind::Standard,
                        "#ff000080".into(),
                        Default::default(),
                    )
                    .unwrap();
                refs.codes[0].focus();
                harness.settle(32);
                harness.deliver_to_first(key(KeyEvent::character("a"), Modifiers::CONTROL));
                harness.deliver_to_first(key(KeyEvent::character("v"), Modifiers::CONTROL));
                harness.settle(32);
                assert_eq!(refs.codes[0].text_content(), "#ff000080");
                assert_eq!(
                    model.current(),
                    precise,
                    "paste inside field committed too early"
                );
                harness.deliver_to_first(key(
                    KeyEvent::named(NamedKey::Enter, PhysicalKey::Unidentified(0)),
                    Modifiers::NONE,
                ));
                harness.settle(32);
                assert!((model.current().components()[3] - 128.0 / 255.0).abs() < 1e-12);

                // IME Enter must not commit a partially composed numeric/color draft.
                let committed = model.current();
                harness.deliver_to_first(key(KeyEvent::character("a"), Modifiers::CONTROL));
                harness.deliver_to_first(SurfaceEvent::Ime(ImeEvent::Enabled));
                harness.deliver_to_first(SurfaceEvent::Ime(ImeEvent::Preedit {
                    text: "#000".into(),
                    cursor: Some(4..4),
                }));
                harness.deliver_to_first(key(
                    KeyEvent::named(NamedKey::Enter, PhysicalKey::Unidentified(0)),
                    Modifiers::NONE,
                ));
                harness.settle(32);
                assert_eq!(model.current(), committed);
                harness.deliver_to_first(SurfaceEvent::Ime(ImeEvent::Commit("#000".into())));
                harness.settle(32);
                assert_eq!(
                    model.current(),
                    committed,
                    "IME commit alone must leave a draft"
                );
                harness.deliver_to_first(key(
                    KeyEvent::named(NamedKey::Escape, PhysicalKey::Unidentified(0)),
                    Modifiers::NONE,
                ));
                refs.slice.focus();
                harness.settle(32);
                assert_eq!(model.current(), committed);

                // The same paste outside text entry applies immediately as one color action.
                harness
                    .platform()
                    .clipboard()
                    .write(
                        ClipboardKind::Standard,
                        "#336699".into(),
                        Default::default(),
                    )
                    .unwrap();
                harness.deliver_to_first(key(KeyEvent::character("v"), Modifiers::CONTROL));
                harness.settle(32);
                assert_eq!(
                    model.current(),
                    okcolor_core::parse_color("#336699", 0.0).unwrap()
                );
                let before_cancel = model.current();
                harness.deliver_to_first(pointer(PointerAction::Pressed, 0.2));
                harness.deliver_to_first(pointer(PointerAction::Moved, 0.9));
                harness.deliver_to_first(pointer(PointerAction::Cancelled, 0.9));
                harness.settle(32);
                assert_eq!(
                    model.current(),
                    before_cancel,
                    "cancel must restore the drag origin"
                );
                // Picking continues to use CSS input coordinates after a 2x DPI change.
                harness.deliver_to_first(SurfaceEvent::ScaleFactorChanged {
                    scale_factor: 2.0,
                    size: Size::new(DevicePx(660.0), DevicePx(1120.0)),
                });
                harness.settle(64);
                let scaled = refs.bars[0].window_bounds().unwrap();
                assert!((scaled.size.width.0 / bounds.size.width.0 - 2.0).abs() < 0.05);
                let at = Point::new(
                    CssPx((scaled.origin.x.0 + scaled.size.width.0 * 0.8) / 2.0),
                    CssPx((scaled.origin.y.0 + scaled.size.height.0 * 0.5) / 2.0),
                );
                for action in [PointerAction::Pressed, PointerAction::Released] {
                    harness.deliver_to_first(SurfaceEvent::Pointer {
                        action,
                        event: PointerEvent::mouse(at).with_button(PointerButton::Primary),
                        modifiers: Modifiers::NONE,
                        timestamp: Timestamp::ORIGIN,
                    });
                    harness.settle(32);
                }
                assert!((model.current().components()[0] - 0.8).abs() < 1e-5);
                let slice = refs.slice.window_bounds().unwrap();
                let slice_event = |action, x, y| SurfaceEvent::Pointer {
                    action,
                    event: PointerEvent::mouse(Point::new(
                        CssPx((slice.origin.x.0 + slice.size.width.0 * x) / 2.0),
                        CssPx((slice.origin.y.0 + slice.size.height.0 * y) / 2.0),
                    ))
                    .with_button(PointerButton::Primary),
                    modifiers: Modifiers::NONE,
                    timestamp: Timestamp::ORIGIN,
                };
                let before_slice = model.current();
                for action in [PointerAction::Pressed, PointerAction::Released] {
                    harness.deliver_to_first(slice_event(action, 0.95, 0.05));
                    harness.settle(32);
                }
                assert_eq!(
                    model.current(),
                    before_slice,
                    "outside-gamut click must do nothing"
                );
                harness.deliver_to_first(slice_event(PointerAction::Pressed, 0.05, 0.5));
                harness.settle(32);
                let last_valid = model.current();
                assert!((last_valid.components()[1] - 0.02).abs() < 1e-6);
                harness.deliver_to_first(slice_event(PointerAction::Moved, 0.95, 0.05));
                harness.deliver_to_first(slice_event(PointerAction::Released, 0.95, 0.05));
                harness.settle(32);
                assert_eq!(
                    model.current(),
                    last_valid,
                    "leaving the gamut must retain the last valid sample"
                );
                model.undo();
                harness.settle(32);
                assert_eq!(model.current(), before_slice);
                // All 24 recent colors remain reachable in the compact strip.
                for index in 0..24 {
                    model.select(okcolor_core::Oklch::new(0.6,0.08,f64::from(index)*15.0,1.0).unwrap());
                }
                harness.settle(64);
                let recent=refs.recents.window_bounds().unwrap();
                let last=Point::new(CssPx((recent.origin.x.0+recent.size.width.0-4.0)/2.0),CssPx((recent.origin.y.0+recent.size.height.0*0.5)/2.0));
                harness.deliver_to_first(SurfaceEvent::Wheel {event:WheelEvent {
                    delta:ScrollDelta::Pixels(Size::new(CssPx(0.0),CssPx(1000.0))),phase:ScrollPhase::Discrete,
                    position:last,id:PointerId::MOUSE,kind:PointerKind::Mouse,
                },modifiers:Modifiers::NONE,timestamp:Timestamp::ORIGIN});
                harness.settle(32);
                assert!(refs.recents.scroll_position().offset.x.0>0.0);
                assert!(refs.recents.scroll_position().is_at_end_horizontally());
                if let Ok(path)=std::env::var("OKCOLOR_TEST_RECENTS_RGBA") {
                    let frames=frames.lock().unwrap();
                    std::fs::write(path,frames.last().unwrap().bytes()).unwrap();
                }
                for action in [PointerAction::Pressed,PointerAction::Released] {
                    harness.deliver_to_first(SurfaceEvent::Pointer {action,event:PointerEvent::mouse(last).with_button(PointerButton::Primary),modifiers:Modifiers::NONE,timestamp:Timestamp::ORIGIN});
                    harness.settle(32);
                }
                assert_eq!(model.current(),okcolor_core::Oklch::new(0.6,0.08,0.0,1.0).unwrap(),"oldest recent color is reachable");
                assert_eq!(refs.recents.scroll_position().offset.x.0,0.0,"recalling a color reveals the newest entry");
                refs.slice.focus();
                harness.settle(32);
                assert_eq!(
                    harness.run_for(Duration::from_secs(2), Duration::from_millis(100)),
                    0,
                    "idle UI must settle"
                );
                // A final commit immediately followed by shutdown must not depend on another frame.
                model.input("oklch(.6543210987654321 .3210987654321098 123.4567890123456 / .3456789012345678)");
                let expected = model.saved_state_untracked();
                harness.shut_down();
                let reopened =
                    StateStore::open(&profile_path).expect("shutdown released the profile");
                assert_eq!(
                    reopened.load().unwrap(),
                    expected,
                    "shutdown flushed the final committed snapshot"
                );
                Owner::new().with(|| {
                    let model = PickerModel::restore(reopened.load().unwrap());
                    assert_eq!(model.current(), expected.current);
                    assert_eq!(model.recent.get_untracked(), expected.recent);
                    assert!(!model.can_undo.get_untracked());
                });
                Ok(())
            },
            move || {
                let model = PickerModel::restore(startup.state.clone());
                autosave::bind(model, startup.store.take().unwrap(), SavedState::default());
                let refs = PickerRefs::default();
                built.set(Some((model, refs)));
                view! { Picker(model=model,refs=refs) }
            },
        )
        .expect("render picker");
}
