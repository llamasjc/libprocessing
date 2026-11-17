import processing::filter::{sample, texel_size, FullscreenVertexOutput};

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

fn luma(c: vec3<f32>) -> f32 {
    let s = srgb_encode(c);
    return 0.30078125 * s.r + 0.58984375 * s.g + 0.109375 * s.b;
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let t = texel_size();
    let offsets = array<vec2<f32>, 4>(
        vec2<f32>(-t.x, 0.0),
        vec2<f32>(t.x, 0.0),
        vec2<f32>(0.0, -t.y),
        vec2<f32>(0.0, t.y),
    );
    var best = sample(in.uv);
    var best_luma = luma(best.rgb);
    for (var i = 0; i < 4; i++) {
        let s = sample(in.uv + offsets[i]);
        let sl = luma(s.rgb);
        if sl < best_luma {
            best = s;
            best_luma = sl;
        }
    }
    return best;
}
