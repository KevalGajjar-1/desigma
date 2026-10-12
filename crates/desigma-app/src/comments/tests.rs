//! App-level comment tests (no window): undo steps, the author setting and the Figma import.

use super::*;
use crate::app::App;
use desigma_doc::{Document, Node, NodeId, Rect, Vec2};

fn app() -> (App, NodeId, NodeId) {
    let mut d = Document::new("t");
    let p1 = d.add_page("One");
    d.add_page("Two");
    let id = d.alloc_id();
    let mut f = Node::frame(id, Rect::new(100.0, 50.0, 200.0, 100.0));
    f.source_id = Some("10:1".into());
    let frame = d.insert(p1, f).unwrap();
    let mut app = App::new(d);
    app.comments.author = Some("Kir".into());
    (app, p1, frame)
}

fn tmp(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("desigma_comments_{}_{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

#[test]
fn every_change_is_one_undo_step() {
    let (mut app, page, frame) = app();
    let anchor = model::make_anchor(&app.doc, page, Vec2::new(150.0, 80.0), Some(frame));
    let id = app.post_comment(anchor, "  Looks off  ").unwrap();
    assert_eq!(app.doc.comments.len(), 1);
    assert_eq!(app.doc.comments[0].messages[0].text, "Looks off");
    assert_eq!(app.doc.comments[0].messages[0].author, "Kir");
    assert!(app.post_comment(anchor, "   ").is_none(), "empty comments are not posted");

    assert!(app.reply_comment(&id, "Agreed"));
    assert_eq!(app.doc.comments[0].messages.len(), 2);
    let reply_id = app.doc.comments[0].messages[1].id.clone();
    assert!(app.edit_comment_text(&id, &reply_id, "Agreed, fixing"));
    assert!(app.resolve_comment_thread(&id, true));
    assert!(app.doc.comments[0].resolved);
    assert!(app.delete_comment_message(&id, &reply_id));
    assert_eq!(app.doc.comments[0].messages.len(), 1);
    assert!(app.delete_comment_thread(&id));
    assert!(app.doc.comments.is_empty());

    // six edits, six undo steps
    app.undo();
    assert_eq!(app.doc.comments.len(), 1, "deleting the thread was one step");
    for _ in 0..5 {
        assert!(app.hist.can_undo());
        app.undo();
    }
    assert!(!app.hist.can_undo());
    assert!(app.doc.comments.is_empty());
    app.redo();
    assert_eq!(app.doc.comments.len(), 1);
}

#[test]
fn each_undo_reverts_exactly_one_edit() {
    let (mut app, page, _) = app();
    let anchor = model::make_anchor(&app.doc, page, Vec2::ZERO, None);
    let id = app.post_comment(anchor, "one").unwrap();
    app.reply_comment(&id, "two");
    app.resolve_comment_thread(&id, true);
    app.undo();
    assert!(!app.doc.comments[0].resolved && app.doc.comments[0].messages.len() == 2);
    app.undo();
    assert_eq!(app.doc.comments[0].messages.len(), 1);
    app.undo();
    assert!(app.doc.comments.is_empty());
}

#[test]
fn goto_switches_page_and_centres_the_pin() {
    let (mut app, _, _) = app();
    let two = app.doc.pages[1];
    let anchor = model::make_anchor(&app.doc, two, Vec2::new(900.0, 700.0), None);
    let id = app.post_comment(anchor, "far away").unwrap();
    assert_eq!(app.page, app.doc.pages[0]);
    app.goto_comment(&id);
    assert_eq!(app.page, two);
    assert_eq!(app.comments.open.as_deref(), Some(id.as_str()));
    let screen = app.view.to_screen(app.canvas_rect.min, Vec2::new(900.0, 700.0));
    assert!((screen.x - app.canvas_rect.center().x).abs() < 40.0 && (screen.y - app.canvas_rect.center().y).abs() < 40.0, "{screen:?}");
}

#[test]
fn the_author_name_is_a_setting_in_the_library_root() {
    let root = tmp("author");
    let mut app = App::new(Document::new("t"));
    app.attach_library(root.clone());
    let default = app.author();
    assert!(!default.is_empty());
    app.set_author("  Ada Lovelace ");
    assert_eq!(app.author(), "Ada Lovelace");
    assert_eq!(settings::load_author(&root).as_deref(), Some("Ada Lovelace"));
    // a new session reads it back
    let mut again = App::new(Document::new("t"));
    again.attach_library(root.clone());
    assert_eq!(again.author(), "Ada Lovelace");
    again.set_author("");
    assert_eq!(again.author(), settings::os_user_name());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn importing_figma_comments_is_one_undo_step_and_idempotent() {
    let (mut app, _, frame) = app();
    let m = app.import_comments_text(import::FIXTURE).unwrap();
    assert_eq!((m.threads, m.replies, m.skipped), (3, 1, 1));
    assert_eq!(app.doc.comments.len(), 3);
    assert_eq!(app.doc.comments[0].anchor.node, Some(frame));
    assert!(app.doc.comments[1].resolved);
    let again = app.import_comments_text(import::FIXTURE).unwrap();
    assert_eq!((again.threads, again.replies), (0, 0));
    assert_eq!(app.doc.comments.len(), 3);
    app.undo();
    assert!(app.doc.comments.is_empty(), "the whole import is one step");
    assert!(app.import_comments_text("{ nope").is_err());
}

#[test]
fn a_comments_json_next_to_a_design_is_merged_when_opened() {
    let (mut app, _, _) = app();
    let dir = tmp("open");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("comments.json");
    std::fs::write(&path, import::FIXTURE).unwrap();
    app.open_path(path);
    assert_eq!(app.doc.comments.len(), 3, "merged into the open file instead of replacing it");
    assert_eq!(app.doc.name, "t");
    let _ = std::fs::remove_dir_all(&dir);
}
