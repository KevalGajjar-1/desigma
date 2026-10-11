//! Demo document shown when no file is given.

use crate::edit::Edit;
use crate::layout;
use crate::theme;
use desigma_doc::*;

fn txt(doc: &mut Document, parent: NodeId, x: f32, y: f32, s: &str, size: f32, weight: u16, color: Color) -> NodeId {
    let id = doc.alloc_id();
    let style = TextStyle { font_size: size, font_weight: weight, fills: vec![Paint::solid(color)], ..Default::default() };
    let mut n = Node::text(id, Rect::new(x, y, 10.0, 10.0), s, style);
    if let NodeKind::Text(t) = &n.kind {
        n.size = layout::estimate_text_size(t);
    }
    n.name = s.to_string();
    doc.insert(parent, n).unwrap()
}

fn shape(doc: &mut Document, parent: NodeId, n: Node, name: &str, radius: f32) -> NodeId {
    let mut n = n;
    n.name = name.into();
    n.corner_radii = [radius; 4];
    doc.insert(parent, n).unwrap()
}

pub fn demo_document() -> Document {
    let mut d = Document::new("Untitled");
    let page = d.add_page("Page 1");
    d.add_page("Components");
    d.add_page("Archive");
    let ink = Color::rgb(0x1E, 0x1E, 0x1E);
    let brand = Color::rgb(0x0C, 0x8C, 0xE9);
    let grey = Color::rgb(0x8A, 0x8A, 0x8A);

    // Mobile screen
    let id = d.alloc_id();
    let mut f = Node::frame(id, Rect::new(120.0, 120.0, 360.0, 640.0));
    f.name = "Welcome".into();
    f.corner_radii = [24.0; 4];
    if let Some(p) = f.kind.frame_props_mut() {
        p.clips_content = true;
    }
    let welcome = d.insert(page, f).unwrap();
    let id = d.alloc_id();
    shape(&mut d, welcome, Node::rectangle(id, Rect::new(0.0, 0.0, 360.0, 240.0)), "Hero", 0.0);
    if let Some(n) = d.get_mut(id) {
        n.fills = vec![Paint::solid(brand)];
    }
    let id = d.alloc_id();
    let av = shape(&mut d, welcome, Node::ellipse(id, Rect::new(24.0, 200.0, 80.0, 80.0)), "Avatar", 0.0);
    if let Some(n) = d.get_mut(av) {
        n.fills = vec![Paint::solid(Color::rgb(0xF2, 0xC9, 0x4C))];
        n.stroke.paints = vec![Paint::solid(Color::WHITE)];
        n.stroke.weight = 4.0;
    }
    txt(&mut d, welcome, 24.0, 304.0, "Welcome back", 32.0, 700, ink);
    txt(&mut d, welcome, 24.0, 352.0, "Pick up where you left off.", 16.0, 400, grey);
    let id = d.alloc_id();
    let btn = shape(&mut d, welcome, Node::rectangle(id, Rect::new(24.0, 560.0, 312.0, 48.0)), "Button", 8.0);
    if let Some(n) = d.get_mut(btn) {
        n.fills = vec![Paint::solid(ink)];
    }
    txt(&mut d, welcome, 132.0, 574.0, "Continue", 16.0, 500, Color::WHITE);

    // Auto layout card
    let id = d.alloc_id();
    let mut card = Node::frame(id, Rect::new(560.0, 120.0, 10.0, 10.0));
    card.name = "Card".into();
    card.corner_radii = [12.0; 4];
    card.effects = vec![Effect::DropShadow {
        color: Color::rgba(0, 0, 0, 40),
        offset: Vec2::new(0.0, 4.0),
        radius: 12.0,
        spread: 0.0,
        visible: true,
        show_behind: false,
    }];
    card.kind = NodeKind::Frame(FrameProps {
        clips_content: false,
        auto_layout: Some(AutoLayout {
            direction: LayoutDirection::Vertical,
            item_spacing: 8.0,
            padding: [20.0; 4],
            primary_sizing: Sizing::Hug,
            counter_sizing: Sizing::Fixed,
            ..Default::default()
        }),
        grids: vec![],
    });
    card.size = Vec2::new(280.0, 10.0);
    let card = d.insert(page, card).unwrap();
    txt(&mut d, card, 0.0, 0.0, "Quarterly report", 20.0, 600, ink);
    txt(&mut d, card, 0.0, 0.0, "Revenue grew 18% over the last quarter.", 12.0, 400, grey);
    let id = d.alloc_id();
    let bar = shape(&mut d, card, Node::rectangle(id, Rect::new(0.0, 0.0, 240.0, 8.0)), "Progress", 4.0);
    if let Some(n) = d.get_mut(bar) {
        n.fills = vec![Paint::solid(brand)];
        n.layout_child.horizontal = Sizing::Fill;
    }

    // Desktop board
    let id = d.alloc_id();
    let mut board = Node::frame(id, Rect::new(560.0, 360.0, 640.0, 400.0));
    board.name = "Dashboard".into();
    board.fills = vec![Paint::solid(Color::rgb(0xFA, 0xFA, 0xFA))];
    let board = d.insert(page, board).unwrap();
    for (i, (name, c)) in [("Tile A", 0xFF6B6B), ("Tile B", 0x4ECDC4), ("Tile C", 0xFFD166)].iter().enumerate() {
        let id = d.alloc_id();
        let t = shape(&mut d, board, Node::rectangle(id, Rect::new(24.0 + i as f32 * 200.0, 24.0, 184.0, 120.0)), name, 12.0);
        d.get_mut(t).unwrap().fills = vec![Paint::solid(Color::from_hex(&format!("{c:06X}")).unwrap())];
    }
    let id = d.alloc_id();
    let st = d.insert(board, Node::new(id, NodeKind::Star { count: 5, inner_ratio: 0.5 })).unwrap();
    {
        let n = d.get_mut(st).unwrap();
        n.transform = Affine::translate(60.0, 200.0);
        n.size = Vec2::new(120.0, 120.0);
        n.fills = vec![Paint::solid(Color::rgb(0xFF, 0xB7, 0x03))];
    }
    let id = d.alloc_id();
    let pg = d.insert(board, Node::new(id, NodeKind::Polygon { count: 3 })).unwrap();
    {
        let n = d.get_mut(pg).unwrap();
        n.transform = Affine::translate(240.0, 210.0);
        n.size = Vec2::new(100.0, 100.0);
        n.fills = vec![Paint::solid(Color::rgb(0x8E, 0x7C, 0xC3))];
    }
    txt(&mut d, board, 400.0, 220.0, "Dashboard", 28.0, 700, ink);

    // Library styles
    d.styles.insert("Brand/Primary".into(), Style::Fill(vec![Paint::solid(brand)]));
    d.styles.insert("Neutral/Ink".into(), Style::Fill(vec![Paint::solid(ink)]));
    d.styles.insert("Accent/Sun".into(), Style::Fill(vec![Paint::solid(Color::rgb(0xFF, 0xB7, 0x03))]));
    d.styles.insert(
        "Heading/Large".into(),
        Style::Text(TextStyle { font_size: 32.0, font_weight: 700, line_height: LineHeight::Pixels(40.0), ..Default::default() }),
    );
    d.styles.insert(
        "Body/Regular".into(),
        Style::Text(TextStyle { font_size: 16.0, line_height: LineHeight::Pixels(24.0), ..Default::default() }),
    );
    if let Some(p) = d.get_mut(page) {
        p.kind = NodeKind::Page { background: Color::from_hex(&format!("{:06X}", theme_page_bg())).unwrap() };
    }

    let mut ed = Edit::new();
    layout::layout_node(&mut d, &mut ed, card);
    d
}

fn theme_page_bg() -> u32 {
    theme::DEFAULT_PAGE_BG
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_has_pages_and_laid_out_card() {
        let d = demo_document();
        assert_eq!(d.pages.len(), 3);
        let card = d.nodes.values().find(|n| n.name == "Card").unwrap();
        assert!(card.size.y > 60.0, "card hugs its children vertically");
        let kids: Vec<f32> = card.children.iter().map(|c| d.get(*c).unwrap().transform.ty).collect();
        assert!(kids.windows(2).all(|w| w[1] > w[0]));
    }
}
