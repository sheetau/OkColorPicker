use okcolor_core::{ColorFormat, Gamut, GamutMapping, Oklch, format_color, parse_color};

#[test]
fn local_minde_matches_independent_colorjs_0_7_1_reference_values() {
    // Generated with Color.js toGamut({space: "srgb", method: "css"}).
    for (input, expected) in [
        (
            "color(display-p3 1 0 0)",
            [1.0, 0.044569516204189065, 0.0459316111401759],
        ),
        ("oklch(.7 .4 145)", [0.0, 0.7660835180636797, 0.0]),
        (
            "oklch(.5 .4 270)",
            [0.20578268663556365, 0.14627975801736723, 1.0],
        ),
        ("oklch(.9 .25 95)", [1.0, 0.855340217677823, 0.0]),
    ] {
        let actual = parse_color(input, 0.0)
            .unwrap()
            .mapped_rgb(Gamut::Srgb, GamutMapping::LocalMinde)
            .unwrap();
        for (a, b) in actual.into_iter().zip(expected) {
            assert!((a - b).abs() < 2e-6, "{input}: {a} != {b}");
        }
    }
}

#[test]
fn all_output_formats_parse_back_with_only_documented_output_rounding() {
    let original = parse_color("#4e83a3bb", 0.0).unwrap();
    for format in [
        ColorFormat::Hex,
        ColorFormat::Hex8,
        ColorFormat::Rgb,
        ColorFormat::Hsl,
        ColorFormat::Lab,
        ColorFormat::Lch,
        ColorFormat::Oklab,
        ColorFormat::Oklch,
        ColorFormat::Color(Gamut::Srgb),
        ColorFormat::Color(Gamut::DisplayP3),
        ColorFormat::Color(Gamut::Rec2020),
    ] {
        let output = format_color(original, format, GamutMapping::LocalMinde).unwrap();
        assert!(!output.mapped);
        let parsed = parse_color(&output.text, 0.0).expect(&output.text);
        let tolerance = if format == ColorFormat::Oklch {
            0.000051
        } else {
            2e-6
        };
        for (a, b) in original.oklab().into_iter().zip(parsed.oklab()) {
            assert!((a - b).abs() < tolerance, "{}: {a} != {b}", output.text);
        }
        assert!((original.components()[3] - parsed.components()[3]).abs() < tolerance);
    }
}

#[test]
fn bounded_outputs_are_marked_but_original_and_wide_outputs_are_retained() {
    let original = parse_color("color(display-p3 1 0 0 / 0.5)", 0.0).unwrap();
    let before = original.components();
    let mapped = format_color(original, ColorFormat::Hex, GamutMapping::LocalMinde).unwrap();
    let clipped = format_color(original, ColorFormat::Hex, GamutMapping::Clip).unwrap();
    assert!(mapped.mapped && clipped.mapped);
    assert_eq!(clipped.text, "#ff000080");
    assert_ne!(mapped.text, clipped.text);
    let p3 = format_color(
        original,
        ColorFormat::Color(Gamut::DisplayP3),
        GamutMapping::Clip,
    )
    .unwrap();
    assert_eq!(p3.text, "color(display-p3 1 0 0 / 0.5)");
    assert!(!p3.mapped);
    assert_eq!(before, original.components());
}

#[test]
fn mapping_is_finite_at_extremes_and_has_identity_inside_each_gamut() {
    for gamut in [Gamut::Srgb, Gamut::DisplayP3, Gamut::Rec2020] {
        for l in [0.0, 0.001, 0.5, 0.999, 1.0] {
            for c in [0.0, 0.3, 1.0, 1e300] {
                for h in (0..360).step_by(15) {
                    let color = Oklch::new(l, c, f64::from(h), 0.3).unwrap();
                    let rgb = color.mapped_rgb(gamut, GamutMapping::LocalMinde).unwrap();
                    assert!(
                        rgb.into_iter()
                            .all(|v| v.is_finite() && (0.0..=1.0).contains(&v))
                    );
                    assert_eq!(color.components()[3], 0.3);
                }
            }
        }
        let rgb = [0.23, 0.45, 0.67];
        let color = Oklch::from_rgb(rgb, gamut, 1.0, 0.0).unwrap();
        for (a, b) in color
            .mapped_rgb(gamut, GamutMapping::LocalMinde)
            .unwrap()
            .into_iter()
            .zip(rgb)
        {
            assert!((a - b).abs() < 1e-10);
        }
    }
    let huge = Oklch::new(0.5, 1e300, 0.0, 1.0).unwrap();
    assert!(huge.clipped_rgb(Gamut::Srgb).is_err());
    assert!(format_color(huge, ColorFormat::Lab, GamutMapping::Clip).is_err());
}

#[test]
fn oklch_text_rounds_four_places_without_changing_source_values() {
    let original = Oklch::new(0.512345678, 0.123456789, 123.456789, 0.567891).unwrap();
    assert_eq!(
        format_color(original, ColorFormat::Oklch, GamutMapping::Clip)
            .unwrap()
            .text,
        "oklch(0.5123 0.1235 123.4568 / 0.5679)"
    );
    assert_eq!(
        original.components(),
        [0.512345678, 0.123456789, 123.456789, 0.567891]
    );
}
