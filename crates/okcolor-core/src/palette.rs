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
    labs: Vec<Lab>,
    targets: Vec<Lab>,
    lock_mismatch: f64,
    span: f64,
    accent: Option<usize>,
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
        let accent = rng.next() as usize % count;
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
        let mut roles = Vec::with_capacity(count);
        for (i, &(original, locked)) in old.iter().enumerate() {
            let role = {
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
            roles.push(role.oklab());
            let visible = if locked {
                original.reduce_chroma(gamut)
            } else {
                role
            };
            labs.push(visible.oklab());
            slots.push((if locked { original } else { role }, locked));
        }
        let (targets, lock_mismatch) = lock_targets(&roles, &labs, old);
        let mut candidate = Candidate {
            slots,
            labs,
            targets,
            lock_mismatch,
            span,
            accent: self.accent.then_some(accent),
            style,
            quality: 0.0,
        };
        candidate.quality = candidate.score();
        candidate
    }
}

/// Compare signed lightness and opponent-axis differences for every pair of locks.
/// Each free role then follows the offsets of nearby locks more closely than distant ones.
fn lock_targets(roles: &[Lab], visible: &[Lab], slots: &[Slot]) -> (Vec<Lab>, f64) {
    let offsets: Vec<(usize, Lab)> = slots
        .iter()
        .enumerate()
        .filter(|(_, slot)| slot.1)
        .map(|(i, _)| (i, std::array::from_fn(|k| visible[i][k] - roles[i][k])))
        .collect();
    if offsets.is_empty() {
        return (Vec::new(), 0.0);
    }
    let mut mismatch = 0.0;
    let mut pairs = 0;
    for (i, &(_, offset)) in offsets.iter().enumerate() {
        for &(_, earlier) in &offsets[..i] {
            // The difference of offsets is the error in the template's lock-to-lock vector.
            mismatch += distance(offset, earlier);
            pairs += 1;
        }
    }
    let targets = roles
        .iter()
        .enumerate()
        .map(|(i, role)| {
            let mut shift = [0.0; 3];
            let mut weight = 0.0;
            for &(j, offset) in &offsets {
                let w = 1.0 / (1.0 + i.abs_diff(j) as f64).powi(2);
                for k in 0..3 {
                    shift[k] += w * offset[k];
                }
                weight += w;
            }
            std::array::from_fn(|k| role[k] + shift[k] / weight)
        })
        .collect();
    (targets, mismatch / pairs.max(1) as f64)
}

impl Candidate {
    fn score(&self) -> f64 {
        let count = self.slots.len();
        let duplicate_threshold = 0.14 / (count as f64).sqrt();
        let mut min_l: f64 = 1.0;
        let mut max_l: f64 = 0.0;
        let mut total_c = 0.0;
        let mut relation_error = 0.0;
        let mut free = 0;
        let mut cost = 2.5 * self.lock_mismatch;
        for (i, &lab) in self.labs.iter().enumerate() {
            min_l = min_l.min(lab[0]);
            max_l = max_l.max(lab[0]);
            total_c += lab[1].hypot(lab[2]);
            if !self.slots[i].1 && !self.targets.is_empty() {
                relation_error += distance(lab, self.targets[i]);
                free += 1;
            }
            for (j, &earlier) in self.labs[..i].iter().enumerate() {
                if self.slots[i].1 && self.slots[j].1 {
                    continue;
                }
                let d = distance(lab, earlier);
                cost += 2.0 * (1.0 - d / duplicate_threshold).max(0.0);
                if i == j + 1 {
                    cost += (0.09 - d).max(0.0);
                }
            }
        }
        cost += 2.0 * (max_l - min_l - self.span).abs() + 4.0 * relation_error / free.max(1) as f64;
        if let Some(i) = self.accent
            && count > 1
        {
            let accent = self.labs[i][1].hypot(self.labs[i][2]);
            cost += 3.0 * ((total_c - accent) / (count - 1) as f64 + 0.03 - accent).max(0.0);
        }
        1.0 / (1.0 + cost)
    }

    fn refine(&mut self, gamut: Gamut, rng: &mut Random) {
        if self.targets.is_empty() {
            return;
        }
        // Coordinate search on the strongest candidates. Score the whole palette after
        // each trial so that fitting a lock cannot silently introduce a duplicate.
        for radius in [0.08, 0.04, 0.02] {
            for i in 0..self.slots.len() {
                if self.slots[i].1 {
                    continue;
                }
                let start = self.labs[i];
                let toward: Lab = std::array::from_fn(|k| (start[k] + self.targets[i][k]) * 0.5);
                let jitter: Lab = std::array::from_fn(|k| start[k] + rng.range(-radius, radius));
                for mut lab in [toward, jitter] {
                    lab[0] = lab[0].clamp(0.02, 0.98);
                    let hue = self.slots[i].0.components()[2];
                    let trial = Oklch::from_oklab(lab, 1.0, hue)
                        .unwrap()
                        .reduce_chroma(gamut);
                    let previous = std::mem::replace(&mut self.labs[i], trial.oklab());
                    let quality = self.score();
                    if quality > self.quality {
                        self.quality = quality;
                        self.slots[i].0 = trial;
                    } else {
                        self.labs[i] = previous;
                    }
                }
            }
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
            for mut candidate in candidates.into_iter().take(2) {
                candidate.refine(gamut, &mut rng);
                let generated: Vec<Lab> = candidate
                    .labs
                    .iter()
                    .zip(&candidate.slots)
                    .filter_map(|(&lab, slot)| (!slot.1).then_some(lab))
                    .collect();
                let mut novelty: f64 = 1.0;
                let mut repeats = 0;
                for (prior_style, previous) in &self.recent {
                    let dissimilarity = 0.5
                        * (nearest_mean(&generated, previous) + nearest_mean(previous, &generated));
                    novelty = novelty.min((dissimilarity / 0.16).clamp(0.025, 1.0));
                    repeats += usize::from(*prior_style == style);
                }
                let weight =
                    candidate.quality.powi(4) * novelty.powi(2) / (1 + repeats * repeats) as f64;
                total_weight += weight;
                if rng.unit() * total_weight < weight {
                    selected = Some((candidate, generated));
                }
            }
        }
        let (chosen, generated) = selected.expect("at least one weighted candidate");
        if self.recent.len() == 8 {
            self.recent.pop_front();
        }
        self.recent.push_back((chosen.style, generated));
        chosen.slots
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_order_changes_the_fit_even_with_the_same_mean_color() {
        let roles = [
            [0.25, -0.08, 0.05],
            [0.38, 0.0, 0.04],
            [0.53, 0.0, 0.0],
            [0.68, 0.0, -0.04],
            [0.82, 0.1, -0.1],
        ];
        let mut visible = roles;
        let mut slots = roles.map(|lab| (Oklch::from_oklab(lab, 1.0, 0.0).unwrap(), false));
        slots[0].1 = true;
        slots[4].1 = true;
        let (exact, cost) = lock_targets(&roles, &visible, &slots);
        assert_eq!(exact, roles);
        assert_eq!(cost, 0.0);
        visible.swap(0, 4);
        let (reversed, cost) = lock_targets(&roles, &visible, &slots);
        assert!(cost > 0.5);
        assert!(reversed[1][0] > roles[1][0] && reversed[3][0] < roles[3][0]);
        assert!(reversed[1][1] > roles[1][1] && reversed[3][2] > roles[3][2]);
    }

    #[test]
    fn search_improves_locked_compositions_and_keeps_the_scored_colors() {
        let mut rng = Random(42);
        let slots = [
            (Oklch::new(0.2, 0.03, 10.0, 0.4).unwrap(), true),
            (Oklch::new(0.5, 0.0, 40.0, 1.0).unwrap(), false),
            (Oklch::new(0.8, 0.23, 110.0, 1.0).unwrap(), true),
            (Oklch::new(0.5, 0.0, 40.0, 1.0).unwrap(), false),
            (Oklch::new(0.53, 0.18, 270.0, 1.0).unwrap(), true),
        ];
        for (style, composition) in STYLES.iter().enumerate() {
            let mut candidate = composition.propose(&slots, Gamut::DisplayP3, &mut rng, style);
            let initial = candidate.quality;
            candidate.refine(Gamut::DisplayP3, &mut rng);
            assert!(candidate.quality > initial);
            assert_eq!(candidate.quality, candidate.score());
            for (i, &slot) in candidate.slots.iter().enumerate() {
                if slot.1 {
                    assert_eq!(slot, slots[i]);
                } else {
                    assert!(slot.0.in_gamut(Gamut::DisplayP3));
                    assert_eq!(slot.0.oklab(), candidate.labs[i]);
                }
            }
        }
    }
}
