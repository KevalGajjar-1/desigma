//! Separable gaussian blur approximated by three box passes, operating on
//! premultiplied RGBA8 pixmaps. Out-of-bounds samples are transparent.
//!
//! Speed comes from three exact or tightly bounded tricks:
//!
//! * zero-region skip: the nonzero bounding box of the input is found first and
//!   every pass only touches that box grown by the radii applied so far, so a
//!   small shape inside a large region costs nothing for the empty space. The
//!   result matches a full-frame blur (blurring zeros gives zeros);
//! * the vertical pass walks whole rows (cache friendly, 4-lane inner loops
//!   without bounds checks) instead of striding down single columns;
//! * for large sigma the image is averaged down by 2x or 4x, blurred there and
//!   bilinearly upsampled. The thresholds are chosen so the result stays within
//!   2 levels of the full resolution blur (see the tests).

use tiny_skia::Pixmap;

type Px = [f32; 4];

/// Box sizes whose successive application approximates a gaussian of `sigma`.
fn boxes_for_gauss(sigma: f32, n: usize) -> Vec<usize> {
    let nf = n as f32;
    let w_ideal = (12.0 * sigma * sigma / nf + 1.0).sqrt();
    let mut wl = w_ideal.floor() as i32;
    if wl % 2 == 0 {
        wl -= 1;
    }
    wl = wl.max(1);
    let wu = wl + 2;
    let wlf = wl as f32;
    let m_ideal = (12.0 * sigma * sigma - nf * wlf * wlf - 4.0 * nf * wlf - 3.0 * nf) / (-4.0 * wlf - 4.0);
    let m = m_ideal.round().max(0.0) as usize;
    (0..n).map(|i| if i < m { wl as usize } else { wu as usize }).collect()
}

/// Box radii (zero radii dropped) for a gaussian of `sigma`, as `(radius, 0)`.
fn radii_for(sigma: f32) -> Vec<(usize, f32)> {
    boxes_for_gauss(sigma, 3).into_iter().map(|s| (s - 1) / 2).filter(|r| *r > 0).map(|r| (r, 0.0)).collect()
}

/// Variance of the discrete box passes `radii_for(sigma)` applies.
fn box_variance(sigma: f32) -> f32 {
    boxes_for_gauss(sigma, 3).iter().map(|w| ((w * w) as f32 - 1.0) / 12.0).sum()
}

/// Variance of a box with core radius `r` and `alpha` weight on the two
/// samples just outside it.
fn frac_box_variance(r: usize, alpha: f32) -> f32 {
    let rf = r as f32;
    let core = rf * (rf + 1.0) * (2.0 * rf + 1.0) / 3.0;
    (core + 2.0 * alpha * (rf + 1.0) * (rf + 1.0)) / (2.0 * rf + 1.0 + 2.0 * alpha)
}

/// Three equal fractional boxes whose combined variance is `target`.
fn frac_radii_for_variance(target: f32) -> Vec<(usize, f32)> {
    let each = (target / 3.0).max(0.0);
    let mut r = 0usize;
    while frac_box_variance(r + 1, 0.0) <= each {
        r += 1;
    }
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..24 {
        let mid = 0.5 * (lo + hi);
        if frac_box_variance(r, mid) < each {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let alpha = 0.5 * (lo + hi);
    if r == 0 && alpha < 1e-3 {
        return Vec::new();
    }
    vec![(r, alpha); 3]
}

/// What the caller guarantees about the pixmap border.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edges {
    /// Pixels within 3 sigma of the pixmap border are never looked at (the
    /// caller blurs a region padded by at least that much and crops it). The
    /// reduced resolution grid can then be used from a small sigma.
    Margined,
    /// Content may touch the border and the border pixels matter (the blur of
    /// a backdrop or a layer). The coarse grid models the loss of mass out of
    /// the pixmap less exactly, so it is only used for larger sigmas.
    Exact,
}

/// `(minimum sigma, downsampling factor)`, largest factor first. Chosen with
/// `scan_thresholds` so that the result stays within 2 levels of the full
/// resolution blur for every test scene. The choice depends on sigma and the
/// caller's [`Edges`] guarantee alone, never on the region being blurred, which
/// keeps tiles and full renders pixel identical.
const DOWN_MARGINED: [(f32, usize); 2] = [(12.0, 4), (8.0, 2)];
const DOWN_EXACT: [(f32, usize); 2] = [(30.0, 4), (22.0, 2)];

fn down_factor(sigma: f32, edges: Edges) -> usize {
    let table: &[(f32, usize)] = if edges == Edges::Margined { &DOWN_MARGINED } else { &DOWN_EXACT };
    table.iter().find(|(s, _)| sigma >= *s).map_or(1, |(_, k)| *k)
}

/// Half-open rectangle `[x0, x1) x [y0, y1)` in pixels.
#[derive(Clone, Copy, Debug)]
struct Span {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

/// Bounding box of non-transparent (any byte nonzero) pixels.
fn nonzero_bbox(data: &[u8], w: usize, h: usize) -> Option<Span> {
    let (mut x0, mut x1) = (usize::MAX, 0usize);
    let (mut y0, mut y1) = (usize::MAX, 0usize);
    let solid = |c: &[u8; 4]| u32::from_ne_bytes(*c) != 0;
    for (y, row) in data.chunks_exact(w * 4).take(h).enumerate() {
        let Some(first) = row.as_chunks::<4>().0.iter().position(solid) else { continue };
        let last = row.as_chunks::<4>().0.iter().rposition(solid).unwrap_or(first);
        x0 = x0.min(first);
        x1 = x1.max(last + 1);
        y0 = y0.min(y);
        y1 = y + 1;
    }
    (y1 > 0).then_some(Span { x0, y0, x1, y1 })
}

/// Independent lines processed together: rows for the horizontal stage,
/// columns for the vertical stage. A sliding window sum is a serial dependency
/// chain along its line; interleaving independent lines keeps the float units
/// busy, and the lane arrays are what the compiler turns into SIMD.
const H_LANES: usize = 8;
const V_LANES: usize = 16;
type HLane = [f32; 4 * H_LANES];
type VLane = [f32; 4 * V_LANES];

/// One box pass along a line of `src.len()` samples, for `N / 4` independent
/// lines at once (`N` floats per sample: RGBA of every lane). The window has
/// weight 1 on the `2r+1` core samples and `alpha` on the two samples just
/// outside it (`alpha == 0` is a plain box); samples outside the line are zero.
/// `dst` is fully overwritten.
fn box_pass<const N: usize>(src: &[[f32; N]], dst: &mut [[f32; N]], r: usize, alpha: f32) {
    let n = src.len();
    debug_assert_eq!(dst.len(), n);
    let inv = 1.0 / (2.0 * r as f32 + 1.0 + 2.0 * alpha);
    let zero = [0.0f32; N];
    let mut sum = [0.0f32; N];
    for s in src.iter().take((r + 1).min(n)) {
        for k in 0..N {
            sum[k] += s[k];
        }
    }
    for i in 0..n {
        let out = &mut dst[i];
        if alpha > 0.0 {
            let left = if i > r { &src[i - r - 1] } else { &zero };
            let right = src.get(i + r + 1).unwrap_or(&zero);
            for k in 0..N {
                out[k] = (sum[k] + alpha * (left[k] + right[k])) * inv;
            }
        } else {
            for k in 0..N {
                out[k] = sum[k] * inv;
            }
        }
        if let Some(add) = src.get(i + r + 1) {
            for k in 0..N {
                sum[k] += add[k];
            }
        }
        if i >= r {
            let rem = &src[i - r];
            for k in 0..N {
                sum[k] -= rem[k];
            }
        }
    }
}

/// Runs every box pass on `lines` in turn (ping-pong between `a` and `b`);
/// returns whether the result ended in `b`.
fn run_passes<const N: usize>(a: &mut [[f32; N]], b: &mut [[f32; N]], radii: &[(usize, f32)]) -> bool {
    let mut in_b = false;
    for &(r, alpha) in radii {
        if in_b {
            box_pass(b, a, r, alpha);
        } else {
            box_pass(a, b, r, alpha);
        }
        in_b = !in_b;
    }
    in_b
}

/// Runs the box passes on `buf` (`bw x bh`). `nz` bounds the nonzero content;
/// everything outside it must be zero, and the buffer must be large enough (or
/// end at the true image edge) so growing `nz` by the radii never leaves it.
///
/// Box passes along different axes commute (each is a linear operator on its
/// own axis, edge truncation included), so all horizontal passes run first,
/// then all vertical ones. Each stage works on small batches that stay in
/// cache while their passes run, so the big buffer is read and written once
/// per stage rather than once per pass.
fn blur_core(buf: &mut [Px], bw: usize, bh: usize, radii: &[(usize, f32)], nz: Span) {
    let reach: usize = radii.iter().map(|(r, a)| r + usize::from(*a > 0.0)).sum();
    if radii.is_empty() {
        return;
    }
    let (x0, x1) = (nz.x0.saturating_sub(reach), (nz.x1 + reach).min(bw));
    let (y0, y1) = (nz.y0.saturating_sub(reach), (nz.y1 + reach).min(bh));

    // horizontal stage: only rows that hold content
    let n = x1 - x0;
    let (mut a, mut b) = (vec![[0.0f32; 4 * H_LANES]; n], vec![[0.0f32; 4 * H_LANES]; n]);
    for ys in (nz.y0..nz.y1).step_by(H_LANES) {
        let m = (nz.y1 - ys).min(H_LANES);
        for j in 0..m {
            let row = &buf[(ys + j) * bw + x0..(ys + j) * bw + x1];
            for (lane, p) in a.iter_mut().zip(row) {
                lane[j * 4..j * 4 + 4].copy_from_slice(p);
            }
        }
        let res: &[HLane] = if run_passes(&mut a, &mut b, radii) { &b } else { &a };
        for j in 0..m {
            let row = &mut buf[(ys + j) * bw + x0..(ys + j) * bw + x1];
            for (lane, p) in res.iter().zip(row.iter_mut()) {
                p.copy_from_slice(&lane[j * 4..j * 4 + 4]);
            }
        }
    }

    // vertical stage: column strips over every row the blur reaches
    let n = y1 - y0;
    let (mut a, mut b) = (vec![[0.0f32; 4 * V_LANES]; n], vec![[0.0f32; 4 * V_LANES]; n]);
    for xs in (x0..x1).step_by(V_LANES) {
        let m = (x1 - xs).min(V_LANES);
        for (i, lane) in a.iter_mut().enumerate() {
            let row = &buf[(y0 + i) * bw + xs..(y0 + i) * bw + xs + m];
            for (c, p) in row.iter().enumerate() {
                lane[c * 4..c * 4 + 4].copy_from_slice(p);
            }
        }
        let res: &[VLane] = if run_passes(&mut a, &mut b, radii) { &b } else { &a };
        for (i, lane) in res.iter().enumerate() {
            let row = &mut buf[(y0 + i) * bw + xs..(y0 + i) * bw + xs + m];
            for (c, p) in row.iter_mut().enumerate() {
                p.copy_from_slice(&lane[c * 4..c * 4 + 4]);
            }
        }
    }
}

fn to_u8(v: f32) -> u8 {
    (v + 0.5).clamp(0.0, 255.0) as u8
}

/// Rounds one pixel to 8 bit, keeping the premultiplied invariant (rounding can
/// push a colour channel above alpha).
fn store_px(d: &mut [u8], v: &Px) {
    let a = to_u8(v[3]);
    d[0] = to_u8(v[0]).min(a);
    d[1] = to_u8(v[1]).min(a);
    d[2] = to_u8(v[2]).min(a);
    d[3] = a;
}

/// Keeps the premultiplied invariant (rounding can push colour above alpha).
fn fix_premul(data: &mut [u8]) {
    for px in data.as_chunks_mut::<4>().0.iter_mut() {
        let a = px[3];
        for c in px.iter_mut().take(3) {
            if *c > a {
                *c = a;
            }
        }
    }
}

static REFERENCE_MODE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Routes every blur through the original implementation (benchmarks, tests).
pub fn set_reference_mode(on: bool) {
    REFERENCE_MODE.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn reference_mode() -> bool {
    REFERENCE_MODE.load(std::sync::atomic::Ordering::Relaxed)
}

/// Blurs `pm` in place with a gaussian of standard deviation `sigma` (pixels).
#[cfg(test)]
fn gaussian_blur(pm: &mut Pixmap, sigma: f32) {
    gaussian_blur_at(pm, sigma, (0, 0), Edges::Exact);
}

/// Like [`gaussian_blur`] where pixel (0, 0) of `pm` sits at `origin` in some
/// shared absolute pixel grid. Large blurs run at reduced resolution on a grid
/// anchored to that absolute grid, so blurring the same scene content in
/// pixmaps with different origins gives identical pixels.
pub fn gaussian_blur_at(pm: &mut Pixmap, sigma: f32, origin: (i32, i32), edges: Edges) {
    if sigma < 0.3 || pm.width() == 0 || pm.height() == 0 {
        return;
    }
    if reference_mode() {
        return gaussian_blur_reference(pm, sigma);
    }
    let (w, h) = (pm.width() as usize, pm.height() as usize);
    let Some(bb) = nonzero_bbox(pm.data(), w, h) else { return };
    let k = down_factor(sigma, edges);
    let phase = (origin.0.rem_euclid(k as i32) as usize, origin.1.rem_euclid(k as i32) as usize);
    blur_with_factor(pm, w, h, bb, sigma, k, phase);
}

fn blur_with_factor(pm: &mut Pixmap, w: usize, h: usize, bb: Span, sigma: f32, k: usize, phase: (usize, usize)) {
    match k {
        1 => blur_full(pm, w, h, bb, sigma),
        k => blur_down(pm, w, h, bb, sigma, k, phase),
    }
}

/// Full resolution blur restricted to the (grown) nonzero box.
fn blur_full(pm: &mut Pixmap, w: usize, h: usize, bb: Span, sigma: f32) {
    let radii = radii_for(sigma);
    if radii.is_empty() {
        return;
    }
    let total: usize = radii.iter().map(|r| r.0).sum();
    let a = Span { x0: bb.x0.saturating_sub(total), y0: bb.y0.saturating_sub(total), x1: (bb.x1 + total).min(w), y1: (bb.y1 + total).min(h) };
    let (aw, ah) = (a.x1 - a.x0, a.y1 - a.y0);
    let mut buf = vec![[0.0f32; 4]; aw * ah];
    let data = pm.data_mut();
    for y in 0..ah {
        let src = &data[((a.y0 + y) * w + a.x0) * 4..((a.y0 + y) * w + a.x1) * 4];
        for (d, s) in buf[y * aw..(y + 1) * aw].iter_mut().zip(src.as_chunks::<4>().0.iter()) {
            *d = [s[0] as f32, s[1] as f32, s[2] as f32, s[3] as f32];
        }
    }
    let nz = Span { x0: bb.x0 - a.x0, y0: bb.y0 - a.y0, x1: bb.x1 - a.x0, y1: bb.y1 - a.y0 };
    blur_core(&mut buf, aw, ah, &radii, nz);
    for y in 0..ah {
        let dst = &mut data[((a.y0 + y) * w + a.x0) * 4..((a.y0 + y) * w + a.x1) * 4];
        for (d, s) in dst.as_chunks_mut::<4>().0.iter_mut().zip(&buf[y * aw..(y + 1) * aw]) {
            store_px(d, s);
        }
    }
}

/// Variance (in full resolution px^2) that averaging down (a `k`-wide box) and
/// bilinear upsampling (a triangle of half-width `k`) add, which the
/// reduced-resolution blur must not repeat.
fn resample_variance(k: usize) -> f32 {
    let k = k as f32;
    (k * k - 1.0) / 12.0 + k * k / 6.0
}

/// Average down by `k`, blur, bilinear upsample. `phase` is the absolute
/// position of the pixmap origin modulo `k`, so the coarse grid sits on the
/// same absolute pixels whichever region of the scene is being blurred (tiles
/// and full renders then agree exactly).
fn blur_down(pm: &mut Pixmap, w: usize, h: usize, bb: Span, sigma: f32, k: usize, phase: (usize, usize)) {
    let target = (box_variance(sigma) - resample_variance(k)).max(0.0) / (k * k) as f32;
    let radii = frac_radii_for_variance(target);
    let total: usize = radii.iter().map(|r| r.0 + 1).sum();
    // Work in grid space X = x + phase: coarse cell (i, j) covers X in
    // [i*k, (i+1)*k) and Y in [j*k, (j+1)*k).
    let (sx, sy) = phase;
    let (cw_all, ch_all) = ((w + sx).div_ceil(k), (h + sy).div_ceil(k));
    let nz = Span { x0: (bb.x0 + sx) / k, y0: (bb.y0 + sy) / k, x1: (bb.x1 + sx).div_ceil(k), y1: (bb.y1 + sy).div_ceil(k) };
    let pad = total + 2;
    let a = Span { x0: nz.x0.saturating_sub(pad), y0: nz.y0.saturating_sub(pad), x1: (nz.x1 + pad).min(cw_all), y1: (nz.y1 + pad).min(ch_all) };
    let (aw, ah) = (a.x1 - a.x0, a.y1 - a.y0);
    let mut buf = vec![[0.0f32; 4]; aw * ah];
    let data = pm.data();
    let inv = 1.0 / (k * k) as f32;
    for cy in nz.y0..nz.y1 {
        let out = &mut buf[(cy - a.y0) * aw..(cy - a.y0 + 1) * aw];
        for gy in (cy * k)..((cy + 1) * k) {
            if gy < sy || gy - sy >= h {
                continue;
            }
            let row = &data[(gy - sy) * w * 4..(gy - sy + 1) * w * 4];
            for cx in nz.x0..nz.x1 {
                let o = &mut out[cx - a.x0];
                let gx0 = (cx * k).max(sx) - sx;
                let gx1 = (((cx + 1) * k).saturating_sub(sx)).min(w);
                if gx1 <= gx0 {
                    continue;
                }
                for px in row[gx0 * 4..gx1 * 4].as_chunks::<4>().0.iter() {
                    for c in 0..4 {
                        o[c] += px[c] as f32;
                    }
                }
            }
        }
        for o in &mut out[nz.x0 - a.x0..nz.x1 - a.x0] {
            for v in o.iter_mut() {
                *v *= inv;
            }
        }
    }
    blur_core(&mut buf, aw, ah, &radii, Span { x0: nz.x0 - a.x0, y0: nz.y0 - a.y0, x1: nz.x1 - a.x0, y1: nz.y1 - a.y0 });
    // upsample over the full-res footprint of the coarse window
    let fx0 = (a.x0 * k).saturating_sub(sx);
    let fx1 = (a.x1 * k).saturating_sub(sx).min(w);
    let fy0 = (a.y0 * k).saturating_sub(sy);
    let fy1 = (a.y1 * k).saturating_sub(sy).min(h);
    // Pad the blurred coarse grid by one cell, linearly extrapolated, so
    // pixels between the outermost cell centre and the edge keep the slope of
    // the falloff towards the (transparent) outside instead of flattening.
    let (pw, ph) = (aw + 2, ah + 2);
    let mut pbuf = vec![[0.0f32; 4]; pw * ph];
    for y in 0..ah {
        pbuf[(y + 1) * pw + 1..(y + 1) * pw + 1 + aw].copy_from_slice(&buf[y * aw..(y + 1) * aw]);
    }
    let extrap = |p: Px, q: Px| -> Px { [2.0 * p[0] - q[0], 2.0 * p[1] - q[1], 2.0 * p[2] - q[2], 2.0 * p[3] - q[3]] };
    for y in 1..=ah {
        let row = &mut pbuf[y * pw..(y + 1) * pw];
        row[0] = if aw > 1 { extrap(row[1], row[2]) } else { row[1] };
        row[pw - 1] = if aw > 1 { extrap(row[pw - 2], row[pw - 3]) } else { row[pw - 2] };
    }
    for x in 0..pw {
        pbuf[x] = if ah > 1 { extrap(pbuf[pw + x], pbuf[2 * pw + x]) } else { pbuf[pw + x] };
        pbuf[(ph - 1) * pw + x] = if ah > 1 { extrap(pbuf[(ph - 2) * pw + x], pbuf[(ph - 3) * pw + x]) } else { pbuf[(ph - 2) * pw + x] };
    }
    // per-column taps (padded index pair + weight)
    let taps: Vec<(usize, usize, f32)> = (fx0..fx1)
        .map(|x| {
            let u = ((x + sx) as f32 + 0.5) / k as f32 - 0.5 - a.x0 as f32;
            let i0 = u.floor();
            let t = u - i0;
            let last = (pw - 1) as f32;
            ((i0 + 1.0).clamp(0.0, last) as usize, (i0 + 2.0).clamp(0.0, last) as usize, t)
        })
        .collect();
    let data = pm.data_mut();
    let mut row_buf: Vec<Px> = vec![[0.0; 4]; pw];
    for y in fy0..fy1 {
        let v = ((y + sy) as f32 + 0.5) / k as f32 - 0.5 - a.y0 as f32;
        let j0 = v.floor();
        let t = v - j0;
        let last = (ph - 1) as f32;
        let (r0, r1) = ((j0 + 1.0).clamp(0.0, last) as usize, (j0 + 2.0).clamp(0.0, last) as usize);
        let (ra, rb) = (&pbuf[r0 * pw..(r0 + 1) * pw], &pbuf[r1 * pw..(r1 + 1) * pw]);
        for ((o, p), q) in row_buf.iter_mut().zip(ra).zip(rb) {
            for c in 0..4 {
                o[c] = p[c] + (q[c] - p[c]) * t;
            }
        }
        let dst = &mut data[(y * w + fx0) * 4..(y * w + fx1) * 4];
        for (d, &(i0, i1, tx)) in dst.as_chunks_mut::<4>().0.iter_mut().zip(&taps) {
            let (p, q) = (&row_buf[i0], &row_buf[i1]);
            let v = [p[0] + (q[0] - p[0]) * tx, p[1] + (q[1] - p[1]) * tx, p[2] + (q[2] - p[2]) * tx, p[3] + (q[3] - p[3]) * tx];
            store_px(d, &v);
        }
    }
}

/// The original, unoptimised implementation. Kept as the accuracy reference
/// for tests and the benchmark.
#[doc(hidden)]
pub fn gaussian_blur_reference(pm: &mut Pixmap, sigma: f32) {
    fn box_line(buf: &mut [f32], line: &mut Vec<[f32; 4]>, start: usize, stride: usize, n: usize, r: usize) {
        line.clear();
        for i in 0..n {
            let p = (start + i * stride) * 4;
            line.push([buf[p], buf[p + 1], buf[p + 2], buf[p + 3]]);
        }
        let inv = 1.0 / (2 * r + 1) as f32;
        let mut sum = [0.0f32; 4];
        for item in line.iter().take((r + 1).min(n)) {
            for c in 0..4 {
                sum[c] += item[c];
            }
        }
        for i in 0..n {
            let p = (start + i * stride) * 4;
            for c in 0..4 {
                buf[p + c] = sum[c] * inv;
            }
            let add = i + r + 1;
            if add < n {
                for c in 0..4 {
                    sum[c] += line[add][c];
                }
            }
            if i >= r {
                let rem = i - r;
                for c in 0..4 {
                    sum[c] -= line[rem][c];
                }
            }
        }
    }
    if sigma < 0.3 || pm.width() == 0 || pm.height() == 0 {
        return;
    }
    let (w, h) = (pm.width() as usize, pm.height() as usize);
    let mut buf: Vec<f32> = pm.data().iter().map(|&v| v as f32).collect();
    let mut line = Vec::with_capacity(w.max(h));
    for size in boxes_for_gauss(sigma, 3) {
        let r = (size - 1) / 2;
        if r == 0 {
            continue;
        }
        for y in 0..h {
            box_line(&mut buf, &mut line, y * w, 1, w, r);
        }
        for x in 0..w {
            box_line(&mut buf, &mut line, x, w, h, r);
        }
    }
    let data = pm.data_mut();
    for (d, s) in data.iter_mut().zip(buf.iter()) {
        *d = to_u8(*s);
    }
    fix_premul(data);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia as sk;

    fn max_delta(a: &Pixmap, b: &Pixmap) -> u8 {
        a.data().iter().zip(b.data()).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0)
    }

    fn fill(pm: &mut Pixmap, path: &sk::Path, rgba: [u8; 4]) {
        let mut p = sk::Paint { anti_alias: true, ..Default::default() };
        p.set_color_rgba8(rgba[0], rgba[1], rgba[2], rgba[3]);
        pm.fill_path(path, &p, sk::FillRule::Winding, sk::Transform::identity(), None);
    }

    /// Scenes covering hard edges, curves, thin features, translucency,
    /// content touching the image border and white noise.
    fn scenes(w: u32, h: u32) -> Vec<(&'static str, Pixmap)> {
        let (wf, hf) = (w as f32, h as f32);
        let mut out = Vec::new();
        let rect = |x: f32, y: f32, ww: f32, hh: f32| sk::PathBuilder::from_rect(sk::Rect::from_xywh(x, y, ww, hh).unwrap());
        let mut pm = Pixmap::new(w, h).unwrap();
        fill(&mut pm, &rect(wf * 0.3 + 0.37, hf * 0.3 + 0.61, wf * 0.4, hf * 0.35), [0, 0, 0, 255]);
        out.push(("rect", pm));
        let mut pm = Pixmap::new(w, h).unwrap();
        fill(&mut pm, &sk::PathBuilder::from_circle(wf * 0.5, hf * 0.5, wf.min(hf) * 0.28).unwrap(), [255, 40, 10, 255]);
        out.push(("circle", pm));
        let mut pm = Pixmap::new(w, h).unwrap();
        fill(&mut pm, &rect(wf * 0.2, hf * 0.5, wf * 0.6, 1.0), [0, 0, 0, 255]);
        fill(&mut pm, &rect(wf * 0.5, hf * 0.2, 1.0, hf * 0.6), [0, 0, 255, 255]);
        out.push(("hairlines", pm));
        let mut pm = Pixmap::new(w, h).unwrap();
        fill(&mut pm, &rect(wf * 0.1, hf * 0.1, wf * 0.2, hf * 0.2), [0, 0, 0, 255]);
        fill(&mut pm, &rect(wf * 0.55, hf * 0.6, wf * 0.3, hf * 0.15), [0, 0, 0, 128]);
        fill(&mut pm, &sk::PathBuilder::from_circle(wf * 0.7, hf * 0.25, 7.0).unwrap(), [0, 0, 0, 255]);
        out.push(("blobs", pm));
        let mut pm = Pixmap::new(w, h).unwrap();
        fill(&mut pm, &rect(-5.0, -5.0, wf * 0.4, hf * 0.4), [0, 0, 0, 255]);
        fill(&mut pm, &rect(wf * 0.8, hf * 0.8, wf, hf), [0, 0, 0, 255]);
        out.push(("edges", pm));
        let mut pm = Pixmap::new(w, h).unwrap();
        pm.fill(sk::Color::from_rgba8(30, 60, 90, 200));
        out.push(("full", pm));
        let mut pm = Pixmap::new(w, h).unwrap();
        let mut s = 12345u32;
        for i in 0..(w * h) as usize {
            s = s.wrapping_mul(1664525).wrapping_add(1013904223);
            let a = if (s >> 24) & 3 == 0 { 255 } else { 0 };
            pm.data_mut()[i * 4..i * 4 + 4].copy_from_slice(&[0, 0, 0, a]);
        }
        out.push(("noise", pm));
        out
    }

    #[test]
    fn blur_preserves_energy_and_spreads() {
        let mut pm = Pixmap::new(41, 41).unwrap();
        let i = (20 * 41 + 20) * 4;
        pm.data_mut()[i + 3] = 255;
        pm.data_mut()[i] = 255;
        gaussian_blur(&mut pm, 3.0);
        let total: f32 = pm.data().as_chunks::<4>().0.iter().map(|p| p[3] as f32).sum();
        assert!((total - 255.0).abs() < 25.0, "total {total}");
        let center = pm.data()[i + 3];
        assert!(center > 0 && center < 60, "center {center}");
        assert!(pm.data()[(20 * 41 + 24) * 4 + 3] > 0);
    }

    /// Below the downsampling threshold the zero-skipping blur is the
    /// reference up to float summation order (never more than one level).
    #[test]
    fn full_res_matches_reference() {
        for (name, src) in scenes(180, 140) {
            for sigma in [0.5f32, 1.0, 1.7, 2.5, 3.3, 4.0, 5.0, 5.9] {
                let (mut a, mut b) = (src.clone(), src.clone());
                gaussian_blur(&mut a, sigma);
                gaussian_blur_reference(&mut b, sigma);
                let d = max_delta(&a, &b);
                assert!(d <= 1, "{name} sigma {sigma}: delta {d}");
            }
        }
    }

    #[test]
    fn empty_pixmap_is_untouched() {
        let mut pm = Pixmap::new(64, 64).unwrap();
        gaussian_blur(&mut pm, 9.0);
        assert!(pm.data().iter().all(|b| *b == 0));
    }

    /// Downsampled blur stays within 2 levels of the reference for every sigma
    /// the thresholds select it for.
    #[test]
    fn downsampled_within_two_levels() {
        let mut worst = (0u8, String::new());
        for (name, src) in scenes(300, 240) {
            let mut sigma = 0.5f32;
            while sigma < 60.0 {
                let (mut a, mut b) = (src.clone(), src.clone());
                gaussian_blur(&mut a, sigma);
                gaussian_blur_reference(&mut b, sigma);
                let d = max_delta(&a, &b);
                if d > worst.0 {
                    worst = (d, format!("{name} sigma {sigma}"));
                }
                assert!(d <= 2, "{name} sigma {sigma}: delta {d}");
                sigma *= 1.13;
            }
        }
        eprintln!("worst blur delta {} at {}", worst.0, worst.1);
    }

    /// Diagnostic used to choose the thresholds: worst delta per sigma for a
    /// forced downsampling factor.
    #[test]
    #[ignore]
    fn where_is_worst() {
        let (k, name) = (4usize, "edges");
        for sigma in [22.0f32, 25.0, 28.0, 33.0, 40.0] {
            let src = scenes(300, 240).into_iter().find(|s| s.0 == name).unwrap().1;
            let (mut a, mut b) = (src.clone(), src.clone());
            let bb = nonzero_bbox(a.data(), 300, 240).unwrap();
            blur_with_factor(&mut a, 300, 240, bb, sigma, k, (0, 0));
            gaussian_blur_reference(&mut b, sigma);
            let mut worst = (0u8, 0usize, 0usize, 0u8, 0u8);
            for (i, (p, q)) in a.data().chunks(4).zip(b.data().chunks(4)).enumerate() {
                let d = p[3].abs_diff(q[3]);
                if d > worst.0 {
                    worst = (d, i % 300, i / 300, p[3], q[3]);
                }
            }
            eprintln!("sigma {sigma}: delta {} at ({}, {}) got {} want {}", worst.0, worst.1, worst.2, worst.3, worst.4);
        }
    }

    #[test]
    #[ignore]
    fn scan_thresholds() {
        for k in [2usize, 4] {
            let mut sigma = 2.0f32;
            while sigma < 70.0 {
                let mut worst = (0u8, "");
                for (name, src) in scenes(300, 240) {
                    let (mut a, mut b) = (src.clone(), src.clone());
                    let bb = nonzero_bbox(a.data(), 300, 240).unwrap();
                    blur_with_factor(&mut a, 300, 240, bb, sigma, k, (0, 0));
                    gaussian_blur_reference(&mut b, sigma);
                    let d = max_delta(&a, &b);
                    if d > worst.0 {
                        worst = (d, name);
                    }
                }
                eprintln!("k {k} sigma {sigma:5.1}: worst {} ({})", worst.0, worst.1);
                sigma *= 1.2;
            }
        }
    }

    /// With the caller guaranteeing a margin around the content, the reduced
    /// resolution grid is used from a smaller sigma and is still within 2
    /// levels everywhere.
    #[test]
    fn margined_within_two_levels() {
        let mut worst = (0u8, String::new());
        for (name, src) in scenes(300, 240) {
            if matches!(name, "edges" | "full" | "noise") {
                continue; // content touches the border
            }
            let mut sigma = 5.0f32;
            while sigma < 70.0 {
                let (mut a, mut b) = (src.clone(), src.clone());
                gaussian_blur_at(&mut a, sigma, (3, 5), Edges::Margined);
                gaussian_blur_reference(&mut b, sigma);
                let d = max_delta(&a, &b);
                if d > worst.0 {
                    worst = (d, format!("{name} sigma {sigma}"));
                }
                assert!(d <= 2, "{name} sigma {sigma}: delta {d}");
                sigma *= 1.07;
            }
        }
        eprintln!("worst margined delta {} at {}", worst.0, worst.1);
    }

    /// Blurring the same absolute content in pixmaps whose origins differ gives
    /// identical pixels, also when the reduced resolution path is taken (this is
    /// what keeps tiles and full renders equal).
    #[test]
    fn origin_shift_invariance() {
        let big = &scenes(420, 330)[1].1; // "circle", inside both crops
        for sigma in [3.0f32, 9.0, 24.0, 33.0, 50.0] {
            // reference crop: origin (100, 70), size 300x240, taken from the big pixmap
            let crop = |x0: u32, y0: u32| {
                let r = sk::IntRect::from_xywh(x0 as i32, y0 as i32, 300, 240).unwrap();
                big.clone_rect(r).unwrap()
            };
            let mut a = crop(40, 30);
            let mut b = crop(47, 33);
            gaussian_blur_at(&mut a, sigma, (40, 30), Edges::Margined);
            gaussian_blur_at(&mut b, sigma, (47, 33), Edges::Margined);
            // compare the overlap, minus a border where the crops see different edges
            let m = (3.0 * sigma).ceil() as usize + 16;
            let mut bad = 0;
            for y in m..240 - m - 3 {
                for x in m..300 - m - 7 {
                    let (pa, pb) = (&a.data()[((y + 3) * 300 + x + 7) * 4..][..4], &b.data()[(y * 300 + x) * 4..][..4]);
                    if pa != pb {
                        bad += 1;
                    }
                }
            }
            assert_eq!(bad, 0, "sigma {sigma}: {bad} pixels differ between origins");
        }
    }

    #[test]
    fn odd_sizes_and_tiny_images() {
        for (w, h) in [(1u32, 1u32), (3, 50), (50, 3), (37, 41), (257, 5), (100, 101)] {
            for sigma in [1.0f32, 7.0, 20.0] {
                let mut a = Pixmap::new(w, h).unwrap();
                a.fill(sk::Color::from_rgba8(0, 0, 0, 255));
                let mut b = a.clone();
                gaussian_blur(&mut a, sigma);
                gaussian_blur_reference(&mut b, sigma);
                let d = max_delta(&a, &b);
                assert!(d <= 2, "{w}x{h} sigma {sigma}: delta {d}");
            }
        }
    }
}
