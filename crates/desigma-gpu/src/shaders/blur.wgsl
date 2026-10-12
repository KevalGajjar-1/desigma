// Full-screen post passes: separable Gaussian blur, bilinear resample and the
// texture-effect displacement. All read with exact texel loads (or the
// sampler for resampling), and treat `size` as the logical extent of the
// source texture (pooled textures are larger than their content).

struct BlurParams {
    dir: vec2<i32>,    // (1,0) or (0,1)
    size: vec2<i32>,   // logical extent of source and destination
    radius: i32,
    clamp_edges: u32,
    sigma: f32,
    p0: f32,
};

@group(0) @binding(0) var<uniform> bp: BlurParams;
@group(0) @binding(1) var bsrc: texture_2d<f32>;

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

fn fetch(p: vec2<i32>) -> vec4<f32> {
    if (bp.clamp_edges != 0u) {
        return textureLoad(bsrc, clamp(p, vec2<i32>(0), bp.size - vec2<i32>(1)), 0);
    }
    if (p.x < 0 || p.y < 0 || p.x >= bp.size.x || p.y >= bp.size.y) {
        return vec4<f32>(0.0);
    }
    return textureLoad(bsrc, p, 0);
}

@fragment
fn fs_blur(f: FsIn) -> @location(0) vec4<f32> {
    let c = vec2<i32>(f.pos.xy);
    var sum = vec4<f32>(0.0);
    var wsum = 0.0;
    let inv = 1.0 / (2.0 * bp.sigma * bp.sigma);
    for (var i = -bp.radius; i <= bp.radius; i = i + 1) {
        let w = exp(-f32(i * i) * inv);
        sum = sum + fetch(c + bp.dir * i) * w;
        wsum = wsum + w;
    }
    return sum / wsum;
}

