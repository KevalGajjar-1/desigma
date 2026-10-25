//! Headless host for live collaboration: keeps one `.desigma` file shared permanently.
//!
//! It speaks the same protocol as a desktop host (`desigma-collab` sync core over `desigma-net`),
//! so the desktop app joins it exactly as it joins another desktop. The server is the authority:
//! it applies guests' ops to its document, autosaves, and relays presence. It has no cursor of its
//! own and nobody to ask for approval, so everyone who passes the password is let in.
//!
//! The wire shapes (`ToGuest` / `ToHost`) and the presence relay mirror
//! `desigma-app/src/collab/mod.rs`; that code is tied to the app's state (toasts, undo history,
//! remote cursors), so the small routing loop is repeated here instead of shared.

mod key;
mod store;

use desigma_collab::{ClientMsg, Dest, HostSession, Out, PeerId, Role, ServerMsg};
use desigma_doc::Document;
use desigma_net::{Host, HostEvent, HostOptions, Network};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub use key::load_or_create_key;
pub use store::{load_document, save_atomic};

/// Host to guest, over the reliable lane (same JSON as the app's `ToGuest`).
#[derive(Serialize, Deserialize)]
enum ToGuest {
    Sync(ServerMsg),
    Role(Role),
}

/// Guest to host (same JSON as the app's `ToHost`).
#[derive(Serialize, Deserialize)]
enum ToHost {
    Sync(ClientMsg),
}

#[derive(Clone, Debug)]
pub struct Config {
    /// Document to serve: `.desigma`, or a `.fig` / REST `.json` / `.svg` that is imported and
    /// saved next to it as `.desigma`.
    pub file: PathBuf,
    pub password: Option<String>,
    /// Guests may look but not edit.
    pub view_only: bool,
    /// Label in the log.
    pub name: String,
    /// Secret key file; `None` means `.<stem>.desigma-key` next to the document.
    pub key_file: Option<PathBuf>,
    pub network: Network,
    /// Fixed UDP port (open it in the firewall); random when `None`.
    pub port: Option<u16>,
    pub autosave: Duration,
    /// Rotating copies of the previous file kept (`<file>.bak.1` is the newest).
    pub backups: usize,
    /// A new backup is taken at most this often.
    pub backup_every: Duration,
    /// Write the join link here.
    pub link_file: Option<PathBuf>,
}

impl Config {
    pub fn new(file: impl Into<PathBuf>) -> Self {
        Self {
            file: file.into(),
            password: None,
            view_only: false,
            name: "Server".into(),
            key_file: None,
            network: Network::Internet,
            port: None,
            autosave: Duration::from_secs(10),
            backups: 3,
            backup_every: Duration::from_secs(600),
            link_file: None,
        }
    }
}

struct Guest {
    /// The transport's number for this guest.
    net: u32,
    name: String,
}

pub struct Server {
    cfg: Config,
    host: Host,
    sess: HostSession,
    doc: Document,
    doc_path: PathBuf,
    link: String,
    /// Guests by session peer index.
    peers: BTreeMap<PeerId, Guest>,
    /// Last presence of each guest, replayed to newcomers.
    presence: BTreeMap<PeerId, Vec<u8>>,
    dirty: bool,
    last_save: Instant,
}

impl Server {
    /// Loads the document, starts listening and returns once the join link is known.
    pub fn start(cfg: Config) -> Result<Server, String> {
        let (doc_path, mut doc, note) = load_document(&cfg.file)?;
        log(&cfg.name, &note);
        let key_path = cfg.key_file.clone().unwrap_or_else(|| key::default_key_path(&doc_path));
        let secret_key = load_or_create_key(&key_path)?;
        let host = Host::start(HostOptions {
            password: cfg.password.clone().filter(|p| !p.is_empty()),
            network: cfg.network,
            secret_key: Some(secret_key),
            port: cfg.port,
            ..Default::default()
        })
        .map_err(|e| e.to_string())?;
        let ticket = host.wait_ticket(Duration::from_secs(30)).ok_or("could not start networking (no ticket after 30 s)")?;
        // Direct addresses change between runs; with a relay known the link carries only that.
        let link = ticket.relay_only().to_link();
        if let Some(p) = &cfg.link_file {
            std::fs::write(p, format!("{link}\n")).map_err(|e| format!("cannot write {}: {e}", p.display()))?;
        }
        let sess = HostSession::new(&mut doc);
        let mut s = Server { cfg, host, sess, doc, doc_path, link, peers: BTreeMap::new(), presence: BTreeMap::new(), dirty: false, last_save: Instant::now() };
        // Make sure the file exists (a fresh import or a new document) and is writable.
        s.save()?;
        Ok(s)
    }

    /// The `desigma://join/...` link. Identical across restarts while the key file, the port
    /// (with `--direct`) and the relay stay the same.
    pub fn link(&self) -> &str {
        &self.link
    }

    pub fn document(&self) -> &Document {
        &self.doc
    }

    /// Where the document is saved.
    pub fn path(&self) -> &Path {
        &self.doc_path
    }

    /// Guests connected now.
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    /// Runs until `stop` is set.
    pub fn run(&mut self, stop: &AtomicBool) {
        while !stop.load(Ordering::Relaxed) {
            self.step(Duration::from_millis(100));
        }
    }

    /// Handles queued network events (waiting up to `wait` for the first) and autosaves when due.
    pub fn step(&mut self, wait: Duration) {
        let mut next = self.host.recv_timeout(wait);
        while let Some(ev) = next {
            self.handle(ev);
            next = self.host.try_recv();
        }
        if self.dirty && self.last_save.elapsed() >= self.cfg.autosave {
            if let Err(e) = self.save() {
                self.log(&format!("autosave failed: {e}"));
            }
        }
    }

    /// Writes the document now (temp file, then rename; rotating backups).
    pub fn save(&mut self) -> Result<(), String> {
        let json = self.doc.to_json().map_err(|e| format!("cannot serialize: {e}"))?;
        store::rotate_backups(&self.doc_path, self.cfg.backups, self.cfg.backup_every);
        save_atomic(&self.doc_path, json.as_bytes())?;
        self.dirty = false;
        self.last_save = Instant::now();
        self.log(&format!("saved {}", self.doc_path.display()));
        Ok(())
    }

    /// Saves unsaved edits and ends the session (guests see "host ended").
    pub fn shutdown(mut self) -> Result<(), String> {
        let res = if self.dirty { self.save() } else { Ok(()) };
        self.log("shutting down");
        drop(self.host);
        res
    }

    fn log(&self, msg: &str) {
        log(&self.cfg.name, msg);
    }

    fn role(&self) -> Role {
        if self.cfg.view_only {
            Role::Viewer
        } else {
            Role::Editor
        }
    }

    fn session_peer(&self, net: u32) -> Option<PeerId> {
        self.peers.iter().find(|(_, g)| g.net == net).map(|(c, _)| *c)
    }

    fn handle(&mut self, ev: HostEvent) {
        match ev {
            HostEvent::Ready(_) => {}
            HostEvent::Failed(e) => self.log(&format!("networking failed: {e}")),
            // Approval is never switched on; answer anyway rather than leave a guest hanging.
            HostEvent::JoinRequest { request, .. } => self.host.respond(request, true),
            HostEvent::JoinRefused { name, error, .. } => self.log(&format!("{name} could not join: {error}")),
            HostEvent::PeerJoined { peer, name } => {
                let role = self.role();
                let cp = self.sess.add_peer(role);
                let _ = self.host.send(peer, encode(&ToGuest::Role(role)));
                let _ = self.host.send(peer, encode(&ToGuest::Sync(self.sess.snapshot(cp))));
                // The newcomer sees the guests already here without waiting for their next update.
                for bytes in self.presence.values() {
                    let _ = self.host.send_presence(peer, bytes);
                }
                self.log(&format!("{name} joined ({} online)", self.peers.len() + 1));
                self.peers.insert(cp, Guest { net: peer, name });
            }
            HostEvent::PeerLeft { peer } => {
                let Some(cp) = self.session_peer(peer) else { return };
                let name = self.peers.remove(&cp).map(|g| g.name).unwrap_or_default();
                self.sess.remove_peer(cp);
                self.presence.remove(&cp);
                let gone = serde_json::json!({ "peer": cp, "name": "", "page": 0, "cursor": null, "sel": [], "view": [0.0, 0.0, 1.0], "gone": true });
                self.host.broadcast_presence(&encode(&gone), None);
                self.log(&format!("{name} left ({} online)", self.peers.len()));
            }
            HostEvent::Message { peer, bytes } => {
                let Some(cp) = self.session_peer(peer) else { return };
                let Ok(ToHost::Sync(msg)) = serde_json::from_slice(&bytes) else { return };
                let before = self.sess.seq();
                let outs = self.sess.receive(&mut self.doc, cp, msg);
                self.deliver(outs);
                if self.sess.seq() != before {
                    self.dirty = true;
                }
            }
            HostEvent::Presence { peer, bytes } => {
                let Some(cp) = self.session_peer(peer) else { return };
                // Stamp the sender, as the app host does, and pass the rest through untouched.
                let Ok(serde_json::Value::Object(mut p)) = serde_json::from_slice(&bytes) else { return };
                p.insert("peer".into(), cp.into());
                p.remove("gone");
                let out = encode(&serde_json::Value::Object(p));
                for (other, g) in &self.peers {
                    if *other != cp {
                        let _ = self.host.send_presence(g.net, &out);
                    }
                }
                self.presence.insert(cp, out);
            }
        }
    }

    fn deliver(&self, outs: Vec<Out>) {
        for out in outs {
            let bytes = encode(&ToGuest::Sync(out.msg));
            for (cp, g) in &self.peers {
                let hit = match out.to {
                    Dest::All => true,
                    Dest::AllBut(x) => *cp != x,
                    Dest::Peer(x) => *cp == x,
                };
                if hit {
                    let _ = self.host.send(g.net, bytes.clone());
                }
            }
        }
    }
}

fn encode<T: Serialize>(msg: &T) -> Vec<u8> {
    serde_json::to_vec(msg).unwrap_or_default()
}

/// One log line on stdout: `2026-10-25 12:00:00 UTC [Server] message`.
pub fn log(name: &str, msg: &str) {
    println!("{} [{name}] {msg}", store::utc_now());
}
