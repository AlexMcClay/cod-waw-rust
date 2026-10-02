#![allow(dead_code)] // ShaderType derives emit unused size checks per field.
//! World at War's image: the map's volumetric fog and its vision set's
//! film grade, as one fullscreen pass after tonemapping (see
//! `postfx.wgsl`). The values come from the map's art script
//! (`SetVolFog`/`SetExpFog`) and the vision set it names (see
//! `waw_assets::look`).

use bevy::asset::{load_internal_asset, weak_handle};
use bevy::core_pipeline::core_3d::graph::{Core3d, Node3d};
use bevy::core_pipeline::fullscreen_vertex_shader::fullscreen_shader_vertex_state;
use bevy::core_pipeline::prepass::ViewPrepassTextures;
use bevy::ecs::query::QueryItem;
use bevy::prelude::*;
use bevy::render::extract_component::{ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin, UniformComponentPlugin};
use bevy::render::render_graph::{NodeRunError, RenderGraphApp, RenderGraphContext, RenderLabel, ViewNode, ViewNodeRunner};
use bevy::render::render_resource::binding_types::{sampler, texture_2d, texture_depth_2d, uniform_buffer};
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderContext, RenderDevice};
use bevy::render::view::{ViewTarget, ViewUniform, ViewUniformOffset, ViewUniforms};
use bevy::render::RenderApp;

const SHADER: Handle<Shader> = weak_handle!("6b1f6d6e-58a4-4a0c-9d43-2f5d1c7a9e11");

/// Fog and film settings for a camera (game units already in metres).
#[allow(dead_code)] // ShaderType's generated size checks
#[derive(Component, Clone, Copy, ExtractComponent, ShaderType)]
pub struct WawPost {
    /// Start, half-way distance, half height, base height.
    pub fog: Vec4,
    /// Gamma-space colour; w = enabled.
    pub fog_color: Vec4,
    /// Contrast, brightness, desaturation; w = enabled.
    pub film: Vec4,
    /// w = invert.
    pub light_tint: Vec4,
    /// w = the player's brightness as a display gamma.
    pub dark_tint: Vec4,
}

impl WawPost {
    /// The map's fog and vision film grade as its own data defines them
    /// (see `waw_assets::look`), in metres. `gamma` is the player's
    /// brightness setting (1 = as the game).
    pub fn from_look(fog: Option<waw_assets::look::Fog>, film: Option<waw_assets::look::Film>, gamma: f32) -> Self {
        let u = 0.0254;
        let mut post = WawPost::off(gamma);
        if let Some(f) = fog {
            post.fog = Vec4::new(f.start * u, f.halfway * u, f.half_height * u, f.base_height * u);
            post.fog_color = Vec4::new(f.color[0], f.color[1], f.color[2], 1.0);
        }
        if let Some(f) = film.filter(|f| f.enabled) {
            post.film = Vec4::new(f.contrast, f.brightness, f.desaturation, 1.0);
            post.light_tint = Vec3::from(f.light_tint).extend(if f.invert { 1.0 } else { 0.0 });
            post.dark_tint = Vec3::from(f.dark_tint).extend(gamma);
        }
        post
    }

    /// No fog or grading (the prototype map).
    pub fn off(gamma: f32) -> Self {
        WawPost { fog: Vec4::ZERO, fog_color: Vec4::ZERO, film: Vec4::ZERO, light_tint: Vec4::new(1.0, 1.0, 1.0, 0.0), dark_tint: Vec4::new(1.0, 1.0, 1.0, gamma) }
    }

    /// Changes only the brightness gamma.
    pub fn set_gamma(&mut self, gamma: f32) {
        self.dark_tint.w = gamma;
    }
}

pub struct PostFxPlugin;

impl Plugin for PostFxPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, SHADER, "postfx.wgsl", Shader::from_wgsl);
        app.add_plugins((ExtractComponentPlugin::<WawPost>::default(), UniformComponentPlugin::<WawPost>::default()));
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app
            .add_render_graph_node::<ViewNodeRunner<WawPostNode>>(Core3d, WawPostLabel)
            .add_render_graph_edges(Core3d, (Node3d::Tonemapping, WawPostLabel, Node3d::EndMainPassPostProcessing));
    }

    fn finish(&self, app: &mut App) {
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.init_resource::<WawPostPipeline>();
        }
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct WawPostLabel;

#[derive(Default)]
struct WawPostNode;

impl ViewNode for WawPostNode {
    type ViewQuery = (
        &'static ViewTarget,
        &'static ViewPrepassTextures,
        &'static ViewUniformOffset,
        &'static DynamicUniformIndex<WawPost>,
    );

    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (target, prepass, view_offset, settings_index): QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let pipe = world.resource::<WawPostPipeline>();
        let Some(pipeline) = world.resource::<PipelineCache>().get_render_pipeline(pipe.pipeline) else { return Ok(()) };
        let (Some(settings), Some(views), Some(depth)) = (
            world.resource::<ComponentUniforms<WawPost>>().uniforms().binding(),
            world.resource::<ViewUniforms>().uniforms.binding(),
            prepass.depth_view(),
        ) else {
            return Ok(());
        };
        let post = target.post_process_write();
        let bind_group = render_context.render_device().create_bind_group(
            "waw_post_bind_group",
            &pipe.layout,
            &BindGroupEntries::sequential((post.source, &pipe.sampler, depth, views, settings)),
        );
        let mut pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
            label: Some("waw_post_pass"),
            color_attachments: &[Some(RenderPassColorAttachment { view: post.destination, resolve_target: None, ops: Operations::default() })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_render_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[view_offset.offset, settings_index.index()]);
        pass.draw(0..3, 0..1);
        Ok(())
    }
}

#[derive(Resource)]
struct WawPostPipeline {
    layout: BindGroupLayout,
    sampler: Sampler,
    pipeline: CachedRenderPipelineId,
}

impl FromWorld for WawPostPipeline {
    fn from_world(world: &mut World) -> Self {
        let device = world.resource::<RenderDevice>();
        let layout = device.create_bind_group_layout(
            "waw_post_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (
                    texture_2d(TextureSampleType::Float { filterable: true }),
                    sampler(SamplerBindingType::Filtering),
                    texture_depth_2d(),
                    uniform_buffer::<ViewUniform>(true),
                    uniform_buffer::<WawPost>(true),
                ),
            ),
        );
        let sampler = device.create_sampler(&SamplerDescriptor::default());
        let pipeline = world.resource_mut::<PipelineCache>().queue_render_pipeline(RenderPipelineDescriptor {
            label: Some("waw_post_pipeline".into()),
            layout: vec![layout.clone()],
            vertex: fullscreen_shader_vertex_state(),
            fragment: Some(FragmentState {
                shader: SHADER,
                shader_defs: vec![],
                entry_point: "fragment".into(),
                targets: vec![Some(ColorTargetState { format: TextureFormat::bevy_default(), blend: None, write_mask: ColorWrites::ALL })],
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            push_constant_ranges: vec![],
            zero_initialize_workgroup_memory: false,
        });
        WawPostPipeline { layout, sampler, pipeline }
    }
}
