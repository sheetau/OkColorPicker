use okcolor_core::{Gamut, Oklch, PaletteGenerator};
use std::{collections::HashSet, time::Instant};

#[test]
fn adjustments_preserve_working_gamut_and_alpha_and_have_neutral_zero() {
    use okcolor_core::adjust_palette_color;
    for gamut in [Gamut::Srgb, Gamut::DisplayP3, Gamut::Rec2020] {
        let base = Oklch::from_rgb([0.0, 1.0, 0.0], gamut, 0.4, 0.0).unwrap();
        assert_eq!(adjust_palette_color(base, gamut, [0; 4]), base);
        for (offsets, rgb) in [
            ([60, 0, 0, 0], [0.0, 0.0, 1.0]),
            ([0, -100, 0, 0], [0.5, 0.5, 0.5]),
            ([0, 0, 50, 0], [0.5, 1.0, 0.5]),
            ([0, 0, 0, -50], [0.0, 0.5, 0.5]),
            ([0, 0, 0, 100], [1.0, 0.0, 0.0]),
            ([0, 0, 50, -50], [0.25, 1.0, 1.0]),
        ] {
            let result = adjust_palette_color(base, gamut, offsets);
            assert_eq!(result.components()[3], 0.4);
            for (actual, expected) in result.encoded_rgb(gamut).into_iter().zip(rgb) {
                assert!(
                    (actual - expected).abs()
                        < if gamut == Gamut::Srgb {
                            0.5 / 255.0 + 0.00001
                        } else {
                            0.0001
                        },
                    "{gamut:?} {offsets:?}: {actual} versus {expected}"
                );
            }
        }
    }
}

#[test]
fn palette_search_preserves_locks_and_fills_each_requested_gamut() {
    let outside = Oklch::new(0.6, 1.2, 235.0, 0.31).unwrap();
    let gray = Oklch::new(0.4, 0.0, 89.0, 0.0).unwrap();
    let mut search = PaletteGenerator::default();
    for gamut in [Gamut::Srgb, Gamut::DisplayP3, Gamut::Rec2020] {
        for count in 2..=10 {
            let mut slots = vec![(gray, false); count];
            slots[0] = (outside, true);
            if count > 2 {
                slots[count - 1] = (gray, true);
            }
            let out = search.generate(&slots, gamut, 42);
            assert_eq!(out.len(), count);
            for (&(color, locked), &entry) in out.iter().zip(&slots) {
                if entry.1 {
                    assert_eq!((color, locked), entry);
                } else {
                    assert!(!locked && color.in_gamut(gamut));
                }
            }
        }
    }
    let locks = [(outside, true), (gray, true)];
    assert_eq!(search.generate(&locks, Gamut::Srgb, 0), locks);
}

#[test]
fn seeded_search_is_repeatable_and_explores_different_compositions() {
    let mut a = PaletteGenerator::default();
    let mut b = PaletteGenerator::default();
    let mut outputs = HashSet::new();
    let mut wide = false;
    let mut narrow_span = false;
    let mut broad_span = false;
    for seed in 0..32 {
        let colors = a.generate(&[], Gamut::Rec2020, seed);
        assert_eq!(colors, b.generate(&[], Gamut::Rec2020, seed));
        assert_eq!(colors.len(), 5);
        let mut min_l: f64 = 1.0;
        let mut max_l: f64 = 0.0;
        for (color, _) in &colors {
            let l = color.components()[0];
            min_l = min_l.min(l);
            max_l = max_l.max(l);
            wide |= !color.in_gamut(Gamut::Srgb);
        }
        broad_span |= max_l - min_l > 0.45;
        narrow_span |= max_l - min_l < 0.3;
        assert!(
            outputs.insert(
                colors
                    .iter()
                    .map(|v| v.0.components().map(f64::to_bits))
                    .collect::<Vec<_>>()
            )
        );
    }
    assert!(wide && narrow_span && broad_span);
}

#[test]
fn session_history_diversifies_selections_from_the_same_candidates() {
    let mut generator = PaletteGenerator::default();
    let mut distinct = HashSet::new();
    for _ in 0..12 {
        let palette = generator.generate(&[], Gamut::Srgb, 19);
        distinct.insert(
            palette
                .iter()
                .map(|slot| slot.0.components().map(f64::to_bits))
                .collect::<Vec<_>>(),
        );
    }
    assert!(distinct.len() >= 5);
}

#[test]
#[ignore = "run alone in release to measure dictionary initialization and repeated generation"]
fn palette_generation_timing() {
    let mut generator = PaletteGenerator::default();
    let now = Instant::now();
    let initial = generator.generate(&[], Gamut::DisplayP3, 0);
    let cold = now.elapsed();
    let mut palette = initial.repeat(2);
    let mut times = Vec::new();
    for seed in 0..100 {
        let start = Instant::now();
        palette = generator.generate(&palette, Gamut::DisplayP3, seed);
        times.push(start.elapsed());
    }
    times.sort();
    eprintln!(
        "first 5-color palette: {cold:?}, warm 10-color median: {:?}, p95: {:?}",
        times[50], times[94]
    );
}
