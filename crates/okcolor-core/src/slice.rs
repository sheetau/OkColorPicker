//! H-fixed, absolute-chroma slice geometry. The display rectangle is not the gamut.
use crate::{ColorError, Gamut, Oklch};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GamutSlice {
    hue: f64,
    max_chroma: f64,
    gamut: Gamut,
}

impl GamutSlice {
    pub fn new(hue: f64, max_chroma: f64, gamut: Gamut) -> Result<Self, ColorError> {
        let hue = Oklch::new(0.5, 0.0, hue, 1.0)?.components()[2];
        if !max_chroma.is_finite() || max_chroma <= 0.0 {
            return Err(ColorError);
        }
        Ok(Self {
            hue,
            max_chroma,
            gamut,
        })
    }

    /// Coordinates are normalized: left C=0, right C=max, top L=1, bottom L=0.
    /// Invalid or out-of-gamut samples are nonselectable, including during drags.
    /// The caller keeps the last valid color while the pointer is outside.
    pub fn pick(self, x: f64, y: f64, alpha: f64) -> Option<Oklch> {
        if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
            return None;
        }
        let color = Oklch::new(1.0 - y, x * self.max_chroma, self.hue, alpha).ok()?;
        color.in_gamut(self.gamut).then_some(color)
    }

    /// Boundary for any of the three reference gamuts, independent of preview mapping.
    pub fn boundary(self, lightness: f64, gamut: Gamut) -> Result<f64, ColorError> {
        let color = Oklch::new(lightness, 1.0, self.hue, 1.0)?;
        Ok(color.reduce_chroma(gamut).components()[1])
    }

    /// Does not clamp: an external or numeric color may require an off-chart marker.
    pub fn marker(self, color: Oklch) -> [f64; 2] {
        let [l, c, _, _] = color.components();
        [c / self.max_chroma, 1.0 - l]
    }
}
