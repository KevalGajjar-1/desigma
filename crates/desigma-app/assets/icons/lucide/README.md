# Vendored Lucide icons

Icon set: [Lucide](https://lucide.dev) (ISC license, see `LICENSE`).
Version: `lucide-static@0.468.0`, fetched from
`https://cdn.jsdelivr.net/npm/lucide-static@0.468.0/icons/<name>.svg`.

Only the icons the editor uses are vendored. They are unmodified; at render time
`src/icons.rs` swaps `currentColor` for white and overrides `stroke-width` (so 16px
glyphs get a ~1.25px line), rasterises with resvg into an alpha mask and tints it
when painting. `Icon::lucide_name` maps every UI icon to the file here; a test fails
when a vendored file is unused or a mapped file is missing.

The brand logo is not from Lucide (it is drawn in `icons.rs`).
