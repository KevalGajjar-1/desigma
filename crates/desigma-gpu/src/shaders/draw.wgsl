// Mesh draws: solid, gradients, images and noise, clipped by a coverage mask.

struct Globals {
    origin: vec2<f32>, // target top-left in root pixels
    size: vec2<f32>,   // target size in pixels
};

struct ClipParams {
    origin: vec2<f32>,
    size: vec2<f32>,
    enabled: f32,
    p0: f32,
    p1: f32,
    p2: f32,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var ramp_tex: texture_2d<f32>;
@group(0) @binding(2) var samp_clamp: sampler;
@group(0) @binding(3) var samp_repeat: sampler;

@group(1) @binding(0) var<uniform> clip: ClipParams;
@group(1) @binding(1) var clip_tex: texture_2d<f32>;

@group(2) @binding(0) var img: texture_2d<f32>;

const RAMP_W: f32 = 512.0;
const RAMP_ROWS: f32 = 1024.0;
const TAU: f32 = 6.283185307179586;

struct VIn {
    @location(0) pos: vec2<f32>,
    @location(1) m: vec4<f32>,
    @location(2) t: vec4<f32>,
    @location(3) c0: vec4<f32>,
    @location(4) c1: vec4<f32>,
    @location(5) c2: vec4<f32>,
    @location(6) q: vec4<u32>,
    @location(7) q2: vec4<i32>,
};

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) t: vec4<f32>,
    @location(2) @interpolate(flat) c0: vec4<f32>,
    @location(3) @interpolate(flat) c1: vec4<f32>,
    @location(4) @interpolate(flat) c2: vec4<f32>,
    @location(5) @interpolate(flat) q: vec4<u32>,
    @location(6) @interpolate(flat) q2: vec4<i32>,
};

@vertex
fn vs_main(v: VIn) -> VOut {
    var o: VOut;
    let dev = vec2<f32>(v.m.x * v.pos.x + v.m.z * v.pos.y + v.t.x, v.m.y * v.pos.x + v.m.w * v.pos.y + v.t.y) - g.origin;
    o.pos = vec4<f32>(dev.x / g.size.x * 2.0 - 1.0, 1.0 - dev.y / g.size.y * 2.0, 0.0, 1.0);
    o.local = v.pos;
    o.t = v.t;
    o.c0 = v.c0;
    o.c1 = v.c1;
    o.c2 = v.c2;
    o.q = v.q;
    o.q2 = v.q2;
    return o;
}

fn clip_cov(frag: vec2<f32>) -> f32 {
    if (clip.enabled < 0.5) {
        return 1.0;
    }
    let p = vec2<i32>(floor(frag + g.origin - clip.origin));
    if (p.x < 0 || p.y < 0 || f32(p.x) >= clip.size.x || f32(p.y) >= clip.size.y) {
        return 0.0;
    }
    return textureLoad(clip_tex, p, 0).a;
}

fn ramp(t: f32, row: u32) -> vec4<f32> {
    let u = (clamp(t, 0.0, 1.0) * (RAMP_W - 1.0) + 0.5) / RAMP_W;
    return textureSampleLevel(ramp_tex, samp_clamp, vec2<f32>(u, (f32(row) + 0.5) / RAMP_ROWS), 0.0);
}

fn mix32(h0: u32) -> u32 {
    var h = h0;
    h = h ^ (h >> 16u);
    h = h * 0x7feb352du;
    h = h ^ (h >> 15u);
    h = h * 0x846ca68bu;
    return h ^ (h >> 16u);
}

fn hash2(seed: u32, x: i32, y: i32) -> u32 {
    let a = bitcast<u32>(x) * 0x85ebca6bu;
    let b = bitcast<u32>(y) * 0xc2b2ae35u;
    let rb = (b << 13u) | (b >> 19u);
    return mix32((seed * 0x9e3779b1u) ^ mix32(a ^ rb));
}

fn unit(h: u32) -> f32 {
    return f32(h >> 8u) / 16777216.0;
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> vec3<f32> {
    let h6 = (h - floor(h)) * 6.0;
    let i = i32(floor(h6));
    let f = h6 - floor(h6);
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    switch (i) {
        case 0: { return vec3<f32>(v, t, p); }
        case 1: { return vec3<f32>(q, v, p); }
        case 2: { return vec3<f32>(p, v, t); }
        case 3: { return vec3<f32>(p, q, v); }
        case 4: { return vec3<f32>(t, p, v); }
        default: { return vec3<f32>(v, p, q); }
    }
}

// straight-colour paint result for kinds that need no texture
fn paint(i: VOut, frag: vec2<f32>) -> vec4<f32> {
    let kind = i.q.x;
    if (kind == 0u) {
        return vec4<f32>(i.c0.rgb * i.c0.a, i.c0.a);
    }
    if (kind == 6u) {
        let px = vec2<i32>(floor((frag + g.origin) / f32(max(i.q2.z, 1))));
        let gx = px.x + i.q2.x;
        let gy = px.y + i.q2.y;
        let cell = i.c2.x;
        let cx = i32(floor(f32(gx) / cell));
        let cy = i32(floor(f32(gy) / cell));
        let h = hash2(i.q.y, cx, cy);
        if (unit(h) >= i.c2.y) {
            return vec4<f32>(0.0);
        }
        let h2 = mix32(h ^ 0x6a09e667u);
        var c = i.c0;
        if (i.q.z == 1u && (h2 & 1u) != 0u) {
            c = i.c1;
        }
        if (i.q.z == 2u) {
            c = vec4<f32>(hsv_to_rgb(unit(h2), 1.0, 1.0), i.c0.a);
        }
        return vec4<f32>(c.rgb * c.a, c.a);
    }
    let u = vec2<f32>(i.c1.x * i.local.x + i.c1.z * i.local.y + i.c2.x, i.c1.y * i.local.x + i.c1.w * i.local.y + i.c2.y);
    var t = u.x;
    if (kind == 2u) {
        t = length(u);
    } else if (kind == 3u) {
        t = atan2(u.y, u.x) / TAU;
        if (t < 0.0) {
            t = t + 1.0;
        }
    } else if (kind == 4u) {
        t = min(abs(u.x) + abs(u.y), 1.0);
    }
    return ramp(t, i.q.w) * i.t.z;
}

@fragment
fn fs_main(i: VOut) -> @location(0) vec4<f32> {
    let cov = clip_cov(i.pos.xy);
    return paint(i, i.pos.xy) * cov;
}

@fragment
fn fs_image(i: VOut) -> @location(0) vec4<f32> {
    let uv = vec2<f32>(i.c1.x * i.local.x + i.c1.z * i.local.y + i.c2.x, i.c1.y * i.local.x + i.c1.w * i.local.y + i.c2.y);
    let sc = textureSample(img, samp_clamp, uv);
    let sr = textureSample(img, samp_repeat, uv);
    var c = sc;
    if (i.q.z == 2u) {
        c = sr;
    }
    if (i.q.z == 1u && (uv.x < 0.0 || uv.y < 0.0 || uv.x > 1.0 || uv.y > 1.0)) {
        c = vec4<f32>(0.0);
    }
    return c * i.t.z * clip_cov(i.pos.xy);
}
