import processing::filter::{sample, texel_size, pass_index, FullscreenVertexOutput};

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
    radius: f32,
    _p0: f32,
    _p1: f32,
    _p2: f32,
}
@group(1) @binding(0) var<uniform> params: Params;

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let t = texel_size();
    let dir = select(vec2<f32>(0.0, t.y), vec2<f32>(t.x, 0.0), pass_index() == 0u);

    let radius = max(params.radius, 1.0);
    let sigma = radius * 0.5 + 0.5;
    let r = i32(radius);

    var sum = vec4<f32>(0.0);
    var weight_sum = 0.0;
    for (var i = -r; i <= r; i++) {
        let x = f32(i);
        let w = exp(-(x * x) / (2.0 * sigma * sigma));
        let s = sample(in.uv + dir * x);
        sum += vec4<f32>(srgb_encode(s.rgb), s.a) * w;
        weight_sum += w;
    }
    let avg = sum / weight_sum;
    return vec4<f32>(srgb_decode(avg.rgb), avg.a);
}
