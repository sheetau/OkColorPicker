use crate::{ColorError, Gamut, GamutMapping, Oklch};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorFormat {
    Hex,
    Hex8,
    Rgb,
    Hsl,
    Oklab,
    Oklch,
    Lab,
    Lch,
    Color(Gamut),
}

#[derive(Clone, Debug, PartialEq)]
pub struct FormattedColor {
    pub text: String,
    /// True when a bounded sRGB output needed gamut mapping or clipping.
    pub mapped: bool,
}

pub fn format_color(
    color: Oklch,
    format: ColorFormat,
    method: GamutMapping,
) -> Result<FormattedColor, ColorError> {
    let [l, c, h, alpha] = color.components();
    let suffix = if alpha < 1.0 {
        format!(
            " / {}",
            decimal_places(alpha, if format == ColorFormat::Oklch { 4 } else { 6 })
        )
    } else {
        String::new()
    };
    let mut mapped = false;
    let text = match format {
        ColorFormat::Oklch => format!(
            "oklch({} {} {}{suffix})",
            decimal_places(l, 4),
            decimal_places(c, 4),
            decimal_places(h, 4)
        ),
        ColorFormat::Oklab => functional("oklab", color.oklab(), &suffix)?,
        ColorFormat::Lab => functional("lab", color.lab_d50(), &suffix)?,
        ColorFormat::Lch => {
            let [l, a, b] = color.lab_d50();
            functional(
                "lch",
                [l, a.hypot(b), b.atan2(a).to_degrees().rem_euclid(360.0)],
                &suffix,
            )?
        }
        ColorFormat::Color(gamut) => {
            let space = match gamut {
                Gamut::Srgb => "srgb",
                Gamut::DisplayP3 => "display-p3",
                Gamut::Rec2020 => "rec2020",
            };
            let values = color.encoded_rgb(gamut);
            ensure_finite(values)?;
            format!(
                "color({space} {} {} {}{suffix})",
                decimal(values[0]),
                decimal(values[1]),
                decimal(values[2])
            )
        }
        ColorFormat::Hex | ColorFormat::Hex8 | ColorFormat::Rgb | ColorFormat::Hsl => {
            let [r, g, b] = color.mapped_rgb(Gamut::Srgb, method)?;
            mapped = !color.in_gamut(Gamut::Srgb);
            match format {
                ColorFormat::Hex | ColorFormat::Hex8 => {
                    let [r, g, b, a] = [r, g, b, alpha].map(|v| (v * 255.0).round() as u8);
                    if format == ColorFormat::Hex8 || alpha < 1.0 {
                        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
                    } else {
                        format!("#{r:02x}{g:02x}{b:02x}")
                    }
                }
                ColorFormat::Rgb => functional("rgb", [r, g, b].map(|v| v * 255.0), &suffix)?,
                ColorFormat::Hsl => {
                    let [hue, saturation, light] = crate::color::rgb_to_hsl([r, g, b]);
                    format!(
                        "hsl({} {}% {}%{suffix})",
                        decimal(hue),
                        decimal(saturation * 100.0),
                        decimal(light * 100.0)
                    )
                }
                _ => unreachable!("bounded output formats only"),
            }
        }
    };
    Ok(FormattedColor { text, mapped })
}

fn functional(name: &str, values: [f64; 3], suffix: &str) -> Result<String, ColorError> {
    ensure_finite(values)?;
    Ok(format!(
        "{name}({} {} {}{suffix})",
        decimal(values[0]),
        decimal(values[1]),
        decimal(values[2])
    ))
}
fn ensure_finite(values: [f64; 3]) -> Result<(), ColorError> {
    if values.into_iter().all(f64::is_finite) {
        Ok(())
    } else {
        Err(ColorError)
    }
}
fn decimal(value: f64) -> String {
    decimal_places(value, 6)
}
fn decimal_places(value: f64, precision: usize) -> String {
    let value = if value.abs() < 0.5 * 10.0_f64.powi(-(precision as i32)) {
        0.0
    } else {
        value
    };
    format!("{value:.precision$}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}
