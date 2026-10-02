#![allow(dead_code)] // ShaderType derives emit unused size checks per field.
//! The lit world surfaces as World at War draws them (see `world.wgsl`):
//! the directional lightmap, normal maps, vertex colour, and each
//! surface's primary light on top.

use bevy::asset::{load_internal_asset, weak_handle};
use bevy::pbr::{Material, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderRef, ShaderType};

const SHADER: Handle<Shader> = weak_handle!("2d8f4a51-7b0e-4c86-9a3e-5f1c6d2b8e47");

pub const FLAG_COLOR: u32 = 1;
pub const FLAG_NORMAL: u32 = 2;
pub const FLAG_VERTEX_COLOR: u32 = 4;
pub const FLAG_ALPHA_TEST: u32 = 8;

#[allow(dead_code)] // ShaderType's generated size checks
#[derive(Clone, Copy, Default, ShaderType)]
pub struct WorldParams {
    pub flags: u32,
    /// 0 none, 1 sun, 2 spot, 3 omni.
    pub light_kind: u32,
    /// 0 linear, 1 tungsten.
    pub falloff: u32,
    pub exponent: f32,
    pub light_color: Vec4,
    /// World position in metres; w = 1 / radius.
    pub light_pos: Vec4,
    pub light_dir: Vec4,
    /// x = cos outer, y = cos inner.
    pub spot: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
#[bind_group_data(WorldKey)]
pub struct WawWorldMaterial {
    #[uniform(0)]
    pub params: WorldParams,
    #[texture(1)]
    #[sampler(2)]
    pub color: Option<Handle<Image>>,
    #[texture(3)]
    #[sampler(4)]
    pub normal: Option<Handle<Image>>,
    #[texture(5)]
    #[sampler(6)]
    pub lightmap_secondary: Handle<Image>,
    #[texture(7)]
    #[sampler(8)]
    pub lightmap_primary: Handle<Image>,
    pub alpha: AlphaMode,
    pub two_sided: bool,
    /// Depth offset of a decal surface lying on other geometry, so it
    /// doesn't z-fight (flicker) with the surface under it (0 = none).
    pub depth_bias: u8,
}

/// Pipeline key: culling and the decal depth offset.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorldKey {
    two_sided: bool,
    depth_bias: u8,
}

impl Material for WawWorldMaterial {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        self.alpha
    }

    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline<Self>,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        _layout: &bevy::render::mesh::MeshVertexBufferLayoutRef,
        key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        if key.bind_group_data.two_sided {
            descriptor.primitive.cull_mode = None;
        }
        if key.bind_group_data.depth_bias > 0 {
            // Reverse Z: positive bias pulls towards the camera.
            if let Some(ds) = descriptor.depth_stencil.as_mut() {
                ds.bias.constant = 2 * key.bind_group_data.depth_bias as i32;
                ds.bias.slope_scale = 0.75 * key.bind_group_data.depth_bias as f32;
            }
        }
        Ok(())
    }
}

impl From<&WawWorldMaterial> for WorldKey {
    fn from(m: &WawWorldMaterial) -> WorldKey {
        WorldKey { two_sided: m.two_sided, depth_bias: m.depth_bias }
    }
}

pub struct WorldMaterialPlugin;

impl Plugin for WorldMaterialPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, SHADER, "world.wgsl", Shader::from_wgsl);
        app.add_plugins(MaterialPlugin::<WawWorldMaterial>::default());
    }
}
