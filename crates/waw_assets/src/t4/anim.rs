//! Decoding of animation keyframes (`XAnimParts`).
//!
//! Bones are listed sorted by rotation type (none, half, full, half
//! constant, full constant). The six data streams are consumed as FIFOs:
//! rotation tracks for every bone in order, then translation tracks grouped
//! by type, each naming its bone. Rotations are absolute local rotations;
//! translations are offsets added to the bind-pose local translation. Frames
//! run `0..=numframes` at `framerate`. Root motion lives in a separate delta
//! part (translation + yaw, cumulative from the start).
//! Layout per OpenAssetTools' XAnim reader (GPL-3.0); verified by exact
//! stream consumption over every animation in the game.

use super::{XAnimInfo, ZoneData};

const Q: f32 = 1.0 / 32767.0;

#[derive(Debug, Clone, Default)]
pub struct Track {
    pub bone: String,
    /// `(frame, quaternion x y z w)`; empty = identity (no rotation data).
    pub rot: Vec<(u16, [f32; 4])>,
    /// `(frame, offset from the bind pose)`; empty = no offset.
    pub trans: Vec<(u16, [f32; 3])>,
}

#[derive(Debug, Clone, Default)]
pub struct Clip {
    pub name: String,
    pub numframes: u16,
    pub framerate: f32,
    pub looping: bool,
    pub tracks: Vec<Track>,
    /// Root motion: cumulative translation and yaw (radians) per key.
    pub delta_trans: Vec<(u16, [f32; 3])>,
    pub delta_yaw: Vec<(u16, f32)>,
    /// `(note, time 0..1)`.
    pub notify: Vec<(String, f32)>,
}

impl Clip {
    pub fn duration(&self) -> f32 {
        if self.framerate > 0.0 {
            self.numframes as f32 / self.framerate
        } else {
            0.0
        }
    }

    /// Horizontal root speed over the whole clip (units per second).
    pub fn root_speed(&self) -> f32 {
        let d = self.duration();
        match (self.delta_trans.last(), d > 0.0) {
            (Some((_, t)), true) => (t[0] * t[0] + t[1] * t[1]).sqrt() / d,
            _ => 0.0,
        }
    }

    pub fn track(&self, bone: &str) -> Option<&Track> {
        self.tracks.iter().find(|t| t.bone.eq_ignore_ascii_case(bone))
    }

    pub fn notify_time(&self, note: &str) -> Option<f32> {
        self.notify.iter().find(|(n, _)| n == note).map(|(_, t)| *t)
    }
}

/// Keys around `frame`: (index before, index after, blend 0..1).
fn bracket(frames: impl Iterator<Item = u16> + Clone, n: usize, frame: f32) -> (usize, usize, f32) {
    if n <= 1 {
        return (0, 0, 0.0);
    }
    let mut prev = 0;
    for (i, f) in frames.enumerate() {
        if f as f32 > frame {
            if i == 0 {
                return (0, 0, 0.0);
            }
            return (prev, i, 0.0);
        }
        prev = i;
    }
    (n - 1, n - 1, 0.0)
}

/// Linear sample of `(frame, value)` keys at a fractional frame.
fn sample<const N: usize>(keys: &[(u16, [f32; N])], frame: f32, shortest: bool) -> Option<[f32; N]> {
    if keys.is_empty() {
        return None;
    }
    let (a, b, _) = bracket(keys.iter().map(|k| k.0), keys.len(), frame);
    let (fa, va) = keys[a];
    let (fb, mut vb) = keys[b];
    if a == b || fb <= fa {
        return Some(va);
    }
    let t = ((frame - fa as f32) / (fb - fa) as f32).clamp(0.0, 1.0);
    if shortest && va.iter().zip(vb.iter()).map(|(x, y)| x * y).sum::<f32>() < 0.0 {
        vb.iter_mut().for_each(|v| *v = -*v);
    }
    let mut out = [0.0; N];
    for i in 0..N {
        out[i] = va[i] + (vb[i] - va[i]) * t;
    }
    Some(out)
}

impl Track {
    /// Local rotation at a fractional frame (normalised), identity if none.
    pub fn rotation(&self, frame: f32) -> [f32; 4] {
        match sample(&self.rot, frame, true) {
            Some(q) => {
                let l = (q.iter().map(|c| c * c).sum::<f32>()).sqrt().max(1e-6);
                [q[0] / l, q[1] / l, q[2] / l, q[3] / l]
            }
            None => [0.0, 0.0, 0.0, 1.0],
        }
    }

    /// Translation offset from the bind pose at a fractional frame.
    pub fn offset(&self, frame: f32) -> [f32; 3] {
        sample(&self.trans, frame, false).unwrap_or([0.0; 3])
    }
}

impl Clip {
    pub fn root_trans(&self, frame: f32) -> [f32; 3] {
        sample(&self.delta_trans, frame, false).unwrap_or([0.0; 3])
    }

    pub fn root_yaw(&self, frame: f32) -> f32 {
        sample(&self.delta_yaw.iter().map(|(f, y)| (*f, [*y])).collect::<Vec<_>>(), frame, false).map(|v| v[0]).unwrap_or(0.0)
    }
}

/// FIFO over one little-endian data stream.
struct Cursor<'a> {
    d: &'a [u8],
    esz: usize,
    n: usize,
    p: usize,
}

impl<'a> Cursor<'a> {
    fn new(zone: &'a ZoneData, s: super::Stream, esz: usize) -> Cursor<'a> {
        let d = s.fpos.and_then(|p| zone.data.get(p..p + esz * s.count)).unwrap_or(&[]);
        Cursor { d, esz, n: d.len() / esz, p: 0 }
    }
    fn take(&mut self) -> Result<u32, String> {
        if self.p >= self.n {
            return Err("stream exhausted".into());
        }
        let o = self.p * self.esz;
        self.p += 1;
        Ok(match self.esz {
            1 => self.d[o] as u32,
            2 => u16::from_le_bytes([self.d[o], self.d[o + 1]]) as u32,
            _ => u32::from_le_bytes([self.d[o], self.d[o + 1], self.d[o + 2], self.d[o + 3]]),
        })
    }
    fn i16(&mut self) -> Result<f32, String> {
        Ok(self.take()? as u16 as i16 as f32)
    }
    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_bits(self.take()?))
    }
    fn done(&self) -> bool {
        self.p == self.n
    }
}

/// Decodes an animation's keyframes. Fails if the streams are not consumed
/// exactly (which would mean a format mismatch).
pub fn decode(zone: &ZoneData, a: &XAnimInfo) -> Result<Clip, String> {
    let byte_idx = a.numframes < 256;
    let mut db = Cursor::new(zone, a.data_byte, 1);
    let mut ds = Cursor::new(zone, a.data_short, 2);
    let mut di = Cursor::new(zone, a.data_int, 4);
    let mut rs = Cursor::new(zone, a.random_data_short, 2);
    let mut rb = Cursor::new(zone, a.random_data_byte, 1);
    let mut ix = Cursor::new(zone, a.indices, if byte_idx { 1 } else { 2 });

    let mut indices = |n: usize, db: &mut Cursor, ds: &mut Cursor| -> Result<Vec<u16>, String> {
        if byte_idx {
            return (0..n).map(|_| db.take().map(|v| v as u16)).collect();
        }
        if n - 1 >= 64 {
            let v: Vec<u16> = (0..n).map(|_| ix.take().map(|v| v as u16)).collect::<Result<_, _>>()?;
            for _ in 0..(n - 2) / 256 + 2 {
                ds.take()?; // checkpoint table for the game's binary search
            }
            return Ok(v);
        }
        (0..n).map(|_| ds.take().map(|v| v as u16)).collect()
    };

    let nb = a.bone_counts[9] as usize;
    let mut tracks: Vec<Track> = a.bones.iter().take(nb).map(|b| Track { bone: b.clone(), ..Default::default() }).collect();
    tracks.resize_with(nb, Track::default);
    let mut bi = 0;
    for qt in 0..5 {
        for _ in 0..a.bone_counts[qt] {
            let t = tracks.get_mut(bi).ok_or("bone count mismatch")?;
            match qt {
                1 | 2 => {
                    let n = ds.take()? as usize + 1;
                    let idx = indices(n, &mut db, &mut ds)?;
                    for f in idx {
                        let q = if qt == 1 { [0.0, 0.0, rs.i16()? * Q, rs.i16()? * Q] } else { [rs.i16()? * Q, rs.i16()? * Q, rs.i16()? * Q, rs.i16()? * Q] };
                        t.rot.push((f, q));
                    }
                }
                3 => t.rot.push((0, [0.0, 0.0, ds.i16()? * Q, ds.i16()? * Q])),
                4 => t.rot.push((0, [ds.i16()? * Q, ds.i16()? * Q, ds.i16()? * Q, ds.i16()? * Q])),
                _ => {}
            }
            bi += 1;
        }
    }
    for tt in 5..9 {
        for _ in 0..a.bone_counts[tt] {
            let b = db.take()? as usize;
            match tt {
                5 | 6 => {
                    let n = ds.take()? as usize + 1;
                    let mins = [di.f32()?, di.f32()?, di.f32()?];
                    let size = [di.f32()?, di.f32()?, di.f32()?];
                    let idx = indices(n, &mut db, &mut ds)?;
                    let src = if tt == 5 { &mut rb } else { &mut rs };
                    let mut keys = Vec::with_capacity(n);
                    for f in idx {
                        let mut v = [0.0; 3];
                        for c in 0..3 {
                            v[c] = mins[c] + size[c] * src.take()? as f32;
                        }
                        keys.push((f, v));
                    }
                    if let Some(t) = tracks.get_mut(b) {
                        t.trans = keys;
                    }
                }
                7 => {
                    let v = [di.f32()?, di.f32()?, di.f32()?];
                    if let Some(t) = tracks.get_mut(b) {
                        t.trans = vec![(0, v)];
                    }
                }
                _ => {}
            }
        }
    }
    if !(db.done() && ds.done() && di.done() && rs.done() && rb.done() && ix.done()) {
        return Err(format!("{}: streams not fully consumed", a.name));
    }

    // Root motion.
    let z = &zone.data;
    let f32_at = |p: usize| f32::from_le_bytes([z[p], z[p + 1], z[p + 2], z[p + 3]]);
    let u16_at = |p: usize| u16::from_le_bytes([z[p], z[p + 1]]);
    let idx_at = |p: usize, i: usize| if byte_idx { z[p + i] as u16 } else { u16_at(p + 2 * i) };
    let mut delta_trans = Vec::new();
    if let Some(t) = &a.delta_trans {
        if t.size == 0 {
            delta_trans.push((0, [f32_at(t.fpos + 4), f32_at(t.fpos + 8), f32_at(t.fpos + 12)]));
        } else if let Some(fp) = t.frames_fpos {
            let n = t.size as usize + 1;
            let mins = [f32_at(t.fpos + 4), f32_at(t.fpos + 8), f32_at(t.fpos + 12)];
            let size = [f32_at(t.fpos + 16), f32_at(t.fpos + 20), f32_at(t.fpos + 24)];
            for j in 0..n {
                let f = idx_at(t.fpos + 32, j);
                let mut v = [0.0; 3];
                for c in 0..3 {
                    let raw = if t.small { z[fp + 3 * j + c] as f32 } else { u16_at(fp + 2 * (3 * j + c)) as f32 };
                    v[c] = mins[c] + size[c] * raw;
                }
                delta_trans.push((f, v));
            }
        }
    }
    let mut delta_yaw = Vec::new();
    if let Some(q) = &a.delta_quat {
        let yaw = |p: usize| {
            let (qz, qw) = (u16_at(p) as i16 as f32, u16_at(p + 2) as i16 as f32);
            2.0 * qz.atan2(qw)
        };
        if q.size == 0 {
            delta_yaw.push((0, yaw(q.fpos + 4)));
        } else if let Some(fp) = q.frames_fpos {
            for j in 0..q.size as usize + 1 {
                delta_yaw.push((idx_at(q.fpos + 8, j), yaw(fp + 4 * j)));
            }
        }
    }
    Ok(Clip {
        name: a.name.clone(),
        numframes: a.numframes,
        framerate: a.framerate,
        looping: a.looping,
        tracks,
        delta_trans,
        delta_yaw,
        notify: a.notify.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampling_interpolates_and_clamps() {
        let keys = vec![(0u16, [0.0f32, 0.0, 0.0]), (10, [10.0, 0.0, 0.0]), (20, [10.0, 10.0, 0.0])];
        assert_eq!(sample(&keys, 5.0, false), Some([5.0, 0.0, 0.0]));
        assert_eq!(sample(&keys, 15.0, false), Some([10.0, 5.0, 0.0]));
        assert_eq!(sample(&keys, 25.0, false), Some([10.0, 10.0, 0.0]));
        assert_eq!(sample(&keys, 0.0, false), Some([0.0, 0.0, 0.0]));
        let t = Track { bone: "b".into(), rot: vec![(0, [0.0, 0.0, 0.0, 1.0]), (2, [0.0, 0.0, 0.0, -1.0])], trans: vec![] };
        // Opposite-sign quaternions are the same rotation: no blend through zero.
        assert_eq!(t.rotation(1.0), [0.0, 0.0, 0.0, 1.0]);
    }

    /// Real-data check: every Nacht animation decodes with exact consumption.
    #[test]
    #[ignore]
    fn decodes_all_nacht_anims() {
        let root = std::env::var("UNDEAD_WAW").expect("set UNDEAD_WAW");
        let ff = std::fs::read(std::path::Path::new(&root).join("zone/english/nazi_zombie_prototype.ff")).unwrap();
        let zd = super::super::walk(crate::zone::decompress(&ff).unwrap());
        assert_eq!(zd.xanims.len(), 364);
        let mut keys = 0;
        for a in &zd.xanims {
            let c = decode(&zd, a).unwrap_or_else(|e| panic!("{e}"));
            keys += c.tracks.iter().map(|t| t.rot.len() + t.trans.len()).sum::<usize>();
        }
        assert!(keys > 400_000, "{keys}");
        let walk = decode(&zd, zd.xanim("ai_zombie_walk_v1").unwrap()).unwrap();
        assert_eq!(walk.numframes, 124);
        assert!((walk.root_trans(124.0)[0] - 156.0).abs() < 2.0, "{:?}", walk.root_trans(124.0));
        assert!(walk.looping);
    }
}
