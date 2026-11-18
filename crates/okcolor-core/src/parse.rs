//! Absolute CSS color input. Tokenization is CSS-aware; numbers retain f64 precision.
use crate::{Gamut, Oklch};
use cssparser::{Parser, ParserInput, Token};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseColorError(pub &'static str);
impl fmt::Display for ParseColorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for ParseColorError {}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Part {
    Number(f64),
    Percent(f64),
    Angle(f64),
    Comma,
    Slash,
}

impl Part {
    fn scaled(self, percent_scale: f64) -> Result<f64, ParseColorError> {
        match self {
            Self::Number(v) => Ok(v),
            Self::Percent(v) => Ok(v * percent_scale / 100.0),
            _ => Err(ParseColorError("Expected a number or percentage")),
        }
    }
    fn hue(self) -> Result<f64, ParseColorError> {
        match self {
            Self::Number(v) | Self::Angle(v) => Ok(v.rem_euclid(360.0)),
            _ => Err(ParseColorError("Expected a hue angle")),
        }
    }
}

/// Parses one complete supported absolute color. It never changes an editor on error.
/// `neutral_hue` retains editing intent when the input has no chromatic hue.
pub fn parse_color(text: &str, neutral_hue: f64) -> Result<Oklch, ParseColorError> {
    if text.len() > 4096 {
        return Err(ParseColorError("Color input is too long"));
    }
    // Bare editor input uses Illustrator-style RGB completion. Hash-prefixed
    // values below retain CSS shorthand alpha semantics.
    let bare = text.trim();
    if bare.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        let expanded;
        let hex = match bare.len() {
            1 | 2 => {
                expanded = bare.repeat(6 / bare.len());
                &expanded
            }
            3 | 6 | 8 => bare,
            4 | 5 | 7 => {
                let width = if bare.len() == 7 { 8 } else { 6 };
                expanded = format!("{bare:0>width$}");
                &expanded
            }
            _ => return Err(ParseColorError("Enter one to eight hexadecimal digits")),
        };
        return parse_hex(hex, neutral_hue);
    }
    let mut input = ParserInput::new(text);
    let mut parser = Parser::new(&mut input);
    let token = parser
        .next()
        .map_err(|_| ParseColorError("Expected a color"))?
        .clone();
    let result = match token {
        Token::Hash(hex) | Token::IDHash(hex) => parse_hex(&hex, neutral_hue),
        Token::Function(name) => {
            // CSS tolerates an omitted final ')'; an editable field must be complete.
            if !text.trim_end().ends_with(')') {
                return Err(ParseColorError("Close the color function with ')'"));
            }
            parser
                .parse_nested_block(|input| {
                    parse_function(&name.to_ascii_lowercase(), input, neutral_hue)
                        .map_err(|e| input.new_custom_error::<_, ParseColorError>(e))
                })
                .map_err(|e| match e.kind {
                    cssparser::ParseErrorKind::Custom(e) => e,
                    _ => ParseColorError("Invalid color syntax"),
                })
        }
        _ => Err(ParseColorError(
            "Use HEX or an absolute RGB, HSL, Lab, LCH, OKLab, OKLCH or color() value",
        )),
    }?;
    parser
        .expect_exhausted()
        .map_err(|_| ParseColorError("Unexpected text after the color"))?;
    Ok(result)
}

fn parse_hex(hex: &str, hue: f64) -> Result<Oklch, ParseColorError> {
    if ![3, 4, 6, 8].contains(&hex.len()) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(ParseColorError(
            "HEX requires 3, 4, 6 or 8 hexadecimal digits",
        ));
    }
    let digits = if hex.len() <= 4 {
        hex.chars().flat_map(|c| [c, c]).collect::<String>()
    } else {
        hex.to_owned()
    };
    let mut rgba = [1.0; 4];
    for (i, pair) in digits.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let pair = std::str::from_utf8(pair).expect("ASCII hex checked above");
        rgba[i] = f64::from(u8::from_str_radix(pair, 16).expect("hex checked above")) / 255.0;
    }
    Oklch::from_rgb([rgba[0], rgba[1], rgba[2]], Gamut::Srgb, rgba[3], hue)
        .map_err(|_| ParseColorError("Color is outside the editor's lightness range"))
}

fn parse_function(
    name: &str,
    input: &mut Parser<'_, '_>,
    neutral: f64,
) -> Result<Oklch, ParseColorError> {
    let gamut = if name == "color" {
        match input
            .expect_ident()
            .map_err(|_| ParseColorError("Expected an RGB color space"))?
            .to_ascii_lowercase()
            .as_str()
        {
            "srgb" => Gamut::Srgb,
            "display-p3" => Gamut::DisplayP3,
            "rec2020" => Gamut::Rec2020,
            _ => {
                return Err(ParseColorError(
                    "Supported color() spaces: srgb, display-p3, rec2020",
                ));
            }
        }
    } else {
        Gamut::Srgb
    };
    let parts = read_parts(input)?;
    let legacy = parts.contains(&Part::Comma);
    let (channels, alpha) = components(&parts, legacy)?;
    if legacy && !matches!(name, "rgb" | "rgba" | "hsl" | "hsla") {
        return Err(ParseColorError(
            "This color function uses spaces, not commas",
        ));
    }
    let [a, b, c] = channels;
    let color = match name {
        "rgb" | "rgba" => {
            if legacy {
                let percentages = channels
                    .iter()
                    .filter(|p| matches!(p, Part::Percent(_)))
                    .count();
                if percentages != 0 && percentages != 3 {
                    return Err(ParseColorError(
                        "Legacy RGB cannot mix numbers and percentages",
                    ));
                }
            }
            let rgb = [a.scaled(255.0)?, b.scaled(255.0)?, c.scaled(255.0)?]
                .map(|v| v.clamp(0.0, 255.0) / 255.0);
            Oklch::from_rgb(rgb, Gamut::Srgb, alpha, neutral)
        }
        "hsl" | "hsla" => {
            if legacy && (!matches!(b, Part::Percent(_)) || !matches!(c, Part::Percent(_))) {
                return Err(ParseColorError(
                    "Legacy HSL saturation and lightness require percentages",
                ));
            }
            let h = a.hue()?;
            let s = b.scaled(100.0)?.clamp(0.0, 100.0) / 100.0;
            let l = c.scaled(100.0)?.clamp(0.0, 100.0) / 100.0;
            Oklch::from_rgb(
                crate::color::hsl_to_rgb([h, s, l]),
                Gamut::Srgb,
                alpha,
                neutral,
            )
        }
        "oklch" => Oklch::new(
            a.scaled(1.0)?.clamp(0.0, 1.0),
            b.scaled(0.4)?.max(0.0),
            c.hue()?,
            alpha,
        ),
        "oklab" => Oklch::from_oklab(
            [
                a.scaled(1.0)?.clamp(0.0, 1.0),
                b.scaled(0.4)?,
                c.scaled(0.4)?,
            ],
            alpha,
            neutral,
        ),
        "lab" => Oklch::from_lab_d50(
            [
                a.scaled(100.0)?.clamp(0.0, 100.0),
                b.scaled(125.0)?,
                c.scaled(125.0)?,
            ],
            alpha,
            neutral,
        ),
        "lch" => {
            let chroma = b.scaled(150.0)?.max(0.0);
            let radians = c.hue()?.to_radians();
            Oklch::from_lab_d50(
                [
                    a.scaled(100.0)?.clamp(0.0, 100.0),
                    chroma * radians.cos(),
                    chroma * radians.sin(),
                ],
                alpha,
                neutral,
            )
        }
        "color" => Oklch::from_rgb(
            [a.scaled(1.0)?, b.scaled(1.0)?, c.scaled(1.0)?],
            gamut,
            alpha,
            neutral,
        ),
        _ => return Err(ParseColorError("Unsupported color function")),
    };
    color.map_err(|_| {
        ParseColorError("Color must be finite and its OKLCH lightness must be in 0..1")
    })
}

fn components(parts: &[Part], legacy: bool) -> Result<([Part; 3], f64), ParseColorError> {
    let (channels, alpha) = match (legacy, parts) {
        (false, [a, b, c]) => ([*a, *b, *c], Part::Number(1.0)),
        (false, [a, b, c, Part::Slash, alpha]) => ([*a, *b, *c], *alpha),
        (true, [a, Part::Comma, b, Part::Comma, c]) => ([*a, *b, *c], Part::Number(1.0)),
        (true, [a, Part::Comma, b, Part::Comma, c, Part::Comma, alpha]) => ([*a, *b, *c], *alpha),
        _ => {
            return Err(ParseColorError(
                "Expected three color components and optional alpha",
            ));
        }
    };
    Ok((channels, alpha.scaled(1.0)?.clamp(0.0, 1.0)))
}

fn read_parts(input: &mut Parser<'_, '_>) -> Result<Vec<Part>, ParseColorError> {
    let mut parts = Vec::new();
    while !input.is_exhausted() {
        input.skip_whitespace();
        let start = input.position();
        let token = input
            .next()
            .map_err(|_| ParseColorError("Invalid component"))?
            .clone();
        let raw = input.slice_from(start);
        // Token numeric values are f32. Reparse the original spelling to avoid early rounding.
        let part = match token {
            Token::Number { .. } => Part::Number(number(raw)?),
            Token::Percentage { .. } => Part::Percent(number(&raw[..raw.len() - 1])?),
            Token::Dimension { unit, .. } => {
                if !raw.is_ascii() || raw.contains('\\') {
                    return Err(ParseColorError("Use a literal hue angle unit"));
                }
                let value = number(&raw[..raw.len() - unit.len()])?;
                let degrees = match unit.to_ascii_lowercase().as_str() {
                    "deg" => value,
                    "grad" => value * 0.9,
                    "rad" => value.to_degrees(),
                    "turn" => value * 360.0,
                    _ => return Err(ParseColorError("Hue units: deg, grad, rad, turn")),
                };
                if !degrees.is_finite() {
                    return Err(ParseColorError("Hue angle is too large"));
                }
                Part::Angle(degrees)
            }
            Token::Comma => Part::Comma,
            Token::Delim('/') => Part::Slash,
            _ => {
                return Err(ParseColorError(
                    "Missing components, expressions and contextual colors are unsupported",
                ));
            }
        };
        parts.push(part);
        if parts.len() > 7 {
            return Err(ParseColorError("Too many color components"));
        }
    }
    Ok(parts)
}

fn number(raw: &str) -> Result<f64, ParseColorError> {
    raw.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or(ParseColorError("Expected a finite number"))
}
