//! The zone walker: block/stream emulation plus one loader per asset type.
//!
//! Stream rules (as the game's loader applies them):
//! * Every read goes into the block on top of the block stack and advances
//!   that block's offset. TEMP, VIRTUAL, LARGE and PHYSICAL reads consume
//!   file bytes; the RUNTIME blocks only reserve memory and are skipped here.
//! * `alloc(n)` aligns the current block offset; the file is never padded.
//! * TEMP's offset is restored when it is popped.
//! * Pointer values: 0 = null, -1 = data follows inline, -2 = follows inline
//!   and reserve a pointer slot in VIRTUAL, anything else = `(block << 29 |
//!   offset) + 1`, a reference to memory loaded earlier.

use super::*;
use std::collections::HashMap;

const TEMP: usize = 0;
const VIRTUAL: usize = 4;
const LARGE: usize = 5;
const RAW: usize = 7;
const FOLLOW: u32 = 0xFFFF_FFFF;
const INSERT: u32 = 0xFFFF_FFFE;

/// A struct read from the stream: its bytes and where it lives in block memory.
#[derive(Clone, Copy)]
struct Rec<'a> {
    d: &'a [u8],
    blk: usize,
    boff: u32,
}

impl<'a> Rec<'a> {
    fn u8(&self, o: usize) -> u8 {
        self.d[o]
    }
    fn u16(&self, o: usize) -> u16 {
        u16::from_le_bytes([self.d[o], self.d[o + 1]])
    }
    fn i16(&self, o: usize) -> i16 {
        self.u16(o) as i16
    }
    fn u32(&self, o: usize) -> u32 {
        u32::from_le_bytes([self.d[o], self.d[o + 1], self.d[o + 2], self.d[o + 3]])
    }
    fn i32(&self, o: usize) -> i32 {
        self.u32(o) as i32
    }
    fn f32(&self, o: usize) -> f32 {
        f32::from_bits(self.u32(o))
    }
    fn vec3(&self, o: usize) -> [f32; 3] {
        [self.f32(o), self.f32(o + 4), self.f32(o + 8)]
    }
    /// Element `i` of an array of `size`-byte structs.
    fn elem(&self, i: usize, size: usize) -> Rec<'a> {
        self.sub(i * size, size)
    }
    fn sub(&self, o: usize, len: usize) -> Rec<'a> {
        Rec { d: &self.d[o..o + len], blk: self.blk, boff: self.boff + o as u32 }
    }
    /// Block location of a field (for pointer slots), if in a normal block.
    fn loc(&self, o: usize) -> Option<(usize, u32)> {
        (VIRTUAL..RAW).contains(&self.blk).then_some((self.blk, self.boff + o as u32))
    }
}

#[derive(Debug)]
struct Eof;

type R<T> = Result<T, Eof>;

struct Walker<'a> {
    z: &'a [u8],
    pos: usize,
    off: [u32; 7],
    stack: Vec<usize>,
    temp_saved: Vec<u32>,
    /// Normal-block segments `(block offset, file position, length)` for
    /// resolving data back-references.
    segs: [Vec<(u32, u32, u32)>; 7],
    /// Pointer slots holding an asset (`(block, offset)` -> asset).
    slots: HashMap<(usize, u32), AssetRef>,
    tex_tables: HashMap<u32, Vec<TexDef>>,
    alias_heads: HashMap<u32, Vec<SoundAlias>>,
    sound_files: HashMap<u32, SoundFile>,
    rigid_lists: HashMap<u32, Vec<RigidVertList>>,
    snd_names: HashMap<u32, String>,
    techsets: Vec<String>,
    out: ZoneData,
}

fn decode_ptr(v: u32) -> (usize, u32) {
    let v = v.wrapping_sub(1);
    ((v >> 29) as usize, v & 0x1FFF_FFFF)
}

/// Walks a decompressed zone (see [`crate::zone::decompress`]).
pub fn walk(data: Vec<u8>) -> ZoneData {
    // The walker borrows the bytes while filling `out`; move them in after.
    let mut out = {
        let mut w = Walker {
            z: &data,
            pos: 36,
            off: [0; 7],
            stack: Vec::new(),
            temp_saved: Vec::new(),
            segs: Default::default(),
            slots: HashMap::new(),
            tex_tables: HashMap::new(),
            alias_heads: HashMap::new(),
            sound_files: HashMap::new(),
            rigid_lists: HashMap::new(),
            snd_names: HashMap::new(),
            techsets: Vec::new(),
            out: ZoneData::default(),
        };
        if data.len() >= 36 {
            w.out.virtual_expected = u32::from_le_bytes([data[24], data[25], data[26], data[27]]);
            if w.run().is_err() && w.out.stopped.is_none() {
                w.out.stopped = Some(format!("unexpected end of zone at {}", w.pos));
            }
        } else {
            w.out.stopped = Some("zone too small".into());
        }
        w.out.end_pos = w.pos;
        w.out.virtual_end = w.off[VIRTUAL];
        std::mem::take(&mut w.out)
    };
    out.data = data;
    out
}

impl<'a> Walker<'a> {
    // ------------------------------------------------------------------ stream

    fn push(&mut self, b: usize) {
        self.stack.push(b);
        if b == TEMP {
            self.temp_saved.push(self.off[TEMP]);
        }
    }

    fn pop(&mut self) {
        if self.stack.pop() == Some(TEMP) {
            self.off[TEMP] = self.temp_saved.pop().unwrap_or(0);
        }
    }

    fn top(&self) -> usize {
        self.stack.last().copied().unwrap_or(RAW)
    }

    fn alloc(&mut self, a: u32) {
        let b = self.top();
        if b < RAW && a > 1 {
            self.off[b] = self.off[b].div_ceil(a) * a;
        }
    }

    fn read(&mut self, n: usize) -> R<Rec<'a>> {
        let z: &'a [u8] = self.z;
        if self.pos + n > z.len() {
            return Err(Eof);
        }
        let b = self.top();
        let boff = if b < RAW { self.off[b] } else { 0 };
        if b < RAW {
            self.off[b] += n as u32;
            if b >= VIRTUAL && n > 0 {
                self.segs[b].push((boff, self.pos as u32, n as u32));
            }
        }
        let d = &z[self.pos..self.pos + n];
        self.pos += n;
        Ok(Rec { d, blk: b, boff })
    }

    /// `alloc(align)` + read `len` bytes; returns the file position.
    fn data(&mut self, align: u32, len: usize) -> R<usize> {
        self.alloc(align);
        let p = self.pos;
        self.read(len)?;
        Ok(p)
    }

    /// Inline data behind a non-reusable pointer.
    fn inline(&mut self, ptr: u32, align: u32, len: usize) -> R<Option<usize>> {
        if ptr == 0 {
            return Ok(None);
        }
        self.data(align, len).map(Some)
    }

    /// Data behind a reusable pointer: inline, or a reference to earlier data.
    fn reusable(&mut self, ptr: u32, align: u32, len: usize) -> R<Option<usize>> {
        match ptr {
            0 => Ok(None),
            FOLLOW => self.data(align, len).map(Some),
            v => Ok(self.native(v)),
        }
    }

    /// File position of earlier data referenced by a pointer value.
    fn native(&self, v: u32) -> Option<usize> {
        let (blk, off) = decode_ptr(v);
        let segs = self.segs.get(blk)?;
        let i = segs.partition_point(|s| s.0 <= off).checked_sub(1)?;
        let (bo, fp, n) = segs[i];
        (off < bo + n).then(|| (fp + off - bo) as usize)
    }

    fn insert_slot(&mut self) -> (usize, u32) {
        self.off[VIRTUAL] = self.off[VIRTUAL].div_ceil(4) * 4;
        let loc = (VIRTUAL, self.off[VIRTUAL]);
        self.off[VIRTUAL] += 4;
        loc
    }

    fn cstr_at(&self, fpos: usize) -> String {
        let end = self.z[fpos..].iter().position(|&b| b == 0).map(|e| fpos + e).unwrap_or(self.z.len());
        String::from_utf8_lossy(&self.z[fpos..end]).into_owned()
    }

    /// A `const char*`: inline, or a reference to an earlier string.
    fn xstring(&mut self, ptr: u32) -> R<Option<String>> {
        match ptr {
            0 => Ok(None),
            FOLLOW => {
                self.alloc(1);
                let end = self.z[self.pos..].iter().position(|&b| b == 0).ok_or(Eof)?;
                let r = self.read(end + 1)?;
                Ok(Some(String::from_utf8_lossy(&r.d[..end]).into_owned()))
            }
            v => Ok(self.native(v).map(|p| self.cstr_at(p))),
        }
    }

    fn lookup_alias(&mut self, v: u32) -> Option<AssetRef> {
        let r = self.slots.get(&decode_ptr(v)).copied();
        if r.is_none() {
            self.out.unresolved += 1;
        }
        r
    }

    // ------------------------------------------------------------------ assets

    /// Generic asset pointer; `loc` is where the pointer itself lives.
    fn asset(&mut self, ty: AssetType, ptr: u32, loc: Option<(usize, u32)>) -> R<Option<AssetRef>> {
        if ptr == 0 {
            return Ok(None);
        }
        let r = if ty == AssetType::StringTable {
            // String tables are not loaded through TEMP.
            if ptr == FOLLOW {
                self.alloc(4);
                Some(self.load(ty)?)
            } else {
                Some(AssetRef::Other(ty))
            }
        } else {
            self.push(TEMP);
            let r = if ptr == FOLLOW || ptr == INSERT {
                self.alloc(4);
                let slot = (ptr == INSERT).then(|| self.insert_slot());
                let r = self.load(ty)?;
                if let Some(s) = slot {
                    self.slots.insert(s, r);
                }
                Some(r)
            } else {
                self.lookup_alias(ptr)
            };
            self.pop();
            r
        };
        if let (Some(l), Some(r)) = (loc, r) {
            self.slots.insert(l, r);
        }
        Ok(r)
    }

    fn asset_idx(&mut self, ty: AssetType, ptr: u32, loc: Option<(usize, u32)>) -> R<Option<u32>> {
        Ok(match self.asset(ty, ptr, loc)? {
            Some(AssetRef::Image(i) | AssetRef::Material(i) | AssetRef::XModel(i) | AssetRef::Sound(i) | AssetRef::LoadedSound(i) | AssetRef::Weapon(i)) => Some(i),
            _ => None,
        })
    }

    fn load(&mut self, ty: AssetType) -> R<AssetRef> {
        use AssetType::*;
        let (r, name) = match ty {
            PhysPreset => self.phys_preset()?,
            PhysConstraints => self.phys_constraints()?,
            DestructibleDef => self.destructible()?,
            XAnim => self.xanim()?,
            XModel => self.xmodel()?,
            Material => self.material()?,
            TechniqueSet => self.techset()?,
            Image => self.image()?,
            Sound => self.sound()?,
            LoadedSound => self.loaded_sound()?,
            ClipMap => self.clipmap()?,
            ComWorld => self.comworld()?,
            GameWorldSp => self.gameworld_sp()?,
            MapEnts => self.map_ents()?,
            GfxWorld => self.gfxworld()?,
            LightDef => self.light_def()?,
            Font => self.font()?,
            SndDriverGlobals => self.snd_driver_globals()?,
            Localize => self.localize()?,
            Weapon => self.weapon()?,
            Fx => self.fx()?,
            ImpactFx => self.impact_fx()?,
            RawFile => self.rawfile()?,
            StringTable => self.string_table()?,
        };
        self.out.assets.push((ty, name));
        Ok(r)
    }

    fn run(&mut self) -> R<()> {
        let list = self.read(16)?;
        let string_count = list.u32(0) as usize;
        let strings_ptr = list.u32(4);
        let asset_count = list.u32(8) as usize;
        let assets_ptr = list.u32(12);
        self.push(VIRTUAL);
        if strings_ptr != 0 {
            self.alloc(4);
            let ptrs = self.read(4 * string_count)?;
            for i in 0..string_count {
                let s = self.xstring(ptrs.u32(4 * i))?.unwrap_or_default();
                self.out.script_strings.push(s);
            }
        }
        if assets_ptr != 0 {
            self.alloc(4);
            let table = self.read(8 * asset_count)?;
            for i in 0..asset_count {
                let id = table.u32(8 * i);
                let Some(ty) = AssetType::from_id(id) else {
                    self.out.stopped = Some(format!("asset {i}: unsupported type {id}"));
                    break;
                };
                self.asset(ty, table.u32(8 * i + 4), table.loc(8 * i + 4))?;
            }
        }
        self.pop();
        Ok(())
    }

    // ---------------------------------------------------------- shared pieces

    /// `SndAliasCustom` at `rec[o]`: a pointer to a struct holding an alias name.
    fn snd_alias_custom(&mut self, rec: Rec<'a>, o: usize) -> R<Option<String>> {
        match rec.u32(o) {
            0 => Ok(None),
            FOLLOW => {
                self.alloc(4);
                let n = self.read(4)?;
                let voff = n.boff;
                let s = self.xstring(n.u32(0))?;
                if let Some(s) = &s {
                    self.snd_names.insert(voff, s.clone());
                }
                Ok(s)
            }
            v => Ok(self.snd_names.get(&decode_ptr(v).1).cloned()),
        }
    }

    fn phys_constraint(&mut self, c: Rec<'a>) -> R<()> {
        self.xstring(c.u32(0x14))?;
        self.xstring(c.u32(0x24))?;
        self.asset(AssetType::Material, c.u32(0x8c), c.loc(0x8c))?;
        Ok(())
    }

    fn cbrushside(&mut self, s: Rec<'a>) -> R<()> {
        self.reusable(s.u32(0), 4, 20)?;
        Ok(())
    }

    fn cbrush(&mut self, b: Rec<'a>) -> R<()> {
        if b.u32(0x20) == FOLLOW {
            self.alloc(4);
            let side = self.read(12)?;
            self.cbrushside(side)?;
        }
        self.reusable(b.u32(0x30), 1, 1)?;
        self.reusable(b.u32(0x4c), 4, 12)?;
        Ok(())
    }

    fn xmodel_pieces(&mut self, p: Rec<'a>) -> R<()> {
        self.xstring(p.u32(0))?;
        if p.u32(8) != 0 {
            self.alloc(4);
            let n = p.u32(4) as usize;
            let arr = self.read(16 * n)?;
            for i in 0..n {
                let e = arr.elem(i, 16);
                self.asset(AssetType::XModel, e.u32(0), e.loc(0))?;
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------- loaders

    fn phys_preset(&mut self) -> R<(AssetRef, String)> {
        let h = self.read(48)?;
        self.push(VIRTUAL);
        let name = self.xstring(h.u32(0))?.unwrap_or_default();
        self.xstring(h.u32(0x1c))?;
        self.pop();
        Ok((AssetRef::Other(AssetType::PhysPreset), name))
    }

    fn phys_constraints(&mut self) -> R<(AssetRef, String)> {
        let h = self.read(2440)?;
        self.push(VIRTUAL);
        let name = self.xstring(h.u32(0))?.unwrap_or_default();
        for i in 0..16 {
            self.phys_constraint(h.sub(8 + i * 152, 152))?;
        }
        self.pop();
        Ok((AssetRef::Other(AssetType::PhysConstraints), name))
    }

    fn destructible(&mut self) -> R<(AssetRef, String)> {
        use AssetType as T;
        let h = self.read(20)?;
        self.push(VIRTUAL);
        let name = self.xstring(h.u32(0))?.unwrap_or_default();
        self.asset(T::XModel, h.u32(4), None)?;
        if h.u32(0xc) != 0 {
            self.alloc(4);
            let n = h.i32(8).max(0) as usize;
            let pieces = self.read(248 * n)?;
            for i in 0..n {
                let p = pieces.elem(i, 248);
                for m in 0..5 {
                    let dm = p.sub(m * 40, 40);
                    self.asset(T::XModel, dm.u32(0), dm.loc(0))?;
                    self.asset(T::Fx, dm.u32(0xc), dm.loc(0xc))?;
                    self.snd_alias_custom(dm, 0x10)?;
                    self.xstring(dm.u32(0x14))?;
                    self.xstring(dm.u32(0x18))?;
                    for k in 0..3 {
                        self.asset(T::XModel, dm.u32(0x1c + 4 * k), dm.loc(0x1c + 4 * k))?;
                    }
                }
                self.xstring(p.u32(0xc8))?;
                self.asset(T::PhysConstraints, p.u32(0xe0), p.loc(0xe0))?;
                self.snd_alias_custom(p, 0xe8)?;
                self.asset(T::Fx, p.u32(0xec), p.loc(0xec))?;
                self.snd_alias_custom(p, 0xf0)?;
            }
        }
        self.pop();
        Ok((AssetRef::Other(T::DestructibleDef), name))
    }

    fn xanim(&mut self) -> R<(AssetRef, String)> {
        let h = self.read(88)?;
        let numframes = h.u16(0xe);
        let idx_size = if numframes < 256 { 1 } else { 2 };
        self.push(VIRTUAL);
        let name = self.xstring(h.u32(0))?.unwrap_or_default();
        let nbones = h.u8(0x12 + 9) as usize;
        let names_p = self.inline(h.u32(0x30), 2, 2 * nbones)?;
        let notify_n = h.u8(0x1c) as usize;
        let notify_p = self.inline(h.u32(0x50), 4, 8 * notify_n)?;
        let (mut delta_trans, mut delta_quat) = (None, None);
        if h.u32(0x54) != 0 {
            self.alloc(4);
            let part = self.read(8)?;
            if part.u32(0) != 0 {
                // XAnimPartTrans: variable-size struct read in one go.
                self.alloc(4);
                let fpos = self.pos;
                let t = self.read(4)?;
                let size = t.u16(0) as usize;
                let small = t.u8(2) != 0;
                let mut frames_fpos = None;
                if size > 0 {
                    let frames = self.read(28)?;
                    self.read((size + 1) * idx_size)?;
                    if frames.u32(24) != 0 {
                        frames_fpos = Some(if small { self.data(1, 3 * (size + 1))? } else { self.data(4, 6 * (size + 1))? });
                    }
                } else {
                    self.read(12)?;
                }
                delta_trans = Some(DeltaTrans { size: size as u16, small, fpos, frames_fpos });
            }
            if part.u32(4) != 0 {
                // XAnimDeltaPartQuat: also variable-size.
                self.alloc(4);
                let fpos = self.pos;
                let q = self.read(4)?;
                let size = q.u16(0) as usize;
                let mut frames_fpos = None;
                if size > 0 {
                    let f = self.read(4)?;
                    self.read((size + 1) * idx_size)?;
                    if f.u32(0) != 0 {
                        frames_fpos = Some(self.data(4, 4 * (size + 1))?);
                    }
                } else {
                    self.read(4)?;
                }
                delta_quat = Some(DeltaQuat { size: size as u16, fpos, frames_fpos });
            }
        }
        let stream = |fpos: Option<usize>, count: usize| Stream { fpos, count };
        let data_byte = stream(self.inline(h.u32(0x34), 1, h.u16(0x4) as usize)?, h.u16(0x4) as usize);
        let data_short = stream(self.inline(h.u32(0x38), 2, 2 * h.u16(0x6) as usize)?, h.u16(0x6) as usize);
        let data_int = stream(self.inline(h.u32(0x3c), 4, 4 * h.u16(0x8) as usize)?, h.u16(0x8) as usize);
        let random_data_short = stream(self.inline(h.u32(0x40), 2, 2 * h.u32(0x20) as usize)?, h.u32(0x20) as usize);
        let random_data_byte = stream(self.inline(h.u32(0x44), 1, h.u16(0xa) as usize)?, h.u16(0xa) as usize);
        let random_data_int = stream(self.inline(h.u32(0x48), 4, 4 * h.u16(0xc) as usize)?, h.u16(0xc) as usize);
        let count = h.u32(0x24) as usize;
        let indices = if numframes < 256 {
            stream(self.inline(h.u32(0x4c), 1, count)?, count)
        } else {
            stream(self.inline(h.u32(0x4c), 2, 2 * count)?, count)
        };
        self.pop();

        let z = self.z;
        let rd16 = |p: usize| u16::from_le_bytes([z[p], z[p + 1]]) as usize;
        let bones = (0..nbones)
            .map(|i| names_p.map(|p| rd16(p + 2 * i)).and_then(|s| self.out.script_strings.get(s).cloned()).unwrap_or_default())
            .collect();
        let notify = (0..notify_n)
            .filter_map(|i| {
                let p = notify_p? + 8 * i;
                let n = self.out.script_strings.get(rd16(p)).cloned().unwrap_or_default();
                Some((n, f32::from_le_bytes([z[p + 4], z[p + 5], z[p + 6], z[p + 7]])))
            })
            .collect();
        let mut bone_counts = [0u8; 10];
        for (i, c) in bone_counts.iter_mut().enumerate() {
            *c = h.u8(0x12 + i);
        }
        self.out.xanims.push(XAnimInfo {
            name: name.clone(),
            numframes,
            framerate: h.f32(0x28),
            frequency: h.f32(0x2c),
            looping: h.u8(0x10) != 0,
            delta: h.u8(0x11) != 0,
            bone_counts,
            bones,
            data_byte,
            data_short,
            data_int,
            random_data_short,
            random_data_byte,
            random_data_int,
            indices,
            notify,
            delta_trans,
            delta_quat,
        });
        Ok((AssetRef::Other(AssetType::XAnim), name))
    }

    fn xmodel(&mut self) -> R<(AssetRef, String)> {
        let h = self.read(228)?;
        self.push(VIRTUAL);
        let name = self.xstring(h.u32(0))?.unwrap_or_default();
        let nb = h.u8(4) as usize;
        let nroot = h.u8(5) as usize;
        let nsurf = h.u8(6) as usize;
        let nchild = nb.saturating_sub(nroot);
        let names_p = self.reusable(h.u32(0x8), 2, 2 * nb)?;
        let parents_p = self.reusable(h.u32(0xc), 1, nchild)?;
        let quats_p = self.reusable(h.u32(0x10), 2, 8 * nchild)?;
        let trans_p = self.reusable(h.u32(0x14), 4, 16 * nchild)?;
        self.reusable(h.u32(0x18), 1, nb)?;
        let base_p = self.reusable(h.u32(0x1c), 4, 32 * nb)?;

        let mut surfs = Vec::with_capacity(nsurf);
        if h.u32(0x20) != 0 {
            self.alloc(4);
            let arr = self.read(64 * nsurf)?;
            for i in 0..nsurf {
                let s = arr.elem(i, 64);
                let vc = s.u16(2);
                let tc = s.u16(4);
                let blend_counts = [s.i16(0x10), s.i16(0x12), s.i16(0x14), s.i16(0x16)];
                let blend_len = blend_counts[0].max(0) as usize
                    + 3 * blend_counts[1].max(0) as usize
                    + 5 * blend_counts[2].max(0) as usize
                    + 7 * blend_counts[3].max(0) as usize;
                let blend_fpos = self.reusable(s.u32(0x18), 2, 2 * blend_len)?;
                let verts_fpos = self.reusable(s.u32(0x1c), 16, 32 * vc as usize)?;
                let rigid = match s.u32(0x28) {
                    0 => Vec::new(),
                    FOLLOW => {
                        self.alloc(4);
                        let n = s.u32(0x24) as usize;
                        let lists = self.read(12 * n)?;
                        let mut out = Vec::with_capacity(n);
                        for k in 0..n {
                            let l = lists.elem(k, 12);
                            out.push(RigidVertList { bone_offset: l.u16(0), vert_count: l.u16(2), tri_offset: l.u16(4), tri_count: l.u16(6) });
                            if l.u32(8) == FOLLOW {
                                self.alloc(4);
                                let tree = self.read(40)?;
                                self.inline(tree.u32(0x1c), 16, 16 * tree.u32(0x18) as usize)?;
                                self.inline(tree.u32(0x24), 2, 2 * tree.u32(0x20) as usize)?;
                            }
                        }
                        self.rigid_lists.insert(lists.boff, out.clone());
                        out
                    }
                    v => self.rigid_lists.get(&decode_ptr(v).1).cloned().unwrap_or_default(),
                };
                let tris_fpos = self.reusable(s.u32(0xc), 16, 6 * tc as usize)?;
                surfs.push(XSurfInfo { vert_count: vc, tri_count: tc, blend_counts, blend_fpos, verts_fpos, tris_fpos, rigid });
            }
        }
        let mut materials = vec![None; nsurf];
        if h.u32(0x24) != 0 {
            self.alloc(4);
            let arr = self.read(4 * nsurf)?;
            for (i, m) in materials.iter_mut().enumerate() {
                *m = self.asset_idx(AssetType::Material, arr.u32(4 * i), arr.loc(4 * i))?;
            }
        }
        if h.u32(0x98) != 0 {
            self.alloc(4);
            let n = h.i32(0x9c).max(0) as usize;
            let arr = self.read(44 * n)?;
            for i in 0..n {
                let c = arr.elem(i, 44);
                self.inline(c.u32(0), 4, 48 * c.i32(4).max(0) as usize)?;
            }
        }
        self.inline(h.u32(0xa4), 4, 40 * nb)?;
        self.asset(AssetType::PhysPreset, h.u32(0xd4), None)?;
        for o in [0xd8, 0xdc] {
            if h.u32(o) == FOLLOW {
                self.alloc(4);
                let list = self.read(20)?;
                if list.u32(4) == FOLLOW {
                    self.alloc(4);
                    let n = list.u32(0) as usize;
                    let geoms = self.read(68 * n)?;
                    for g in 0..n {
                        let gi = geoms.elem(g, 68);
                        if gi.u32(0) == FOLLOW {
                            self.alloc(4);
                            let bw = self.read(96)?;
                            if bw.u32(0x20) != 0 {
                                self.alloc(4);
                                let ns = bw.u32(0x1c) as usize;
                                let sides = self.read(12 * ns)?;
                                for k in 0..ns {
                                    self.cbrushside(sides.elem(k, 12))?;
                                }
                            }
                            self.inline(bw.u32(0x30), 1, bw.i32(0x50).max(0) as usize)?;
                            self.reusable(bw.u32(0x4c), 4, 12 * bw.u32(0x48) as usize)?;
                            self.reusable(bw.u32(0x54), 4, 20 * bw.u32(0x1c) as usize)?;
                        }
                    }
                }
            }
        }
        self.asset(AssetType::PhysConstraints, h.u32(0xe0), None)?;
        self.pop();

        // Bones.
        let z = self.z;
        let rd16 = |p: usize| u16::from_le_bytes([z[p], z[p + 1]]);
        let rdf = |p: usize| f32::from_le_bytes([z[p], z[p + 1], z[p + 2], z[p + 3]]);
        let mut bones = Vec::with_capacity(nb);
        for i in 0..nb {
            let bname = names_p.map(|p| rd16(p + 2 * i) as usize).and_then(|s| self.out.script_strings.get(s).cloned()).unwrap_or_default();
            let (parent, lq, lt) = if i < nroot {
                (None, [0.0, 0.0, 0.0, 1.0], [0.0; 3])
            } else {
                let c = i - nroot;
                let rel = parents_p.map(|p| z[p + c] as usize).unwrap_or(0);
                let q = quats_p
                    .map(|p| {
                        let g = |k: usize| rd16(p + 8 * c + 2 * k) as i16 as f32 / 32767.0;
                        [g(0), g(1), g(2), g(3)]
                    })
                    .unwrap_or([0.0, 0.0, 0.0, 1.0]);
                let t = trans_p.map(|p| [rdf(p + 12 * c), rdf(p + 12 * c + 4), rdf(p + 12 * c + 8)]).unwrap_or([0.0; 3]);
                (i.checked_sub(rel), q, t)
            };
            let (bq, bt) = base_p
                .map(|p| {
                    let o = p + 32 * i;
                    ([rdf(o), rdf(o + 4), rdf(o + 8), rdf(o + 12)], [rdf(o + 16), rdf(o + 20), rdf(o + 24)])
                })
                .unwrap_or(([0.0, 0.0, 0.0, 1.0], [0.0; 3]));
            bones.push(Bone { name: bname, parent, local_quat: lq, local_trans: lt, base_quat: bq, base_trans: bt });
        }
        let num_lods = (h.u16(0xc4) as usize).min(4);
        let lods = (0..num_lods)
            .map(|l| {
                let o = 0x28 + 28 * l;
                LodInfo { dist: h.f32(o), num_surfs: h.u16(o + 4), surf_index: h.u16(o + 6) }
            })
            .collect();
        let idx = self.out.xmodels.len() as u32;
        self.out.xmodels.push(XModelInfo {
            name: name.clone(),
            bones,
            surfs,
            materials,
            lods,
            mins: h.vec3(0xac),
            maxs: h.vec3(0xb8),
            num_coll_surfs: h.i32(0x9c).max(0) as u32,
            contents: h.i32(0xa0),
            coll_lod: h.u16(0xc6) as i16,
        });
        Ok((AssetRef::XModel(idx), name))
    }

    fn material(&mut self) -> R<(AssetRef, String)> {
        let h = self.read(112)?;
        self.push(VIRTUAL);
        let name = self.xstring(h.u32(0))?.unwrap_or_default();
        let techset = match self.asset(AssetType::TechniqueSet, h.u32(0x60), None)? {
            Some(AssetRef::TechSet(i)) => self.techsets.get(i as usize).cloned(),
            _ => None,
        };
        let count = h.u8(0x5b) as usize;
        let textures = match h.u32(0x64) {
            0 => Vec::new(),
            FOLLOW => {
                self.alloc(4);
                let table = self.read(16 * count)?;
                let mut defs = Vec::with_capacity(count);
                for i in 0..count {
                    let t = table.elem(i, 16);
                    let semantic = t.u8(7);
                    let image = if semantic == 11 {
                        if t.u32(0xc) == FOLLOW {
                            self.alloc(4);
                            let w = self.read(68)?;
                            let nm = (w.i32(0x10).max(0) * w.i32(0xc).max(0)) as usize;
                            self.inline(w.u32(4), 4, 8 * nm)?;
                            self.inline(w.u32(8), 4, 4 * nm)?;
                            self.asset_idx(AssetType::Image, w.u32(0x40), w.loc(0x40))?
                        } else {
                            None
                        }
                    } else {
                        self.asset_idx(AssetType::Image, t.u32(0xc), t.loc(0xc))?
                    };
                    defs.push(TexDef { name_hash: t.u32(0), semantic, image });
                }
                self.tex_tables.insert(table.boff, defs.clone());
                defs
            }
            v => self.tex_tables.get(&decode_ptr(v).1).cloned().unwrap_or_default(),
        };
        self.reusable(h.u32(0x68), 16, 32 * h.u8(0x5c) as usize)?;
        let state_count = h.u8(0x5d) as usize;
        let state_bits = self.reusable(h.u32(0x6c), 4, 8 * state_count)?.map(|p| (p, state_count));
        self.pop();
        let idx = self.out.materials.len() as u32;
        let state_entry: Vec<u8> = (0..59).map(|i| h.u8(0x20 + i)).collect();
        self.out.materials.push(MaterialInfo { name: name.clone(), techset, textures, sort_key: h.u8(5), state_bits, state_entry });
        Ok((AssetRef::Material(idx), name))
    }

    fn shader(&mut self, ptr: u32) -> R<()> {
        if ptr == FOLLOW {
            self.alloc(4);
            let s = self.read(16)?;
            self.xstring(s.u32(0))?;
            self.inline(s.u32(8), 4, 4 * s.u16(12) as usize)?;
        }
        Ok(())
    }

    fn techset(&mut self) -> R<(AssetRef, String)> {
        let h = self.read(248)?;
        self.push(VIRTUAL);
        let name = self.xstring(h.u32(0))?.unwrap_or_default();
        for t in 0..59 {
            if h.u32(0xc + 4 * t) != FOLLOW {
                continue;
            }
            self.alloc(4);
            let th = self.read(8)?;
            let passes_n = th.u16(6) as usize;
            let passes = self.read(20 * passes_n)?;
            for p in 0..passes_n {
                let pass = passes.elem(p, 20);
                if pass.u32(0) == FOLLOW {
                    self.alloc(4);
                    self.read(104)?;
                }
                self.shader(pass.u32(4))?;
                self.shader(pass.u32(8))?;
                if pass.u32(0x10) != 0 {
                    self.alloc(4);
                    let n = pass.u8(0xc) as usize + pass.u8(0xd) as usize + pass.u8(0xe) as usize;
                    let args = self.read(8 * n)?;
                    for a in 0..n {
                        let arg = args.elem(a, 8);
                        let ty = arg.u16(0);
                        if ty == 1 || ty == 7 {
                            self.reusable(arg.u32(4), 4, 16)?;
                        }
                    }
                }
            }
            self.xstring(th.u32(0))?;
        }
        self.pop();
        self.techsets.push(name.clone());
        Ok((AssetRef::TechSet(self.techsets.len() as u32 - 1), name))
    }

    fn image(&mut self) -> R<(AssetRef, String)> {
        let h = self.read(36)?;
        self.push(VIRTUAL);
        let name = self.xstring(h.u32(0x20))?.unwrap_or_default();
        let idx = self.out.images.len() as u32;
        let mut inline = None;
        self.push(TEMP);
        let ld = h.u32(4);
        if ld == FOLLOW || ld == INSERT {
            self.alloc(4);
            let slot = (ld == INSERT).then(|| self.insert_slot());
            let def = self.read(16)?;
            let size = def.u32(0xc) as usize;
            let fpos = self.pos;
            self.read(size)?;
            if size > 0 {
                inline = Some(InlinePixels {
                    fpos,
                    len: size,
                    format: def.u32(8),
                    levels: def.u8(0),
                    dims: [def.u16(2), def.u16(4), def.u16(6)],
                });
            }
            if let Some(s) = slot {
                self.slots.insert(s, AssetRef::LoadDef(idx));
            }
        } else if ld != 0 {
            self.lookup_alias(ld);
        }
        self.pop();
        self.pop();
        self.out.images.push(ImageInfo { name: name.clone(), map_type: h.u32(0), width: h.u16(0x18), height: h.u16(0x1a), inline });
        Ok((AssetRef::Image(idx), name))
    }

    fn sound_file(&mut self, ptr: u32) -> R<SoundFile> {
        match ptr {
            0 => Ok(SoundFile::None),
            FOLLOW => {
                self.alloc(4);
                let f = self.read(20)?;
                let file = if f.u8(0) == 1 {
                    match self.asset(AssetType::LoadedSound, f.u32(4), f.loc(4))? {
                        Some(AssetRef::LoadedSound(i)) => SoundFile::Loaded(i),
                        _ => SoundFile::None,
                    }
                } else {
                    let dir = self.xstring(f.u32(8))?.unwrap_or_default();
                    let name = self.xstring(f.u32(12))?.unwrap_or_default();
                    if f.u32(16) == FOLLOW {
                        self.alloc(4);
                        let ps = self.read(12)?;
                        self.xstring(ps.u32(0))?;
                        self.push(LARGE);
                        self.inline(ps.u32(4), 2048, ps.u32(8) as usize)?;
                        self.pop();
                    }
                    SoundFile::Streamed { dir, name }
                };
                self.sound_files.insert(f.boff, file.clone());
                Ok(file)
            }
            v => Ok(self.sound_files.get(&decode_ptr(v).1).cloned().unwrap_or(SoundFile::None)),
        }
    }

    fn sound(&mut self) -> R<(AssetRef, String)> {
        let h = self.read(12)?;
        self.push(VIRTUAL);
        let name = self.xstring(h.u32(0))?.unwrap_or_default();
        let count = h.i32(8).max(0) as usize;
        let aliases = match h.u32(4) {
            0 => Vec::new(),
            FOLLOW => {
                self.alloc(4);
                let arr = self.read(184 * count)?;
                let mut out = Vec::with_capacity(count);
                for i in 0..count {
                    let a = arr.elem(i, 184);
                    let aname = self.xstring(a.u32(0))?.unwrap_or_default();
                    self.xstring(a.u32(8))?;
                    let secondary = self.xstring(a.u32(0xc))?.filter(|s| !s.is_empty());
                    let chain = self.xstring(a.u32(0x10))?.filter(|s| !s.is_empty());
                    let file = self.sound_file(a.u32(0x14))?;
                    out.push(SoundAlias {
                        name: aname,
                        file,
                        vol_min: a.f32(0x1c),
                        vol_max: a.f32(0x20),
                        pitch_min: a.f32(0x24),
                        pitch_max: a.f32(0x28),
                        dist_min: a.f32(0x2c),
                        dist_max: a.f32(0x30),
                        flags: a.i32(0x84),
                        secondary,
                        chain,
                        start_delay: a.u32(0x7c) as f32 / 1000.0,
                    });
                }
                self.alias_heads.insert(arr.boff, out.clone());
                out
            }
            v => self.alias_heads.get(&decode_ptr(v).1).cloned().unwrap_or_default(),
        };
        self.pop();
        let idx = self.out.sounds.len() as u32;
        self.out.sounds.push(SoundList { name: name.clone(), aliases });
        Ok((AssetRef::Sound(idx), name))
    }

    fn loaded_sound(&mut self) -> R<(AssetRef, String)> {
        let h = self.read(12)?;
        self.push(VIRTUAL);
        let name = self.xstring(h.u32(0))?.unwrap_or_default();
        let len = h.i32(8).max(0) as usize;
        let fpos = self.inline(h.u32(4), 1, len)?;
        self.pop();
        let idx = self.out.loaded_sounds.len() as u32;
        self.out.loaded_sounds.push(LoadedSoundInfo { name: name.clone(), fpos: fpos.unwrap_or(0), len: if fpos.is_some() { len } else { 0 } });
        Ok((AssetRef::LoadedSound(idx), name))
    }

    fn clipmap(&mut self) -> R<(AssetRef, String)> {
        use AssetType as T;
        let h = self.read(332)?;
        self.push(VIRTUAL);
        let name = self.xstring(h.u32(0))?.unwrap_or_default();
        let n = |o: usize| h.u32(o) as usize;
        self.reusable(h.u32(0xc), 4, 20 * n(0x8))?;
        if h.u32(0x14) != 0 {
            self.alloc(4);
            let arr = self.read(80 * n(0x10))?;
            for i in 0..n(0x10) {
                let m = arr.elem(i, 80);
                self.asset(T::XModel, m.u32(4), m.loc(4))?;
            }
        }
        self.inline(h.u32(0x1c), 4, 72 * n(0x18))?;
        if h.u32(0x24) != 0 {
            self.alloc(4);
            let arr = self.read(12 * n(0x20))?;
            for i in 0..n(0x20) {
                self.cbrushside(arr.elem(i, 12))?;
            }
        }
        self.inline(h.u32(0x2c), 1, n(0x28))?;
        if h.u32(0x34) != 0 {
            self.alloc(4);
            let arr = self.read(8 * n(0x30))?;
            for i in 0..n(0x30) {
                self.reusable(arr.elem(i, 8).u32(0), 4, 20)?;
            }
        }
        self.inline(h.u32(0x3c), 4, 44 * n(0x38))?;
        self.inline(h.u32(0x4c), 2, 2 * n(0x48))?;
        if h.u32(0x44) != 0 {
            self.alloc(4);
            let arr = self.read(20 * n(0x40))?;
            for i in 0..n(0x40) {
                let node = arr.elem(i, 20);
                let count = node.i16(2);
                if count > 0 {
                    self.reusable(node.u32(8), 2, 2 * count as usize)?;
                }
            }
        }
        self.inline(h.u32(0x54), 4, 4 * n(0x50))?;
        self.inline(h.u32(0x5c), 4, 12 * n(0x58))?;
        self.inline(h.u32(0x64), 4, 12 * n(0x60))?;
        self.inline(h.u32(0x6c), 2, 2 * n(0x68))?;
        let tri = h.i32(0x70).max(0) as usize;
        self.inline(h.u32(0x74), 2, 6 * tri)?;
        self.inline(h.u32(0x78), 1, (3 * tri).div_ceil(32) * 4)?;
        self.inline(h.u32(0x80), 4, 28 * n(0x7c))?;
        if h.u32(0x88) != 0 {
            self.alloc(4);
            let arr = self.read(20 * n(0x84))?;
            for i in 0..n(0x84) {
                self.reusable(arr.elem(i, 20).u32(0x10), 4, 28)?;
            }
        }
        self.inline(h.u32(0x90), 16, 32 * n(0x8c))?;
        self.inline(h.u32(0x98), 4, 72 * n(0x94))?;
        if h.u32(0xa0) != 0 {
            self.alloc(16);
            let nb = h.u16(0x9c) as usize;
            let arr = self.read(80 * nb)?;
            for i in 0..nb {
                self.cbrush(arr.elem(i, 80))?;
            }
        }
        self.inline(h.u32(0xac), 1, h.i32(0xa4).max(0) as usize * h.i32(0xa8).max(0) as usize)?;
        self.asset(T::MapEnts, h.u32(0xb4), None)?;
        if h.u32(0xb8) == FOLLOW {
            self.alloc(16);
            let b = self.read(80)?;
            self.cbrush(b)?;
        }
        for k in 0..2 {
            if h.u32(0x110 + 4 * k) != 0 {
                self.alloc(4);
                let cnt = h.u16(0x106 + 2 * k) as usize;
                let arr = self.read(84 * cnt)?;
                for i in 0..cnt {
                    let d = arr.elem(i, 84);
                    self.asset(T::XModel, d.u32(0x20), d.loc(0x20))?;
                    self.asset(T::Fx, d.u32(0x28), d.loc(0x28))?;
                    if d.u32(0x2c) == FOLLOW {
                        self.alloc(4);
                        let p = self.read(12)?;
                        self.xmodel_pieces(p)?;
                    }
                    self.asset(T::PhysPreset, d.u32(0x30), d.loc(0x30))?;
                }
            }
        }
        if h.u32(0x144) != 0 {
            self.alloc(4);
            let cnt = h.i32(0x140).max(0) as usize;
            let arr = self.read(152 * cnt)?;
            for i in 0..cnt {
                self.phys_constraint(arr.elem(i, 152))?;
            }
        }
        self.pop();
        Ok((AssetRef::Other(T::ClipMap), name))
    }

    fn comworld(&mut self) -> R<(AssetRef, String)> {
        let h = self.read(64)?;
        self.push(VIRTUAL);
        let name = self.xstring(h.u32(0))?.unwrap_or_default();
        if h.u32(0xc) != 0 {
            self.alloc(4);
            let n = h.u32(8) as usize;
            let arr = self.read(72 * n)?;
            for i in 0..n {
                self.xstring(arr.elem(i, 72).u32(0x44))?;
            }
        }
        self.inline(h.u32(0x24), 4, 8 * h.u32(0x20) as usize)?;
        if h.u32(0x3c) != 0 {
            self.alloc(4);
            let n = h.u32(0x38) as usize;
            let arr = self.read(12 * n)?;
            for i in 0..n {
                self.inline(arr.elem(i, 12).u32(8), 1, 32)?;
            }
        }
        self.pop();
        Ok((AssetRef::Other(AssetType::ComWorld), name))
    }

    fn pathnode_tree(&mut self, t: Rec<'a>) -> R<()> {
        if t.i32(0) >= 0 {
            for c in 0..2 {
                if t.u32(8 + 4 * c) == FOLLOW {
                    self.alloc(4);
                    let child = self.read(16)?;
                    self.pathnode_tree(child)?;
                }
            }
        } else {
            self.inline(t.u32(12), 2, 2 * t.i32(8).max(0) as usize)?;
        }
        Ok(())
    }

    fn gameworld_sp(&mut self) -> R<(AssetRef, String)> {
        let h = self.read(44)?;
        self.push(VIRTUAL);
        let name = self.xstring(h.u32(0))?.unwrap_or_default();
        let p = h.sub(4, 40);
        let nodes = p.u32(0) as usize;
        if p.u32(4) != 0 {
            self.alloc(4);
            let arr = self.read(128 * nodes)?;
            for i in 0..nodes {
                let c = arr.elem(i, 128);
                self.inline(c.u32(0x40), 4, 12 * c.u16(0x3e) as usize)?;
            }
        }
        self.inline(p.u32(0x10), 2, 2 * nodes)?;
        self.inline(p.u32(0x14), 2, 2 * nodes)?;
        self.inline(p.u32(0x1c), 1, p.i32(0x18).max(0) as usize)?;
        if p.u32(0x24) != 0 {
            self.alloc(4);
            let n = p.i32(0x20).max(0) as usize;
            let arr = self.read(16 * n)?;
            for i in 0..n {
                self.pathnode_tree(arr.elem(i, 16))?;
            }
        }
        self.pop();
        Ok((AssetRef::Other(AssetType::GameWorldSp), name))
    }

    fn map_ents(&mut self) -> R<(AssetRef, String)> {
        let h = self.read(12)?;
        self.push(VIRTUAL);
        let name = self.xstring(h.u32(0))?.unwrap_or_default();
        let len = h.i32(8).max(0) as usize;
        if let Some(p) = self.inline(h.u32(4), 1, len)? {
            let text = &self.z[p..p + len];
            let end = text.iter().position(|&b| b == 0).unwrap_or(len);
            self.out.map_ents = Some(String::from_utf8_lossy(&text[..end]).into_owned());
        }
        self.pop();
        Ok((AssetRef::Other(AssetType::MapEnts), name))
    }

    fn gfx_cell(&mut self, c: Rec<'a>) -> R<()> {
        if c.u32(0x1c) != 0 {
            self.alloc(4);
            let n = c.i32(0x18).max(0) as usize;
            let arr = self.read(40 * n)?;
            for i in 0..n {
                let t = arr.elem(i, 40);
                self.reusable(t.u32(0x20), 2, 2 * t.u16(0x1e) as usize)?;
            }
        }
        if c.u32(0x24) != 0 {
            self.alloc(4);
            let n = c.i32(0x20).max(0) as usize;
            let arr = self.read(68 * n)?;
            for i in 0..n {
                let p = arr.elem(i, 68);
                if p.u32(0x20) == FOLLOW {
                    self.alloc(4);
                    let cell = self.read(56)?;
                    self.gfx_cell(cell)?;
                }
                self.inline(p.u32(0x24), 4, 12 * p.u8(0x28) as usize)?;
            }
        }
        self.inline(c.u32(0x2c), 4, 4 * c.i32(0x28).max(0) as usize)?;
        self.inline(c.u32(0x34), 1, c.u8(0x30) as usize)?;
        Ok(())
    }

    fn gfxworld(&mut self) -> R<(AssetRef, String)> {
        use AssetType as T;
        let h = self.read(800)?;
        self.push(VIRTUAL);
        let name = self.xstring(h.u32(0))?.unwrap_or_default();
        self.xstring(h.u32(4))?;
        let index_count = h.u32(0x10);
        let indices_fpos = self.inline(h.u32(0x14), 2, 2 * index_count as usize)?;
        self.inline(h.u32(0x24), 4, 4 * h.u32(0x20) as usize)?;
        let sky_image = self.asset_idx(T::Image, h.u32(0x28), None)?;
        let sky_box_model = self.xstring(h.u32(0x30))?;
        if h.u32(0xd8) == FOLLOW {
            self.alloc(4);
            let l = self.read(64)?;
            self.asset(T::LightDef, l.u32(0x3c), l.loc(0x3c))?;
        }
        if h.u32(0xf8) != 0 {
            self.alloc(4);
            let n = h.u32(0xf4) as usize;
            let arr = self.read(16 * n)?;
            for i in 0..n {
                let p = arr.elem(i, 16);
                self.asset(T::Image, p.u32(0xc), p.loc(0xc))?;
            }
        }
        self.inline(h.u32(0x104), 4, 32 * h.u32(0x100) as usize)?;
        // dpvsPlanes
        let cell_count = h.i32(0x108).max(0) as usize;
        self.reusable(h.u32(0x10c), 4, 20 * h.i32(0x8).max(0) as usize)?;
        self.inline(h.u32(0x110), 2, 2 * h.i32(0xc).max(0) as usize)?;
        if h.u32(0x11c) != 0 {
            self.alloc(4);
            let arr = self.read(56 * cell_count)?;
            for i in 0..cell_count {
                self.gfx_cell(arr.elem(i, 56))?;
            }
        }
        if h.u32(0x124) != 0 {
            self.alloc(4);
            let n = h.i32(0x120).max(0) as usize;
            let arr = self.read(8 * n)?;
            for i in 0..n {
                let l = arr.elem(i, 8);
                self.asset(T::Image, l.u32(0), l.loc(0))?;
                self.asset(T::Image, l.u32(4), l.loc(4))?;
            }
        }
        // lightGrid
        let g = h.sub(0x128, 56);
        let axis = (g.u32(0x14) as usize).min(2);
        let rows = (g.u16(0xe + 2 * axis) as i64 - g.u16(0x8 + 2 * axis) as i64 + 1).max(0) as usize;
        self.inline(g.u32(0x1c), 2, 2 * rows)?;
        self.inline(g.u32(0x24), 1, g.u32(0x20) as usize)?;
        self.inline(g.u32(0x2c), 4, 4 * g.u32(0x28) as usize)?;
        self.inline(g.u32(0x34), 4, 168 * g.u32(0x30) as usize)?;
        let model_count = h.i32(0x168).max(0) as usize;
        let models_fpos = self.inline(h.u32(0x16c), 4, 56 * model_count)?;
        if h.u32(0x190) != 0 {
            self.alloc(4);
            let n = h.i32(0x18c).max(0) as usize;
            let arr = self.read(8 * n)?;
            for i in 0..n {
                let m = arr.elem(i, 8);
                self.asset(T::Material, m.u32(0), m.loc(0))?;
            }
        }
        let vertex_count = h.u32(0x34);
        let vertices_fpos = self.inline(h.u32(0x38), 4, 44 * vertex_count as usize)?;
        let layer_data_fpos = self.inline(h.u32(0x44), 1, h.u32(0x40) as usize)?;
        self.asset(T::Material, h.u32(0x198), None)?;
        self.asset(T::Material, h.u32(0x19c), None)?;
        self.asset(T::Image, h.u32(0x234), None)?;
        let lights = h.u32(0xec) as usize;
        if h.u32(0x254) != 0 {
            self.alloc(4);
            let arr = self.read(12 * lights)?;
            for i in 0..lights {
                let s = arr.elem(i, 12);
                self.inline(s.u32(4), 2, 2 * s.u16(0) as usize)?;
                self.inline(s.u32(8), 2, 2 * s.u16(2) as usize)?;
            }
        }
        if h.u32(0x258) != 0 {
            self.alloc(4);
            let arr = self.read(8 * lights)?;
            for i in 0..lights {
                let r = arr.elem(i, 8);
                if r.u32(4) != 0 {
                    self.alloc(4);
                    let n = r.u32(0) as usize;
                    let hulls = self.read(80 * n)?;
                    for k in 0..n {
                        let hl = hulls.elem(k, 80);
                        self.inline(hl.u32(0x4c), 4, 20 * hl.u32(0x48) as usize)?;
                    }
                }
            }
        }
        // dpvs
        let d = h.sub(0x25c, 100);
        let smodel_count = d.u32(0) as usize;
        let static_surface_count = d.u32(4);
        self.inline(d.u32(0x44), 2, 2 * static_surface_count as usize)?;
        self.inline(d.u32(0x48), 4, 28 * smodel_count)?;
        let surface_count = h.i32(0x18).max(0) as usize;
        let mut surfaces = Vec::with_capacity(surface_count);
        if d.u32(0x4c) != 0 {
            self.alloc(4);
            let arr = self.read(52 * surface_count)?;
            for i in 0..surface_count {
                let s = arr.elem(i, 52);
                let material = self.asset_idx(T::Material, s.u32(0x14), s.loc(0x14))?;
                surfaces.push(WorldSurface {
                    vertex_layer_data: s.u32(0),
                    first_vertex: s.u32(4),
                    vertex_count: s.u16(8),
                    tri_count: s.u16(0xa),
                    base_index: s.u32(0xc),
                    material,
                    lightmap: s.u8(0x18),
                    flags: s.u8(0x1b),
                });
            }
        }
        self.inline(d.u32(0x50), 4, 32 * h.i32(0xf0).max(0) as usize)?;
        let mut smodels = Vec::with_capacity(smodel_count);
        if d.u32(0x54) != 0 {
            self.alloc(4);
            let arr = self.read(92 * smodel_count)?;
            for i in 0..smodel_count {
                let s = arr.elem(i, 92);
                let model = self.asset_idx(T::XModel, s.u32(0x38), s.loc(0x38))?;
                smodels.push(StaticModel {
                    origin: s.vec3(4),
                    axis: [s.vec3(0x10), s.vec3(0x1c), s.vec3(0x28)],
                    scale: s.f32(0x34),
                    model,
                });
            }
        }
        self.inline(h.u32(0x2f4), 4, 24 * h.u32(0x2f0) as usize)?;
        self.inline(h.u32(0x2fc), 4, 12 * h.u32(0x2f8) as usize)?;
        self.inline(h.u32(0x304), 4, 4 * h.u32(0x300) as usize)?;
        for w in 0..2 {
            let b = h.sub(0x30c + 8 * w, 8);
            self.inline(b.u32(4), 4, 16 * (b.u32(0) as usize / 16))?;
        }
        self.asset(T::Material, h.u32(0x31c), None)?;
        self.pop();

        let models = models_fpos
            .map(|p| {
                let rec = Rec { d: &self.z[p..p + 56 * model_count], blk: RAW, boff: 0 };
                (0..model_count)
                    .map(|i| {
                        let m = rec.elem(i, 56);
                        BrushModel { mins: m.vec3(0x18), maxs: m.vec3(0x24), surface_count: m.u32(0x30), start_surface: m.u32(0x34) }
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.out.world = Some(WorldInfo {
            name: name.clone(),
            vertex_count,
            vertices_fpos,
            index_count,
            indices_fpos,
            layer_data_fpos,
            surfaces,
            static_surface_count,
            lit_range: d.u32(0x8)..d.u32(0xc),
            decal_range: d.u32(0x10)..d.u32(0x14),
            emissive_range: d.u32(0x18)..d.u32(0x1c),
            models,
            smodels,
            sky_image,
            sky_box_model,
            mins: h.vec3(0x170),
            maxs: h.vec3(0x17c),
            sun_color: h.vec3(0xdc),
        });
        Ok((AssetRef::Other(T::GfxWorld), name))
    }

    fn light_def(&mut self) -> R<(AssetRef, String)> {
        let h = self.read(16)?;
        self.push(VIRTUAL);
        let name = self.xstring(h.u32(0))?.unwrap_or_default();
        self.asset(AssetType::Image, h.u32(4), None)?;
        self.pop();
        Ok((AssetRef::Other(AssetType::LightDef), name))
    }

    fn font(&mut self) -> R<(AssetRef, String)> {
        let h = self.read(24)?;
        self.push(VIRTUAL);
        let name = self.xstring(h.u32(0))?.unwrap_or_default();
        let material = self.asset_idx(AssetType::Material, h.u32(0xc), None)?;
        self.asset(AssetType::Material, h.u32(0x10), None)?;
        let glyph_count = h.i32(8).max(0) as usize;
        let glyphs_fpos = self.reusable(h.u32(0x14), 4, 24 * glyph_count)?;
        self.pop();
        self.out.fonts.push(FontInfo { name: name.clone(), pixel_height: h.i32(4), glyph_count, glyphs_fpos, material });
        Ok((AssetRef::Other(AssetType::Font), name))
    }

    fn snd_driver_globals(&mut self) -> R<(AssetRef, String)> {
        let h = self.read(23556)?;
        self.push(VIRTUAL);
        let name = self.xstring(h.u32(0))?.unwrap_or_default();
        self.pop();
        Ok((AssetRef::Other(AssetType::SndDriverGlobals), name))
    }

    fn localize(&mut self) -> R<(AssetRef, String)> {
        let h = self.read(8)?;
        self.push(VIRTUAL);
        let value = self.xstring(h.u32(0))?.unwrap_or_default();
        let name = self.xstring(h.u32(4))?.unwrap_or_default();
        self.pop();
        self.out.localize.push((name.clone(), value));
        Ok((AssetRef::Other(AssetType::Localize), name))
    }

    fn weapon(&mut self) -> R<(AssetRef, String)> {
        use AssetType as T;
        const SOUNDS: [&str; 60] = [
            "pickupSound", "pickupSoundPlayer", "ammoPickupSound", "ammoPickupSoundPlayer", "projectileSound",
            "pullbackSound", "pullbackSoundPlayer", "fireSound", "fireSoundPlayer", "fireLoopSound",
            "fireLoopSoundPlayer", "fireStopSound", "fireStopSoundPlayer", "fireLastSound", "fireLastSoundPlayer",
            "emptyFireSound", "emptyFireSoundPlayer", "crackSound", "whizbySound", "meleeSwipeSound",
            "meleeSwipeSoundPlayer", "meleeHitSound", "meleeMissSound", "rechamberSound", "rechamberSoundPlayer",
            "reloadSound", "reloadSoundPlayer", "reloadEmptySound", "reloadEmptySoundPlayer", "reloadStartSound",
            "reloadStartSoundPlayer", "reloadEndSound", "reloadEndSoundPlayer", "rotateLoopSound",
            "rotateLoopSoundPlayer", "deploySound", "deploySoundPlayer", "finishDeploySound",
            "finishDeploySoundPlayer", "breakdownSound", "breakdownSoundPlayer", "finishBreakdownSound",
            "finishBreakdownSoundPlayer", "detonateSound", "detonateSoundPlayer", "nightVisionWearSound",
            "nightVisionWearSoundPlayer", "nightVisionRemoveSound", "nightVisionRemoveSoundPlayer", "altSwitchSound",
            "altSwitchSoundPlayer", "raiseSound", "raiseSoundPlayer", "firstRaiseSound", "firstRaiseSoundPlayer",
            "putawaySound", "putawaySoundPlayer", "overheatSound", "overheatSoundPlayer", "bounceSound",
        ];
        let h = self.read(2476)?;
        self.push(VIRTUAL);
        let mut w = WeaponInfo { name: self.xstring(h.u32(0))?.unwrap_or_default(), ..Default::default() };
        w.display_name = self.xstring(h.u32(4))?.unwrap_or_default();
        self.xstring(h.u32(8))?;
        for i in 0..16 {
            let m = self.asset_idx(T::XModel, h.u32(0xc + 4 * i), None)?;
            if i == 0 {
                w.view_model = m;
            }
        }
        w.hand_model = self.asset_idx(T::XModel, h.u32(0x4c), None)?;
        for i in 0..35 {
            let a = self.xstring(h.u32(0x50 + 4 * i))?;
            w.xanims.push(a.unwrap_or_default());
        }
        // Viewmodel notetrack -> sound alias (u16 script strings, 0-terminated).
        for i in 0..20 {
            let (k, v) = (h.u16(0xf0 + 2 * i) as usize, h.u16(0x118 + 2 * i) as usize);
            if k == 0 {
                break;
            }
            if let (Some(k), Some(v)) = (self.out.script_strings.get(k), self.out.script_strings.get(v)) {
                w.notetrack_sounds.push((k.clone(), v.clone()));
            }
        }
        self.xstring(h.u32(0xdc))?;
        self.asset(T::Fx, h.u32(0x17c), None)?;
        self.asset(T::Fx, h.u32(0x180), None)?;
        for (i, field) in SOUNDS.iter().enumerate().take(59) {
            if let Some(s) = self.snd_alias_custom(h, 0x184 + 4 * i)? {
                w.sounds.push((field, s));
            }
        }
        if h.u32(0x270) == FOLLOW {
            self.alloc(4);
            let arr = self.read(4 * 31)?;
            for i in 0..31 {
                self.snd_alias_custom(arr, 4 * i)?;
            }
        }
        for o in [0x274, 0x278, 0x27c] {
            self.xstring(h.u32(o))?;
        }
        for o in [0x28c, 0x290, 0x294, 0x298] {
            self.asset(T::Fx, h.u32(o), None)?;
        }
        self.asset(T::Material, h.u32(0x29c), None)?;
        self.asset(T::Material, h.u32(0x2a0), None)?;
        for i in 0..16 {
            let m = self.asset_idx(T::XModel, h.u32(0x384 + 4 * i), None)?;
            if i == 0 {
                w.world_model = m;
            }
        }
        for o in [0x3c4, 0x3c8, 0x3cc, 0x3d0, 0x3d4] {
            self.asset(T::XModel, h.u32(o), None)?;
        }
        self.asset(T::Material, h.u32(0x3d8), None)?;
        self.asset(T::Material, h.u32(0x3e0), None)?;
        for o in [0x3f0, 0x3f8, 0x410] {
            self.xstring(h.u32(o))?;
        }
        for o in [0x51c, 0x520, 0x610, 0x61c] {
            self.asset(T::Material, h.u32(o), None)?;
        }
        self.xstring(h.u32(0x638))?;
        self.asset(T::XModel, h.u32(0x680), None)?;
        self.asset(T::Fx, h.u32(0x688), None)?;
        self.asset(T::Fx, h.u32(0x690), None)?;
        for (o, field) in [(0x694, "projExplosionSound"), (0x698, "projDudSound"), (0x69c, "mortarShellSound"), (0x6a0, "tankShellSound")] {
            if let Some(s) = self.snd_alias_custom(h, o)? {
                w.sounds.push((field, s));
            }
        }
        self.asset(T::Fx, h.u32(0x7bc), None)?;
        self.asset(T::Fx, h.u32(0x7d8), None)?;
        if let Some(s) = self.snd_alias_custom(h, 0x7dc)? {
            w.sounds.push(("projIgnitionSound", s));
        }
        self.xstring(h.u32(0x880))?;
        self.reusable(h.u32(0x888), 4, 8 * h.i32(0x898).max(0) as usize)?;
        self.reusable(h.u32(0x890), 4, 8 * h.i32(0x898).max(0) as usize)?;
        self.xstring(h.u32(0x884))?;
        self.reusable(h.u32(0x88c), 4, 8 * h.i32(0x89c).max(0) as usize)?;
        self.reusable(h.u32(0x894), 4, 8 * h.i32(0x89c).max(0) as usize)?;
        for o in [0x8f0, 0x8f4, 0x908, 0x97c, 0x980, 0x994, 0x998] {
            self.xstring(h.u32(o))?;
        }
        for o in [0x99c, 0x9a0] {
            if h.u32(o) == FOLLOW {
                self.alloc(4);
                let f = self.read(476)?;
                self.xstring(f.u32(0x1a8))?;
                for k in 0..8 {
                    self.asset(T::Material, f.u32(0x1ac + 4 * k), f.loc(0x1ac + 4 * k))?;
                }
                for k in 0..4 {
                    self.snd_alias_custom(f, 0x1cc + 4 * k)?;
                }
            }
        }
        self.asset(T::Fx, h.u32(0x9a4), None)?;
        self.asset(T::Fx, h.u32(0x9a8), None)?;
        self.pop();
        let name = w.name.clone();
        let idx = self.out.weapons.len() as u32;
        self.out.weapons.push(w);
        Ok((AssetRef::Weapon(idx), name))
    }

    /// One `FxElemVisuals` (4 bytes) whose meaning depends on the element type.
    fn fx_visual(&mut self, v: Rec<'a>, elem_type: u8) -> R<()> {
        match elem_type {
            0..=5 => {
                self.asset(AssetType::Material, v.u32(0), v.loc(0))?;
            }
            6 => {
                self.asset(AssetType::XModel, v.u32(0), v.loc(0))?;
            }
            9 | 11 => {
                self.xstring(v.u32(0))?;
            }
            _ => {}
        }
        Ok(())
    }

    fn fx(&mut self) -> R<(AssetRef, String)> {
        let h = self.read(36)?;
        self.push(VIRTUAL);
        let name = self.xstring(h.u32(0))?.unwrap_or_default();
        if h.u32(0x20) != 0 {
            self.alloc(4);
            let n = (h.i32(0x10) + h.i32(0x14) + h.i32(0x18)).max(0) as usize;
            let arr = self.read(256 * n)?;
            for i in 0..n {
                let e = arr.elem(i, 256);
                self.inline(e.u32(0xb8), 4, 96 * (e.u8(0xb6) as usize + 1))?;
                self.inline(e.u32(0xbc), 4, 48 * (e.u8(0xb7) as usize + 1))?;
                let ty = e.u8(0xb4);
                let vc = e.u8(0xb5) as usize;
                if ty == 10 {
                    if e.u32(0xc0) != 0 {
                        self.alloc(4);
                        let marks = self.read(8 * vc)?;
                        for k in 0..vc {
                            let m = marks.elem(k, 8);
                            self.asset(AssetType::Material, m.u32(0), m.loc(0))?;
                            self.asset(AssetType::Material, m.u32(4), m.loc(4))?;
                        }
                    }
                } else if vc > 1 {
                    if e.u32(0xc0) != 0 {
                        self.alloc(4);
                        let vis = self.read(4 * vc)?;
                        for k in 0..vc {
                            self.fx_visual(vis.elem(k, 4), ty)?;
                        }
                    }
                } else {
                    self.fx_visual(e.sub(0xc0, 4), ty)?;
                }
                for o in [0xdc, 0xe0, 0xe4] {
                    self.xstring(e.u32(o))?;
                }
                if e.u32(0xf8) != 0 {
                    self.alloc(4);
                    let t = self.read(28)?;
                    self.inline(t.u32(0x10), 4, 20 * t.i32(0xc).max(0) as usize)?;
                    self.inline(t.u32(0x18), 2, 2 * t.i32(0x14).max(0) as usize)?;
                }
            }
        }
        self.pop();
        Ok((AssetRef::Other(AssetType::Fx), name))
    }

    fn impact_fx(&mut self) -> R<(AssetRef, String)> {
        let h = self.read(8)?;
        self.push(VIRTUAL);
        let name = self.xstring(h.u32(0))?.unwrap_or_default();
        if h.u32(4) != 0 {
            self.alloc(4);
            let arr = self.read(140 * 16)?;
            for i in 0..16 {
                let e = arr.elem(i, 140);
                for k in 0..35 {
                    self.asset(AssetType::Fx, e.u32(4 * k), e.loc(4 * k))?;
                }
            }
        }
        self.pop();
        Ok((AssetRef::Other(AssetType::ImpactFx), name))
    }

    fn rawfile(&mut self) -> R<(AssetRef, String)> {
        let h = self.read(12)?;
        self.push(VIRTUAL);
        let name = self.xstring(h.u32(0))?.unwrap_or_default();
        self.inline(h.u32(8), 1, h.i32(4).max(0) as usize + 1)?;
        self.pop();
        Ok((AssetRef::Other(AssetType::RawFile), name))
    }

    fn string_table(&mut self) -> R<(AssetRef, String)> {
        let h = self.read(16)?;
        self.push(VIRTUAL);
        let name = self.xstring(h.u32(0))?.unwrap_or_default();
        if h.u32(0xc) != 0 {
            self.alloc(4);
            let n = (h.i32(4).max(0) * h.i32(8).max(0)) as usize;
            let ptrs = self.read(4 * n)?;
            for i in 0..n {
                self.xstring(ptrs.u32(4 * i))?;
            }
        }
        self.pop();
        Ok((AssetRef::Other(AssetType::StringTable), name))
    }
}
