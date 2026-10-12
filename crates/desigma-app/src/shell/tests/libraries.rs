//! UI tests for shared libraries: the Libraries modal, library components in
//! the Assets panel and the update review.

use super::*;
use crate::app::RailItem;
use desigma_doc::{instances_of, library_instance_info, Command, NodeKind, OverrideField};
use std::path::{Path, PathBuf};

fn tmp(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("desigma-ui-libs-{name}-{}", desigma_library::new_id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn sim(root: &Path) -> Sim {
    let mut s = Sim::new();
    s.shell.app.libs.set_root(root.to_path_buf());
    s
}

fn frames(s: &mut Sim, n: usize) {
    for _ in 0..n {
        s.frame(vec![]);
    }
}

/// File A publishes its components; returns after the first publish.
fn publish_from_a(root: &Path) -> Sim {
    let mut a = sim(root);
    a.shell.app.open_libraries();
    frames(&mut a, 3);
    a.shell.app.libs.description = "First version".into();
    let b = a.rect_of("lib-publish");
    a.click(b.center());
    frames(&mut a, 2);
    a
}

fn enable_in_b(root: &Path) -> Sim {
    let mut b = sim(root);
    b.shell.app.new_document();
    b.shell.app.open_libraries();
    frames(&mut b, 3);
    let t = b.rect_of("lib-toggle:Untitled");
    b.click(t.center());
    frames(&mut b, 2);
    b
}

#[test]
fn modal_publishes_this_file_and_lists_the_library() {
    let root = tmp("publish");
    let a = publish_from_a(&root);
    let info = a.app().doc.library.published.clone().expect("published");
    assert_eq!(info.version, 1);
    assert_eq!(info.description, "First version");
    assert!(root.join("libraries").join(&info.library_id).join("1.json").exists());
    let idx = desigma_library::Library::open(&root).unwrap().library_index(&info.library_id).unwrap();
    assert_eq!(idx.versions[0].description, "First version");
    assert!(idx.components >= 1);
    assert!(!idx.versions[0].changelog.is_empty());
    // Nothing changed since: the modal shows no changes and the button is disabled.
    let mut a = a;
    a.shell.app.refresh_libraries();
    frames(&mut a, 2);
    assert!(a.app().libs.changes.is_empty());
    let before = std::fs::read_dir(root.join("libraries").join(&info.library_id)).unwrap().count();
    let b = a.rect_of("lib-publish");
    a.click(b.center());
    assert_eq!(std::fs::read_dir(root.join("libraries").join(&info.library_id)).unwrap().count(), before, "disabled button does not republish");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn changelog_lists_changed_assets_since_last_publish() {
    let root = tmp("changelog");
    let mut a = publish_from_a(&root);
    let main = a.by_name("Primary button");
    a.shell.app.edit_nodes(&[main], |n| n.size.x += 30.0);
    a.shell.app.refresh_libraries();
    frames(&mut a, 2);
    let ch = a.app().libs.changes.clone();
    assert_eq!(ch.len(), 1, "{ch:?}");
    assert_eq!(ch[0].name, "Primary button");
    assert_eq!(ch[0].change, desigma_doc::UpdateKind::Changed);
    a.rect_of("lib-change:Primary button");
    let b = a.rect_of("lib-publish");
    a.click(b.center());
    assert_eq!(a.app().doc.library.published.as_ref().unwrap().version, 2);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn enable_library_drag_component_and_review_updates() {
    let root = tmp("flow");
    let mut a = publish_from_a(&root);
    let mut b = enable_in_b(&root);

    // Enable pins the latest version and the modal toggle reflects it.
    assert_eq!(b.app().doc.library.enabled.len(), 1);
    assert_eq!(b.app().doc.library.enabled[0].version, 1);
    assert_eq!(b.app().doc.library.enabled[0].name, "Untitled");

    // The library's components show up in the Assets panel and can be dragged out.
    b.shell.app.close_libraries();
    b.shell.app.ui.rail = Some(RailItem::Assets);
    frames(&mut b, 6);
    let row = b.rect_of("asset:Primary button");
    let canvas_mid = b.app().canvas_rect.center();
    b.drag(row.center(), canvas_mid);
    let inst = b.app().selection.first().copied().expect("instance selected");
    assert!(matches!(b.app().doc.get(inst).unwrap().kind, NodeKind::Instance { .. }));
    let info = library_instance_info(&b.app().doc, inst).expect("library instance");
    assert_eq!((info.library_name.as_str(), info.asset_name.as_str()), ("Untitled", "Primary button"));
    let lp = b.app().doc.library_page.expect("hidden library page");
    assert!(!b.app().doc.pages.contains(&lp));

    // Library copies cannot be selected: sanitize drops them and they are not on a page.
    let comp = desigma_doc::instance_component(b.app().doc.get(inst).unwrap()).unwrap();
    b.shell.app.selection = vec![comp];
    b.shell.app.sanitize();
    assert!(b.app().selection.is_empty());

    // Inspector row names the library.
    b.shell.app.selection = vec![inst];
    frames(&mut b, 3);
    b.rect_of("instance-library");

    // Override a label in the instance.
    let label =
        b.app().doc.get(inst).unwrap().children.iter().copied().find(|c| matches!(b.app().doc.get(*c).unwrap().kind, NodeKind::Text(_))).expect("a text layer");
    let mut n = b.app().doc.get(label).unwrap().clone();
    if let NodeKind::Text(t) = &mut n.kind {
        t.content = "Mine".into();
    }
    n.component.overrides.insert(OverrideField::Characters);
    let mut hist_doc = std::mem::take(&mut b.shell.app.doc);
    Command::Update { node: Box::new(n) }.apply(&mut hist_doc).unwrap();
    b.shell.app.doc = hist_doc;
    let w0 = b.app().doc.get(inst).unwrap().size.x;

    // A changes the component and republishes.
    let main = a.by_name("Primary button");
    a.shell.app.edit_nodes(&[main], |n| n.size.x += 40.0);
    a.shell.app.refresh_libraries();
    frames(&mut a, 2);
    let pb = a.rect_of("lib-publish");
    a.click(pb.center());
    assert_eq!(a.app().doc.library.published.as_ref().unwrap().version, 2);

    // B notices the new version and offers a review.
    frames(&mut b, 60);
    assert!(b.app().update_banner_visible());
    b.rect_of("library-updates-banner");
    let rv = b.rect_of("library-updates-review");
    b.click(rv.center());
    frames(&mut b, 2);
    assert!(b.app().libs.review);
    b.rect_of("lib-upd:Primary button");

    // "Update selected" is disabled until something is checked.
    let sel_btn = b.rect_of("library-update-selected");
    b.click(sel_btn.center());
    assert!(b.app().libs.review, "nothing selected: stays open");
    let check = b.rect_of("lib-check:Primary button");
    b.click(check.center());
    assert_eq!(b.app().libs.selected.len(), 1);
    let sel_btn = b.rect_of("library-update-selected");
    b.click(sel_btn.center());
    frames(&mut b, 2);
    assert!(!b.app().libs.review);
    assert_eq!(b.app().doc.get(inst).unwrap().size.x, w0 + 40.0, "instance follows the updated library component");
    let NodeKind::Text(t) = &b.app().doc.get(label).unwrap().kind else { panic!() };
    assert_eq!(t.content, "Mine", "instance override survives the update");
    assert_eq!(b.app().doc.library.enabled[0].version, 2);

    // One undo step reverts the whole update.
    b.cmd_key(Key::Z);
    assert_eq!(b.app().doc.get(inst).unwrap().size.x, w0);
    assert_eq!(b.app().doc.library.enabled[0].version, 1);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn update_all_and_dismissing_the_banner() {
    let root = tmp("all");
    let mut a = publish_from_a(&root);
    let mut b = enable_in_b(&root);
    let key = {
        let snap = b.shell.app.latest_snapshot(&b.app().doc.library.enabled[0].id.clone()).unwrap();
        snap.assets.iter().find(|x| x.name == "Primary button").unwrap().key.clone()
    };
    let lib_id = b.app().doc.library.enabled[0].id.clone();
    b.shell.app.drop_library_asset(&lib_id, &key, desigma_doc::Vec2::new(40.0, 40.0)).expect("dropped");
    let main = a.by_name("Primary button");
    a.shell.app.edit_nodes(&[main], |n| n.size.y += 8.0);
    a.shell.app.refresh_libraries();
    frames(&mut a, 2);
    let pb = a.rect_of("lib-publish");
    a.click(pb.center());

    // The banner can be dismissed for this version...
    b.shell.app.close_libraries();
    frames(&mut b, 60);
    assert!(b.app().update_banner_visible());
    let x = b.rect_of("library-updates-dismiss");
    b.click(x.center());
    frames(&mut b, 2);
    assert!(!b.app().update_banner_visible());
    // ...but the Libraries modal still offers the review.
    b.shell.app.open_libraries();
    frames(&mut b, 3);
    let upd = b.rect_of("lib-update:Untitled");
    b.click(upd.center());
    frames(&mut b, 2);
    assert!(b.app().libs.review);
    let all = b.rect_of("library-update-all");
    b.click(all.center());
    frames(&mut b, 2);
    assert!(!b.app().libs.review);
    assert_eq!(b.app().doc.library.enabled[0].version, 2);
    let comp = b.app().doc.library.external.values().find_map(|e| e.node).unwrap();
    assert!(!instances_of(&b.app().doc, desigma_doc::components_of(&b.app().doc, comp)[0]).is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn removed_library_assets_are_flagged_in_the_review() {
    let root = tmp("removed");
    let mut a = publish_from_a(&root);
    let mut b = enable_in_b(&root);
    let lib_id = b.app().doc.library.enabled[0].id.clone();
    let key = {
        let snap = b.shell.app.latest_snapshot(&lib_id).unwrap();
        snap.assets.iter().find(|x| x.name == "Primary button").unwrap().key.clone()
    };
    b.shell.app.drop_library_asset(&lib_id, &key, desigma_doc::Vec2::new(40.0, 40.0)).expect("dropped");

    // A deletes the component and republishes.
    let main = a.by_name("Primary button");
    a.shell.app.selection = vec![main];
    a.shell.app.doc = {
        let mut d = std::mem::take(&mut a.shell.app.doc);
        Command::Remove { id: main }.apply(&mut d).unwrap();
        d
    };
    a.shell.app.refresh_libraries();
    frames(&mut a, 2);
    let pb = a.rect_of("lib-publish");
    a.click(pb.center());

    b.shell.app.close_libraries();
    frames(&mut b, 3);
    b.shell.app.libs.invalidate();
    b.shell.app.open_updates_review();
    frames(&mut b, 3);
    let all = b.app().libs.pending_all();
    let u = all[0].updates.iter().find(|u| u.name == "Primary button").expect("listed");
    assert_eq!(u.change, desigma_doc::UpdateKind::Removed);
    b.rect_of("lib-upd:Primary button");
    // The local copy stays and instances keep working.
    assert!(b.app().doc.library.external.contains_key(&desigma_doc::local_key(&lib_id, &key)));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn go_to_main_of_unavailable_library_shows_a_read_only_preview() {
    let root = tmp("preview");
    let _a = publish_from_a(&root);
    let mut b = enable_in_b(&root);
    let lib_id = b.app().doc.library.enabled[0].id.clone();
    let key = {
        let snap = b.shell.app.latest_snapshot(&lib_id).unwrap();
        snap.assets.iter().find(|x| x.name == "Primary button").unwrap().key.clone()
    };
    let inst = b.shell.app.drop_library_asset(&lib_id, &key, desigma_doc::Vec2::new(40.0, 40.0)).unwrap();
    // The source file was never saved to disk: no file to open.
    b.shell.app.go_to_main(inst);
    frames(&mut b, 3);
    assert!(b.app().libs.preview.is_some());
    b.rect_of("library-preview-done");
    let done = b.rect_of("library-preview-done");
    b.click(done.center());
    frames(&mut b, 2);
    assert!(b.app().libs.preview.is_none());
    let _ = std::fs::remove_dir_all(root);
}
