//! The Web page: one 1440 px dashboard with sidebar, KPI cards, charts and a table.

use super::icons::{smooth_line, Ic};
use super::*;

fn nav_item(s: &mut S, parent: NodeId, ic: Ic, label: &str, active: bool) {
    let it = s.row(parent, label, 12.0, [10.0, 12.0, 10.0, 12.0]);
    s.fill_w(it);
    s.align(it, Start, Center);
    s.radius(it, 12.0);
    if active {
        s.fill(it, Cl::T(Tok::BrandSoft));
    }
    let cl = Cl::T(if active { Tok::Brand } else { Tok::Muted });
    s.vicon(it, ic, 20.0, cl);
    s.text(it, label, "Text/Body strong", cl);
}

fn sidebar(s: &mut S, root: NodeId) {
    let sb = s.col(root, "Sidebar", 4.0, [28.0, 20.0, 24.0, 20.0]);
    s.fixed_w(sb, 248.0);
    s.fill_h(sb);
    s.fill(sb, Cl::T(Tok::Card));
    s.n(sb).stroke.paints = vec![Paint::solid(light(Tok::Border))];
    s.n(sb).stroke.weight = 1.0;
    s.n(sb).stroke.side_weights = Some([0.0, 1.0, 0.0, 0.0]);
    let v = s.var(Tok::Border).to_string();
    s.bind(sb, "strokes/0", &v);

    let logo = s.row(sb, "Logo", 10.0, [0.0, 0.0, 24.0, 8.0]);
    s.align(logo, Start, Center);
    let mark = s.row(logo, "Mark", 0.0, [0.0; 4]);
    s.fixed(mark, 34.0, 34.0);
    s.centered(mark);
    s.radius(mark, 11.0);
    s.fill_paint(mark, grad_d(&[(0.0, hex(0x4C5BF0)), (1.0, hex(0x8D5CF6))]));
    s.text(mark, "M", "Text/Heading", WHITE);
    s.text(logo, "Marlow", "Text/Heading", Cl::T(Tok::Ink));

    let l = s.text(sb, "Menu", "Text/Micro", Cl::T(Tok::Faint));
    let _ = l;
    for (ic, label, active) in [
        (Ic::Home, "Overview", true),
        (Ic::Wallet, "Accounts", false),
        (Ic::Card, "Cards", false),
        (Ic::Swap, "Transfers", false),
        (Ic::Insights, "Analytics", false),
        (Ic::Gift, "Rewards", false),
    ] {
        nav_item(s, sb, ic, label, active);
    }
    let gap = s.frame(sb, "Gap");
    s.size(gap, 1.0, 14.0);
    s.text(sb, "Support", "Text/Micro", Cl::T(Tok::Faint));
    nav_item(s, sb, Ic::Help, "Help centre", false);
    nav_item(s, sb, Ic::Shield, "Security", false);

    let spacer = s.frame(sb, "Spacer");
    s.size(spacer, 1.0, 1.0);
    s.grow(spacer);

    let up = s.col(sb, "Upgrade", 8.0, [16.0; 4]);
    s.fill_w(up);
    s.radius(up, 20.0);
    s.fill_paint(up, grad_d(&[(0.0, hex(0x4C5BF0)), (1.0, hex(0x8D5CF6))]));
    s.style_fill(up, "Gradient/Brand");
    s.text(up, "Marlow Plus", "Text/Heading", WHITE);
    s.text_w(up, "Cashback on every card payment.", "Text/Caption", white(0.85), 176.0);
    let b = s.inst(up, "Button/Secondary/Compact");
    s.set_text(b, "Label", "Upgrade");

    let user = s.row(sb, "Account", 10.0, [16.0, 4.0, 0.0, 4.0]);
    s.fill_w(user);
    s.align(user, Start, Center);
    s.inst(user, "Avatar/Small");
    let col = s.col(user, "Name", 0.0, [0.0; 4]);
    s.text(col, "Alex Morgan", "Text/Body strong", Cl::T(Tok::Ink));
    s.text(col, "Personal account", "Text/Caption", Cl::T(Tok::Muted));
}

fn header(s: &mut S, main: NodeId) {
    let h = s.row(main, "Header", 12.0, [0.0; 4]);
    s.fill_w(h);
    s.spread(h, Center);
    let t = s.col(h, "Title", 2.0, [0.0; 4]);
    s.text(t, "Overview", "Text/Headline", Cl::T(Tok::Ink));
    s.text(t, "Here is what happened with your money this week.", "Text/Body", Cl::T(Tok::Muted));
    let right = s.row(h, "Actions", 12.0, [0.0; 4]);
    s.align(right, Start, Center);
    let search = s.row(right, "Search", 10.0, [0.0, 14.0, 0.0, 14.0]);
    s.fixed(search, 300.0, 44.0);
    s.align(search, Start, Center);
    s.radius(search, 14.0);
    s.fill(search, Cl::T(Tok::Card));
    s.border(search);
    s.vicon(search, Ic::Search, 18.0, Cl::T(Tok::Muted));
    s.text(search, "Search payments, people...", "Text/Body", Cl::T(Tok::Faint));
    let bell = s.row(right, "Notifications", 0.0, [0.0; 4]);
    s.fixed(bell, 44.0, 44.0);
    s.centered(bell);
    s.radius(bell, 22.0);
    s.fill(bell, Cl::T(Tok::Card));
    s.border(bell);
    s.vicon(bell, Ic::Bell, 22.0, Cl::T(Tok::Ink));
    let b = s.inst(right, "Button/Primary/Compact");
    s.set_text(b, "Label", "New transfer");
    s.set_prop(b, "Show icon", PropValue::Bool(true));
    s.inst(right, "Avatar/Small");
}

fn card(s: &mut S, parent: NodeId, name: &str, gap: f32, pad: f32) -> NodeId {
    let c = s.col(parent, name, gap, [pad; 4]);
    s.fill(c, Cl::T(Tok::Card));
    s.radius_var(c, true);
    s.border(c);
    s.effect(c, card_shadow(), Some("Shadow/Card"));
    c
}

fn sparkline(s: &mut S, parent: NodeId, data: &[f32], cl: Cl) {
    let (w, h) = (64.0, 30.0);
    let pts: Vec<(f32, f32)> = data.iter().enumerate().map(|(i, v)| (2.0 + i as f32 * (w - 4.0) / (data.len() - 1) as f32, 3.0 + v * (h - 6.0))).collect();
    let v = s.vector(parent, "Sparkline", w, h, vec![Path { cmds: smooth_line(&pts), fill_rule: FillRule::NonZero }]);
    s.stroke(v, cl, 2.0);
}

fn kpis(s: &mut S, main: NodeId) {
    let row = s.row(main, "KPIs", 20.0, [0.0; 4]);
    s.fill_w(row);
    #[allow(clippy::type_complexity)]
    let data: [(&str, &str, Ic, &str, &str, [f32; 7]); 4] = [
        ("Total balance", "$48,920.14", Ic::Wallet, "4.2%", "Positive", [0.8, 0.7, 0.75, 0.45, 0.5, 0.25, 0.1]),
        ("Income", "$12,480.00", Ic::Request, "12.8%", "Positive", [0.7, 0.8, 0.5, 0.6, 0.35, 0.3, 0.12]),
        ("Spending", "$7,316.52", Ic::Send, "6.4%", "Negative", [0.2, 0.4, 0.3, 0.55, 0.45, 0.7, 0.85]),
        ("Savings rate", "41.4%", Ic::Bank, "2.1 pts", "Positive", [0.6, 0.62, 0.5, 0.52, 0.4, 0.3, 0.2]),
    ];
    for (label, value, ic, delta, tone, spark) in data {
        let c = card(s, row, label, 10.0, 20.0);
        s.fill_w(c);
        let top = s.row(c, "Top", 0.0, [0.0; 4]);
        s.fill_w(top);
        s.spread(top, Center);
        s.text(top, label, "Text/Caption", Cl::T(Tok::Muted));
        let tile = s.row(top, "Icon tile", 0.0, [0.0; 4]);
        s.fixed(tile, 36.0, 36.0);
        s.centered(tile);
        s.radius(tile, 12.0);
        s.fill(tile, Cl::T(Tok::BrandSoft));
        let comp = s.icon_comps[&ic];
        s.instance(tile, comp);
        s.text(c, value, "Text/Headline", Cl::T(Tok::Ink));
        let bottom = s.row(c, "Bottom", 0.0, [0.0; 4]);
        s.fill_w(bottom);
        s.spread(bottom, Center);
        let left = s.row(bottom, "Delta", 8.0, [0.0; 4]);
        s.align(left, Start, Center);
        let tag = s.inst(left, &format!("Tag/{tone}"));
        s.set_text(tag, "Label", &format!("+{delta}"));
        s.text(left, "vs last month", "Text/Caption", Cl::T(Tok::Muted));
        sparkline(s, bottom, &spark, Cl::T(if tone == "Positive" { Tok::Pos } else { Tok::Neg }));
    }
}

fn cash_flow(s: &mut S, parent: NodeId) {
    let c = card(s, parent, "Cash flow", 16.0, 24.0);
    s.fill_w(c);
    s.fill_h(c);
    let head = s.row(c, "Header", 0.0, [0.0; 4]);
    s.fill_w(head);
    s.spread(head, Center);
    let t = s.col(head, "Title", 0.0, [0.0; 4]);
    s.text(t, "Cash flow", "Text/Heading", Cl::T(Tok::Ink));
    s.text(t, "Income and spending, last 12 months", "Text/Caption", Cl::T(Tok::Muted));
    let legend = s.row(head, "Legend", 16.0, [0.0; 4]);
    s.align(legend, Start, Center);
    for (label, cl) in [("Income", Tok::Brand), ("Spending", Tok::Neg)] {
        let item = s.row(legend, label, 6.0, [0.0; 4]);
        s.align(item, Start, Center);
        let d = s.ellipse(item, "Dot", 10.0, 10.0, Cl::T(cl));
        let _ = d;
        s.text(item, label, "Text/Caption", Cl::T(Tok::Muted));
    }

    let (w, h) = (700.0, 190.0);
    let chart = s.frame(c, "Cash flow chart");
    s.size(chart, w, h);
    let (x0, top, bottom) = (46.0, 8.0, 156.0);
    for (i, label) in ["$20k", "$15k", "$10k", "$5k", "$0"].into_iter().enumerate() {
        let y = top + i as f32 * (bottom - top) / 4.0;
        let line = s.rect(chart, "Gridline", w - x0, 1.0, Cl::T(Tok::Border));
        s.at(line, x0, y);
        let t = s.text(chart, label, "Text/Label", Cl::T(Tok::Faint));
        s.at(t, 0.0, y - 8.0);
    }
    let months = ["Nov", "Dec", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct"];
    let pitch = (w - x0 - 12.0) / 11.0;
    let income = [0.42, 0.50, 0.46, 0.58, 0.55, 0.66, 0.62, 0.74, 0.70, 0.80, 0.77, 0.88];
    let spend = [0.36, 0.40, 0.52, 0.44, 0.48, 0.42, 0.50, 0.46, 0.58, 0.50, 0.54, 0.49];
    for (i, m) in months.iter().enumerate() {
        let x = x0 + 6.0 + i as f32 * pitch;
        let t = s.text(chart, m, "Text/Label", Cl::T(Tok::Muted));
        let tw = s.n(t).size.x;
        s.at(t, x - tw / 2.0, bottom + 14.0);
    }
    let to_pts =
        |vals: &[f32]| -> Vec<(f32, f32)> { vals.iter().enumerate().map(|(i, v)| (x0 + 6.0 + i as f32 * pitch, bottom - v * (bottom - top))).collect() };
    let ip = to_pts(&income);
    let sp = to_pts(&spend);
    let mut area = smooth_line(&ip);
    area.push(PathCmd::LineTo(Vec2::new(ip[ip.len() - 1].0, bottom)));
    area.push(PathCmd::LineTo(Vec2::new(ip[0].0, bottom)));
    area.push(PathCmd::Close);
    let a = s.vector(chart, "Income area", w, h, vec![Path { cmds: area, fill_rule: FillRule::NonZero }]);
    s.fill_paint(a, grad_v(&[(0.0, hex(0x4C5BF0).with_alpha(0.28)), (1.0, hex(0x4C5BF0).with_alpha(0.0))]));
    let l = s.vector(chart, "Income line", w, h, vec![Path { cmds: smooth_line(&ip), fill_rule: FillRule::NonZero }]);
    s.stroke(l, Cl::T(Tok::Brand), 2.5);
    let l2 = s.vector(chart, "Spending line", w, h, vec![Path { cmds: smooth_line(&sp), fill_rule: FillRule::NonZero }]);
    s.stroke(l2, Cl::T(Tok::Neg), 2.0);
    let (lx, ly) = ip[ip.len() - 1];
    let ring = s.ellipse(chart, "Marker ring", 18.0, 18.0, Cl::C(hex(0x4C5BF0).with_alpha(0.25)));
    s.at(ring, lx - 9.0, ly - 9.0);
    let dot = s.ellipse(chart, "Marker", 10.0, 10.0, Cl::T(Tok::Brand));
    s.stroke(dot, Cl::T(Tok::Card), 2.0);
    s.at(dot, lx - 5.0, ly - 5.0);
}

fn breakdown(s: &mut S, parent: NodeId) {
    let c = card(s, parent, "Spending by category", 16.0, 24.0);
    s.fixed_w(c, 360.0);
    s.fill_h(c);
    let t = s.col(c, "Title", 0.0, [0.0; 4]);
    s.text(t, "Spending by category", "Text/Heading", Cl::T(Tok::Ink));
    s.text(t, "October so far", "Text/Caption", Cl::T(Tok::Muted));
    let body = s.row(c, "Body", 20.0, [0.0; 4]);
    s.fill_w(body);
    s.align(body, Start, Center);

    let d = 140.0;
    let donut = s.frame(body, "Donut");
    s.size(donut, d, d);
    let parts = [("Housing", 38.0, Tok::Brand), ("Food", 24.0, Tok::Pos), ("Leisure", 22.0, Tok::Neg), ("Transport", 16.0, Tok::Faint)];
    let mut at = -std::f32::consts::FRAC_PI_2;
    for (name, pct, tok) in parts {
        let sweep = pct / 100.0 * std::f32::consts::TAU;
        let e = s.ellipse(donut, name, d, d, Cl::T(tok));
        s.n(e).kind = NodeKind::Ellipse(ArcData { start: at + 0.03, end: at + sweep - 0.03, inner_radius: 0.7 });
        at += sweep;
    }
    let total = s.text(donut, "$7.3k", "Text/Title", Cl::T(Tok::Ink));
    let tw = s.n(total).size.x;
    s.at(total, (d - tw) / 2.0, 46.0);
    let sub = s.text(donut, "spent", "Text/Caption", Cl::T(Tok::Muted));
    let sw = s.n(sub).size.x;
    s.at(sub, (d - sw) / 2.0, 76.0);

    let legend = s.col(body, "Legend", 12.0, [0.0; 4]);
    s.fill_w(legend);
    for (name, pct, tok) in parts {
        let r = s.row(legend, name, 0.0, [0.0; 4]);
        s.fill_w(r);
        s.spread(r, Center);
        let l = s.row(r, "Label", 8.0, [0.0; 4]);
        s.align(l, Start, Center);
        s.ellipse(l, "Dot", 10.0, 10.0, Cl::T(tok));
        s.text(l, name, "Text/Caption", Cl::T(Tok::Ink));
        s.text(r, &format!("{pct:.0}%"), "Text/Label", Cl::T(Tok::Muted));
    }
}

fn cell(s: &mut S, parent: NodeId, w: Option<f32>) -> NodeId {
    let c = s.row(parent, "Cell", 12.0, [0.0; 4]);
    match w {
        Some(w) => s.fixed_w(c, w),
        None => s.fill_w(c),
    }
    s.align(c, Start, Center);
    c
}

fn table(s: &mut S, main: NodeId) {
    let t = card(s, main, "Transactions", 0.0, 0.0);
    s.fill_w(t);
    let head = s.row(t, "Header", 0.0, [18.0, 24.0, 14.0, 24.0]);
    s.fill_w(head);
    s.spread(head, Center);
    s.text(head, "Recent transactions", "Text/Heading", Cl::T(Tok::Ink));
    s.text(head, "View all", "Text/Label", Cl::T(Tok::Brand));

    let cols = s.row(t, "Columns", 0.0, [10.0, 24.0, 10.0, 24.0]);
    s.fill_w(cols);
    s.fill(cols, Cl::T(Tok::Raised));
    for (label, w) in [("Name", None), ("Category", Some(160.0)), ("Date", Some(150.0)), ("Status", Some(130.0)), ("Amount", Some(130.0))] {
        let c = cell(s, cols, w);
        if label == "Amount" {
            s.end_aligned(c);
        }
        s.text(c, label, "Text/Micro", Cl::T(Tok::Muted));
    }
    let rows = [
        ("Hollis Studio", "Payroll", "10 Oct, 09:41", "Completed", "+$3,850.00", "HS", (0x4C5BF0, 0x8D5CF6)),
        ("Greenleaf Market", "Groceries", "09 Oct, 18:22", "Completed", "-$62.40", "GM", (0x12A67F, 0x36D3A8)),
        ("Corner Roast", "Coffee", "09 Oct, 08:12", "Completed", "-$4.80", "CR", (0xF29A2E, 0xFFD58A)),
        ("Northline Transit", "Transport", "08 Oct, 07:55", "Pending", "-$38.00", "NT", (0x2B7FFF, 0x59C3FF)),
        ("Driftwood Hosting", "Subscriptions", "07 Oct, 23:10", "Failed", "-$24.00", "DH", (0xE5436B, 0xFF8FA8)),
    ];
    for (i, (name, cat, date, status, amount, initials, (g0, g1))) in rows.into_iter().enumerate() {
        if i > 0 {
            let d = s.row(t, "Divider", 0.0, [0.0, 24.0, 0.0, 24.0]);
            s.fill_w(d);
            let r = s.rect(d, "Line", 1.0, 1.0, Cl::T(Tok::Border));
            s.fill_w(r);
        }
        let r = s.row(t, name, 0.0, [10.0, 24.0, 10.0, 24.0]);
        s.fill_w(r);
        s.align(r, Start, Center);
        let c = cell(s, r, None);
        let av = s.inst(c, "Avatar/Small");
        s.set_text(av, "Initials", initials);
        s.ov_fill(av, grad_d(&[(0.0, hex(g0)), (1.0, hex(g1))]));
        s.text(c, name, "Text/Body strong", Cl::T(Tok::Ink));
        let c = cell(s, r, Some(160.0));
        s.text(c, cat, "Text/Body", Cl::T(Tok::Muted));
        let c = cell(s, r, Some(150.0));
        s.text(c, date, "Text/Body", Cl::T(Tok::Muted));
        let c = cell(s, r, Some(130.0));
        let tone = match status {
            "Completed" => "Positive",
            "Pending" => "Neutral",
            _ => "Negative",
        };
        let tag = s.inst(c, &format!("Tag/{tone}"));
        s.set_text(tag, "Label", status);
        let c = cell(s, r, Some(130.0));
        s.end_aligned(c);
        let a = s.text(c, amount, "Text/Amount", Cl::T(if amount.starts_with('+') { Tok::Pos } else { Tok::Ink }));
        let _ = a;
    }
}

/// Builds the Web page and returns the dashboard frame.
pub fn build(s: &mut S) -> NodeId {
    let page = s.page(1);
    let root = s.screen_frame(page, "Web dashboard", 0.0, 0.0, 1440.0, 1040.0, Tok::Bg);
    s.make_stack(root, H, 0.0, [0.0; 4]);
    s.fixed(root, 1440.0, 1040.0);
    sidebar(s, root);
    let main = s.col(root, "Main", 24.0, [32.0; 4]);
    s.fill_w(main);
    s.fill_h(main);
    header(s, main);
    kpis(s, main);
    let charts = s.row(main, "Charts", 20.0, [0.0; 4]);
    s.fill_w(charts);
    s.fixed_h(charts, 296.0);
    cash_flow(s, charts);
    breakdown(s, charts);
    table(s, main);
    root
}
