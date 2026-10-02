#![allow(dead_code)] // ShaderType derives emit unused size checks per field.
//! Models, props and brush models: Bevy's standard material plus the map's
//! light grid (see `model.wgsl`), sampled by our own shader.
//!
//! Bevy's `IrradianceVolume` finds its volume per screen cluster from the
//! volume's bounding sphere; one volume spanning the whole map holds the
//! camera deep inside that sphere, where the cluster assignment drops tiles
//! — models went black in screen-aligned blocks that flickered as the
//! camera moved. Sampling the grid directly has no such lookup.

use bevy::asset::{load_internal_asset, weak_handle};
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderRef, ShaderType};

const SHADER: Handle<Shader> = weak_handle!("8c3e1f27-5d94-4b0a-a6e2-1f7b9c4d2e83");

/// The material models use.
pub type ModelMaterial = ExtendedMaterial<StandardMaterial, GridLight>;

#[derive(Clone, Copy, Default, ShaderType, Debug)]
pub struct GridParams {
    /// World to the grid's unit cube (-0.5..0.5 on each axis).
    pub light_from_world: Mat4,
    /// x: intensity (0 = no grid).
    pub intensity: Vec4,
}

/// The light grid as a packed ambient cube volume: `(Rx, 2Ry, 3Rz)`,
/// positive faces in the first half of each slice, X/Y/Z faces in thirds
/// (the same layout Bevy's irradiance volumes use).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct GridLight {
    #[uniform(100)]
    pub params: GridParams,
    #[texture(101, dimension = "3d")]
    #[sampler(102)]
    pub grid: Option<Handle<Image>>,
}

impl GridLight {
    /// The grid with its volume transform; `None` lights nothing.
    pub fn new(grid: Option<(Handle<Image>, Transform)>, intensity: f32) -> GridLight {
        match grid {
            Some((image, t)) => GridLight {
                params: GridParams { light_from_world: t.compute_matrix().inverse(), intensity: Vec4::new(intensity, 0.0, 0.0, 0.0) },
                grid: Some(image),
            },
            None => GridLight { params: GridParams::default(), grid: None },
        }
    }
}

impl MaterialExtension for GridLight {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
}

pub struct ModelMaterialPlugin;

impl Plugin for ModelMaterialPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, SHADER, "model.wgsl", Shader::from_wgsl);
        app.add_plugins(MaterialPlugin::<ModelMaterial>::default());
    }
}
