//! APCA 0.0.98G contrast values for sRGB and Display P3.
// Formula implementation adapted from Color.js (MIT), copyright 2021 Lea Verou,
// Chris Lilley. See licenses/Colorjs-MIT.txt and docs/CONTRAST.md for provenance.
use crate::{ColorError, Gamut, GamutMapping, Oklch};

/// Foreground alpha is composited in encoded sRGB over an opaque background.
/// Positive Lc means dark text on light; negative means light text on dark.
pub fn apca(text: Oklch, background: Oklch, mapping: GamutMapping) -> Result<f64, ColorError> {
    apca_in(text, background, mapping, Gamut::Srgb)
}

pub fn apca_in(
    text: Oklch,
    background: Oklch,
    mapping: GamutMapping,
    gamut: Gamut,
) -> Result<f64, ColorError> {
    if background.components()[3] != 1.0 {
        return Err(ColorError);
    }
    let bg = background.mapped_rgb(gamut, mapping)?;
    let alpha = text.components()[3];
    let mut fg = text.mapped_rgb(gamut, mapping)?;
    for i in 0..3 {
        fg[i] = fg[i] * alpha + bg[i] * (1.0 - alpha);
    }
    let weights = match gamut {
        Gamut::Srgb => [0.2126729, 0.7151522, 0.072175],
        Gamut::DisplayP3 => [0.228982959480578, 0.691749262585238, 0.0792677779341829],
        Gamut::Rec2020 => return Err(ColorError),
    };
    Ok(apca_rgb(fg, bg, weights))
}

fn apca_rgb(text: [f64; 3], background: [f64; 3], weights: [f64; 3]) -> f64 {
    let luminance = |rgb: [f64; 3]| {
        let y: f64 = rgb
            .into_iter()
            .zip(weights)
            .map(|(v, w)| v.powf(2.4) * w)
            .sum();
        if y < 0.022 {
            y + (0.022 - y).powf(1.414)
        } else {
            y
        }
    };
    let txt = luminance(text);
    let bg = luminance(background);
    if (bg - txt).abs() < 0.0005 {
        return 0.0;
    }
    let contrast = if bg > txt {
        (bg.powf(0.56) - txt.powf(0.57)) * 1.14
    } else {
        (bg.powf(0.65) - txt.powf(0.62)) * 1.14
    };
    if contrast.abs() < 0.1 {
        0.0
    } else {
        (contrast - 0.027 * contrast.signum()) * 100.0
    }
}

/// Search both polarities and prefer the reachable color closest to the source lightness.
pub fn contrast_sample(
    source: Oklch,
    reference: Oklch,
    target: f64,
    gamut: Gamut,
    to_text: bool,
    max_chroma: bool,
) -> Option<Oklch> {
    if !(0.0..=110.0).contains(&target) {
        return None;
    }
    let [original, c, h, _] = source.components();
    let at = |l| {
        Oklch::new(l, if max_chroma { 1.0 } else { c }, h, 1.0)
            .ok()
            .map(|v| v.reduce_chroma(gamut))
    };
    let score = |v| {
        if to_text {
            apca_in(reference, v, GamutMapping::Clip, gamut)
        } else {
            apca_in(v, reference, GamutMapping::Clip, gamut)
        }
    };
    let current = at(original)?;
    if (score(current).ok()?.abs() - target).abs() < 0.05 {
        return Some(current);
    }
    let mut best = None;
    for signed in [-target, target] {
        let (mut lo, mut hi) = (0.0, 1.0);
        let start = score(at(lo)?).ok()?;
        let end = score(at(hi)?).ok()?;
        if signed < start.min(end) || signed > start.max(end) {
            continue;
        }
        for _ in 0..24 {
            let mid = (lo + hi) * 0.5;
            if (score(at(mid)?).ok()? < signed) == (end > start) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let color = at((lo + hi) * 0.5)?;
        if (score(color).ok()? - signed).abs() < 0.05
            && best.is_none_or(|previous: Oklch| {
                (previous.components()[0] - original).abs()
                    > (color.components()[0] - original).abs()
            })
        {
            best = Some(color);
        }
    }
    best
}

/// Find the closest lightness achieving a signed Lc target. Chroma, hue and
/// alpha remain exact; unreachable targets (including the low-Lc gap) fail.
pub fn lightness_for_contrast(
    source: Oklch,
    background: Oklch,
    target: f64,
    mapping: GamutMapping,
) -> Result<Option<Oklch>, ColorError> {
    if !target.is_finite() || !(-110.0..=110.0).contains(&target) {
        return Err(ColorError);
    }
    let [original, c, h, a] = source.components();
    let at = |l| Oklch::new(l, c, h, a);
    let error = |l| apca(at(l)?, background, mapping).map(|v| v - target);
    let mut best = None::<Oklch>;
    let mut consider = |l: f64| -> Result<(), ColorError> {
        if error(l)?.abs() <= 0.01
            && best.is_none_or(|b| (l - original).abs() < (b.components()[0] - original).abs())
        {
            best = Some(at(l)?);
        }
        Ok(())
    };
    consider(original)?;
    let mut low = 0.0;
    let mut previous = error(low)?;
    consider(low)?;
    for i in 1..=256 {
        let high = f64::from(i) / 256.0;
        let next = error(high)?;
        consider(high)?;
        if previous.signum() != next.signum() {
            let (mut left, mut right) = (low, high);
            let mut left_error = previous;
            for _ in 0..40 {
                let mid = (left + right) * 0.5;
                let mid_error = error(mid)?;
                if left_error.signum() == mid_error.signum() {
                    left = mid;
                    left_error = mid_error;
                } else {
                    right = mid;
                }
            }
            consider((left + right) * 0.5)?;
        }
        low = high;
        previous = next;
    }
    Ok(best)
}
