// D65/OKLab transforms mirror the f64 CPU reference in okcolor-core.
// Working-gamut membership is independent of the display's preview policy.
struct Params {
    channels: vec4<f32>,
    gamut: f32,
    kind: f32,
    mapping: f32,
    max_chroma: f32,
    wide: f32,
    show_p3: f32,
    show_rec2020: f32,
    outline_dark: f32,
    checker_offset: vec2<f32>,
    padding: vec2<f32>,
}

fn xyz_from_lab(lab: vec3<f32>) -> vec3<f32> {
    let root = vec3<f32>(
        dot(vec3<f32>(1.0, 0.3963377774, 0.2158037573), lab),
        dot(vec3<f32>(1.0, -0.1055613458, -0.0638541728), lab),
        dot(vec3<f32>(1.0, -0.0894841775, -1.2914855480), lab));
    let lms = root * root * root;
    return vec3<f32>(
        dot(vec3<f32>(1.2268798758, -0.5578149945, 0.2813910457), lms),
        dot(vec3<f32>(-0.0405757452, 1.1122868033, -0.0717110581), lms),
        dot(vec3<f32>(-0.0763729367, -0.4214933324, 1.5869240198), lms));
}

fn linear_rgb(lab: vec3<f32>, gamut: f32) -> vec3<f32> {
    let xyz = xyz_from_lab(lab);
    if gamut < 0.5 {
        return vec3<f32>(
            dot(vec3<f32>(3.2409699419, -1.5373831776, -0.4986107603), xyz),
            dot(vec3<f32>(-0.9692436363, 1.8759675015, 0.0415550574), xyz),
            dot(vec3<f32>(0.0556300797, -0.2039769589, 1.0569715142), xyz));
    }
    if gamut < 1.5 {
        return vec3<f32>(
            dot(vec3<f32>(2.4934969119, -0.9313836179, -0.4027107845), xyz),
            dot(vec3<f32>(-0.8294889696, 1.7626640603, 0.0236246858), xyz),
            dot(vec3<f32>(0.0358458302, -0.0761723893, 0.9568845240), xyz));
    }
    return vec3<f32>(
        dot(vec3<f32>(1.7166511880, -0.3556707838, -0.2533662814), xyz),
        dot(vec3<f32>(-0.6666843518, 1.6164812366, 0.0157685458), xyz),
        dot(vec3<f32>(0.0176398574, -0.0427706133, 0.9421031212), xyz));
}

fn in_gamut(lab: vec3<f32>, gamut: f32) -> bool {
    let rgb = linear_rgb(lab, gamut);
    return all(rgb >= vec3<f32>(-0.0000001)) && all(rgb <= vec3<f32>(1.0000001));
}

fn encode_channel(v: f32) -> f32 {
    let a = abs(v);
    if a <= 0.0031308 { return 12.92 * v; }
    return sign(v) * (1.055 * pow(a, 1.0 / 2.4) - 0.055);
}
fn decode_channel(v: f32) -> f32 {
    if v <= 0.04045 { return v / 12.92; }
    return pow((v + 0.055) / 1.055, 2.4);
}
fn clipped(lab: vec3<f32>) -> vec3<f32> {
    let rgb = linear_rgb(lab, 0.0);
    return clamp(vec3<f32>(encode_channel(rgb.r), encode_channel(rgb.g), encode_channel(rgb.b)), vec3<f32>(0.0), vec3<f32>(1.0));
}
fn extended(lab: vec3<f32>) -> vec3<f32> {
    let rgb = linear_rgb(lab, 0.0);
    // FP16 has a finite representation. This guard affects extreme preview
    // coordinates, never the original f64 color, and is not gamut mapping.
    return clamp(vec3<f32>(encode_channel(rgb.r), encode_channel(rgb.g), encode_channel(rgb.b)), vec3<f32>(-65504.0), vec3<f32>(65504.0));
}
fn lab_from_srgb(rgb: vec3<f32>) -> vec3<f32> {
    let lin = vec3<f32>(decode_channel(rgb.r), decode_channel(rgb.g), decode_channel(rgb.b));
    let xyz = vec3<f32>(
        dot(vec3<f32>(0.4123907993, 0.3575843394, 0.1804807884), lin),
        dot(vec3<f32>(0.2126390059, 0.7151686788, 0.0721923154), lin),
        dot(vec3<f32>(0.0193308187, 0.1191947798, 0.9505321522), lin));
    let lms = vec3<f32>(
        dot(vec3<f32>(0.8190224380, 0.3619062601, -0.1288737815), xyz),
        dot(vec3<f32>(0.0329836539, 0.9292868616, 0.0361446664), xyz),
        dot(vec3<f32>(0.0481771894, 0.2642395318, 0.6335478285), xyz));
    let root = sign(lms) * pow(abs(lms), vec3<f32>(1.0 / 3.0));
    return vec3<f32>(
        dot(vec3<f32>(0.2104542683, 0.7936177747, -0.0040720430), root),
        dot(vec3<f32>(1.9779985324, -2.4285922420, 0.4505937096), root),
        dot(vec3<f32>(0.0259040425, 0.7827717125, -0.8086757549), root));
}

// CSS Color 4 (2026-11-12), binary-search local MINDE, matching the CPU policy.
fn preview_rgb(original: vec3<f32>, method: f32) -> vec3<f32> {
    if method > 0.5 { return clipped(original); }
    if original.x <= 0.0 { return vec3<f32>(0.0); }
    if original.x >= 1.0 { return vec3<f32>(1.0); }
    if in_gamut(original, 0.0) { return clipped(original); }
    let chroma = length(original.yz);
    let direction = original.yz / max(chroma, 0.0000001);
    var high = min(chroma, 1.0);
    var low = 0.0;
    var current = vec3<f32>(original.x, direction * high);
    var rgb = clipped(current);
    if distance(current, lab_from_srgb(rgb)) < 0.02 { return rgb; }
    var low_inside = true;
    for (var i = 0; i < 16; i += 1) {
        if high - low <= 0.0001 { break; }
        let c = (low + high) * 0.5;
        current = vec3<f32>(original.x, direction * c);
        if low_inside && in_gamut(current, 0.0) { low = c; continue; }
        rgb = clipped(current);
        let delta = distance(current, lab_from_srgb(rgb));
        if delta < 0.02 {
            if 0.02 - delta < 0.0001 { return rgb; }
            low_inside = false;
            low = c;
        } else { high = c; }
    }
    return rgb;
}

fn color_at(uv: vec2<f32>, params: Params) -> vec3<f32> {
    var lch = params.channels.xyz;
    if params.kind < 0.5 { lch.x = 1.0 - uv.y; lch.y = uv.x * params.max_chroma; }
    else if params.kind < 1.5 { lch.x = uv.x; }
    else if params.kind < 2.5 { lch.y = uv.x * params.max_chroma; }
    else if params.kind < 3.5 { lch.z = uv.x * 360.0; }
    let radians = lch.z * 0.017453292519943295;
    return vec3<f32>(lch.x, lch.y * cos(radians), lch.y * sin(radians));
}

fn shade(input: ShaderInput, params: Params) -> vec4<f32> {
    let uv = clamp(input.local / max(input.size, vec2<f32>(1.0)), vec2<f32>(0.0), vec2<f32>(1.0));
    let lab = color_at(uv, params);
    var rgb = vec3<f32>(0.03529412);
    if params.kind >= 0.5 || in_gamut(lab, params.gamut) {
        if params.wide > 0.5 { rgb = extended(lab); }
        else { rgb = preview_rgb(lab, params.mapping); }
    }
    if params.kind > 3.5 {
        // Marker coordinates are relative to the bar's fixed checker origin.
        let cell = floor((input.local + params.checker_offset) / (5.0 * input.scale));
        let tile = (i32(cell.x) + i32(cell.y)) & 1;
        let backdrop = select(0.13, 0.23, tile == 1);
        let alpha = select(uv.x, params.channels.w, params.kind > 4.5);
        rgb = mix(vec3<f32>(backdrop), rgb, alpha);
    } else {
        // A crossing in either axis marks the exact RGB-boundary neighborhood.
        if params.kind > 0.5 && !in_gamut(lab, params.gamut) {
            let pixel = vec2<i32>(floor(input.local / input.scale));
            let stripe = (pixel.x + pixel.y) & 3;
            if stripe == 0 { rgb = mix(rgb, vec3<f32>(0.0), 0.55); }
        }
        let step = vec2<f32>(0.5) / max(input.size, vec2<f32>(1.0));
        for (var g = 2; g >= 0; g -= 1) {
            if (g == 1 && params.show_p3 < 0.5) || (g == 2 && params.show_rec2020 < 0.5) { continue; }
            let gamut = f32(g);
            let left = in_gamut(color_at(clamp(uv - vec2<f32>(step.x, 0.0), vec2<f32>(0.0), vec2<f32>(1.0)), params), gamut);
            let right = in_gamut(color_at(clamp(uv + vec2<f32>(step.x, 0.0), vec2<f32>(0.0), vec2<f32>(1.0)), params), gamut);
            var boundary = left != right;
            if params.kind < 0.5 {
                let above = in_gamut(color_at(clamp(uv - vec2<f32>(0.0, step.y), vec2<f32>(0.0), vec2<f32>(1.0)), params), gamut);
                let below = in_gamut(color_at(clamp(uv + vec2<f32>(0.0, step.y), vec2<f32>(0.0), vec2<f32>(1.0)), params), gamut);
                boundary = boundary || above != below;
            }
            if boundary {
                let ink = select(select(vec3<f32>(0.65, 0.71, 0.79), vec3<f32>(0.71, 0.56, 0.82), g == 1), vec3<f32>(0.93), g == 0);
                rgb = mix(rgb, ink, 0.85);
            }
        }
    }
    if params.outline_dark > 0.5 && max(max(rgb.r, rgb.g), rgb.b) < 9.0 / 255.0 {
        let edge = min(min(input.local.x,input.local.y),min(input.size.x-input.local.x,input.size.y-input.local.y));
        if edge < input.scale { rgb = vec3<f32>(40.0 / 255.0); }
    }
    if params.kind > 5.5 {
        // An upward-pointing half diamond sits inside the lower edge of the bar.
        let p = vec2<f32>(abs(input.local.x - input.size.x * 0.5), input.local.y);
        let d = max((p.x - p.y) * 0.70710678, p.y - input.size.y);
        let stroke = input.scale;
        let coverage = 1.0 - smoothstep(-0.5, 0.5, d);
        rgb = mix(rgb, vec3<f32>(1.0), smoothstep(-stroke - 0.5, -stroke + 0.5, d));
        return vec4<f32>(rgb * coverage, coverage);
    }
    return vec4<f32>(rgb, 1.0);
}
