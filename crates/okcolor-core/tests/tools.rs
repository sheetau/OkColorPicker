use okcolor_core::{
    Gamut, GamutMapping, Harmony, Oklch, Ramp, apca, apca_in, contrast_sample, harmony,
    lightness_for_contrast, parse_color, ramp,
};
const M: GamutMapping = GamutMapping::LocalMinde;
fn color(s: &str) -> Oklch {
    parse_color(s, 0.0).unwrap()
}

#[test]
fn variations_keep_source_and_alpha_in_each_center() {
    use okcolor_core::{Variation, variations};
    let source = Oklch::new(0.51, 0.12, 359.0, 0.37).unwrap();
    for gamut in [Gamut::Srgb, Gamut::DisplayP3] {
        for row in [
            Variation::Lightness,
            Variation::Chroma,
            Variation::Hue,
            Variation::Temperature,
        ] {
            let samples = variations(source, row, gamut);
            assert_eq!(samples[6], source);
            assert!(samples.iter().all(|c| c.components()[3] == 0.37));
            assert_ne!(samples[0], samples[12]);
        }
    }
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

#[test]
fn contrast_grid_respects_direction_gamut_and_chroma_limit() {
    let source = Oklch::new(0.6, 0.04, 260.0, 0.5).unwrap();
    for gamut in [Gamut::Srgb, Gamut::DisplayP3] {
        for reference in [color("#fff"), color("#000")] {
            for to_text in [true, false] {
                for max_chroma in [false, true] {
                    let sample =
                        contrast_sample(source, reference, 60.0, gamut, to_text, max_chroma)
                            .unwrap();
                    let (fg, bg) = if to_text {
                        (reference, sample)
                    } else {
                        (sample, reference)
                    };
                    let score = apca_in(fg, bg, M, gamut).unwrap();
                    assert!((score.abs() - 60.0).abs() < 0.05);
                    assert!(sample.in_gamut(gamut));
                    let [_, c, h, a] = sample.components();
                    assert!(max_chroma || c <= 0.04);
                    assert_eq!((h, a), (260.0, 1.0));
                }
            }
        }
    }
}

#[test]
fn contrast_grid_retains_matching_lightness_and_skips_unreachable_levels() {
    let source = color("#777");
    assert_eq!(
        contrast_sample(source, source, 0.0, Gamut::Srgb, true, false),
        Some(source)
    );
    for target in [3.0, 110.0, f64::NAN] {
        assert!(contrast_sample(source, source, target, Gamut::Srgb, false, false).is_none());
    }
}

#[test]
fn srgb_wheel_matches_displayed_reference_swatches() {
    let hex = |c| {
        okcolor_core::format_color(c, okcolor_core::ColorFormat::Hex, M)
            .unwrap()
            .text
    };
    for (source, analogous, shade, tint, tone, ends) in [
        (
            "#4e83a3",
            ["#4ea28a", "#4e83a3", "#524ea2"],
            "#274151",
            "#a4c1d3",
            "#637d8d",
            ["#4e69a3", "#796979"],
        ),
        (
            "#d03264",
            ["#d133ce", "#d03264", "#d16a33"],
            "#6a1832",
            "#e899b2",
            "#a95b74",
            ["#81329b", "#d1324b"],
        ),
    ] {
        let source = color(source);
        assert_eq!(
            harmony(source, Harmony::Analogous, Gamut::Srgb)
                .into_iter()
                .map(hex)
                .collect::<Vec<_>>(),
            analogous
        );
        for (kind, expected) in [
            (Ramp::Shades, shade),
            (Ramp::Tints, tint),
            (Ramp::Tones, tone),
        ] {
            assert_eq!(
                hex(okcolor_core::ramp_steps::<15>(source, kind, Gamut::Srgb)[7]),
                expected
            );
        }
        let temp = okcolor_core::ramp_steps::<15>(source, Ramp::Temperatures, Gamut::Srgb);
        assert_eq!([hex(temp[0]), hex(temp[14])], ends);
    }
}
