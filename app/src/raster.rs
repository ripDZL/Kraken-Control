//! CPU rasteriser for egui's triangle meshes (software rendering).
//!
//! Matches egui_glow: premultiplied alpha, vertex colour × texture in gamma
//! space, blend `src + dst * (1 - src.a)`. The target is opaque 0x00RRGGBB
//! (softbuffer's format). Rows are split into bands, one thread each.

use std::collections::HashMap;

use eframe::egui::epaint::{ClippedPrimitive, ImageData, ImageDelta, Primitive, Vertex};
use eframe::egui::{Color32, TextureFilter, TextureId, TexturesDelta};

struct Texture {
    w: usize,
    h: usize,
    px: Vec<Color32>,
    linear: bool,
}

#[derive(Default)]
pub struct Raster {
    textures: HashMap<TextureId, Texture>,
}

impl Raster {
    /// New and changed textures; call before `paint`.
    pub fn set_textures(&mut self, delta: &TexturesDelta) {
        for (id, d) in &delta.set {
            self.set(*id, d);
        }
    }

    /// Textures egui no longer needs; call after `paint`.
    pub fn free_textures(&mut self, delta: &TexturesDelta) {
        for id in &delta.free {
            self.textures.remove(id);
        }
    }

    fn set(&mut self, id: TextureId, d: &ImageDelta) {
        let (w, h, px): (usize, usize, Vec<Color32>) = match &d.image {
            ImageData::Color(img) => (img.size[0], img.size[1], img.pixels.clone()),
            ImageData::Font(f) => (f.size[0], f.size[1], f.srgba_pixels(None).collect()),
        };
        let linear = d.options.magnification == TextureFilter::Linear;
        match d.pos {
            None => {
                self.textures.insert(id, Texture { w, h, px, linear });
            }
            Some([x0, y0]) => {
                let Some(t) = self.textures.get_mut(&id) else { return };
                for y in 0..h.min(t.h.saturating_sub(y0)) {
                    let n = w.min(t.w.saturating_sub(x0));
                    let dst = (y0 + y) * t.w + x0;
                    t.px[dst..dst + n].copy_from_slice(&px[y * w..y * w + n]);
                }
            }
        }
    }

    /// Draws `prims` into `buf` (`width` × `height`, row-major), cleared to `clear` first.
    pub fn paint(&self, buf: &mut [u32], width: usize, height: usize, ppp: f32, prims: &[ClippedPrimitive], clear: Color32) {
        if width == 0 || height == 0 || buf.len() < width * height {
            return;
        }
        let buf = &mut buf[..width * height];
        // Small windows don't gain from more threads; thread start-up is ~20 µs.
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).clamp(1, 8).min(height.div_ceil(64));
        if threads <= 1 {
            self.paint_band(buf, width, 0, ppp, prims, clear);
            return;
        }
        let band_rows = height.div_ceil(threads);
        std::thread::scope(|s| {
            for (i, band) in buf.chunks_mut(band_rows * width).enumerate() {
                s.spawn(move || self.paint_band(band, width, i * band_rows, ppp, prims, clear));
            }
        });
    }

    /// One horizontal band: rows `y0 .. y0 + band.len() / width`.
    fn paint_band(&self, band: &mut [u32], width: usize, y0: usize, ppp: f32, prims: &[ClippedPrimitive], clear: Color32) {
        band.fill(rgb(clear));
        let rows = band.len() / width;
        let band_rect = Clip { x0: 0, x1: width as i32, y0: y0 as i32, y1: (y0 + rows) as i32 };
        for p in prims {
            let Primitive::Mesh(mesh) = &p.primitive else { continue };
            let c = p.clip_rect;
            let clip = Clip {
                x0: (c.min.x * ppp).round() as i32,
                x1: (c.max.x * ppp).round() as i32,
                y0: (c.min.y * ppp).round() as i32,
                y1: (c.max.y * ppp).round() as i32,
            }
            .intersect(&band_rect);
            if clip.is_empty() {
                continue;
            }
            let tex = self.textures.get(&mesh.texture_id);
            for t in mesh.indices.as_chunks::<3>().0 {
                let v = |i: u32| mesh.vertices.get(i as usize);
                if let (Some(a), Some(b), Some(c)) = (v(t[0]), v(t[1]), v(t[2])) {
                    triangle(band, width, y0 as i32, &clip, ppp, [a, b, c], tex);
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct Clip {
    x0: i32,
    x1: i32,
    y0: i32,
    y1: i32,
}

impl Clip {
    fn intersect(&self, o: &Clip) -> Clip {
        Clip { x0: self.x0.max(o.x0), x1: self.x1.min(o.x1), y0: self.y0.max(o.y0), y1: self.y1.min(o.y1) }
    }
    fn is_empty(&self) -> bool {
        self.x0 >= self.x1 || self.y0 >= self.y1
    }
}

fn rgb(c: Color32) -> u32 {
    (c.r() as u32) << 16 | (c.g() as u32) << 8 | c.b() as u32
}

/// Premultiplied `src` over opaque `dst`.
#[inline(always)]
fn blend(dst: &mut u32, s: [f32; 4]) {
    let a = s[3];
    if a >= 254.5 {
        *dst = (s[0] as u32) << 16 | (s[1] as u32) << 8 | s[2] as u32;
        return;
    }
    if a <= 0.0 && s[0] <= 0.0 && s[1] <= 0.0 && s[2] <= 0.0 {
        return;
    }
    let k = 1.0 - a / 255.0;
    let d = *dst;
    let ch = |src: f32, shift: u32| -> u32 { (src + ((d >> shift) & 255) as f32 * k + 0.5).min(255.0) as u32 };
    *dst = ch(s[0], 16) << 16 | ch(s[1], 8) << 8 | ch(s[2], 0);
}

fn premul(c: Color32) -> [f32; 4] {
    [c.r() as f32, c.g() as f32, c.b() as f32, c.a() as f32]
}

/// Texture sample at normalised `uv`, premultiplied 0..255.
#[inline(always)]
fn sample(t: &Texture, u: f32, v: f32) -> [f32; 4] {
    let (w, h) = (t.w as i32, t.h as i32);
    if w == 0 || h == 0 {
        return [255.0; 4];
    }
    let at = |x: i32, y: i32| premul(t.px[(y.clamp(0, h - 1) * w + x.clamp(0, w - 1)) as usize]);
    if !t.linear {
        return at(ifloor(u * w as f32), ifloor(v * h as f32));
    }
    let x = u * w as f32 - 0.5;
    let y = v * h as f32 - 0.5;
    let (xi, yi) = (ifloor(x), ifloor(y));
    let (fx, fy) = (x - xi as f32, y - yi as f32);
    // Exactly on a texel centre (text drawn 1:1): one fetch.
    if fx == 0.0 && fy == 0.0 {
        return at(xi, yi);
    }
    let (a, b, c, d) = (at(xi, yi), at(xi + 1, yi), at(xi, yi + 1), at(xi + 1, yi + 1));
    let mut out = [0.0; 4];
    for i in 0..4 {
        let top = a[i] + (b[i] - a[i]) * fx;
        let bot = c[i] + (d[i] - c[i]) * fx;
        out[i] = top + (bot - top) * fy;
    }
    out
}

/// floor / ceil to i32 without libm calls (f32::floor is a function call on baseline x86-64).
#[inline(always)]
fn ifloor(x: f32) -> i32 {
    // `as` saturates (and maps NaN to 0); stay saturating so huge values can't wrap.
    let i = x as i32;
    if (i as f32) > x {
        i.saturating_sub(1)
    } else {
        i
    }
}

#[inline(always)]
fn iceil(x: f32) -> i32 {
    let i = x as i32;
    if (i as f32) < x {
        i.saturating_add(1)
    } else {
        i
    }
}

/// Edge function: > 0 when `p` is on the inner side of `a -> b` (for area > 0).
#[inline(always)]
fn edge(ax: f32, ay: f32, bx: f32, by: f32, px: f32, py: f32) -> f32 {
    (bx - ax) * (py - ay) - (by - ay) * (px - ax)
}

/// Top-left fill rule: pixels exactly on a shared edge belong to one triangle only,
/// so semi-transparent quads don't get a darker diagonal.
#[inline(always)]
fn top_left(ax: f32, ay: f32, bx: f32, by: f32) -> bool {
    let (dx, dy) = (bx - ax, by - ay);
    dy < 0.0 || (dy == 0.0 && dx > 0.0)
}

fn triangle(band: &mut [u32], width: usize, band_y0: i32, clip: &Clip, ppp: f32, v: [&Vertex; 3], tex: Option<&Texture>) {
    let p = |i: usize| (v[i].pos.x * ppp, v[i].pos.y * ppp);
    // Most triangles lie in other threads' rows: reject before any setup.
    let (ymin, ymax) = (v[0].pos.y.min(v[1].pos.y).min(v[2].pos.y) * ppp, v[0].pos.y.max(v[1].pos.y).max(v[2].pos.y) * ppp);
    if iceil(ymax) <= clip.y0 || ifloor(ymin) >= clip.y1 {
        return;
    }
    let (mut a, mut b, c) = (0usize, 1usize, 2usize);
    let (ax, ay) = p(a);
    let (bx, by) = p(b);
    let (cx, cy) = p(c);
    let area = edge(ax, ay, bx, by, cx, cy);
    if area.abs() < 1e-6 {
        return;
    }
    if area < 0.0 {
        std::mem::swap(&mut a, &mut b);
    }
    let (ax, ay) = p(a);
    let (bx, by) = p(b);
    let area = area.abs();

    let x0 = ifloor(ax.min(bx).min(cx)).max(clip.x0);
    let x1 = iceil(ax.max(bx).max(cx)).min(clip.x1);
    let y0 = ifloor(ay.min(by).min(cy)).max(clip.y0);
    let y1 = iceil(ay.max(by).max(cy)).min(clip.y1);
    if x0 >= x1 || y0 >= y1 {
        return;
    }

    // Weights: wa = edge(b,c), wb = edge(c,a), wc = edge(a,b).
    let (tl_a, tl_b, tl_c) = (top_left(bx, by, cx, cy), top_left(cx, cy, ax, ay), top_left(ax, ay, bx, by));
    let (sa, sb, sc) = (-(cy - by), -(ay - cy), -(by - ay));
    let inside = |w: f32, tl: bool| w > 0.0 || (w == 0.0 && tl);

    let (va, vb, vc) = (v[a], v[b], v[c]);
    let ca = premul(va.color);
    let cb = premul(vb.color);
    let cc = premul(vc.color);
    let same_uv = va.uv == vb.uv && vb.uv == vc.uv;
    // Solid shapes (one colour, one texel): no per-pixel interpolation.
    let flat = if va.color == vb.color && vb.color == vc.color && same_uv {
        let t = tex.map_or([255.0; 4], |t| sample(t, va.uv.x, va.uv.y));
        Some(mul(ca, t))
    } else {
        None
    };
    let inv = 1.0 / area;
    // Text: one colour, texture coordinates change linearly along the row.
    let one_color = va.color == vb.color && vb.color == vc.color;
    let (dudx, dvdx) = ((va.uv.x * sa + vb.uv.x * sb + vc.uv.x * sc) * inv, (va.uv.y * sa + vb.uv.y * sb + vc.uv.y * sc) * inv);

    let n = x1 - x0;
    // Per-triangle reciprocals: the per-row span maths then needs no division.
    let recip = |s: f32| if s != 0.0 { 1.0 / s } else { 0.0 };
    let (inv_sa, inv_sb, inv_sc) = (recip(sa), recip(sb), recip(sc));
    let opaque = flat.filter(|c| c[3] >= 254.5).map(|c| (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32);
    for py in y0..y1 {
        let fy = py as f32 + 0.5;
        let fx0 = x0 as f32 + 0.5;
        let wa0 = edge(bx, by, cx, cy, fx0, fy);
        let wb0 = edge(cx, cy, ax, ay, fx0, fy);
        let wc0 = edge(ax, ay, bx, by, fx0, fy);
        // Pixels i (from x0) where each edge can be >= 0, with 1 px slack;
        // the exact test runs only at the span ends. Big fan-triangulated
        // shapes would otherwise test their whole bounding box many times.
        let (mut lo, mut hi) = (0i32, n - 1);
        for (w0, s, inv_s) in [(wa0, sa, inv_sa), (wb0, sb, inv_sb), (wc0, sc, inv_sc)] {
            if s > 0.0 {
                lo = lo.max(ifloor(-w0 * inv_s).saturating_sub(1));
            } else if s < 0.0 {
                hi = hi.min(iceil(-w0 * inv_s).saturating_add(1));
            } else if w0 < 0.0 {
                lo = n;
            }
        }
        let at = |i: i32| {
            let f = i as f32;
            (wa0 + sa * f, wb0 + sb * f, wc0 + sc * f)
        };
        let covered = |i: i32| {
            let (wa, wb, wc) = at(i);
            inside(wa, tl_a) && inside(wb, tl_b) && inside(wc, tl_c)
        };
        let (mut first, mut last) = (lo.max(0), hi.min(n - 1));
        while first <= last && !covered(first) {
            first += 1;
        }
        while last >= first && !covered(last) {
            last -= 1;
        }
        if first > last {
            continue;
        }
        // A triangle's row is one interval: everything between first and last is covered.
        let row = ((py - band_y0) as usize) * width + x0 as usize;
        let span = &mut band[row + first as usize..=row + last as usize];
        match (opaque, flat) {
            (Some(px), _) => span.fill(px),
            (None, Some(col)) => span.iter_mut().for_each(|d| blend(d, col)),
            (None, None) if one_color && tex.is_some() => {
                let t = tex.unwrap_or_else(|| unreachable!());
                let (wa, wb, wc) = at(first);
                let (la, lb, lc) = (wa * inv, wb * inv, wc * inv);
                let mut u = va.uv.x * la + vb.uv.x * lb + vc.uv.x * lc;
                let mut v = va.uv.y * la + vb.uv.y * lb + vc.uv.y * lc;
                for dst in span.iter_mut() {
                    blend(dst, mul(ca, sample(t, u, v)));
                    u += dudx;
                    v += dvdx;
                }
            }
            (None, None) => {
                for (k, dst) in span.iter_mut().enumerate() {
                    let (wa, wb, wc) = at(first + k as i32);
                    let (la, lb, lc) = (wa * inv, wb * inv, wc * inv);
                    let mut col = [0.0; 4];
                    for i in 0..4 {
                        col[i] = ca[i] * la + cb[i] * lb + cc[i] * lc;
                    }
                    if let Some(t) = tex {
                        let u = va.uv.x * la + vb.uv.x * lb + vc.uv.x * lc;
                        let v = va.uv.y * la + vb.uv.y * lb + vc.uv.y * lc;
                        col = mul(col, sample(t, u, v));
                    }
                    blend(dst, col);
                }
            }
        }
    }
}

/// Vertex colour × texture colour, both premultiplied 0..255.
#[inline(always)]
fn mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [a[0] * b[0] / 255.0, a[1] * b[1] / 255.0, a[2] * b[2] / 255.0, a[3] * b[3] / 255.0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::epaint::{Mesh, WHITE_UV};
    use eframe::egui::{pos2, Rect};

    fn quad(rect: Rect, color: Color32) -> ClippedPrimitive {
        let mut m = Mesh::default();
        m.add_colored_rect(rect, color);
        ClippedPrimitive { clip_rect: Rect::EVERYTHING, primitive: Primitive::Mesh(m) }
    }

    fn white_texture() -> Raster {
        let mut r = Raster::default();
        let img = eframe::egui::ColorImage::new([1, 1], Color32::WHITE);
        r.textures.insert(TextureId::default(), Texture { w: 1, h: 1, px: img.pixels, linear: true });
        r
    }

    /// A half-transparent quad blends once per pixel: no darker diagonal where its triangles meet.
    #[test]
    fn quad_has_no_seam() {
        let r = white_texture();
        let (w, h) = (32, 32);
        let mut buf = vec![0u32; w * h];
        let half = Color32::from_rgba_premultiplied(128, 128, 128, 128);
        r.paint(&mut buf, w, h, 1.0, &[quad(Rect::from_min_max(pos2(0.0, 0.0), pos2(32.0, 32.0)), half)], Color32::BLACK);
        let first = buf[0];
        assert!(buf.iter().all(|&p| p == first), "uneven fill");
        assert_eq!(first, 0x808080);
    }

    /// Opaque fill covers exactly its pixel rectangle, in every band.
    #[test]
    fn rect_edges_are_exact() {
        let r = white_texture();
        let (w, h) = (100, 300);
        let mut buf = vec![0u32; w * h];
        r.paint(&mut buf, w, h, 1.0, &[quad(Rect::from_min_max(pos2(10.0, 20.0), pos2(50.0, 280.0)), Color32::RED)], Color32::BLACK);
        for y in 0..h {
            for x in 0..w {
                let inside = (10..50).contains(&x) && (20..280).contains(&y);
                assert_eq!(buf[y * w + x] == 0xFF0000, inside, "pixel {x},{y}");
            }
        }
    }

    /// Nearly flat and far-away triangles (egui feathering, huge shapes) neither panic nor drop pixels.
    #[test]
    fn extreme_triangles_are_safe() {
        let r = white_texture();
        let (w, h) = (64, 64);
        let mut buf = vec![0u32; w * h];
        let v = |x: f32, y: f32| Vertex { pos: pos2(x, y), uv: WHITE_UV, color: Color32::WHITE };
        let mut m = Mesh::default();
        for (a, b, c) in [
            (v(0.0, 0.0), v(1e9, 0.0001), v(0.0, 64.0)),
            (v(-1e9, 10.0), v(1e9, 10.00001), v(0.0, 50.0)),
            (v(0.0, 0.0), v(64.0, 1e-7), v(64.0, 0.0)),
            (v(f32::MAX, 0.0), v(0.0, f32::MAX), v(0.0, 0.0)),
        ] {
            let i = m.vertices.len() as u32;
            m.vertices.extend([a, b, c]);
            m.indices.extend([i, i + 1, i + 2]);
        }
        r.paint(&mut buf, w, h, 1.0, &[ClippedPrimitive { clip_rect: Rect::EVERYTHING, primitive: Primitive::Mesh(m) }], Color32::BLACK);
        // The first triangle alone covers the left half of the buffer.
        assert!(buf[32 * w + 10] == 0xFFFFFF);
    }

    /// Clip rects are honoured.
    #[test]
    fn clip_rect_limits_drawing() {
        let r = white_texture();
        let (w, h) = (40, 40);
        let mut buf = vec![0u32; w * h];
        let mut p = quad(Rect::from_min_max(pos2(0.0, 0.0), pos2(40.0, 40.0)), Color32::GREEN);
        p.clip_rect = Rect::from_min_max(pos2(5.0, 5.0), pos2(15.0, 15.0));
        r.paint(&mut buf, w, h, 1.0, &[p], Color32::BLACK);
        let lit = buf.iter().filter(|&&px| px != 0).count();
        assert_eq!(lit, 100);
    }
}
