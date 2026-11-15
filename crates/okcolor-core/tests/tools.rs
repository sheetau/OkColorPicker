use okcolor_core::{
    Gamut, GamutMapping, Harmony, Oklch, Ramp, apca, harmony, lightness_for_contrast, parse_color,
    ramp,
};
const M: GamutMapping = GamutMapping::LocalMinde;
fn color(s: &str) -> Oklch {
    parse_color(s, 0.0).unwrap()
}

#[test]
fn signed_apca_reference_values_and_alpha() {
    // Color.js 0.0.98G reference results, encoded sRGB endpoints.
    assert!((apca(color("#000"), color("#fff"), M).unwrap() - 106.04067321268862).abs() < 1e-5);
    assert!((apca(color("#fff"), color("#000"), M).unwrap() + 107.88473318309848).abs() < 1e-5);
    assert_eq!(apca(color("#777"), color("#777"), M).unwrap(), 0.0);
    assert_eq!(apca(color("#ff000000"), color("#fff"), M).unwrap(), 0.0);
    assert!(apca(color("#000"), color("#ffffff80"), M).is_err());
}
#[test]
fn contrast_solver_preserves_components_and_rejects_unreachable_targets() {
    let original = Oklch::new(0.6, 0.13, 272.123, 0.9).unwrap();
    for (bg, target) in [("#fff", 60.0), ("#000", -60.0)] {
        let adjusted = lightness_for_contrast(original, color(bg), target, M)
            .unwrap()
            .unwrap();
        assert_eq!(&adjusted.components()[1..], &original.components()[1..]);
        assert!((apca(adjusted, color(bg), M).unwrap() - target).abs() <= 0.01);
    }
    for target in [3.0, 110.0] {
        assert!(
            lightness_for_contrast(original, color("#fff"), target, M)
                .unwrap()
                .is_none()
        );
    }
    assert!(
        lightness_for_contrast(color("#ff000000"), color("#fff"), 60.0, M)
            .unwrap()
            .is_none()
    );
    assert!(lightness_for_contrast(original, color("#fff"), f64::NAN, M).is_err());
}
#[test]
fn related_colors_fit_each_gamut_and_preserve_hue_and_alpha() {
    let original = Oklch::new(0.63, 0.35, 342.5, 0.61).unwrap();
    for gamut in [Gamut::Srgb, Gamut::DisplayP3, Gamut::Rec2020] {
        for kind in [Ramp::Shades, Ramp::Tints, Ramp::Tones] {
            let colors = ramp(original, kind, gamut);
            for c in colors {
                assert!(c.in_gamut(gamut));
                assert_eq!(c.components()[2..], original.components()[2..]);
            }
            assert_eq!(colors[6].components()[1], 0.0);
        }
        for kind in Harmony::ALL {
            for c in harmony(original, kind, gamut) {
                assert!(c.in_gamut(gamut));
                assert_eq!(c.components()[0], 0.63);
                assert_eq!(c.components()[3], 0.61);
            }
        }
    }
    assert_eq!(
        ramp(original, Ramp::Shades, Gamut::Srgb)[6].components()[0],
        0.0
    );
    assert_eq!(
        ramp(original, Ramp::Tints, Gamut::Srgb)[6].components()[0],
        1.0
    );
}
