use crate::matrix::*;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gamut {
    Srgb,
    DisplayP3,
    Rec2020,
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
            "Color components must be finite; L/alpha must be in 0..=1 and chroma nonnegative",
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
        let [mut l, a, b] = multiply(LMS_TO_LAB, multiply(XYZ_TO_LMS, xyz).map(f64::cbrt));
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

    pub fn clipped_rgb(self, gamut: Gamut) -> [f64; 3] {
        self.encoded_rgb(gamut).map(|v| v.clamp(0.0, 1.0))
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
            if a < 0.0812428582986315 {
                a / 4.5
            } else {
                ((a + 0.09929682680944) / 1.09929682680944).powf(1.0 / 0.45)
            }
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
            if a < 0.018053968510807 {
                a * 4.5
            } else {
                1.09929682680944 * a.powf(0.45) - 0.09929682680944
            }
        } else if a <= 0.0031308 {
            a * 12.92
        } else {
            1.055 * a.powf(1.0 / 2.4) - 0.055
        };
        encoded.copysign(v)
    }
}
