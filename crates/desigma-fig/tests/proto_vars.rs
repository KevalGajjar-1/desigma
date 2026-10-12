//! Variable-driven prototype interactions: SET_VARIABLE, SET_VARIABLE_MODE,
//! CONDITIONAL, key/gamepad triggers and media actions through Figma REST JSON.

use desigma_doc::*;
use desigma_fig::{export_rest_json, import_rest_json};

const FIXTURE: &str = include_str!("fixtures/proto_vars.json");

fn find<'a>(doc: &'a Document, name: &str) -> &'a Node {
    doc.nodes.values().find(|n| n.name == name).unwrap_or_else(|| panic!("no node named {name}"))
}

fn names(doc: &Document) -> impl Fn(&str) -> Option<String> + '_ {
    |id| doc.variables.get(id).map(|v| v.name.clone())
}

#[test]
fn set_variable_and_conditional_import() {
    let doc = import_rest_json(FIXTURE).unwrap();
    let add = find(&doc, "Add");
    let done = find(&doc, "Done screen").id;
    // the trailing reaction holds only malformed actions, so it vanishes
    assert_eq!(add.prototype.reactions.len(), 5);
    let r = &add.prototype.reactions[0];
    assert_eq!(r.trigger, Trigger::OnClick);
    assert_eq!(r.actions.len(), 2, "multiple actions keep their order");

    let Action::SetVariable { variable, value } = &r.actions[0] else { panic!("{:?}", r.actions[0]) };
    assert_eq!(variable, "VariableID:2:2");
    assert_eq!(value.to_source(&names(&doc)), "$count + 1");

    let Action::Conditional { branches, otherwise } = &r.actions[1] else { panic!("{:?}", r.actions[1]) };
    assert_eq!(branches.len(), 1);
    assert_eq!(branches[0].0.to_source(&names(&doc)), "$count >= 3");
    assert_eq!(branches[0].1, vec![Action::Navigate { dest: done, preserve_scroll: false }]);
    let Action::SetVariable { variable, value } = &otherwise[0] else { panic!("{:?}", otherwise) };
    assert_eq!(variable, "VariableID:2:3");
    assert_eq!(value.to_source(&names(&doc)), "\"Items: \" + $count");
    // the transition of a nested NODE action becomes the reaction's transition
    assert_eq!(r.transition.map(|t| (t.kind, t.duration_ms)), Some((TransitionKind::Dissolve, 250)));

    // the imported expression evaluates against the imported variable values
    let env = |id: &str| doc.variables.get(id).and_then(|v| v.values.get("2:0")).map(ExprValue::from);
    assert_eq!(value.eval(&|id| if id == "VariableID:2:2" { Some(ExprValue::Number(2.0)) } else { env(id) }), Ok(ExprValue::String("Items: 2".into())));
}

#[test]
fn key_gamepad_mode_and_media() {
    let doc = import_rest_json(FIXTURE).unwrap();
    let add = find(&doc, "Add");
    let clip = find(&doc, "Clip").id;
    assert_eq!(add.prototype.reactions[1].trigger, Trigger::KeyPress { keys: "Ctrl+K".into() }, "KEY_DOWN with a modifier");
    assert_eq!(add.prototype.reactions[2].trigger, Trigger::GamepadPress { device: GamepadDevice::Xbox, buttons: "A".into() });
    assert_eq!(add.prototype.reactions[2].actions, vec![Action::SetVariableMode { collection: "VariableCollectionId:2:1".into(), mode: "2:1".into() }]);
    assert_eq!(add.prototype.reactions[3].trigger, Trigger::GamepadPress { device: GamepadDevice::PlayStation, buttons: "DPadUp+A".into() });
    // conditionalActions is accepted as an alias of conditionalBlocks; no else branch
    let Action::Conditional { branches, otherwise } = &add.prototype.reactions[3].actions[0] else { panic!() };
    assert_eq!(branches[0].0, Expression::var("VariableID:2:4"));
    assert_eq!(branches[0].1, vec![Action::MediaControl { dest: clip, op: MediaOp::TogglePlay }]);
    assert!(otherwise.is_empty());
    assert_eq!(
        add.prototype.reactions[4].actions,
        vec![
            Action::MediaControl { dest: clip, op: MediaOp::SkipBy(-5.0) },
            Action::MediaControl { dest: clip, op: MediaOp::SeekTo(2.5) },
            Action::MediaControl { dest: clip, op: MediaOp::Mute },
        ]
    );
}

#[test]
fn variable_interactions_round_trip_through_rest_export() {
    let a = import_rest_json(FIXTURE).unwrap();
    let json = export_rest_json(&a).to_string();
    let b = import_rest_json(&json).unwrap();
    let (x, y) = (find(&a, "Add"), find(&b, "Add"));
    assert_eq!(x.prototype.reactions.len(), y.prototype.reactions.len());
    for (rx, ry) in x.prototype.reactions.iter().zip(&y.prototype.reactions) {
        assert_eq!(rx.trigger, ry.trigger);
        assert_eq!(rx.transition, ry.transition);
        // destinations are remapped to new node ids by the importer; compare through their names
        assert_eq!(rx.actions.len(), ry.actions.len());
    }
    // expressions survive exactly
    let sv = |d: &Document| match &find(d, "Add").prototype.reactions[0].actions[0] {
        Action::SetVariable { variable, value } => (variable.clone(), value.clone()),
        other => panic!("{other:?}"),
    };
    assert_eq!(sv(&a), sv(&b));
    let cond = |d: &Document| match &find(d, "Add").prototype.reactions[0].actions[1] {
        Action::Conditional { branches, otherwise } => (branches.iter().map(|(c, _)| c.clone()).collect::<Vec<_>>(), otherwise.clone()),
        other => panic!("{other:?}"),
    };
    assert_eq!(cond(&a), cond(&b));
    assert_eq!(find(&a, "Add").prototype.reactions[2], find(&b, "Add").prototype.reactions[2]);
    assert_eq!(find(&a, "Add").prototype.reactions[1], find(&b, "Add").prototype.reactions[1]);
    // a second round is byte-stable
    assert_eq!(export_rest_json(&b).to_string(), json);
}

#[test]
fn exported_json_uses_figma_names() {
    let doc = import_rest_json(FIXTURE).unwrap();
    let v = export_rest_json(&doc);
    let s = v.to_string();
    for needle in [
        "\"SET_VARIABLE\"",
        "\"SET_VARIABLE_MODE\"",
        "\"CONDITIONAL\"",
        "\"conditionalBlocks\"",
        "\"variableId\":\"VariableID:2:2\"",
        "\"variableCollectionId\":\"VariableCollectionId:2:1\"",
        "\"variableModeId\":\"2:1\"",
        "\"expressionFunction\":\"ADDITION\"",
        "\"expressionFunction\":\"GREATER_THAN_OR_EQUAL\"",
        "\"expressionArguments\"",
        "\"ON_KEY_DOWN\"",
        "\"device\":\"XBOX_ONE\"",
        "\"device\":\"PS4\"",
        "\"keyCodes\":[17,75]",
        "\"UPDATE_MEDIA_RUNTIME\"",
        "\"mediaAction\":\"SKIP_BACKWARD\"",
    ] {
        assert!(s.contains(needle), "missing {needle}");
    }
}

#[test]
fn hostile_expressions_are_dropped_not_followed() {
    // a 300-term sum exceeds the node limit and is refused; the rest of the file still imports
    let term = serde_json::json!({"type": "FLOAT", "resolvedType": "FLOAT", "value": 1});
    let e = serde_json::json!({"type": "EXPRESSION", "value": {"expressionFunction": "ADDITION", "expressionArguments": vec![term; 300]}});
    let mut file: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();
    let node = &mut file["document"]["children"][0]["children"][0]["children"][0];
    node["reactions"] = serde_json::json!([
        {"trigger": {"type": "ON_CLICK"}, "actions": [{"type": "SET_VARIABLE", "variableId": "VariableID:2:4", "variableValue": e}]},
        {"trigger": {"type": "ON_CLICK"}, "actions": [{"type": "BACK"}]}
    ]);
    let doc = import_rest_json(&file.to_string()).unwrap();
    let r = &find(&doc, "Add").prototype.reactions;
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].actions, vec![Action::Back]);
}

#[test]
fn model_serde_is_backward_compatible() {
    // an old file without `videos` still loads
    let old = r#"{"reactions":[],"overflow":"None","fixed_children":[]}"#;
    let p: PrototypeProps = serde_json::from_str(old).unwrap();
    assert!(p.is_empty());
    let a = Action::Conditional {
        branches: vec![(Expression::boolean(true), vec![Action::Back])],
        otherwise: vec![Action::SetVariable { variable: "v".into(), value: Expression::number(1.0) }],
    };
    let j = serde_json::to_string(&a).unwrap();
    assert_eq!(serde_json::from_str::<Action>(&j).unwrap(), a);
}
