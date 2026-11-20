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
// The headless surface has no window manager to deliver native resize events.
fn fit_page(h: &mut Harness<impl zgui::platform::AppHandler>, refs: PickerRefs) {
    let body = refs.page_content.window_bounds().unwrap();
    let height = (body.origin.y.0 + body.size.height.0 + 32.0).round();
    h.deliver_to_first(SurfaceEvent::Resized(zgui::geom::Size::new(
        zgui::geom::DevicePx(330.0),
        zgui::geom::DevicePx(height),
    )));
    h.settle(64);
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

fn check_slot_shadows(
    h: &mut Harness<impl zgui::platform::AppHandler>,
    model: PickerModel,
    refs: PickerRefs,
    frames: &Mutex<Vec<zgui_render_wgpu::Pixels>>,
) {
    tile_pointer(
        h,
        PointerAction::Moved,
        PointerButton::Primary,
        Point::new(CssPx(2.0), CssPx(2.0)),
    );
    // White on white isolates the shadow on either side of the overlap.
    let slots = model.slots.map(|color| color.get_untracked());
    let selected = model.selected.get_untracked();
    for slot in 0..2 {
        model.select_slot(slot);
        model.input("#ffffff");
    }
    for slot in 0..2 {
        model.select_slot(slot);
        h.settle(32);
        let main = refs.swatches[0].window_bounds().unwrap();
        let x = main.origin.x.0 as i32;
        let y = main.origin.y.0 as i32 + 32;
        let captured = frames.lock().unwrap();
        let frame = captured.last().unwrap();
        let ink = |dx| frame.rgba(x + dx, y)[0];
        let (edge, direction, inactive) = if slot == 0 { (40, 1, 23) } else { (23, -1, 40) };
        assert!(
            ink(edge) < ink(edge + direction) && ink(edge + direction) < ink(edge + 3 * direction),
            "selected slot {slot} shadow: {:?}",
            [ink(edge), ink(edge + direction), ink(edge + 3 * direction)]
        );
        assert_eq!(
            ink(inactive),
            ink(edge + 3 * direction),
            "shadow under the inactive slot"
        );
    }
    for (i, color) in slots.into_iter().enumerate() {
        model.select_slot(i as u8);
        model.select(color);
    }
    model.select_slot(selected);
}

fn check_palette_adjustment(
    h: &mut Harness<impl zgui::platform::AppHandler>,
    model: PickerModel,
    refs: PickerRefs,
    frames: &Mutex<Vec<zgui_render_wgpu::Pixels>>,
) {
    let button = refs.palette_adjust.window_bounds().unwrap();
    let generate = refs.tools[0].window_bounds().unwrap();
    assert_eq!(
        generate.origin.x.0 - button.origin.x.0 - button.size.width.0,
        5.0
    );
    assert_eq!(button.origin.y, generate.origin.y);
    let ink_bounds = |node: NodeRef| {
        let b = node.window_bounds().unwrap();
        let (x, y) = (b.origin.x.0.round() as i32, b.origin.y.0.round() as i32);
        let frames = frames.lock().unwrap();
        let frame = frames.last().unwrap();
        let mut peak = 0;
        let mut left = b.size.width.0 as i32;
        for dx in 0..b.size.width.0 as i32 {
            for dy in 0..b.size.height.0 as i32 {
                let color = frame.rgba(x + dx, y + dy)[0];
                peak = peak.max(color);
                if color > 32 {
                    left = left.min(dx);
                }
            }
        }
        (peak, left)
    };
    let before = model.palette.get_untracked();
    click(h, refs.palette_adjust);
    let menu = refs.adjustment_menu.window_bounds().unwrap();
    let body = refs.tool_page.window_bounds().unwrap();
    let strip = refs.tools[5].window_bounds().unwrap();
    assert_eq!(menu.origin.y, body.origin.y);
    assert_eq!(
        menu.origin.x.0 + menu.size.width.0,
        button.origin.x.0 + button.size.width.0 * 0.5 - 12.0
    );
    assert_eq!(menu.origin.y.0 + menu.size.height.0, strip.origin.y.0);
    let cancel = refs.adjustment_cancel.window_bounds().unwrap();
    assert_eq!(cancel.origin.y.0 + cancel.size.height.0, strip.origin.y.0);
    assert_eq!(refs.adjustment_rows[3].text_content(), "T0");
    let first = refs.adjustment_rows[0].window_bounds().unwrap();
    let x = first.origin.x.0.round() as i32;
    let y = first.origin.y.0.round() as i32;
    let width = first.size.width.0.round() as i32;
    let height = first.size.height.0.round() as i32;
    let ink = || {
        let frames = frames.lock().unwrap();
        let frame = frames.last().unwrap();
        let mut peaks = [0u8; 2];
        let mut text_edges = [0, width];
        for dx in 0..width {
            let side = usize::from(dx >= width - 30);
            for dy in 2..height - 2 {
                let gray = frame.rgba(x + dx, y + dy)[0];
                peaks[side] = peaks[side].max(gray);
                if gray > 32 {
                    if side == 0 {
                        text_edges[0] = text_edges[0].max(dx);
                    } else {
                        text_edges[1] = text_edges[1].min(dx);
                    }
                }
            }
        }
        (peaks, text_edges)
    };
    let (normal, edges) = ink();
    assert!(
        (45..=67).contains(&normal[0]) && (75..=118).contains(&normal[1]),
        "normal ink {normal:?}"
    );
    assert!(edges[1] - edges[0] >= 10);
    let hover = Point::new(CssPx(x as f32 + 9.0), CssPx(y as f32 + 10.0));
    tile_pointer(h, PointerAction::Moved, PointerButton::Primary, hover);
    let (hovered, _) = ink();
    assert_eq!(hovered[0], normal[0]);
    assert!(
        hovered[1] > normal[1] + 30,
        "hovered ink {hovered:?}, normal {normal:?}"
    );
    for node in [refs.adjustment_apply, refs.adjustment_cancel] {
        let (normal, left) = ink_bounds(node);
        assert!((6..=10).contains(&left) && (75..=118).contains(&normal));
        let b = node.window_bounds().unwrap();
        let over = Point::new(CssPx(b.origin.x.0 + 10.0), CssPx(b.origin.y.0 + 10.0));
        tile_pointer(h, PointerAction::Moved, PointerButton::Primary, over);
        let hovered = ink_bounds(node).0;
        assert!(
            hovered > normal + 30,
            "{} action: normal {normal}, hovered {hovered}",
            node.text_content()
        );
    }
    for row in refs.adjustment_rows {
        click(h, row);
        assert!(refs.adjustment_menu.get().is_some());
        assert_eq!(model.palette.get_untracked(), before);
    }
    let row = refs.adjustment_rows[2].window_bounds().unwrap();
    let start = Point::new(CssPx(row.origin.x.0 + 10.0), CssPx(row.origin.y.0 + 10.0));
    let end = Point::new(CssPx(start.x.0 + 40.0), start.y);
    let primary = PointerButton::Primary;
    tile_pointer(h, PointerAction::Pressed, primary, start);
    tile_pointer(h, PointerAction::Moved, primary, end);
    let after = model.palette.get_untracked();
    assert_ne!(after, before);
    for (&(source, locked), &entry) in before.iter().zip(&after) {
        assert_eq!(
            entry,
            (
                if locked {
                    source
                } else {
                    okcolor_core::adjust_palette_color(
                        source,
                        model.gamut.get_untracked(),
                        [0, 0, 10, 0],
                    )
                },
                locked
            )
        );
    }
    let below = Point::new(end.x, CssPx(menu.origin.y.0 + menu.size.height.0 + 12.0));
    tile_pointer(h, PointerAction::Moved, primary, below);
    tile_pointer(h, PointerAction::Released, primary, below);
    assert_eq!(model.palette.get_untracked(), after);
    assert!(refs.adjustment_menu.get().is_some());
    // The exposed swatches and the generation shortcut are inactive during preview.
    let original_color = model.current();
    click(h, refs.tools[6]);
    key(
        h,
        KeyEvent::named(NamedKey::Space, PhysicalKey::Unidentified(0)),
        Modifiers::NONE,
    );
    assert_eq!(model.current(), original_color);
    assert!(refs.adjustment_menu.get().is_some());
    assert_eq!(model.palette.get_untracked(), after);
    assert_eq!(
        refs.adjustment_menu.window_bounds().unwrap().size.width,
        menu.size.width
    );
    let visible_value = refs.adjustment_rows[2].text_content();
    assert!(visible_value.contains("+10"), "hue offset: {visible_value}");
    let light = refs.adjustment_rows[0].window_bounds().unwrap();
    let light_start = Point::new(start.x, CssPx(light.origin.y.0 + 9.0));
    let light_end = Point::new(end.x, light_start.y);
    tile_pointer(h, PointerAction::Pressed, primary, light_start);
    tile_pointer(h, PointerAction::Moved, primary, light_end);
    tile_pointer(h, PointerAction::Released, primary, light_end);
    let combined = model.palette.get_untracked();
    assert_ne!(combined, after);
    let after = combined;
    click(h, refs.adjustment_apply);
    assert!(refs.adjustment_menu.get().is_none());
    assert_eq!(model.palette.get_untracked(), after);
    model.undo();
    h.settle(32);
    assert_eq!(model.palette.get_untracked(), before);
    model.redo();
    h.settle(32);
    assert_eq!(model.palette.get_untracked(), after);

    // Cancel and Escape abandon every drag since opening, including a held one.
    for escape in [false, true] {
        click(h, refs.palette_adjust);
        tile_pointer(h, PointerAction::Pressed, primary, start);
        tile_pointer(h, PointerAction::Moved, primary, end);
        tile_pointer(h, PointerAction::Released, primary, end);
        assert_ne!(model.palette.get_untracked(), after);
        if escape {
            tile_pointer(h, PointerAction::Pressed, primary, start);
            tile_pointer(h, PointerAction::Moved, primary, end);
            key(
                h,
                KeyEvent::named(NamedKey::Escape, PhysicalKey::Unidentified(0)),
                Modifiers::NONE,
            );
            tile_pointer(h, PointerAction::Released, primary, end);
        } else {
            click(h, refs.adjustment_cancel);
        }
        assert!(refs.adjustment_menu.get().is_none());
        assert_eq!(model.palette.get_untracked(), after);
    }
    click(h, refs.palette_adjust);
    for (row, limit) in refs.adjustment_rows.into_iter().zip([100, 100, 360, 100]) {
        let rect = row.window_bounds().unwrap();
        let p = Point::new(start.x, CssPx(rect.origin.y.0 + 8.0));
        for sign in [1, -1] {
            let far = Point::new(CssPx(p.x.0 + 10000.0 * sign as f32), p.y);
            tile_pointer(h, PointerAction::Pressed, primary, p);
            tile_pointer(h, PointerAction::Moved, primary, far);
            tile_pointer(h, PointerAction::Released, primary, far);
            assert!(row.text_content().ends_with(&format!("{:+}", limit * sign)));
            assert_eq!(
                refs.adjustment_menu.window_bounds().unwrap().size.width,
                menu.size.width
            );
        }
    }
    click(h, refs.adjustment_cancel);
    assert_eq!(model.palette.get_untracked(), after);
    click(h, refs.palette_adjust);
    let current = model.current();
    let start = Point::new(
        CssPx(strip.origin.x.0 + strip.size.width.0 * 0.5),
        CssPx(strip.origin.y.0 + 10.0),
    );
    for action in [
        PointerAction::Moved,
        PointerAction::Pressed,
        PointerAction::Released,
    ] {
        tile_pointer(h, action, PointerButton::Middle, start);
    }
    assert!(refs.adjustment_menu.get().is_none());
    assert_eq!(model.current(), current);
    assert_eq!(model.palette.get_untracked(), after);
    model.undo();
    h.settle(32);
    assert_eq!(model.palette.get_untracked(), before);
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
                let badge=refs.tabs[2].window_bounds().unwrap();
                {
                    let captured=frames.lock().unwrap();
                    let img=captured.last().unwrap();
                    let (x,y)=(badge.origin.x.0.round() as i32,badge.origin.y.0.round() as i32);
                    for dy in 0..24 {for dx in 0..24 {
                        let radius=(dx as f32-11.5).hypot(dy as f32-11.5);
                        if (7.25..8.4).contains(&radius) {
                            assert!(img.rgba(x+dx,y+dy)[0].abs_diff(img.rgba(x+23-dx,y+23-dy)[0])<=6,
                                "APCA inner arc must stay inside the circular contour at {dx}/{dy}");
                        }
                    }}
                }
                let eye=refs.eyedropper.window_bounds().unwrap();
                let eye_center=Point::new(CssPx(eye.origin.x.0+8.0),CssPx(eye.origin.y.0+8.0));
                tile_pointer(&mut h,PointerAction::Moved,PointerButton::Primary,eye_center);
                h.run_for(Duration::from_secs(2),Duration::from_millis(20));h.settle(32);
                assert!(refs.tooltip.window_bounds().is_some(),"eyedropper hover must mount a tooltip");
                assert!(refs.tooltip.text_content().contains("Eyedropper"),"tooltip: {}",refs.tooltip.text_content());
                for (i,shortcut) in ["l","c","h","a"].into_iter().enumerate() {
                    key(&mut h,KeyEvent::character(shortcut),Modifiers::NONE);
                    assert!(model.editing_text.get_untracked());
                    key(&mut h,KeyEvent::character("7"),Modifiers::NONE);
                    assert_eq!(refs.numbers[i].text_content(),"7");
                    key(&mut h,KeyEvent::named(NamedKey::Escape,PhysicalKey::Unidentified(0)),Modifiers::NONE);
                    assert_eq!(model.current(),original);
                }
                // A fresh window routes a printable key to the document root.
                key(&mut h,KeyEvent::character("r"),Modifiers::NONE);
                assert!(model.editing_text.get_untracked(),"unfocused window shortcut");
                key(&mut h,KeyEvent::character("oklch(0.678901234567890123456789012345 0.123456789012345 230.23456789012345)"),Modifiers::NONE);
                h.settle(32);
                assert!(refs.codes[1].scroll_position().offset.x.0>0.0,"the end of a long draft must be visible: {:?}",refs.codes[1].scroll_position());
                key(&mut h,KeyEvent::named(NamedKey::Home,PhysicalKey::Unidentified(0)),Modifiers::NONE);
                assert!(refs.codes[1].scroll_position().offset.x.0<8.0,"Home reveals the beginning");
                key(&mut h,KeyEvent::named(NamedKey::Escape,PhysicalKey::Unidentified(0)),Modifiers::NONE);

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
                check_slot_shadows(&mut h,model,refs,&frames);
                for slot in 0..2 {
                    model.select_slot(slot);
                    let old = model.current();
                    let near = okcolor_core::Oklch::new(0.65,0.4,140.0,1.0).unwrap();
                    model.select(near);
                    h.settle(32);
                    let slot_bounds = refs.swatches[usize::from(slot)].window_bounds().unwrap();
                    let single = refs.fallback_samples[0].window_bounds().unwrap();
                    assert!(refs.fallback_samples[1].window_bounds().is_none());
                    assert!((slot_bounds.origin.y.0 + 20.0 - single.origin.y.0 - 10.0).abs() < 1.0);
                    let wide = okcolor_core::Oklch::new(0.8,0.7,275.0,1.0).unwrap();
                    model.select(wide);
                    h.settle(32);
                    // Verify both hover targets report the exact color the action will select.
                    for (index, (name, method)) in [
                        ("Mapped sRGB", okcolor_core::GamutMapping::LocalMinde),
                        ("Clipped sRGB", okcolor_core::GamutMapping::Clip),
                    ].into_iter().enumerate() {
                        let b = refs.fallback_samples[index].window_bounds().unwrap();
                        let hex = okcolor_core::format_color(wide, okcolor_core::ColorFormat::Hex, method).unwrap().text;
                        for x in [b.origin.x.0 + 10.0, b.origin.x.0 - 15.0] {
                            let point = Point::new(CssPx(x), CssPx(b.origin.y.0 + 10.0));
                            tile_pointer(&mut h, PointerAction::Moved, PointerButton::Primary, point);
                            h.run_for(Duration::from_secs(2), Duration::from_millis(20));
                            h.settle(32);
                            // The label and monospace HEX are separate nodes with a CSS gap.
                            assert_eq!(refs.tooltip.text_content(), format!("{name}{hex}"));
                        }
                    }
                    let b = refs.swatches[usize::from(slot)].window_bounds().unwrap();
                    {
                        let captured = frames.lock().unwrap();
                        let image = captured.last().unwrap();
                        let (x,y) = (b.origin.x.0 as i32, b.origin.y.0 as i32);
                        for (dx,dy) in [(10,0),(0,10),(39,10),(10,39)] {
                            assert_eq!(image.rgba(x+dx, y+dy), [66,66,66,255], "outer slot frame");
                        }
                        let fill = image.rgba(x+10, y+2)[1];
                        assert_eq!(fill, 0, "unavailable fill");
                        for dx in [10,29] {
                            let diagonal = image.rgba(x+dx,y+10)[1];
                            assert!(diagonal.abs_diff(66) < fill.abs_diff(66)/2, "fixed-ink cross");
                        }
                    }
                    if slot==0 {
                        let center=Point::new(CssPx(b.origin.x.0+20.0),CssPx(b.origin.y.0+20.0));
                        for action in [PointerAction::Moved,PointerAction::Pressed,PointerAction::Released] {
                            tile_pointer(&mut h,action,PointerButton::Secondary,center);
                        }
                        h.run_for(Duration::from_millis(300),Duration::from_millis(16)); h.settle(32);
                        let captured=frames.lock().unwrap();
                        let image=captured.last().unwrap();
                        let (x,y)=(center.x.0 as i32,center.y.0 as i32);
                        assert!((-8..8).any(|dx|(-8..8).any(|dy|image.rgba(x+dx,y+dy)[0]>170)),
                            "white copy confirmation on the black unavailable swatch");
                        drop(captured);
                        click(&mut h,refs.fallback_samples[0]);
                    }
                    else {
                        let b=refs.fallback_samples[1].window_bounds().unwrap();
                        let icon=Point::new(CssPx(b.origin.x.0-15.0),CssPx(b.origin.y.0+10.0));
                        tile_pointer(&mut h,PointerAction::Moved,PointerButton::Primary,icon);
                        {
                            let captured=frames.lock().unwrap();let image=captured.last().unwrap();
                            let brightest=(-9..10).flat_map(|dy|(-9..10).map(move |dx| image.rgba(icon.x.0 as i32+dx, icon.y.0 as i32+dy)[0])).max().unwrap();
                            assert!(brightest>155,"hovered fallback glyph intensity {brightest}");
                        }
                        for action in [PointerAction::Pressed,PointerAction::Released] {tile_pointer(&mut h,action,PointerButton::Primary,icon);}
                    }
                    assert!(model.current().in_gamut(okcolor_core::Gamut::Srgb));
                    model.undo();
                    assert_eq!(model.current(), wide);
                    model.select(old);
                }
                model.select_slot(0);
                h.settle(32);
                assert_eq!(model.current(), original);
                click(&mut h, refs.tabs[0]);
                assert_eq!(model.tab.get_untracked(), Tab::Picker);
                click(&mut h, refs.tabs[1]);
                assert_eq!(model.tab.get_untracked(), Tab::Harmonies);
                assert_eq!(refs.similar.focusables().len(), 6, "six mounted named-color tiles");
                fit_page(&mut h,refs);
                let original_format=model.format.get_untracked();
                click(&mut h,refs.format_trigger);
                let panel=refs.format_menu.window_bounds().expect("output format band");
                let page=refs.tool_page.window_bounds().unwrap();
                let trigger=refs.format_trigger.window_bounds().unwrap();
                assert_eq!(panel.origin.y,page.origin.y);
                assert_eq!(panel.size.height,page.size.height);
                assert_eq!(panel.origin.x.0+panel.size.width.0,trigger.origin.x.0+trigger.size.width.0/2.0-12.0);
                let rows=11.0*22.0;
                let first=(trigger.origin.y.0+trigger.size.height.0/2.0-rows/2.0)
                    .clamp(page.origin.y.0,page.origin.y.0+page.size.height.0-rows);
                let rgb=Point::new(CssPx(panel.origin.x.0+15.0),CssPx(first+55.0));
                for action in [PointerAction::Moved,PointerAction::Pressed,PointerAction::Released] {
                    tile_pointer(&mut h,action,PointerButton::Primary,rgb);
                }
                assert_eq!(model.format.get_untracked(),okcolor_core::ColorFormat::Rgb);
                assert!(refs.format_menu.get_untracked().is_none());
                model.format.set(original_format);
                // Black in the variations uses the same inner edge as a dark swatch.
                let shades = refs.tools[7].window_bounds().unwrap();
                {
                    let captured = frames.lock().unwrap();
                    let image = captured.last().unwrap();
                    let (x,y) = (shades.origin.x.0 as i32 + 5, shades.origin.y.0 as i32);
                    assert_eq!(image.rgba(x,y), [40,40,40,255], "dark variation edge");
                    assert!(image.rgba(x,y+2)[0] < 9, "dark variation fill");
                }
                for folded in [false, true] {
                    if folded { click(&mut h,refs.group_toggles[0]); }
                    let toggle=refs.group_toggles[0].window_bounds().unwrap();
                    let x=toggle.origin.x.0.round() as i32;
                    let y=(toggle.origin.y.0+(toggle.size.height.0-10.0)/2.0).round() as i32;
                    let frames=frames.lock().unwrap();
                    let image=frames.last().unwrap();
                    for dy in 0..10 {
                        for dx in 0..10 {
                            let ink=if folded {(4..6).contains(&dx) && (4..6).contains(&dy)} else {(1..9).contains(&dx) && dy==4};
                            assert_eq!(image.rgba(x+dx,y+dy)[0]>45,ink,"folded {folded} at {dx}/{dy}, {toggle:?}");
                        }
                    }
                }
                click(&mut h,refs.group_toggles[0]);
                fit_page(&mut h,refs);
                let strip = refs.tools[5].window_bounds().unwrap();
                let page_height = refs.page_content.window_bounds().unwrap().size.height.0;
                click(&mut h,refs.library[1]);
                let folder = refs.folder_menu.window_bounds().expect("mounted swatch menu");
                let anchor = refs.library[1].window_bounds().unwrap();
                assert!(anchor.size.width.0==strip.size.width.0,"full-width folder trigger {anchor:?}");
                let body=refs.tool_page.window_bounds().unwrap();
                assert_eq!(folder.origin.y,body.origin.y);
                assert_eq!(folder.size.height,body.size.height);
                assert_eq!(folder.origin.x.0+folder.size.width.0,anchor.origin.x.0+anchor.size.width.0-8.0-12.0);
                assert_eq!(folder.size.width.0,100.0);
                let mx=folder.origin.x.0.round() as i32;
                let my=refs.library[4].window_bounds().unwrap().origin.y.0.round() as i32-22;
                assert!((my as f32+22.0-anchor.origin.y.0-anchor.size.height.0/2.0).abs()<=1.0);
                { let frames=frames.lock().unwrap(); let image=frames.last().unwrap();
                  for dy in [0,1,35,folder.size.height.0 as i32-1] {
                      let y=folder.origin.y.0 as i32+dy;
                      assert_eq!(image.rgba(mx,y)[0],224,"left border");
                      assert_eq!(image.rgba(mx+99,y)[0],224,"right border");
                      assert_eq!(image.rgba(mx+50,y)[0],0,"black panel, no horizontal border");
                  }
                }
                tile_pointer(&mut h,PointerAction::Moved,PointerButton::Primary,Point::new(CssPx(mx as f32+8.0),CssPx(my as f32+37.0)));
                {
                    let captured=frames.lock().unwrap();
                    let image=captured.last().unwrap();
                    if let Ok(dir)=std::env::var("OKCOLOR_TOOLS_CAPTURE_DIR"){
                        std::fs::write(std::path::Path::new(&dir).join("menu.rgba"),image.bytes()).unwrap();
                    }
                    for row in [0,22] {
                        let (mut left,mut right,mut top,mut bottom)=(100,0,22,0);
                        for y in 0..22 {
                            assert_eq!(image.rgba(mx+1,my+row+y),[0,0,0,255],"selected and hovered rows share the panel background");
                            for x in 2..98 {
                                if image.rgba(mx+x,my+row+y)[0]>60 {
                                    left=left.min(x);right=right.max(x);top=top.min(y);bottom=bottom.max(y);
                                }
                            }
                        }
                        assert!(left<=right && (6..=10).contains(&left) && (top+bottom-21_i32).abs()<=4,
                            "menu row {row} ink bounds {left}..{right}, {top}..{bottom}");
                    }
                }
                assert_eq!(refs.page_content.window_bounds().unwrap().size.height.0,page_height,"an open menu must not extend the page");
                key(&mut h,KeyEvent::named(NamedKey::Escape,PhysicalKey::Unidentified(0)),Modifiers::NONE);
                assert!(refs.folder_menu.get().is_none());
                let short_width=folder.size.width.0;
                let folder_id=model.library.try_update(|lib|lib.create_folder(&"W".repeat(64)).unwrap()).unwrap();
                click(&mut h,refs.library[1]);
                let wide_menu=refs.folder_menu.window_bounds().unwrap();
                assert!(wide_menu.size.width.0>short_width+20.0,"folder menu must fit its longest item: {short_width} -> {wide_menu:?}");
                key(&mut h,KeyEvent::named(NamedKey::Escape,PhysicalKey::Unidentified(0)),Modifiers::NONE);
                assert_eq!(wide_menu.origin.x.0,body.origin.x.0+20.0);
                assert_eq!(wide_menu.origin.x.0+wide_menu.size.width.0,folder.origin.x.0+folder.size.width.0);
                model.library.update(|lib|lib.remove(folder_id));
                let original_library=model.library.get_untracked();
                model.library.update(|lib|{
                    for i in 0..32 {lib.create_folder(&format!("Folder {i}")).unwrap();}
                });
                click(&mut h,refs.library[1]);
                let band=refs.folder_menu;
                assert_eq!(band.window_bounds().unwrap().size.height,body.size.height);
                assert!(band.scroll_position().content_size.height.0>body.size.height.0);
                band.scroll_to(ScrollTarget::Offset(Point::new(zgui::geom::DevicePx(0.0),zgui::geom::DevicePx(10000.0))),ScrollBehavior::Instant);
                h.settle(32);
                let last=refs.library[4].window_bounds().unwrap();
                assert!((last.origin.y.0+last.size.height.0-body.origin.y.0-body.size.height.0).abs()<=1.0);
                key(&mut h,KeyEvent::named(NamedKey::Escape,PhysicalKey::Unidentified(0)),Modifiers::NONE);
                model.library.set(original_library);
                h.settle(32);
                let palette_start = Point::new(
                    CssPx(strip.origin.x.0 + 8.0),
                    CssPx(strip.origin.y.0 + 8.0),
                );
                let tail = refs.tools[2].window_bounds().unwrap();
                let palette_end = Point::new(CssPx(strip.origin.x.0 + strip.size.width.0 - 2.0), CssPx(strip.origin.y.0 + 2.0));
                let last_color = Point::new(CssPx(tail.origin.x.0 - 22.0), palette_start.y);
                let primary = PointerButton::Primary;
                tile_pointer(&mut h,PointerAction::Moved,primary,palette_start);
                tile_pointer(&mut h, PointerAction::Pressed, primary, palette_start);
                tile_pointer(&mut h, PointerAction::Released, primary, palette_start);
                let locked = model.current();
                assert_ne!(locked, original);
                assert!(refs.palette_dot.window_bounds().is_none(),"hover alone must not show a lock");
                for action in [PointerAction::Pressed,PointerAction::Released]{
                    h.deliver_to_first(SurfaceEvent::Pointer{action,event:PointerEvent::mouse(palette_start).with_button(primary),modifiers:Modifiers::SHIFT,timestamp:Timestamp::ORIGIN});
                    h.settle(32);
                }
                let dot=refs.palette_dot.window_bounds().expect("locked tile indicator");
                let cell=refs.tools[6].window_bounds().unwrap();
                assert!((dot.origin.x.0+2.5-cell.origin.x.0-cell.size.width.0*0.5).abs()<=1.0);
                assert!((cell.origin.y.0+cell.size.height.0-dot.origin.y.0-dot.size.height.0-5.0).abs()<=1.0);
                check_palette_adjustment(&mut h, model, refs, &frames);
                let before_generation=model.palette.get_untracked();
                click(&mut h, refs.tools[0]);
                let generated=model.palette.get_untracked();
                assert_ne!(generated,before_generation);
                model.undo();h.settle(32);
                assert_eq!(model.palette.get_untracked(),before_generation);
                model.redo();h.settle(32);
                assert_eq!(model.palette.get_untracked(),generated);
                for action in [PointerAction::Pressed,PointerAction::Released] {tile_pointer(&mut h,action,primary,last_color);}
                let before_space = model.current();
                key(&mut h,KeyEvent::named(NamedKey::Space,PhysicalKey::Unidentified(0)),Modifiers::NONE);
                for action in [PointerAction::Pressed,PointerAction::Released] {tile_pointer(&mut h,action,primary,last_color);}
                assert_ne!(model.current(),before_space,"Space changes an unlocked color");
                model.select(original);
                tile_pointer(&mut h, PointerAction::Pressed, primary, palette_start);
                tile_pointer(&mut h, PointerAction::Released, primary, palette_start);
                assert_eq!(
                    model.current(),
                    locked,
                    "generation preserves the locked color"
                );
                tile_pointer(&mut h, PointerAction::Pressed, primary, palette_start);
                let before_reorder=model.palette.get_untracked();
                let boundary=cell.origin.x.0+cell.size.width.0*2.0;
                let over_boundary=Point::new(CssPx(boundary+2.0),palette_start.y);
                for (light,ink) in [(1.0,0u8),(0.0,255u8)] {
                    let fill=okcolor_core::Oklch::new(light,0.0,0.0,1.0).unwrap();
                    model.palette.update(|colors| {colors[1].0=fill;colors[2].0=fill;});h.settle(32);
                    tile_pointer(&mut h,PointerAction::Moved,primary,over_boundary);
                    let x=boundary.round() as i32;
                    let y=(strip.origin.y.0+19.0) as i32;
                    let shown={
                        let capture=frames.lock().unwrap();
                        let frame=capture.last().unwrap();
                        (x-2..=x+1).map(|px|frame.rgba(px,y)[0]).collect::<Vec<_>>()
                    };
                    assert!(shown.contains(&ink),"drop line between tiles of lightness {light}: {shown:?}");
                }
                model.palette.set(before_reorder.clone());h.settle(32);
                tile_pointer(&mut h, PointerAction::Moved, primary, palette_end);
                {
                    let capture=frames.lock().unwrap();
                    let frame=capture.last().unwrap();
                    let x=(tail.origin.x.0+tail.size.width.0/2.0) as i32;
                    let y=(tail.origin.y.0+tail.size.height.0/2.0) as i32;
                    assert_eq!(frame.rgba(x,y),frame.rgba(x+3,y+3),"dragging hides the plus icon");
                }
                tile_pointer(&mut h, PointerAction::Released, primary, palette_end);
                let reordered=model.palette.get_untracked();
                model.undo();h.settle(32);
                assert_eq!(model.palette.get_untracked(),before_reorder);
                model.redo();h.settle(32);
                assert_eq!(model.palette.get_untracked(),reordered);
                for action in [PointerAction::Pressed,PointerAction::Released] {
                    tile_pointer(&mut h,action,PointerButton::Middle,last_color);
                }
                assert_eq!(model.palette.get_untracked().len(),reordered.len()-1);
                model.undo();h.settle(32);
                assert_eq!(model.palette.get_untracked(),reordered,"restore deleted color and its lock");
                model.select(original);
                tile_pointer(&mut h, PointerAction::Pressed, primary, last_color);
                tile_pointer(&mut h, PointerAction::Released, primary, last_color);
                assert_eq!(
                    model.current(),
                    locked,
                    "drop on the trailing target appends"
                );
                let plus = refs.tools[2];
                let center = Point::new(CssPx(tail.origin.x.0 + 10.5), CssPx(tail.origin.y.0 + 10.5));
                tile_pointer(&mut h,PointerAction::Moved,primary,center);
                tile_pointer(&mut h,PointerAction::Pressed,primary,center);
                h.run_for(Duration::from_secs(1),Duration::from_millis(25));
                assert!(plus.text_content().is_empty(), "a stationary press keeps the plus glyph");
                for (up, expected) in [(120.0, "5"), (36.0, "3"), (12.0, "1"), (0.0, "")] {
                    let point = Point::new(center.x, CssPx(center.y.0 - up));
                    tile_pointer(&mut h,PointerAction::Moved,primary,point);
                    assert_eq!(plus.text_content(), expected);
                    if up == 36.0 {
                        let captured = frames.lock().unwrap();
                        let image = captured.last().unwrap();
                        let mut ink = (21,21,-1,-1);
                        for dy in 1..20 {
                            for dx in 1..20 {
                                if image.rgba(tail.origin.x.0.round() as i32+dx,tail.origin.y.0.round() as i32+dy)[0] > 100 {
                                    ink = (ink.0.min(dx),ink.1.min(dy),ink.2.max(dx),ink.3.max(dy));
                                }
                            }
                        }
                        assert!(ink.0 <= ink.2 && ink.1 <= ink.3, "count must paint visible ink");
                        assert!((ink.0+ink.2-20i32).abs()<=3 && (ink.1+ink.3-20i32).abs()<=3,
                            "count must be centered in the 21px button: {ink:?}");
                    }
                }
                tile_pointer(&mut h,PointerAction::Released,primary,center);
                let between=Point::new(CssPx(strip.origin.x.0+strip.size.width.0/5.0+2.0),CssPx(strip.origin.y.0+20.0));
                tile_pointer(&mut h,PointerAction::Moved,primary,between);
                tile_pointer(&mut h,PointerAction::Pressed,primary,between);
                h.run_for(Duration::from_millis(525),Duration::from_millis(25));
                let raised = |pixels:f32| Point::new(between.x,CssPx(between.y.0-pixels));
                tile_pointer(&mut h,PointerAction::Moved,primary,raised(36.0));
                tile_pointer(&mut h,PointerAction::Moved,primary,between);
                tile_pointer(&mut h,PointerAction::Released,primary,between);
                assert!((refs.tools[6].window_bounds().unwrap().size.width.0-strip.size.width.0/5.0).abs()<2.0,
                    "dragging back to zero cancels insertion");
                tile_pointer(&mut h,PointerAction::Moved,primary,between);
                tile_pointer(&mut h,PointerAction::Pressed,primary,between);
                tile_pointer(&mut h,PointerAction::Moved,primary,raised(48.0));
                tile_pointer(&mut h,PointerAction::Moved,primary,raised(24.0));
                tile_pointer(&mut h,PointerAction::Released,primary,raised(24.0));
                let inserted=model.palette.get_untracked();
                model.undo();h.settle(32);
                assert_eq!(model.palette.get_untracked().len(),5);
                model.redo();h.settle(32);
                assert_eq!(model.palette.get_untracked(),inserted);
                let lock=refs.tools[6].window_bounds().unwrap();
                let cell_width=lock.size.width.0;
                assert!((cell_width-strip.size.width.0/7.0).abs()<2.0,"releasing at two upward steps inserts two colors, got cell width {cell_width}");
                tile_pointer(&mut h,PointerAction::Moved,primary,center);
                click(&mut h, plus);
                assert!((refs.tools[6].window_bounds().unwrap().size.width.0-strip.size.width.0/8.0).abs()<2.0,
                    "an ordinary plus click inserts one color");
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
                model.undo();h.settle(32);
                assert_eq!(model.library.get_untracked().swatches[0].id, first_id);
                model.redo();h.settle(32);
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
                model.undo();h.settle(32);
                assert_eq!(model.library.get_untracked().swatches.len(),2);
                model.redo();h.settle(32);
                assert_eq!(model.library.get_untracked().swatches.len(),1);
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
                    "Enter formats the committed value"
                );
                assert!(!model.editing_text.get_untracked());
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
                let enter = || KeyEvent::named(NamedKey::Enter,PhysicalKey::Unidentified(0));
                for (input,lc) in [("150",108),("0",0),("65",65)] {
                    assert!(!model.editing_text.get_untracked());
                    type_text(&mut h,refs.tools[3],input);
                    key(&mut h,enter(),Modifiers::NONE);
                    assert_eq!(model.tools.get_untracked().contrast_grid.levels[0],lc);
                }
                for (node,dark,values) in [(refs.tools[4],true,["#123456","#000000"]),(refs.tools[7],false,["#fedcba","#ffffff"])] {
                    for value in values {
                        type_text(&mut h,node,value);
                        key(&mut h,enter(),Modifiers::NONE);
                        let tools=model.tools.get_untracked();
                        let components=if dark {tools.contrast_grid.dark_reference} else {tools.background};
                        assert_eq!(components,okcolor_core::parse_color(value,0.0).unwrap().components());
                        assert_eq!(node.text_content().to_ascii_lowercase(),value);
                    }
                }
                // Changing the reference immediately recalculates both the visible target and its editable limit.
                type_text(&mut h,refs.tools[3],"200");
                key(&mut h,enter(),Modifiers::NONE);
                type_text(&mut h,refs.tools[4],"#7e7799");
                key(&mut h,enter(),Modifiers::NONE);
                let mid=okcolor_core::parse_color("#7e7799",0.0).unwrap();
                let white=okcolor_core::parse_color("#fff",0.0).unwrap();
                let expected=okcolor_core::apca(white,mid,okcolor_core::GamutMapping::Clip).unwrap().abs().round() as u8;
                assert_eq!(refs.tools[3].text_content(),expected.to_string());
                type_text(&mut h,refs.tools[3],"200");
                key(&mut h,enter(),Modifiers::NONE);
                assert_eq!(model.tools.get_untracked().contrast_grid.levels[0],expected);
                if let Ok(dir)=std::env::var("OKCOLOR_TOOLS_CAPTURE_DIR"){
                    std::fs::write(std::path::Path::new(&dir).join("apca-colored.rgba"),frames.lock().unwrap().last().unwrap().bytes()).unwrap();
                }
                for signed in ["+20","-20"] {
                    type_text(&mut h,refs.tools[3],signed);
                    key(&mut h,enter(),Modifiers::NONE);
                    let config=model.tools.get_untracked().contrast_grid;
                    assert_eq!(config.signed_target(0,mid),signed.parse::<f64>().unwrap());
                    assert_eq!(refs.tools[3].text_content(),"20");
                    type_text(&mut h,refs.current_lc[0],signed);
                    key(&mut h,enter(),Modifiers::NONE);
                    let actual=okcolor_core::apca(model.current(),mid,model.mapping.get_untracked()).unwrap();
                    assert_eq!(actual.round(),signed.parse::<f64>().unwrap());
                }
                model.select(original);
                type_text(&mut h,refs.tools[4],"#000000");
                key(&mut h,enter(),Modifiers::NONE);
                type_text(&mut h,refs.tools[3],"65");
                key(&mut h,enter(),Modifiers::NONE);
                let source=model.current();
                type_text(&mut h,refs.current_lc[0],"32");
                key(&mut h,enter(),Modifiers::NONE);
                assert_eq!(refs.current_lc[0].text_content(),"32");
                assert_ne!(model.current(),source);
                model.undo();h.settle(32);assert_eq!(model.current(),source);
                let grid = refs.tools[5].window_bounds().unwrap();
                let point=|dx,dy|Point::new(CssPx(grid.origin.x.0+dx),CssPx(grid.origin.y.0+dy));
                for action in [PointerAction::Moved,PointerAction::Pressed,PointerAction::Released] {
                    tile_pointer(&mut h,action,primary,point(166.0,62.0));
                }
                assert_eq!(model.tools.get_untracked().contrast_grid.levels,vec![65,58,50,50,70]);
                let row = point(148.0,63.0);
                tile_pointer(&mut h,PointerAction::Moved,primary,row);
                for action in [PointerAction::Pressed,PointerAction::Released] {
                    tile_pointer(&mut h,action,PointerButton::Middle,row);
                }
                assert_eq!(model.tools.get_untracked().contrast_grid.levels,vec![65,50,50,70]);
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
                model.undo(); assert_eq!(model.tools.get_untracked().contrast_grid.split,2);
                model.redo(); h.settle(32); assert_eq!(model.tools.get_untracked().contrast_grid.split,0);
                for upper in [true,false] {
                    if !upper {
                        let bottom=Point::new(divider.x,CssPx(grid.origin.y.0+grid.size.height.0-1.0));
                        for (action,at) in [(PointerAction::Pressed,divider),(PointerAction::Moved,bottom),(PointerAction::Released,bottom)] {
                            tile_pointer(&mut h,action,primary,at);
                        }
                        assert_eq!(model.tools.get_untracked().contrast_grid.split,4);
                    }
                    let plus=refs.tools[6].window_bounds().unwrap();
                    let reference=refs.tools[if upper {7} else {4}].window_bounds().unwrap();
                    assert_eq!((plus.size.width.0,plus.size.height.0),(21.0,21.0));
                    assert_eq!(plus.origin.y.0<reference.origin.y.0,upper);
                    let (x,y)=(plus.origin.x.0.round() as i32,plus.origin.y.0.round() as i32);
                    {
                        let frames=frames.lock().unwrap();
                        let image=frames.last().unwrap();
                        let background=image.rgba(x+5,y+5);
                        for dy in 5..16 {
                            for dx in 5..16 {
                                let ink=(dx==10 && (6..15).contains(&dy)) || (dy==10 && (6..15).contains(&dx));
                                assert_eq!(image.rgba(x+dx,y+dy)!=background,ink,"restore cross at {dx}/{dy}");
                            }
                        }
                    }
                    click(&mut h, refs.tools[6]);
                    assert_eq!(model.tools.get_untracked().contrast_grid.split, 2);
                }
                type_text(&mut h,refs.tools[7],"#ffccee");
                key(&mut h,enter(),Modifiers::NONE);
                let previous=model.tools.get_untracked();
                let reset=refs.apca_reset.window_bounds().unwrap();
                {
                    let frames=frames.lock().unwrap();
                    let image=frames.last().unwrap();
                    let left=reset.origin.x.0.round() as i32;
                    let top=reset.origin.y.0.round() as i32;
                    assert!(image.rgba(left+13,top+2)[0]>=60 && image.rgba(left+12,top+2)[0]<15 && image.rgba(left+14,top+2)[0]<15,"reset arrowhead occupies one pixel");
                    let x=(reset.origin.x.0+reset.size.width.0+5.0).round() as i32;
                    let y=reset.origin.y.0.round() as i32;
                    let bg=image.rgba(x+5,y+9);
                    assert_eq!(image.rgba(x+7,y+9),bg);
                    assert!(image.rgba(x+6,y+9)[0]>bg[0]+40,"compact copy stroke aligned after 5px gap");
                }
                click(&mut h,refs.apca_reset);
                let defaults=okcolor_app::workspace::ToolSettings::default();
                let settings=model.tools.get_untracked();
                assert_eq!(settings.contrast_grid,defaults.contrast_grid);
                assert_eq!(settings.background,defaults.background);
                model.undo(); h.settle(32);
                assert_eq!(model.tools.get_untracked(),previous);
                let saved = model.saved_state_untracked();
                assert_eq!(SavedState::decode(&saved.encode().unwrap()).unwrap(), saved);
                if let Ok(directory) = std::env::var("OKCOLOR_TOOLS_CAPTURE_DIR") {
                    model.tools.update(|t|t.contrast_grid=Default::default());
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
                    for (index, name, height) in [(0, "swatches",582.0), (1, "harmonies",720.0), (2, "apca",606.0)] {
                        h.deliver_to_first(SurfaceEvent::Resized(zgui::geom::Size::new(zgui::geom::DevicePx(330.0),zgui::geom::DevicePx(height))));
                        h.settle(64);
                        click(&mut h, refs.tabs[index]);
                        // Clear the tab hover, including its delayed native-style tooltip.
                        tile_pointer(
                            &mut h,
                            PointerAction::Moved,
                            primary,
                            Point::new(CssPx(145.0), CssPx(15.0)),
                        );
                        h.run_for(Duration::from_secs(2), Duration::from_millis(16));
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
                // A growing swatch library expands the fitted page.
                click(&mut h, refs.tabs[0]);
                model.library.update(|library| {
                    for _ in 0..200 {
                        library.add(original, None).unwrap();
                    }
                });
                h.settle(32);
                fit_page(&mut h,refs);
                refs.tool_page.scroll_to(
                    ScrollTarget::By(zgui::geom::Point::new(
                        zgui::geom::DevicePx(0.0),
                        zgui::geom::DevicePx(10000.0),
                    )),
                    ScrollBehavior::Instant,
                );
                h.settle(32);
                assert_eq!(refs.tool_page.scroll_position().offset.y.0,0.0);
                assert!(refs.page_content.window_bounds().unwrap().size.height.0>560.0);
                assert!(refs.tool_page.scroll_position().is_at_end_vertically());
                assert!(refs.title.window_bounds().unwrap().origin.y.0 < 32.0);
                click(&mut h, refs.title);
                assert_eq!(model.tab.get_untracked(), Tab::Picker);
                assert!(refs.slice.window_bounds().is_some());
                // In a taller window the rows track the heading and the last entry remains clickable.
                h.deliver_to_first(SurfaceEvent::Resized(zgui::geom::Size::new(zgui::geom::DevicePx(330.0),zgui::geom::DevicePx(840.0))));
                h.settle(32);
                refs.tool_page.scroll_to(ScrollTarget::Offset(Point::new(zgui::geom::DevicePx(0.0),zgui::geom::DevicePx(0.0))),ScrollBehavior::Instant);
                h.settle(32);
                click(&mut h,refs.library[1]);
                let anchor=refs.library[1].window_bounds().unwrap();
                let menu=refs.folder_menu.window_bounds().unwrap();
                let item=refs.library[4].window_bounds().unwrap();
                assert_eq!(menu.origin.y.0,32.0);
                assert!((item.origin.y.0-anchor.origin.y.0-anchor.size.height.0/2.0).abs()<=1.0);
                let edit = Point::new(CssPx(item.origin.x.0+20.0),CssPx(item.origin.y.0+10.0));
                for action in [PointerAction::Pressed,PointerAction::Released] {tile_pointer(&mut h,action,primary,edit);}
                assert!(refs.library[2].window_bounds().is_some(),"the overlaid menu receives clicks");
                let input=refs.library[2];
                let heading=refs.library[1];
                let new_folder=refs.library[4];
                let escape=||KeyEvent::named(NamedKey::Escape,PhysicalKey::Unidentified(0));
                let point_in=|node:NodeRef,dx,dy| {
                    let b=node.window_bounds().unwrap();
                    Point::new(CssPx(b.origin.x.0+dx),CssPx(b.origin.y.0+dy))
                };
                let field=input.window_bounds().unwrap();
                assert_eq!(field.size,anchor.size);
                assert!(model.editing_text.get_untracked(),"new name receives keyboard focus");
                key(&mut h,KeyEvent::character("Discarded draft"),Modifiers::NONE);
                key(&mut h,escape(),Modifiers::NONE);
                assert!(input.window_bounds().is_none());
                assert!(!model.editing_text.get_untracked());
                click(&mut h,heading);click(&mut h,new_folder);
                key(&mut h,KeyEvent::character("Sample group"),Modifiers::NONE);
                key(&mut h,enter(),Modifiers::NONE);
                let id=model.library.get_untracked().folders.last().expect("created folder").id;
                assert_eq!(heading.text_content(),"Sample group");
                click(&mut h,heading);click(&mut h,new_folder);
                key(&mut h,enter(),Modifiers::NONE);
                assert_eq!(heading.text_content(),"Sample group","blank creation returns to previous folder");
                let center=point_in(heading,10.0,8.0);
                let hold=Duration::from_millis(730);
                tile_pointer(&mut h,PointerAction::Pressed,primary,center);
                h.run_for(hold,Duration::from_millis(20));h.settle(32);
                tile_pointer(&mut h,PointerAction::Released,primary,center);
                assert_eq!(input.text_content(),"Sample group");
                type_text(&mut h,input,"Warm");
                refs.menu.focus();h.settle(32);
                assert_eq!(heading.text_content(),"Warm","blur commits a rename");
                click(&mut h,refs.library[0]);
                let plus=refs.library[0].window_bounds().unwrap();
                assert_eq!((plus.size.width.0,plus.size.height.0),(19.0,19.0));
                {
                    let capture=frames.lock().unwrap();
                    let frame=capture.last().unwrap();
                    let (x,y)=(plus.origin.x.0.round() as i32,plus.origin.y.0.round() as i32);
                    for dy in 4..15 {for dx in 4..15 {
                        let ink=(dx==9 && (5..14).contains(&dy)) || (dy==9 && (5..14).contains(&dx));
                        assert_eq!(frame.rgba(x+dx,y+dy)[0]>50,ink,"19px swatch add at {dx}/{dy}");
                    }}
                }
                let entry=model.library.get_untracked().swatches.last().unwrap().id;
                let tile=point_in(refs.library[7],9.0,9.0);
                tile_pointer(&mut h,PointerAction::Pressed,primary,tile);
                h.run_for(hold,Duration::from_millis(20));h.settle(32);
                tile_pointer(&mut h,PointerAction::Released,primary,tile);
                let tile_border=point_in(refs.library[7],0.0,9.0);
                let pixel=|p:Point<CssPx,zgui::geom::Css>|frames.lock().unwrap().last().unwrap().rgba(p.x.0 as i32,p.y.0 as i32);
                assert_ne!(pixel(tile_border),pixel(tile),"rename outlines the selected swatch");
                type_text(&mut h,input,"Peach");
                key(&mut h,enter(),Modifiers::NONE);
                assert_eq!(model.library.get_untracked().swatches.last().unwrap().name,"Peach");
                tile_pointer(&mut h,PointerAction::Moved,primary,center);
                tile_pointer(&mut h,PointerAction::Moved,primary,tile);
                h.run_for(Duration::from_secs(2),Duration::from_millis(20));h.settle(32);
                let hex=okcolor_core::format_color(model.library.get_untracked().swatches.last().unwrap().color(),okcolor_core::ColorFormat::Hex,model.mapping.get_untracked()).unwrap().text;
                assert_eq!(refs.tooltip.text_content(),format!("Peach{hex}"));
                // Press then move early: the same hold opens the menu without entering the name field.
                tile_pointer(&mut h,PointerAction::Pressed,primary,tile);
                tile_pointer(&mut h,PointerAction::Moved,primary,center);
                h.run_for(Duration::from_millis(240),Duration::from_millis(20));h.settle(32);
                assert!(refs.folder_menu.window_bounds().is_none());
                h.run_for(hold,Duration::from_millis(20));h.settle(32);
                let unfiled=point_in(refs.library[4],12.0,10.0-44.0);
                tile_pointer(&mut h,PointerAction::Moved,primary,unfiled);
                tile_pointer(&mut h,PointerAction::Released,primary,unfiled);
                let moved=model.library.get_untracked();
                assert_eq!(moved.swatches.last().unwrap().id,entry);
                assert_eq!(moved.swatches.last().unwrap().folder,None);
                assert!(refs.folder_menu.window_bounds().is_none());
                refs.menu.focus();
                key(&mut h,KeyEvent::character("z"),Modifiers::CONTROL);
                assert_eq!(model.library.get_untracked().swatches.last().unwrap().folder,Some(id));
                // Populated folders require confirmation and preserve their colors unfiled.
                for cancel in [true,false] {
                    tile_pointer(&mut h,PointerAction::Pressed,PointerButton::Middle,center);
                    tile_pointer(&mut h,PointerAction::Released,PointerButton::Middle,center);
                    assert!(refs.library[5].window_bounds().is_some());
                    if cancel {key(&mut h,escape(),Modifiers::NONE);} else {click(&mut h,refs.library[5]);}
                }
                assert!(model.library.get_untracked().folders.iter().all(|f|f.id!=id));
                assert_eq!(model.library.get_untracked().swatches.last().unwrap().folder,None);
                model.undo(); h.settle(32);
                let restored=model.library.get_untracked();
                assert_eq!(restored.swatches.last().unwrap().folder,Some(id));
                assert_eq!(restored.swatches.last().unwrap().name,"Peach");
                assert_eq!(restored.folders.last().unwrap().name,"Warm");
                let outside_tile=point_in(refs.swatches[0],17.0,17.0);
                tile_pointer(&mut h,PointerAction::Pressed,primary,outside_tile);
                tile_pointer(&mut h,PointerAction::Moved,primary,center);
                h.run_for(hold,Duration::from_millis(20));h.settle(32);
                let folder_option=point_in(refs.library[4],12.0,10.0-22.0);
                tile_pointer(&mut h,PointerAction::Released,primary,folder_option);
                let after_drop=model.library.get_untracked();
                assert_eq!(after_drop.swatches.len(),restored.swatches.len()+1);
                assert_eq!(after_drop.swatches.last().unwrap().folder,Some(id));
                assert_eq!(after_drop.swatches.last().unwrap().color(),model.colors()[0]);
                assert_eq!(heading.text_content(),"Swatch Tiles");
                tile_pointer(&mut h,PointerAction::Pressed,PointerButton::Middle,center);
                tile_pointer(&mut h,PointerAction::Released,PointerButton::Middle,center);
                assert!(refs.library[5].window_bounds().is_none());
                assert_eq!(model.library.get_untracked(),after_drop);
                click(&mut h,heading);click(&mut h,new_folder);
                key(&mut h,KeyEvent::character("Empty"),Modifiers::NONE);
                key(&mut h,enter(),Modifiers::NONE);
                tile_pointer(&mut h,PointerAction::Pressed,PointerButton::Middle,center);
                tile_pointer(&mut h,PointerAction::Released,PointerButton::Middle,center);
                assert!(refs.library[5].window_bounds().is_none());
                assert_eq!(heading.text_content(),"Swatch Tiles");
                assert_eq!(model.library.get_untracked().folders,after_drop.folders);
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
