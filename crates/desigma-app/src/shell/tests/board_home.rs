//! UI tests of boards on the home screen: the separate "New board" button, the kind read from the
//! document, the board marker on its card and the board thumbnail.

use super::*;
use crate::canvas::tools::Tool;

struct TmpLib(std::path::PathBuf);

impl TmpLib {
    fn new() -> Self {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!("desigma_board_ui_{}_{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        Self(p)
    }
}

impl Drop for TmpLib {
    fn drop(&mut self) {
        // the thumbnail thread may still be writing; ignore failures
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn home_creates_boards_separately_from_design_files_and_marks_them() {
    let dir = TmpLib::new();
    let mut s = Sim::with(Options { home: true, library: Some(dir.0.clone()), ..Default::default() });
    // two distinct buttons
    let design = s.rect_of("home-new-file");
    let board = s.rect_of("home-new-board");
    assert_ne!(design, board);
    s.click(board.center());
    assert!(s.app().home.is_none(), "the new board opens");
    assert!(s.app().is_board());
    assert!(s.app().canvas_rect.right() > 1500.0, "board layout: no right panel");
    let lib = s.app().lib.clone().unwrap();
    let id = s.app().lib_file.as_ref().unwrap().id.clone();
    // the kind is read from the document on disk; the manifest knows nothing about it
    assert_eq!(lib.file_kind(&id).unwrap(), desigma_doc::DocumentKind::Board);
    assert_eq!(lib.load_file(&id).unwrap().kind, desigma_doc::DocumentKind::Board);
    // put a note on it, save, and leave
    s.key(Key::S);
    let at = s.screen(300.0, 300.0);
    s.click(at);
    s.type_text("hello");
    s.key(Key::Escape);
    s.shell.app.save_library_file().unwrap();
    s.shell.app.go_home();
    s.frame(vec![]);
    s.frame(vec![]);
    // the card of the board carries the board marker
    let _ = s.rect_of("home-kind-board");
    // a design file from the other button
    let design = s.rect_of("home-new-file");
    s.click(design.center());
    assert!(!s.app().is_board());
    let did = s.app().lib_file.as_ref().unwrap().id.clone();
    assert_eq!(lib.file_kind(&did).unwrap(), desigma_doc::DocumentKind::Design);
    assert_ne!(did, id);
}

#[test]
fn boards_get_a_thumbnail_of_the_whole_page_on_the_dotted_canvas() {
    let doc = crate::board::demo::document();
    let png = crate::home::thumb::thumbnail_png(&doc).expect("a board with content has a thumbnail");
    let img = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!((img.width(), img.height()), (480, 300));
    // the canvas colour in a corner, and sticky yellow somewhere inside
    assert_eq!(img.get_pixel(2, 2).0[..3], [0xF4, 0xF4, 0xF4]);
    let yellow = img.pixels().filter(|p| p.0[0] > 240 && p.0[1] > 215 && p.0[2] < 170 && p.0[2] > 120).count();
    assert!(yellow > 300, "{yellow} sticky pixels");
    // an empty board has none
    assert!(crate::home::thumb::thumbnail_png(&crate::board::create::new_document("e")).is_none());
}

#[test]
fn file_menu_offers_new_board() {
    let mut s = Sim::new();
    let ctx = s.ctx.clone();
    crate::panels::menus::run(&mut s.shell.app, &ctx, crate::panels::menus::Action::NewBoard);
    assert!(s.app().is_board(), "File > New board opens an empty board");
    assert_eq!(s.app().tool, Tool::Move);
    s.frame(vec![]);
    // and a design tool picked before does not leak into the board
    let mut d = Sim::new();
    d.key(Key::R);
    assert_eq!(d.app().tool, Tool::Rectangle);
    let ctx = d.ctx.clone();
    crate::panels::menus::run(&mut d.shell.app, &ctx, crate::panels::menus::Action::NewBoard);
    assert_eq!(d.app().tool, Tool::Move);
}
