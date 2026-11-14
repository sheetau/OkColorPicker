// The retained UI composes premultiplied, extended, gamma-encoded sRGB.
// Decode only at the presentation boundary; FP16 scRGB uses linear RGB, D65,
// and 80 nits at (1,1,1). Keep negative and >1 channels for wide-gamut colors.
struct OutputSettings {
    white_scale: f32,
    _padding0: f32,
    _padding1: f32,
    _padding2: f32,
}
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var<uniform> settings: OutputSettings;

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(positions[index], 0.0, 1.0);
}

fn decode_srgb(value: vec3<f32>) -> vec3<f32> {
    let linear = value / 12.92;
    let power = sign(value) * pow((abs(value) + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(power, linear, abs(value) <= vec3<f32>(0.04045));
}

@fragment
fn fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let color = textureLoad(source, vec2<i32>(position.xy), 0);
    if color.a <= 0.0 { return vec4<f32>(0.0); }
    let straight = color.rgb / color.a;
    return vec4<f32>(decode_srgb(straight) * settings.white_scale * color.a, color.a);
}

// The bounded SDR fallback uses the UI's original gamma-encoded values.
@fragment
fn fragment_sdr(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(source, vec2<i32>(position.xy), 0);
}
