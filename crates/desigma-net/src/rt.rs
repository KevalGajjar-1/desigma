//! Plumbing between the tokio runtime and the synchronous UI thread.
use iroh::{endpoint::presets, Endpoint, RelayMode};
use std::{
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc, Mutex, RwLock,
    },
    time::Duration,
};

use crate::{NetError, Network, ALPN};

pub type Wake = Arc<dyn Fn() + Send + Sync>;

/// Producer half: queues an event, then pokes the wake callback.
pub struct Events<T> {
    tx: Sender<T>,
    wake: Arc<RwLock<Option<Wake>>>,
}

impl<T> Clone for Events<T> {
    fn clone(&self) -> Self {
        Self { tx: self.tx.clone(), wake: self.wake.clone() }
    }
}

impl<T> Events<T> {
    pub fn emit(&self, ev: T) {
        if self.tx.send(ev).is_ok() {
            if let Ok(g) = self.wake.read() {
                if let Some(w) = g.as_ref() {
                    w();
                }
            }
        }
    }
}

/// Consumer half, held by the public handle.
pub struct EventQueue<T> {
    rx: Mutex<Receiver<T>>,
    wake: Arc<RwLock<Option<Wake>>>,
}

pub fn channel<T>() -> (Events<T>, EventQueue<T>) {
    let (tx, rx) = mpsc::channel();
    let wake = Arc::new(RwLock::new(None));
    (Events { tx, wake: wake.clone() }, EventQueue { rx: Mutex::new(rx), wake })
}

impl<T> EventQueue<T> {
    pub fn set_wake(&self, f: Box<dyn Fn() + Send + Sync>) {
        if let Ok(mut g) = self.wake.write() {
            *g = Some(Arc::from(f));
        }
    }
    pub fn try_recv(&self) -> Option<T> {
        self.rx.lock().ok()?.try_recv().ok()
    }
    pub fn drain(&self) -> Vec<T> {
        let Ok(rx) = self.rx.lock() else { return Vec::new() };
        std::iter::from_fn(|| rx.try_recv().ok()).collect()
    }
    /// Blocking receive with a timeout (tests, headless tools; not for the UI thread).
    pub fn recv_timeout(&self, d: Duration) -> Option<T> {
        self.rx.lock().ok()?.recv_timeout(d).ok()
    }
}

/// Bind an endpoint for `network`. `Direct` skips relays and discovery.
pub async fn bind(network: Network, accept: bool) -> Result<Endpoint, NetError> {
    let alpns = if accept { vec![ALPN.to_vec()] } else { Vec::new() };
    let b = match network {
        Network::Internet => Endpoint::builder(presets::N0),
        Network::Direct => Endpoint::builder(presets::Minimal).relay_mode(RelayMode::Disabled),
    };
    b.alpns(alpns).bind().await.map_err(|e| NetError::Bind(e.to_string()))
}

/// Datagram size limit of a connection, if datagrams are usable.
pub fn presence_fits(conn: &iroh::endpoint::Connection, len: usize) -> bool {
    conn.max_datagram_size().is_some_and(|m| len <= m)
}
