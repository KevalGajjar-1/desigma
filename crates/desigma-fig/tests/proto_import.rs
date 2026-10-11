use desigma_doc::*;
use desigma_fig::{export_rest_json, import_rest_json};

const FIXTURE: &str = include_str!("fixtures/proto_file.json");

fn find<'a>(doc: &'a Document, name: &str) -> &'a Node {
    doc.nodes.values().find(|n| n.name == name).unwrap_or_else(|| panic!("no node named {name}"))
}

#[test]
fn reactions_map_triggers_actions_and_transitions() {
    let doc = import_rest_json(FIXTURE).unwrap();
    let dash = find(&doc, "Dashboard").id;
    let menu = find(&doc, "Menu").id;
    let btn = find(&doc, "Button");
    assert_eq!(btn.prototype.reactions.len(), 3);
    let r = &btn.prototype.reactions[0];
    assert_eq!(r.trigger, Trigger::OnClick);
    assert_eq!(r.actions, vec![Action::Navigate { dest: dash, preserve_scroll: true }]);
    let t = r.transition.unwrap();
    assert_eq!(t.kind, TransitionKind::SmartAnimate);
    assert_eq!(t.duration_ms, 450);
    assert_eq!(t.easing, Easing::Bezier([0.1, 0.2, 0.3, 0.4]));
    assert_eq!(btn.prototype.reactions[1].trigger, Trigger::AfterDelay { delay_ms: 2500 });
    assert_eq!(btn.prototype.reactions[1].actions, vec![Action::Back]);
    assert_eq!(btn.prototype.reactions[2].trigger, Trigger::KeyPress { keys: "Space".into() });
    assert_eq!(btn.prototype.reactions[2].actions, vec![Action::OpenUrl { url: "https://example.com".into(), new_tab: false }]);

    // legacy single `action`, overlay presentation read from the destination
    let mb = find(&doc, "Menu button");
    let r = &mb.prototype.reactions[0];
    assert_eq!(r.trigger, Trigger::MouseEnter { delay_ms: 120 });
    let Action::OpenOverlay { dest, position, offset, close_on_click_outside, background } = &r.actions[0] else { panic!("{:?}", r.actions) };
    assert_eq!(*dest, menu);
    assert_eq!(*position, OverlayPosition::Manual);
    assert_eq!(*offset, Vec2::new(4.0, 8.0));
    assert!(*close_on_click_outside);
    assert!((background.unwrap().a - 0.5).abs() < 1e-6);
    let t = r.transition.unwrap();
    assert_eq!(t.kind, TransitionKind::MoveIn(Direction::Bottom));
    assert!(matches!(t.easing, Easing::Spring { stiffness, .. } if stiffness == 300.0));
}

#[test]
fn legacy_node_level_transition_fields() {
    let doc = import_rest_json(FIXTURE).unwrap();
    let dash = find(&doc, "Dashboard").id;
    let l = find(&doc, "Legacy link");
    assert_eq!(l.prototype.reactions.len(), 1);
    assert_eq!(l.prototype.reactions[0].actions, vec![Action::Navigate { dest: dash, preserve_scroll: false }]);
    let t = l.prototype.reactions[0].transition.unwrap();
    assert_eq!((t.kind, t.duration_ms), (TransitionKind::Dissolve, 200));
}

#[test]
fn unknown_and_dangling_entries_are_dropped() {
    let doc = import_rest_json(FIXTURE).unwrap();
    let menu = find(&doc, "Menu");
    // the unknown trigger and the action to a missing node vanish; the known one keeps its CLOSE
    assert_eq!(menu.prototype.reactions.len(), 1);
    assert_eq!(menu.prototype.reactions[0].actions, vec![Action::CloseOverlay]);
    // flow pointing at a node that does not exist is skipped
    assert_eq!(doc.flows.len(), 1);
    assert_eq!(doc.flows[0].name, "Onboarding");
    assert_eq!(doc.flows[0].start, find(&doc, "Welcome").id);
    assert_eq!(doc.flows[0].page, doc.pages[0]);
}

#[test]
fn scroll_behaviour() {
    let doc = import_rest_json(FIXTURE).unwrap();
    let dash = find(&doc, "Dashboard");
    assert_eq!(dash.prototype.overflow, Overflow::Vertical);
    let fixed: Vec<&str> = dash.prototype.fixed_children.iter().map(|c| doc.get(*c).unwrap().name.as_str()).collect();
    assert_eq!(fixed, ["Header", "Old fixed"], "FIXED and legacy isFixed, not SCROLLS");
    assert_eq!(find(&doc, "Welcome").prototype.overflow, Overflow::None);
}

#[test]
fn prototype_round_trips_through_rest_export() {
    let a = import_rest_json(FIXTURE).unwrap();
    let json = export_rest_json(&a).to_string();
    let b = import_rest_json(&json).unwrap();
    let names: Vec<String> = a.nodes.values().map(|n| n.name.clone()).collect();
    for name in names {
        let (x, y) = (find(&a, &name), find(&b, &name));
        assert_eq!(x.prototype.overflow, y.prototype.overflow, "{name}");
        assert_eq!(x.prototype.reactions.len(), y.prototype.reactions.len(), "{name}");
        assert_eq!(x.prototype.fixed_children.len(), y.prototype.fixed_children.len(), "{name}");
        for (rx, ry) in x.prototype.reactions.iter().zip(&y.prototype.reactions) {
            assert_eq!(rx.trigger, ry.trigger, "{name}");
            assert_eq!(rx.transition, ry.transition, "{name}");
            assert_eq!(rx.actions.len(), ry.actions.len(), "{name}");
        }
    }
    assert_eq!(b.flows.len(), 1);
    assert_eq!(b.flows[0].name, "Onboarding");
    // a second round is byte-stable
    assert_eq!(export_rest_json(&b).to_string(), json);
    // overlay presentation survives
    let mb = find(&b, "Menu button");
    assert!(matches!(mb.prototype.reactions[0].actions[0], Action::OpenOverlay { position: OverlayPosition::Manual, close_on_click_outside: true, .. }));
}

#[test]
fn documents_without_prototype_data_export_none() {
    let mut d = Document::new("t");
    let p = d.add_page("p");
    let id = d.alloc_id();
    d.insert(p, Node::frame(id, Rect::new(0.0, 0.0, 10.0, 10.0))).unwrap();
    let j = export_rest_json(&d).to_string();
    assert!(!j.contains("reactions") && !j.contains("flowStartingPoints") && !j.contains("overflowDirection"));
}
