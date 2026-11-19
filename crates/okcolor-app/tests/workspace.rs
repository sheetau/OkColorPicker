use okcolor_app::{
    persistence::SavedState,
    workspace::{Library, Tab},
};
use okcolor_core::Oklch;

#[test]
fn contrast_css_suffixes_renumber_after_every_structural_edit() {
    let mut grid = okcolor_app::workspace::ContrastGrid {
        level_names: vec![100, 300, 438, 450],
        ..Default::default()
    };
    for count in 4..=12 {
        let step = if count <= 9 { 100 } else { 50 };
        assert_eq!(grid.level_name(0), step);
        assert_eq!(grid.level_name(count - 1), step * count as u32);
        grid.insert(2);
    }
    for _ in 0..8 {
        grid.remove(1);
    }
    assert_eq!(grid.level_name(4), 500);
}

#[test]
fn signed_grid_edits_survive_insertion_removal_and_profile_reload() {
    let mut state = SavedState::default();
    let grid = &mut state.tools.contrast_grid;
    grid.set_target(1, -25.0, Some(false));
    grid.insert(0);
    assert_eq!(grid.positive_polarities[2], Some(false));
    grid.remove(1);
    assert_eq!(grid.positive_polarities[1], Some(false));
    let mut restored = SavedState::decode(&state.encode().unwrap()).unwrap();
    assert_eq!(restored, state);
    restored.tools.contrast_grid.set_target(1, 20.0, None);
    assert!(restored.tools.contrast_grid.positive_polarities.is_empty());
}

#[test]
fn library_edits_keep_originals_and_folder_removal_keeps_colors() {
    let color = Oklch::new(0.6543210987, 0.42, 312.123456, 0.65).unwrap();
    let mut library = Library::default();
    let folder = library.create_folder(" Palette ").unwrap();
    let first = library.add(color, Some(folder)).unwrap();
    let second = library.add(color, None).unwrap();
    library.rename(first, "Accent").unwrap();
    library.move_swatch(second, Some(folder)).unwrap();
    assert!(library.move_swatch(first, Some(999)).is_err());
    assert!(library.rename(first, "").is_err());
    library.remove(folder);
    assert_eq!(library.swatches.len(), 2);
    assert!(
        library
            .swatches
            .iter()
            .all(|s| s.folder.is_none() && s.color() == color)
    );
    library.remove(first);
    assert_eq!(library.swatches[0].id, second);
    library.validate().unwrap();
    let state = SavedState {
        library,
        ..Default::default()
    };
    assert_eq!(SavedState::decode(&state.encode().unwrap()).unwrap(), state);
}

#[test]
fn malformed_library_and_tool_settings_cannot_replace_profile() {
    let state = SavedState::default();
    let value: serde_json::Value = serde_json::from_slice(&state.encode().unwrap()).unwrap();
    for (field, bad) in [
        ("target_lc", serde_json::json!(111)),
        ("harmony", serde_json::json!(5)),
        ("background", serde_json::json!([0.5, 0.1, 200, 0.5])),
    ] {
        let mut corrupted = value.clone();
        corrupted["tools"][field] = bad;
        assert!(SavedState::decode(&serde_json::to_vec(&corrupted).unwrap()).is_err());
    }
    let mut corrupted = value;
    corrupted["library"]["swatches"] =
        serde_json::json!([{"id":1,"name":"bad","color":[0.5,0.1,20,1],"folder":42}]);
    assert!(SavedState::decode(&serde_json::to_vec(&corrupted).unwrap()).is_err());
}

#[test]
fn v3_migration_keeps_color_settings_and_adds_empty_tools() {
    let state = SavedState::default();
    let mut value: serde_json::Value = serde_json::from_slice(&state.encode().unwrap()).unwrap();
    let fields = value.as_object_mut().unwrap();
    fields.insert("version".into(), 3.into());
    fields.remove("library");
    fields.remove("tools");
    assert_eq!(
        SavedState::decode(&serde_json::to_vec(&value).unwrap()).unwrap(),
        state
    );
    assert_eq!(Tab::Picker.next(true), Tab::Apca);
    assert_eq!(Tab::Apca.next(false), Tab::Picker);
}

#[test]
fn full_library_with_maximum_unicode_names_round_trips() {
    let mut state = SavedState::default();
    let name = "🌈".repeat(64);
    for _ in 0..32 {
        state.library.create_folder(&name).unwrap();
    }
    for _ in 0..256 {
        let id = state.library.add(state.main, None).unwrap();
        state.library.rename(id, &name).unwrap();
    }
    assert!(state.library.add(state.main, None).is_err());
    assert!(state.library.create_folder("Overflow").is_err());
    assert_eq!(SavedState::decode(&state.encode().unwrap()).unwrap(), state);
}

#[test]
fn existing_v4_profile_loads_with_default_grid_and_rejects_invalid_splits() {
    let state = SavedState::default();
    let mut value: serde_json::Value = serde_json::from_slice(&state.encode().unwrap()).unwrap();
    let tools = value["tools"].as_object_mut().unwrap();
    for key in ["contrast_grid", "always_on_top", "show_alpha_slider"] {
        tools.remove(key);
    }
    assert_eq!(
        SavedState::decode(&serde_json::to_vec(&value).unwrap()).unwrap(),
        state
    );
    value["tools"]["contrast_grid"] = serde_json::to_value(state.tools.contrast_grid).unwrap();
    value["tools"]["contrast_grid"]["split"] = 17.into();
    assert!(SavedState::decode(&serde_json::to_vec(&value).unwrap()).is_err());
}
