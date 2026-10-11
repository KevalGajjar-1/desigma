//! Minimal SVG path-data parser/writer (the `d` attribute), as used by Figma's
//! `fillGeometry` / `strokeGeometry` strings.

use desigma_doc::{FillRule, Path, PathCmd, Vec2};

struct Lexer<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Lexer<'a> {
    fn skip_sep(&mut self) {
        while self.i < self.s.len() && (self.s[self.i].is_ascii_whitespace() || self.s[self.i] == b',') {
            self.i += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_sep();
        self.s.get(self.i).copied()
    }

    fn number(&mut self) -> Option<f32> {
        self.skip_sep();
        let start = self.i;
        let s = self.s;
        let mut i = self.i;
        if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
            i += 1;
        }
        let mut digits = 0;
        while i < s.len() && s[i].is_ascii_digit() {
            i += 1;
            digits += 1;
        }
        if i < s.len() && s[i] == b'.' {
            i += 1;
            while i < s.len() && s[i].is_ascii_digit() {
                i += 1;
                digits += 1;
            }
        }
        if digits == 0 {
            return None;
        }
        if i < s.len() && (s[i] == b'e' || s[i] == b'E') {
            let mut j = i + 1;
            if j < s.len() && (s[j] == b'+' || s[j] == b'-') {
                j += 1;
            }
            let ds = j;
            while j < s.len() && s[j].is_ascii_digit() {
                j += 1;
            }
            if j > ds {
                i = j;
            }
        }
        self.i = i;
        std::str::from_utf8(&s[start..i]).ok()?.parse::<f32>().ok()
    }

    /// Arc flags may be glued to following numbers ("a1 1 0 011 1").
    fn flag(&mut self) -> Option<bool> {
        self.skip_sep();
        let c = *self.s.get(self.i)?;
        match c {
            b'0' | b'1' => {
                self.i += 1;
                Some(c == b'1')
            }
            _ => None,
        }
    }
}

/// Parses SVG path data. Unknown/garbled input yields whatever parsed so far.
pub fn parse_path(d: &str, fill_rule: FillRule) -> Path {
    let mut lx = Lexer { s: d.as_bytes(), i: 0 };
    let mut cmds: Vec<PathCmd> = Vec::new();
    let mut cur = Vec2::ZERO;
    let mut start = Vec2::ZERO;
    let mut last_ctrl: Option<Vec2> = None; // for S/T reflection
    let mut last_cmd = b' ';
    let mut cmd = b' ';

    'outer: while let Some(c) = lx.peek() {
        if c.is_ascii_alphabetic() {
            cmd = c;
            lx.i += 1;
            if cmd == b'z' || cmd == b'Z' {
                if !cmds.is_empty() && !matches!(cmds.last(), Some(PathCmd::Close)) {
                    cmds.push(PathCmd::Close);
                }
                cur = start;
                last_ctrl = None;
                last_cmd = b'Z';
                continue;
            }
        } else if cmd == b' ' || cmd == b'z' || cmd == b'Z' {
            break; // number without a command
        } else if cmd == b'M' {
            cmd = b'L'; // implicit lineto after moveto
        } else if cmd == b'm' {
            cmd = b'l';
        }
        let rel = cmd.is_ascii_lowercase();
        let base = if rel { cur } else { Vec2::ZERO };
        macro_rules! num {
            () => {
                match lx.number() {
                    Some(v) => v,
                    None => break 'outer,
                }
            };
        }
        match cmd.to_ascii_uppercase() {
            b'M' => {
                let p = Vec2::new(base.x + num!(), base.y + num!());
                cmds.push(PathCmd::MoveTo(p));
                cur = p;
                start = p;
                last_ctrl = None;
            }
            b'L' => {
                let p = Vec2::new(base.x + num!(), base.y + num!());
                cmds.push(PathCmd::LineTo(p));
                cur = p;
                last_ctrl = None;
            }
            b'H' => {
                let p = Vec2::new(base.x + num!(), cur.y);
                cmds.push(PathCmd::LineTo(p));
                cur = p;
                last_ctrl = None;
            }
            b'V' => {
                let p = Vec2::new(cur.x, base.y + num!());
                cmds.push(PathCmd::LineTo(p));
                cur = p;
                last_ctrl = None;
            }
            b'C' => {
                let c1 = Vec2::new(base.x + num!(), base.y + num!());
                let c2 = Vec2::new(base.x + num!(), base.y + num!());
                let p = Vec2::new(base.x + num!(), base.y + num!());
                cmds.push(PathCmd::CubicTo(c1, c2, p));
                cur = p;
                last_ctrl = Some(c2);
            }
            b'S' => {
                let c1 = match (last_ctrl, last_cmd.to_ascii_uppercase()) {
                    (Some(lc), b'C' | b'S') => Vec2::new(2.0 * cur.x - lc.x, 2.0 * cur.y - lc.y),
                    _ => cur,
                };
                let c2 = Vec2::new(base.x + num!(), base.y + num!());
                let p = Vec2::new(base.x + num!(), base.y + num!());
                cmds.push(PathCmd::CubicTo(c1, c2, p));
                cur = p;
                last_ctrl = Some(c2);
            }
            b'Q' => {
                let c1 = Vec2::new(base.x + num!(), base.y + num!());
                let p = Vec2::new(base.x + num!(), base.y + num!());
                cmds.push(PathCmd::QuadTo(c1, p));
                cur = p;
                last_ctrl = Some(c1);
            }
            b'T' => {
                let c1 = match (last_ctrl, last_cmd.to_ascii_uppercase()) {
                    (Some(lc), b'Q' | b'T') => Vec2::new(2.0 * cur.x - lc.x, 2.0 * cur.y - lc.y),
                    _ => cur,
                };
                let p = Vec2::new(base.x + num!(), base.y + num!());
                cmds.push(PathCmd::QuadTo(c1, p));
                cur = p;
                last_ctrl = Some(c1);
            }
            b'A' => {
                let rx = num!();
                let ry = num!();
                let rot = num!();
                let Some(large) = lx.flag() else { break };
                let Some(sweep) = lx.flag() else { break };
                let p = Vec2::new(base.x + num!(), base.y + num!());
                arc_to_cubics(&mut cmds, cur, rx, ry, rot, large, sweep, p);
                cur = p;
                last_ctrl = None;
            }
            _ => break,
        }
        last_cmd = cmd;
    }
    Path { cmds, fill_rule }
}

/// SVG elliptical arc -> cubic Béziers (endpoint to center parametrization).
#[allow(clippy::too_many_arguments)]
fn arc_to_cubics(out: &mut Vec<PathCmd>, p0: Vec2, rx: f32, ry: f32, rot_deg: f32, large: bool, sweep: bool, p1: Vec2) {
    let (mut rx, mut ry) = (rx.abs() as f64, ry.abs() as f64);
    if (p0.x == p1.x && p0.y == p1.y) || rx == 0.0 || ry == 0.0 {
        if p0 != p1 {
            out.push(PathCmd::LineTo(p1));
        }
        return;
    }
    let phi = (rot_deg as f64).to_radians();
    let (sp, cp) = phi.sin_cos();
    let dx = (p0.x as f64 - p1.x as f64) / 2.0;
    let dy = (p0.y as f64 - p1.y as f64) / 2.0;
    let x1 = cp * dx + sp * dy;
    let y1 = -sp * dx + cp * dy;
    let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }
    let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut coef = if den == 0.0 { 0.0 } else { (num / den).max(0.0).sqrt() };
    if large == sweep {
        coef = -coef;
    }
    let cxp = coef * rx * y1 / ry;
    let cyp = -coef * ry * x1 / rx;
    let cx = cp * cxp - sp * cyp + (p0.x as f64 + p1.x as f64) / 2.0;
    let cy = sp * cxp + cp * cyp + (p0.y as f64 + p1.y as f64) / 2.0;
    let ang = |ux: f64, uy: f64, vx: f64, vy: f64| -> f64 {
        let dot = ux * vx + uy * vy;
        let len = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
        let mut a = (dot / len).clamp(-1.0, 1.0).acos();
        if ux * vy - uy * vx < 0.0 {
            a = -a;
        }
        a
    };
    let th1 = ang(1.0, 0.0, (x1 - cxp) / rx, (y1 - cyp) / ry);
    let mut dth = ang((x1 - cxp) / rx, (y1 - cyp) / ry, (-x1 - cxp) / rx, (-y1 - cyp) / ry);
    if !sweep && dth > 0.0 {
        dth -= std::f64::consts::TAU;
    } else if sweep && dth < 0.0 {
        dth += std::f64::consts::TAU;
    }
    let segs = (dth.abs() / (std::f64::consts::FRAC_PI_2)).ceil().max(1.0) as usize;
    let delta = dth / segs as f64;
    let t = 4.0 / 3.0 * (delta / 4.0).tan();
    let pt = |th: f64| -> (f64, f64) {
        let (s, c) = th.sin_cos();
        (cx + rx * c * cp - ry * s * sp, cy + rx * c * sp + ry * s * cp)
    };
    let dpt = |th: f64| -> (f64, f64) {
        let (s, c) = th.sin_cos();
        (-rx * s * cp - ry * c * sp, -rx * s * sp + ry * c * cp)
    };
    let mut th = th1;
    for i in 0..segs {
        let (ax, ay) = pt(th);
        let (adx, ady) = dpt(th);
        let th2 = th + delta;
        let (bx, by) = pt(th2);
        let (bdx, bdy) = dpt(th2);
        let end = if i + 1 == segs { p1 } else { Vec2::new(bx as f32, by as f32) };
        out.push(PathCmd::CubicTo(Vec2::new((ax + t * adx) as f32, (ay + t * ady) as f32), Vec2::new((bx - t * bdx) as f32, (by - t * bdy) as f32), end));
        th = th2;
    }
}

fn fmt(v: f32) -> String {
    if v == v.trunc() && v.abs() < 1e9 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// Writes absolute-coordinate SVG path data (M/L/Q/C/Z only).
pub fn write_path(p: &Path) -> String {
    let mut s = String::new();
    let pt = |s: &mut String, v: Vec2| {
        s.push_str(&fmt(v.x));
        s.push(' ');
        s.push_str(&fmt(v.y));
    };
    for (i, c) in p.cmds.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        match *c {
            PathCmd::MoveTo(a) => {
                s.push_str("M ");
                pt(&mut s, a);
            }
            PathCmd::LineTo(a) => {
                s.push_str("L ");
                pt(&mut s, a);
            }
            PathCmd::QuadTo(a, b) => {
                s.push_str("Q ");
                pt(&mut s, a);
                s.push(' ');
                pt(&mut s, b);
            }
            PathCmd::CubicTo(a, b, c) => {
                s.push_str("C ");
                pt(&mut s, a);
                s.push(' ');
                pt(&mut s, b);
                s.push(' ');
                pt(&mut s, c);
            }
            PathCmd::Close => s.push('Z'),
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_commands() {
        let p = parse_path("M10 20 L30 40 H50 V60 h-10 v-10 Z", FillRule::NonZero);
        assert_eq!(p.cmds.len(), 7);
        assert_eq!(p.cmds[2], PathCmd::LineTo(Vec2::new(50.0, 40.0)));
        assert_eq!(p.cmds[4], PathCmd::LineTo(Vec2::new(40.0, 60.0)));
        assert_eq!(p.cmds[6], PathCmd::Close);
    }

    #[test]
    fn implicit_lineto_and_compact_numbers() {
        let p = parse_path("M0,0 10,10 20-5.5e0.5", FillRule::EvenOdd);
        assert_eq!(p.cmds[1], PathCmd::LineTo(Vec2::new(10.0, 10.0)));
        assert!(matches!(p.cmds[2], PathCmd::LineTo(_)));
        assert_eq!(p.fill_rule, FillRule::EvenOdd);
    }

    #[test]
    fn arc_ends_at_target() {
        let p = parse_path("M0 0 A10 10 0 0 1 20 0", FillRule::NonZero);
        match p.cmds.last().unwrap() {
            PathCmd::CubicTo(_, _, e) => assert_eq!(*e, Vec2::new(20.0, 0.0)),
            c => panic!("{c:?}"),
        }
        // semicircle: two quarter arcs
        assert_eq!(p.cmds.len(), 3);
    }

    #[test]
    fn smooth_curves_reflect() {
        let p = parse_path("M0 0 C0 10 10 10 10 0 S20 -10 20 0", FillRule::NonZero);
        match p.cmds[2] {
            PathCmd::CubicTo(c1, _, _) => assert_eq!(c1, Vec2::new(10.0, -10.0)),
            _ => panic!(),
        }
    }

    #[test]
    fn write_roundtrip() {
        let p = parse_path("M0 0 L1.5 2 Q3 4 5 6 C1 2 3 4 5 6 Z", FillRule::NonZero);
        let q = parse_path(&write_path(&p), FillRule::NonZero);
        assert_eq!(p, q);
    }

    #[test]
    fn garbage_does_not_panic() {
        for s in ["", "M", "M 1", "x", "L 1 2", "A 1 2 3 4 5 6 7", "M1 2 3", "M0 0 A 5 5 0 2 2 1 1"] {
            let _ = parse_path(s, FillRule::NonZero);
        }
    }
}
