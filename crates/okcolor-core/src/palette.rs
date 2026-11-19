//! Generate related color sets in OKLCH and rank the complete compositions.
use crate::{Gamut, Oklch, named::palette_anchor};
use std::collections::VecDeque;

struct Random(u64);
impl Random {
    fn next(&mut self) -> u64 {
        // SplitMix64 supplies reproducible candidates without a runtime dependency.
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, low: f64, high: f64) -> f64 {
        low + self.unit() * (high - low)
    }
}

struct Composition {
    // Repeated hue offsets assign a family to multiple positions. Lightness
    // and chroma roles distinguish a subdued accent palette from a tonal ramp.
    hues: &'static [f64],
    span: f64,
    center: f64,
    chroma: f64,
    accent: bool,
}
const STYLES: [Composition; 8] = [
    Composition {
        hues: &[0.0],
        span: 0.60,
        center: 0.58,
        chroma: 0.55,
        accent: false,
    },
    Composition {
        hues: &[-28.0, 0.0, 24.0],
        span: 0.42,
        center: 0.60,
        chroma: 0.64,
        accent: false,
    },
    Composition {
        hues: &[0.0, 12.0, -12.0, 150.0],
        span: 0.48,
        center: 0.61,
        chroma: 0.72,
        accent: true,
    },
    Composition {
        hues: &[0.0, 180.0],
        span: 0.55,
        center: 0.56,
        chroma: 0.72,
        accent: false,
    },
    Composition {
        hues: &[0.0, 150.0, 210.0],
        span: 0.34,
        center: 0.63,
        chroma: 0.78,
        accent: true,
    },
    Composition {
        hues: &[0.0, 120.0, 240.0],
        span: 0.21,
        center: 0.83,
        chroma: 0.40,
        accent: false,
    },
    Composition {
        hues: &[0.0, 72.0, 180.0, 252.0],
        span: 0.50,
        center: 0.61,
        chroma: 0.80,
        accent: false,
    },
    Composition {
        hues: &[0.0, 18.0, 175.0],
        span: 0.28,
        center: 0.37,
        chroma: 0.88,
        accent: true,
    },
];

type Lab = [f64; 3];
type Slot = (Oklch, bool);
fn distance(a: Lab, b: Lab) -> f64 {
    a.into_iter()
        .zip(b)
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f64>()
        .sqrt()
}

struct Candidate {
    slots: Vec<Slot>,
    generated: Vec<Lab>,
    style: usize,
    quality: f64,
}

impl Composition {
    fn propose(&self, old: &[Slot], gamut: Gamut, rng: &mut Random, style: usize) -> Candidate {
        let count = old.len();
        let anchor = palette_anchor(rng.next() as usize, rng.next() as usize);
        let [anchor_l, anchor_c, anchor_h, _] = anchor.components();
        let phase = rng.next() as usize % self.hues.len();
        let hue_scale = rng.range(0.85, 1.15);
        let accent_start = rng.next() as usize % count;
        let accent = (0..count)
            .map(|offset| (accent_start + offset) % count)
            .find(|&i| !old[i].1)
            .expect("at least one unlocked slot");
        let offset = |i: usize| {
            let family = if self.accent && i == accent {
                self.hues.len() - 1
            } else {
                (i + phase) % (self.hues.len() - usize::from(self.accent))
            };
            self.hues[family] * hue_scale
        };
        // Fit the hue arrangement to every chromatic lock on the circle.
        let mut vector = [0.0; 2];
        let mut locked_l = 0.0;
        let mut locks = 0;
        for (i, (color, locked)) in old.iter().enumerate() {
            if *locked {
                let [l, c, h, _] = color.components();
                locked_l += l;
                locks += 1;
                if c > 0.025 {
                    let angle = (h - offset(i)).to_radians();
                    vector[0] += c.min(0.35) * angle.cos();
                    vector[1] += c.min(0.35) * angle.sin();
                }
            }
        }
        let hue = if f64::hypot(vector[0], vector[1]) > 0.01 {
            vector[1].atan2(vector[0]).to_degrees()
        } else if anchor_c > 0.025 {
            anchor_h
        } else {
            rng.range(0.0, 360.0)
        };
        let base_l = if locks == 0 {
            anchor_l
        } else {
            locked_l / f64::from(locks)
        };
        let span = self.span * rng.range(0.84, 1.15);
        let center = (self.center + 0.25 * (base_l - 0.55) + rng.range(-0.055, 0.055))
            .clamp(0.025 + span * 0.5, 0.975 - span * 0.5);
        let strength =
            (self.chroma + 0.35 * (anchor_c - 0.12) + rng.range(-0.12, 0.12)).clamp(0.18, 0.99);
        let start = rng.next() as usize % count;
        let reverse = rng.unit() < 0.5;
        let mut slots = Vec::with_capacity(count);
        let mut labs = Vec::with_capacity(count);
        let mut generated = Vec::new();
        let mut min_l: f64 = 1.0;
        let mut max_l: f64 = 0.0;
        let mut accent_c = 0.0;
        let mut others_c = 0.0;
        for (i, &(original, locked)) in old.iter().enumerate() {
            let color = if locked {
                original
            } else {
                let rank = (i + start) % count;
                let t = rank as f64 / (count - 1).max(1) as f64;
                let t = if reverse { 1.0 - t } else { t };
                let l = (center + span * (t - 0.5 + rng.range(-0.22, 0.22) / count as f64))
                    .clamp(0.02, 0.98);
                let h = hue + offset(i) + rng.range(-5.0, 5.0);
                let limit = Oklch::new(l, 1.0, h, 1.0)
                    .unwrap()
                    .reduce_chroma(gamut)
                    .components()[1];
                let saturation = if self.accent && i != accent {
                    rng.range(0.12, 0.40)
                } else {
                    strength * rng.range(0.83, 1.0)
                };
                Oklch::new(l, limit * saturation, h, 1.0).unwrap()
            };
            slots.push((color, locked));
            // Extreme out-of-gamut locks stay intact. Assess their visible contribution.
            let visible = if locked {
                color.reduce_chroma(gamut)
            } else {
                color
            };
            let lab = visible.oklab();
            if !locked {
                generated.push(lab);
            }
            labs.push(lab);
            min_l = min_l.min(lab[0]);
            max_l = max_l.max(lab[0]);
            let chroma = visible.components()[1];
            if i == accent {
                accent_c = chroma;
            } else {
                others_c += chroma;
            }
        }
        let mut duplicates = 0.0;
        let mut adjacent = 0.0;
        for i in 0..count {
            for j in 0..i {
                if old[i].1 && old[j].1 {
                    continue;
                }
                let d = distance(labs[i], labs[j]);
                duplicates += (1.0 - d / (0.14 / (count as f64).sqrt())).max(0.0);
                if i == j + 1 {
                    adjacent += (0.09 - d).max(0.0);
                }
            }
        }
        let mut cost = 2.0 * duplicates + adjacent + 2.0 * ((max_l - min_l) - span).abs();
        if self.accent && count > 1 {
            cost += 3.0 * (others_c / (count - 1) as f64 + 0.03 - accent_c).max(0.0);
        }
        Candidate {
            slots,
            generated,
            style,
            quality: 1.0 / (1.0 + cost),
        }
    }
}

fn nearest_mean(a: &[Lab], b: &[Lab]) -> f64 {
    a.iter()
        .map(|&x| {
            b.iter()
                .map(|&y| distance(x, y))
                .fold(f64::INFINITY, f64::min)
        })
        .sum::<f64>()
        / a.len() as f64
}

/// Session-only selection history. No training data or network access is needed.
#[derive(Default)]
pub struct PaletteGenerator {
    recent: VecDeque<(usize, Vec<Lab>)>,
}
impl PaletteGenerator {
    /// An empty strip starts with five slots. Every subsequent generation preserves
    /// the length, the positions of locks and their exact source components.
    pub fn generate(&mut self, old: &[Slot], gamut: Gamut, seed: u64) -> Vec<Slot> {
        let initial = [(Oklch::new(0.5, 0.0, 0.0, 1.0).unwrap(), false); 5];
        let old = if old.is_empty() { &initial[..] } else { old };
        if old.iter().all(|slot| slot.1) {
            return old.to_vec();
        }
        let mut rng = Random(seed);
        let mut total_weight = 0.0;
        let mut selected = None;
        for (style, composition) in STYLES.iter().enumerate() {
            let mut candidates: Vec<_> = (0..8)
                .map(|_| composition.propose(old, gamut, &mut rng, style))
                .collect();
            candidates.sort_by(|a, b| b.quality.total_cmp(&a.quality));
            for candidate in candidates.into_iter().take(2) {
                let mut novelty: f64 = 1.0;
                let mut repeats = 0;
                for (prior_style, previous) in &self.recent {
                    let dissimilarity = 0.5
                        * (nearest_mean(&candidate.generated, previous)
                            + nearest_mean(previous, &candidate.generated));
                    novelty = novelty.min((dissimilarity / 0.16).clamp(0.025, 1.0));
                    repeats += usize::from(*prior_style == style);
                }
                let weight =
                    candidate.quality.powi(4) * novelty.powi(2) / (1 + repeats * repeats) as f64;
                total_weight += weight;
                if rng.unit() * total_weight < weight {
                    selected = Some(candidate);
                }
            }
        }
        let chosen = selected.expect("at least one weighted candidate");
        if self.recent.len() == 8 {
            self.recent.pop_front();
        }
        self.recent.push_back((chosen.style, chosen.generated));
        chosen.slots
    }
}
