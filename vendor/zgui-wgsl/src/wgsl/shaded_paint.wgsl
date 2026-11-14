// A paint effect: the application shades the colour, and the framework applies the box.
//
// The rounded rectangle still bounds it. An effect that wants to draw past its own corners gives
// itself square corners and shapes what it returns.

@fragment
fn fs_shaded_paint(in: ShadedVarying) -> @location(0) vec4<f32> {
    let quad = shaded[in.instance];
    // The clip is in device space, so it is evaluated at the real pixel; the box is in the
    // primitive's own space, so it is evaluated at the point that maps to this pixel.
    let clip = clip_coverage(device_position(in.position.xy), quad.clip);
    if clip <= 0.0 {
        return vec4<f32>(0.0);
    }
    let input = shader_input(quad, in.local);
    // OkColorPicker modification: extended RGB represents valid wide-gamut colors.
    // FP16 targets preserve it; normalized targets still constrain their stored values.
    // Alpha remains bounded independently of the RGB encoding and gamut.
    let shaded_color = shade(input, shader_params.user);
    let color = vec4<f32>(shaded_color.rgb, clamp(shaded_color.a, 0.0, 1.0));
    let bounds = bounds_from(quad.bounds, in.origin);
    let shape = coverage_of(quad_sdf(in.local, bounds, quad.radii, CORNER_ROUND));
    return color * shape * clip * quad.opacity;
}
