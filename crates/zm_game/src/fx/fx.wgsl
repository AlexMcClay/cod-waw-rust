// Effect particles: texture x vertex colour (the element's colour and
// alpha over its life), drawn additively, alpha-blended or multiplied as
// the effect's material says. "zfeather" materials fade where they meet
// the scene (soft particles), using the camera's depth prepass.

#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::view
#ifdef DEPTH_PREPASS
#import bevy_pbr::prepass_utils
#endif

struct FxParams {
    // x: 0 add, 1 alpha blend, 2 multiply
    // y: soft-particle fade distance in metres (0 = hard)
    mode: vec4<f32>,
}

@group(2) @binding(0) var<uniform> params: FxParams;
@group(2) @binding(1) var color_map: texture_2d<f32>;
@group(2) @binding(2) var color_sampler: sampler;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var c = textureSample(color_map, color_sampler, in.uv);
#ifdef VERTEX_COLORS
    c = c * in.color;
#endif
#ifdef DEPTH_PREPASS
    if params.mode.y > 0.0 {
        // Infinite reverse-Z: view distance = near / depth.
        let near = view.clip_from_view[3][2];
        let scene = prepass_utils::prepass_depth(in.position, 0u);
        let scene_dist = near / max(scene, 1e-7);
        let frag_dist = near / max(in.position.z, 1e-7);
        c.a = c.a * clamp((scene_dist - frag_dist) / params.mode.y, 0.0, 1.0);
    }
#endif
    if params.mode.x < 0.5 {
        // Premultiplied blending with zero alpha: pure addition.
        return vec4<f32>(c.rgb * c.a, 0.0);
    }
    if params.mode.x < 1.5 {
        return vec4<f32>(c.rgb * c.a, c.a);
    }
    return vec4<f32>(mix(vec3<f32>(1.0), c.rgb, c.a), 1.0);
}
