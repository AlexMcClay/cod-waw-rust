//! Positional sound: our own stereo panning for 3D sounds.
//!
//! Bevy's spatial audio (rodio's `Spatial`) turns the louder side the wrong
//! way and pans at most 2:1, so every sound seemed to come from the same
//! place. A positional voice here is a mono copy of the sound played as
//! stereo with a left and right gain, recomputed every frame from where the
//! sound is relative to the player's view: constant-power panning, a little
//! quieter behind, and the game's linear distance falloff.

use bevy::audio::{Decodable, Source};
use bevy::prelude::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// A voice's left and right gains (f32 bits), shared with the audio thread.
#[derive(Default)]
pub struct PanGains {
    l: AtomicU32,
    r: AtomicU32,
}

impl PanGains {
    pub fn set(&self, l: f32, r: f32) {
        self.l.store(l.to_bits(), Ordering::Relaxed);
        self.r.store(r.to_bits(), Ordering::Relaxed);
    }
    fn get(&self) -> (f32, f32) {
        (f32::from_bits(self.l.load(Ordering::Relaxed)), f32::from_bits(self.r.load(Ordering::Relaxed)))
    }
}

/// A sound played through [`PanGains`].
#[derive(Asset, TypePath, Clone)]
pub struct PannedSound {
    pub mono: Arc<[f32]>,
    pub rate: u32,
    pub gains: Arc<PanGains>,
    /// Loops until its entity is despawned.
    pub looping: bool,
}

pub struct PannedDecoder {
    sound: PannedSound,
    pos: usize,
    gl: f32,
    gr: f32,
    right: Option<f32>,
}

impl Decodable for PannedSound {
    type DecoderItem = f32;
    type Decoder = PannedDecoder;

    fn decoder(&self) -> PannedDecoder {
        let (gl, gr) = self.gains.get();
        PannedDecoder { sound: self.clone(), pos: 0, gl, gr, right: None }
    }
}

impl Iterator for PannedDecoder {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if let Some(r) = self.right.take() {
            return Some(r);
        }
        let s = &self.sound;
        if self.pos >= s.mono.len() {
            if !s.looping || s.mono.is_empty() {
                return None;
            }
            self.pos = 0;
        }
        let m = s.mono[self.pos];
        self.pos += 1;
        // Glide to the latest gains (~5 ms) so moving sounds don't click.
        let (tl, tr) = s.gains.get();
        let k = 1.0 / (0.005 * s.rate as f32).max(1.0);
        self.gl += (tl - self.gl) * k;
        self.gr += (tr - self.gr) * k;
        self.right = Some(m * self.gr);
        Some(m * self.gl)
    }
}

impl Source for PannedDecoder {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        2
    }
    fn sample_rate(&self) -> u32 {
        self.sound.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        (!self.sound.looping).then(|| Duration::from_secs_f32(self.sound.mono.len() as f32 / self.sound.rate.max(1) as f32))
    }
}

/// Mono copies of sounds, made once per sound.
#[derive(Resource, Default)]
pub struct MonoCache(HashMap<AssetId<AudioSource>, Option<(Arc<[f32]>, u32)>>);

impl MonoCache {
    pub fn get(&mut self, handle: &Handle<AudioSource>, sources: &Assets<AudioSource>) -> Option<(Arc<[f32]>, u32)> {
        self.0
            .entry(handle.id())
            .or_insert_with(|| {
                let pcm = zm_core::wav::decode(&sources.get(handle)?.bytes).ok()?;
                let ch = pcm.channels.max(1) as usize;
                let mono: Vec<f32> = pcm.samples.chunks(ch).map(|f| f.iter().map(|&s| s as f32 / 32768.0).sum::<f32>() / ch as f32).collect();
                Some((mono.into(), pcm.sample_rate))
            })
            .clone()
    }
}

/// The gains of a positional voice on an entity.
#[derive(Component)]
pub struct Pan(pub Arc<PanGains>);

/// Left/right gains for a sound at `at` heard from `ear` (the camera):
/// constant-power pan by the direction in the view's horizontal plane
/// (centred when very close), a little quieter from behind, times `gain`.
pub fn gains(ear: &GlobalTransform, at: Vec3, gain: f32) -> (f32, f32) {
    let to = at - ear.translation();
    let d = to.length();
    let local = ear.rotation().inverse() * to;
    let flat = Vec2::new(local.x, local.z);
    let len = flat.length();
    // -1 left .. 1 right; sounds right at the listener stay centred.
    let pan = if len > 1e-3 { local.x / len } else { 0.0 } * (d / 1.5).clamp(0.0, 1.0);
    let a = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
    let (l, r) = ((a.cos() * std::f32::consts::SQRT_2).min(1.0), (a.sin() * std::f32::consts::SQRT_2).min(1.0));
    // Behind (+Z is backwards for a camera): up to 30 % quieter.
    let behind = if len > 1e-3 { (local.z / len).max(0.0) } else { 0.0 };
    let g = gain * (1.0 - 0.3 * behind);
    (l * g, r * g)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pans_to_the_side_the_sound_is_on() {
        let ear = GlobalTransform::default(); // looking down -Z, right is +X
        let (l, r) = gains(&ear, Vec3::new(5.0, 0.0, 0.0), 1.0);
        assert!(r > 0.99 && l < 0.01, "{l} {r}");
        let (l, r) = gains(&ear, Vec3::new(-5.0, 0.0, 0.0), 1.0);
        assert!(l > 0.99 && r < 0.01, "{l} {r}");
        let (l, r) = gains(&ear, Vec3::new(0.0, 0.0, -5.0), 1.0);
        assert!((l - r).abs() < 1e-4 && l > 0.99, "{l} {r}");
        let (bl, br) = gains(&ear, Vec3::new(0.0, 0.0, 5.0), 1.0);
        assert!(bl < l && (bl - br).abs() < 1e-4);
        // Turned around, the same sound is on the other side.
        let turned = GlobalTransform::from(Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::PI)));
        let (l, r) = gains(&turned, Vec3::new(5.0, 0.0, 0.0), 1.0);
        assert!(l > 0.99 && r < 0.01);
    }
}
