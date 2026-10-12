//! Curve fitting: turns dense polylines (flattened curves, freehand strokes,
//! boolean results) back into a few smooth cubic béziers, Philip Schneider's
//! algorithm ("An Algorithm for Automatically Fitting Digitized Curves", 1990)
//! with corner detection so sharp corners stay sharp.

use crate::Vec2;

pub(crate) fn v(x: f32, y: f32) -> Vec2 {
    Vec2::new(x, y)
}
pub(crate) fn add(a: Vec2, b: Vec2) -> Vec2 {
    v(a.x + b.x, a.y + b.y)
}
pub(crate) fn sub(a: Vec2, b: Vec2) -> Vec2 {
    v(a.x - b.x, a.y - b.y)
}
pub(crate) fn mul(a: Vec2, s: f32) -> Vec2 {
    v(a.x * s, a.y * s)
}
pub(crate) fn dot(a: Vec2, b: Vec2) -> f32 {
    a.x * b.x + a.y * b.y
}
pub(crate) fn cross(a: Vec2, b: Vec2) -> f32 {
    a.x * b.y - a.y * b.x
}
pub(crate) fn len(a: Vec2) -> f32 {
    dot(a, a).sqrt()
}
pub(crate) fn dist(a: Vec2, b: Vec2) -> f32 {
    len(sub(a, b))
}
pub(crate) fn lerp(a: Vec2, b: Vec2, t: f32) -> Vec2 {
    add(a, mul(sub(b, a), t))
}
pub(crate) fn norm(a: Vec2) -> Vec2 {
    let l = len(a);
    if l < 1e-9 {
        Vec2::ZERO
    } else {
        mul(a, 1.0 / l)
    }
}

/// A cubic bézier: start, two control points, end.
pub type Cubic = [Vec2; 4];

pub fn cubic_point(c: &Cubic, t: f32) -> Vec2 {
    let u = 1.0 - t;
    let (a, b, cc, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    v(a * c[0].x + b * c[1].x + cc * c[2].x + d * c[3].x, a * c[0].y + b * c[1].y + cc * c[2].y + d * c[3].y)
}

pub fn cubic_deriv(c: &Cubic, t: f32) -> Vec2 {
    let u = 1.0 - t;
    let (a, b, cc) = (3.0 * u * u, 6.0 * u * t, 3.0 * t * t);
    let d0 = sub(c[1], c[0]);
    let d1 = sub(c[2], c[1]);
    let d2 = sub(c[3], c[2]);
    v(a * d0.x + b * d1.x + cc * d2.x, a * d0.y + b * d1.y + cc * d2.y)
}

fn cubic_deriv2(c: &Cubic, t: f32) -> Vec2 {
    let u = 1.0 - t;
    let e0 = add(sub(c[2], mul(c[1], 2.0)), c[0]);
    let e1 = add(sub(c[3], mul(c[2], 2.0)), c[1]);
    add(mul(e0, 6.0 * u), mul(e1, 6.0 * t))
}

/// Splits at `t` (de Casteljau).
pub fn cubic_split(c: &Cubic, t: f32) -> (Cubic, Cubic) {
    let p01 = lerp(c[0], c[1], t);
    let p12 = lerp(c[1], c[2], t);
    let p23 = lerp(c[2], c[3], t);
    let p012 = lerp(p01, p12, t);
    let p123 = lerp(p12, p23, t);
    let m = lerp(p012, p123, t);
    ([c[0], p01, p012, m], [m, p123, p23, c[3]])
}

/// The part of `c` between parameters `t0 <= t1`.
pub fn cubic_sub(c: &Cubic, t0: f32, t1: f32) -> Cubic {
    let (_, right) = cubic_split(c, t0);
    if t0 >= 1.0 - 1e-6 {
        return right;
    }
    let (left, _) = cubic_split(&right, ((t1 - t0) / (1.0 - t0)).clamp(0.0, 1.0));
    left
}

/// Polyline appended to `out` (excluding the start point), fine enough for `tol`.
pub fn flatten_cubic(c: &Cubic, tol: f32, out: &mut Vec<Vec2>) {
    let l = dist(c[0], c[1]) + dist(c[1], c[2]) + dist(c[2], c[3]);
    let chord = dist(c[0], c[3]);
    if l - chord < tol * 0.1 {
        out.push(c[3]);
        return;
    }
    let n = (((l / tol.max(1e-3)).sqrt() * 0.7).ceil() as usize).clamp(4, 128);
    for i in 1..=n {
        out.push(cubic_point(c, i as f32 / n as f32));
    }
}

/// Approximate length by sampling.
pub fn cubic_length(c: &Cubic) -> f32 {
    let mut prev = c[0];
    let mut total = 0.0;
    for i in 1..=24 {
        let p = cubic_point(c, i as f32 / 24.0);
        total += dist(prev, p);
        prev = p;
    }
    total
}

/// Parameter at arc distance `d` from the start (clamped to the curve).
pub fn cubic_param_at_distance(c: &Cubic, d: f32) -> f32 {
    if d <= 0.0 {
        return 0.0;
    }
    const N: usize = 48;
    let mut prev = c[0];
    let mut acc = 0.0;
    for i in 1..=N {
        let t = i as f32 / N as f32;
        let p = cubic_point(c, t);
        let seg = dist(prev, p);
        if acc + seg >= d {
            let f = if seg > 1e-9 { (d - acc) / seg } else { 0.0 };
            return ((i - 1) as f32 + f) / N as f32;
        }
        acc += seg;
        prev = p;
    }
    1.0
}

/// Closest parameter on `c` to `p` and the distance.
pub fn cubic_nearest(c: &Cubic, p: Vec2) -> (f32, f32) {
    const N: usize = 32;
    let mut best = (0.0, f32::MAX);
    for i in 0..=N {
        let t = i as f32 / N as f32;
        let d = dist(cubic_point(c, t), p);
        if d < best.1 {
            best = (t, d);
        }
    }
    // Newton refinement around the best sample
    let mut t = best.0;
    for _ in 0..4 {
        let q = cubic_point(c, t);
        let d1 = cubic_deriv(c, t);
        let d2 = cubic_deriv2(c, t);
        let diff = sub(q, p);
        let num = dot(diff, d1);
        let den = dot(d1, d1) + dot(diff, d2);
        if den.abs() < 1e-9 {
            break;
        }
        t = (t - num / den).clamp(0.0, 1.0);
    }
    let d = dist(cubic_point(c, t), p);
    if d < best.1 {
        (t, d)
    } else {
        best
    }
}

/// Anchor of a fitted curve with optional absolute handles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    pub pos: Vec2,
    pub hin: Option<Vec2>,
    pub hout: Option<Vec2>,
}

impl Anchor {
    pub fn new(pos: Vec2) -> Self {
        Self { pos, hin: None, hout: None }
    }
}

// ---------------------------------------------------------------- Schneider
fn chord_params(pts: &[Vec2]) -> Vec<f32> {
    let mut u = vec![0.0];
    for i in 1..pts.len() {
        u.push(u[i - 1] + dist(pts[i], pts[i - 1]));
    }
    let total = *u.last().unwrap();
    if total > 0.0 {
        for x in &mut u {
            *x /= total;
        }
    }
    u
}

fn generate(pts: &[Vec2], u: &[f32], t1: Vec2, t2: Vec2) -> Cubic {
    let (first, last) = (pts[0], pts[pts.len() - 1]);
    let mut c = [[0.0f32; 2]; 2];
    let mut x = [0.0f32; 2];
    for (p, &t) in pts.iter().zip(u) {
        let s = 1.0 - t;
        let (b0, b1, b2, b3) = (s * s * s, 3.0 * s * s * t, 3.0 * s * t * t, t * t * t);
        let a1 = mul(t1, b1);
        let a2 = mul(t2, b2);
        c[0][0] += dot(a1, a1);
        c[0][1] += dot(a1, a2);
        c[1][0] = c[0][1];
        c[1][1] += dot(a2, a2);
        let tmp = sub(*p, add(mul(first, b0 + b1), mul(last, b2 + b3)));
        x[0] += dot(a1, tmp);
        x[1] += dot(a2, tmp);
    }
    let det = c[0][0] * c[1][1] - c[1][0] * c[0][1];
    let seg_len = dist(first, last);
    let (mut al, mut ar) = if det.abs() > 1e-12 { ((x[0] * c[1][1] - x[1] * c[0][1]) / det, (c[0][0] * x[1] - c[1][0] * x[0]) / det) } else { (0.0, 0.0) };
    let eps = 1e-6 * seg_len;
    if al < eps || ar < eps || al > seg_len * 2.0 || ar > seg_len * 2.0 {
        al = seg_len / 3.0;
        ar = seg_len / 3.0;
    }
    [first, add(first, mul(t1, al)), add(last, mul(t2, ar)), last]
}

fn newton(c: &Cubic, p: Vec2, u: f32) -> f32 {
    let d = sub(cubic_point(c, u), p);
    let d1 = cubic_deriv(c, u);
    let d2 = cubic_deriv2(c, u);
    let num = dot(d, d1);
    let den = dot(d1, d1) + dot(d, d2);
    if den.abs() < 1e-12 {
        u
    } else {
        (u - num / den).clamp(0.0, 1.0)
    }
}

fn max_error(pts: &[Vec2], c: &Cubic, u: &[f32]) -> (f32, usize) {
    let mut worst = (0.0f32, pts.len() / 2);
    for i in 1..pts.len() - 1 {
        let d = dist(cubic_point(c, u[i]), pts[i]);
        if d >= worst.0 {
            worst = (d, i);
        }
    }
    worst
}

/// cos of the largest turn (start to end tangent) one fitted curve may make: 100 degrees.
const MAX_TURN_COS: f32 = -0.173_648_2;

fn fit_cubic(pts: &[Vec2], t1: Vec2, t2: Vec2, err: f32, out: &mut Vec<Cubic>) {
    if pts.len() == 2 {
        let d = dist(pts[0], pts[1]) / 3.0;
        out.push([pts[0], add(pts[0], mul(t1, d)), add(pts[1], mul(t2, d)), pts[1]]);
        return;
    }
    let mut u = chord_params(pts);
    let mut c = generate(pts, &u, t1, t2);
    let (mut e, mut split) = max_error(pts, &c, &u);
    // a curve that turns too far reads as a blob however well it fits: a full circle takes four
    let turns_too_far = pts.len() >= 5 && dot(t1, mul(t2, -1.0)) < MAX_TURN_COS;
    if e < err && !turns_too_far {
        out.push(c);
        return;
    }
    if turns_too_far {
        e = e.max(err);
        split = pts.len() / 2;
    }
    if e < err * 16.0 && !turns_too_far {
        for _ in 0..6 {
            u = pts.iter().zip(&u).map(|(p, &t)| newton(&c, *p, t)).collect();
            c = generate(pts, &u, t1, t2);
            (e, split) = max_error(pts, &c, &u);
            if e < err {
                out.push(c);
                return;
            }
        }
    }
    let split = split.clamp(1, pts.len() - 2);
    let mut center = norm(sub(pts[split - 1], pts[split + 1]));
    if center == Vec2::ZERO {
        center = norm(sub(pts[split], pts[split + 1]));
    }
    fit_cubic(&pts[..=split], t1, center, err, out);
    fit_cubic(&pts[split..], mul(center, -1.0), t2, err, out);
}

// ---------------------------------------------------------------- public API
/// Removes consecutive duplicates (and the repeated closing point of a loop).
fn dedupe(points: &[Vec2], closed: bool) -> Vec<Vec2> {
    let mut out: Vec<Vec2> = Vec::with_capacity(points.len());
    for p in points {
        if out.last().is_none_or(|l| dist(*l, *p) > 1e-4) {
            out.push(*p);
        }
    }
    if closed && out.len() > 1 && dist(out[0], out[out.len() - 1]) <= 1e-4 {
        out.pop();
    }
    out
}

/// Indices of sharp corners of the polyline (turning angle above `corner_deg`).
fn corners(pts: &[Vec2], closed: bool, arm: f32, tol: f32, corner_deg: f32) -> Vec<usize> {
    let n = pts.len();
    if n < 3 {
        return vec![];
    }
    let cos_limit = corner_deg.to_radians().cos();
    let at = |i: isize| -> usize {
        if closed {
            i.rem_euclid(n as isize) as usize
        } else {
            i.clamp(0, n as isize - 1) as usize
        }
    };
    let mut score = vec![f32::MAX; n];
    let mut hard = vec![false; n];
    for (i, s) in score.iter_mut().enumerate() {
        if !closed && (i == 0 || i == n - 1) {
            continue;
        }
        // walk away until `arm` distance is covered on both sides
        let walk = |dir: isize| -> Vec2 {
            let mut j = i as isize;
            let mut acc = 0.0;
            let mut steps = 0;
            while acc < arm && steps < n {
                let nj = at(j + dir);
                if nj == at(j) {
                    break;
                }
                acc += dist(pts[at(j)], pts[nj]);
                j += dir;
                steps += 1;
            }
            pts[at(j)]
        };
        let a = norm(sub(pts[i], walk(-1)));
        let b = norm(sub(walk(1), pts[i]));
        *s = dot(a, b);
        // long edges on both sides: the vertex itself decides (sparse polygons)
        let (p, q) = (pts[at(i as isize - 1)], pts[at(i as isize + 1)]);
        if dist(p, pts[i]).min(dist(q, pts[i])) > tol * 4.0 {
            let c = dot(norm(sub(pts[i], p)), norm(sub(q, pts[i])));
            if c < cos_limit {
                hard[i] = true;
            }
            *s = s.min(c);
        }
    }
    let mut out = vec![];
    for i in 0..n {
        if score[i] >= cos_limit {
            continue;
        }
        if hard[i] {
            out.push(i);
            continue;
        }
        // keep only the sharpest point of a cluster of corners within `arm` of each other
        let beaten = [-1isize, 1].iter().any(|&dir| {
            let mut acc = 0.0;
            let mut j = i as isize;
            for _ in 0..n {
                let nj = j + dir;
                if !closed && !(0..n as isize).contains(&nj) {
                    break;
                }
                acc += dist(pts[at(j)], pts[at(nj)]);
                j = nj;
                if acc > arm {
                    break;
                }
                let jj = at(j);
                if jj == i {
                    break;
                }
                if score[jj] < score[i] || (score[jj] == score[i] && jj < i) {
                    return true;
                }
            }
            false
        });
        if !beaten {
            out.push(i);
        }
    }
    out
}

pub(crate) fn is_flat(c: &Cubic, tol: f32) -> bool {
    let d = sub(c[3], c[0]);
    let l = len(d);
    if l < 1e-6 {
        return dist(c[0], c[1]) < tol && dist(c[0], c[2]) < tol;
    }
    let n = v(-d.y / l, d.x / l);
    dot(sub(c[1], c[0]), n).abs() < tol && dot(sub(c[2], c[0]), n).abs() < tol
}

fn chain_to_anchors(curves: &[Cubic], tol: f32, closed: bool) -> Vec<Anchor> {
    let mut anchors: Vec<Anchor> = vec![Anchor::new(curves[0][0])];
    let n = curves.len();
    for (i, c) in curves.iter().enumerate() {
        let flat = is_flat(c, tol);
        let last_closing = closed && i == n - 1;
        let end_pos = c[3];
        if !flat {
            anchors.last_mut().unwrap().hout = Some(c[1]);
        }
        if last_closing {
            if !flat {
                anchors[0].hin = Some(c[2]);
            }
        } else {
            anchors.push(Anchor { pos: end_pos, hin: (!flat).then_some(c[2]), hout: None });
        }
    }
    anchors
}

/// Fits béziers to `points`. `tol` is the maximum deviation, `corner_deg` the
/// turning angle above which a polyline vertex stays a sharp corner.
///
/// Returns anchors in order; for closed curves the last curve runs from the
/// last anchor back to the first.
pub fn fit_polyline(points: &[Vec2], closed: bool, tol: f32, corner_deg: f32) -> Vec<Anchor> {
    let pts = dedupe(points, closed);
    let n = pts.len();
    if n == 0 {
        return vec![];
    }
    if n == 1 {
        return vec![Anchor::new(pts[0])];
    }
    if n == 2 {
        return vec![Anchor::new(pts[0]), Anchor::new(pts[1])];
    }
    let total: f32 = pts.windows(2).map(|w| dist(w[0], w[1])).sum::<f32>() + if closed { dist(pts[n - 1], pts[0]) } else { 0.0 };
    let arm = (total / n as f32 * 0.6).max(tol * 3.0);
    let cs = corners(&pts, closed, arm, tol, corner_deg);

    // tangents at the ends of a piece, measured over its first / last few steps
    let piece_tangents = |piece: &[Vec2]| -> (Vec2, Vec2) {
        let m = piece.len();
        // the first point at least `arm` away from the end: exact on clean polylines,
        // smoothed on noisy freehand input
        let reach = |from: usize, dir: isize| -> usize {
            let mut k = 1usize;
            while k < m - 1 && dist(piece[(from as isize + dir * k as isize) as usize], piece[from]) < arm {
                k += 1;
            }
            k
        };
        let (k1, k2) = (reach(0, 1), reach(m - 1, -1));
        (norm(sub(piece[k1], piece[0])), norm(sub(piece[m - 1 - k2], piece[m - 1])))
    };

    let anchors: Vec<Anchor>;
    if closed {
        // rotate so the loop starts at a corner, or at index 0 if it is smooth
        let start = cs.first().copied().unwrap_or(0);
        let mut ring: Vec<Vec2> = (0..n).map(|k| pts[(start + k) % n]).collect();
        ring.push(ring[0]);
        let mut cuts: Vec<usize> = cs.iter().map(|&c| (c + n - start) % n).collect();
        cuts.sort_unstable();
        if cuts.first() != Some(&0) {
            cuts.insert(0, 0);
        }
        cuts.push(n);
        let smooth_start = cs.is_empty();
        let mut curves: Vec<Cubic> = Vec::new();
        let last_piece = cuts.len() - 2;
        for (ci, w) in cuts.windows(2).enumerate() {
            let piece = &ring[w[0]..=w[1]];
            if piece.len() < 2 {
                continue;
            }
            let (mut t1, mut t2) = piece_tangents(piece);
            if smooth_start {
                // one smooth loop: both ends share the tangent across the start point
                let c = norm(sub(ring[1], ring[n - 1]));
                if ci == 0 {
                    t1 = c;
                }
                if ci == last_piece {
                    t2 = mul(c, -1.0);
                }
            }
            fit_cubic(piece, t1, t2, tol, &mut curves);
        }
        if curves.is_empty() {
            return vec![Anchor::new(pts[0])];
        }
        anchors = chain_to_anchors(&curves, tol, true);
    } else {
        let mut cuts: Vec<usize> = cs;
        cuts.insert(0, 0);
        cuts.push(n - 1);
        cuts.dedup();
        let mut curves: Vec<Cubic> = Vec::new();
        for w in cuts.windows(2) {
            let piece = &pts[w[0]..=w[1]];
            if piece.len() < 2 {
                continue;
            }
            let (t1, t2) = piece_tangents(piece);
            fit_cubic(piece, t1, t2, tol, &mut curves);
        }
        anchors = if curves.is_empty() { vec![] } else { chain_to_anchors(&curves, tol, false) };
    }
    anchors
}

#[cfg(test)]
mod tests {
    use super::*;

    fn circle(n: usize, r: f32) -> Vec<Vec2> {
        (0..n)
            .map(|i| {
                let a = std::f32::consts::TAU * i as f32 / n as f32;
                v(100.0 + r * a.cos(), 100.0 + r * a.sin())
            })
            .collect()
    }

    #[test]
    fn closed_circle_becomes_a_few_smooth_anchors() {
        let a = fit_polyline(&circle(96, 50.0), true, 0.2, 40.0);
        assert!(a.len() <= 8, "{} anchors", a.len());
        assert!(a.iter().all(|x| x.hin.is_some() && x.hout.is_some()), "all smooth");
    }

    #[test]
    fn square_keeps_four_sharp_corners_and_no_handles() {
        let mut pts = vec![];
        for i in 0..20 {
            pts.push(v(i as f32 * 5.0, 0.0));
        }
        for i in 0..20 {
            pts.push(v(100.0, i as f32 * 5.0));
        }
        for i in 0..20 {
            pts.push(v(100.0 - i as f32 * 5.0, 100.0));
        }
        for i in 0..20 {
            pts.push(v(0.0, 100.0 - i as f32 * 5.0));
        }
        let a = fit_polyline(&pts, true, 0.2, 40.0);
        assert_eq!(a.len(), 4, "{a:?}");
        assert!(a.iter().all(|x| x.hin.is_none() && x.hout.is_none()));
    }

    #[test]
    fn open_arc_is_few_curves_within_tolerance() {
        let pts: Vec<Vec2> = (0..=40)
            .map(|i| {
                let a = std::f32::consts::PI * i as f32 / 40.0;
                v(50.0 * a.cos(), 50.0 * a.sin())
            })
            .collect();
        let a = fit_polyline(&pts, false, 0.3, 40.0);
        assert!(a.len() <= 4, "{}", a.len());
        // every original point is within tolerance of the fitted curve
        let mut poly = vec![a[0].pos];
        for w in a.windows(2) {
            let c = [w[0].pos, w[0].hout.unwrap_or(w[0].pos), w[1].hin.unwrap_or(w[1].pos), w[1].pos];
            flatten_cubic(&c, 0.01, &mut poly);
        }
        for p in &pts {
            let d = poly.windows(2).map(|s| seg_dist(*p, s[0], s[1])).fold(f32::MAX, f32::min);
            assert!(d < 0.6, "deviation {d}");
        }
    }

    fn seg_dist(p: Vec2, a: Vec2, b: Vec2) -> f32 {
        let ab = sub(b, a);
        let l2 = dot(ab, ab);
        let t = if l2 < 1e-9 { 0.0 } else { (dot(sub(p, a), ab) / l2).clamp(0.0, 1.0) };
        dist(p, add(a, mul(ab, t)))
    }

    #[test]
    fn straight_dense_line_is_two_anchors() {
        let pts: Vec<Vec2> = (0..=30).map(|i| v(i as f32 * 3.0, i as f32)).collect();
        let a = fit_polyline(&pts, false, 0.3, 40.0);
        assert_eq!(a.len(), 2);
        assert!(a[0].hout.is_none() && a[1].hin.is_none());
    }

    #[test]
    fn cubic_helpers() {
        let c: Cubic = [v(0.0, 0.0), v(0.0, 60.0), v(100.0, 60.0), v(100.0, 0.0)];
        let (l, r) = cubic_split(&c, 0.5);
        assert!(dist(l[3], cubic_point(&c, 0.5)) < 1e-4 && dist(r[0], l[3]) < 1e-6);
        let (t, d) = cubic_nearest(&c, cubic_point(&c, 0.3));
        assert!((t - 0.3).abs() < 0.01 && d < 0.05);
        let s = cubic_sub(&c, 0.25, 0.75);
        assert!(dist(s[0], cubic_point(&c, 0.25)) < 1e-4 && dist(s[3], cubic_point(&c, 0.75)) < 1e-4);
        let line: Cubic = [v(0.0, 0.0), v(10.0, 0.0), v(20.0, 0.0), v(30.0, 0.0)];
        assert!((cubic_param_at_distance(&line, 15.0) - 0.5).abs() < 0.01);
    }
}
