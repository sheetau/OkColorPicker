use okcolor_app::{
    autosave::SaveWorker,
    persistence::{SavedState, StartupState, StateStore},
};
use okcolor_core::{ColorFormat, Gamut, GamutMapping, Oklch};
use std::{
    fs,
    sync::{Arc, Mutex},
};

fn varied_state() -> SavedState {
    SavedState {
        current: Oklch::new(
            0.7654321098765432,
            0.4567890123456789,
            321.1234567890123,
            0.2345678901234567,
        )
        .unwrap(),
        previous: Oklch::new(0.4321098765432109, 0.0, 179.9876543210123, 0.0).unwrap(),
        recent: vec![
            Oklch::new(0.32, 0.4, 140.0, 0.5).unwrap(),
            Oklch::new(0.9, 0.01, 200.0, 1.0).unwrap(),
        ],
        gamut: Gamut::Rec2020,
        mapping: GamutMapping::Clip,
        format: ColorFormat::Color(Gamut::DisplayP3),
        max_chroma: 0.5,
    }
}

#[test]
fn schema_preserves_wide_values_alpha_neutral_hue_and_all_output_choices() {
    let formats = [
        ColorFormat::Hex,
        ColorFormat::Hex8,
        ColorFormat::Rgb,
        ColorFormat::Hsl,
        ColorFormat::Oklch,
        ColorFormat::Oklab,
        ColorFormat::Lab,
        ColorFormat::Lch,
        ColorFormat::Color(Gamut::Srgb),
        ColorFormat::Color(Gamut::DisplayP3),
        ColorFormat::Color(Gamut::Rec2020),
    ];
    for gamut in [Gamut::Srgb, Gamut::DisplayP3, Gamut::Rec2020] {
        for format in formats {
            let state = SavedState {
                gamut,
                format,
                ..varied_state()
            };
            let encoded = state.encode().unwrap();
            assert_eq!(SavedState::decode(&encoded).unwrap(), state);
            assert!(!encoded.starts_with(&[0xef, 0xbb, 0xbf]));
        }
    }
    // Subnormal and extreme chroma remain original f64 data, not display-rounded text.
    for c in [
        f64::from_bits(1),
        1e-200,
        0.12345678901234567,
        1e200,
        f64::MAX,
    ] {
        let state = SavedState {
            current: Oklch::new(0.51, c, 234.1, 0.51).unwrap(),
            ..varied_state()
        };
        assert_eq!(SavedState::decode(&state.encode().unwrap()).unwrap(), state);
    }
}

#[test]
fn malformed_unknown_and_oversized_profiles_are_preserved_and_disable_writes() {
    let root = tempfile::tempdir().unwrap();
    let valid = varied_state().encode().unwrap();
    let base: serde_json::Value = serde_json::from_slice(&valid).unwrap();
    let mut cases = vec![
        b"{broken".to_vec(),
        vec![b' '; 128 * 1024 + 1],
        b"{\"version\":999}".to_vec(),
    ];
    for (field, value) in [
        ("current", serde_json::json!([2.0, 0.1, 30.0, 1.0])),
        ("current", serde_json::json!([0.5, -0.1, 30.0, 1.0])),
        ("recent", serde_json::json!(vec![[0.5, 0.1, 20.0, 1.0]; 25])),
        ("max_chroma", serde_json::json!(0.0)),
        ("gamut", serde_json::json!("unknown")),
        ("format", serde_json::json!("unknown")),
        ("mapping", serde_json::json!("unknown")),
        ("extra", serde_json::json!(true)),
    ] {
        let mut invalid = base.clone();
        invalid[field] = value;
        cases.push(serde_json::to_vec(&invalid).unwrap());
    }
    for bytes in cases {
        fs::write(root.path().join("state.json"), &bytes).unwrap();
        let startup = StartupState::open(Ok(root.path().to_path_buf()));
        assert!(startup.store.is_none());
        assert!(!startup.warning.is_empty());
        assert_eq!(fs::read(root.path().join("state.json")).unwrap(), bytes);
    }
}

#[test]
fn atomic_replacement_reopens_exact_state_and_ignores_unfinished_temp_files() {
    let root = tempfile::tempdir().unwrap();
    {
        let store = StateStore::open(root.path()).unwrap();
        assert_eq!(store.load().unwrap(), SavedState::default());
        assert!(!root.path().join("state.json").exists());
        store.save(&SavedState::default()).unwrap();
        store.save(&varied_state()).unwrap();
        fs::write(root.path().join(".state-interrupted.tmp"), b"{partial").unwrap();
    }
    let store = StateStore::open(root.path()).unwrap();
    assert_eq!(store.load().unwrap(), varied_state());
    let original = fs::read(root.path().join("state.json")).unwrap();
    let invalid = SavedState {
        max_chroma: f64::NAN,
        ..varied_state()
    };
    assert!(store.save(&invalid).is_err());
    assert_eq!(fs::read(root.path().join("state.json")).unwrap(), original);
}

#[test]
fn only_one_writer_can_hold_the_profile_and_closing_releases_it() {
    let root = tempfile::tempdir().unwrap();
    let first = StateStore::open(root.path()).unwrap();
    assert!(StateStore::open(root.path()).is_err());
    let startup = StartupState::open(Ok(root.path().to_path_buf()));
    assert!(startup.store.is_none());
    assert!(!startup.warning.is_empty());
    drop(first);
    assert!(StateStore::open(root.path()).is_ok());
    assert!(StateStore::open(std::path::Path::new("relative-profile")).is_err());
}

#[test]
fn writer_flushes_the_latest_pending_snapshot_before_shutdown() {
    let root = tempfile::tempdir().unwrap();
    let store = StateStore::open(root.path()).unwrap();
    let outcomes = Arc::new(Mutex::new(Vec::new()));
    let reports = outcomes.clone();
    let mut worker = SaveWorker::start(store, SavedState::default(), move |result| {
        reports.lock().unwrap().push(result.is_ok())
    })
    .unwrap();
    for index in 0..100 {
        let mut state = varied_state();
        state.current = Oklch::new(index as f64 / 100.0, 0.3, 123.0, 0.7).unwrap();
        worker.submit(state);
    }
    worker.submit(varied_state());
    worker.finish();
    assert_eq!(
        StateStore::open(root.path()).unwrap().load().unwrap(),
        varied_state()
    );
    let results = outcomes.lock().unwrap();
    assert!(!results.is_empty());
    assert!(results.iter().all(|success| *success));
}

#[cfg(windows)]
#[test]
fn failed_windows_replace_preserves_previous_bytes_and_can_be_retried() {
    use std::os::windows::fs::OpenOptionsExt;
    let root = tempfile::tempdir().unwrap();
    let store = StateStore::open(root.path()).unwrap();
    store.save(&SavedState::default()).unwrap();
    let path = root.path().join("state.json");
    let before = fs::read(&path).unwrap();
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&path)
        .unwrap();
    assert!(store.save(&varied_state()).is_err());
    drop(held);
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(!fs::read_dir(root.path()).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")
    }));
    store.save(&varied_state()).unwrap();
    assert_eq!(store.load().unwrap(), varied_state());
}

#[cfg(windows)]
#[test]
fn worker_reports_failure_and_retries_without_treating_failed_data_as_saved() {
    use std::{os::windows::fs::OpenOptionsExt, sync::mpsc, time::Duration};
    let root = tempfile::tempdir().unwrap();
    let store = StateStore::open(root.path()).unwrap();
    store.save(&SavedState::default()).unwrap();
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(root.path().join("state.json"))
        .unwrap();
    let (send, receive) = mpsc::channel();
    let mut worker = SaveWorker::start(store, SavedState::default(), move |result| {
        send.send(result.is_ok()).unwrap();
    })
    .unwrap();
    worker.submit(varied_state());
    assert!(!receive.recv_timeout(Duration::from_secs(3)).unwrap());
    // Returning to the on-disk snapshot clears the failed-save status without another write.
    worker.submit(SavedState::default());
    assert!(receive.recv_timeout(Duration::from_secs(3)).unwrap());
    drop(held);
    worker.submit(varied_state());
    assert!(receive.recv_timeout(Duration::from_secs(3)).unwrap());
    worker.finish();
    assert_eq!(
        StateStore::open(root.path()).unwrap().load().unwrap(),
        varied_state()
    );
}
