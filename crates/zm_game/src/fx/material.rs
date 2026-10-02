#![allow(dead_code)] // ShaderType derives emit unused size checks per field.
//! The material effect particles are drawn with (see `fx.wgsl`).

use bevy::asset::{load_internal_asset, weak_handle};
use bevy::pbr::{Material, MaterialPlugin, NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderRef, ShaderType};

const SHADER: Handle<Shader> = weak_handle!("6b1f0c3e-2a47-4d59-8e1b-7c2d9a4f5e61");

#[derive(Clone, Copy, Default, ShaderType)]
pub struct FxParams {
    /// x: 0 add, 1 blend, 2 multiply; y: soft fade distance (m).
    pub mode: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct FxParticleMaterial {
    #[uniform(0)]
    pub params: FxParams,
    #[texture(1)]
    #[sampler(2)]
    pub color: Option<Handle<Image>>,
    pub alpha: AlphaMode,
}

impl Material for FxParticleMaterial {
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
        _key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// Components every particle batch carries.
pub fn batch_bundle() -> impl Bundle {
    (NotShadowCaster, NotShadowReceiver)
}

pub struct FxMaterialPlugin;

impl Plugin for FxMaterialPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, SHADER, "fx.wgsl", Shader::from_wgsl);
        app.add_plugins(MaterialPlugin::<FxParticleMaterial>::default());
    }
}
