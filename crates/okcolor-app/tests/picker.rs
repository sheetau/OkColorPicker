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

fn check_numeric_adjustments(
    harness: &mut Harness<impl zgui::platform::AppHandler>,
    model: PickerModel,
    refs: PickerRefs,
) {
    let original = model.current();
    let key = |key, modifiers| SurfaceEvent::Key {
        state: KeyState::Pressed,
        event: KeyEvent::named(key, PhysicalKey::Unidentified(0)),
        modifiers,
        timestamp: Timestamp::ORIGIN,
    };
    let type_draft = |harness: &mut Harness<_>, text: &str| {
        for (text, modifiers) in [("a", Modifiers::CONTROL), (text, Modifiers::NONE)] {
            harness.deliver_to_first(SurfaceEvent::Key {
                state: KeyState::Pressed,
                event: KeyEvent::character(text),
                modifiers,
                timestamp: Timestamp::ORIGIN,
            });
        }
        harness.settle(32);
    };
    let wheel = |node: NodeRef, delta, phase, modifiers| {
        let bounds = node.window_bounds().unwrap();
        SurfaceEvent::Wheel {
            event: WheelEvent {
                delta,
                phase,
                position: Point::new(
                    CssPx(bounds.origin.x.0 + bounds.size.width.0 * 0.5),
                    CssPx(bounds.origin.y.0 + bounds.size.height.0 * 0.5),
                ),
                id: PointerId::MOUSE,
                kind: PointerKind::Mouse,
            },
            modifiers,
            timestamp: Timestamp::ORIGIN,
        }
    };
    let lines = |y| ScrollDelta::Lines { x: 0.0, y };
    let pixels = |y| ScrollDelta::Pixels(Size::new(CssPx(0.0), CssPx(y)));
    model.select(okcolor_core::Oklch::new(0.625, 0.123456789, 359.5, 0.5).unwrap());
    refs.numbers[1].focus();
    harness.settle(32);
    for _ in 0..2 {
        harness.deliver_to_first(key(NamedKey::ArrowUp, Modifiers::CONTROL));
        harness.settle(32);
    }
    assert!((model.current().components()[1] - 0.123656789).abs() < 1e-12);
    assert_eq!(refs.numbers[1].text_content(), "0.1237");
    harness.deliver_to_first(key(NamedKey::ArrowDown, Modifiers::SHIFT));
    harness.settle(32);
    assert!((model.current().components()[1] - 0.113656789).abs() < 1e-12);

    // A valid draft and step are one history action; hue wraps without rounding its base.
    refs.numbers[2].focus();
    harness.settle(32);
    let before_draft = model.current();
    type_draft(harness, "359.75");
    harness.deliver_to_first(key(NamedKey::ArrowUp, Modifiers::NONE));
    harness.settle(32);
    assert_eq!(model.current().components()[2], 0.75);
    model.undo();
    assert_eq!(model.current(), before_draft);

    refs.numbers[0].focus();
    harness.settle(32);
    for invalid in ["-", "NaN", "1.1"] {
        type_draft(harness, invalid);
        harness.deliver_to_first(key(NamedKey::ArrowUp, Modifiers::NONE));
        harness.deliver_to_first(wheel(
            refs.numbers[0],
            lines(-1.0),
            ScrollPhase::Discrete,
            Modifiers::NONE,
        ));
        harness.settle(32);
        assert_eq!(model.current(), before_draft);
        assert_eq!(refs.numbers[0].text_content(), invalid);
        assert!(!model.message.get_untracked().is_empty());
    }
    harness.deliver_to_first(key(NamedKey::Escape, Modifiers::NONE));
    harness.settle(32);
    assert!(!model.editing_text.get_untracked());
    refs.numbers[0].focus();
    harness.settle(32);
    type_draft(harness, "0.999");
    harness.deliver_to_first(key(NamedKey::ArrowUp, Modifiers::NONE));
    harness.settle(32);
    assert_eq!(model.current().components()[0], 1.0);
    refs.numbers[3].focus();
    harness.settle(32);
    type_draft(harness, "0.001");
    harness.deliver_to_first(key(NamedKey::ArrowDown, Modifiers::NONE));
    harness.settle(32);
    assert_eq!(model.current().components()[3], 0.0);

    refs.numbers[1].focus();
    harness.settle(32);
    let before_wheel = model.current();
    let recent_before_wheel = model.recent.get_untracked();
    harness.deliver_to_first(wheel(
        refs.numbers[1],
        pixels(-11.0),
        ScrollPhase::Started,
        Modifiers::CONTROL,
    ));
    harness.settle(32);
    assert_eq!(model.current(), before_wheel);
    harness.deliver_to_first(wheel(
        refs.numbers[1],
        pixels(-11.0),
        ScrollPhase::Moved,
        Modifiers::CONTROL,
    ));
    harness.settle(32);
    assert!(
        (model.current().components()[1] - before_wheel.components()[1] - 0.0001).abs() < 1e-12
    );
    let after_wheel = model.current();
    harness.deliver_to_first(wheel(
        refs.numbers[1],
        pixels(-11.0),
        ScrollPhase::Moved,
        Modifiers::CONTROL,
    ));
    harness.deliver_to_first(wheel(
        refs.numbers[1],
        pixels(0.0),
        ScrollPhase::Ended,
        Modifiers::CONTROL,
    ));
    harness.deliver_to_first(wheel(
        refs.numbers[1],
        pixels(-11.0),
        ScrollPhase::Moved,
        Modifiers::CONTROL,
    ));
    harness.deliver_to_first(wheel(
        refs.numbers[1],
        pixels(-100.0),
        ScrollPhase::Momentum,
        Modifiers::CONTROL,
    ));
    harness.settle(32);
    assert_eq!(
        model.current(),
        after_wheel,
        "gesture remainder/momentum changed color"
    );
    harness.deliver_to_first(wheel(
        refs.numbers[1],
        lines(1.0),
        ScrollPhase::Discrete,
        Modifiers::SHIFT,
    ));
    harness.settle(32);
    assert!((model.current().components()[1] - after_wheel.components()[1] + 0.01).abs() < 1e-12);
    assert_eq!(model.recent.get_untracked(), recent_before_wheel);
    assert_eq!(model.saved_state_untracked().recent, recent_before_wheel);
    let stepped = model.current();
    model.undo();
    assert_eq!(model.current(), after_wheel);
    model.redo();
    assert_eq!(model.current(), stepped);

    let before_ime = model.current();
    harness.deliver_to_first(SurfaceEvent::Ime(ImeEvent::Enabled));
    harness.deliver_to_first(SurfaceEvent::Ime(ImeEvent::Preedit {
        text: "0.2".into(),
        cursor: Some(3..3),
    }));
    harness.deliver_to_first(key(NamedKey::ArrowUp, Modifiers::NONE));
    harness.deliver_to_first(wheel(
        refs.numbers[1],
        lines(-1.0),
        ScrollPhase::Discrete,
        Modifiers::NONE,
    ));
    harness.settle(32);
    assert_eq!(model.current(), before_ime);
    harness.deliver_to_first(SurfaceEvent::Ime(ImeEvent::Commit(String::new().into())));
    harness.settle(32);
    assert_eq!(model.current(), before_ime, "IME commit changed color");
    harness.deliver_to_first(SurfaceEvent::Ime(ImeEvent::Disabled));
    harness.deliver_to_first(key(NamedKey::Escape, Modifiers::NONE));
    harness.settle(32);
    assert_eq!(model.current(), before_ime, "Escape changed color");
    refs.slice.focus();
    harness.settle(32);
    assert_eq!(model.current(), before_ime, "blur changed color");
    harness.deliver_to_first(wheel(
        refs.numbers[1],
        lines(-1.0),
        ScrollPhase::Discrete,
        Modifiers::NONE,
    ));
    harness.settle(32);
    assert!(
        (model.current().components()[1] - before_ime.components()[1] - 0.001).abs() < 1e-12,
        "hovered field must accept wheel without focus"
    );
    model.select(original);
    harness.settle(32);
}

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
                if let Ok(path) = std::env::var("OKCOLOR_TEST_RGBA") {
                    let frames = frames.lock().unwrap();
                    let image = frames.last().expect("UI frame");
                    assert_eq!((image.size().width, image.size().height), (330, 560));
                    assert!(!image.is_bgra());
                    std::fs::write(path, image.bytes()).expect("write optional preview");
                }
                for node in [refs.minimize, refs.maximize, refs.close] {
                    let bounds = node.window_bounds().unwrap();
                    assert!((bounds.size.width.0 - 36.0).abs() < 0.01);
                    assert!((bounds.size.height.0 - 31.0).abs() < 0.01);
                }
                for (index, pressed, expected) in [
                    (0, false, [22, 22, 22]), (0, true, [31, 31, 31]),
                    (2, false, [232, 17, 32]), (2, true, [186, 14, 26]),
                ] {
                    refs.caption.set(okcolor_app::window_frame::CaptionVisual {
                        hovered: Some(index), pressed: pressed.then_some(index), maximized: pressed,
                        ..Default::default()
                    });
                    harness.settle(32);
                    let node = [refs.minimize, refs.maximize, refs.close][index];
                    let bounds = node.window_bounds().unwrap();
                    let frames = frames.lock().unwrap();
                    let pixels = frames.last().unwrap();
                    let offset = ((bounds.origin.y.0 as usize + 3) * 330 + bounds.origin.x.0 as usize + 3) * 4;
                    for (actual, expected) in pixels.bytes()[offset..offset + 3].iter().zip(expected) {
                        assert!(actual.abs_diff(expected) <= 1, "caption background {actual} != {expected}");
                    }

                }
                refs.caption.set(Default::default());
                harness.settle(32);
                #[cfg(target_os="windows")]
                {
                    let frames = frames.lock().unwrap();
                    let pixels = frames.last().unwrap().bytes();
                    for node in [refs.minimize,refs.maximize,refs.close] {
                        let bounds = node.window_bounds().unwrap();
                        let (x,y) = (bounds.origin.x.0 as usize,bounds.origin.y.0 as usize);
                        let mut ink = vec![];
                        for dy in 0..31 { for dx in 0..36 {
                            let offset = ((y+dy)*330+x+dx)*4;
                            if pixels[offset..offset+3].iter().all(|v| *v>30) { ink.push((dx,dy)); }
                        }}
                        assert!(!ink.is_empty(), "installed caption glyph must be painted");
                        let center_x = (ink.iter().map(|p|p.0).min().unwrap()+ink.iter().map(|p|p.0).max().unwrap()) as f32/2.0;
                        let center_y = (ink.iter().map(|p|p.1).min().unwrap()+ink.iter().map(|p|p.1).max().unwrap()) as f32/2.0;
                        assert!((center_x-17.5).abs()<=0.5 && (center_y-15.0).abs()<=0.5,"caption glyph is not centered: {center_x},{center_y}");
                    }
                }
                // SVG ink follows the control hover color, with no hover plate.
                let swap_bounds=refs.swap.window_bounds().unwrap();
                let swap_ink=|| {
                    let frames=frames.lock().unwrap();let image=frames.last().unwrap();
                    let mut max=0;
                    for y in 0..16 {for x in 0..16 { max=max.max(image.rgba(swap_bounds.origin.x.0 as i32+x,swap_bounds.origin.y.0 as i32+y)[0]); }}
                    max
                };
                assert!((60..=70).contains(&swap_ink()),"normal swap ink must be #42");
                harness.deliver_to_first(SurfaceEvent::Pointer {action:PointerAction::Moved,event:PointerEvent::mouse(Point::new(CssPx(swap_bounds.origin.x.0+8.0),CssPx(swap_bounds.origin.y.0+8.0))),modifiers:Modifiers::NONE,timestamp:Timestamp::ORIGIN});
                harness.settle(32);
                assert!(swap_ink()>210,"hovered SVG ink must be #e0");
                harness.deliver_to_first(SurfaceEvent::Pointer {action:PointerAction::Moved,event:PointerEvent::mouse(Point::new(CssPx(329.0),CssPx(559.0))),modifiers:Modifiers::NONE,timestamp:Timestamp::ORIGIN});
                harness.settle(32);
                let before = model.current();
                model.companion.state.set(okcolor_csp::worker::State::Failed("Show the CSP companion QR code".into()));
                harness.settle(32);
                assert_eq!(refs.status_notice.text_content(),"Show the CSP companion QR code");
                harness.run_for(Duration::from_secs(5),Duration::from_millis(100));
                assert_eq!(refs.status_notice.text_content(),model.display.get_untracked().label());
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
                let bounds = refs.bars[2].window_bounds().expect("hue bar");
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
                    (model.current().components()[2] / 360.0 - 0.7).abs() < 1e-5,
                    "drag did not reach H=252: {:?}",
                    model.current()
                );
                assert_eq!(model.recent.get_untracked().len(), 1);
                model.undo();
                harness.settle(32);
                assert_eq!(model.current(), before);
                assert!(!model.can_undo.get_untracked());
                // A numeric scrub is one history transaction; clicking instead enters text.
                let number = refs.numbers[1].window_bounds().unwrap();
                let scrub = |action,dx| SurfaceEvent::Pointer {
                    action,
                    event: PointerEvent::mouse(Point::new(CssPx(number.origin.x.0+20.0+dx),CssPx(number.origin.y.0+10.0))).with_button(PointerButton::Primary),
                    modifiers: Modifiers::NONE,timestamp: Timestamp::ORIGIN,
                };
                let before_scrub=model.current();
                harness.deliver_to_first(scrub(PointerAction::Pressed,0.0));harness.settle(32);
                assert!(!model.editing_text.get_untracked(),"press alone must not enter text editing");
                for dx in [3.5,8.0,16.0,24.0] {
                    harness.deliver_to_first(scrub(PointerAction::Moved,dx));harness.settle(32);
                    assert!(!model.editing_text.get_untracked(),"scrubbing must keep the caret outside the field");
                    if dx<4.0 {assert_eq!(model.current(),before_scrub);}
                }
                harness.deliver_to_first(scrub(PointerAction::Released,24.0));harness.settle(32);
                assert!((model.current().components()[1]-before_scrub.components()[1]-0.006).abs()<1e-12);
                model.undo();harness.settle(32);assert_eq!(model.current(),before_scrub);
                harness.deliver_to_first(scrub(PointerAction::Pressed,0.0));
                harness.deliver_to_first(scrub(PointerAction::Released,0.0));harness.settle(32);
                assert!(model.editing_text.get_untracked(),"plain click must enter text editing");

                harness.deliver_to_first(scrub(PointerAction::Pressed,0.0));
                harness.deliver_to_first(scrub(PointerAction::Moved,12.0));harness.settle(32);
                assert!(!model.dragging.get_untracked(),"a focused field must allow text selection");
                harness.deliver_to_first(scrub(PointerAction::Released,12.0));harness.settle(32);
                assert_eq!(model.current(), before_scrub);
                refs.menu.focus();harness.settle(32);
                harness.deliver_to_first(scrub(PointerAction::Pressed,0.0));
                harness.deliver_to_first(scrub(PointerAction::Moved,12.0));harness.settle(32);
                assert!(model.dragging.get_untracked());
                harness.deliver_to_first(SurfaceEvent::Key {state:KeyState::Pressed,event:KeyEvent::named(NamedKey::Escape,PhysicalKey::Unidentified(0)),modifiers:Modifiers::NONE,timestamp:Timestamp::ORIGIN});
                harness.settle(32);
                assert!(!model.dragging.get_untracked());assert_eq!(model.current(),before_scrub);
                harness.deliver_to_first(scrub(PointerAction::Released,12.0));harness.settle(32);

                // Both gamut flags remain independent, even when the larger gamut dominates.
                refs.slice.focus();harness.settle(32);
                let key = |event, modifiers| SurfaceEvent::Key {
                    state: KeyState::Pressed,
                    event,
                    modifiers,
                    timestamp: Timestamp::ORIGIN,
                };
                let click = |harness: &mut Harness<_>,node: NodeRef| {
                    let b=node.window_bounds().unwrap();
                    for action in [PointerAction::Pressed,PointerAction::Released] {
                        harness.deliver_to_first(SurfaceEvent::Pointer {action,event:PointerEvent::mouse(Point::new(CssPx(b.origin.x.0+b.size.width.0*0.5),CssPx(b.origin.y.0+b.size.height.0*0.5))).with_button(PointerButton::Primary),modifiers:Modifiers::NONE,timestamp:Timestamp::ORIGIN});
                        harness.settle(32);
                    }
                };
                let original_color=model.current();
                click(&mut harness,refs.menu);
                assert!(refs.gamut_toggles[0].window_bounds().is_some(),"Settings did not open after cancelling a captured scrub");
                assert!(refs.settings_thumb.window_bounds().is_none());
                harness.deliver_to_first(SurfaceEvent::Resized(Size::new(DevicePx(330.0),DevicePx(150.0))));
                harness.settle(64);
                let thumb=refs.settings_thumb.window_bounds().expect("short settings page scrollbar");
                assert_eq!((thumb.origin.x.0,thumb.origin.y.0,thumb.size.width.0),(320.0,32.0,10.0));
                let pixel=|x,y|frames.lock().unwrap().last().unwrap().rgba(x,y)[0];
                assert_eq!(pixel(324,35),22);
                let mouse=|action,y|SurfaceEvent::Pointer {action,event:PointerEvent::mouse(Point::new(CssPx(325.0),CssPx(y))).with_button(PointerButton::Primary),modifiers:Modifiers::NONE,timestamp:Timestamp::ORIGIN};
                for action in [PointerAction::Moved,PointerAction::Pressed] { harness.deliver_to_first(mouse(action,36.0));harness.settle(32); }
                assert_eq!(pixel(324,35),66);
                harness.deliver_to_first(mouse(PointerAction::Moved,115.0));harness.settle(32);
                harness.deliver_to_first(mouse(PointerAction::Released,115.0));harness.settle(32);
                assert!(refs.settings_viewport.scroll_position().is_at_end_vertically());
                let thumb=refs.settings_thumb.window_bounds().unwrap();
                assert!((thumb.origin.y.0+thumb.size.height.0-118.0).abs()<=1.0);
                harness.deliver_to_first(SurfaceEvent::Resized(Size::new(DevicePx(330.0),DevicePx(560.0))));
                harness.settle(64);
                click(&mut harness,refs.axis_trigger);
                harness.deliver_to_first(key(KeyEvent::named(NamedKey::Escape,PhysicalKey::Unidentified(0)),Modifiers::NONE));
                harness.settle(32);
                assert!(refs.axis_options[0].window_bounds().is_none() && refs.axis_trigger.window_bounds().is_some(),"Escape closes the chart menu first");
                for (index,axis) in [okcolor_app::workspace::FixedAxis::Lightness,okcolor_app::workspace::FixedAxis::Chroma,okcolor_app::workspace::FixedAxis::Hue].into_iter().enumerate() {
                    click(&mut harness,refs.axis_trigger);
                    assert_eq!(refs.axis_options[index].text_content(), ["Chroma","Hue","Lightness"][index]);
                    click(&mut harness,refs.axis_options[index]);
                    assert_eq!(model.tools.get_untracked().fixed_axis,axis);
                    click(&mut harness,refs.menu);
                    assert!(refs.bars[index].window_bounds().is_some());
                    assert_eq!(refs.slice.text_content(),if index==0 {"C"} else {"L"});
                    click(&mut harness,refs.menu);
                }
                assert!(!model.tools.get_untracked().always_snap);
                click(&mut harness,refs.always_snap);
                assert!(model.tools.get_untracked().always_snap);
                click(&mut harness,refs.always_snap);
                click(&mut harness,refs.gamut_toggles[0]);
                assert!(model.show_p3.get_untracked());
                assert_eq!(model.gamut.get_untracked(),okcolor_core::Gamut::DisplayP3);
                click(&mut harness,refs.gamut_toggles[1]);
                assert_eq!(model.max_chroma.get_untracked(),0.47);
                click(&mut harness,refs.gamut_toggles[0]);
                assert!(!model.show_p3.get_untracked());
                assert_eq!(model.gamut.get_untracked(),okcolor_core::Gamut::Rec2020);
                click(&mut harness,refs.gamut_toggles[1]);
                assert_eq!(model.max_chroma.get_untracked(),0.37);
                assert_eq!(model.gamut.get_untracked(),okcolor_core::Gamut::Srgb);
                assert_eq!(model.current(),original_color);
                click(&mut harness,refs.menu);
                let height=refs.slice.window_bounds().unwrap().size.height.0;
                model.message.set("A long diagnostic message that must stay on a single line without changing the gamut height or the window size".repeat(4));
                model.companion.hint.set("An equally long companion status message".repeat(8));
                model.companion.can_resolve.set(true);
                model.storage_message.set("A long storage diagnostic".repeat(8));
                model.can_save.set(true);
                harness.settle(32);
                assert_eq!(refs.slice.window_bounds().unwrap().size.height.0,height);
                model.message.set(String::new());model.companion.hint.set(String::new());
                model.companion.can_resolve.set(false);model.storage_message.set(String::new());
                harness.settle(32);
                // Focus and type through the platform path, not through the model setter.
                refs.numbers[0].focus();
                harness.settle(32);
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
                // Slot selection changes only which color is edited. The separate
                // swap control exchanges both colors as one undoable transaction.
                harness.deliver_to_first(SurfaceEvent::ScaleFactorChanged {
                    scale_factor: 1.0, size: Size::new(DevicePx(330.0), DevicePx(560.0)),
                });
                harness.settle(64);
                let pair = model.colors();
                let sub = refs.swatches[1].window_bounds().unwrap();
                let at = Point::new(CssPx(sub.origin.x.0+32.0), CssPx(sub.origin.y.0+32.0));
                for action in [PointerAction::Pressed,PointerAction::Released] {
                    harness.deliver_to_first(SurfaceEvent::Pointer {action,event:PointerEvent::mouse(at).with_button(PointerButton::Primary),modifiers:Modifiers::NONE,timestamp:Timestamp::ORIGIN});
                    harness.settle(32);
                }
                assert_eq!(model.selected.get_untracked(),1);
                assert_eq!(model.colors(),pair);
                if let Ok(path)=std::env::var("OKCOLOR_SLOT_RGBA") { std::fs::write(path,frames.lock().unwrap().last().unwrap().bytes()).unwrap(); }
                // The overlapping region now belongs to the selected sub swatch.
                let at = Point::new(CssPx(sub.origin.x.0+5.0), CssPx(sub.origin.y.0+5.0));
                for action in [PointerAction::Pressed,PointerAction::Released] {
                    harness.deliver_to_first(SurfaceEvent::Pointer {action,event:PointerEvent::mouse(at).with_button(PointerButton::Primary),modifiers:Modifiers::NONE,timestamp:Timestamp::ORIGIN});
                    harness.settle(32);
                }
                assert_eq!(model.selected.get_untracked(),1);
                let swap = refs.swap.window_bounds().unwrap();
                let at = Point::new(CssPx(swap.origin.x.0+8.0),CssPx(swap.origin.y.0+8.0));
                for action in [PointerAction::Pressed,PointerAction::Released] {
                    harness.deliver_to_first(SurfaceEvent::Pointer {action,event:PointerEvent::mouse(at).with_button(PointerButton::Primary),modifiers:Modifiers::NONE,timestamp:Timestamp::ORIGIN});
                    harness.settle(32);
                }
                assert_eq!(model.colors(),[pair[1],pair[0]]);
                model.undo();
                assert_eq!(model.colors(),pair);
                model.select_slot(0);

                // Start inside the triangular hue thumb and drag along the bar.
                model.set_channel(2,180.0);
                harness.settle(32);
                let bar=refs.bars[2].window_bounds().unwrap();
                for (action,x) in [(PointerAction::Pressed,0.5),(PointerAction::Moved,0.75),(PointerAction::Moved,-2.0),(PointerAction::Released,-2.0)] {
                    let at=Point::new(CssPx(bar.origin.x.0+bar.size.width.0*x),CssPx(bar.origin.y.0+bar.size.height.0-2.0));
                    harness.deliver_to_first(SurfaceEvent::Pointer {action,event:PointerEvent::mouse(at).with_button(PointerButton::Primary),modifiers:Modifiers::NONE,timestamp:Timestamp::ORIGIN});
                    harness.settle(32);
                }
                assert!((model.current().components()[2]/360.0-0.75).abs()<1e-6,"protruding knob did not capture the drag");
                model.undo();
                assert_eq!(model.current().components()[2],180.0);

                // Snapping must use the same membership crossing as the vertical ink.
                let before_snap=model.current();
                let at_hue=|h|okcolor_core::Oklch::new(0.65,0.24,h,1.0).unwrap();
                model.select(at_hue(0.0));harness.settle(32);
                let edge=(10..350).find(|h|at_hue(f64::from(*h)).in_gamut(okcolor_core::Gamut::Srgb)!=at_hue(f64::from(*h-1)).in_gamut(okcolor_core::Gamut::Srgb)).unwrap();
                let point=Point::new(CssPx(bar.origin.x.0+bar.size.width.0*edge as f32/360.0+2.0),CssPx(bar.origin.y.0+8.0));
                for action in [PointerAction::Pressed,PointerAction::Released] {
                    harness.deliver_to_first(SurfaceEvent::Pointer {action,event:PointerEvent::mouse(point).with_button(PointerButton::Primary),modifiers:Modifiers::NONE,timestamp:Timestamp::ORIGIN});
                    harness.settle(32);
                }
                assert!((model.current().components()[2]-f64::from(edge)).abs()>0.5,"ordinary clicks follow the pointer");
                for action in [PointerAction::Pressed,PointerAction::Released] {
                    harness.deliver_to_first(SurfaceEvent::Pointer {action,event:PointerEvent::mouse(point).with_button(PointerButton::Primary),modifiers:Modifiers::SHIFT,timestamp:Timestamp::ORIGIN});
                    harness.settle(32);
                }
                let snapped=model.current().components()[2];
                assert!((f64::from(edge-1)..=f64::from(edge)).contains(&snapped));
                assert_ne!(at_hue(snapped-0.01).in_gamut(okcolor_core::Gamut::Srgb),at_hue(snapped+0.01).in_gamut(okcolor_core::Gamut::Srgb));
                model.select(before_snap);harness.settle(32);
                for (width,height) in [(330.0,520.0),(330.0,720.0),(600.0,720.0),(300.0,520.0),(330.0,560.0)] {
                    harness.deliver_to_first(SurfaceEvent::Resized(Size::new(DevicePx(width),DevicePx(height))));
                    harness.advance(Duration::from_millis(20));
                    harness.settle(64);
                    for node in [refs.slice,refs.bars[2],refs.numbers[0],refs.codes[0],refs.codes[1],refs.companion,refs.close] {
                        let bounds=node.window_bounds().unwrap();
                        assert!(bounds.origin.x.0>=0.0 && bounds.origin.x.0+bounds.size.width.0<=width+0.1,"horizontal overflow: {bounds:?} in {width}");
                        assert!(bounds.origin.y.0>=0.0 && bounds.origin.y.0+bounds.size.height.0<=height+0.1,"vertical overflow: {bounds:?} in {height}");
                    }
                    let slice=refs.slice.window_bounds().unwrap().size.height.0;
                    assert!((slice-model.tools.get_untracked().plot_height).abs()<0.1,"layout must honor the saved chart height");
                }
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

                check_numeric_adjustments(&mut harness, model, refs);

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
                assert!(!model.editing_text.get_untracked());
                refs.codes[0].focus();harness.settle(32);
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
                harness.advance(Duration::from_millis(20));
                harness.settle(64);
                let scaled = refs.bars[2].window_bounds().unwrap();
                assert!((scaled.size.height.0 - 2.0 * bounds.size.height.0).abs() < 0.1 && (scaled.size.width.0 - 2.0 * bounds.size.width.0).abs() < 0.1, "DPI bounds: {bounds:?} -> {scaled:?}");
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
                assert!((model.current().components()[2] / 360.0 - 0.8).abs() < 1e-5);
                refs.numbers[1].focus();
                harness.settle(32);
                let number = refs.numbers[1].window_bounds().unwrap();
                let before_wheel = model.current().components()[1];
                harness.deliver_to_first(SurfaceEvent::Wheel {
                    event: WheelEvent {
                        delta: ScrollDelta::Pixels(Size::new(CssPx(0.0), CssPx(-22.0))),
                        phase: ScrollPhase::Discrete,
                        position: Point::new(
                            CssPx((number.origin.x.0 + number.size.width.0 * 0.5) / 2.0),
                            CssPx((number.origin.y.0 + number.size.height.0 * 0.5) / 2.0),
                        ),
                        id: PointerId::MOUSE,
                        kind: PointerKind::Mouse,
                    },
                    modifiers: Modifiers::NONE,
                    timestamp: Timestamp::ORIGIN,
                });
                harness.settle(32);
                assert!((model.current().components()[1] - before_wheel - 0.001).abs() < 1e-12,
                    "wheel stepping must remain in CSS units at 2x DPI");
                refs.slice.focus();
                harness.settle(32);
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
                let projection = okcolor_core::GamutSlice::new(model.h.get_untracked(), model.max_chroma.get_untracked(), model.gamut.get_untracked()).unwrap().projection();
                let nearest = |x,y| projection.project(x,y,model.alpha.get_untracked(), f64::from(slice.size.width.0/slice.size.height.0)).unwrap().components()[0];
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
                assert!((last_valid.components()[1] - 0.05 * model.max_chroma.get_untracked()).abs() < 1e-6);
                harness.deliver_to_first(slice_event(PointerAction::Moved, 0.95, 0.05));
                harness.settle(32);
                assert!(model.dragging.get_untracked());
                assert!((model.current().components()[0] - nearest(0.95,0.05)).abs() < 1e-5);
                assert!(model.current().in_gamut(model.gamut.get_untracked()));
                harness.deliver_to_first(slice_event(PointerAction::Moved, 1.4, 0.6));
                harness.settle(32);
                assert!((model.current().components()[0] - nearest(1.4,0.6)).abs() < 1e-5);
                assert!(model.current().in_gamut(model.gamut.get_untracked()));
                harness.deliver_to_first(slice_event(PointerAction::Moved, 0.05, 0.5));
                harness.settle(32);
                assert_eq!(model.current(), last_valid, "captured drag can reenter the gamut");
                harness.deliver_to_first(slice_event(PointerAction::Released, 0.95, 0.05));
                harness.settle(32);
                assert!(!model.dragging.get_untracked());
                assert!((model.current().components()[0] - nearest(0.95,0.05)).abs() < 1e-5);
                assert!(model.current().in_gamut(model.gamut.get_untracked()));
                model.undo();
                harness.settle(32);
                assert_eq!(model.current(), before_slice);
                // All 24 recent colors remain reachable in the compact strip.
                for index in 0..24 {
                    model.select(okcolor_core::Oklch::new(0.6,0.08,f64::from(index)*15.0,1.0).unwrap());
                }
                harness.settle(64);
                let recent=refs.recents.window_bounds().unwrap();
                let older=Point::new(CssPx((recent.origin.x.0+recent.size.width.0)/2.0+8.0),CssPx((recent.origin.y.0+recent.size.height.0*0.5)/2.0));
                for action in [PointerAction::Pressed,PointerAction::Released] {
                    harness.deliver_to_first(SurfaceEvent::Pointer {action,event:PointerEvent::mouse(older).with_button(PointerButton::Primary),modifiers:Modifiers::NONE,timestamp:Timestamp::ORIGIN});
                    harness.settle(32);
                }
                assert!(refs.recents.scroll_position().offset.x.0>0.0,"right chevron scrolls the strip at 2x DPI");
                let last=Point::new(CssPx((recent.origin.x.0+recent.size.width.0-4.0)/2.0),CssPx((recent.origin.y.0+recent.size.height.0*0.5)/2.0));
                harness.deliver_to_first(SurfaceEvent::Wheel {event:WheelEvent {
                    delta:ScrollDelta::Pixels(Size::new(CssPx(0.0),CssPx(1000.0))),phase:ScrollPhase::Discrete,
                    position:last,id:PointerId::MOUSE,kind:PointerKind::Mouse,
                },modifiers:Modifiers::NONE,timestamp:Timestamp::ORIGIN});
                harness.settle(32);
                assert!(refs.recents.scroll_position().offset.x.0>0.0);
                assert!(refs.recents.scroll_position().is_at_end_horizontally());
                {
                    let rendered=frames.lock().unwrap();
                    let frame=rendered.last().unwrap();
                    let top=recent.origin.y.0.round() as i32;
                    for y in top..top+recent.size.height.0 as i32 {
                        for x in frame.size().width-32..frame.size().width {
                            assert!(frame.rgba(x,y)[0]<16,"right gutter retained ink at {x}, {y}");
                        }
                    }
                }
                let delete=Point::new(CssPx(last.x.0-46.0),last.y);
                let old_len=model.recent.get_untracked().len();
                for action in [PointerAction::Pressed,PointerAction::Released] {
                    harness.deliver_to_first(SurfaceEvent::Pointer {action,event:PointerEvent::mouse(delete).with_button(PointerButton::Middle),modifiers:Modifiers::NONE,timestamp:Timestamp::ORIGIN});
                    harness.settle(32);
                }
                assert_eq!(model.recent.get_untracked().len(),old_len-1);
                assert!(refs.recents.scroll_position().is_at_end_horizontally());
                assert!(refs.recents.scroll_position().offset.x.0>0.0);
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
                // A recalled tile can move another swatch beneath the stationary pointer.
                // Let that one-shot hover timer expire before measuring idle frames.
                harness.advance(Duration::from_secs(2));
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
                    assert_eq!(model.current(), [expected.main, expected.sub][usize::from(expected.selected)]);
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
