# desigma-gpu

GPU canvas renderer for desigma documents. It draws a `desigma_doc::Document`
page with wgpu, either straight into an egui paint callback (the editor canvas)
or into an offscreen texture (tests, benchmarks, `desigma --screenshot --gpu`).

The CPU renderer in `desigma-render` (tiny-skia) is the deterministic
reference: `--screenshot` and every pixel-diff baseline keep using it, and this
crate is held to it by `tests/fidelity.rs`, which renders scenes with both and
compares them with `desigma-diff`.

## Backend decision: wgpu pipelines + lyon, not vello

Candidates: **vello** (GPU compute 2D renderer on wgpu) and a **custom wgpu
renderer** that tessellates paths with `lyon` and draws them with instanced
pipelines.

### wgpu version

`Cargo.lock` pins eframe/egui-wgpu 0.35 to **wgpu 29.0.4**. Checking
crates.io (October 2026): vello 0.11 depends on `wgpu ^30`, vello 0.10 and 0.9
on `wgpu ^29.0.3`, vello 0.8 on `wgpu ^28`. So vello 0.10 *does* share the
wgpu major with egui 0.35; the version match is not what rules it out. The
decision rests on feature coverage and complexity.

### Feature coverage

| Need (CPU renderer parity) | vello 0.10 | wgpu + lyon (this crate) |
|---|---|---|
| Solid, linear, radial gradients | yes | yes (shader, ramp texture) |
| Angular gradient | sweep gradient (needs mapping) | yes |
| Diamond gradient | **no** | yes |
| Image fills: fill / fit / crop / tile | images, no letterbox / pad semantics | yes |
| Stroke with inside / outside alignment | via clip layers | clip mask (same construction as the CPU path) |
| Dashes, caps, joins, miter limit | own stroker (results differ from tiny-skia) | the CPU renderer's own stroker output is tessellated, so strokes are the same geometry |
| Opacity layers, blend modes, clips, masks | layers; no LinearBurn / LinearDodge | layers, all 19 modes |
| Drop / inner shadow, layer blur, background blur | **only blurred rounded rects**; no general blur and no backdrop access | render-to-texture + separable Gaussian |
| Noise, Texture, Glass | **no** (would need a second pipeline anyway) | shaders |
| Retained per-node cache | none: the scene is re-encoded every frame | tessellation cached per node and zoom bucket |
| Needs | compute shaders + storage textures (fails on GL / old drivers) | plain render pipelines (runs on every wgpu backend) |

Shadows, blurs, background blur, noise, texture and glass are half of what the
CPU renderer does. All of them need render-to-texture with custom passes in
the middle of the draw order, which vello's single "encode scene, then render"
model does not offer: every such node would need its own full vello render plus
an image round trip. Building our own layer system for those effects also
gives us blend modes, masks and clips for free, so vello would only replace
the path rasteriser while we kept writing everything around it.

### Complexity

vello adds a large dependency tree (shader preprocessing, peniko, kurbo,
skrifa copies), a multi-second shader compile at start-up and a hard
requirement for compute support. The custom renderer adds `lyon_tessellation`
and about 2k lines of Rust + WGSL that we fully control and can debug with the
pixel-diff harness.

Trade-off accepted: MSAA 4x edges instead of vello's analytic coverage (small
edge differences, covered by the tolerance of the fidelity tests), and more
code to own.

## Architecture

```
Document ──► compile (CPU, per frame) ──► Frame ──► execute (GPU) ──► texture
              geometry cache (lyon)        programs     passes, pools
```

* `compile`: walks the visible part of the page exactly like the CPU renderer
  (`draw_children` / `draw_node` / `draw_body`) and emits a `Frame`: a list of
  render *programs* (the page, plus layers, masks, clip masks and effect
  textures), each a list of draw and composite ops. Geometry comes from
  `desigma-render` (`outline_of`, `text_outlines`, `stroke_plan`), so shapes,
  glyph positions, stroke expansion and boolean operations are the CPU
  renderer's. Meshes are cached per node, keyed by the node's geometry-relevant
  fields and a zoom bucket, so pan and zoom never re-tessellate.
* `execute`: owns pipelines, a mesh arena, a texture pool and the passes. It
  runs programs (children first, in-order when they read the backdrop), blurs
  and composites.
* The compile half needs `&Document`; the execute half is `Send + Sync` and is
  what egui's paint callback runs, so the editor builds the `Frame` while
  painting the UI and hands it to the callback.

## Fallbacks

Anything the GPU path does not draw is rendered by the CPU renderer into a
texture for just that node and composited at the right place in z-order, so
content is never dropped. See the list in the crate docs (`Fallback`).
