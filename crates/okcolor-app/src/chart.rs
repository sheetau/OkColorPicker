use crate::{model::PickerModel, surface::SurfaceKind, workspace::FixedAxis};
use okcolor_core::{Gamut, GamutProjection, GamutSlice, Oklch};
use zgui::prelude::GetUntracked;

impl FixedAxis {
    pub(crate) fn bar(self) -> SurfaceKind {
        match self {
            Self::Lightness => SurfaceKind::Lightness,
            Self::Chroma => SurfaceKind::Chroma,
            Self::Hue => SurfaceKind::Hue,
        }
    }

    pub(crate) fn coordinates(self, color: Oklch, max: f64) -> [f64; 2] {
        let [l, c, h, _] = color.components();
        match self {
            Self::Hue => [c / max, 1.0 - l],
            Self::Lightness => [h / 360.0, 1.0 - c / max],
            Self::Chroma => [h / 360.0, 1.0 - l],
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Chart {
    axis: FixedAxis,
    color: Oklch,
    max: f64,
    gamut: Gamut,
}

impl Chart {
    pub(crate) fn current(model: PickerModel) -> Self {
        Self {
            axis: model.tools.get_untracked().fixed_axis,
            color: model.current(),
            max: model.max_chroma.get_untracked(),
            gamut: model.gamut.get_untracked(),
        }
    }
    fn color_at(self, [x, y]: [f64; 2]) -> Oklch {
        let [l, c, h, a] = self.color.components();
        let hue = (360.0 * x).min(360.0_f64.next_down());
        let (l, c, h) = match self.axis {
            FixedAxis::Hue => (1.0 - y, x * self.max, h),
            FixedAxis::Lightness => (l, (1.0 - y) * self.max, hue),
            FixedAxis::Chroma => (1.0 - y, c, hue),
        };
        Oklch::new(l, c, h, a).expect("normalized chart coordinates")
    }
    pub(crate) fn pick(self, x: f64, y: f64) -> Option<Oklch> {
        if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
            return None;
        }
        let color = self.color_at([x, y]);
        color.in_gamut(self.gamut).then_some(color)
    }
}

// Keep the in-gamut side of a crossing. Sampling both directions handles
// disjoint hue intervals and vertical tangents on constant-chroma charts.
struct Contour(Vec<[f64; 2]>);
impl Contour {
    fn crossing(
        chart: Chart,
        gamut: Gamut,
        mut a: [f64; 2],
        mut b: [f64; 2],
        inside: bool,
    ) -> [f64; 2] {
        for _ in 0..20 {
            let m = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
            if chart.color_at(m).in_gamut(gamut) == inside {
                a = m
            } else {
                b = m
            }
        }
        if inside { a } else { b }
    }
    fn new(chart: Chart, gamut: Gamut) -> Self {
        const N: usize = 96;
        let mut points = Vec::new();
        let mut above = [false; N + 1];
        for j in 0..=N {
            let y = j as f64 / N as f64;
            let mut left = false;
            for (i, up) in above.iter_mut().enumerate() {
                let x = i as f64 / N as f64;
                let p = [x, y];
                let inside = chart.color_at(p).in_gamut(gamut);
                if i > 0 && left != inside {
                    points.push(Self::crossing(
                        chart,
                        gamut,
                        p,
                        [(i - 1) as f64 / N as f64, y],
                        inside,
                    ));
                }
                if j > 0 && *up != inside {
                    points.push(Self::crossing(
                        chart,
                        gamut,
                        p,
                        [x, (j - 1) as f64 / N as f64],
                        inside,
                    ));
                }
                left = inside;
                *up = inside;
            }
        }
        Self(points)
    }
    fn nearest(
        &self,
        chart: Chart,
        gamut: Gamut,
        target: [f64; 2],
        size: [f64; 2],
        seed: Option<[f64; 2]>,
    ) -> Option<(f64, Oklch)> {
        let distance =
            |p: [f64; 2]| ((p[0] - target[0]) * size[0]).hypot((p[1] - target[1]) * size[1]);
        // A very high fixed chroma can leave an island smaller than one grid cell.
        // Bracket a crossing from the last valid sample to retain such islands.
        let cursor = target.map(|v| v.clamp(0.0, 1.0));
        let from_seed = seed
            .filter(|p| chart.color_at(*p).in_gamut(gamut))
            .and_then(|p| {
                (!chart.color_at(cursor).in_gamut(gamut))
                    .then(|| Self::crossing(chart, gamut, p, cursor, true))
            });
        let mut best = self
            .0
            .iter()
            .copied()
            .chain(from_seed)
            .min_by(|a, b| distance(*a).total_cmp(&distance(*b)))?;
        let mut score = distance(best);
        // Refine locally against actual gamut membership in screen space.
        let mut step = 1.0 / 96.0;
        for _ in 0..13 {
            let center = best;
            for axis in 0..2 {
                for sign in [-1.0, 1.0] {
                    let mut a = center;
                    a[axis] = (center[axis] + sign * step).clamp(0.0, 1.0);
                    let mut b = a;
                    a[1 - axis] = (a[1 - axis] - step * 2.0).max(0.0);
                    b[1 - axis] = (b[1 - axis] + step * 2.0).min(1.0);
                    let inside = chart.color_at(a).in_gamut(gamut);
                    if chart.color_at(b).in_gamut(gamut) != inside {
                        let p = Self::crossing(chart, gamut, a, b, inside);
                        let d = distance(p);
                        if d < score {
                            score = d;
                            best = p;
                        }
                    }
                }
            }
            step *= 0.5;
        }
        Some((score, chart.color_at(best)))
    }
}

pub(crate) struct ChartGesture {
    chart: Chart,
    lc: Option<GamutProjection>,
    anchor: Option<[f64; 2]>,
    contours: Vec<(Gamut, Contour)>,
}
impl ChartGesture {
    pub(crate) fn new(chart: Chart) -> Self {
        Self {
            chart,
            lc: None,
            anchor: None,
            contours: Vec::new(),
        }
    }
    fn nearest(&mut self, gamut: Gamut, p: [f64; 2], size: [f64; 2]) -> Option<(f64, Oklch)> {
        let index = if let Some(i) = self.contours.iter().position(|(g, _)| *g == gamut) {
            i
        } else {
            self.contours.push((gamut, Contour::new(self.chart, gamut)));
            self.contours.len() - 1
        };
        self.contours[index]
            .1
            .nearest(self.chart, gamut, p, size, self.anchor)
    }
    pub(crate) fn select(
        &mut self,
        model: PickerModel,
        p: [f64; 2],
        size: [f64; 2],
        snap: bool,
        drag: bool,
    ) -> Option<Oklch> {
        let chart = self.chart;
        let pick = chart.pick(p[0], p[1]);
        if pick.is_some() {
            self.anchor = Some(p);
        }
        if snap {
            let mut best = None;
            let mut distance = 4.0;
            for (g, visible) in [
                (Gamut::Srgb, true),
                (Gamut::DisplayP3, model.show_p3.get_untracked()),
                (Gamut::Rec2020, model.show_rec2020.get_untracked()),
            ] {
                if visible
                    && let Some((d, c)) = self.nearest(g, p, size)
                    && d <= distance
                    && c.in_gamut(chart.gamut)
                {
                    distance = d;
                    best = Some(c);
                }
            }
            if best.is_some() {
                return best;
            }
        }
        if pick.is_some() || !drag {
            return pick;
        }
        if chart.axis == FixedAxis::Hue {
            let lc = self.lc.get_or_insert_with(|| {
                GamutSlice::new(chart.color.components()[2], chart.max, chart.gamut)
                    .expect("validated chart")
                    .projection()
            });
            return lc.project(p[0], p[1], chart.color.components()[3], size[0] / size[1]);
        }
        chart
            .pick(p[0].clamp(0.0, 1.0), p[1].clamp(0.0, 1.0))
            .or_else(|| self.nearest(chart.gamut, p, size).map(|(_, c)| c))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zgui::prelude::{Owner, Update};
    #[test]
    fn snapping_preserves_the_fixed_axis_on_each_plane() {
        Owner::new().with(|| {
            let model = PickerModel::default();
            let edge = Oklch::new(0.65, 1.0, 30.0, 0.64)
                .unwrap()
                .reduce_chroma(Gamut::Srgb);
            model.select(edge);
            for axis in [FixedAxis::Lightness, FixedAxis::Chroma, FixedAxis::Hue] {
                model.tools.update(|t| t.fixed_axis = axis);
                let chart = Chart::current(model);
                let mut g = ChartGesture::new(chart);
                let p = axis.coordinates(edge, chart.max);
                // Find an interior sample one screen pixel from a known boundary.
                let size = [290.0, 152.0];
                let probe = [[1.0, 0.0], [-1.0, 0.0], [0.0, 1.0], [0.0, -1.0]]
                    .into_iter()
                    .map(|d| [p[0] + d[0] / size[0], p[1] + d[1] / size[1]])
                    .find(|v| chart.pick(v[0], v[1]).is_some())
                    .unwrap();
                let free = g.select(model, probe, size, false, false).unwrap();
                assert_eq!(Some(free), chart.pick(probe[0], probe[1]));
                let snapped = g.select(model, probe, size, true, false).unwrap();
                assert_ne!(free, snapped);
                let index = axis.bar() as usize - 1;
                assert_eq!(snapped.components()[index], edge.components()[index]);
                assert_eq!(snapped.components()[3], edge.components()[3]);
                let loc = axis.coordinates(snapped, chart.max);
                assert!(
                    ((probe[0] - loc[0]) * size[0]).hypot((probe[1] - loc[1]) * size[1]) < 1.05
                );
                assert!(snapped.in_gamut(chart.gamut));
                let outside = [0.99, -0.25];
                assert!(g.select(model, outside, size, false, false).is_none());
                let clamped = g.select(model, outside, size, false, true).unwrap();
                assert!(clamped.in_gamut(chart.gamut));
                assert_eq!(clamped.components()[index], edge.components()[index]);
            }
        });
    }
}
