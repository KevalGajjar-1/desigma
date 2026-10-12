//! The text editing session: caret and selection over UTF-8 byte offsets,
//! keyboard / clipboard / IME handling, per-range styling and in-session undo.
//!
//! While a session is open the node is mutated live through the session's
//! transaction; closing it commits everything as one document undo step.
//! Inside the session Ctrl+Z / Ctrl+Shift+Z step through finer edits (typing
//! coalesced per word or burst, see [`crate::textedit::UndoLog`]).

use super::tools::Tool;
use super::*;
use crate::layout;
use crate::richtext::{self, ListKind};
use crate::textedit::{self as te, EditKind, Snap, UndoLog};
use crate::textmeasure as tm;
use crate::util::*;
use desigma_doc::{NodeKind, TextDecoration, TextProps, TextStyle};
use std::ops::Range;

pub struct TextSession {
    pub id: NodeId,
    /// Caret as a byte offset into the content (always on a char boundary).
    pub caret: usize,
    /// The other end of the selection; equal to `caret` when collapsed.
    pub anchor: usize,
    pub tx: Tx,
    pub created: bool,
    /// Column (node-local x) kept while moving up/down.
    pub goal_x: Option<f32>,
    /// Style for the next typed text when the caret is collapsed (set by Ctrl+B etc.).
    pub typing: Option<TextStyle>,
    pub log: UndoLog,
    /// Active IME composition (byte range inside the content).
    pub preedit: Option<Range<usize>>,
    /// The pointer is dragging a selection.
    pub dragging: bool,
}

impl TextSession {
    pub fn new(id: NodeId, caret: usize, anchor: usize, tx: Tx, created: bool) -> Self {
        Self { id, caret, anchor, tx, created, goal_x: None, typing: None, log: UndoLog::default(), preedit: None, dragging: false }
    }

    /// Selected byte range (empty when only a caret).
    pub fn range(&self) -> Range<usize> {
        self.caret.min(self.anchor)..self.caret.max(self.anchor)
    }
}

/// Commands that edit the style of the selected range or text layers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextCmd {
    Bold,
    Italic,
    Underline,
    SizeUp,
    SizeDown,
    List(ListKind),
}

// ------------------------------------------------------------------ lifecycle
pub fn begin_text_edit(app: &mut App, id: NodeId) {
    finish_text(app, true);
    let len = match app.doc.get(id).map(|n| &n.kind) {
        Some(NodeKind::Text(t)) => t.content.len(),
        _ => return,
    };
    app.selection = vec![id];
    // Like Figma, entering edit mode selects all the text.
    app.canvas.text = Some(TextSession::new(id, len, 0, Tx::new(), false));
}

pub fn finish_text(app: &mut App, commit: bool) {
    let Some(mut sess) = app.canvas.text.take() else { return };
    if let Some(range) = sess.preedit.take() {
        // an unfinished composition is discarded
        if let Some(n) = sess.tx.ed.node(&mut app.doc, sess.id) {
            if let NodeKind::Text(t) = &mut n.kind {
                richtext::splice(t, range, "", None);
            }
        }
    }
    let empty = matches!(app.doc.get(sess.id).map(|n| &n.kind), Some(NodeKind::Text(t)) if t.content.is_empty());
    if (sess.created && empty) || !commit {
        sess.tx.cancel(&mut app.doc);
        app.sanitize();
    } else {
        let id = sess.id;
        app.run_tx(sess.tx);
        app.selection = vec![id];
    }
    app.changed();
}

// ------------------------------------------------------------------ queries
fn props_of(app: &App) -> Option<(TextProps, DV)> {
    let sess = app.canvas.text.as_ref()?;
    let n = app.doc.get(sess.id)?;
    match &n.kind {
        NodeKind::Text(t) => Some((t.clone(), n.size)),
        _ => None,
    }
}

/// Node-local point of a page-space point for the edited node.
fn to_local(app: &App, id: NodeId, page: DV) -> Option<DV> {
    Some(app.doc.world_transform(id).invert()?.apply(page))
}

/// Whether `page` is on (or within 2px of) the edited node.
pub fn node_contains(app: &App, id: NodeId, page: DV) -> bool {
    let Some(n) = app.doc.get(id) else { return false };
    let Some(l) = to_local(app, id, page) else { return false };
    l.x >= -2.0 && l.y >= -2.0 && l.x <= n.size.x + 2.0 && l.y <= n.size.y + 2.0
}

/// Byte offset under the page-space point in the edited node.
fn hit_at(app: &App, page: DV) -> Option<usize> {
    let id = app.canvas.text.as_ref()?.id;
    let l = to_local(app, id, page)?;
    let (t, size) = props_of(app)?;
    Some(tm::hit_caret(&t, size, l.x, l.y))
}

// ------------------------------------------------------------------ selection
fn set_sel(app: &mut App, caret: usize, anchor: usize, keep_goal: bool) {
    let Some(s) = app.canvas.text.as_mut() else { return };
    s.caret = caret;
    s.anchor = anchor;
    if !keep_goal {
        s.goal_x = None;
    }
    s.typing = None;
    s.log.break_burst();
    app.canvas.dirty = true;
}

/// Press at `page`: places the caret (Shift extends) and starts a drag selection.
pub fn press_at(app: &mut App, page: DV, shift: bool) {
    let Some(at) = hit_at(app, page) else { return };
    let anchor = if shift { app.canvas.text.as_ref().map(|s| s.anchor).unwrap_or(at) } else { at };
    set_sel(app, at, anchor, false);
    if let Some(s) = app.canvas.text.as_mut() {
        s.dragging = true;
    }
}

/// Pointer moved while dragging a selection.
pub fn drag_to(app: &mut App, page: DV) {
    let Some(at) = hit_at(app, page) else { return };
    let anchor = app.canvas.text.as_ref().map(|s| s.anchor).unwrap_or(at);
    set_sel(app, at, anchor, false);
}

/// Double-click: selects the word under the pointer.
pub fn select_word(app: &mut App, page: DV) {
    let Some(at) = hit_at(app, page) else { return };
    let Some((t, _)) = props_of(app) else { return };
    let r = te::word_at(&t.content, at);
    set_sel(app, r.end, r.start, false);
}

/// Triple-click: selects the paragraph under the pointer.
pub fn select_paragraph(app: &mut App, page: DV) {
    let Some(at) = hit_at(app, page) else { return };
    let Some((t, _)) = props_of(app) else { return };
    let r = te::paragraph_at(&t.content, at);
    set_sel(app, r.end, r.start, false);
}

fn select_all(app: &mut App) {
    let Some((t, _)) = props_of(app) else { return };
    set_sel(app, t.content.len(), 0, false);
}

/// Movement keys; `shift` extends the selection, `ctrl` moves by word / to text edges.
fn move_caret(app: &mut App, key: egui::Key, shift: bool, ctrl: bool) {
    let Some((t, size)) = props_of(app) else { return };
    let Some(sess) = app.canvas.text.as_ref() else { return };
    let (caret, anchor, goal) = (sess.caret, sess.anchor, sess.goal_x);
    let range = sess.range();
    let c = &t.content;
    let mut keep_goal = false;
    let target = match key {
        egui::Key::ArrowLeft if !shift && !range.is_empty() => range.start,
        egui::Key::ArrowRight if !shift && !range.is_empty() => range.end,
        egui::Key::ArrowLeft => {
            if ctrl {
                te::prev_word(c, caret)
            } else {
                te::prev_grapheme(c, caret)
            }
        }
        egui::Key::ArrowRight => {
            if ctrl {
                te::next_word(c, caret)
            } else {
                te::next_grapheme(c, caret)
            }
        }
        egui::Key::ArrowUp | egui::Key::ArrowDown => {
            let gx = goal.unwrap_or_else(|| tm::caret_rect(&t, size, caret).0);
            keep_goal = true;
            if let Some(s) = app.canvas.text.as_mut() {
                s.goal_x = Some(gx);
            }
            tm::move_vertical(&t, size, caret, if key == egui::Key::ArrowUp { -1 } else { 1 }, gx)
        }
        egui::Key::Home if ctrl => 0,
        egui::Key::End if ctrl => c.len(),
        egui::Key::Home => tm::line_bounds(&t, size, caret).0,
        egui::Key::End => tm::line_bounds(&t, size, caret).1,
        _ => return,
    };
    set_sel(app, target, if shift { anchor } else { target }, keep_goal);
}

// ------------------------------------------------------------------ editing
/// Runs `f` on the edited text. `log` records an undo step (None = no step, for IME previews).
fn edit_props(app: &mut App, log: Option<(EditKind, bool)>, f: impl FnOnce(&mut TextProps, &mut usize, &mut usize)) {
    let now = app.now;
    let App { doc, canvas, .. } = app;
    let Some(sess) = canvas.text.as_mut() else { return };
    let id = sess.id;
    let (mut caret, mut anchor) = (sess.caret, sess.anchor);
    let mut before = None;
    if let Some(n) = sess.tx.ed.node(doc, id) {
        if let NodeKind::Text(t) = &mut n.kind {
            let snap = Snap { props: t.clone(), caret, anchor };
            f(t, &mut caret, &mut anchor);
            if *t != snap.props {
                before = Some(snap);
            }
            n.size = layout::fit_text(t, n.size);
        }
    }
    if let (Some(b), Some((kind, new_word))) = (before, log) {
        sess.log.record(b, kind, now, new_word);
    }
    sess.caret = caret;
    sess.anchor = anchor;
    layout::relayout_up(doc, &mut sess.tx.ed, id);
    canvas.dirty = true;
}

/// Replaces the selection with `s` (typing / paste).
pub fn insert_text(app: &mut App, s: &str, kind: EditKind) {
    let s = te::clean_input(s);
    if s.is_empty() {
        return;
    }
    let typing = app.canvas.text.as_mut().and_then(|x| x.typing.take());
    let Some(sess) = app.canvas.text.as_ref() else { return };
    let new_word = props_of(app).is_some_and(|(t, _)| te::starts_new_word(&t.content, sess.range().start, &s)) || s.contains('\n');
    edit_props(app, Some((kind, new_word)), |t, caret, anchor| {
        let r = (*caret).min(*anchor)..(*caret).max(*anchor);
        let end = richtext::splice(t, r, &s, typing.clone());
        *caret = end;
        *anchor = end;
    });
    if let Some(sess) = app.canvas.text.as_mut() {
        sess.goal_x = None;
    }
}

fn delete(app: &mut App, forward: bool, ctrl: bool) {
    // Backspace at the start of a list item turns it into a plain paragraph (outdents nested ones).
    if !forward {
        if let Some(caret) = app.canvas.text.as_ref().filter(|s| s.range().is_empty()).map(|s| s.caret) {
            if props_of(app).is_some_and(|(t, _)| t.has_lists()) {
                let mut done = false;
                edit_props(app, Some((EditKind::Deleting, true)), |t, _, _| done = richtext::backspace_at_item_start(t, caret));
                if done {
                    return;
                }
            }
        }
    }
    edit_props(app, Some((EditKind::Deleting, false)), |t, caret, anchor| {
        let mut r = (*caret).min(*anchor)..(*caret).max(*anchor);
        if r.is_empty() {
            r = if forward {
                *caret..if ctrl { te::next_word(&t.content, *caret) } else { te::next_grapheme(&t.content, *caret) }
            } else {
                (if ctrl { te::prev_word(&t.content, *caret) } else { te::prev_grapheme(&t.content, *caret) })..*caret
            };
        }
        if r.is_empty() {
            return;
        }
        richtext::splice(t, r.clone(), "", None);
        *caret = r.start;
        *anchor = r.start;
    });
    if let Some(sess) = app.canvas.text.as_mut() {
        sess.goal_x = None;
        sess.typing = None;
    }
}

fn enter(app: &mut App) {
    // Enter on an empty list item ends the list (or outdents a nested item); elsewhere the
    // newline keeps the paragraph's list membership, so Enter inside an item continues the list.
    let caret = app.canvas.text.as_ref().filter(|s| s.range().is_empty()).map(|s| s.caret);
    if let Some(caret) = caret {
        let mut done = false;
        edit_props(app, Some((EditKind::Other, true)), |t, _, _| done = richtext::exit_empty_item(t, caret));
        if done {
            return;
        }
    }
    insert_text(app, "\n", EditKind::Other);
}

/// Tab / Shift+Tab: nest or un-nest the list items touched by the selection.
fn change_list_level(app: &mut App, delta: i8) {
    let Some(range) = app.canvas.text.as_ref().map(|s| s.range()) else { return };
    edit_props(app, Some((EditKind::Other, true)), |t, _, _| {
        richtext::change_level(t, range.clone(), delta);
    });
}

/// In-session undo (`redo = false`) or redo.
fn undo_step(app: &mut App, redo: bool) {
    let now = (|| {
        let (t, _) = props_of(app)?;
        let s = app.canvas.text.as_ref()?;
        Some(Snap { props: t, caret: s.caret, anchor: s.anchor })
    })();
    let Some(cur) = now else { return };
    let Some(sess) = app.canvas.text.as_mut() else { return };
    let snap = if redo { sess.log.redo(cur) } else { sess.log.undo(cur) };
    let Some(snap) = snap else { return };
    let App { doc, canvas, .. } = app;
    let sess = canvas.text.as_mut().unwrap();
    let id = sess.id;
    if let Some(n) = sess.tx.ed.node(doc, id) {
        if let NodeKind::Text(t) = &mut n.kind {
            *t = snap.props;
            n.size = layout::fit_text(t, n.size);
        }
    }
    sess.caret = snap.caret;
    sess.anchor = snap.anchor;
    sess.typing = None;
    layout::relayout_up(doc, &mut sess.tx.ed, id);
    canvas.dirty = true;
}

// ------------------------------------------------------------------ styling
/// Applies `f` to the selected range of the open session (or to the typing style
/// of a collapsed caret; or to the whole text when it is empty). Returns false
/// when no session is open.
pub fn style_selection(app: &mut App, f: impl Fn(&mut TextStyle)) -> bool {
    style_selection_as(app, f, EditKind::Other)
}

/// [`style_selection`] with an explicit undo kind (inspector edits coalesce).
pub fn style_selection_as(app: &mut App, f: impl Fn(&mut TextStyle), kind: EditKind) -> bool {
    let Some(sess) = app.canvas.text.as_ref() else { return false };
    let range = sess.range();
    let empty_text = props_of(app).is_none_or(|(t, _)| t.content.is_empty());
    if range.is_empty() && !empty_text {
        let Some((t, _)) = props_of(app) else { return true };
        let caret = app.canvas.text.as_ref().map(|s| s.caret).unwrap_or(0);
        let mut st = app.canvas.text.as_ref().and_then(|s| s.typing.clone()).unwrap_or_else(|| richtext::style_for_typing(&t, caret));
        f(&mut st);
        if let Some(s) = app.canvas.text.as_mut() {
            s.typing = Some(st);
        }
        return true;
    }
    edit_props(app, Some((kind, kind == EditKind::Other)), |t, _, _| {
        if empty_text {
            richtext::apply_all(t, &f);
        } else {
            richtext::apply_style(t, range.clone(), &f);
        }
    });
    true
}

/// Edits the base style of the edited text (paragraph level fields such as alignment).
pub fn base_style(app: &mut App, f: impl Fn(&mut TextStyle)) {
    edit_props(app, Some((EditKind::Other, true)), |t, _, _| f(&mut t.style));
}

/// Style summary of the open session's selection (for the inspector).
pub fn session_summary(app: &App) -> Option<richtext::Summary> {
    let (t, _) = props_of(app)?;
    let s = app.canvas.text.as_ref()?;
    let mut sum = richtext::summary(&t, s.range());
    if let (true, Some(ty)) = (s.range().is_empty(), &s.typing) {
        sum = richtext::summary(&TextProps { content: "x".into(), style: ty.clone(), runs: vec![], ..t }, 0..0);
    }
    Some(sum)
}

/// Runs a styling command on the open session, or on the selected text layers.
pub fn text_command(app: &mut App, cmd: TextCmd) {
    if app.canvas.text.is_some() {
        let sum = session_summary(app);
        match cmd {
            TextCmd::List(kind) => {
                let Some(range) = app.canvas.text.as_ref().map(|s| s.range()) else { return };
                edit_props(app, Some((EditKind::Other, true)), |t, _, _| richtext::toggle_list(t, range.clone(), kind));
            }
            c => {
                let f = style_fn(c, sum.as_ref());
                style_selection(app, f);
            }
        }
        return;
    }
    let ids = app.selection.clone();
    for id in ids {
        let Some(NodeKind::Text(t)) = app.doc.get(id).map(|n| n.kind.clone()) else { continue };
        let sum = richtext::summary(&t, 0..t.content.len());
        match cmd {
            TextCmd::List(kind) => {
                app.edit_nodes(&[id], |n| {
                    if let NodeKind::Text(t) = &mut n.kind {
                        let len = t.content.len();
                        richtext::toggle_list(t, 0..len, kind);
                        n.size = layout::fit_text(t, n.size);
                    }
                });
            }
            c => {
                let f = style_fn(c, Some(&sum));
                app.edit_nodes(&[id], |n| {
                    if let NodeKind::Text(t) = &mut n.kind {
                        richtext::apply_all(t, &f);
                        n.size = layout::fit_text(t, n.size);
                    }
                });
            }
        }
    }
}

fn style_fn(cmd: TextCmd, sum: Option<&richtext::Summary>) -> impl Fn(&mut TextStyle) {
    let bold = sum.and_then(|s| s.weight).is_some_and(|w| w >= 600);
    let italic = sum.and_then(|s| s.italic) == Some(true);
    let underline = sum.and_then(|s| s.decoration) == Some(TextDecoration::Underline);
    move |st: &mut TextStyle| match cmd {
        TextCmd::Bold => st.font_weight = if bold { 400 } else { 700 },
        TextCmd::Italic => st.italic = !italic,
        TextCmd::Underline => st.decoration = if underline { TextDecoration::None } else { TextDecoration::Underline },
        TextCmd::SizeUp => st.font_size = (st.font_size + 1.0).min(1000.0),
        TextCmd::SizeDown => st.font_size = (st.font_size - 1.0).max(1.0),
        TextCmd::List(_) => {}
    }
}

// ------------------------------------------------------------------ clipboard
fn selected_text(app: &App) -> Option<String> {
    let (t, _) = props_of(app)?;
    let r = app.canvas.text.as_ref()?.range();
    (!r.is_empty()).then(|| t.content[r].to_string())
}

fn cut(app: &mut App) {
    delete(app, false, false);
}

// ------------------------------------------------------------------ IME
fn preedit(app: &mut App, text: &str) {
    let old = app.canvas.text.as_mut().and_then(|s| s.preedit.take());
    let sel_start = app.canvas.text.as_ref().map(|s| s.range().start).unwrap_or(0);
    let mut new_range = None;
    edit_props(app, None, |t, caret, anchor| {
        let mut at = (*caret).min(*anchor);
        if let Some(r) = old.clone() {
            richtext::splice(t, r.clone(), "", None);
            at = r.start;
        } else {
            let r = (*caret).min(*anchor)..(*caret).max(*anchor);
            if !r.is_empty() {
                richtext::splice(t, r.clone(), "", None);
                at = r.start;
            }
        }
        let _ = sel_start;
        if !text.is_empty() {
            let mut st = richtext::style_for_typing(t, at);
            st.decoration = TextDecoration::Underline;
            let end = richtext::splice(t, at..at, text, Some(st));
            new_range = Some(at..end);
            *caret = end;
        } else {
            *caret = at;
        }
        *anchor = *caret;
    });
    if let Some(s) = app.canvas.text.as_mut() {
        s.preedit = new_range;
    }
}

fn commit_ime(app: &mut App, text: &str) {
    // Drop the composition text, then insert the final string as regular typing.
    preedit(app, "");
    insert_text(app, text, EditKind::Typing);
}

// ------------------------------------------------------------------ events
pub fn text_keys(app: &mut App, ctx: &Context) {
    // A focused inspector field owns the keyboard.
    if ctx.egui_wants_keyboard_input() {
        return;
    }
    let events = ctx.input(|i| i.events.clone());
    for ev in events {
        if app.canvas.text.is_none() {
            return;
        }
        match ev {
            egui::Event::Text(s) => insert_text(app, &s, EditKind::Typing),
            egui::Event::Paste(s) => insert_text(app, &s, EditKind::Other),
            egui::Event::Copy => {
                if let Some(s) = selected_text(app) {
                    ctx.copy_text(s);
                }
            }
            egui::Event::Cut => {
                if let Some(s) = selected_text(app) {
                    ctx.copy_text(s);
                    cut(app);
                }
            }
            egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) => preedit(app, &text),
            egui::Event::Ime(egui::ImeEvent::Commit(text)) => commit_ime(app, &text),
            egui::Event::Key { key, pressed: true, modifiers, .. } => {
                if !key_event(app, key, modifiers) {
                    return;
                }
            }
            _ => {}
        }
    }
}

/// Handles one key press; returns false when the session ended.
fn key_event(app: &mut App, key: egui::Key, m: egui::Modifiers) -> bool {
    use egui::Key as K;
    let ctrl = m.command;
    // an unfinished IME composition owns navigation / editing keys
    if app.canvas.text.as_ref().is_some_and(|s| s.preedit.is_some()) && !matches!(key, K::Escape) {
        return true;
    }
    match key {
        K::Escape => {
            finish_text(app, true);
            app.tool = Tool::Move;
            return false;
        }
        K::Backspace => delete(app, false, ctrl),
        K::Delete => delete(app, true, ctrl),
        K::Enter => enter(app),
        K::ArrowLeft | K::ArrowRight | K::ArrowUp | K::ArrowDown | K::Home | K::End => move_caret(app, key, m.shift, ctrl),
        K::A if ctrl => select_all(app),
        K::B if ctrl => text_command(app, TextCmd::Bold),
        K::I if ctrl => text_command(app, TextCmd::Italic),
        K::U if ctrl => text_command(app, TextCmd::Underline),
        K::Period if ctrl && m.shift => text_command(app, TextCmd::SizeUp),
        K::Comma if ctrl && m.shift => text_command(app, TextCmd::SizeDown),
        K::Tab if !ctrl => change_list_level(app, if m.shift { -1 } else { 1 }),
        K::Num7 if ctrl && m.shift => text_command(app, TextCmd::List(ListKind::Numbered)),
        K::Num8 if ctrl && m.shift => text_command(app, TextCmd::List(ListKind::Bullet)),
        K::Z if ctrl && !m.shift => undo_step(app, false),
        K::Z if ctrl && m.shift => undo_step(app, true),
        K::Y if ctrl => undo_step(app, true),
        _ => {}
    }
    true
}

/// Screen rect of the caret (for the IME candidate window), if a session is open.
pub fn caret_screen_rect(app: &App, origin: egui::Pos2) -> Option<ERect> {
    let sess = app.canvas.text.as_ref()?;
    let (t, size) = props_of(app)?;
    let (x, y, h) = tm::caret_rect(&t, size, sess.caret);
    let w = app.doc.world_transform(sess.id);
    let a = app.view.to_screen(origin, w.apply(v2(x, y)));
    let b = app.view.to_screen(origin, w.apply(v2(x, y + h)));
    Some(ERect::from_two_pos(a, b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use desigma_doc::{Document, Node};

    fn app_with_text(s: &str) -> (App, NodeId) {
        let mut d = Document::new("t");
        let page = d.add_page("p");
        let id = d.alloc_id();
        let mut n = Node::text(id, Rect::new(0.0, 0.0, 10.0, 10.0), s, TextStyle { font_size: 16.0, ..Default::default() });
        if let NodeKind::Text(t) = &n.kind {
            n.size = layout::estimate_text_size(t);
        }
        d.insert(page, n).unwrap();
        let mut app = App::new(d);
        begin_text_edit(&mut app, id);
        (app, id)
    }

    fn text_of(app: &App, id: NodeId) -> TextProps {
        match &app.doc.get(id).unwrap().kind {
            NodeKind::Text(t) => t.clone(),
            _ => panic!(),
        }
    }

    fn key(app: &mut App, k: egui::Key, shift: bool, ctrl: bool) {
        key_event(app, k, egui::Modifiers { shift, command: ctrl, ctrl, ..Default::default() });
    }

    #[test]
    fn entering_selects_all_and_typing_replaces() {
        let (mut app, id) = app_with_text("hello");
        assert_eq!(app.canvas.text.as_ref().unwrap().range(), 0..5);
        insert_text(&mut app, "bye", EditKind::Typing);
        assert_eq!(text_of(&app, id).content, "bye");
        assert_eq!(app.canvas.text.as_ref().unwrap().caret, 3);
    }

    #[test]
    fn arrows_shift_select_and_collapse() {
        let (mut app, _) = app_with_text("héllo wörld");
        key(&mut app, egui::Key::Home, false, false);
        for _ in 0..3 {
            key(&mut app, egui::Key::ArrowRight, true, false);
        }
        assert_eq!(app.canvas.text.as_ref().unwrap().range(), 0.."hél".len());
        // plain right collapses to the end of the selection
        key(&mut app, egui::Key::ArrowRight, false, false);
        let s = app.canvas.text.as_ref().unwrap();
        assert_eq!((s.caret, s.anchor), ("hél".len(), "hél".len()));
        // ctrl+right jumps a word, ctrl+shift extends
        key(&mut app, egui::Key::ArrowRight, true, true);
        assert_eq!(app.canvas.text.as_ref().unwrap().range(), "hél".len().."héllo".len());
        key(&mut app, egui::Key::End, false, true);
        assert_eq!(app.canvas.text.as_ref().unwrap().caret, "héllo wörld".len());
        key(&mut app, egui::Key::ArrowLeft, false, true);
        assert_eq!(app.canvas.text.as_ref().unwrap().caret, "héllo ".len());
    }

    #[test]
    fn backspace_deletes_graphemes_and_words() {
        let (mut app, id) = app_with_text("a👨‍👩‍👧 two words");
        key(&mut app, egui::Key::Home, false, true);
        key(&mut app, egui::Key::ArrowRight, false, false);
        key(&mut app, egui::Key::ArrowRight, false, false);
        key(&mut app, egui::Key::Backspace, false, false);
        assert_eq!(text_of(&app, id).content, "a two words", "the whole ZWJ emoji goes in one press");
        key(&mut app, egui::Key::End, false, true);
        key(&mut app, egui::Key::Backspace, false, true);
        assert_eq!(text_of(&app, id).content, "a two ");
        key(&mut app, egui::Key::Delete, false, false);
        assert_eq!(text_of(&app, id).content, "a two ");
    }

    #[test]
    fn ctrl_a_and_clipboard_text() {
        let (mut app, _) = app_with_text("abc def");
        key(&mut app, egui::Key::Home, false, true);
        key(&mut app, egui::Key::A, false, true);
        assert_eq!(selected_text(&app).as_deref(), Some("abc def"));
        key(&mut app, egui::Key::ArrowRight, false, false);
        assert_eq!(selected_text(&app), None, "collapsed after Right");
    }

    #[test]
    fn per_range_bold_and_mixed_summary() {
        let (mut app, id) = app_with_text("hello world");
        set_sel(&mut app, 5, 0, false);
        text_command(&mut app, TextCmd::Bold);
        let t = text_of(&app, id);
        assert_eq!(t.runs.len(), 1);
        assert_eq!((t.runs[0].start, t.runs[0].end, t.runs[0].style.font_weight), (0, 5, 700));
        assert_eq!(t.style.font_weight, 400);
        set_sel(&mut app, 11, 0, false);
        assert_eq!(session_summary(&app).unwrap().weight, None, "mixed");
        // toggling over a mixed range makes it bold first
        text_command(&mut app, TextCmd::Bold);
        let t = text_of(&app, id);
        assert!(t.runs.is_empty());
        assert_eq!(t.style.font_weight, 700);
        text_command(&mut app, TextCmd::Bold);
        assert_eq!(text_of(&app, id).style.font_weight, 400);
    }

    #[test]
    fn caret_style_applies_to_next_typed_text_only() {
        let (mut app, id) = app_with_text("ab");
        set_sel(&mut app, 2, 2, false);
        text_command(&mut app, TextCmd::Bold);
        insert_text(&mut app, "C", EditKind::Typing);
        insert_text(&mut app, "d", EditKind::Typing);
        let t = text_of(&app, id);
        assert_eq!(t.content, "abCd");
        assert_eq!(t.runs.len(), 1);
        assert_eq!((t.runs[0].start, t.runs[0].end), (2, 4), "bold continues from the typed char");
    }

    #[test]
    fn size_shortcuts_change_by_one() {
        let (mut app, id) = app_with_text("abc");
        text_command(&mut app, TextCmd::SizeUp);
        assert_eq!(text_of(&app, id).style.font_size, 17.0);
        set_sel(&mut app, 1, 0, false);
        text_command(&mut app, TextCmd::SizeDown);
        let t = text_of(&app, id);
        assert_eq!((t.runs[0].start, t.runs[0].end, t.runs[0].style.font_size), (0, 1, 16.0));
    }

    #[test]
    fn undo_steps_follow_words_and_redo() {
        let (mut app, id) = app_with_text("");
        app.now = 0.0;
        for (i, w) in ["he", "llo", " ", "wor", "ld"].iter().enumerate() {
            app.now = 0.1 * i as f64;
            insert_text(&mut app, w, EditKind::Typing);
        }
        assert_eq!(text_of(&app, id).content, "hello world");
        undo_step(&mut app, false);
        assert_eq!(text_of(&app, id).content, "hello ", "the last word goes first");
        undo_step(&mut app, false);
        assert_eq!(text_of(&app, id).content, "");
        undo_step(&mut app, true);
        assert_eq!(text_of(&app, id).content, "hello ");
        undo_step(&mut app, true);
        assert_eq!(text_of(&app, id).content, "hello world");
    }

    fn kinds(app: &App, id: NodeId) -> Vec<Option<(ListKind, u8)>> {
        text_of(app, id).paragraph_lists().iter().map(|i| i.map(|i| (i.kind, i.level))).collect()
    }

    #[test]
    fn enter_continues_lists_and_toggles_work() {
        let (mut app, id) = app_with_text("one");
        set_sel(&mut app, 3, 3, false);
        text_command(&mut app, TextCmd::List(ListKind::Bullet));
        assert_eq!(text_of(&app, id).content, "one", "the text itself is untouched");
        assert_eq!(kinds(&app, id), vec![Some((ListKind::Bullet, 0))]);
        key(&mut app, egui::Key::Enter, false, false);
        assert_eq!(text_of(&app, id).content, "one\n");
        assert_eq!(kinds(&app, id), vec![Some((ListKind::Bullet, 0)); 2], "Enter continues the list");
        insert_text(&mut app, "two", EditKind::Typing);
        key(&mut app, egui::Key::Enter, false, false);
        assert_eq!(kinds(&app, id).len(), 3);
        key(&mut app, egui::Key::Enter, false, false);
        let t = text_of(&app, id);
        assert_eq!(t.content, "one\ntwo\n", "Enter on an empty item adds no paragraph");
        assert_eq!(kinds(&app, id), vec![Some((ListKind::Bullet, 0)), Some((ListKind::Bullet, 0)), None], "and ends the list");
        // toggling off
        set_sel(&mut app, 0, 0, false);
        text_command(&mut app, TextCmd::List(ListKind::Bullet));
        assert_eq!(kinds(&app, id), vec![None, Some((ListKind::Bullet, 0)), None]);
    }

    #[test]
    fn shortcuts_toggle_and_tab_nests() {
        let (mut app, id) = app_with_text("a\nb\nc");
        key(&mut app, egui::Key::A, false, true);
        key(&mut app, egui::Key::Num8, true, true);
        assert_eq!(kinds(&app, id), vec![Some((ListKind::Bullet, 0)); 3]);
        key(&mut app, egui::Key::Num7, true, true);
        assert_eq!(kinds(&app, id), vec![Some((ListKind::Numbered, 0)); 3], "ctrl+shift+7 switches to numbers");
        set_sel(&mut app, 2, 2, false);
        key(&mut app, egui::Key::Tab, false, false);
        key(&mut app, egui::Key::Tab, false, false);
        assert_eq!(kinds(&app, id)[1], Some((ListKind::Numbered, 2)));
        key(&mut app, egui::Key::Tab, true, false);
        assert_eq!(kinds(&app, id)[1], Some((ListKind::Numbered, 1)));
        assert_eq!(kinds(&app, id)[0], Some((ListKind::Numbered, 0)), "only the caret's item moved");
        key(&mut app, egui::Key::Num7, true, true);
        assert_eq!(kinds(&app, id)[1], None, "toggling the caret's item off leaves the others");
        assert_eq!(kinds(&app, id)[2], Some((ListKind::Numbered, 0)));
    }

    #[test]
    fn backspace_at_item_start_unlists() {
        let (mut app, id) = app_with_text("ab");
        text_command(&mut app, TextCmd::List(ListKind::Numbered));
        set_sel(&mut app, 0, 0, false);
        key(&mut app, egui::Key::Backspace, false, false);
        assert_eq!(text_of(&app, id).content, "ab");
        assert!(text_of(&app, id).lists.is_empty());
        // mid text it deletes as usual
        set_sel(&mut app, 2, 2, false);
        key(&mut app, egui::Key::Backspace, false, false);
        assert_eq!(text_of(&app, id).content, "a");
    }

    #[test]
    fn ime_composition_is_previewed_then_committed() {
        let (mut app, id) = app_with_text("ab");
        set_sel(&mut app, 2, 2, false);
        preedit(&mut app, "に");
        assert_eq!(text_of(&app, id).content, "abに");
        preedit(&mut app, "にほ");
        assert_eq!(text_of(&app, id).content, "abにほ");
        commit_ime(&mut app, "日本");
        let t = text_of(&app, id);
        assert_eq!(t.content, "ab日本");
        assert!(t.runs.is_empty(), "no leftover underline style: {:?}", t.runs);
        assert!(app.canvas.text.as_ref().unwrap().preedit.is_none());
        assert_eq!(app.canvas.text.as_ref().unwrap().caret, "ab日本".len());
    }

    #[test]
    fn finish_commits_one_document_undo_step() {
        let (mut app, id) = app_with_text("x");
        insert_text(&mut app, "abc", EditKind::Typing);
        finish_text(&mut app, true);
        assert_eq!(text_of(&app, id).content, "abc");
        app.undo();
        assert_eq!(text_of(&app, id).content, "x");
    }
}
