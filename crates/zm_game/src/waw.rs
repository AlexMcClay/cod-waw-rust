//! Bridge between `waw_assets` (the user's World at War install) and Bevy:
//! locating the install, turning IWI images into textures and IWD sounds into
//! audio sources. Everything degrades gracefully when no install is found.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, WgpuFeatures};
use bevy::render::renderer::RenderDevice;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use waw_assets::iwi::{Format, Iwi};
use waw_assets::{Install, Iwd};
use zm_core::wav;

/// The located install (if any) and its merged IWD file system.
#[derive(Resource, Clone)]
pub struct Waw {
    pub install: Option<Install>,
    pub iwd: Option<Arc<Iwd>>,
    /// Human-readable reason when the install could not be used.
    pub error: Option<String>,
}

impl Waw {
    /// Search order: `--waw <dir>`, `UNDEAD_WAW`, `waw_path=` in `undead.cfg`,
    /// then Steam libraries and common install folders.
    pub fn locate(cfg: &crate::settings::LaunchConfig) -> Waw {
        let args: Vec<String> = std::env::args().collect();
        let mut explicit: Vec<PathBuf> = Vec::new();
        if let Some(i) = args.iter().position(|a| a == "--waw") {
            if let Some(p) = args.get(i + 1) {
                explicit.push(PathBuf::from(p));
            }
        }
        if let Ok(p) = std::env::var("UNDEAD_WAW") {
            explicit.push(PathBuf::from(p));
        }
        if let Some(p) = &cfg.waw_path {
            explicit.push(p.clone());
        }
        match Install::locate(&explicit) {
            Ok(install) => {
                info!("World at War install: {}", install.root.display());
                match Iwd::open(&install.main_dir()) {
                    Ok(iwd) => {
                        info!("Indexed {} files from IWD archives", iwd.len());
                        Waw { install: Some(install), iwd: Some(Arc::new(iwd)), error: None }
                    }
                    Err(e) => Waw { install: Some(install), iwd: None, error: Some(format!("could not read IWD archives: {e}")) },
                }
            }
            Err(e) => {
                warn!("{e}");
                Waw { install: None, iwd: None, error: Some("World at War install not found - set waw_path in undead.cfg".into()) }
            }
        }
    }

    pub fn available(&self) -> bool {
        self.install.is_some() && self.iwd.is_some()
    }

    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.iwd.as_ref()?.read(path)
    }
}

/// Cache of textures created from IWI images, keyed by image name.
#[derive(Resource, Default)]
pub struct WawImages {
    cache: HashMap<String, Option<Handle<Image>>>,
}

/// Whether textures should stay block-compressed on the GPU.
pub fn bc_supported(device: Option<&RenderDevice>) -> bool {
    device.is_some_and(|d| d.features().contains(WgpuFeatures::TEXTURE_COMPRESSION_BC))
}

/// Converts a parsed IWI into a Bevy image with its full mip chain.
pub fn iwi_to_image(iwi: &Iwi, srgb: bool, repeat: bool, bc: bool) -> Image {
    let compressed = bc && iwi.format.is_compressed() && iwi.width % 4 == 0 && iwi.height % 4 == 0;
    let (format, data, levels) = if compressed {
        let format = match (iwi.format, srgb) {
            (Format::Dxt1, true) => TextureFormat::Bc1RgbaUnormSrgb,
            (Format::Dxt1, false) => TextureFormat::Bc1RgbaUnorm,
            (Format::Dxt3, true) => TextureFormat::Bc2RgbaUnormSrgb,
            (Format::Dxt3, false) => TextureFormat::Bc2RgbaUnorm,
            (_, true) => TextureFormat::Bc3RgbaUnormSrgb,
            (_, false) => TextureFormat::Bc3RgbaUnorm,
        };
        (format, iwi.levels.concat(), iwi.levels.len())
    } else {
        let format = if srgb { TextureFormat::Rgba8UnormSrgb } else { TextureFormat::Rgba8Unorm };
        let mut data = Vec::new();
        for l in 0..iwi.levels.len() {
            data.extend_from_slice(&iwi.level_rgba(l).2);
        }
        (format, data, iwi.levels.len())
    };
    let mut image = Image::new_fill(
        Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.size = Extent3d { width: iwi.width, height: iwi.height, depth_or_array_layers: 1 };
    image.texture_descriptor.format = format;
    image.texture_descriptor.mip_level_count = levels as u32;
    image.data = Some(data);
    let mode = if repeat { ImageAddressMode::Repeat } else { ImageAddressMode::ClampToEdge };
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: mode,
        address_mode_v: mode,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: if levels > 1 { 8 } else { 1 },
        ..default()
    });
    image
}

impl WawImages {
    /// Loads `images/<name>.iwi` as a texture (cached; `None` if unavailable).
    pub fn get(
        &mut self,
        waw: &Waw,
        images: &mut Assets<Image>,
        device: Option<&RenderDevice>,
        name: &str,
        srgb: bool,
        repeat: bool,
    ) -> Option<Handle<Image>> {
        if let Some(h) = self.cache.get(name) {
            return h.clone();
        }
        let handle = waw
            .iwd
            .as_ref()
            .and_then(|iwd| iwd.read_image(name))
            .and_then(|bytes| match Iwi::parse(&bytes) {
                Ok(iwi) => Some(iwi),
                Err(e) => {
                    warn!("image {name}: {e}");
                    None
                }
            })
            .map(|iwi| images.add(iwi_to_image(&iwi, srgb, repeat, bc_supported(device))));
        self.cache.insert(name.to_string(), handle.clone());
        handle
    }
}

/// Decodes a WAV from the IWD archives into a playable source.
pub fn load_wav(waw: &Waw, path: &str) -> Option<AudioSource> {
    let bytes = waw.read(path)?;
    match wav::to_pcm_wav(&bytes) {
        Ok(pcm) => Some(AudioSource { bytes: pcm.into() }),
        Err(e) => {
            warn!("sound {path}: {e}");
            None
        }
    }
}
