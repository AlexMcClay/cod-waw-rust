//! Small geometry helpers. World axes: X east, Y up, Z north (metres).

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct V3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl V3 {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
    pub fn sub(self, o: V3) -> V3 {
        V3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
    pub fn add(self, o: V3) -> V3 {
        V3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
    pub fn scale(self, s: f32) -> V3 {
        V3::new(self.x * s, self.y * s, self.z * s)
    }
    pub fn dot(self, o: V3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    pub fn len(self) -> f32 {
        self.dot(self).sqrt()
    }
    pub fn cross(self, o: V3) -> V3 {
        V3::new(self.y * o.z - self.z * o.y, self.z * o.x - self.x * o.z, self.x * o.y - self.y * o.x)
    }
    /// Unit vector, or zero for a zero-length input.
    pub fn normalize(self) -> V3 {
        let l = self.len();
        if l > 1e-12 {
            self.scale(1.0 / l)
        } else {
            V3::new(0.0, 0.0, 0.0)
        }
    }
    pub fn min(self, o: V3) -> V3 {
        V3::new(self.x.min(o.x), self.y.min(o.y), self.z.min(o.z))
    }
    pub fn max(self, o: V3) -> V3 {
        V3::new(self.x.max(o.x), self.y.max(o.y), self.z.max(o.z))
    }
}

/// Axis-aligned box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: V3,
    pub max: V3,
}

impl Aabb {
    pub fn new(min: V3, max: V3) -> Self {
        Self {
            min: V3::new(min.x.min(max.x), min.y.min(max.y), min.z.min(max.z)),
            max: V3::new(min.x.max(max.x), min.y.max(max.y), min.z.max(max.z)),
        }
    }

    pub fn center(&self) -> V3 {
        self.min.add(self.max).scale(0.5)
    }

    pub fn size(&self) -> V3 {
        self.max.sub(self.min)
    }

    /// Slab test. Returns the entry distance along `dir` (not normalised-safe:
    /// pass a unit vector to get metres), or `None` if missed / behind.
    pub fn ray_hit(&self, origin: V3, dir: V3, max_t: f32) -> Option<f32> {
        let mut tmin = 0.0f32;
        let mut tmax = max_t;
        for (o, d, lo, hi) in [
            (origin.x, dir.x, self.min.x, self.max.x),
            (origin.y, dir.y, self.min.y, self.max.y),
            (origin.z, dir.z, self.min.z, self.max.z),
        ] {
            if d.abs() < 1e-8 {
                if o < lo || o > hi {
                    return None;
                }
            } else {
                let inv = 1.0 / d;
                let (mut t0, mut t1) = ((lo - o) * inv, (hi - o) * inv);
                if t0 > t1 {
                    std::mem::swap(&mut t0, &mut t1);
                }
                tmin = tmin.max(t0);
                tmax = tmax.min(t1);
                if tmin > tmax {
                    return None;
                }
            }
        }
        Some(tmin)
    }

    /// Push a vertical cylinder (centre `x,z`, radius `r`) out of this box in
    /// the XZ plane. Returns the corrected position, or `None` if not touching.
    pub fn push_circle(&self, x: f32, z: f32, r: f32) -> Option<(f32, f32)> {
        let cx = x.clamp(self.min.x, self.max.x);
        let cz = z.clamp(self.min.z, self.max.z);
        let dx = x - cx;
        let dz = z - cz;
        let d2 = dx * dx + dz * dz;
        if d2 >= r * r {
            return None;
        }
        if d2 > 1e-10 {
            let d = d2.sqrt();
            let push = r - d;
            return Some((x + dx / d * push, z + dz / d * push));
        }
        // Centre is inside the box: push out along the shallowest axis.
        let left = x - self.min.x + r;
        let right = self.max.x - x + r;
        let back = z - self.min.z + r;
        let front = self.max.z - z + r;
        let m = left.min(right).min(back).min(front);
        Some(if m == left {
            (self.min.x - r, z)
        } else if m == right {
            (self.max.x + r, z)
        } else if m == back {
            (x, self.min.z - r)
        } else {
            (x, self.max.z + r)
        })
    }

    pub fn contains_xz(&self, x: f32, z: f32) -> bool {
        x >= self.min.x && x <= self.max.x && z >= self.min.z && z <= self.max.z
    }

    pub fn inflate_xz(&self, r: f32) -> Aabb {
        Aabb::new(
            V3::new(self.min.x - r, self.min.y, self.min.z - r),
            V3::new(self.max.x + r, self.max.y, self.max.z + r),
        )
    }
}

/// Ray vs sphere; `dir` must be unit length.
pub fn ray_sphere(origin: V3, dir: V3, center: V3, radius: f32) -> Option<f32> {
    let oc = origin.sub(center);
    let b = oc.dot(dir);
    let c = oc.dot(oc) - radius * radius;
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    let s = disc.sqrt();
    let t = -b - s;
    if t >= 0.0 {
        Some(t)
    } else if -b + s >= 0.0 {
        Some(0.0)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_box() {
        let b = Aabb::new(V3::new(-1.0, -1.0, 5.0), V3::new(1.0, 1.0, 6.0));
        let t = b.ray_hit(V3::new(0.0, 0.0, 0.0), V3::new(0.0, 0.0, 1.0), 100.0).unwrap();
        assert!((t - 5.0).abs() < 1e-5);
        assert!(b.ray_hit(V3::new(0.0, 0.0, 0.0), V3::new(0.0, 0.0, -1.0), 100.0).is_none());
        assert!(b.ray_hit(V3::new(0.0, 0.0, 0.0), V3::new(0.0, 0.0, 1.0), 4.0).is_none());
    }

    #[test]
    fn sphere() {
        let t = ray_sphere(V3::new(0.0, 0.0, 0.0), V3::new(1.0, 0.0, 0.0), V3::new(5.0, 0.0, 0.0), 1.0);
        assert!((t.unwrap() - 4.0).abs() < 1e-5);
        assert!(ray_sphere(V3::new(0.0, 0.0, 0.0), V3::new(0.0, 1.0, 0.0), V3::new(5.0, 0.0, 0.0), 1.0).is_none());
    }

    #[test]
    fn push_out() {
        let b = Aabb::new(V3::new(0.0, 0.0, 0.0), V3::new(1.0, 3.0, 1.0));
        let (x, _z) = b.push_circle(1.2, 0.5, 0.3).unwrap();
        assert!((x - 1.3).abs() < 1e-5);
        assert!(b.push_circle(2.0, 0.5, 0.3).is_none());
        let (x, z) = b.push_circle(0.5, 0.9, 0.3).unwrap();
        assert!(!b.inflate_xz(0.29).contains_xz(x, z));
    }
}
