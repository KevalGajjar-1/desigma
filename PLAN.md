# desigma: pixel-perfect Figma-style editor in Rust

Decisions: pure Rust native, full MVP, reference file = shadcn/ui Design System (Community) copy, Figma file import/export required.

## Stack
- `winit` + `wgpu` window and GPU surface
- `vello` (or `tiny-skia` fallback) for vector canvas rendering; `parley`/`cosmic-text` for text; `kurbo` for paths and booleans
- Chrome UI: `egui` with a fully custom theme and custom-painted widgets (stock egui look will not pixel-match), or `iced` if retained-mode proves easier. Decide in a Phase 1 spike.
- Workspace crates: `desigma-doc` (model, undo), `desigma-render`, `desigma-ui`, `desigma-fig` (file formats), `desigma-app`, `desigma-diff` (pixel harness)

## Phase 0: reference capture (Figma editor in the browser pane, read-only)
- Fixed viewport 1440x900, screenshot every UI state: toolbar, left panel (pages, layers, assets), right panel (design/prototype/inspect), menus, tooltips, dark and light themes
- Extract computed styles via JS (colors, fonts, spacing, radii, shadows) into `reference/tokens.json`
- Manifest of states in `reference/manifest.json`; shadcn file pages (Cover, Components, Typography, Colors, Primitives, Icons) double as render-fidelity test content

## Phase 1: tokens, specs, spikes
- Per-component spec (size, states, icons redrawn as our own SVGs, no Figma logo/branding)
- Spike: egui vs iced for fidelity; wgpu+vello canvas inside the window

## Phase 2: core
- Scene graph (frame, group, rect, ellipse, line, polygon, star, vector, text, image, component, instance), fills/strokes/effects, constraints, auto-layout, variables/styles
- Undo/redo, selection, hit-testing, snapping/guides, pan/zoom

## Phase 3: editor
1. Shell and chrome matched to screenshots
2. Canvas, selection handles, shape tools
3. Layers tree, properties panel, text editing
4. Pen/vector edit, boolean ops, auto-layout, components/variants
5. Native `.desigma` save/load

## Figma file support (`desigma-fig`)
Two import paths, so we are not blocked by the proprietary format:
1. **REST/JSON import**: Figma REST API `GET /v1/files/:key` returns a documented node tree; map to our model. Needs a user-supplied personal access token (never stored in the repo). Images via `/v1/images`.
2. **`.fig` binary import**: `.fig` is a "fig-kiwi" container (header, then deflate-compressed Kiwi-encoded schema and message, often inside a zip with `meta.json`/thumbnail/images). Implement a Kiwi schema decoder in Rust, decode the node-changes message into the same intermediate model. The format is undocumented and community reverse-engineered, so expect version drift; test against the shadcn file exported from Figma (File > Save local copy).
3. **Export**: write `.fig`-compatible output last (stretch goal); the REST JSON shape plus clipboard HTML are fallbacks.
- Fidelity corpus: the shadcn file. Import it, render, and pixel-diff against Figma screenshots of each page.

## Phase 4: pixel verification
- `desigma-diff` renders each state headlessly at the reference viewport and compares with `reference/` (SSIM plus per-pixel threshold). A component passes only when under threshold; CI blocks regressions.
- Chrome is the strict pixel target; canvas anti-aliasing will differ slightly from Figma's renderer, so use a looser threshold there.

## Risks
- `.fig` format changes without notice; keep decoder tolerant, ignore unknown fields.
- Full MVP in pure Rust is large; milestones in order: chrome, canvas, import, editing, export.
- Figma assets/fonts (Inter is OFL and fine) and branding are not copied.
