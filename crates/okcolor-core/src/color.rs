use crate::matrix::*;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gamut {
    Srgb,
    DisplayP3,
    Rec2020,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GamutMapping {
    Clip,
    LocalMinde,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Oklch {
    l: f64,
    c: f64,
    h: f64,
    alpha: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColorError;

impl fmt::Display for ColorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            "Invalid or unrepresentable color: use finite components, L/alpha in 0..=1 and nonnegative chroma",
        )
    }
}
impl std::error::Error for ColorError {}

impl Oklch {
    /// Validates an editor color without imposing an arbitrary upper chroma limit.
    pub fn new(l: f64, c: f64, h: f64, alpha: f64) -> Result<Self, ColorError> {
        if ![l, c, h, alpha].into_iter().all(f64::is_finite)
            || !(0.0..=1.0).contains(&l)
            || c < 0.0
            || !(0.0..=1.0).contains(&alpha)
        {
            return Err(ColorError);
        }
        Ok(Self {
            l,
            c,
            h: h.rem_euclid(360.0),
            alpha,
        })
    }

    pub fn components(self) -> [f64; 4] {
        [self.l, self.c, self.h, self.alpha]
    }

    pub fn oklab(self) -> [f64; 3] {
        let radians = self.h.to_radians();
        [self.l, self.c * radians.cos(), self.c * radians.sin()]
    }

    /// Linear RGB retains out-of-gamut and negative channels.
    pub fn linear_rgb(self, gamut: Gamut) -> [f64; 3] {
        let lms = multiply(LAB_TO_LMS, self.oklab()).map(|v| v * v * v);
        multiply(gamut.xyz_to_rgb(), multiply(LMS_TO_XYZ, lms))
    }

    pub fn encoded_rgb(self, gamut: Gamut) -> [f64; 3] {
        self.linear_rgb(gamut).map(|v| gamut.encode(v))
    }

    /// Accepts finite extended RGB, provided its resulting L is in the editor range.
    /// Neutral colors keep the supplied editing hue instead of choosing a noisy angle.
    pub fn from_rgb(
        rgb: [f64; 3],
        gamut: Gamut,
        alpha: f64,
        neutral_hue: f64,
    ) -> Result<Self, ColorError> {
        if !rgb.into_iter().all(f64::is_finite) {
            return Err(ColorError);
        }
        let xyz = multiply(gamut.to_xyz(), rgb.map(|v| gamut.decode(v)));
        Self::from_xyz_d65(xyz, alpha, neutral_hue)
    }

    pub fn from_oklab(
        [mut l, a, b]: [f64; 3],
        alpha: f64,
        neutral_hue: f64,
    ) -> Result<Self, ColorError> {
        // Matrix roundoff at black/white must not reject valid RGB endpoints.
        if (-1e-12..=1.0 + 1e-12).contains(&l) {
            l = l.clamp(0.0, 1.0);
        }
        let c = a.hypot(b);
        if c < 1e-7 {
            Self::new(l, 0.0, neutral_hue, alpha)
        } else {
            Self::new(l, c, b.atan2(a).to_degrees(), alpha)
        }
    }

    pub fn from_lab_d50(
        [l, a, b]: [f64; 3],
        alpha: f64,
        neutral_hue: f64,
    ) -> Result<Self, ColorError> {
        const EPSILON: f64 = 216.0 / 24389.0;
        const KAPPA: f64 = 24389.0 / 27.0;
        let fy = (l + 16.0) / 116.0;
        let f = [fy + a / 500.0, fy, fy - b / 200.0];
        let xyz = f.map(|v| {
            if v.powi(3) > EPSILON {
                v.powi(3)
            } else {
                (116.0 * v - 16.0) / KAPPA
            }
        });
        let xyz = std::array::from_fn(|i| xyz[i] * D50_WHITE[i]);
        Self::from_xyz_d65(multiply(D50_TO_D65, xyz), alpha, neutral_hue)
    }

    pub fn lab_d50(self) -> [f64; 3] {
        let xyz = multiply(D65_TO_D50, self.xyz_d65());
        let scaled: [f64; 3] = std::array::from_fn(|i| xyz[i] / D50_WHITE[i]);
        let [x, y, z] = scaled.map(|v| {
            if v > 216.0 / 24389.0 {
                v.cbrt()
            } else {
                (24389.0 / 27.0 * v + 16.0) / 116.0
            }
        });
        [116.0 * y - 16.0, 500.0 * (x - y), 200.0 * (y - z)]
    }

    fn xyz_d65(self) -> [f64; 3] {
        multiply(
            LMS_TO_XYZ,
            multiply(LAB_TO_LMS, self.oklab()).map(|v| v.powi(3)),
        )
    }

    fn from_xyz_d65(xyz: [f64; 3], alpha: f64, neutral_hue: f64) -> Result<Self, ColorError> {
        Self::from_oklab(
            multiply(LMS_TO_LAB, multiply(XYZ_TO_LMS, xyz).map(f64::cbrt)),
            alpha,
            neutral_hue,
        )
    }

    /// Membership uses linear RGB, never a clipped or mapped preview.
    pub fn in_gamut(self, gamut: Gamut) -> bool {
        self.linear_rgb(gamut)
            .into_iter()
            .all(|v| v.is_finite() && (-1e-7..=1.0 + 1e-7).contains(&v))
    }

    /// Simple constant-L/H chroma reduction, distinct from CSS local-MINDE mapping.
    /// Returns a derived color; the editor's original color is unchanged.
    pub fn reduce_chroma(self, gamut: Gamut) -> Self {
        if self.in_gamut(gamut) {
            return self;
        }
        let mut mapped = Self { c: 0.0, ..self };
        if self.l == 0.0 || self.l == 1.0 {
            return mapped;
        }
        // 1 is above the maximum chroma of these bounded SDR RGB gamuts.
        let mut high = self.c.min(1.0);
        let mut low = 0.0;
        for _ in 0..40 {
            let middle = (low + high) * 0.5;
            if (Self { c: middle, ..self }).in_gamut(gamut) {
                low = middle;
            } else {
                high = middle;
            }
        }
        mapped.c = low;
        mapped
    }

    pub fn clipped_rgb(self, gamut: Gamut) -> Result<[f64; 3], ColorError> {
        let rgb = self.encoded_rgb(gamut);
        if !rgb.into_iter().all(f64::is_finite) {
            return Err(ColorError);
        }
        Ok(rgb.map(|v| v.clamp(0.0, 1.0)))
    }

    /// CSS Color 4, 2026-11-12, binary search with local MINDE (deltaEOK 0.02).
    /// Mapping returns destination RGB without changing the source editing coordinates.
    pub fn mapped_rgb(self, gamut: Gamut, method: GamutMapping) -> Result<[f64; 3], ColorError> {
        if method == GamutMapping::Clip {
            return self.clipped_rgb(gamut);
        }
        if self.l == 0.0 {
            return Ok([0.0; 3]);
        }
        if self.l == 1.0 {
            return Ok([1.0; 3]);
        }
        if self.in_gamut(gamut) {
            return self.clipped_rgb(gamut);
        }
        // Chroma above 1 is beyond these SDR gamuts by more than a JND.
        // Bound the search before cubing, so extreme finite input cannot overflow it.
        let mut current = Self {
            c: self.c.min(1.0),
            ..self
        };
        let mut clipped = current.clipped_rgb(gamut)?;
        let difference = |candidate: Self, rgb: [f64; 3]| -> Result<f64, ColorError> {
            let clipped = Self::from_rgb(rgb, gamut, candidate.alpha, candidate.h)?;
            Ok(candidate
                .oklab()
                .into_iter()
                .zip(clipped.oklab())
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f64>()
                .sqrt())
        };
        const JND: f64 = 0.02;
        const EPSILON: f64 = 0.0001;
        if difference(current, clipped)? < JND {
            return Ok(clipped);
        }
        let (mut low, mut high) = (0.0, current.c);
        let mut low_in_gamut = true;
        while high - low > EPSILON {
            current.c = (low + high) / 2.0;
            if low_in_gamut && current.in_gamut(gamut) {
                low = current.c;
                continue;
            }
            clipped = current.clipped_rgb(gamut)?;
            let delta = difference(current, clipped)?;
            if delta < JND {
                if JND - delta < EPSILON {
                    return Ok(clipped);
                }
                low_in_gamut = false;
                low = current.c;
            } else {
                high = current.c;
            }
        }
        Ok(clipped)
    }
}

impl Gamut {
    fn to_xyz(self) -> Matrix {
        match self {
            Self::Srgb => SRGB_TO_XYZ,
            Self::DisplayP3 => P3_TO_XYZ,
            Self::Rec2020 => REC2020_TO_XYZ,
        }
    }
    fn xyz_to_rgb(self) -> Matrix {
        match self {
            Self::Srgb => XYZ_TO_SRGB,
            Self::DisplayP3 => XYZ_TO_P3,
            Self::Rec2020 => XYZ_TO_REC2020,
        }
    }
    fn decode(self, v: f64) -> f64 {
        let a = v.abs();
        let linear = if self == Self::Rec2020 {
            // CSS Color 4 (2026-11-12) uses display-referred BT.1886 gamma.
            a.powf(2.4)
        } else if a <= 0.04045 {
            a / 12.92
        } else {
            ((a + 0.055) / 1.055).powf(2.4)
        };
        linear.copysign(v)
    }
    fn encode(self, v: f64) -> f64 {
        let a = v.abs();
        let encoded = if self == Self::Rec2020 {
            a.powf(1.0 / 2.4)
        } else if a <= 0.0031308 {
            a * 12.92
        } else {
            1.055 * a.powf(1.0 / 2.4) - 0.055
        };
        encoded.copysign(v)
    }
}

pub(crate) fn rgb_to_hsl([r, g, b]: [f64; 3]) -> [f64; 3] {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let light = (max + min) / 2.0;
    let (hue, saturation) = if delta < 1e-12 {
        (0.0, 0.0)
    } else {
        let hue = if max == r {
            (g - b) / delta
        } else if max == g {
            (b - r) / delta + 2.0
        } else {
            (r - g) / delta + 4.0
        };
        (
            (hue * 60.0).rem_euclid(360.0),
            delta / (1.0 - (2.0 * light - 1.0).abs()),
        )
    };
    [hue, saturation, light]
}

pub(crate) fn hsl_to_rgb([h, s, l]: [f64; 3]) -> [f64; 3] {
    let h = h.rem_euclid(360.0);
    let amplitude = s * l.min(1.0 - l);
    let channel = |offset: f64| {
        let k = (offset + h / 30.0) % 12.0;
        l - amplitude * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0)
    };
    [channel(0.0), channel(8.0), channel(4.0)]
}
