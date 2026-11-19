use okcolor_app::{
    display::{DisplayBridge, DisplayMode, DisplayStatus},
    model::PickerModel,
};
use okcolor_core::{Gamut, Oklch};
use zgui::prelude::*;

#[test]
fn display_transitions_update_only_preview_and_cleanup_detaches_the_signal() {
    zgui::reactive::install().expect("reactive runtime");
    let bridge = DisplayBridge::default();
    let wide = DisplayStatus {
        mode: DisplayMode::WideColor,
        wide_output: true,
    };
    assert!(!bridge.publish(wide), "An unmounted view owes no repaint");
    let owner = Owner::new();
    owner.with(|| {
        let model = PickerModel::new(
            Oklch::from_rgb([1.0, 0.0, 0.0], Gamut::DisplayP3, 0.31, 0.0).unwrap(),
        );
        bridge.bind(model.display);
        assert_eq!(model.display.get_untracked(), wide);
        let saved = model.saved_state_untracked();
        assert!(!bridge.publish(wide));
        for status in [
            DisplayStatus {
                mode: DisplayMode::Sdr,
                wide_output: false,
            },
            DisplayStatus::default(),
            wide,
        ] {
            assert!(bridge.publish(status));
            assert_eq!(model.display.get_untracked(), status);
            assert_eq!(model.saved_state_untracked(), saved);
            assert!(!model.can_undo.get_untracked());
        }
    });
    drop(owner);
    assert!(
        !bridge.publish(DisplayStatus::default()),
        "A dropped view must not receive signal writes"
    );
}

#[test]
fn availability_checks_each_supported_gamut_before_marking_a_color_unavailable() {
    let sdr = DisplayStatus::default();
    let wide = DisplayStatus {
        mode: DisplayMode::WideColor,
        wide_output: true,
    };
    let p3_red = Oklch::from_rgb([1.0, 0.0, 0.0], Gamut::DisplayP3, 1.0, 0.0).unwrap();
    let rec_green = Oklch::from_rgb([0.0, 1.0, 0.0], Gamut::Rec2020, 1.0, 0.0).unwrap();
    let outside = Oklch::new(0.5, 0.8, 285.0, 1.0).unwrap();
    assert_eq!(wide.unavailable(p3_red), None);
    assert_eq!(wide.unavailable(rec_green), None);
    assert_eq!(
        sdr.unavailable(p3_red),
        Some("P3 is unavailable on this monitor")
    );
    assert_eq!(
        sdr.unavailable(rec_green),
        Some("Rec.2020 is unavailable on this monitor")
    );
    for display in [sdr, wide] {
        assert_eq!(
            display.unavailable(outside),
            Some("unavailable on any device")
        );
    }
}
