# Releasing

Releases are tag-driven. Pushing a tag `vX.Y.Z` runs `.github/workflows/release.yml`.

## Checklist

1. Make sure `main` is green (tests on Linux, Windows and macOS, Clippy).
2. Bump `version` under `[workspace.package]` in the root `Cargo.toml`, run `cargo build --workspace` so `Cargo.lock` updates, and commit both (`release: 0.2.0`).
3. Tag and push:

   ```sh
   git tag -a v0.2.0 -m "desigma 0.2.0"
   git push origin v0.2.0
   ```

   A tag containing a hyphen (`v0.2.0-rc.1`) is published as a pre-release.
4. Watch the Release workflow. When all builds pass, a GitHub Release is created with auto-generated notes. Edit the notes if needed.

## What gets built

| Target | Runner | Archive |
|---|---|---|
| `x86_64-pc-windows-msvc` | `windows-latest` | `desigma-<version>-<target>.zip` |
| `x86_64-unknown-linux-gnu` | `ubuntu-latest` | `desigma-<version>-<target>.tar.gz` |
| `aarch64-apple-darwin` | `macos-14` | `desigma-<version>-<target>.tar.gz` |
| `x86_64-apple-darwin` | `macos-14` (cross-compiled; `macos-13` is retired) | `desigma-<version>-<target>.tar.gz` |

Each archive holds the `desigma` editor, the `desigma-diff` CLI, `LICENSE-MIT`, `LICENSE-APACHE` and `README.md`. The release also carries a combined `SHA256SUMS` (verify with `sha256sum -c SHA256SUMS`). Packaging is done by `scripts/package.sh <target> <version>`, which you can run locally after a release build.

## Dry run

Run the workflow from the Actions tab (Release, Run workflow) on any branch. It builds and packages everything and uploads the archives as workflow artifacts, but creates no release. The version is stamped `0.0.0-dev.<sha>`.

## Not covered yet

- macOS binaries are unsigned and not notarized, so Gatekeeper will warn; users can right-click Open or run `xattr -d com.apple.quarantine`.
- Windows binaries are unsigned (SmartScreen warning) and carry no embedded icon or version resource.
- Linux binaries link the build runner's glibc; they run on that distribution or newer.
