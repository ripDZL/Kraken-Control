//! Generated per-key colour patterns for the gen-2 "Custom" lighting effect.
//! Pure: key centres (in key units) in, one colour per key out; the caller
//! maps keys to LED slots and writes the table once (nothing animates).

pub type Rgb = [u8; 3];

/// Axis a gradient / rainbow runs along.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    LeftToRight,
    TopToBottom,
    Diagonal,
    FromCenter,
}

impl Direction {
    pub const ALL: [Direction; 4] = [Direction::LeftToRight, Direction::TopToBottom, Direction::Diagonal, Direction::FromCenter];

    pub fn label(self) -> &'static str {
        match self {
            Direction::LeftToRight => "Left → right",
            Direction::TopToBottom => "Top → bottom",
            Direction::Diagonal => "Diagonal",
            Direction::FromCenter => "From centre",
        }
    }
}

/// Key groups by factory HID code (`LayoutKey::code`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Wasd,
    Arrows,
    FKeys,
    Numbers,
    Modifiers,
}

impl Group {
    pub const ALL: [Group; 5] = [Group::Wasd, Group::Arrows, Group::FKeys, Group::Numbers, Group::Modifiers];

    pub fn label(self) -> &'static str {
        match self {
            Group::Wasd => "WASD",
            Group::Arrows => "Arrows",
            Group::FKeys => "F-keys",
            Group::Numbers => "Numbers",
            Group::Modifiers => "Modifiers",
        }
    }

    pub fn contains(self, code: u16) -> bool {
        match self {
            Group::Wasd => matches!(code, 0x1A | 0x04 | 0x16 | 0x07),
            Group::Arrows => (0x4F..=0x52).contains(&code),
            Group::FKeys => (0x3A..=0x45).contains(&code),
            Group::Numbers => (0x1E..=0x27).contains(&code),
            // Ctrl/Shift/Alt/GUI left + right, FN1 (255), FN2 (254).
            Group::Modifiers => (0xE0..=0xE7).contains(&code) || code == 254 || code == 255,
        }
    }
}

/// Position 0..=1 of each key centre along `dir` (0 = start of the axis).
pub fn positions(centres: &[[f32; 2]], dir: Direction) -> Vec<f32> {
    let (mut x0, mut x1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for [x, y] in centres {
        x0 = x0.min(*x);
        x1 = x1.max(*x);
        y0 = y0.min(*y);
        y1 = y1.max(*y);
    }
    let norm = |v: f32, lo: f32, hi: f32| if hi > lo { (v - lo) / (hi - lo) } else { 0.0 };
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    // Aspect-correct distance from the centre, scaled so the farthest key is 1.
    let far = centres.iter().map(|[x, y]| (x - cx).hypot(y - cy)).fold(0.0, f32::max);
    centres
        .iter()
        .map(|&[x, y]| {
            let t = match dir {
                Direction::LeftToRight => norm(x, x0, x1),
                Direction::TopToBottom => norm(y, y0, y1),
                Direction::Diagonal => {
                    // Weighted by extent so the diagonal runs corner to corner.
                    let (w, h) = (x1 - x0, y1 - y0);
                    if w + h > 0.0 {
                        ((x - x0) + (y - y0)) / (w + h)
                    } else {
                        0.0
                    }
                }
                Direction::FromCenter => {
                    if far > 0.0 {
                        (x - cx).hypot(y - cy) / far
                    } else {
                        0.0
                    }
                }
            };
            t.clamp(0.0, 1.0)
        })
        .collect()
}

fn lerp(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round().clamp(0.0, 255.0) as u8
}

/// Two-colour gradient along `dir`.
pub fn gradient(centres: &[[f32; 2]], dir: Direction, from: Rgb, to: Rgb) -> Vec<Rgb> {
    positions(centres, dir).into_iter().map(|t| [lerp(from[0], to[0], t), lerp(from[1], to[1], t), lerp(from[2], to[2], t)]).collect()
}

/// Rainbow along `dir`: hue runs from `offset` over `spread` turns of the
/// colour wheel (1.0 = full wheel). Full saturation and value.
pub fn rainbow(centres: &[[f32; 2]], dir: Direction, spread: f32, offset: f32) -> Vec<Rgb> {
    positions(centres, dir).into_iter().map(|t| hsv(offset + t * spread, 1.0, 1.0)).collect()
}

/// Named three-stop palettes for `palette`.
pub const PRESETS: [(&str, [Rgb; 3]); 6] = [
    ("Sunset", [[255, 150, 0], [255, 0, 110], [90, 0, 170]]),
    ("Ocean", [[0, 255, 200], [0, 120, 255], [0, 20, 130]]),
    ("Forest", [[190, 255, 0], [0, 200, 40], [0, 90, 70]]),
    ("Fire", [[255, 220, 0], [255, 90, 0], [210, 0, 0]]),
    ("Vapor", [[255, 0, 200], [140, 0, 255], [0, 220, 255]]),
    ("Ice", [[255, 255, 255], [110, 200, 255], [0, 70, 255]]),
];

/// Multi-stop gradient along `dir`, stops evenly spaced.
pub fn palette(centres: &[[f32; 2]], dir: Direction, stops: &[Rgb]) -> Vec<Rgb> {
    positions(centres, dir)
        .into_iter()
        .map(|t| match stops {
            [] => [0, 0, 0],
            [only] => *only,
            _ => {
                let seg = t * (stops.len() - 1) as f32;
                let i = (seg.floor() as usize).min(stops.len() - 2);
                let (a, b, u) = (stops[i], stops[i + 1], seg - i as f32);
                [lerp(a[0], b[0], u), lerp(a[1], b[1], u), lerp(a[2], b[2], u)]
            }
        })
        .collect()
}

/// Row index of each key (distinct centre heights, top = 0).
fn row_ranks(centres: &[[f32; 2]]) -> Vec<usize> {
    let ys: Vec<i32> = centres.iter().map(|c| (c[1] * 4.0).round() as i32).collect();
    let mut rows = ys.clone();
    rows.sort_unstable();
    rows.dedup();
    ys.iter().map(|y| rows.binary_search(y).unwrap_or(0)).collect()
}

/// Alternating `a` / `b`: by row (top → bottom), by 1-key column bands
/// (left → right), 1-key diagonal bands, or 1-key rings from the centre.
pub fn stripes(centres: &[[f32; 2]], dir: Direction, a: Rgb, b: Rgb) -> Vec<Rgb> {
    let x0 = centres.iter().map(|c| c[0]).fold(f32::MAX, f32::min);
    let y0 = centres.iter().map(|c| c[1]).fold(f32::MAX, f32::min);
    let (x1, y1) = (centres.iter().map(|c| c[0]).fold(f32::MIN, f32::max), centres.iter().map(|c| c[1]).fold(f32::MIN, f32::max));
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let rows = row_ranks(centres);
    centres
        .iter()
        .zip(rows)
        .map(|(&[x, y], row)| {
            let band = match dir {
                Direction::TopToBottom => row,
                Direction::LeftToRight => (x - x0).floor() as usize,
                Direction::Diagonal => ((x - x0) + (y - y0)).floor() as usize,
                Direction::FromCenter => (x - cx).hypot(y - cy).floor() as usize,
            };
            if band & 1 == 0 {
                a
            } else {
                b
            }
        })
        .collect()
}

/// Checkerboard: 1-key columns crossed with rows.
pub fn checker(centres: &[[f32; 2]], a: Rgb, b: Rgb) -> Vec<Rgb> {
    let x0 = centres.iter().map(|c| c[0]).fold(f32::MAX, f32::min);
    centres
        .iter()
        .zip(row_ranks(centres))
        .map(|(c, row)| if ((c[0] - x0).floor() as usize + row) & 1 == 0 { a } else { b })
        .collect()
}

/// One random vivid colour per key (deterministic for a seed).
pub fn random(n: usize, seed: u64) -> Vec<Rgb> {
    // xorshift64*; zero seed would stick at zero.
    let mut s = seed | 1;
    (0..n)
        .map(|_| {
            s ^= s >> 12;
            s ^= s << 25;
            s ^= s >> 27;
            let r = s.wrapping_mul(0x2545_F491_4F6C_DD1D);
            hsv((r >> 40) as f32 / (1u64 << 24) as f32, 1.0, 1.0)
        })
        .collect()
}

/// HSV to RGB; `h` in turns (wraps), `s` and `v` in 0..=1.
pub fn hsv(h: f32, s: f32, v: f32) -> Rgb {
    let h = h.rem_euclid(1.0) * 6.0;
    let (s, v) = (s.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
    let i = h.floor();
    let f = h - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    let (r, g, b) = match i as u8 % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    let c = |x: f32| (x * 255.0).round().clamp(0.0, 255.0) as u8;
    [c(r), c(g), c(b)]
}

#[cfg(test)]
mod tests {
    use super::*;

    // 3 keys in a row plus one key a row lower.
    const KEYS: [[f32; 2]; 4] = [[0.5, 0.5], [1.5, 0.5], [2.5, 0.5], [1.5, 1.5]];

    #[test]
    fn hsv_primaries() {
        assert_eq!(hsv(0.0, 1.0, 1.0), [255, 0, 0]);
        assert_eq!(hsv(1.0 / 3.0, 1.0, 1.0), [0, 255, 0]);
        assert_eq!(hsv(2.0 / 3.0, 1.0, 1.0), [0, 0, 255]);
        assert_eq!(hsv(1.0, 1.0, 1.0), [255, 0, 0]);
        assert_eq!(hsv(-0.5, 0.0, 1.0), [255, 255, 255]);
        assert_eq!(hsv(0.3, 1.0, 0.0), [0, 0, 0]);
    }

    #[test]
    fn positions_per_direction() {
        assert_eq!(positions(&KEYS, Direction::LeftToRight), vec![0.0, 0.5, 1.0, 0.5]);
        assert_eq!(positions(&KEYS, Direction::TopToBottom), vec![0.0, 0.0, 0.0, 1.0]);
        assert_eq!(positions(&KEYS, Direction::Diagonal), vec![0.0, 1.0 / 3.0, 2.0 / 3.0, 2.0 / 3.0]);
        let c = positions(&KEYS, Direction::FromCenter);
        assert!(c[1] < c[0] && (c[0] - 1.0).abs() < 1e-6 && (c[2] - 1.0).abs() < 1e-6);
        // Degenerate input: everything at the start.
        assert_eq!(positions(&[[3.0, 3.0]], Direction::LeftToRight), vec![0.0]);
        assert!(positions(&[], Direction::Diagonal).is_empty());
    }

    #[test]
    fn gradient_endpoints() {
        let g = gradient(&KEYS, Direction::LeftToRight, [255, 0, 0], [0, 0, 255]);
        assert_eq!(g, vec![[255, 0, 0], [128, 0, 128], [0, 0, 255], [128, 0, 128]]);
    }

    #[test]
    fn rainbow_spans_wheel() {
        let r = rainbow(&KEYS, Direction::LeftToRight, 2.0 / 3.0, 0.0);
        assert_eq!((r[0], r[2]), ([255, 0, 0], [0, 0, 255]));
        assert_eq!(r[1], [0, 255, 0]);
    }

    #[test]
    fn random_is_deterministic_and_vivid() {
        let a = random(50, 7);
        assert_eq!(a, random(50, 7));
        assert_ne!(a, random(50, 8));
        assert!(random(1, 0).len() == 1);
        // Full saturation/value: max channel 255, min channel 0.
        assert!(a.iter().all(|c| c.iter().max() == Some(&255) && c.iter().min() == Some(&0)));
    }

    #[test]
    fn palette_stops() {
        let p = palette(&KEYS, Direction::LeftToRight, &[[255, 0, 0], [0, 255, 0], [0, 0, 255]]);
        assert_eq!(p, vec![[255, 0, 0], [0, 255, 0], [0, 0, 255], [0, 255, 0]]);
        assert_eq!(palette(&KEYS, Direction::LeftToRight, &[[9, 9, 9]])[2], [9, 9, 9]);
        assert_eq!(palette(&KEYS, Direction::LeftToRight, &[])[0], [0, 0, 0]);
        assert!(PRESETS.iter().all(|(n, _)| !n.is_empty()));
    }

    #[test]
    fn stripes_and_checker() {
        let (a, b) = ([1, 1, 1], [2, 2, 2]);
        assert_eq!(stripes(&KEYS, Direction::LeftToRight, a, b), vec![a, b, a, b]);
        assert_eq!(stripes(&KEYS, Direction::TopToBottom, a, b), vec![a, a, a, b]);
        assert_eq!(stripes(&KEYS, Direction::Diagonal, a, b), vec![a, b, a, a]);
        assert_eq!(checker(&KEYS, a, b), vec![a, b, a, a]);
        // Rows are ranked, not measured: uneven row spacing still alternates.
        let uneven = [[0.5, 0.5], [0.5, 1.7], [0.5, 2.7]];
        assert_eq!(stripes(&uneven, Direction::TopToBottom, a, b), vec![a, b, a]);
    }

    #[test]
    fn groups() {
        assert!(Group::Wasd.contains(0x1A) && !Group::Wasd.contains(0x14));
        assert!(Group::Arrows.contains(0x52) && !Group::Arrows.contains(0x53));
        assert!(Group::FKeys.contains(0x3A) && Group::FKeys.contains(0x45) && !Group::FKeys.contains(0x46));
        assert!(Group::Numbers.contains(0x27) && !Group::Numbers.contains(0x28));
        assert!(Group::Modifiers.contains(0xE0) && Group::Modifiers.contains(255) && !Group::Modifiers.contains(0x39));
    }
}
