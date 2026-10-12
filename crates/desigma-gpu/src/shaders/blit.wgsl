// Bilinear resample (down/up scaling, mip generation). `src_extent` is the
// logical size of the source content; pooled textures are larger than that.

struct FsIn {
    @builtin(position) pos: vec4<f32>,
};

@vertex
fn vs_full(@builtin(vertex_index) vi: u32) -> FsIn {
    // one oversized triangle covering the viewport
    var o: FsIn;
    let x = f32((vi << 1u) & 2u);
    let y = f32(vi & 2u);
    o.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    return o;
}

struct BlitParams {
    src_extent: vec2<f32>, // logical size of the source content
    dst_extent: vec2<f32>, // logical size of the destination
    src_tex: vec2<f32>,    // allocated size of the source texture
    p0: vec2<f32>,
};

@group(0) @binding(0) var<uniform> tp: BlitParams;
@group(0) @binding(1) var tsrc: texture_2d<f32>;
@group(0) @binding(2) var tsamp: sampler;

@fragment
fn fs_blit(f: FsIn) -> @location(0) vec4<f32> {
    var s = (f.pos.xy) * (tp.src_extent / tp.dst_extent);
    s = clamp(s, vec2<f32>(0.5), tp.src_extent - vec2<f32>(0.5));
    return textureSampleLevel(tsrc, tsamp, s / tp.src_tex, 0.0);
}
