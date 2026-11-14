use okcolor_core::{EditError, Gamut, GamutSlice, History, Oklch};

fn color(l: f64) -> Oklch {
    Oklch::new(l, 0.1, 250.0, 0.37).unwrap()
}

#[test]
fn a_drag_is_one_exact_undo_step_and_one_recent_color() {
    let mut history = History::new(color(0.1), 100, 10);
    history.begin().unwrap();
    for sample in 0..100 {
        history.preview(color(f64::from(sample) / 100.0)).unwrap();
    }
    assert!(history.recent().is_empty());
    assert_eq!(history.undo(), Err(EditError::AlreadyEditing));
    history.commit().unwrap();
    assert_eq!(history.recent().len(), 1);
    history.undo().unwrap();
    assert_eq!(history.current(), color(0.1));
    assert!(!history.can_undo());
    history.redo().unwrap();
    assert_eq!(history.current(), color(0.99));
}

#[test]
fn cancel_and_noop_preserve_redo_but_a_new_edit_branches() {
    let mut history = History::new(color(0.1), 2, 2);
    history.set(color(0.2)).unwrap();
    history.undo().unwrap();
    history.begin().unwrap();
    history.preview(color(0.7)).unwrap();
    history.cancel().unwrap();
    assert_eq!(history.current(), color(0.1));
    assert!(!history.set(color(0.1)).unwrap());
    assert!(history.can_redo());
    history.set(color(0.3)).unwrap();
    assert!(!history.can_redo());
    history.set(color(0.4)).unwrap();
    history.set(color(0.3)).unwrap();
    assert_eq!(
        history.recent().iter().copied().collect::<Vec<_>>(),
        [color(0.3), color(0.4)]
    );
    history.undo().unwrap();
    history.undo().unwrap();
    assert!(!history.can_undo());
}

#[test]
fn slice_rejects_dead_regions_and_keeps_absolute_chroma() {
    let slice = GamutSlice::new(250.0, 0.4, Gamut::Srgb).unwrap();
    assert!(slice.pick(1.0, 0.5, 1.0).is_none());
    assert!(slice.pick(-0.1, 0.5, 1.0).is_none());
    assert!(slice.pick(f64::NAN, 0.5, 1.0).is_none());
    let pick = slice.pick(0.1, 0.5, 0.2).unwrap();
    assert!((pick.components()[1] - 0.04).abs() < 1e-14);
    assert_eq!(pick.components()[3], 0.2);
    assert_eq!(
        slice.marker(Oklch::new(0.5, 0.8, 250.0, 1.0).unwrap()),
        [2.0, 0.5]
    );
}

#[test]
fn gamut_boundaries_have_valid_interior_and_invalid_exterior() {
    for hue in (0..360).step_by(5) {
        let slice = GamutSlice::new(f64::from(hue), 0.4, Gamut::Rec2020).unwrap();
        for l in 1..20 {
            let l = f64::from(l) / 20.0;
            for gamut in [Gamut::Srgb, Gamut::DisplayP3, Gamut::Rec2020] {
                let c = slice.boundary(l, gamut).unwrap();
                assert!(
                    Oklch::new(l, c, f64::from(hue), 1.0)
                        .unwrap()
                        .in_gamut(gamut)
                );
                assert!(
                    !Oklch::new(l, c + 1e-6, f64::from(hue), 1.0)
                        .unwrap()
                        .in_gamut(gamut)
                );
            }
        }
    }
}

#[test]
fn wheel_edits_keep_recents_and_still_save_undo_and_branch() {
    let mut history = History::new(color(0.1), 100, 24);
    history.set(color(0.2)).unwrap();
    let recent = history.recent().clone();
    for index in 30..40 {
        history
            .set_without_recent(color(f64::from(index) / 100.0))
            .unwrap();
    }
    assert_eq!(history.recent(), &recent);
    assert_eq!(history.committed(), color(0.39));
    history.undo().unwrap();
    assert_eq!(history.current(), color(0.38));
    history.redo().unwrap();
    assert_eq!(history.current(), color(0.39));
    history.undo().unwrap();
    history.set_without_recent(color(0.5)).unwrap();
    assert!(!history.can_redo());
    assert_eq!(history.recent(), &recent);
}

#[test]
fn captured_slice_drag_tracks_the_boundary_and_can_reenter() {
    for gamut in [Gamut::Srgb, Gamut::DisplayP3, Gamut::Rec2020] {
        for hue in (0..360).step_by(15) {
            let slice = GamutSlice::new(f64::from(hue), 0.4, gamut).unwrap();
            for y in [-1.0, 0.0, 0.1, 0.3, 0.5, 0.9, 1.0, 2.0] {
                let picked = slice.drag(1.5, y, 0.37).unwrap();
                let [l, c, h, alpha] = picked.components();
                assert_eq!(l, 1.0 - y.clamp(0.0, 1.0));
                assert_eq!((h, alpha), (f64::from(hue), 0.37));
                assert!(picked.in_gamut(gamut));
                assert!((c - slice.boundary(l, gamut).unwrap().min(0.4)).abs() < 1e-7);
            }
            assert_eq!(slice.drag(-1.0, 0.5, 1.0).unwrap().components()[1], 0.0);
            assert_eq!(slice.drag(0.01, 0.5, 0.37), slice.pick(0.01, 0.5, 0.37));
            assert!(slice.drag(f64::NAN, 0.5, 1.0).is_none());
        }
    }
}
