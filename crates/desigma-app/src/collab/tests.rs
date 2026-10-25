//! Two editors in one process: through in-memory channels (fast, deterministic) and, in
//! `over_real_network`, through desigma-net on loopback.

use super::link::{Closed, GuestEv, GuestLink, HostEv, HostLink};
use super::*;
use crate::app::App;
use desigma_doc::{Document, Node, NodeId, Rect};
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;

#[derive(Default)]
pub(crate) struct Wire {
    pub host: VecDeque<HostEv>,
    pub guests: BTreeMap<u32, VecDeque<GuestEv>>,
    next: u32,
    answered: Vec<(u64, bool)>,
}
pub(crate) type Net = Rc<RefCell<Wire>>;

pub(crate) struct MemHost(pub(crate) Net);
impl HostLink for MemHost {
    fn drain(&self) -> Vec<HostEv> {
        self.0.borrow_mut().host.drain(..).collect()
    }
    fn respond(&self, id: u64, accept: bool) {
        self.0.borrow_mut().answered.push((id, accept));
    }
    fn send(&self, peer: u32, bytes: Vec<u8>) {
        if let Some(q) = self.0.borrow_mut().guests.get_mut(&peer) {
            q.push_back(GuestEv::Msg(bytes));
        }
    }
    fn send_presence(&self, peer: u32, bytes: &[u8]) {
        if let Some(q) = self.0.borrow_mut().guests.get_mut(&peer) {
            q.push_back(GuestEv::Presence(bytes.to_vec()));
        }
    }
    fn kick(&self, peer: u32) {
        if let Some(q) = self.0.borrow_mut().guests.get_mut(&peer) {
            q.push_back(GuestEv::Closed(Closed::Kicked));
        }
    }
}

pub(crate) struct MemGuest(pub(crate) Net, pub(crate) u32);
impl GuestLink for MemGuest {
    fn drain(&self) -> Vec<GuestEv> {
        self.0.borrow_mut().guests.get_mut(&self.1).map(|q| q.drain(..).collect()).unwrap_or_default()
    }
    fn send(&self, bytes: Vec<u8>) {
        self.0.borrow_mut().host.push_back(HostEv::Msg { peer: self.1, bytes });
    }
    fn send_presence(&self, bytes: &[u8]) {
        self.0.borrow_mut().host.push_back(HostEv::Presence { peer: self.1, bytes: bytes.to_vec() });
    }
    fn leave(&self) {
        self.0.borrow_mut().host.push_back(HostEv::Left { peer: self.1 });
    }
}

pub(crate) fn connect(net: &Net, name: &str) -> MemGuest {
    let mut w = net.borrow_mut();
    w.next += 1;
    let peer = w.next;
    w.guests.insert(peer, VecDeque::from([GuestEv::Joined]));
    w.host.push_back(HostEv::Joined { peer, name: name.into() });
    MemGuest(net.clone(), peer)
}

fn doc() -> (Document, NodeId, NodeId) {
    let mut d = Document::new("Shared");
    let page = d.add_page("Page 1");
    let a = d.alloc_id();
    d.insert(page, Node::rectangle(a, Rect::new(0.0, 0.0, 100.0, 80.0))).unwrap();
    let b = d.alloc_id();
    d.insert(page, Node::rectangle(b, Rect::new(200.0, 0.0, 50.0, 50.0))).unwrap();
    (d, a, b)
}

struct Rig {
    host: App,
    guest: App,
    net: Net,
    now: f64,
    a: NodeId,
    b: NodeId,
}

impl Rig {
    fn new(role: Role) -> Rig {
        let (d, a, b) = doc();
        let mut host = App::new(d);
        let net = Net::default();
        host.start_sharing_with(Box::new(MemHost(net.clone())), false, false, role);
        let mut guest = App::new(Document::new("Empty guest window"));
        guest.join_with(Box::new(connect(&net, "Bea")));
        let mut r = Rig { host, guest, net, now: 1.0, a, b };
        r.settle();
        assert!(r.guest.collab.is_guest(), "guest joined");
        r
    }

    /// A few frames on both sides, 100 ms apart.
    fn settle(&mut self) {
        for _ in 0..8 {
            self.now += 0.1;
            frame(&mut self.host, self.now);
            frame(&mut self.guest, self.now);
        }
    }

    fn name(app: &App, id: NodeId) -> String {
        app.doc.get(id).map(|n| n.name.clone()).unwrap_or_else(|| "<gone>".into())
    }
}

/// One editor frame as far as the session is concerned (the canvas render consumes `dirty`).
fn frame(app: &mut App, now: f64) {
    app.now = now;
    pump_with(app, now, &Local::default());
    crate::canvas::tiles::take_rev(app);
}

#[test]
fn the_shared_file_opens_on_the_guest() {
    let r = Rig::new(Role::Editor);
    assert_eq!(r.guest.doc.name, "Shared");
    assert_eq!(r.guest.doc.nodes.len(), r.host.doc.nodes.len());
    assert!(r.guest.file_path.is_none() && r.guest.lib_file.is_none(), "a shared file is not tied to a path");
    assert_eq!(r.guest.doc.id_peer(), Some(1));
    // ids the guest hands out come from its own range
    let mut g = r.guest;
    assert_eq!(g.doc.alloc_id().0 >> 40, 1);
    assert_eq!(r.host.doc.id_peer(), Some(0));
}

#[test]
fn edits_travel_both_ways() {
    let mut r = Rig::new(Role::Editor);
    let a = r.a;
    r.host.edit_nodes(&[a], |n| n.name = "From host".into());
    r.settle();
    assert_eq!(Rig::name(&r.guest, a), "From host");

    let b = r.b;
    r.guest.edit_nodes(&[b], |n| n.name = "From guest".into());
    r.settle();
    assert_eq!(Rig::name(&r.host, b), "From guest");
    assert!(r.host.canvas.tiles.rev() > 0, "the host canvas was told to redraw");
}

#[test]
fn nodes_created_by_a_guest_use_its_id_range_and_reach_the_host() {
    let mut r = Rig::new(Role::Editor);
    let page = r.guest.page;
    let id = r.guest.doc.alloc_id();
    let mut cmd = Node::rectangle(id, Rect::new(5.0, 5.0, 10.0, 10.0));
    cmd.name = "New".into();
    let mut tx = crate::edit::Tx::new();
    tx.cmd(&mut r.guest.doc, desigma_doc::Command::Insert { parent: page, index: 0, nodes: vec![cmd] }).unwrap();
    r.guest.run_tx(tx);
    r.settle();
    assert_eq!(Rig::name(&r.host, id), "New");
    assert_eq!(id.0 >> 40, 1);
}

#[test]
fn presence_round_trips_and_leaving_removes_it() {
    let mut r = Rig::new(Role::Editor);
    let a = r.a;
    r.host.selection = vec![a];
    // the guest's pointer is over its canvas at (300, 200) on screen
    let pointer = r.guest.canvas_rect.min + egui::vec2(300.0, 200.0);
    for _ in 0..4 {
        r.now += 0.1;
        frame(&mut r.host, r.now);
        pump_with(&mut r.guest, r.now, &Local { pointer: Some(pointer), esc: false });
        crate::canvas::tiles::take_rev(&mut r.guest);
    }
    let seen = &r.guest.collab.remotes[&0];
    assert_eq!(seen.p.sel, vec![a.0], "the host's selection reached the guest");
    assert!(seen.p.name.len() > 1);
    let theirs = &r.host.collab.remotes[&1];
    let want = r.guest.view.to_page(r.guest.canvas_rect.min, pointer);
    let got = theirs.p.cursor.expect("cursor");
    assert!((got[0] - want.x).abs() < 0.2 && (got[1] - want.y).abs() < 0.2);
    assert_eq!(theirs.p.page, r.guest.page.0);

    // the guest leaves: its cursor goes away on the host
    r.guest.leave_session();
    r.settle();
    assert!(r.host.collab.remotes.is_empty());
    assert!(r.host.collab.host.as_ref().unwrap().peers.is_empty());
}

#[test]
fn a_third_person_sees_a_guest_through_the_host() {
    let mut r = Rig::new(Role::Editor);
    let mut carol = App::new(Document::new("c"));
    carol.join_with(Box::new(connect(&r.net, "Cy")));
    for _ in 0..8 {
        r.now += 0.1;
        frame(&mut r.host, r.now);
        frame(&mut r.guest, r.now);
        frame(&mut carol, r.now);
    }
    assert!(carol.collab.is_guest());
    assert!(carol.collab.remotes.contains_key(&0) && carol.collab.remotes.contains_key(&1), "host and the other guest are both visible");
    assert_eq!(carol.collab.remotes[&1].p.name, r.guest.collab_name());
}

#[test]
fn view_only_guests_cannot_edit() {
    let mut r = Rig::new(Role::Viewer);
    assert!(r.guest.collab.view_only());
    assert!(r.guest.read_only());
    let a = r.a;
    // through the editor's own edit path: refused outright
    r.guest.edit_nodes(&[a], |n| n.name = "Nope".into());
    r.settle();
    assert_eq!(Rig::name(&r.guest, a), Rig::name(&r.host, a));
    assert_ne!(Rig::name(&r.host, a), "Nope");
    // a change forced into the document anyway is refused by the host and corrected
    r.guest.doc.get_mut(a).unwrap().name = "Sneaky".into();
    r.guest.changed();
    r.settle();
    assert_ne!(Rig::name(&r.host, a), "Sneaky");
    assert_ne!(Rig::name(&r.guest, a), "Sneaky", "the guest was corrected");
    // the host still reaches the guest
    r.host.edit_nodes(&[a], |n| n.name = "Host".into());
    r.settle();
    assert_eq!(Rig::name(&r.guest, a), "Host");
}

#[test]
fn the_host_can_switch_a_guest_to_view_only_and_back() {
    let mut r = Rig::new(Role::Editor);
    assert!(!r.guest.read_only());
    r.host.set_all_roles(Role::Viewer);
    r.settle();
    assert!(r.guest.read_only());
    r.host.set_peer_role(1, Role::Editor);
    r.settle();
    assert!(!r.guest.read_only());
}

#[test]
fn a_guests_undo_does_not_revert_a_host_edit() {
    let mut r = Rig::new(Role::Editor);
    let a = r.a;
    let before = Rig::name(&r.host, a);
    r.guest.edit_nodes(&[a], |n| n.name = "Guest".into());
    r.settle();
    r.host.edit_nodes(&[a], |n| n.name = "Host later".into());
    r.settle();
    assert_eq!(Rig::name(&r.guest, a), "Host later");
    r.guest.undo();
    r.settle();
    assert_eq!(Rig::name(&r.guest, a), "Host later", "undo kept the newer host value");
    assert_eq!(Rig::name(&r.host, a), "Host later");
    assert_ne!(before, "Host later");
}

#[test]
fn a_hosts_undo_does_not_revert_a_guest_edit() {
    let mut r = Rig::new(Role::Editor);
    let a = r.a;
    r.host.edit_nodes(&[a], |n| n.name = "Host".into());
    r.settle();
    r.guest.edit_nodes(&[a], |n| n.name = "Guest later".into());
    r.settle();
    assert_eq!(Rig::name(&r.host, a), "Guest later");
    r.host.undo();
    r.settle();
    assert_eq!(Rig::name(&r.host, a), "Guest later");
    assert_eq!(Rig::name(&r.guest, a), "Guest later");
}

#[test]
fn undo_of_something_a_person_deleted_is_a_quiet_no_op() {
    let mut r = Rig::new(Role::Editor);
    let b = r.b;
    r.guest.edit_nodes(&[b], |n| n.name = "Edited".into());
    r.settle();
    // the host deletes the node
    let mut tx = crate::edit::Tx::new();
    tx.cmd(&mut r.host.doc, desigma_doc::Command::Remove { id: b }).unwrap();
    r.host.run_tx(tx);
    r.settle();
    assert!(r.guest.doc.get(b).is_none());
    r.guest.toast = None;
    assert!(!r.guest.undo());
    assert!(r.guest.toast.is_none(), "no error toast: {:?}", r.guest.toast.as_ref().map(|t| &t.text));
    // the document still agrees
    assert_eq!(r.guest.doc.nodes.len(), r.host.doc.nodes.len());
}

#[test]
fn kicking_a_guest_ends_its_session_but_keeps_its_copy() {
    let mut r = Rig::new(Role::Editor);
    r.host.kick_peer(1);
    r.settle();
    let g = r.guest.collab.guest.as_ref().expect("copy kept");
    assert_eq!(g.ended, Some(Closed::Kicked));
    assert!(!r.guest.read_only(), "an ended session is just a local copy");
    assert!(r.guest.doc.get(r.a).is_some());
    // edits after the end stay local
    let a = r.a;
    r.guest.edit_nodes(&[a], |n| n.name = "Local".into());
    r.settle();
    assert_ne!(Rig::name(&r.host, a), "Local");
}

#[test]
fn the_host_ending_the_session_tells_guests() {
    let mut r = Rig::new(Role::Editor);
    r.net.borrow_mut().guests.get_mut(&1).unwrap().push_back(GuestEv::Closed(Closed::HostEnded));
    r.host.stop_sharing();
    r.settle();
    assert_eq!(r.guest.collab.guest.as_ref().unwrap().ended, Some(Closed::HostEnded));
    assert!(r.guest.collab.remotes.is_empty());
    assert!(!r.host.collab.hosting());
}

#[test]
fn approval_requests_are_listed_until_answered() {
    let (d, _, _) = doc();
    let mut host = App::new(d);
    let net = Net::default();
    host.start_sharing_with(Box::new(MemHost(net.clone())), true, true, Role::Editor);
    net.borrow_mut().host.push_back(HostEv::Request { id: 7, name: "Dee".into() });
    net.borrow_mut().host.push_back(HostEv::Ready { link: "desigma://join/abc".into() });
    frame(&mut host, 1.0);
    assert_eq!(host.collab.requests, vec![Request { id: 7, name: "Dee".into() }]);
    assert_eq!(host.collab.host.as_ref().unwrap().url.as_deref(), Some("desigma://join/abc"));
    host.respond_request(7, true);
    assert!(host.collab.requests.is_empty());
    assert_eq!(net.borrow().answered, vec![(7, true)]);
}

#[test]
fn replacing_the_document_ends_the_session() {
    let mut r = Rig::new(Role::Editor);
    r.host.new_document();
    assert!(!r.host.collab.active());
    r.guest.new_document();
    assert!(!r.guest.collab.active());
}

#[test]
fn the_session_does_not_flood_when_nothing_changes() {
    let mut r = Rig::new(Role::Editor);
    r.settle();
    let before = r.net.borrow().host.len();
    for _ in 0..20 {
        r.now += 0.01;
        frame(&mut r.guest, r.now);
    }
    // only presence keepalives at most, never edits
    let w = r.net.borrow();
    assert!(w.host.iter().skip(before).all(|e| matches!(e, HostEv::Presence { .. })));
}

#[test]
fn following_tracks_the_viewport_until_the_user_takes_over() {
    let mut r = Rig::new(Role::Editor);
    r.host.view.zoom = 2.0;
    r.host.view.offset = desigma_doc::Vec2::new(400.0, 300.0);
    for _ in 0..6 {
        r.settle();
    }
    r.guest.toggle_follow(0);
    for _ in 0..40 {
        r.settle();
    }
    assert!((r.guest.view.zoom - 2.0).abs() < 0.05, "zoom {}", r.guest.view.zoom);
    let hc = r.host.view.to_page(r.host.canvas_rect.min, r.host.canvas_rect.center());
    let gc = r.guest.view.to_page(r.guest.canvas_rect.min, r.guest.canvas_rect.center());
    assert!((hc.x - gc.x).abs() < 2.0 && (hc.y - gc.y).abs() < 2.0, "{hc:?} {gc:?}");
    assert_eq!(r.guest.collab.following, Some(0));
    // the user pans: following stops
    r.guest.view.offset.x += 50.0;
    r.settle();
    assert_eq!(r.guest.collab.following, None);
    // Escape also stops it
    r.guest.toggle_follow(0);
    r.settle();
    assert_eq!(r.guest.collab.following, Some(0));
    pump_with(&mut r.guest, r.now + 0.1, &Local { pointer: None, esc: true });
    assert_eq!(r.guest.collab.following, None);
}

#[test]
fn join_error_texts_are_clear() {
    use desigma_net::JoinError::*;
    let all =
        [VersionMismatch { host: 2, guest: 1 }, WrongPassword, Locked { retry_secs: 30 }, Denied, Full, Connect("x".into()), Timeout, Protocol("x".into())];
    for e in all {
        let t = link::join_error_text(&e);
        assert!(t.len() > 20 && !t.contains("Error"), "{t}");
    }
}

#[test]
fn id_ranges_survive_everything_the_app_does_to_a_document() {
    let (d, ..) = doc();
    let mut app = App::new(d);
    let ids_before: Vec<u64> = (0..3).map(|_| app.doc.alloc_id().0).collect();
    let net = Net::default();
    app.start_sharing_with(Box::new(MemHost(net)), false, false, Role::Editor);
    assert_eq!(app.doc.id_peer(), Some(0));
    let after = app.doc.alloc_id().0;
    assert!(after > *ids_before.last().unwrap(), "host ids keep counting up");
}

#[test]
fn over_real_network() {
    std::env::set_var("DESIGMA_COLLAB_NETWORK", "direct");
    let ctx = egui::Context::default();
    let (d, a, _) = doc();
    let mut host = App::new(d);
    host.start_sharing(&ctx, Some("pw".into()), false, Role::Editor);
    let mut guest = App::new(Document::new("x"));
    let mut now = 1.0;
    let end = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let step = |host: &mut App, guest: &mut App, now: &mut f64| {
        *now += 0.1;
        frame(host, *now);
        frame(guest, *now);
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    let link = loop {
        step(&mut host, &mut guest, &mut now);
        if let Some(u) = host.collab.host.as_ref().and_then(|h| h.url.clone()) {
            break u;
        }
        assert!(std::time::Instant::now() < end, "host never became ready");
    };
    // wrong password first
    assert!(guest.join_session(&ctx, &link, "Bea", "").is_err(), "a password is required");
    guest.join_session(&ctx, &link, "Bea", "pw").unwrap();
    while !guest.collab.is_guest() {
        step(&mut host, &mut guest, &mut now);
        assert!(std::time::Instant::now() < end, "guest never joined");
    }
    assert_eq!(guest.doc.nodes.len(), host.doc.nodes.len());
    host.edit_nodes(&[a], |n| n.name = "Over the wire".into());
    while Rig::name(&guest, a) != "Over the wire" {
        step(&mut host, &mut guest, &mut now);
        assert!(std::time::Instant::now() < end, "host edit never arrived");
    }
    guest.edit_nodes(&[a], |n| n.name = "And back".into());
    while Rig::name(&host, a) != "And back" {
        step(&mut host, &mut guest, &mut now);
        assert!(std::time::Instant::now() < end, "guest edit never arrived");
    }
    for _ in 0..40 {
        step(&mut host, &mut guest, &mut now);
    }
    assert!(guest.collab.remotes.contains_key(&0) && host.collab.remotes.contains_key(&1), "presence over datagrams");
    host.stop_sharing();
    while guest.collab.guest.as_ref().is_some_and(|g| g.ended.is_none()) {
        step(&mut host, &mut guest, &mut now);
        assert!(std::time::Instant::now() < end, "guest never noticed the end");
    }
}
