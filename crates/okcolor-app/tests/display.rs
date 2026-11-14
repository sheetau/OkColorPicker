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
