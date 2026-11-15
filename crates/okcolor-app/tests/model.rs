use okcolor_app::{model::PickerModel, persistence::SavedState};
use okcolor_core::{ColorFormat, Gamut, Oklch};
use zgui::prelude::*;

#[test]
fn invalid_drafts_and_competing_edits_do_not_replace_the_color() {
    Owner::new().with(|| {
        let model = PickerModel::default();
        let before = model.current();
        assert!(!model.input("oklch(.5 .2"));
        assert_eq!(model.current(), before);
        assert!(model.begin());
        assert!(!model.input("#f00"));
        assert_eq!(model.current(), before);
        model.cancel();
        assert!(model.input("#f00"));
        assert_eq!(model.output(ColorFormat::Hex), "#ff0000");
        assert!(model.can_undo.get());
        model.undo();
        assert_eq!(model.current(), before);
    });
}

#[test]
fn drag_updates_only_changed_channels_and_commits_once() {
    Owner::new().with(|| {
        let model = PickerModel::default();
        let before = model.current();
        let [_, c, h, a] = before.components();
        assert!(model.begin());
        for i in 30..80 {
            model.preview(Oklch::new(f64::from(i) / 100.0, c, h, a).unwrap());
        }
        assert_eq!(model.h.get(), h);
        assert_eq!(model.c.get(), c);
        assert!(model.recent.get().is_empty());
        assert!(!model.can_undo.get());
        model.commit();
        assert_eq!(model.slots[1].get(), SavedState::default().sub);
        assert_eq!(model.recent.get().len(), 1);
        model.undo();
        assert_eq!(model.current(), before);
        assert!(!model.can_undo.get());
        model.redo();
        assert_eq!(model.current().components()[0], 0.79);
    });
}

#[test]
fn persisted_snapshots_exclude_previews_even_when_settings_or_failed_edits_publish() {
    Owner::new().with(|| {
        let _zone = zgui::reactive::enter_non_reactive_zone();
        let model = PickerModel::default();
        let before = model.saved_state();
        model.begin();
        model.preview(Oklch::new(0.32, 0.45, 123.0, 0.71).unwrap());
        model.gamut.set(Gamut::DisplayP3);
        assert!(!model.input("#ff0"));
        let during = model.saved_state();
        assert_eq!(during.main, before.main);
        assert_eq!(during.recent, before.recent);
        assert_eq!(during.gamut, Gamut::DisplayP3);
        model.cancel();
        assert_eq!(model.saved_state(), during);
        model.begin();
        let final_color = Oklch::new(0.62, 0.32, 144.0, 0.4).unwrap();
        model.preview(final_color);
        model.commit();
        assert_eq!(model.saved_state().main, final_color);
        model.undo();
        assert_eq!(model.saved_state().main, before.main);
        model.redo();
        assert_eq!(model.saved_state().main, final_color);
    });
}

#[test]
fn restore_retains_recent_order_and_both_slots_without_restoring_undo() {
    Owner::new().with(|| {
        let _zone = zgui::reactive::enter_non_reactive_zone();
        let a = Oklch::new(0.3, 0.5, 140.0, 0.5).unwrap();
        let b = Oklch::new(0.7, 0.0, 259.0, 0.7).unwrap();
        let saved = SavedState {
            main: b,
            sub: a,
            recent: vec![a, b, a],
            ..SavedState::default()
        };
        let model = PickerModel::restore(saved);
        assert_eq!(model.current(), b);
        assert_eq!(model.slots[1].get_untracked(), a);
        assert_eq!(model.recent.get_untracked(), vec![a, b]);
        assert!(!model.can_undo.get_untracked());
        assert!(!model.can_redo.get_untracked());
        model.select(a);
        assert_eq!(model.recent.get_untracked(), vec![a, b]);
        model.undo();
        assert_eq!(model.current(), b);
        assert!(!model.can_undo.get_untracked());
    });
}

#[test]
fn slots_select_without_swapping_and_swap_undo_restores_both_exact_colors() {
    Owner::new().with(|| {
        let model = PickerModel::default();
        let originals = model.colors();
        model.select_slot(1);
        assert_eq!(model.colors(), originals);
        assert_eq!(model.current(), originals[1]);
        assert!(!model.can_undo.get_untracked());
        assert!(model.recent.get_untracked().is_empty());
        let sub = Oklch::new(0.612345678901, 0.42, 301.123456789, 0.25).unwrap();
        model.select(sub);
        model.select_slot(0);
        assert_eq!(model.colors(), [originals[0], sub]);
        model.swap();
        assert_eq!(model.colors(), [sub, originals[0]]);
        model.undo();
        assert_eq!(model.colors(), [originals[0], sub]);
        model.undo();
        assert_eq!(model.colors(), originals);
        model.redo();
        model.redo();
        model.select_slot(1);
        let saved = model.saved_state_untracked();
        let restored = PickerModel::restore(SavedState::decode(&saved.encode().unwrap()).unwrap());
        assert_eq!(restored.colors(), model.colors());
        assert_eq!(restored.selected.get_untracked(), 1);
        assert_eq!(restored.current(), originals[0]);
    });
}
