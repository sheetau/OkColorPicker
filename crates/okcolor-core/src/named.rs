//! Similar-color lookup in the vendored meodai/color-names dictionary.
use crate::{Gamut, GamutMapping, Oklch};
use std::sync::LazyLock;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NamedColor {
    pub name: &'static str,
    pub color: Oklch,
}

struct Entry {
    sample: NamedColor,
    lab: [f64; 3],
    rgb: [u8; 3],
}
static DICTIONARY: LazyLock<Vec<Entry>> = LazyLock::new(|| {
    include_str!("color-names.tsv")
        .lines()
        .map(|line| {
            let (hex, name) = line.split_once('\t').expect("dictionary row");
            let rgb: [u8; 3] = std::array::from_fn(|i| {
                u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).expect("dictionary hex")
            });
            let color = Oklch::from_rgb(rgb.map(|v| f64::from(v) / 255.0), Gamut::Srgb, 1.0, 0.0)
                .expect("sRGB color");
            Entry {
                sample: NamedColor { name, color },
                lab: color.lab_d50(),
                rgb,
            }
        })
        .collect()
});

// CIEDE2000 with the standard unit weighting factors.
fn delta_e(a: [f64; 3], b: [f64; 3]) -> f64 {
    let [l1, a1, b1] = a;
    let [l2, a2, b2] = b;
    let ratio = |c: f64| {
        let p = (c / 25.0).powi(7);
        (p / (1.0 + p)).sqrt()
    };
    let correction = 1.0 + 0.5 * (1.0 - ratio((a1.hypot(b1) + a2.hypot(b2)) * 0.5));
    let polar = |a: f64, b: f64| {
        let a = a * correction;
        (a.hypot(b), b.atan2(a).to_degrees().rem_euclid(360.0))
    };
    let (c1, h1) = polar(a1, b1);
    let (c2, h2) = polar(a2, b2);
    let chroma = (c1 + c2) * 0.5;
    let diff = h2 - h1;
    let sum = h1 + h2;
    let (dh, h) = if c1 * c2 == 0.0 {
        (0.0, sum)
    } else if diff.abs() <= 180.0 {
        (diff, sum * 0.5)
    } else {
        (
            diff - 360.0 * diff.signum(),
            (sum + if sum < 360.0 { 360.0 } else { -360.0 }) * 0.5,
        )
    };
    let cos = |deg: f64| deg.to_radians().cos();
    let t = 1.0 - 0.17 * cos(h - 30.0) + 0.24 * cos(2.0 * h) + 0.32 * cos(3.0 * h + 6.0)
        - 0.20 * cos(4.0 * h - 63.0);
    let offset = ((l1 + l2) * 0.5 - 50.0).powi(2);
    let light = (l2 - l1) / (1.0 + 0.015 * offset / (20.0 + offset).sqrt());
    let c = (c2 - c1) / (1.0 + 0.045 * chroma);
    let hue = 2.0 * (c1 * c2).sqrt() * (dh * 0.5).to_radians().sin() / (1.0 + 0.015 * chroma * t);
    let rotation = -2.0
        * ratio(chroma)
        * (60.0 * (-((h - 275.0) / 25.0).powi(2)).exp())
            .to_radians()
            .sin();
    (light * light + c * c + hue * hue + rotation * c * hue).sqrt()
}

/// Closest distinct dictionary colors under ΔE00 30, excluding the input HEX.
pub fn nearest_named(color: Oklch, count: usize) -> Vec<NamedColor> {
    if count == 0 {
        return Vec::new();
    }
    let lab = color.lab_d50();
    if !lab.into_iter().all(f64::is_finite) {
        return Vec::new();
    }
    let rgb = color
        .in_gamut(Gamut::Srgb)
        .then(|| color.mapped_rgb(Gamut::Srgb, GamutMapping::Clip).ok())
        .flatten()
        .map(|channels| channels.map(|v| (v * 255.0).round() as u8));
    let mut matches: Vec<(f64, NamedColor)> = Vec::new();
    let mut limit = 30.0;
    for entry in DICTIONARY.iter() {
        // The chroma/hue quadratic is nonnegative. S_L <= 1.75 for L* in [0,100].
        if Some(entry.rgb) == rgb
            || ((0.0..=100.0).contains(&lab[0]) && (entry.lab[0] - lab[0]).abs() > 1.76 * limit)
        {
            continue;
        }
        let d = delta_e(lab, entry.lab);
        if d < limit {
            let i = matches.partition_point(|(score, _)| *score <= d);
            matches.insert(i, (d, entry.sample));
            if matches.len() >= count {
                matches.truncate(count);
                limit = matches[count - 1].0;
            }
        }
    }
    matches.into_iter().map(|(_, entry)| entry).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sharma_reference_pairs() {
        for (a, b, expected) in [
            ([50.0, 2.6772, -79.7751], [50.0, 0.0, -82.7485], 2.0425),
            ([50.0, 0.0, 0.0], [50.0, -1.0, 2.0], 2.3669),
            ([50.0, -0.001, 2.49], [50.0, 0.0011, -2.49], 4.7461),
        ] {
            assert!((delta_e(a, b) - expected).abs() < 0.0001);
        }
    }
    #[test]
    fn exact_match_excluded_and_distances_sorted() {
        let source = crate::parse_color("#c93f38", 0.0).unwrap();
        let hits = nearest_named(source, 6);
        assert_eq!(hits.len(), 6);
        assert_eq!(hits[..4], nearest_named(source, 4));
        let scores: Vec<_> = hits
            .iter()
            .map(|e| delta_e(source.lab_d50(), e.color.lab_d50()))
            .collect();
        assert!(scores[0] > 0.0 && scores[5] < 30.0 && scores.windows(2).all(|p| p[0] <= p[1]));
    }
}
