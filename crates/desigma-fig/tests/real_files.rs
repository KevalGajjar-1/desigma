//! Import checks against the owner's private design files. They are ignored by
//! default and skip when a file is absent (the files are local and never
//! committed). Run with `cargo test -p desigma-fig --test real_files -- --ignored`;
//! set `DESIGMA_REFERENCE_DIR` to point at the folder holding them.

use desigma_doc::*;
use desigma_fig::import_fig;
use std::path::PathBuf;

fn reference(name: &str) -> Option<Vec<u8>> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("DESIGMA_REFERENCE_DIR").map(PathBuf::from).into_iter().collect();
    // `reference/` of this checkout or of any ancestor (worktrees live inside the main checkout).
    let mut cur = Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    while let Some(d) = cur {
        dirs.push(d.join("reference"));
        cur = d.parent().map(PathBuf::from);
    }
    dirs.into_iter().map(|d| d.join(name)).find_map(|p| std::fs::read(p).ok())
}

fn is_clone(n: &Node) -> bool {
    n.source_id.as_deref().is_some_and(|s| s.starts_with('I'))
}

fn subtree(doc: &Document, root: NodeId) -> Vec<&Node> {
    let mut out = Vec::new();
    doc.walk(root, |n, _| out.push(doc.get(n.id).unwrap()));
    out
}

#[test]
#[ignore = "needs the private reference/dray-insight.fig"]
fn dray_insight_imports_with_library_instances_resolved() {
    let Some(bytes) = reference("dray-insight.fig") else {
        eprintln!("dray-insight.fig not found: skipped");
        return;
    };
    let doc = import_fig(&bytes).expect("import");
    assert!(!doc.nodes.values().any(|n| matches!(n.kind, NodeKind::Unknown { .. })), "no Unknown node kinds");

    let kit = doc.pages.iter().copied().find(|p| doc.get(*p).unwrap().name.contains("UI Kit")).expect("UI Kit page");
    let own: Vec<&Node> = subtree(&doc, kit).into_iter().filter(|n| !is_clone(n)).collect();
    assert_eq!(own.len(), 224, "nodes authored on the UI Kit page");
    let instances: Vec<&&Node> = own.iter().filter(|n| matches!(n.kind, NodeKind::Instance { .. })).collect();
    assert_eq!(instances.len(), 66);
    for i in &instances {
        let comp = instance_component(i).unwrap_or_else(|| panic!("instance {:?} unresolved", i.name));
        assert!(is_library_node(&doc, comp), "points at a library component");
        assert!(!i.children.is_empty(), "instance {:?} has no children", i.name);
    }

    let lp = doc.library_page.expect("library page");
    assert!(!doc.pages.contains(&lp));
    assert!(doc.library.external.len() > 50, "remote components are external assets");
    assert!(doc.library.external.values().all(|e| e.library_id.starts_with(REMOTE_LIBRARY_PREFIX)));

    assert_eq!(doc.variables.len(), 112);
    assert_eq!(doc.variable_collections.len(), 8);
    assert!(
        doc.variables.values().any(|v| v.values.values().any(|x| matches!(x, VariableValue::Alias(t) if doc.variables.contains_key(t)))),
        "aliases resolve"
    );
    assert!(doc.nodes.values().any(|n| !n.variables.bindings.is_empty()), "nodes bind variables");
    assert_eq!(doc.images.len(), 34);
}

#[test]
#[ignore = "needs the private reference/shadcn-ui.fig"]
fn shadcn_ui_imports_with_local_components() {
    let Some(bytes) = reference("shadcn-ui.fig") else {
        eprintln!("shadcn-ui.fig not found: skipped");
        return;
    };
    let doc = import_fig(&bytes).expect("import");
    assert!(!doc.nodes.values().any(|n| matches!(n.kind, NodeKind::Unknown { .. })));
    assert_eq!(doc.pages.len(), 6);
    let (mut instances, mut resolved) = (0, 0);
    for n in doc.nodes.values().filter(|n| matches!(n.kind, NodeKind::Instance { .. })) {
        instances += 1;
        resolved += instance_component(n).is_some() as usize;
    }
    assert!(instances >= 400 && instances == resolved, "{resolved} of {instances} instances resolved");
    assert_eq!(doc.styles.len(), 35);
    assert!(doc.library.external.is_empty(), "everything is local");
}
