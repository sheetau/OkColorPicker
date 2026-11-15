use okcolor_csp::{
    color::{ColorCache, ColorCommand},
    frame::{Frame, Kind},
};
use serde_json::{Value, json};
fn frame(command: &str, detail: Value) -> Frame {
    Frame {
        kind: Kind::Success,
        command: command.into(),
        serial: 1,
        detail: Some(detail),
        data: vec![],
    }
}
fn circle(detail: Value) -> Frame {
    frame("SyncColorCircleUIState", detail)
}
fn full() -> Value {
    json!({"CurrentColorIndex":0,"ColorSelectionModel":"HSV",
        "HSVColorMainH":0,"HSVColorMainS":4294967295u32,"HSVColorMainV":4294967295u32,
        "HSVColorSubH":0,"HSVColorSubS":0,"HSVColorSubV":4294967295u32,
        "IsCurrentColorTransparent":false,"CanExecuteSetterCommand":true,"IsManipulating":false})
}
#[test]
fn complete_partial_and_slot_switches_keep_unsigned_precision() {
    let mut cache = ColorCache::default();
    assert!(
        cache
            .apply(&circle(json!({"CurrentColorIndex":0})))
            .unwrap()
            .is_none()
    );
    let red = cache.apply(&circle(full())).unwrap().unwrap();
    assert_eq!(red.rgb, [1.0, 0.0, 0.0]);
    let cyan = cache
        .apply(&circle(
            json!({"HSVColorMainH":2147483648u32,"IsManipulating":true}),
        ))
        .unwrap()
        .unwrap();
    assert!(
        cyan.rgb[0] < 1e-9 && (cyan.rgb[1] - 1.0).abs() < 1e-9 && (cyan.rgb[2] - 1.0).abs() < 1e-9
    );
    assert!(cyan.manipulating);
    let white = cache
        .apply(&circle(
            json!({"CurrentColorIndex":1,"IsCurrentColorTransparent":true}),
        ))
        .unwrap()
        .unwrap();
    assert_eq!(white.rgb, [1.0; 3]);
    assert!(white.transparent);
    assert_eq!(white.slot, 1);
    // Brush opacity is never an alpha channel or a color-selector change.
    assert_eq!(
        cache
            .apply(&circle(json!({"CurrentToolAlphaPercent":0})))
            .unwrap()
            .unwrap(),
        white
    );
}
#[test]
fn invalid_updates_are_atomic_and_missing_channels_never_become_black() {
    let mut cache = ColorCache::default();
    assert!(cache.apply(&circle(json!({}))).unwrap().is_none());
    let before = cache.apply(&circle(full())).unwrap().unwrap();
    for invalid in [
        json!(-1),
        json!(4294967296u64),
        json!(0.5),
        json!("0"),
        Value::Null,
    ] {
        assert!(
            cache
                .apply(&circle(
                    json!({"HSVColorMainH":100,"HSVColorMainS":invalid})
                ))
                .is_err()
        );
        assert_eq!(cache.apply(&circle(json!({}))).unwrap().unwrap(), before);
    }
    assert!(cache.apply(&circle(json!({"IsManipulating":1}))).is_err());
    assert!(
        cache
            .apply(&circle(json!({"CurrentColorIndex":2})))
            .is_err()
    );
    assert!(
        cache
            .apply(&circle(json!({"ServerSelectedTabKind":"ColorCircle"})))
            .unwrap()
            .is_none()
    );
    // A fresh connection has no baseline, regardless of a previous connection.
    assert!(
        ColorCache::default()
            .apply(&circle(json!({"HSVColorMainH":0})))
            .unwrap()
            .is_none()
    );
}
#[test]
fn incomplete_circle_uses_observed_rgb_without_carrying_it_across_slots() {
    let mut cache = ColorCache::default();
    cache.apply(&circle(full())).unwrap();
    assert!(
        cache
            .apply(&circle(json!({"ColorSelectionModel":"HLS"})))
            .unwrap()
            .is_none()
    );
    let rgb=cache.apply(&frame("SyncSubViewUIState",json!({"CurrentColorRed":1285049496u32,"CurrentColorGreen":1304645059u32,"CurrentColorBlue":899102103u32}))).unwrap().unwrap();
    assert!((rgb.rgb[0] - 1285049496.0 / 4294967295.0).abs() < 1e-12);
    // Cached active RGB must not carry across a foreground/background selection.
    cache
        .apply(&circle(json!({"CurrentColorIndex":1})))
        .unwrap();
    assert!(
        cache
            .apply(&frame("SyncSubViewUIState", json!({})))
            .unwrap()
            .is_none()
    );
}

#[test]
fn hls_notifications_update_both_slots_and_partial_samples_without_rgb_polling() {
    let mut cache = ColorCache::default();
    let mut state = full();
    state["ColorSelectionModel"] = json!("HLS");
    for (key, value) in [
        ("HLSColorMainH", 0),
        ("HLSColorMainL", 2147483648u32),
        ("HLSColorMainS", u32::MAX),
        ("HLSColorSubH", 0),
        ("HLSColorSubL", u32::MAX),
        ("HLSColorSubS", 0),
    ] {
        state[key] = json!(value);
    }
    let red = cache.apply(&circle(state)).unwrap().unwrap();
    assert!(red.rgb[0] > 0.999999 && red.rgb[1] < 1e-8);
    assert_eq!(red.colors[1], Some([1.0; 3]));
    let cyan = cache
        .apply(&circle(
            json!({"HLSColorMainH":2147483648u32,"IsManipulating":true}),
        ))
        .unwrap()
        .unwrap();
    assert!(cyan.rgb[0] < 1e-8 && cyan.rgb[1] > 0.999999 && cyan.rgb[2] > 0.999999);
    assert!(cyan.manipulating);
    assert_eq!(cyan.colors[1], Some([1.0; 3]));
    let sub = cache
        .apply(&circle(json!({"CurrentColorIndex":1,"HLSColorSubL":0})))
        .unwrap()
        .unwrap();
    assert_eq!(sub.rgb, [0.0; 3]);
    assert_eq!(sub.colors[0], cyan.colors[0]);
}
#[test]
fn explicit_slot_hsv_commands_round_trip_bounded_rgb_without_alpha() {
    for r in [0.0, 0.1, 0.5, 1.0] {
        for g in [0.0, 0.2, 0.6, 1.0] {
            for b in [0.0, 0.3, 0.7, 1.0] {
                let command = ColorCommand::new([r, g, b], 1, true).unwrap();
                assert_eq!(command.slot(), 1);
                assert!(command.transparent());
                for (actual, expected) in command.rgb().into_iter().zip([r, g, b]) {
                    assert!((actual - expected).abs() < 2e-9);
                }
            }
        }
    }
    for rgb in [[f64::NAN, 0.0, 0.0], [-0.1, 0.0, 0.0], [1.1, 0.0, 0.0]] {
        assert!(ColorCommand::new(rgb, 0, false).is_err());
    }
    assert!(ColorCommand::new([0.0; 3], 2, false).is_err());
}

#[test]
fn changing_color_model_drops_obsolete_channel_baselines() {
    let mut cache = ColorCache::default();
    cache.apply(&circle(full())).unwrap();
    cache
        .apply(&circle(json!({"ColorSelectionModel":"HLS"})))
        .unwrap();
    assert!(
        cache
            .apply(&circle(
                json!({"ColorSelectionModel":"HSV","HSVColorMainH":1})
            ))
            .unwrap()
            .is_none()
    );
    assert!(
        cache
            .apply(&circle(
                json!({"HSVColorMainS":0,"HSVColorMainV":4294967295u32})
            ))
            .unwrap()
            .is_some()
    );
}
