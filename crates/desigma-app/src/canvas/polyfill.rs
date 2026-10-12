//! Filling arbitrary (concave, with holes) polygons for overlays: egui only
//! fills convex shapes, so the polygon is triangulated here (holes are bridged
//! into the outline, then ear clipping).

use egui::epaint::Mesh;
use egui::{Color32, Painter, Pos2, Shape, TextureId};

fn cross(o: Pos2, a: Pos2, b: Pos2) -> f32 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

fn area2(p: &[Pos2]) -> f32 {
    let mut a = 0.0;
    for i in 0..p.len() {
        let (q, r) = (p[i], p[(i + 1) % p.len()]);
        a += q.x * r.y - r.x * q.y;
    }
    a
}

fn segments_cross(a: Pos2, b: Pos2, c: Pos2, d: Pos2) -> bool {
    let share = |p: Pos2, q: Pos2| (p - q).length_sq() < 1e-6;
    if share(a, c) || share(a, d) || share(b, c) || share(b, d) {
        return false;
    }
    let (d1, d2, d3, d4) = (cross(a, b, c), cross(a, b, d), cross(c, d, a), cross(c, d, b));
    ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0))
}

fn in_triangle(p: Pos2, a: Pos2, b: Pos2, c: Pos2) -> bool {
    let (d1, d2, d3) = (cross(a, b, p), cross(b, c, p), cross(c, a, p));
    d1 >= 0.0 && d2 >= 0.0 && d3 >= 0.0
}

/// Triangles covering `outer` minus `holes`. Contours may have any orientation.
pub fn triangulate(outer: &[Pos2], holes: &[Vec<Pos2>]) -> Vec<[Pos2; 3]> {
    if outer.len() < 3 {
        return vec![];
    }
    let mut poly: Vec<Pos2> = outer.to_vec();
    if area2(&poly) < 0.0 {
        poly.reverse();
    }
    let mut holes: Vec<Vec<Pos2>> = holes
        .iter()
        .filter(|h| h.len() >= 3)
        .map(|h| {
            let mut h = h.clone();
            if area2(&h) > 0.0 {
                h.reverse();
            }
            h
        })
        .collect();
    // bridge the hole with the rightmost vertex first
    holes.sort_by(|a, b| {
        let m = |h: &Vec<Pos2>| h.iter().map(|p| p.x).fold(f32::MIN, f32::max);
        m(b).total_cmp(&m(a))
    });
    for (hi, hole) in holes.clone().iter().enumerate() {
        let (mi, m) = hole.iter().enumerate().max_by(|a, b| a.1.x.total_cmp(&b.1.x)).map(|(i, p)| (i, *p)).unwrap();
        let mut order: Vec<usize> = (0..poly.len()).collect();
        order.sort_by(|a, b| (poly[*a] - m).length_sq().total_cmp(&(poly[*b] - m).length_sq()));
        let blocked = |v: Pos2, poly: &[Pos2]| -> bool {
            let n = poly.len();
            (0..n).any(|i| segments_cross(m, v, poly[i], poly[(i + 1) % n]))
                || holes.iter().enumerate().any(|(oi, o)| (oi >= hi) && (0..o.len()).any(|i| segments_cross(m, v, o[i], o[(i + 1) % o.len()])))
        };
        let Some(&vi) = order.iter().find(|&&i| !blocked(poly[i], &poly)) else { continue };
        let mut merged: Vec<Pos2> = poly[..=vi].to_vec();
        for k in 0..=hole.len() {
            merged.push(hole[(mi + k) % hole.len()]);
        }
        merged.extend_from_slice(&poly[vi..]);
        poly = merged;
    }
    // ear clipping
    let mut idx: Vec<usize> = (0..poly.len()).collect();
    let mut tris = vec![];
    let mut guard = 0;
    while idx.len() > 3 && guard < poly.len() * poly.len() + 16 {
        guard += 1;
        let n = idx.len();
        let mut clipped = false;
        for k in 0..n {
            let (ia, ib, ic) = (idx[(k + n - 1) % n], idx[k], idx[(k + 1) % n]);
            let (a, b, c) = (poly[ia], poly[ib], poly[ic]);
            if cross(a, b, c) <= 1e-6 {
                continue;
            }
            let covered = idx.iter().any(|&j| {
                let p = poly[j];
                j != ia && j != ib && j != ic && p != a && p != b && p != c && in_triangle(p, a, b, c)
            });
            if covered {
                continue;
            }
            tris.push([a, b, c]);
            idx.remove(k);
            clipped = true;
            break;
        }
        if !clipped {
            // degenerate input: fan out what is left
            for k in 1..idx.len() - 1 {
                tris.push([poly[idx[0]], poly[idx[k]], poly[idx[k + 1]]]);
            }
            return tris;
        }
    }
    if idx.len() == 3 {
        tris.push([poly[idx[0]], poly[idx[1]], poly[idx[2]]]);
    }
    tris
}

/// Fills `outer` minus `holes` with a flat colour.
pub fn fill(p: &Painter, outer: &[Pos2], holes: &[Vec<Pos2>], color: Color32) {
    let tris = triangulate(outer, holes);
    if tris.is_empty() {
        return;
    }
    let mut mesh = Mesh::with_texture(TextureId::default());
    for t in tris {
        let base = mesh.vertices.len() as u32;
        for v in t {
            mesh.colored_vertex(v, color);
        }
        mesh.add_triangle(base, base + 1, base + 2);
    }
    p.add(Shape::mesh(mesh));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(tris: &[[Pos2; 3]]) -> f32 {
        tris.iter().map(|t| cross(t[0], t[1], t[2]).abs() / 2.0).sum()
    }

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Vec<Pos2> {
        vec![Pos2::new(x, y), Pos2::new(x + w, y), Pos2::new(x + w, y + h), Pos2::new(x, y + h)]
    }

    #[test]
    fn concave_polygon_is_covered_exactly() {
        // an L shape
        let l = vec![Pos2::new(0.0, 0.0), Pos2::new(100.0, 0.0), Pos2::new(100.0, 50.0), Pos2::new(50.0, 50.0), Pos2::new(50.0, 100.0), Pos2::new(0.0, 100.0)];
        assert!((area(&triangulate(&l, &[])) - 7500.0).abs() < 0.5);
    }

    #[test]
    fn holes_are_left_out() {
        let t = triangulate(&rect(0.0, 0.0, 100.0, 100.0), &[rect(30.0, 30.0, 40.0, 40.0)]);
        assert!((area(&t) - (10000.0 - 1600.0)).abs() < 0.5, "{}", area(&t));
        let two = triangulate(&rect(0.0, 0.0, 200.0, 100.0), &[rect(20.0, 20.0, 40.0, 40.0), rect(120.0, 30.0, 40.0, 40.0)]);
        assert!((area(&two) - (20000.0 - 3200.0)).abs() < 0.5, "{}", area(&two));
    }

    #[test]
    fn orientation_does_not_matter() {
        let mut r = rect(0.0, 0.0, 10.0, 10.0);
        r.reverse();
        assert!((area(&triangulate(&r, &[])) - 100.0).abs() < 1e-3);
    }
}
