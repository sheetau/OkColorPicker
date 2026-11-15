//! Related colors in OKLCH. Generated samples are reduced into the working gamut.
use crate::{Gamut, Oklch};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ramp {
    Shades,
    Tints,
    Tones,
}

pub fn ramp(source: Oklch, kind: Ramp, gamut: Gamut) -> [Oklch; 7] {
    let [l, c, h, a] = source.components();
    std::array::from_fn(|i| {
        let t = i as f64 / 6.0;
        let light = match kind {
            Ramp::Shades => l * (1.0 - t),
            Ramp::Tints => l + (1.0 - l) * t,
            Ramp::Tones => l,
        };
        Oklch::new(light, c * (1.0 - t), h, a)
            .expect("bounded interpolation")
            .reduce_chroma(gamut)
    })
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
