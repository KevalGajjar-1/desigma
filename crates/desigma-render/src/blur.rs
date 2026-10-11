//! Separable gaussian blur approximated by three box passes, operating on
//! premultiplied RGBA8 pixmaps. Out-of-bounds samples are transparent.

use tiny_skia::Pixmap;

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

/// One box pass of radius `r` along a line of `n` pixels with the given
/// stride (in pixels) starting at pixel index `start`.
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
        // slide window: add i+r+1, remove i-r
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

/// Blurs `pm` in place with a gaussian of standard deviation `sigma` (pixels).
pub fn gaussian_blur(pm: &mut Pixmap, sigma: f32) {
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
        *d = (s + 0.5).clamp(0.0, 255.0) as u8;
    }
    // keep premultiplied invariant (rounding can push colour above alpha)
    for px in data.chunks_exact_mut(4) {
        let a = px[3];
        for c in px.iter_mut().take(3) {
            if *c > a {
                *c = a;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blur_preserves_energy_and_spreads() {
        let mut pm = Pixmap::new(41, 41).unwrap();
        let i = (20 * 41 + 20) * 4;
        pm.data_mut()[i + 3] = 255;
        pm.data_mut()[i] = 255;
        gaussian_blur(&mut pm, 3.0);
        let total: f32 = pm.data().chunks_exact(4).map(|p| p[3] as f32).sum();
        assert!((total - 255.0).abs() < 25.0, "total {total}");
        let center = pm.data()[i + 3];
        assert!(center > 0 && center < 60, "center {center}");
        assert!(pm.data()[(20 * 41 + 24) * 4 + 3] > 0);
    }
}
