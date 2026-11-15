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
    /// Invalid or out-of-gamut samples cannot start a selection.
    pub fn pick(self, x: f64, y: f64, alpha: f64) -> Option<Oklch> {
        if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
            return None;
        }
        let color = Oklch::new(1.0 - y, x * self.max_chroma, self.hue, alpha).ok()?;
        color.in_gamut(self.gamut).then_some(color)
    }

    /// Cache boundary samples once per gesture, not on every pointer move.
    pub fn projection(self) -> GamutProjection {
        GamutProjection {
            slice: self,
            boundary: std::array::from_fn(|i| {
                self.boundary(i as f64 / 256.0, self.gamut)
                    .unwrap()
                    .min(self.max_chroma)
            }),
        }
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

/// Nearest-point projection in the chart's screen metric, including outside drags.
pub struct GamutProjection {
    slice: GamutSlice,
    boundary: [f64; 257],
}
impl GamutProjection {
    /// `aspect` is rendered width / height, so a pixel has equal weight in both axes.
    /// Search all sampled local minima, then refine against the actual RGB boundary.
    pub fn project(&self, x: f64, y: f64, alpha: f64, aspect: f64) -> Option<Oklch> {
        if !x.is_finite() || !y.is_finite() || !aspect.is_finite() || aspect <= 0.0 {
            return None;
        }
        if let Some(color) = self.slice.pick(x, y, alpha) {
            return Some(color);
        }
        let desired = x.clamp(0.0, 1.0) * self.slice.max_chroma;
        let distance =
            |l: f64, c: f64| ((c / self.slice.max_chroma - x) * aspect).hypot(1.0 - l - y);
        let candidate = |l: f64| {
            let c = desired.min(self.slice.boundary(l, self.slice.gamut).unwrap());
            (distance(l, c), l, c)
        };
        let distances: [f64; 257] =
            std::array::from_fn(|i| distance(i as f64 / 256.0, desired.min(self.boundary[i])));
        let mut best = candidate((1.0 - y).clamp(0.0, 1.0));
        for i in 0..=256 {
            let sampled = (
                distances[i],
                i as f64 / 256.0,
                desired.min(self.boundary[i]),
            );
            if sampled.0 < best.0 {
                best = sampled;
            }
            // The RGB membership tolerance makes tiny neighborhoods of black
            // and white sharper than the regular samples. Always refine those.
            if i > 1
                && i < 255
                && ((i > 0 && distances[i] > distances[i - 1])
                    || (i < 256 && distances[i] > distances[i + 1]))
            {
                continue;
            }
            let mut low = i.saturating_sub(1) as f64 / 256.0;
            let mut high = (i + 1).min(256) as f64 / 256.0;
            // Golden-section minimization also converges on a cusp or clipped extent.
            const RATIO: f64 = 0.6180339887498949;
            let mut a = low + (1.0 - RATIO) * (high - low);
            let mut b = low + RATIO * (high - low);
            let mut ca = candidate(a);
            let mut cb = candidate(b);
            for _ in 0..36 {
                if ca.0 < cb.0 {
                    high = b;
                    b = a;
                    cb = ca;
                    a = low + (1.0 - RATIO) * (high - low);
                    ca = candidate(a);
                } else {
                    low = a;
                    a = b;
                    ca = cb;
                    b = low + RATIO * (high - low);
                    cb = candidate(b);
                }
            }
            for refined in [ca, cb] {
                if refined.0 < best.0 {
                    best = refined;
                }
            }
        }
        Oklch::new(best.1, best.2, self.slice.hue, alpha).ok()
    }
}
