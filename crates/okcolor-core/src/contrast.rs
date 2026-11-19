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

/// Harmonizer searches lighter candidates against a dark reference and darker ones against a light reference.
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
    let signed = default_grid_target(reference, to_text, target);
    let (color, actual) =
        closest_grid_sample(source, reference, signed, gamut, to_text, max_chroma)?;
    ((actual.abs() - target).abs() < 0.05).then_some(color)
}

/// Default grid direction: lighter candidates for a dark reference, darker for a light one.
pub fn default_grid_target(reference: Oklch, to_text: bool, magnitude: f64) -> f64 {
    if (reference.components()[0] <= 0.6) == to_text {
        magnitude
    } else {
        -magnitude
    }
}

/// Return the nearest reachable color and its actual signed APCA score.
/// Each reference, polarity and gamut has its own endpoints and low-contrast dead zone.
pub fn closest_grid_sample(
    source: Oklch,
    reference: Oklch,
    target: f64,
    gamut: Gamut,
    to_text: bool,
    max_chroma: bool,
) -> Option<(Oklch, f64)> {
    let [original, c, h, _] = source.components();
    let at = |l| {
        let color = Oklch::new(l, if max_chroma { 1.0 } else { c }, h, 1.0)
            .ok()?
            .reduce_chroma(gamut);
        let score = if to_text {
            apca_in(reference, color, GamutMapping::Clip, gamut)
        } else {
            apca_in(color, reference, GamutMapping::Clip, gamut)
        }
        .ok()?;
        Some((color, score))
    };
    nearest_lightness(original, target, at)
}

pub fn closest_current_contrast(
    source: Oklch,
    reference: Oklch,
    target: f64,
    gamut: Gamut,
    to_text: bool,
    mapping: GamutMapping,
) -> Option<(Oklch, f64)> {
    let [original, c, h, a] = source.components();
    let at = |l| {
        let color = Oklch::new(l, c, h, a).ok()?;
        let score = if to_text {
            apca_in(reference, color, mapping, gamut)
        } else {
            apca_in(color, reference, mapping, gamut)
        }
        .ok()?;
        Some((color, score))
    };
    if mapping == GamutMapping::Clip {
        nearest_clipped_lightness(original, target, at)
    } else {
        nearest_lightness(original, target, at)
    }
}

type Sample = (Oklch, f64);

fn nearer(a: Sample, b: Sample, target: f64, original: f64) -> Sample {
    let key = |s: Sample| ((s.1 - target).abs(), (s.0.components()[0] - original).abs());
    if key(a) <= key(b) { a } else { b }
}

// Clipping RGB channels at fixed, high chroma can create interior extrema.
// Search each crossing before refining the closest sampled extremum.
fn nearest_clipped_lightness(
    original: f64,
    target: f64,
    at: impl Fn(f64) -> Option<Sample>,
) -> Option<Sample> {
    if !target.is_finite() {
        return None;
    }
    const STEPS: u32 = 256;
    let mut best = at(original)?;
    let mut previous = at(0.0)?;
    best = nearer(best, previous, target, original);
    for i in 1..=STEPS {
        let current = at(f64::from(i) / f64::from(STEPS))?;
        best = nearer(best, current, target, original);
        if (previous.1 < target && current.1 > target)
            || (previous.1 > target && current.1 < target)
        {
            best = nearer(
                best,
                bisect_score(previous, current, target, original, &at)?,
                target,
                original,
            );
        }
        previous = current;
    }
    let center = best.0.components()[0];
    let step = 1.0 / f64::from(STEPS);
    let (mut left, mut right) = ((center - step).max(0.0), (center + step).min(1.0));
    for _ in 0..28 {
        let third = (right - left) / 3.0;
        let a = at(left + third)?;
        let b = at(right - third)?;
        best = nearer(best, nearer(a, b, target, original), target, original);
        if (a.1 - target).abs() <= (b.1 - target).abs() {
            right -= third;
        } else {
            left += third;
        }
    }
    Some(best)
}

fn nearest_lightness(
    original: f64,
    target: f64,
    at: impl Fn(f64) -> Option<(Oklch, f64)>,
) -> Option<(Oklch, f64)> {
    if !target.is_finite() {
        return None;
    }
    let current = at(original)?;
    if (current.1 - target).abs() < 0.00001 {
        return Some(current);
    }
    let lower = at(0.0)?;
    let upper = at(1.0)?;
    let increasing = upper.1 > lower.1;
    let direction = if increasing { 1.0 } else { -1.0 };
    if (target - lower.1) * direction <= 0.0 {
        return Some(nearer(current, lower, target, original));
    }
    if (target - upper.1) * direction >= 0.0 {
        return Some(nearer(current, upper, target, original));
    }
    let sample = bisect_score(lower, upper, target, original, &at)?;
    Some(nearer(current, sample, target, original))
}

fn bisect_score(
    mut lower: Sample,
    mut upper: Sample,
    target: f64,
    original: f64,
    at: &impl Fn(f64) -> Option<Sample>,
) -> Option<Sample> {
    let increasing = upper.1 > lower.1;
    for _ in 0..28 {
        let mid = at((lower.0.components()[0] + upper.0.components()[0]) * 0.5)?;
        if (mid.1 < target) == increasing {
            lower = mid
        } else {
            upper = mid
        }
    }
    Some(nearer(lower, upper, target, original))
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
