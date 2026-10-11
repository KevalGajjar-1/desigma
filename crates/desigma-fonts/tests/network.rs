//! Live Fontsource tests. Run with `cargo test -p desigma-fonts -- --ignored`.
#![cfg(feature = "net")]

use desigma_fonts::{FontCache, FontRequest};

#[test]
#[ignore = "needs network access to api.fontsource.org"]
fn fetches_roboto_and_caches_it() {
    let dir = std::env::temp_dir().join(format!("desigma-fonts-live-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let cache = FontCache::with_dir(&dir);
    let req = FontRequest::new("Roboto", 700, false);

    let bytes = cache.resolve(&req).expect("fetch Roboto 700");
    assert!(bytes.len() > 10_000);
    assert!(matches!(&bytes[..4], [0, 1, 0, 0] | b"OTTO" | b"true"));
    let side = cache.sidecar(&req).expect("sidecar written");
    assert_eq!(side.license, "OFL-1.1");
    assert_eq!(side.weight, 700);

    // Served from disk now.
    assert_eq!(cache.offline_clone_resolve(&req), bytes);
    let _ = std::fs::remove_dir_all(&dir);
}

trait OfflineResolve {
    fn offline_clone_resolve(&self, req: &FontRequest) -> Vec<u8>;
}

impl OfflineResolve for FontCache {
    fn offline_clone_resolve(&self, req: &FontRequest) -> Vec<u8> {
        FontCache::with_dir(self.dir()).offline().resolve(req).expect("cached copy")
    }
}
