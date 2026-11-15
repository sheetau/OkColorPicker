use okcolor_core::{Gamut, Oklch, parse_color};

fn same_color(a: Oklch, b: Oklch, epsilon: f64) {
    for (a, b) in a.oklab().into_iter().zip(b.oklab()) {
        assert!((a - b).abs() < epsilon, "{a} != {b}");
    }
    assert!((a.components()[3] - b.components()[3]).abs() < epsilon);
}

#[test]
fn rgb_hex_hsl_and_hue_units_agree() {
    let expected = Oklch::from_rgb([1.0, 0.0, 0.0], Gamut::Srgb, 1.0, 0.0).unwrap();
    for input in [
        "#f00",
        "#ff0000",
        "#ff0000ff",
        "rgb(255, 0, 0)",
        "RGBA(100% 0% 0% / 100%)",
        "rgb(255 0% 0)",
        "hsl(0 100% 50%)",
        "hsla(1turn, 100%, 50%, 1)",
        "hsl(400grad 100% 50%)",
        "hsl(0rad 100% 50%)",
    ] {
        same_color(parse_color(input, 270.0).expect(input), expected, 1e-12);
    }
    assert_eq!(
        parse_color("#1238", 0.0).unwrap().components()[3],
        136.0 / 255.0
    );
}

#[test]
fn bare_rgb_and_rgba_hex_are_complete_editor_inputs() {
    for input in ["ff0000", "aBcDeF", "000000", "12345678", "ffffff00"] {
        assert_eq!(
            parse_color(input, 237.0),
            parse_color(&format!("#{input}"), 237.0)
        );
        assert_eq!(
            parse_color(&format!("  {input}\n"), 237.0),
            parse_color(input, 237.0)
        );
    }
    for input in [
        "f00",
        "1234",
        "12345",
        "1234567",
        "123456789",
        "ab cd ef",
        "gg0000",
        "0xff0000",
        "１２３４５６",
    ] {
        assert!(parse_color(input, 0.0).is_err(), "{input}");
    }
}

#[test]
fn absolute_color_input_keeps_precision_and_out_of_gamut_values() {
    let c = parse_color(
        "oklch(0.1234567890123456 0.2345678901234567 234.5678901234567 / 0.3456789012345678)",
        0.0,
    )
    .unwrap();
    assert_eq!(
        c.components(),
        [
            0.1234567890123456,
            0.2345678901234567,
            234.5678901234567,
            0.3456789012345678
        ]
    );
    let p3 = parse_color("color(display-p3 1 0 0 / 25%)", 0.0).unwrap();
    assert!(!p3.in_gamut(Gamut::Srgb));
    assert_eq!(p3.components()[3], 0.25);
    let outside = parse_color("oklch(60% 125% 0)", 0.0).unwrap();
    assert_eq!(outside.components()[1], 0.5);
    let neutral = parse_color("rgb(50% 50% 50%)", 275.0).unwrap();
    assert_eq!(neutral.components()[1..3], [0.0, 275.0]);
}

#[test]
fn lab_d50_white_and_cylindrical_inputs_round_trip() {
    // Independent Color.js 0.7.1 conversion of sRGB red to D50 Lab.
    let red = parse_color("#f00", 0.0).unwrap();
    for (a, b) in
        red.lab_d50()
            .into_iter()
            .zip([54.29054140467191, 80.80492817043522, 69.89096476862429])
    {
        assert!((a - b).abs() < 1e-8);
    }
    let white = parse_color("lab(100% 0 0)", 275.0).unwrap();
    assert!((white.components()[0] - 1.0).abs() < 1e-10);
    assert_eq!(white.components()[1..3], [0.0, 275.0]);
    let black = parse_color("lab(0 0 0)", 0.0).unwrap();
    assert!(black.components()[0].abs() < 1e-12);
    for input in [
        "lab(50 40 -30 / .7)",
        "lab(50% 32% -24% / 70%)",
        "lch(50 50 323.13010235415598 / .7)",
    ] {
        let color = parse_color(input, 0.0).unwrap();
        for (a, b) in color.lab_d50().into_iter().zip([50.0, 40.0, -30.0]) {
            assert!((a - b).abs() < 1e-8, "{input}: {a} != {b}");
        }
    }
    same_color(
        parse_color("oklab(50% 25% 0%)", 0.0).unwrap(),
        parse_color("oklch(.5 .1 0)", 0.0).unwrap(),
        1e-12,
    );
}

#[test]
fn malformed_incomplete_and_contextual_inputs_are_errors() {
    for input in [
        "",
        "#12",
        "#gg0000",
        "rgb(1 2)",
        "rgb(1 2 3",
        "rgb(1 2 3))",
        "rgb(1 2 3)junk",
        "rgb(1,2,3/.5)",
        "rgb(1%,2,3)",
        "oklch(0.5,.2,30)",
        "oklch(.5 -.2 none)",
        "oklch(.5 .2 NaN)",
        "oklch(1e999 .2 30)",
        "oklch(.5 .2 10px)",
        "var(--color)",
        "currentColor",
        "color(srgb 1 0)",
        "color(unknown 1 0 0)",
        "oklch(from red l c h)",
        "rgb(calc(1+2) 0 0)",
    ] {
        assert!(parse_color(input, 0.0).is_err(), "accepted {input}");
    }
}

#[test]
fn css_clamps_reference_components_without_clipping_color_function_channels() {
    assert_eq!(
        parse_color("oklch(120% -1 0 / -1)", 0.0)
            .unwrap()
            .components(),
        [1.0, 0.0, 0.0, 0.0]
    );
    same_color(
        parse_color("rgb(300 -10 0 / 200%)", 0.0).unwrap(),
        parse_color("#f00", 0.0).unwrap(),
        1e-12,
    );
    let color = parse_color("color(srgb 1.1 0 0)", 0.0).unwrap();
    assert!(!color.in_gamut(Gamut::Srgb));
    assert!((color.encoded_rgb(Gamut::Srgb)[0] - 1.1).abs() < 1e-10);
    assert!(parse_color("rgb(255 /* red */ 0 0)", 0.0).is_ok());
}
