// Draws the rendered page texture 1:1 into the viewport it is called with.

@group(0) @binding(0) var tex: texture_2d<f32>;

struct V {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> V {
    var o: V;
    let x = f32((vi << 1u) & 2u);
    let y = f32(vi & 2u);
    o.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    o.uv = vec2<f32>(x, y);
    return o;
}

@fragment
fn fs_main(v: V) -> @location(0) vec4<f32> {
    let dim = vec2<f32>(textureDimensions(tex));
    let p = vec2<i32>(clamp(floor(v.uv * dim), vec2<f32>(0.0), dim - vec2<f32>(1.0)));
    return textureLoad(tex, p, 0);
}
