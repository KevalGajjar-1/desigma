//! UI tests for variables: the modal, the inspector binding UI and mode switching.

use super::*;
use crate::panels::rail;
use desigma_doc::{PaintKind, VariableKind, VariableValue};

fn open_modal(s: &mut Sim) {
    let top = rail::cell_tops()[3];
    s.click(Pos2::new(28.0, top + 20.0));
    assert!(s.app().vars.open, "the Variables rail item opens the modal");
}

/// (collection id, light mode id, dark mode id) of the demo Theme collection.
fn theme_ids(app: &App) -> (String, String, String) {
    let c = app.doc.variable_collections.values().find(|c| c.name == "Theme").expect("Theme collection");
    (c.id.clone(), c.modes[0].id.clone(), c.modes[1].id.clone())
}

fn var<'a>(app: &'a App, name: &str) -> &'a desigma_doc::Variable {
    app.doc.variables.values().find(|v| v.name == name).unwrap_or_else(|| panic!("no variable {name}"))
}

/// Pixel of the Card rendered through the document renderer.
fn card_pixel(s: &Sim) -> [u8; 4] {
    let pm = desigma_render::render_node(&s.app().doc, s.by_name("Card"), 1.0).unwrap();
    let p = pm.pixel(268, 40).unwrap();
    [p.red(), p.green(), p.blue(), p.alpha()]
}

#[test]
fn demo_has_a_theme_collection_bound_to_the_card() {
    let s = Sim::new();
    let (c, light, dark) = theme_ids(s.app());
    let coll = &s.app().doc.variable_collections[&c];
    assert_eq!(coll.modes.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["Light", "Dark"]);
    assert_eq!(coll.default_mode, light);
    let card = s.app().doc.get(s.by_name("Card")).unwrap();
    assert_eq!(card.variables.bindings["fills/0"], var(s.app(), "Surface/Card").id);
    let title = s.app().doc.get(s.by_name("Quarterly report")).unwrap();
    assert_eq!(title.variables.bindings["fills/0"], var(s.app(), "Text/Primary").id);
    assert!(var(s.app(), "Surface/Card").values.contains_key(&dark));
}

#[test]
fn switching_the_card_to_dark_changes_rendered_pixels() {
    let mut s = Sim::with(Options { select: Some("Card".into()), ..Default::default() });
    let card = s.by_name("Card");
    let (c, _, dark) = theme_ids(s.app());
    let light_px = card_pixel(&s);
    assert!(light_px[0] > 240, "light card is white-ish: {light_px:?}");

    // Appearance -> mode dropdown of the Theme collection: Auto, Light, Dark.
    let dd = s.rect_of("mode:Theme");
    s.click(dd.center());
    // menu rows: Auto, Light, Dark (24px rows under the field, 8px padding)
    let row = |i: f32| Pos2::new(dd.left() + 20.0, dd.bottom() + 2.0 + theme::MENU_PADDING + i * theme::MENU_ITEM_HEIGHT + 12.0);
    s.click(row(2.0));
    assert_eq!(s.app().doc.get(card).unwrap().variables.modes.get(&c), Some(&dark));
    let dark_px = card_pixel(&s);
    assert!(dark_px[0] < 80, "dark card is dark: {dark_px:?}");
    assert_ne!(light_px, dark_px);

    // the canvas tile cache follows too: render the page the way the canvas does
    let page = s.app().page;
    let mut cache = desigma_render::TileCache::new();
    let vp = desigma_render::Viewport { offset: DV::new(540.0, 100.0), zoom: 1.0, dpr: 1.0 };
    let tiles = cache.render_tiles(&s.app().doc, page, &vp, (256, 256), 1, desigma_render::TileBudget::unlimited());
    assert!(tiles.pending() == 0);

    // one undo step brings the light theme back
    s.cmd_key(Key::Z);
    assert!(s.app().doc.get(card).unwrap().variables.modes.is_empty());
    assert_eq!(card_pixel(&s), light_px);
    s.cmd_key(Key::Y);
    assert_eq!(card_pixel(&s), dark_px);
}

#[test]
fn rail_item_opens_modal_and_escape_closes_it() {
    let mut s = Sim::new();
    open_modal(&mut s);
    assert_eq!(s.app().vars.collection.as_deref(), s.app().doc.variable_collections.values().find(|c| c.name == "Theme").map(|c| c.id.as_str()));
    s.key(Key::Escape);
    assert!(!s.app().vars.open);
}

#[test]
fn editing_a_number_cell_is_one_undo_step() {
    let mut s = Sim::new();
    open_modal(&mut s);
    let (_, light, _) = theme_ids(s.app());
    let r = s.rect_of(&format!("vars-num:Radius/Card:{light}"));
    s.click(Pos2::new(r.left() + 30.0, r.center().y));
    s.type_text("20");
    s.key(Key::Enter);
    assert_eq!(var(s.app(), "Radius/Card").values[&light], VariableValue::Number(20.0));
    // the bound Card radius follows at render time
    s.cmd_key(Key::Z);
    assert_eq!(var(s.app(), "Radius/Card").values[&light], VariableValue::Number(12.0));
    s.cmd_key(Key::Y);
    assert_eq!(var(s.app(), "Radius/Card").values[&light], VariableValue::Number(20.0));
}

#[test]
fn create_variable_menu_adds_a_typed_variable() {
    let mut s = Sim::new();
    open_modal(&mut s);
    let before = s.app().doc.variables.len();
    let b = s.rect_of("vars-create");
    s.click(b.center());
    assert!(s.app().vars.menu.is_some());
    s.key(Key::ArrowDown);
    s.key(Key::ArrowDown); // Color, Number
    s.key(Key::Enter);
    s.frame(vec![]);
    assert_eq!(s.app().doc.variables.len(), before + 1);
    let v = s.app().doc.variables.values().find(|v| v.name == "Number").expect("new Number variable");
    assert_eq!(v.kind, VariableKind::Number);
    assert_eq!(v.values.len(), 2, "one value per mode");
    // the new variable's name field has focus (typing), so undo through the app
    s.shell.app.undo();
    assert_eq!(s.app().doc.variables.len(), before);
}

#[test]
fn add_collection_and_mode() {
    let mut s = Sim::new();
    open_modal(&mut s);
    let colls = s.app().doc.variable_collections.len();
    s.click(s.rect_of("vars-add-collection").center());
    assert_eq!(s.app().doc.variable_collections.len(), colls + 1);
    s.type_text("Spacing");
    s.key(Key::Enter);
    assert!(s.app().doc.variable_collections.values().any(|c| c.name == "Spacing"));
    let id = s.app().vars.collection.clone().unwrap();
    s.click(s.rect_of("vars-add-mode").center());
    assert_eq!(s.app().doc.variable_collections[&id].modes.len(), 2);
}

#[test]
fn bind_button_opens_picker_and_binds_a_fill() {
    let mut s = Sim::with(Options { select: Some("Hero".into()), ..Default::default() });
    let hero = s.by_name("Hero");
    let sw = s.rect_of("sw:false:0");
    s.mv(Pos2::new(sw.right() + 40.0, sw.center().y));
    s.frame(vec![]);
    s.click(s.rect_of("bind:fills/0").center());
    assert!(s.app().vars.bind_picker.is_some());
    s.frame(vec![]);
    let item = s.rect_of("pick:Brand/Primary");
    s.click(item.center());
    let brand = var(s.app(), "Brand/Primary").id.clone();
    let n = s.app().doc.get(hero).unwrap();
    assert_eq!(n.variables.bindings["fills/0"], brand);
    assert!(s.app().vars.bind_picker.is_none());
    // the field now shows the variable pill with a detach action
    s.frame(vec![]);
    let pill = s.rect_of("bound:fills/0");
    s.mv(pill.center());
    s.frame(vec![]);
    s.click(Pos2::new(pill.right() - 13.0, pill.center().y));
    let n = s.app().doc.get(hero).unwrap();
    assert!(n.variables.bindings.is_empty());
    assert!(matches!(n.fills[0].kind, PaintKind::Solid(_)));
}

#[test]
fn numeric_fields_bind_and_unbind() {
    let mut s = Sim::with(Options { select: Some("Hero".into()), ..Default::default() });
    let hero = s.by_name("Hero");
    // add a number variable to bind the width to
    let mut store = desigma_doc::VariableStore::from_doc(&s.app().doc);
    let (c, light, _) = theme_ids(s.app());
    let w = store.add_variable(&c, "Size/Wide", VariableKind::Number).unwrap();
    store.set_value(&w, &light, VariableValue::Number(222.0));
    s.shell.app.commit_store(store, &[], &[]);
    s.frame(vec![]);
    let field = s.field_rect("w");
    s.mv(field.center());
    s.frame(vec![]);
    s.click(s.rect_of("bind:width").center());
    s.frame(vec![]);
    s.click(s.rect_of("pick:Size/Wide").center());
    let n = s.app().doc.get(hero).unwrap();
    assert_eq!(n.variables.bindings["width"], w);
    assert_eq!(n.size.x, 222.0);
    s.cmd_key(Key::Z);
    assert!(s.app().doc.get(hero).unwrap().variables.is_empty());
}

#[test]
fn names_edit_inline_and_keep_the_folder() {
    let mut s = Sim::new();
    open_modal(&mut s);
    let r = s.rect_of("vars-name:Text/Primary");
    s.click(Pos2::new(r.left() + 20.0, r.center().y));
    s.type_text("Ink");
    s.key(Key::Enter);
    assert!(s.app().doc.variables.values().any(|v| v.name == "Text/Ink"));
    let m = s.rect_of("vars-mode:Dark");
    s.click(Pos2::new(m.left() + 20.0, m.center().y));
    s.type_text("Night");
    s.key(Key::Enter);
    let (c, _, dark) = theme_ids(s.app());
    assert_eq!(s.app().doc.variable_collections[&c].modes.iter().find(|m| m.id == dark).unwrap().name, "Night");
}

#[test]
fn row_menu_deletes_a_variable_and_its_bindings_in_one_step() {
    let mut s = Sim::new();
    open_modal(&mut s);
    let ink = var(s.app(), "Text/Primary").id.clone();
    let title = s.by_name("Quarterly report");
    assert_eq!(s.app().doc.get(title).unwrap().variables.bindings["fills/0"], ink);
    let r = s.rect_of("vars-name:Text/Primary");
    let p = Pos2::new(r.left() + 20.0, r.center().y);
    s.right_click(p);
    assert!(s.app().vars.menu.is_some());
    // Rename, Duplicate, Delete
    s.click(Pos2::new(p.x + 20.0, p.y + theme::MENU_PADDING + 2.0 * theme::MENU_ITEM_HEIGHT + 12.0));
    assert!(!s.app().doc.variables.contains_key(&ink));
    assert!(!s.app().doc.get(title).unwrap().variables.bindings.contains_key("fills/0"));
    s.shell.app.undo();
    assert!(s.app().doc.variables.contains_key(&ink));
    assert_eq!(s.app().doc.get(title).unwrap().variables.bindings["fills/0"], ink);
}

#[test]
fn alias_button_picks_another_variable() {
    let mut s = Sim::new();
    open_modal(&mut s);
    let (_, light, _) = theme_ids(s.app());
    let cell = s.rect_of(&format!("vars-hex:Text/Primary:{light}"));
    s.mv(cell.center());
    s.frame(vec![]);
    s.click(s.rect_of(&format!("vars-bind:Text/Primary:{light}")).center());
    assert!(s.app().vars.alias_picker.is_some());
    s.frame(vec![]);
    s.click(s.rect_of("pick:Gray/900").center());
    let target = var(s.app(), "Gray/900").id.clone();
    assert_eq!(var(s.app(), "Text/Primary").values[&light], VariableValue::Alias(target.clone()));
    // the alias shows as a pill; its detach button bakes the resolved colour back
    s.frame(vec![]);
    let pill = s.rect_of(&format!("vars-cell:Text/Primary:{light}"));
    s.mv(pill.center());
    s.frame(vec![]);
    s.click(Pos2::new(pill.right() - 13.0, pill.center().y));
    assert!(matches!(var(s.app(), "Text/Primary").values[&light], VariableValue::Color(_)));
    // cycles and type mismatches are not offered: Gray/900 cannot alias a Theme colour that aliases it
    let store = desigma_doc::VariableStore::from_doc(&s.app().doc);
    assert!(!store.can_alias(&target, &var(s.app(), "Radius/Card").id));
}

#[test]
fn dragging_the_grip_reorders_rows() {
    let mut s = Sim::new();
    open_modal(&mut s);
    let order = |s: &Sim| {
        let c = s.app().vars.collection.clone().unwrap();
        s.app().doc.variable_collections[&c].variables.iter().map(|id| s.app().doc.variables[id].name.clone()).collect::<Vec<_>>()
    };
    let before = order(&s);
    let pos = |n: &str| before.iter().position(|x| x == n).unwrap();
    assert!(pos("Text/Primary") < pos("Text/Secondary"));
    let grip = s.rect_of("vars-grip:Text/Secondary");
    let target = s.rect_of("vars-name:Text/Primary");
    // hover the row first so the grip is drawn and interactive
    s.mv(grip.center());
    s.frame(vec![]);
    s.drag(grip.center(), Pos2::new(target.center().x, target.top() - 6.0));
    let after = order(&s);
    let p = |n: &str| after.iter().position(|x| x == n).unwrap();
    assert!(p("Text/Secondary") < p("Text/Primary"), "{after:?}");
    s.shell.app.undo();
    assert_eq!(order(&s), before);
}

#[test]
fn text_fill_edits_only_the_selected_range_and_binds_the_layer() {
    let mut s = Sim::new();
    s.key(Key::T);
    s.click(s.screen(300.0, 820.0));
    s.type_text("hello world");
    let id = s.app().selection[0];
    s.cmd_key(Key::Home);
    s.mods = Modifiers::SHIFT;
    for _ in 0..5 {
        s.key(Key::ArrowRight);
    }
    s.mods = Modifiers::NONE;
    s.frame(vec![]);
    // the Fill section shows the text colour; editing it recolours just "hello"
    s.scroll_inspector_down();
    let hex = s.rect_of("text-hex");
    s.click(Pos2::new(hex.left() + 30.0, hex.center().y));
    s.type_text("FF0000");
    s.key(Key::Enter);
    let t = text_of(&s, id);
    assert_eq!(t.runs.len(), 1, "{:?}", t.runs);
    assert_eq!((t.runs[0].start, t.runs[0].end), (0, 5));
    assert!(matches!(&t.runs[0].style.fills[0].kind, PaintKind::Solid(c) if c.to_hex() == "FF0000"));
    assert!(!matches!(&t.style.fills[0].kind, PaintKind::Solid(c) if c.to_hex() == "FF0000"), "the rest keeps its colour");
    s.key(Key::Escape);
    assert!(s.app().canvas.text.is_none(), "Escape leaves the text session");

    // with one colour, the whole layer's colour can be bound to a variable (`fills/0`)
    s.key(Key::T);
    s.click(s.screen(300.0, 900.0));
    s.type_text("plain");
    let id = s.app().selection[0];
    s.key(Key::Escape);
    s.shell.app.selection = vec![id];
    s.scroll_inspector_down();
    let sw = s.rect_of("text-sw");
    s.mv(Pos2::new(sw.right() + 40.0, sw.center().y));
    s.frame(vec![]);
    s.click(s.rect_of("bind:fills/0").center());
    s.frame(vec![]);
    s.click(s.rect_of("pick:Text/Primary").center());
    let ink = var(s.app(), "Text/Primary").id.clone();
    let n = s.app().doc.get(id).unwrap();
    assert_eq!(n.variables.bindings["fills/0"], ink);
    let t = text_of(&s, id);
    assert!(matches!(&t.style.fills[0].kind, PaintKind::Solid(c) if c.to_hex() == "1E1E1E"), "the raw colour takes the variable's value");
    s.cmd_key(Key::Z);
    assert!(s.app().doc.get(id).unwrap().variables.is_empty(), "binding is one undo step");
}
