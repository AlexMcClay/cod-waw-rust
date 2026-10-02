// Models: Bevy's standard lighting plus the map's light grid as ambient
// light, sampled here per fragment (an ambient cube: six faces weighted by
// the squared normal, as Valve's "Ambient Cube Basis").

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
    mesh_view_bindings::view,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}
#endif

struct GridParams {
    light_from_world: mat4x4<f32>,
    intensity: vec4<f32>,
}

@group(2) @binding(100) var<uniform> grid_params: GridParams;
@group(2) @binding(101) var grid: texture_3d<f32>;
@group(2) @binding(102) var grid_sampler: sampler;

fn grid_light(world_position: vec3<f32>, N: vec3<f32>) -> vec3<f32> {
    let atlas = vec3<f32>(textureDimensions(grid));
    let resolution = atlas / vec3(1.0, 2.0, 3.0);
    let unit_pos = (grid_params.light_from_world * vec4(world_position, 1.0)).xyz;
    // Stay half a texel inside each slice so faces don't bleed together.
    let stp = clamp((unit_pos + 0.5) * resolution, vec3(0.5), resolution - vec3(0.5));
    let uvw = stp / atlas;
    let neg = select(vec3(0.0), vec3(0.5), N < vec3(0.0));
    let x = textureSampleLevel(grid, grid_sampler, uvw + vec3(0.0, neg.x, 0.0), 0.0).rgb;
    let y = textureSampleLevel(grid, grid_sampler, uvw + vec3(0.0, neg.y, 1.0 / 3.0), 0.0).rgb;
    let z = textureSampleLevel(grid, grid_sampler, uvw + vec3(0.0, neg.z, 2.0 / 3.0), 0.0).rgb;
    let nn = N * N;
    return (x * nn.x + y * nn.y + z * nn.z) * grid_params.intensity.x;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) != 0u {
        out.color = pbr_input.material.base_color;
    } else {
        out.color = apply_pbr_lighting(pbr_input);
        if grid_params.intensity.x > 0.0 {
            // As Bevy adds indirect light: diffuse colour x irradiance, at
            // the view's exposure.
            let diffuse = pbr_input.material.base_color.rgb * (1.0 - pbr_input.material.metallic);
            let light = grid_light(pbr_input.world_position.xyz, pbr_input.N);
            out.color = vec4(out.color.rgb + diffuse * light * view.exposure, out.color.a);
        }
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif
    return out;
}
