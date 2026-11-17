import processing::filter::{sample, FullscreenVertexOutput};

fn srgb_encode(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

fn srgb_decode(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((max(c, vec3<f32>(0.0)) + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

struct Params {
    levels: u32,
    _p0: u32,
    _p1: u32,
    _p2: u32,
}
@group(1) @binding(0) var<uniform> params: Params;

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let c = sample(in.uv);
    let n = f32(max(params.levels, 2u));
    let q = floor(srgb_encode(c.rgb) * n) / (n - 1.0);
    return vec4<f32>(srgb_decode(clamp(q, vec3<f32>(0.0), vec3<f32>(1.0))), c.a);
}
