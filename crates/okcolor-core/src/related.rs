//! Related colors from the sRGB color wheel or the selected wide OKLCH gamut.
use crate::{
    Gamut, GamutMapping, Oklch,
    color::{hsl_to_rgb, rgb_to_hsl},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ramp {
    Shades,
    Tints,
    Tones,
    Temperatures,
    Similar,
}

pub fn ramp(source: Oklch, kind: Ramp, gamut: Gamut) -> [Oklch; 7] {
    ramp_steps(source, kind, gamut)
}

// The sRGB reference wheel starts from whole-degree HSL and whole percentages.
fn wheel_hsl(rgb: [f64; 3]) -> [f64; 3] {
    let [h, s, l] = rgb_to_hsl(rgb);
    [
        h.round(),
        (100.0 * s).round() / 100.0,
        (100.0 * l).round() / 100.0,
    ]
}

fn byte_rgb(rgb: [f64; 3]) -> [f64; 3] {
    rgb.map(|v| (v * 255.0).round() / 255.0)
}

// The reference wheel rounds HSV percentages before converting back to sRGB.
fn wheel_extrema([r, g, b]: [f64; 3], quantize: bool) -> (f64, f64) {
    let high = r.max(g).max(b);
    let low = r.min(g).min(b);
    if !quantize || high == 0.0 {
        return (low, high);
    }
    let value = (high * 100.0).round() / 100.0;
    let saturation = ((1.0 - low / high) * 100.0).round() / 100.0;
    (value * (1.0 - saturation), value)
}

/// Signed picker steps: lightness hundredths, chroma thousandths, hue degrees,
/// and a percentage mix toward the warm or cool RGB endpoint.
pub fn adjust_palette_color(source: Oklch, gamut: Gamut, offsets: [i16; 4]) -> Oklch {
    let [lightness, chroma, hue, temperature] = offsets;
    let source = if lightness == 0 && chroma == 0 && hue % 360 == 0 {
        source
    } else {
        let [l, c, h, a] = source.components();
        Oklch::new(
            (l + f64::from(lightness) / 100.0).clamp(0.0, 1.0),
            (c + f64::from(chroma) / 1000.0).max(0.0),
            h + f64::from(hue),
            a,
        )
        .expect("bounded palette adjustment")
        .reduce_chroma(gamut)
    };
    if temperature == 0 {
        return source;
    }
    let Ok(rgb) = source.mapped_rgb(gamut, GamutMapping::LocalMinde) else {
        return source;
    };
    let bounded = gamut == Gamut::Srgb;
    let encode = |rgb| if bounded { byte_rgb(rgb) } else { rgb };
    let rgb = encode(rgb);
    let (low, high) = wheel_extrema(rgb, bounded);
    let target = encode(if temperature < 0 {
        [low, low, high]
    } else {
        [high, low, low]
    });
    let mix = f64::from(temperature.clamp(-100, 100)).abs() / 100.0;
    let rgb = encode(std::array::from_fn(|i| rgb[i] + (target[i] - rgb[i]) * mix));
    let [_, _, h, alpha] = source.components();
    Oklch::from_rgb(rgb, gamut, alpha, h).expect("bounded temperature adjustment")
}

fn srgb_ramp<const N: usize>(source: Oklch, kind: Ramp, rgb: [f64; 3]) -> [Oklch; N] {
    let rgb = byte_rgb(rgb);
    let [h, s, l] = wheel_hsl(rgb);
    let [_, _, neutral, alpha] = source.components();
    let (low, value) = wheel_extrema(rgb, true);
    std::array::from_fn(|i| {
        let t = i as f64 / N.saturating_sub(1).max(1) as f64;
        if (kind == Ramp::Temperatures && t == 0.5) || (kind != Ramp::Temperatures && i == 0) {
            return source;
        }
        let result = match kind {
            Ramp::Shades => hsl_to_rgb([h, s, l * (1.0 - t)]),
            Ramp::Tints => hsl_to_rgb([h, s, l + (1.0 - l) * t]),
            Ramp::Tones => hsl_to_rgb([h, s * (1.0 - t), l]),
            Ramp::Temperatures => {
                let end = if t < 0.5 {
                    [low, low, value]
                } else {
                    [value, low, low]
                };
                std::array::from_fn(|k| rgb[k] + (end[k] - rgb[k]) * (t - 0.5).abs())
            }
            Ramp::Similar => unreachable!("similar samples use named colors"),
        };
        Oklch::from_rgb(byte_rgb(result), Gamut::Srgb, alpha, neutral).expect("bounded sRGB sample")
    })
}

pub fn ramp_steps<const N: usize>(source: Oklch, kind: Ramp, gamut: Gamut) -> [Oklch; N] {
    if gamut == Gamut::Srgb
        && kind != Ramp::Similar
        && source.in_gamut(gamut)
        && let Ok(rgb) = source.mapped_rgb(gamut, GamutMapping::Clip)
    {
        return srgb_ramp(source, kind, rgb);
    }
    let [l, c, h, a] = source.components();
    std::array::from_fn(|i| {
        let t = i as f64 / N.saturating_sub(1).max(1) as f64;
        let light = match kind {
            Ramp::Shades => l * (1.0 - t),
            Ramp::Tints => l + (1.0 - l) * t,
            Ramp::Tones | Ramp::Temperatures | Ramp::Similar => l,
        };
        let (chroma, hue) = match kind {
            Ramp::Temperatures => {
                let anchor = if t < 0.5 { 250.0 } else { 65.0 };
                let amount = (t - 0.5).abs();
                let delta = (anchor - h + 180.0).rem_euclid(360.0) - 180.0;
                (c + (0.08 - c) * amount, h + delta * amount)
            }
            Ramp::Similar => (c, h + (t - 0.5) * 36.0),
            _ => (c * (1.0 - t), h),
        };
        Oklch::new(light, chroma, hue, a)
            .expect("bounded interpolation")
            .reduce_chroma(gamut)
    })
}

/// Thirteen samples with the exact source in the middle of each variation.
pub fn variations(source: Oklch, row: Variation, gamut: Gamut) -> [Oklch; 13] {
    let [l, c, h, a] = source.components();
    match row {
        Variation::Lightness => {
            let shades = ramp_steps::<7>(source, Ramp::Shades, gamut);
            let tints = ramp_steps::<7>(source, Ramp::Tints, gamut);
            std::array::from_fn(|i| if i <= 6 { shades[6 - i] } else { tints[i - 6] })
        }
        Variation::Temperature => {
            let mut colors = ramp_steps(source, Ramp::Temperatures, gamut);
            colors[6] = source;
            colors
        }
        Variation::Chroma | Variation::Hue => {
            let max = Oklch::new(l, 1.0, h, a)
                .expect("bounded lightness")
                .reduce_chroma(gamut)
                .components()[1];
            std::array::from_fn(|i| {
                if i == 6 {
                    return source;
                }
                let t = (i as f64 - 6.0) / 6.0;
                let (c, h) = if row == Variation::Hue {
                    (c, h + t * 60.0)
                } else {
                    (c + t * if t < 0.0 { c } else { (max - c).max(0.0) }, h)
                };
                Oklch::new(l, c, h, a)
                    .expect("bounded variation")
                    .reduce_chroma(gamut)
            })
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variation {
    Lightness,
    Chroma,
    Hue,
    Temperature,
}

/// Four nearest distinct named samples, in perceptual distance order.
pub fn similar_named(color: Oklch) -> Vec<Oklch> {
    crate::nearest_named(color, 4)
        .into_iter()
        .map(|entry| entry.color)
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Harmony {
    Complementary,
    Analogous,
    SplitComplementary,
    Triadic,
    Tetradic,
    Square,
}
impl Harmony {
    pub const ALL: [Self; 6] = [
        Self::Analogous,
        Self::Complementary,
        Self::SplitComplementary,
        Self::Triadic,
        Self::Tetradic,
        Self::Square,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Complementary => "Complementary",
            Self::Analogous => "Analogous",
            Self::SplitComplementary => "Split complementary",
            Self::Triadic => "Triadic",
            Self::Tetradic => "Tetradic",
            Self::Square => "Square",
        }
    }
    fn offsets(self) -> &'static [f64] {
        match self {
            Self::Complementary => &[0.0, 180.0],
            Self::Analogous => &[-30.0, 0.0, 30.0],
            Self::SplitComplementary => &[0.0, 150.0, 210.0],
            Self::Triadic => &[0.0, 120.0, 240.0],
            Self::Tetradic => &[0.0, 60.0, 180.0, 240.0],
            Self::Square => &[0.0, 90.0, 180.0, 270.0],
        }
    }
}
pub fn harmony(source: Oklch, kind: Harmony, gamut: Gamut) -> Vec<Oklch> {
    let [l, c, h, a] = source.components();
    if gamut == Gamut::Srgb
        && source.in_gamut(gamut)
        && let Ok(rgb) = source.mapped_rgb(gamut, GamutMapping::Clip)
    {
        let [angle, s, light] = wheel_hsl(byte_rgb(rgb));
        let offsets = match kind {
            Harmony::Analogous => &[-40.0, 0.0, 40.0][..],
            Harmony::SplitComplementary => &[0.0, 160.0, 200.0][..],
            _ => kind.offsets(),
        };
        return offsets
            .iter()
            .map(|offset| {
                if *offset == 0.0 {
                    return source;
                }
                Oklch::from_rgb(
                    byte_rgb(hsl_to_rgb([angle + offset, s, light])),
                    gamut,
                    a,
                    h,
                )
                .expect("bounded color wheel")
            })
            .collect();
    }
    kind.offsets()
        .iter()
        .map(|offset| {
            Oklch::new(l, c, h + offset, a)
                .expect("finite hue offset")
                .reduce_chroma(gamut)
        })
        .collect()
}
