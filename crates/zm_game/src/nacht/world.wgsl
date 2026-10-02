// World at War's lit world surfaces (`lm_*` pixel shaders), ported from the
// disassembled game shaders (research/lighting/WAW_LIGHTING.md):
//
//   A, B  = the lightmap's top and bottom halves (secondary page)
//   d     = light direction slope from their alphas, Lz = its cosine
//   light = A + B * Lz                                 (no normal map)
//   light = A * Nz + B * sat(Lz * Nz * (1 + d.n))      (normal map slope n)
//   light += P * primary light (sun / spot / omni)     (P = primary page)
//   colour = albedo * vertex colour * light
//
// Everything is in the game's gamma space; fog and the film grade follow
// in the post-process pass.

#import bevy_pbr::forward_io::VertexOutput

struct WorldParams {
    // bit 0: colour map, 1: normal map, 2: tint by vertex colour, 3: alpha test
    flags: u32,
    // 0 none, 1 sun, 2 spot, 3 omni
    light_kind: u32,
    // 0 linear, 1 tungsten
    falloff: u32,
    exponent: f32,
    light_color: vec4<f32>,
    // world position (metres); w = 1 / radius
    light_pos: vec4<f32>,
    // sun: towards the sun; spot: towards the light (minus the cone axis)
    light_dir: vec4<f32>,
    // x = cos outer, y = cos inner
    spot: vec4<f32>,
}

@group(2) @binding(0) var<uniform> params: WorldParams;
@group(2) @binding(1) var color_map: texture_2d<f32>;
@group(2) @binding(2) var color_sampler: sampler;
@group(2) @binding(3) var normal_map: texture_2d<f32>;
@group(2) @binding(4) var normal_sampler: sampler;
@group(2) @binding(5) var lightmap_secondary: texture_2d<f32>;
@group(2) @binding(6) var lightmap_sampler: sampler;
@group(2) @binding(7) var lightmap_primary: texture_2d<f32>;
@group(2) @binding(8) var primary_sampler: sampler;

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

// 8-bit alpha (x) and 6-bit DXT green (y) -> tangent-space slope.
fn slope(x: f32, y: f32) -> vec2<f32> {
    return vec2<f32>(x * 4.08 - 2.08, y * 4.0645161 - 2.0645161);
}

// 0.6 * 2^(-|s|^2) + 0.4 ~ 1 / sqrt(1 + |s|^2): the z of the unit vector.
fn cos_from_slope(s: vec2<f32>) -> f32 {
    return clamp(0.6 * exp2(-dot(s, s)) + 0.4, 0.0, 1.0);
}

// The light's falloff curve, stored in row 0 of the secondary lightmap.
fn falloff(t: f32) -> vec3<f32> {
    let size = vec2<f32>(textureDimensions(lightmap_secondary));
    var start = 1.0;
    var width = 16.0;
    if params.falloff == 1u {
        start = 19.0;
        width = 32.0;
    }
    let uv = vec2<f32>((start + 0.5 + t * (width - 1.0)) / size.x, 0.5 / size.y);
    return textureSampleLevel(lightmap_secondary, lightmap_sampler, uv, 0.0).rgb;
}

fn primary_light(p: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    let color = params.light_color.rgb;
    if params.light_kind == 1u {
        return color * clamp(dot(params.light_dir.xyz, n), 0.0, 1.0);
    }
    if params.light_kind == 0u {
        return vec3<f32>(0.0);
    }
    let to_light = params.light_pos.xyz - p;
    let dist = length(to_light);
    let l = to_light / max(dist, 1e-4);
    let t = clamp(dist * params.light_pos.w, 0.0, 1.0);
    var lit = color * falloff(t) * clamp(dot(l, n), 0.0, 1.0);
    if params.light_kind == 2u {
        let x = 1.0 / max(params.spot.y - params.spot.x, 1e-4);
        let s = clamp(dot(l, params.light_dir.xyz) * x - params.spot.x * x, 0.0, 1.0);
        var cone = 0.0;
        if s > 0.0 {
            cone = pow(s, max(params.exponent, 1e-4));
        }
        lit = lit * cone;
    }
    return lit;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var albedo = vec3<f32>(0.5);
    var alpha = 1.0;
#ifdef VERTEX_UVS_A
    if (params.flags & 1u) != 0u {
        let c = textureSample(color_map, color_sampler, in.uv);
        albedo = to_gamma(c.rgb);
        alpha = c.a;
    }
#endif
    if (params.flags & 8u) != 0u && alpha < 0.5 {
        discard;
    }
#ifdef VERTEX_COLORS
    if (params.flags & 4u) != 0u {
        albedo = albedo * in.color.rgb;
    }
#endif

    var lm_uv = vec2<f32>(0.0);
#ifdef VERTEX_UVS_B
    lm_uv = in.uv_b;
#endif
    let a = textureSample(lightmap_secondary, lightmap_sampler, lm_uv * vec2<f32>(1.0, 0.5));
    let b = textureSample(lightmap_secondary, lightmap_sampler, lm_uv * vec2<f32>(1.0, 0.5) + vec2<f32>(0.0, 0.5));
    let d = slope(a.a, b.a);
    let lz = cos_from_slope(d);

    var n = normalize(in.world_normal);
    var light = a.rgb + b.rgb * lz;
#ifdef VERTEX_UVS_A
#ifdef VERTEX_TANGENTS
    if (params.flags & 2u) != 0u {
        let nm = textureSample(normal_map, normal_sampler, in.uv);
        let s = slope(nm.a, nm.g);
        let nz = cos_from_slope(s);
        light = a.rgb * nz + b.rgb * clamp(lz * nz * (1.0 + dot(d, s)), 0.0, 1.0);
        let t = normalize(in.world_tangent.xyz);
        let bn = sign(in.world_tangent.w) * cross(n, t);
        n = normalize(n + s.x * t + s.y * bn);
    }
#endif
#endif

    let p = textureSample(lightmap_primary, primary_sampler, lm_uv).r;
    light = light + p * primary_light(in.world_position.xyz, n);

    return vec4<f32>(to_linear(clamp(albedo * light, vec3(0.0), vec3(1.0))), alpha);
}
