//! The server logic in-process on `Network::Direct`: guests join through desigma-net, edit, and
//! the document, the autosave and the link survive a restart.

use desigma_collab::{ClientMsg, GuestSession, Role, ServerMsg};
use desigma_doc::{Document, Node, NodeId, Rect};
use desigma_net::{DisconnectReason, Guest, GuestEvent, GuestOptions, JoinError, Network, Ticket};
use desigma_server::{Config, Server};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

const T: Duration = Duration::from_secs(20);

// The app's wire enums (desigma-app/src/collab/mod.rs); the JSON must stay identical.
#[derive(Serialize, Deserialize)]
enum ToGuest {
    Sync(ServerMsg),
    Role(Role),
}
#[derive(Serialize, Deserialize)]
enum ToHost {
    Sync(ClientMsg),
}

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("desigma-server-it-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn free_port() -> u16 {
    std::net::UdpSocket::bind("0.0.0.0:0").unwrap().local_addr().unwrap().port()
}

fn seed(path: &Path) -> NodeId {
    let mut d = Document::new("Shared");
    let page = d.add_page("Page 1");
    let a = d.alloc_id();
    d.insert(page, Node::rectangle(a, Rect::new(0.0, 0.0, 100.0, 80.0))).unwrap();
    std::fs::write(path, d.to_json().unwrap()).unwrap();
    a
}

fn config(file: &Path, port: u16) -> Config {
    let mut c = Config::new(file);
    c.password = Some("pw".into());
    c.network = Network::Direct;
    c.port = Some(port);
    c.autosave = Duration::from_millis(200);
    c.link_file = Some(file.with_extension("link"));
    c
}

struct Running {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<Server>,
}

impl Running {
    fn start(mut server: Server) -> Running {
        let stop = Arc::new(AtomicBool::new(false));
        let s = stop.clone();
        Running {
            stop,
            thread: std::thread::spawn(move || {
                server.run(&s);
                server
            }),
        }
    }
    fn stop(self) -> Server {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.join().unwrap()
    }
}

fn join(link: &str, name: &str, pw: Option<&str>) -> Result<Guest, JoinError> {
    let ticket = Ticket::parse(link).unwrap();
    Guest::join_blocking(&ticket, name, pw, GuestOptions { network: Network::Direct, ..Default::default() }, T).map(|(g, _)| g)
}

fn send(g: &Guest, m: ClientMsg) {
    g.send(serde_json::to_vec(&ToHost::Sync(m)).unwrap()).unwrap();
}

/// Waits for the snapshot and starts a guest session on it. Presence seen meanwhile comes along.
fn open(g: &Guest) -> (GuestSession, Document, Role, Vec<serde_json::Value>) {
    let end = Instant::now() + T;
    let mut role = Role::Viewer;
    let mut presence = Vec::new();
    loop {
        let left = end.checked_duration_since(Instant::now()).expect("no snapshot");
        match g.recv_timeout(left) {
            Some(GuestEvent::Presence(b)) => presence.push(serde_json::from_slice(&b).unwrap()),
            Some(GuestEvent::Message(b)) => match serde_json::from_slice::<ToGuest>(&b).unwrap() {
                ToGuest::Role(r) => role = r,
                ToGuest::Sync(s @ ServerMsg::Snapshot { .. }) => {
                    let (sess, doc, reqs) = GuestSession::join(s).unwrap();
                    assert!(reqs.is_empty());
                    return (sess, doc, role, presence);
                }
                ToGuest::Sync(_) => {}
            },
            _ => {}
        }
    }
}

/// Sends the guest's edits and pumps the host's answers until they are acknowledged.
fn commit(g: &Guest, sess: &mut GuestSession, doc: &mut Document) {
    if let Some(m) = sess.local_changes(doc) {
        send(g, m);
    }
    let end = Instant::now() + T;
    while sess.has_pending() {
        let left = end.checked_duration_since(Instant::now()).expect("edit never acknowledged");
        if let Some(GuestEvent::Message(b)) = g.recv_timeout(left) {
            if let ToGuest::Sync(m) = serde_json::from_slice(&b).unwrap() {
                for out in sess.receive(doc, m).out {
                    send(g, out);
                }
            }
        }
    }
}

fn wait_for<R>(what: &str, mut f: impl FnMut() -> Option<R>) -> R {
    let end = Instant::now() + T;
    loop {
        if let Some(r) = f() {
            return r;
        }
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn names(doc: &Document) -> Vec<String> {
    let mut v: Vec<String> = doc.nodes.values().map(|n| n.name.clone()).collect();
    v.sort();
    v
}

#[test]
fn edits_autosave_and_the_link_survive_a_restart() {
    let d = dir("restart");
    let file = d.join("shared.desigma");
    let a = seed(&file);
    let port = free_port();

    // --- first run
    let server = Server::start(config(&file, port)).unwrap();
    let link1 = server.link().to_string();
    assert!(link1.starts_with("desigma://join/"));
    assert_eq!(std::fs::read_to_string(file.with_extension("link")).unwrap().trim(), link1);
    let key_file = d.join(".shared.desigma-key");
    assert!(key_file.exists(), "key file next to the document");
    let running = Running::start(server);

    assert_eq!(join(&link1, "Eve", Some("wrong")).err(), Some(JoinError::WrongPassword));
    assert_eq!(join(&link1, "Eve", None).err(), Some(JoinError::WrongPassword));

    let ana = join(&link1, "Ana", Some("pw")).unwrap();
    let (mut sess, mut doc, role, _) = open(&ana);
    assert_eq!(role, Role::Editor);
    let page = doc.pages[0];
    doc.nodes.get_mut(&a).unwrap().name = "Renamed by Ana".into();
    let b = doc.alloc_id();
    assert_eq!(b.0 >> 40, u64::from(sess.peer()), "guest ids come from its own range");
    let mut rect = Node::rectangle(b, Rect::new(10.0, 10.0, 20.0, 20.0));
    rect.name = "Added by Ana".into();
    doc.insert(page, rect).unwrap();
    commit(&ana, &mut sess, &mut doc);

    // Presence goes out stamped with the sender, is replayed to a newcomer, and "gone" follows a leave.
    let hello = serde_json::json!({ "peer": 99, "name": "Ana", "page": page.0, "cursor": [1.0, 2.0], "sel": [], "view": [0.0, 0.0, 1.0] });
    ana.send_presence(hello.to_string().as_bytes()).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let bea = join(&link1, "Bea", Some("pw")).unwrap();
    let (_, bdoc, _, mut early) = open(&bea);
    let ana_peer = Some(u64::from(sess.peer()));
    // The replay follows the snapshot, so it may still be on its way.
    let p = wait_for("Ana's presence to be replayed", || {
        if let Some(GuestEvent::Presence(b)) = bea.recv_timeout(Duration::from_millis(50)) {
            early.push(serde_json::from_slice(&b).unwrap());
        }
        early.iter().find(|p| p["peer"].as_u64() == ana_peer).cloned()
    });
    assert_eq!(p["name"].as_str(), Some("Ana"));
    assert!(!p["gone"].as_bool().unwrap_or(false));
    // Bea's snapshot already has Ana's edits.
    assert!(names(&bdoc).contains(&"Added by Ana".to_string()));
    ana.leave();
    wait_for("gone", || match bea.recv_timeout(Duration::from_millis(100)) {
        Some(GuestEvent::Presence(b)) => {
            let v: serde_json::Value = serde_json::from_slice(&b).unwrap();
            (v["gone"].as_bool() == Some(true) && v["peer"].as_u64() == ana_peer).then_some(())
        }
        _ => None,
    });
    drop(bea);

    // The autosave writes the edits without anybody saving.
    wait_for("autosave", || {
        let doc = Document::from_json(&std::fs::read_to_string(&file).ok()?).ok()?;
        names(&doc).contains(&"Added by Ana".to_string()).then_some(())
    });
    let server = running.stop();
    assert!(names(server.document()).contains(&"Renamed by Ana".to_string()));
    server.shutdown().unwrap();
    assert!(!d.join("shared.desigma.tmp").exists(), "temp file is renamed away");
    let saved = Document::from_json(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(saved.get(a).unwrap().name, "Renamed by Ana");
    assert!(d.join("shared.desigma.bak.1").exists(), "previous version kept as a backup");

    // --- second run: same key file, same port
    let server = Server::start(config(&file, port)).unwrap();
    assert_eq!(server.link(), link1, "the link is stable across restarts");
    assert_eq!(names(server.document()), names(&saved));
    let running = Running::start(server);
    let cy = join(&link1, "Cy", Some("pw")).unwrap();
    let (_, cdoc, ..) = open(&cy);
    assert_eq!(cdoc.get(a).unwrap().name, "Renamed by Ana");
    assert!(names(&cdoc).contains(&"Added by Ana".to_string()));

    // Stopping the server tells guests the host ended.
    let server = running.stop();
    server.shutdown().unwrap();
    wait_for("end of session", || match cy.recv_timeout(Duration::from_millis(100)) {
        Some(GuestEvent::Disconnected(DisconnectReason::HostEnded)) => Some(()),
        _ => None,
    });
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn view_only_guests_cannot_change_the_document() {
    let d = dir("viewonly");
    let file = d.join("v.desigma");
    let a = seed(&file);
    let mut cfg = config(&file, free_port());
    cfg.view_only = true;
    cfg.password = None;
    let server = Server::start(cfg).unwrap();
    let link = server.link().to_string();
    let running = Running::start(server);
    let g = join(&link, "Vi", None).unwrap();
    let (mut sess, mut doc, role, _) = open(&g);
    assert_eq!(role, Role::Viewer);
    doc.nodes.get_mut(&a).unwrap().name = "Nope".into();
    commit(&g, &mut sess, &mut doc);
    let server = running.stop();
    assert_ne!(server.document().get(a).unwrap().name, "Nope");
    server.shutdown().unwrap();
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_figma_import_is_saved_as_desigma_and_then_wins() {
    let d = dir("import");
    let mut doc = Document::new("From json");
    doc.add_page("Page 1");
    // A native document in a .json file is accepted by the importer too.
    std::fs::write(d.join("in.json"), doc.to_json().unwrap()).unwrap();
    let mut cfg = config(&d.join("in.json"), free_port());
    cfg.password = None;
    let server = Server::start(cfg.clone()).unwrap();
    assert_eq!(server.path(), d.join("in.desigma"));
    assert!(d.join("in.desigma").exists());
    server.shutdown().unwrap();
    // Restarting with the same command serves the saved .desigma, not the source again.
    std::fs::remove_file(d.join("in.json")).unwrap();
    let server = Server::start(cfg).unwrap();
    assert_eq!(server.path(), d.join("in.desigma"));
    server.shutdown().unwrap();
    let _ = std::fs::remove_dir_all(d);
}
