use okcolor_core::{Gamut, Oklch};

fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() < tolerance,
        "{actual} != {expected}"
    );
}

#[test]
fn red_matches_the_published_oklab_reference() {
    let c = Oklch::from_rgb([1.0, 0.0, 0.0], Gamut::Srgb, 0.4, 0.0).unwrap();
    for (actual, expected) in c.oklab().into_iter().zip([0.627955, 0.224863, 0.125846]) {
        close(actual, expected, 1e-6);
    }
    assert_eq!(c.components()[3], 0.4);
}

#[test]
fn all_rgb_cube_samples_round_trip_in_each_gamut() {
    for gamut in [Gamut::Srgb, Gamut::DisplayP3, Gamut::Rec2020] {
        for r in 0..=8 {
            for g in 0..=8 {
                for b in 0..=8 {
                    let rgb = [r, g, b].map(|v| f64::from(v) / 8.0);
                    let color = Oklch::from_rgb(rgb, gamut, 1.0, 135.0).unwrap();
                    assert!(color.in_gamut(gamut), "{gamut:?} {rgb:?}");
                    for (actual, expected) in color.encoded_rgb(gamut).into_iter().zip(rgb) {
                        close(actual, expected, 3e-6);
                    }
                }
            }
        }
    }
}

#[test]
fn wide_gamut_color_and_alpha_survive_mapping() {
    let original = Oklch::from_rgb([1.0, 0.0, 0.0], Gamut::DisplayP3, 0.25, 0.0).unwrap();
    assert!(!original.in_gamut(Gamut::Srgb));
    assert!(original.linear_rgb(Gamut::Srgb)[0] > 1.0);
    let mapped = original.reduce_chroma(Gamut::Srgb);
    assert!(mapped.in_gamut(Gamut::Srgb));
    assert!(mapped.components()[1] < original.components()[1]);
    for index in [0, 2, 3] {
        assert_eq!(mapped.components()[index], original.components()[index]);
    }
    assert!(original.in_gamut(Gamut::DisplayP3));
}

#[test]
fn neutral_hue_and_input_invariants_are_preserved() {
    let neutral = Oklch::from_rgb([0.5; 3], Gamut::Srgb, 1.0, 270.0).unwrap();
    assert_eq!(neutral.components()[1..3], [0.0, 270.0]);
    assert_eq!(
        Oklch::new(0.5, 0.2, -30.0, 1.0).unwrap().components()[2],
        330.0
    );
    assert!(Oklch::new(0.5, f64::NAN, 0.0, 1.0).is_err());
    assert!(Oklch::new(0.5, -0.1, 0.0, 1.0).is_err());
    assert!(Oklch::new(0.5, 0.2, 0.0, 1.1).is_err());
}
