//! Related colors in OKLCH. Generated samples are reduced into the working gamut.
use crate::{Gamut, Oklch};

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

pub fn ramp_steps<const N: usize>(source: Oklch, kind: Ramp, gamut: Gamut) -> [Oklch; N] {
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

/// Nearest CSS named color by Euclidean distance in OKLab.
pub fn color_name(color: Oklch) -> &'static str {
    use std::sync::LazyLock;
    static NAMES: LazyLock<Vec<(&str, [f64; 3])>> = LazyLock::new(|| {
        cssparser::color::all_named_colors()
            .map(|(name, (r, g, b))| {
                let rgb = [r, g, b].map(|v| f64::from(v) / 255.0);
                (
                    name,
                    Oklch::from_rgb(rgb, Gamut::Srgb, 1.0, 0.0)
                        .expect("CSS named color")
                        .oklab(),
                )
            })
            .collect()
    });
    let lab = color.oklab();
    let distance = |other: [f64; 3]| {
        lab.into_iter()
            .zip(other)
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f64>()
    };
    NAMES
        .iter()
        .min_by(|a, b| distance(a.1).total_cmp(&distance(b.1)).then(a.0.cmp(b.0)))
        .expect("CSS color table")
        .0
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Harmony {
    Complementary,
    Analogous,
    SplitComplementary,
    Triadic,
    Tetradic,
}
impl Harmony {
    pub const ALL: [Self; 5] = [
        Self::Complementary,
        Self::Analogous,
        Self::SplitComplementary,
        Self::Triadic,
        Self::Tetradic,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Complementary => "Complementary",
            Self::Analogous => "Analogous",
            Self::SplitComplementary => "Split complementary",
            Self::Triadic => "Triadic",
            Self::Tetradic => "Tetradic",
        }
    }
    fn offsets(self) -> &'static [f64] {
        match self {
            Self::Complementary => &[0.0, 180.0],
            Self::Analogous => &[-30.0, 0.0, 30.0],
            Self::SplitComplementary => &[0.0, 150.0, 210.0],
            Self::Triadic => &[0.0, 120.0, 240.0],
            Self::Tetradic => &[0.0, 90.0, 180.0, 270.0],
        }
    }
}
pub fn harmony(source: Oklch, kind: Harmony, gamut: Gamut) -> Vec<Oklch> {
    let [l, c, h, a] = source.components();
    kind.offsets()
        .iter()
        .map(|offset| {
            Oklch::new(l, c, h + offset, a)
                .expect("finite hue offset")
                .reduce_chroma(gamut)
        })
        .collect()
}
