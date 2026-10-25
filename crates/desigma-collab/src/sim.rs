//! Tests: unit tests per piece and a randomized multi-peer simulation.

use crate::*;
use desigma_doc::*;
use std::collections::VecDeque;

/// Small seeded xorshift PRNG.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.next() % 100 < pct
    }
    fn pick<T: Copy>(&mut self, v: &[T]) -> Option<T> {
        (!v.is_empty()).then(|| v[self.below(v.len())])
    }
}

fn base_doc() -> Document {
    let mut d = Document::new("t");
    let page = d.add_page("P1");
    for i in 0..3 {
        let id = d.alloc_id();
        d.insert(page, Node::frame(id, Rect::new(i as f32 * 100.0, 0.0, 90.0, 90.0))).unwrap();
    }
    d
}

fn assert_same(a: &Document, b: &Document, ctx: &str) {
    assert_eq!(a.nodes.keys().map(|k| k.0).collect::<Vec<_>>(), b.nodes.keys().map(|k| k.0).collect::<Vec<_>>(), "{ctx}: node ids");
    for (id, n) in &a.nodes {
        assert_eq!(Some(n), b.nodes.get(id), "{ctx}: node {id:?}");
    }
    assert_eq!(a.pages, b.pages, "{ctx}: pages");
    assert_eq!(doc_fields(a), doc_fields(b), "{ctx}: doc fields");
    assert_eq!(a.images.keys().collect::<Vec<_>>(), b.images.keys().collect::<Vec<_>>(), "{ctx}: images");
}

fn apply_all(doc: &mut Document, ops: &[Op]) {
    for op in ops {
        apply(doc, op).unwrap_or_else(|e| panic!("{e}: {op:?}"));
    }
}

fn non_pages(d: &Document) -> Vec<NodeId> {
    d.nodes.values().filter(|n| n.parent.is_some()).map(|n| n.id).collect()
}

fn containers(d: &Document) -> Vec<NodeId> {
    d.nodes.values().filter(|n| n.kind.is_container()).map(|n| n.id).collect()
}

/// One random edit of the kind the app makes directly on its document.
fn random_edit(rng: &mut Rng, d: &mut Document) {
    match rng.below(100) {
        0..=27 => {
            let Some(parent) = rng.pick(&containers(d)) else { return };
            let id = d.alloc_id();
            let r = Rect::new(rng.below(500) as f32, rng.below(500) as f32, 10.0 + rng.below(80) as f32, 10.0 + rng.below(80) as f32);
            let n = if rng.chance(50) { Node::frame(id, r) } else { Node::rectangle(id, r) };
            d.insert(parent, n).unwrap();
            if rng.chance(50) {
                let idx = rng.below(d.get(parent).unwrap().children.len() + 1);
                d.reparent(id, parent, idx).unwrap();
            }
        }
        28..=57 => {
            let Some(id) = rng.pick(&non_pages(d)) else { return };
            let n = d.get_mut(id).unwrap();
            match rng.below(7) {
                0 => n.name = format!("n{}", rng.below(1000)),
                1 => n.opacity = rng.below(100) as f32 / 100.0,
                2 => n.visible = !n.visible,
                3 => n.locked = !n.locked,
                4 => n.transform.tx = rng.below(1000) as f32,
                5 => n.size = Vec2::new(rng.below(300) as f32, rng.below(300) as f32),
                _ => n.fills = vec![Paint::solid(Color::rgb(rng.below(256) as u8, 0, 0))],
            }
        }
        58..=79 => {
            let (Some(id), Some(to)) = (rng.pick(&non_pages(d)), rng.pick(&containers(d))) else { return };
            let idx = rng.below(d.get(to).unwrap().children.len() + 1);
            let _ = d.reparent(id, to, idx);
        }
        80..=89 => {
            if let Some(id) = rng.pick(&non_pages(d)) {
                let _ = d.remove(id);
            }
        }
        90..=93 => {
            let key = format!("s{}", rng.below(4));
            if rng.chance(30) {
                d.styles.remove(&key);
                d.style_meta.remove(&key);
            } else {
                d.styles.insert(key.clone(), Style::Fill(vec![Paint::solid(Color::rgb(0, rng.below(256) as u8, 0))]));
                d.style_meta.insert(key, StyleMeta { name: format!("S{}", rng.below(100)), ..Default::default() });
            }
        }
        94..=95 => d.name = format!("doc{}", rng.below(100)),
        96..=97 => {
            d.add_page(format!("page{}", rng.below(100)));
        }
        _ => {
            if d.pages.len() > 1 {
                let (a, b) = (rng.below(d.pages.len()), rng.below(d.pages.len()));
                d.pages.swap(a, b);
            }
        }
    }
}

// ---- diff / apply ----

#[test]
fn diff_apply_roundtrip_basic() {
    let a = base_doc();
    let mut b = a.clone();
    let page = b.pages[0];
    let frame = b.get(page).unwrap().children[0];
    let rid = b.alloc_id();
    b.insert(frame, Node::rectangle(rid, Rect::new(1.0, 2.0, 3.0, 4.0))).unwrap();
    b.get_mut(frame).unwrap().name = "Renamed".into();
    b.get_mut(frame).unwrap().fills.clear();
    b.name = "other".into();
    let ops = diff(&a, &b);
    assert!(matches!(ops[0], Op::Create { .. }));
    let mut c = a.clone();
    apply_all(&mut c, &ops);
    assert_same(&c, &b, "basic");
    assert!(diff(&c, &b).is_empty());
}

#[test]
fn diff_skips_unchanged_and_is_field_level() {
    let a = base_doc();
    assert!(diff(&a, &a.clone()).is_empty());
    let mut b = a.clone();
    let id = b.get(b.pages[0]).unwrap().children[1];
    b.get_mut(id).unwrap().opacity = 0.5;
    b.get_mut(id).unwrap().locked = true;
    let ops = diff(&a, &b);
    assert_eq!(ops.len(), 1);
    let Op::Patch { id: pid, fields } = &ops[0] else { panic!("{ops:?}") };
    assert_eq!(*pid, id);
    assert_eq!(fields.keys().map(String::as_str).collect::<Vec<_>>(), vec!["locked", "opacity"]);
}

#[test]
fn diff_swapped_nesting_has_no_transient_cycle() {
    let mut a = base_doc();
    let page = a.pages[0];
    let (x, y) = (a.get(page).unwrap().children[0], a.get(page).unwrap().children[1]);
    a.reparent(y, x, 0).unwrap();
    let mut b = a.clone();
    b.reparent(y, page, 0).unwrap();
    b.reparent(x, y, 0).unwrap();
    let mut c = a.clone();
    apply_all(&mut c, &diff(&a, &b));
    assert_same(&c, &b, "swap");
}

#[test]
fn diff_deletes_only_subtree_roots_and_rescues_moved_children() {
    let mut a = base_doc();
    let page = a.pages[0];
    let (x, y) = (a.get(page).unwrap().children[0], a.get(page).unwrap().children[1]);
    let k1 = a.alloc_id();
    a.insert(x, Node::rectangle(k1, Rect::new(0.0, 0.0, 1.0, 1.0))).unwrap();
    let k2 = a.alloc_id();
    a.insert(x, Node::rectangle(k2, Rect::new(0.0, 0.0, 1.0, 1.0))).unwrap();
    let mut b = a.clone();
    b.reparent(k2, y, 0).unwrap();
    b.remove(x).unwrap();
    let ops = diff(&a, &b);
    assert_eq!(ops.iter().filter(|o| matches!(o, Op::Delete { .. })).count(), 1);
    let mut c = a.clone();
    apply_all(&mut c, &ops);
    assert_same(&c, &b, "delete");
}

#[test]
fn diff_reorder_moves_few_nodes() {
    let mut a = Document::new("t");
    let page = a.add_page("P");
    for i in 0..20 {
        let id = a.alloc_id();
        a.insert(page, Node::rectangle(id, Rect::new(i as f32, 0.0, 1.0, 1.0))).unwrap();
    }
    let mut b = a.clone();
    let first = b.get(page).unwrap().children[0];
    b.reparent(first, page, 19).unwrap();
    let ops = diff(&a, &b);
    assert_eq!(ops.len(), 1);
    let mut c = a.clone();
    apply_all(&mut c, &ops);
    assert_same(&c, &b, "reorder");
}

#[test]
fn diff_images_by_hash() {
    let a = base_doc();
    let mut b = a.clone();
    b.images.insert("abc".into(), vec![1, 2, 3]);
    let ops = diff(&a, &b);
    assert_eq!(ops, vec![Op::Image { hash: "abc".into() }]);
    let mut c = a.clone();
    apply_all(&mut c, &ops);
    assert!(c.images.contains_key("abc"));
}

#[test]
fn diff_apply_random_property() {
    for seed in 0..200 {
        let mut rng = Rng::new(seed);
        let mut a = base_doc();
        a.set_id_peer(1);
        for _ in 0..30 {
            random_edit(&mut rng, &mut a);
        }
        let mut b = a.clone();
        for _ in 0..1 + rng.below(25) {
            random_edit(&mut rng, &mut b);
        }
        let mut c = a.clone();
        apply_all(&mut c, &diff(&a, &b));
        assert_same(&c, &b, &format!("seed {seed}"));
    }
}

#[test]
fn doc_fields_cover_the_document() {
    let mut d = base_doc();
    d.timelines.insert(d.pages[0], TimelineSettings { duration_ms: 3000, ..Default::default() });
    let Ok(serde_json::Value::Object(m)) = serde_json::to_value(&d) else { panic!() };
    for k in m.keys() {
        assert!(DOC_FIELDS.contains(&k.as_str()) || ["nodes", "images", "next_id"].contains(&k.as_str()), "field {k} is not synced");
    }
    assert_eq!(doc_fields(&d).keys().map(String::as_str).collect::<std::collections::BTreeSet<_>>(), DOC_FIELDS.iter().copied().collect());
}

#[test]
fn apply_validates() {
    let mut d = base_doc();
    let page = d.pages[0];
    let (x, y) = (d.get(page).unwrap().children[0], d.get(page).unwrap().children[1]);
    let rid = d.alloc_id();
    d.insert(x, Node::rectangle(rid, Rect::new(0.0, 0.0, 1.0, 1.0))).unwrap();
    // Into own descendant, into itself, into a non-container, into a missing parent.
    d.reparent(y, x, 0).unwrap();
    assert_eq!(apply(&mut d, &Op::Move { id: x, parent: y, after: None }), Err(ApplyError::Cycle(x)));
    assert_eq!(apply(&mut d, &Op::Move { id: x, parent: x, after: None }), Err(ApplyError::Cycle(x)));
    assert_eq!(apply(&mut d, &Op::Move { id: y, parent: rid, after: None }), Err(ApplyError::NotContainer(rid)));
    assert_eq!(apply(&mut d, &Op::Move { id: y, parent: NodeId(9999), after: None }), Err(ApplyError::NoParent(NodeId(9999))));
    // Unknown anchor appends; None goes first.
    let z = d.get(page).unwrap().children[1];
    apply(&mut d, &Op::Move { id: z, parent: x, after: Some(NodeId(777)) }).unwrap();
    assert_eq!(d.get(x).unwrap().children.last(), Some(&z));
    apply(&mut d, &Op::Move { id: z, parent: x, after: None }).unwrap();
    assert_eq!(d.get(x).unwrap().children.first(), Some(&z));
    // Missing nodes: patch and delete are no-ops, move is refused.
    let gone = NodeId(4242);
    let p = Op::Patch { id: gone, fields: [("name".to_string(), serde_json::json!("x"))].into() };
    apply(&mut d, &p).unwrap();
    assert_eq!(apply(&mut d, &Op::Move { id: gone, parent: x, after: None }), Err(ApplyError::Missing(gone)));
    apply(&mut d, &Op::Delete { id: gone }).unwrap();
    // Duplicate create.
    let dup = Op::Create { node: Box::new(d.get(rid).unwrap().clone()), parent: Some(x), after: None };
    assert_eq!(apply(&mut d, &dup), Err(ApplyError::Exists(rid)));
    // Bad patch value.
    let bad = Op::Patch { id: rid, fields: [("opacity".to_string(), serde_json::json!("not a number"))].into() };
    assert_eq!(apply(&mut d, &bad), Err(ApplyError::BadPatch(rid)));
    // Patch cannot rewrite placement.
    let sneaky = Op::Patch { id: rid, fields: [("parent".to_string(), serde_json::json!(null)), ("id".to_string(), serde_json::json!(5))].into() };
    apply(&mut d, &sneaky).unwrap();
    assert_eq!(d.get(rid).unwrap().parent, Some(x));
}

#[test]
fn op_messages_roundtrip_json() {
    let a = base_doc();
    let mut b = a.clone();
    let id = b.alloc_id();
    b.insert(b.pages[0], Node::rectangle(id, Rect::new(0.0, 0.0, 4.0, 4.0))).unwrap();
    b.styles.insert("s".into(), Style::Fill(vec![Paint::solid(Color::WHITE)]));
    let msg = ClientMsg::Ops { seq: 1, ops: diff(&a, &b) };
    let back: ClientMsg = serde_json::from_str(&serde_json::to_string(&msg).unwrap()).unwrap();
    assert_eq!(msg, back);
    let mut host = a.clone();
    let s = HostSession::new(&mut host);
    let snap = s.snapshot(1);
    let ServerMsg::Snapshot { doc, .. } = serde_json::from_str(&serde_json::to_string(&snap).unwrap()).unwrap() else { panic!() };
    assert_eq!(doc.nodes.len(), a.nodes.len());
}

// ---- history rebase ----

#[test]
fn rebase_keeps_remote_values_when_undoing() {
    let mut d = base_doc();
    let id = d.get(d.pages[0]).unwrap().children[0];
    let mut h = History::default();
    // Local edit changes name and opacity.
    let mut n = d.get(id).unwrap().clone();
    n.name = "mine".into();
    n.opacity = 0.5;
    h.exec(&mut d, Command::Update { node: Box::new(n) }).unwrap();
    // A batch containing an update of the same node.
    let mut n2 = d.get(id).unwrap().clone();
    n2.locked = true;
    h.exec(&mut d, Command::Batch(vec![Command::Update { node: Box::new(n2) }])).unwrap();
    // Someone else changes the name and opacity (applied to the doc as the guest does).
    let op = Op::Patch { id, fields: [("name".to_string(), serde_json::json!("theirs")), ("opacity".to_string(), serde_json::json!(0.25))].into() };
    apply(&mut d, &op).unwrap();
    rebase_history(&mut h, &op);
    h.undo(&mut d).unwrap();
    h.undo(&mut d).unwrap();
    let n = d.get(id).unwrap();
    assert_eq!(n.name, "theirs");
    assert_eq!(n.opacity, 0.25);
    assert!(!n.locked, "own field is still undone");
}

#[test]
fn rebase_updates_restored_subtrees() {
    let mut d = base_doc();
    let id = d.get(d.pages[0]).unwrap().children[0];
    let mut h = History::default();
    h.exec(&mut d, Command::Remove { id }).unwrap();
    let op = Op::Patch { id, fields: [("name".to_string(), serde_json::json!("theirs"))].into() };
    rebase_history(&mut h, &op);
    h.undo(&mut d).unwrap();
    assert_eq!(d.get(id).unwrap().name, "theirs");
}

// ---- host ----

fn session() -> (Document, HostSession) {
    let mut host_doc = base_doc();
    let host = HostSession::new(&mut host_doc);
    (host_doc, host)
}

fn join(host: &mut HostSession, role: Role) -> (PeerId, GuestSession, Document) {
    let peer = host.add_peer(role);
    let (g, d, _) = GuestSession::join(host.snapshot(peer)).unwrap();
    (peer, g, d)
}

#[test]
fn host_assigns_seq_acks_and_broadcasts() {
    let (mut hd, mut host) = session();
    let (p1, mut g1, mut d1) = join(&mut host, Role::Editor);
    let (_p2, mut g2, mut d2) = join(&mut host, Role::Editor);
    assert_eq!(d1.id_peer(), Some(1));
    let id = d1.alloc_id();
    assert_eq!(id.0 >> 40, 1);
    d1.insert(d1.pages[0], Node::rectangle(id, Rect::new(0.0, 0.0, 5.0, 5.0))).unwrap();
    let m = g1.local_changes(&d1).unwrap();
    let outs = host.receive(&mut hd, p1, m);
    assert!(hd.get(id).is_some());
    assert_eq!(outs.len(), 2);
    assert_eq!(outs[0].to, Dest::AllBut(p1));
    assert_eq!(outs[1].to, Dest::Peer(p1));
    assert!(matches!(outs[1].msg, ServerMsg::Ack { client_seq: 1, seq: 1, .. }));
    for o in outs {
        match o.to {
            Dest::AllBut(_) => {
                g2.receive(&mut d2, o.msg);
            }
            _ => {
                g1.receive(&mut d1, o.msg);
            }
        }
    }
    assert_same(&hd, &d2, "g2");
    assert_same(&hd, &d1, "g1");
    assert!(!g1.has_pending());
    assert_eq!(host.seq(), 1);
    assert_eq!(g2.seq(), 1);
}

#[test]
fn host_local_edits_are_numbered_and_snapshot_matches_seq() {
    let (mut hd, mut host) = session();
    let id = hd.alloc_id();
    assert_eq!(id.0 >> 40, 0);
    hd.insert(hd.pages[0], Node::rectangle(id, Rect::new(0.0, 0.0, 5.0, 5.0))).unwrap();
    // Unnumbered host edits are not in the snapshot (they arrive as later ops).
    let (peer, mut g, mut gd) = join(&mut host, Role::Editor);
    assert!(gd.get(id).is_none());
    let outs = host.local_changes(&hd);
    assert_eq!(outs.len(), 1);
    g.receive(&mut gd, outs.into_iter().next().unwrap().msg);
    assert_same(&hd, &gd, "host edit");
    assert!(host.local_changes(&hd).is_empty());
    let _ = peer;
}

#[test]
fn host_rejects_bad_structure_with_correction() {
    let (mut hd, mut host) = session();
    let (p1, mut g1, mut d1) = join(&mut host, Role::Editor);
    let page = d1.pages[0];
    let (x, y) = (d1.get(page).unwrap().children[0], d1.get(page).unwrap().children[1]);
    // Host nests y in x; the guest concurrently nests x in y.
    hd.reparent(y, x, 0).unwrap();
    d1.reparent(x, y, 0).unwrap();
    let m = g1.local_changes(&d1).unwrap();
    let host_outs = host.local_changes(&hd);
    let outs = host.receive(&mut hd, p1, m);
    assert!(hd.is_ancestor(x, y), "host order wins");
    let ack = outs.iter().find_map(|o| if let ServerMsg::Ack { rejected, .. } = &o.msg { Some(rejected.clone()) } else { None }).unwrap();
    assert_eq!(ack, vec![0]);
    assert!(outs.iter().any(|o| matches!(o.msg, ServerMsg::Correction { .. })));
    for o in host_outs.into_iter().chain(outs) {
        if matches!(o.to, Dest::All | Dest::Peer(_)) {
            g1.receive(&mut d1, o.msg);
        }
    }
    assert_same(&hd, &d1, "after correction");
}

#[test]
fn host_rejects_foreign_id_range_and_viewer_edits() {
    let (mut hd, mut host) = session();
    let (p1, mut g1, mut d1) = join(&mut host, Role::Editor);
    let (pv, mut gv, mut dv) = join(&mut host, Role::Viewer);
    // A guest that hands out ids in someone else's range is refused.
    d1.set_id_peer(5);
    let id = d1.alloc_id();
    d1.insert(d1.pages[0], Node::rectangle(id, Rect::new(0.0, 0.0, 1.0, 1.0))).unwrap();
    let m = g1.local_changes(&d1).unwrap();
    let outs = host.receive(&mut hd, p1, m);
    assert!(hd.get(id).is_none());
    for o in outs {
        g1.receive(&mut d1, o.msg);
    }
    assert!(d1.get(id).is_none(), "corrected away");
    // A viewer's edits (including a delete of a subtree) are dropped and undone.
    let frame = dv.get(dv.pages[0]).unwrap().children[0];
    dv.get_mut(frame).unwrap().name = "hax".into();
    dv.styles.insert("s".into(), Style::Fill(vec![]));
    let sub = dv.alloc_id();
    dv.insert(frame, Node::rectangle(sub, Rect::new(0.0, 0.0, 1.0, 1.0))).unwrap();
    let m = gv.local_changes(&dv).unwrap();
    let outs = host.receive(&mut hd, pv, m);
    assert_ne!(hd.get(frame).unwrap().name, "hax");
    assert!(hd.styles.is_empty() && hd.get(sub).is_none());
    assert!(outs.iter().all(|o| o.to == Dest::Peer(pv)));
    for o in outs {
        gv.receive(&mut dv, o.msg);
    }
    assert_same(&hd, &dv, "viewer corrected");
    assert_eq!(host.seq(), 0);
}

#[test]
fn images_are_fetched_by_hash() {
    let (mut hd, mut host) = session();
    hd.images.insert("h1".into(), vec![9, 9]);
    let mut host = {
        let _ = &mut host;
        HostSession::new(&mut hd)
    };
    let (p1, mut g1, mut d1) = join(&mut host, Role::Editor);
    let (_p2, mut g2, mut d2) = join(&mut host, Role::Editor);
    let ServerMsg::Snapshot { doc, .. } = host.snapshot(p1) else { panic!() };
    assert_eq!(doc.images.get("h1"), Some(&Vec::new()));
    let (_, d, reqs) = GuestSession::join(host.snapshot(p1)).unwrap();
    assert_eq!(reqs, vec![ClientMsg::ImageRequest { hash: "h1".into() }]);
    let _ = d;
    // Guest 1 adds an image: host asks for the bytes, then tells guest 2.
    d1.images.insert("h2".into(), vec![7]);
    let m = g1.local_changes(&d1).unwrap();
    let outs = host.receive(&mut hd, p1, m);
    let mut to_g1 = Vec::new();
    for o in outs {
        assert_eq!(o.to, Dest::Peer(p1));
        to_g1.push(o.msg);
    }
    let mut up = Vec::new();
    for m in to_g1 {
        up.extend(g1.receive(&mut d1, m).out);
    }
    let ask = up.iter().filter(|m| matches!(m, ClientMsg::ImageData { .. })).count();
    assert_eq!(ask, 1);
    let mut broadcast = Vec::new();
    for m in up {
        broadcast.extend(host.receive(&mut hd, p1, m));
    }
    assert_eq!(hd.images.get("h2"), Some(&vec![7]));
    let o = broadcast.into_iter().find(|o| o.to == Dest::AllBut(p1)).unwrap();
    let r = g2.receive(&mut d2, o.msg);
    assert_eq!(r.out, vec![ClientMsg::ImageRequest { hash: "h2".into() }]);
    let back = host.receive(&mut hd, _p2, r.out.into_iter().next().unwrap());
    g2.receive(&mut d2, back.into_iter().next().unwrap().msg);
    assert_eq!(d2.images.get("h2"), Some(&vec![7]));
    assert!(g2.local_changes(&d2).is_none(), "fetched bytes are not local changes");
}

// ---- guest ----

/// Delivers everything, FIFO per link, until quiet.
struct Net {
    hd: Document,
    host: HostSession,
    guests: Vec<(PeerId, GuestSession, Document)>,
    up: Vec<VecDeque<ClientMsg>>,
    down: Vec<VecDeque<ServerMsg>>,
    /// Guests whose connection is up; messages to the others are lost.
    online: Vec<bool>,
}

impl Net {
    fn new(n: usize) -> Self {
        let (hd, mut host) = session();
        let mut guests = Vec::new();
        for _ in 0..n {
            let (p, mut g, mut d) = join(&mut host, Role::Editor);
            g.receive(&mut d, host.resume_msg(p).unwrap());
            guests.push((p, g, d));
        }
        Self { hd, host, up: vec![VecDeque::new(); n], down: vec![VecDeque::new(); n], online: vec![true; n], guests }
    }

    fn route(&mut self, outs: Vec<Out>) {
        for o in outs {
            for (i, (p, _, _)) in self.guests.iter().enumerate() {
                let hit = match o.to {
                    Dest::Peer(x) => x == *p,
                    Dest::AllBut(x) => x != *p,
                    Dest::All => true,
                };
                if hit && self.online[i] {
                    self.down[i].push_back(o.msg.clone());
                }
            }
        }
    }

    fn sync_guest(&mut self, i: usize) {
        if !self.online[i] {
            return;
        }
        let (_, g, d) = &mut self.guests[i];
        if let Some(m) = g.local_changes(d) {
            self.up[i].push_back(m);
        }
    }

    fn sync_host(&mut self) {
        let outs = self.host.local_changes(&self.hd);
        self.route(outs);
    }

    /// The connection of guest `i` drops: what is in flight either way is lost.
    fn cut(&mut self, i: usize) {
        self.online[i] = false;
        self.up[i].clear();
        self.down[i].clear();
        self.host.remove_peer(self.guests[i].0);
    }

    /// Guest `i` reconnects with its secret and the host sends a fresh snapshot.
    fn reconnect(&mut self, i: usize) {
        let token = self.guests[i].1.token().expect("token").to_string();
        self.sync_host();
        let peer = self.host.resume(&token, Role::Editor).expect("known token");
        assert_eq!(peer, self.guests[i].0, "same peer index");
        let (_, g, d) = &mut self.guests[i];
        let r = g.rejoin(d, self.host.snapshot(peer)).expect("snapshot for the same peer");
        self.online[i] = true;
        self.up[i].extend(r.out);
        self.down[i].push_back(self.host.resume_msg(peer).unwrap());
    }

    fn deliver_up(&mut self, i: usize) -> bool {
        let Some(m) = self.up[i].pop_front() else { return false };
        let p = self.guests[i].0;
        let outs = self.host.receive(&mut self.hd, p, m);
        self.route(outs);
        true
    }

    fn deliver_down(&mut self, i: usize) -> bool {
        let Some(m) = self.down[i].pop_front() else { return false };
        let (_, g, d) = &mut self.guests[i];
        let r = g.receive(d, m);
        self.up[i].extend(r.out);
        true
    }

    fn drain(&mut self, rng: &mut Rng) {
        loop {
            self.sync_host();
            for i in 0..self.guests.len() {
                self.sync_guest(i);
            }
            let n = self.guests.len();
            let mut links: Vec<(bool, usize)> = Vec::new();
            for i in 0..n {
                if !self.up[i].is_empty() {
                    links.push((true, i));
                }
                if !self.down[i].is_empty() {
                    links.push((false, i));
                }
            }
            let Some((up, i)) = rng.pick(&links) else { break };
            if up {
                self.deliver_up(i);
            } else {
                self.deliver_down(i);
            }
        }
    }

    fn assert_converged(&self, ctx: &str) {
        for (p, g, d) in &self.guests {
            assert_same(&self.hd, d, &format!("{ctx}: guest {p}"));
            assert!(!g.has_pending(), "{ctx}: guest {p} still pending");
        }
    }
}

#[test]
fn concurrent_edits_to_different_fields_both_survive() {
    let mut net = Net::new(2);
    let id = net.hd.get(net.hd.pages[0]).unwrap().children[0];
    net.guests[0].2.get_mut(id).unwrap().opacity = 0.3;
    net.guests[1].2.get_mut(id).unwrap().name = "renamed".into();
    net.hd.get_mut(id).unwrap().locked = true;
    for i in 0..2 {
        net.sync_guest(i);
    }
    net.sync_host();
    let mut rng = Rng::new(1);
    net.drain(&mut rng);
    net.assert_converged("fields");
    let n = net.hd.get(id).unwrap();
    assert_eq!((n.opacity, n.name.as_str(), n.locked), (0.3, "renamed", true));
}

#[test]
fn same_field_last_in_host_order_wins_without_flicker() {
    let mut net = Net::new(2);
    let id = net.hd.get(net.hd.pages[0]).unwrap().children[0];
    net.guests[0].2.get_mut(id).unwrap().name = "A".into();
    net.guests[1].2.get_mut(id).unwrap().name = "B".into();
    net.sync_guest(0);
    net.sync_guest(1);
    // Host gets A first, then B; guest 0 sees B's change only after its own ack.
    net.deliver_up(0);
    net.deliver_up(1);
    // Guest 0 receives B (ordered after A) -> applies, since A is acked first.
    net.deliver_down(0);
    net.deliver_down(0);
    assert_eq!(net.guests[0].2.get(id).unwrap().name, "B");
    // Guest 1 receives A while its own B is pending: held back.
    net.deliver_down(1);
    assert_eq!(net.guests[1].2.get(id).unwrap().name, "B");
    let mut rng = Rng::new(2);
    net.drain(&mut rng);
    net.assert_converged("same field");
    assert_eq!(net.hd.get(id).unwrap().name, "B");
}

#[test]
fn concurrent_inserts_into_same_frame_both_survive() {
    for seed in 0..20 {
        let mut net = Net::new(3);
        let frame = net.hd.get(net.hd.pages[0]).unwrap().children[0];
        let mut ids = Vec::new();
        for i in 0..3 {
            let d = &mut net.guests[i].2;
            let id = d.alloc_id();
            d.insert(frame, Node::rectangle(id, Rect::new(i as f32, 0.0, 1.0, 1.0))).unwrap();
            ids.push(id);
            net.sync_guest(i);
        }
        let hid = net.hd.alloc_id();
        net.hd.insert(frame, Node::rectangle(hid, Rect::new(9.0, 0.0, 1.0, 1.0))).unwrap();
        ids.push(hid);
        net.sync_host();
        net.drain(&mut Rng::new(seed));
        net.assert_converged("inserts");
        let kids = &net.hd.get(frame).unwrap().children;
        for id in ids {
            assert!(kids.contains(&id), "seed {seed}: {id:?} lost");
        }
    }
}

#[test]
fn remote_changes_are_not_resent_as_local() {
    let mut net = Net::new(2);
    let id = net.hd.get(net.hd.pages[0]).unwrap().children[0];
    net.hd.get_mut(id).unwrap().opacity = 0.1;
    net.sync_host();
    net.deliver_down(0);
    let (_, g, d) = &mut net.guests[0];
    assert!(g.local_changes(d).is_none());
}

#[test]
fn delete_wins_over_concurrent_edit_and_insert() {
    let mut net = Net::new(2);
    let page = net.hd.pages[0];
    let frame = net.hd.get(page).unwrap().children[0];
    net.guests[0].2.remove(frame).unwrap();
    {
        let d = &mut net.guests[1].2;
        d.get_mut(frame).unwrap().name = "edit".into();
        let id = d.alloc_id();
        d.insert(frame, Node::rectangle(id, Rect::new(0.0, 0.0, 1.0, 1.0))).unwrap();
    }
    net.sync_guest(0);
    net.sync_guest(1);
    net.drain(&mut Rng::new(3));
    net.assert_converged("delete wins");
    assert!(net.hd.get(frame).is_none());
    assert_eq!(net.hd.nodes.len(), 1 + 2);
}

// ---- simulation ----

fn run_sim(seed: u64, guests: usize, steps: usize) {
    let mut rng = Rng::new(seed);
    let mut net = Net::new(guests);
    for step in 0..steps {
        match rng.below(10) {
            0..=3 => {
                let who = rng.below(guests + 1);
                for _ in 0..1 + rng.below(3) {
                    if who == guests {
                        random_edit(&mut rng, &mut net.hd);
                    } else {
                        random_edit(&mut rng, &mut net.guests[who].2);
                    }
                }
                // Sometimes leave the edit unsynced so the next receive has to capture it.
                if rng.chance(75) {
                    if who == guests {
                        net.sync_host();
                    } else {
                        net.sync_guest(who);
                    }
                }
            }
            4..=6 => {
                let i = rng.below(guests);
                net.deliver_down(i);
            }
            _ => {
                let i = rng.below(guests);
                net.deliver_up(i);
            }
        }
        if step % 97 == 0 {
            net.drain(&mut rng);
            net.assert_converged(&format!("seed {seed} step {step}"));
        }
    }
    net.drain(&mut rng);
    net.assert_converged(&format!("seed {seed} end"));
}

#[test]
fn simulation_converges_three_guests() {
    for seed in 0..60 {
        run_sim(seed, 3, 250);
    }
}

#[test]
fn simulation_converges_many_guests() {
    for seed in 1000..1020 {
        run_sim(seed, 5, 400);
    }
    for seed in 2000..2030 {
        run_sim(seed, 1, 200);
    }
}

// ---- resume after a lost connection ----

#[test]
fn a_rejoining_guest_keeps_its_peer_and_its_unconfirmed_edits() {
    let mut net = Net::new(2);
    let page = net.hd.pages[0];
    let kids = net.hd.get(page).unwrap().children.clone();
    let mut rng = Rng::new(7);
    // acknowledged before the outage
    net.guests[0].2.get_mut(kids[0]).unwrap().name = "before".into();
    net.drain(&mut rng);
    let token = net.guests[0].1.token().unwrap().to_string();
    assert_eq!(token.len(), 64);
    // sent but never acknowledged, and not even sent
    net.guests[0].2.get_mut(kids[1]).unwrap().opacity = 0.25;
    net.sync_guest(0);
    net.cut(0);
    let made = net.guests[0].2.alloc_id();
    assert_eq!(made.0 >> PEER_ID_BITS, u64::from(net.guests[0].0), "ids stay in the guest's range");
    net.guests[0].2.insert(kids[2], Node::rectangle(made, Rect::new(1.0, 2.0, 3.0, 4.0))).unwrap();
    // meanwhile the host and the other guest edit, including a field the guest also wrote
    net.hd.get_mut(kids[1]).unwrap().name = "host renamed".into();
    net.hd.get_mut(kids[1]).unwrap().opacity = 0.9;
    net.guests[1].2.get_mut(kids[2]).unwrap().locked = true;
    net.sync_host();
    net.sync_guest(1);
    net.drain(&mut rng);
    net.reconnect(0);
    net.drain(&mut rng);
    net.assert_converged("rejoin");
    let n = net.hd.get(kids[1]).unwrap();
    assert_eq!((n.name.as_str(), n.opacity), ("host renamed", 0.25), "the other fields come from the host, the guest's own write wins");
    assert!(net.hd.get(made).is_some(), "a node made offline reached the host");
    assert!(net.hd.get(kids[2]).unwrap().locked);
    assert_eq!(net.hd.get(kids[0]).unwrap().name, "before");
    // the guest keeps working with fresh ids
    let next = net.guests[0].2.alloc_id();
    assert!(next > made);
}

#[test]
fn an_unknown_secret_or_another_peers_snapshot_does_not_resume() {
    let mut net = Net::new(2);
    assert!(net.host.resume("nope", Role::Editor).is_none());
    let token = net.guests[0].1.token().unwrap().to_string();
    net.host.forget_peer(net.guests[0].0);
    assert!(net.host.resume(&token, Role::Editor).is_none(), "a kicked guest cannot come back");
    // a snapshot for someone else is refused by the guest
    let other = net.guests[1].0;
    let snap = net.host.snapshot(other);
    let (_, g, d) = &mut net.guests[0];
    assert!(g.rejoin(d, snap).is_none());
}

#[test]
fn a_departed_peer_stays_reserved_and_a_second_connection_replaces_the_first() {
    let mut net = Net::new(1);
    let p = net.guests[0].0;
    let token = net.guests[0].1.token().unwrap().to_string();
    net.host.remove_peer(p);
    let fresh = net.host.add_peer(Role::Editor);
    assert_ne!(fresh, p, "the index is not handed out again");
    // the old connection was never noticed as dead: resuming still works
    assert_eq!(net.host.resume(&token, Role::Viewer), Some(p));
    assert_eq!(net.host.resume(&token, Role::Editor), Some(p));
}

fn run_outage_sim(seed: u64, guests: usize, steps: usize) {
    let mut rng = Rng::new(seed);
    let mut net = Net::new(guests);
    for step in 0..steps {
        match rng.below(12) {
            0..=3 => {
                let who = rng.below(guests + 1);
                for _ in 0..1 + rng.below(3) {
                    if who == guests {
                        random_edit(&mut rng, &mut net.hd);
                    } else {
                        random_edit(&mut rng, &mut net.guests[who].2);
                    }
                }
                if rng.chance(75) {
                    if who == guests {
                        net.sync_host();
                    } else {
                        net.sync_guest(who);
                    }
                }
            }
            4..=5 => {
                let i = rng.below(guests);
                if net.online[i] {
                    net.deliver_down(i);
                }
            }
            6..=7 => {
                let i = rng.below(guests);
                if net.online[i] {
                    net.deliver_up(i);
                }
            }
            8 => {
                let i = rng.below(guests);
                if net.online[i] {
                    net.cut(i);
                }
            }
            _ => {
                let i = rng.below(guests);
                if !net.online[i] && rng.chance(40) {
                    net.reconnect(i);
                }
            }
        }
        if step % 97 == 0 && net.online.iter().all(|o| *o) {
            net.drain(&mut rng);
            net.assert_converged(&format!("seed {seed} step {step}"));
        }
    }
    for i in 0..guests {
        if !net.online[i] {
            net.reconnect(i);
        }
    }
    net.drain(&mut rng);
    net.assert_converged(&format!("seed {seed} end"));
}

#[test]
fn simulation_converges_across_outages() {
    for seed in 0..60 {
        run_outage_sim(seed, 3, 300);
    }
    for seed in 3000..3030 {
        run_outage_sim(seed, 1, 200);
    }
}

#[test]
fn image_messages_are_base64_and_much_smaller_than_number_arrays() {
    let bytes: Vec<u8> = (0..30_000u32).map(|i| (i.wrapping_mul(2654435761) >> 13) as u8).collect();
    let msg = ServerMsg::ImageData { hash: "h".into(), bytes: bytes.clone() };
    let json = serde_json::to_string(&msg).unwrap();
    let as_array = serde_json::json!({ "ImageData": { "hash": "h", "bytes": bytes } }).to_string();
    assert!(json.len() < bytes.len() * 4 / 3 + 64, "{} bytes of JSON for {}", json.len(), bytes.len());
    assert!(as_array.len() > json.len() * 2, "array form {} against base64 {}", as_array.len(), json.len());
    let ServerMsg::ImageData { bytes: back, .. } = serde_json::from_str(&json).unwrap() else { panic!() };
    assert_eq!(back, bytes);
    let up = ClientMsg::ImageData { hash: "h".into(), bytes: bytes.clone() };
    assert_eq!(serde_json::from_str::<ClientMsg>(&serde_json::to_string(&up).unwrap()).unwrap(), up);
    assert!(serde_json::from_str::<ClientMsg>(r#"{"ImageData":{"hash":"h","bytes":"***"}}"#).is_err());
}
