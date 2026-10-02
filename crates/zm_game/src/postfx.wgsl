// World at War's image in one fullscreen pass after the scene, in the
// game's gamma space (research/lighting/WAW_LIGHTING.md):
//  1. volumetric fog from the map's art script (SetVolFog / SetExpFog),
//     denser low down, from the depth buffer;
//  2. the vision set's film grade (`postfx_color`):
//     o = contrast * lerp(c, L, desat) * lerp(darkTint, lightTint, L) + brightness
//  plus the player's brightness setting as a gamma curve.

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput
#import bevy_render::view::View

struct WawPost {
    // fog: start, half-way distance, half height, base height (metres)
    fog: vec4<f32>,
    // rgb fog colour (gamma), w = fog enabled (0/1)
    fog_color: vec4<f32>,
    // x contrast, y brightness, z desaturation, w film enabled
    film: vec4<f32>,
    // rgb light tint, w = invert (0/1)
    light_tint: vec4<f32>,
    // rgb dark tint, w = display gamma (1 = unchanged)
    dark_tint: vec4<f32>,
}

@group(0) @binding(0) var screen: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@group(0) @binding(2) var depth: texture_depth_2d;
@group(0) @binding(3) var<uniform> view: View;
@group(0) @binding(4) var<uniform> settings: WawPost;

fn to_gamma(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(max(c, vec3(0.0)), vec3(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3(0.0031308));
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((max(c, vec3(0.0)) + 0.055) / 1.055, vec3(2.4));
    return select(hi, lo, c <= vec3(0.04045));
}

// Fraction of the scene colour left after fog (1 = clear), as the game's
// vertex shaders compute it, with the height integral normalised so a level
// ray sees the eye's density.
fn fog_factor(eye: vec3<f32>, p: vec3<f32>) -> f32 {
    let start = settings.fog.x;
    let half_way = max(settings.fog.y - start, 0.01);
    var k = 0.0;
    if settings.fog.z > 0.0 {
        k = 1.0 / settings.fog.z;
    }
    let eye_density = exp2(-(eye.y - settings.fog.w) * k);
    let a = k * (p.y - eye.y);
    var h = 1.0;
    if abs(a) > 1e-4 {
        h = (1.0 - exp2(-a)) / (a * 0.6931472);
    }
    let d = distance(p, eye) * eye_density * h;
    return min(1.0, exp2((start - d) / half_way));
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let src = textureSample(screen, screen_sampler, in.uv);
    var c = to_gamma(src.rgb);

    if settings.fog_color.w > 0.5 {
        let size = vec2<f32>(textureDimensions(depth));
        let d = textureLoad(depth, vec2<i32>(in.uv * size), 0);
        // Reverse-Z: 0 is the far plane (nothing drawn: sky, unfogged).
        if d > 0.0 {
            let ndc = vec4<f32>(in.uv.x * 2.0 - 1.0, 1.0 - in.uv.y * 2.0, d, 1.0);
            let w = view.world_from_clip * ndc;
            let f = fog_factor(view.world_position, w.xyz / w.w);
            c = mix(settings.fog_color.rgb, c, f);
        }
    }

    // The player's brightness setting, as a display gamma.
    c = pow(clamp(c, vec3(0.0), vec3(1.0)), vec3(1.0 / max(settings.dark_tint.w, 0.1)));

    if settings.film.w > 0.5 {
        let lum = dot(c, vec3<f32>(0.299, 0.587, 0.114));
        let tint = mix(settings.dark_tint.rgb, settings.light_tint.rgb, lum);
        c = settings.film.x * mix(c, vec3<f32>(lum), settings.film.z) * tint + settings.film.y;
        if settings.light_tint.w > 0.5 {
            c = 1.0 - c;
        }
    }

    return vec4<f32>(to_linear(clamp(c, vec3(0.0), vec3(1.0))), src.a);
}
