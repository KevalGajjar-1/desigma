# Contributing to desigma

Thanks for helping. desigma is a pure-Rust Figma-style editor; the goal is pixel-accurate behaviour with a small, readable codebase.

## Build and run

You need a recent stable Rust toolchain. On Linux install the windowing headers first:

```sh
sudo apt-get install -y pkg-config libxkbcommon-dev libwayland-dev libx11-dev \
  libxcursor-dev libxrandr-dev libxi-dev libgl1-mesa-dev libfontconfig1-dev
```

```sh
cargo run --release -p desigma-app               # editor with the demo document
cargo run --release -p desigma-app -- file.fig   # open a Figma file
```

Dev builds compile dependencies with `opt-level = 2` (see the root `Cargo.toml`), so `cargo run -p desigma-app` is usable too.

## Test and lint

CI runs these on Linux, Windows and macOS; run them before opening a PR:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings   # must be warning-free
cargo fmt --all --check                                  # advisory for now, see below
```

Clippy is blocking. If a lint is genuinely wrong for a spot (for example `too_many_arguments` on a renderer internal), add a narrowly scoped `#[allow(...)]` with a one-line reason instead of restructuring.

Rustfmt settings live in `rustfmt.toml`. The tree was written before it, so `cargo fmt --check` still reports differences and the CI fmt job is non-blocking. Format only the files you are already changing, and avoid drive-by reformatting so diffs stay reviewable.

## Screenshots and pixel diffs

The app renders one frame headlessly, which is how UI changes are checked:

```sh
desigma --screenshot out.png --size 1600x1000 --select Card
desigma --tab prototype --present --screenshot present.png
```

Compare against a reference with the `desigma-diff` CLI (exit code 1 when the mismatch exceeds the threshold, 2 on errors):

```sh
desigma-diff compare reference.png out.png --out diff.png --tolerance 2 --threshold 0.001
desigma-diff suite path/to/manifest.json          # run many cases, writes an HTML report
```

Anti-aliased edge pixels are forgiven by default (`--no-aa` turns that off). Use `--ignore x,y,w,h` to mask regions such as a clock or cursor. Reference captures of Figma's own UI live under `reference/screens/`; they are local-only and git-ignored, so do not commit them. Screenshots for the README go in `docs/images/`.

## Crate map

| Crate | Role |
|---|---|
| `desigma-doc` | Scene graph, paints, text, auto layout, components, variables, prototype model, undo/redo. Pure data, no UI. |
| `desigma-render` | Deterministic CPU renderer (tiny-skia), spatial index, tile cache. |
| `desigma-fig` | Figma REST JSON and `.fig` import, REST JSON export. |
| `desigma-fonts` | Bundled Inter, Fontsource fetch and cache, font substitution. |
| `desigma-export` | PNG, JPG, SVG export and Dev mode CSS. |
| `desigma-app` | The egui desktop editor (binary `desigma`). |
| `desigma-diff` | Pixel-diff library and the `desigma-diff` CLI. |

Dependencies flow downward: `app` depends on everything, `doc` depends on nothing in the workspace. Keep UI code out of the lower crates, and make document edits go through undoable `Command`s.

## Commit style

- Short imperative subject, optionally prefixed with the area: `app: assets panel thumbnails`, `doc: variables with modes`, `render: cached shadows`.
- Explain the why in the body when it is not obvious. Keep commits logically separate (a lint pass, a feature and a docs change are three commits).
- Add or update tests with behaviour changes. Rendering changes should keep the output deterministic.

## Cutting a release

Releases are built by `.github/workflows/release.yml` when a `v*` tag is pushed. See [docs/releasing.md](docs/releasing.md) for the checklist.
