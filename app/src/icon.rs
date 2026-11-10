//! App icon: `res/kraken.ico` (generated from `res/kraken.png`; 16–64 px as
//! 32-bit DIBs, 256 px as PNG). The exe icon comes from the resource linked
//! by build.rs; window and tray icons are created from this embedded copy.

pub const ICO: &[u8] = include_bytes!("../res/kraken.ico");

fn rd16(o: usize) -> usize {
    u16::from_le_bytes([ICO[o], ICO[o + 1]]) as usize
}

fn rd32(o: usize) -> usize {
    u32::from_le_bytes([ICO[o], ICO[o + 1], ICO[o + 2], ICO[o + 3]]) as usize
}

/// `(pixel size, image data)` per entry, as stored in the .ico.
pub fn entries() -> impl Iterator<Item = (u32, &'static [u8])> {
    (0..rd16(4)).map(|i| {
        let e = 6 + 16 * i;
        let w = match ICO[e] {
            0 => 256,
            w => w as u32,
        };
        let (len, off) = (rd32(e + 8), rd32(e + 12));
        (w, &ICO[off..off + len])
    })
}

/// Smallest entry of at least `size` px (else the largest): input for
/// `CreateIconFromResourceEx`, which accepts DIB and PNG entries.
#[allow(dead_code)] // tray (Windows)
pub fn best(size: u32) -> &'static [u8] {
    let mut all: Vec<_> = entries().collect();
    all.sort_by_key(|(w, _)| *w);
    all.iter().find(|(w, _)| *w >= size).or(all.last()).map(|(_, d)| *d).expect("icon entries")
}

/// Straight RGBA of the `size` px DIB entry (rows top-down).
pub fn rgba(size: u32) -> Option<Vec<u8>> {
    let (_, d) = entries().find(|(w, d)| *w == size && d.starts_with(&[40, 0, 0, 0]))?;
    let n = size as usize;
    let mut out = Vec::with_capacity(n * n * 4);
    for y in 0..n {
        let row = 40 + (n - 1 - y) * n * 4; // DIB rows are bottom-up BGRA
        for px in d[row..row + n * 4].as_chunks::<4>().0 {
            out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_and_pixels() {
        let sizes: Vec<u32> = entries().map(|(w, _)| w).collect();
        assert_eq!(sizes, [16, 20, 24, 32, 40, 48, 64, 256]);
        let p = rgba(64).unwrap();
        assert_eq!(p.len(), 64 * 64 * 4);
        let px = |x: usize, y: usize| &p[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
        assert!(px(0, 0)[3] < 32, "rounded corner ~transparent");
        assert_eq!(px(16, 16), [119, 30, 173, 255], "purple wing, upper left (rows top-down)");
        assert_eq!(px(32, 56), [255, 187, 65, 255], "orange background, bottom");
        assert!(best(16).starts_with(&[40, 0, 0, 0]));
        assert!(best(300).starts_with(b"\x89PNG"));
    }
}
