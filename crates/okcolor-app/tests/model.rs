use okcolor_app::model::PickerModel;
use okcolor_core::{ColorFormat, Oklch};
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
        assert_eq!(model.previous.get(), before);
        assert_eq!(model.recent.get().len(), 1);
        model.undo();
        assert_eq!(model.current(), before);
        assert!(!model.can_undo.get());
        model.redo();
        assert_eq!(model.current().components()[0], 0.79);
    });
}
