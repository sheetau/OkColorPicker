//! Similar-color lookup in the vendored meodai/color-names dictionary.
use crate::{Gamut, Oklch};
use std::sync::LazyLock;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NamedColor {
    pub name: &'static str,
    pub color: Oklch,
}

const COLORS: &str = include_str!("color-names.tsv");

fn rows() -> impl Iterator<Item = ([u8; 3], u32)> {
    let mut offset = 0;
    COLORS.split_inclusive('\n').map(move |line| {
        let (hex, _) = line.split_once('\t').expect("dictionary row");
        let name_offset = u32::try_from(offset + hex.len() + 1).expect("dictionary fits in 4 GiB");
        offset += line.len();
        let [_, r, g, b] = u32::from_str_radix(hex, 16)
            .expect("dictionary hex")
            .to_be_bytes();
        ([r, g, b], name_offset)
    })
}

fn from_bytes(rgb: [u8; 3]) -> Oklch {
    Oklch::from_rgb(rgb.map(|v| f64::from(v) / 255.0), Gamut::Srgb, 1.0, 0.0).expect("sRGB color")
}

struct Entry {
    lab: [f64; 3],
    name_offset: u32,
    rgb: [u8; 3],
}

impl Entry {
    fn sample(&self) -> NamedColor {
        let name = COLORS[self.name_offset as usize..]
            .lines()
            .next()
            .expect("dictionary name");
        NamedColor {
            name,
            color: from_bytes(self.rgb),
        }
    }
}

static DICTIONARY: LazyLock<Box<[Entry]>> = LazyLock::new(|| {
    let mut entries: Box<[_]> = rows()
        .map(|(rgb, name_offset)| Entry {
            lab: from_bytes(rgb).lab_d50(),
            name_offset,
            rgb,
        })
        .collect();
    entries.sort_unstable_by(|a, b| a.lab[0].total_cmp(&b.lab[0]));
    entries
});

/// Uniform sampling of occupied lightness, chroma and hue bands avoids favoring
/// parts of sRGB with many nearly identical dictionary entries.
pub(crate) fn palette_anchor(band: usize, entry: usize) -> Oklch {
    static BANDS: LazyLock<Vec<Vec<[u8; 3]>>> = LazyLock::new(|| {
        let mut bands = vec![Vec::new(); 5 * 4 * 13];
        for (rgb, _) in rows() {
            let [l, c, h, _] = from_bytes(rgb).components();
            let light = ((l * 5.0) as usize).min(4);
            let chroma = ((c / 0.075) as usize).min(3);
            let hue = if c < 0.025 { 12 } else { (h / 30.0) as usize };
            bands[(light * 4 + chroma) * 13 + hue].push(rgb);
        }
        bands.retain(|band| !band.is_empty());
        bands
    });
    let choices = &BANDS[band % BANDS.len()];
    from_bytes(choices[entry % choices.len()])
}

/// Perceptual distance using CIEDE2000 with unit weighting factors.
pub fn delta_e_2000(first: Oklch, second: Oklch) -> f64 {
    delta_e(first.lab_d50(), second.lab_d50())
}

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
    let exclude = color.in_gamut(Gamut::Srgb).then(|| {
        color
            .encoded_rgb(Gamut::Srgb)
            .map(|v| (v * 255.0).round() as u8)
    });
    nearest(color, count, 30.0, exclude)
}

/// Display names include the exact dictionary color and have no distance cutoff.
pub fn color_name(color: Oklch) -> &'static str {
    nearest(color, 1, f64::INFINITY, None)
        .first()
        .map_or("", |sample| sample.name)
}

fn nearest(
    color: Oklch,
    count: usize,
    mut limit: f64,
    exclude: Option<[u8; 3]>,
) -> Vec<NamedColor> {
    let lab = color.lab_d50();
    if count == 0 || !lab.into_iter().all(f64::is_finite) {
        return Vec::new();
    }
    let split = DICTIONARY.partition_point(|entry| entry.lab[0] < lab[0]);
    let mut left = DICTIONARY[..split].iter().rev().peekable();
    let mut right = DICTIONARY[split..].iter().peekable();
    let mut hits: Vec<(f64, &Entry)> = Vec::new();
    loop {
        let entry = match (left.peek(), right.peek()) {
            (Some(a), Some(b)) if lab[0] - a.lab[0] <= b.lab[0] - lab[0] => left.next(),
            (_, Some(_)) => right.next(),
            _ => left.next(),
        };
        let Some(entry) = entry else {
            break;
        };
        // For L* in [0,100], ΔE00 >= |ΔL*| / 1.76. Both tails are now farther away.
        if (0.0..=100.0).contains(&lab[0]) && (entry.lab[0] - lab[0]).abs() > 1.76 * limit {
            break;
        }
        if Some(entry.rgb) == exclude {
            continue;
        }
        let d = delta_e(lab, entry.lab);
        if d < limit || (d == limit && hits.len() == count) {
            let i = hits.partition_point(|(score, other)| {
                (*score, other.name_offset) <= (d, entry.name_offset)
            });
            if i < count {
                hits.insert(i, (d, entry));
                hits.truncate(count);
                if hits.len() == count {
                    limit = hits[count - 1].0;
                }
            }
        }
    }
    hits.into_iter().map(|(_, entry)| entry.sample()).collect()
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
    fn pruned_lookup_matches_exhaustive_dictionary_order() {
        for (rgb, offset) in rows().step_by(4931) {
            let color = from_bytes(rgb);
            let lab = color.lab_d50();
            let mut scores: Vec<_> = rows()
                .filter(|(other, _)| *other != rgb)
                .map(|(other, name)| (delta_e(lab, from_bytes(other).lab_d50()), name))
                .filter(|(score, _)| *score < 30.0)
                .collect();
            scores.sort_by(|a, b| a.0.total_cmp(&b.0));
            let name_at = |offset: u32| COLORS[offset as usize..].lines().next().unwrap();
            let found: Vec<_> = nearest_named(color, 6)
                .into_iter()
                .map(|hit| hit.name)
                .collect();
            assert_eq!(
                found,
                scores
                    .iter()
                    .take(6)
                    .map(|hit| name_at(hit.1))
                    .collect::<Vec<_>>()
            );
            assert_eq!(color_name(color), name_at(offset));
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
