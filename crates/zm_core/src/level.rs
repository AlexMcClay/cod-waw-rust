//! The playable map: an original three-room bunker.
//!
//! ```text
//!   z=16 +-----------W------+
//!        |   North wing (C) W                 W = boarded window
//!        W        [crate]   |                 D = debris (buyable)
//!   z=8  +---D D-----------+------W---+
//!        |                            |
//!        W        Main hall (A)       D   East wing (B)  W
//!        |                            |
//!   z=-8 +-------W-----------W--------+
//!       x=-10                       x=10           x=20
//! ```

use crate::geom::{Aabb, V3};
use crate::rules::BOARDS_PER_WINDOW;

pub const WALL_H: f32 = 4.0;
pub const WALL_T: f32 = 0.4;
pub const WINDOW_W: f32 = 1.6;
pub const SILL: f32 = 0.9;
pub const LINTEL: f32 = 2.3;
pub const DOOR_TOP: f32 = 2.6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    Main = 0,
    East = 1,
    North = 2,
}

#[derive(Debug, Clone)]
pub struct Window {
    pub center: V3,
    /// Unit vector pointing *out* of the building (XZ only).
    pub outward: (f32, f32),
    /// Index into [`Level::areas`] (for the bunker, an [`Area`]).
    pub area: usize,
    /// Boards when fully barricaded.
    pub boards: u8,
    /// Real board models (brush submodel number, world origin), bottom
    /// first. Empty for the procedural bunker.
    pub board_models: Vec<(usize, V3)>,
}

impl Window {
    pub fn inside_point(&self) -> (f32, f32) {
        (self.center.x - self.outward.0 * 0.9, self.center.z - self.outward.1 * 0.9)
    }
    pub fn outside_point(&self) -> (f32, f32) {
        (self.center.x + self.outward.0 * 0.9, self.center.z + self.outward.1 * 0.9)
    }
    /// Where zombies for this window appear (before jitter).
    pub fn spawn_point(&self) -> (f32, f32) {
        (self.center.x + self.outward.0 * 9.0, self.center.z + self.outward.1 * 9.0)
    }
    /// Unit vector along the wall.
    pub fn tangent(&self) -> (f32, f32) {
        (-self.outward.1, self.outward.0)
    }
}

#[derive(Debug, Clone)]
pub struct Door {
    pub name: String,
    pub blocker: Aabb,
    pub cost: u32,
    /// Area made reachable by buying this door.
    pub opens: usize,
}

#[derive(Debug, Clone)]
pub struct WallBuy {
    pub weapon_id: String,
    pub cost: u32,
    /// Point on the inner wall surface (y = chest height).
    pub pos: V3,
    /// Unit vector pointing into the room.
    pub facing: (f32, f32),
}

#[derive(Debug, Clone)]
pub struct Level {
    /// Solid wall pieces (rendered and collided).
    pub walls: Vec<Aabb>,
    /// Invisible full-height blockers in window openings (players can't climb out).
    pub window_fills: Vec<Aabb>,
    pub windows: Vec<Window>,
    pub doors: Vec<Door>,
    pub wall_buys: Vec<WallBuy>,
    pub crate_box: Aabb,
    /// Whether `crate_box` itself blocks (false when the box model has its
    /// own collision, as on the real maps).
    pub crate_solid: bool,
    /// Interior floor rectangles per area, indexed by `Area as usize`.
    pub areas: Vec<Aabb>,
    pub player_start: (f32, f32),
    /// Height of the player's feet at the start (0 for the flat bunker).
    pub player_start_y: f32,
    /// Initial look direction (radians, as `PlayerCtl::yaw`).
    pub player_yaw: f32,
    /// Ceiling lights (x, y, z).
    pub lights: Vec<V3>,
    /// Zombie spawn points with the area that must be open to use them.
    /// Empty for the bunker, which spawns outside each window instead.
    pub spawners: Vec<(V3, usize)>,
    /// The map's own box list (game weapon names); `None` offers every weapon.
    pub crate_weapons: Option<&'static [&'static str]>,
}

#[derive(Clone, Copy)]
enum Opening {
    Window(f32),
    Gap(f32, f32),
}

/// Wall along X at fixed z, from x0 to x1, with openings.
fn wall_x(out: &mut Level, z: f32, x0: f32, x1: f32, openings: &[Opening], inward: f32, area: Area) {
    build_wall(out, true, z, x0, x1, openings, inward, area);
}

/// Wall along Z at fixed x, from z0 to z1, with openings.
fn wall_z(out: &mut Level, x: f32, z0: f32, z1: f32, openings: &[Opening], inward: f32, area: Area) {
    build_wall(out, false, x, z0, z1, openings, inward, area);
}

#[allow(clippy::too_many_arguments)]
fn build_wall(
    out: &mut Level,
    along_x: bool,
    fixed: f32,
    a0: f32,
    a1: f32,
    openings: &[Opening],
    inward: f32,
    area: Area,
) {
    let h = WALL_T / 2.0;
    let mk = |s0: f32, s1: f32, y0: f32, y1: f32| {
        if along_x {
            Aabb::new(V3::new(s0, y0, fixed - h), V3::new(s1, y1, fixed + h))
        } else {
            Aabb::new(V3::new(fixed - h, y0, s0), V3::new(fixed + h, y1, s1))
        }
    };
    // Gaps sorted along the wall.
    let mut gaps: Vec<(f32, f32, Opening)> = openings
        .iter()
        .map(|o| match *o {
            Opening::Window(c) => (c - WINDOW_W / 2.0, c + WINDOW_W / 2.0, *o),
            Opening::Gap(s, e) => (s, e, *o),
        })
        .collect();
    gaps.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    // Extend walls by half thickness so corners close.
    let mut cursor = a0 - h;
    for (s, e, kind) in gaps {
        if s > cursor {
            out.walls.push(mk(cursor, s, 0.0, WALL_H));
        }
        match kind {
            Opening::Window(c) => {
                out.walls.push(mk(s, e, 0.0, SILL));
                out.walls.push(mk(s, e, LINTEL, WALL_H));
                out.window_fills.push(mk(s, e, 0.0, WALL_H));
                let outward = if along_x { (0.0, -inward) } else { (-inward, 0.0) };
                let center = if along_x {
                    V3::new(c, (SILL + LINTEL) / 2.0, fixed)
                } else {
                    V3::new(fixed, (SILL + LINTEL) / 2.0, c)
                };
                out.windows.push(Window { center, outward, area: area as usize, boards: BOARDS_PER_WINDOW, board_models: Vec::new() });
            }
            Opening::Gap(..) => {
                out.walls.push(mk(s, e, DOOR_TOP, WALL_H));
            }
        }
        cursor = e;
    }
    if a1 + h > cursor {
        out.walls.push(mk(cursor, a1 + h, 0.0, WALL_H));
    }
}

impl Level {
    pub fn bunker() -> Level {
        let mut l = Level {
            walls: Vec::new(),
            window_fills: Vec::new(),
            windows: Vec::new(),
            doors: Vec::new(),
            wall_buys: Vec::new(),
            crate_box: Aabb::new(V3::new(-7.8, 0.0, 14.9), V3::new(-6.2, 0.9, 15.7)),
            crate_solid: true,
            areas: vec![
                Aabb::new(V3::new(-10.0, 0.0, -8.0), V3::new(10.0, WALL_H, 8.0)),
                Aabb::new(V3::new(10.0, 0.0, -6.0), V3::new(20.0, WALL_H, 6.0)),
                Aabb::new(V3::new(-10.0, 0.0, 8.0), V3::new(4.0, WALL_H, 16.0)),
            ],
            player_start: (0.0, 0.0),
            player_start_y: 0.0,
            player_yaw: 0.0,
            spawners: Vec::new(),
            crate_weapons: None,
            lights: vec![
                V3::new(-5.0, 3.6, 0.0),
                V3::new(5.0, 3.6, 0.0),
                V3::new(15.0, 3.6, 0.0),
                V3::new(-3.0, 3.6, 12.0),
            ],
        };
        use Area::*;
        use Opening::*;
        // Main hall.
        wall_x(&mut l, -8.0, -10.0, 10.0, &[Window(-4.0), Window(5.0)], 1.0, Main);
        wall_z(&mut l, -10.0, -8.0, 8.0, &[Window(2.0)], 1.0, Main);
        wall_x(&mut l, 8.0, -10.0, 10.0, &[Gap(-6.0, -3.0), Window(7.0)], -1.0, Main);
        wall_z(&mut l, 10.0, -8.0, 8.0, &[Gap(-1.5, 1.5)], -1.0, Main);
        // East wing.
        wall_x(&mut l, -6.0, 10.0, 20.0, &[Window(15.0)], 1.0, East);
        wall_x(&mut l, 6.0, 10.0, 20.0, &[Window(14.0)], -1.0, East);
        wall_z(&mut l, 20.0, -6.0, 6.0, &[Window(0.0)], -1.0, East);
        // North wing.
        wall_z(&mut l, -10.0, 8.0, 16.0, &[Window(12.0)], 1.0, North);
        wall_x(&mut l, 16.0, -10.0, 4.0, &[Window(-2.0)], -1.0, North);
        wall_z(&mut l, 4.0, 8.0, 16.0, &[Window(12.0)], -1.0, North);

        let h = WALL_T / 2.0;
        l.doors.push(Door {
            name: "East wing".into(),
            blocker: Aabb::new(V3::new(10.0 - h, 0.0, -1.5), V3::new(10.0 + h, DOOR_TOP, 1.5)),
            cost: 750,
            opens: East as usize,
        });
        l.doors.push(Door {
            name: "North wing".into(),
            blocker: Aabb::new(V3::new(-6.0, 0.0, 8.0 - h), V3::new(-3.0, DOOR_TOP, 8.0 + h)),
            cost: 1000,
            opens: North as usize,
        });

        let y = 1.5;
        let inner = 10.0 - h - 0.01;
        l.wall_buys = vec![
            WallBuy { weapon_id: "kar98k".into(), cost: 200, pos: V3::new(0.0, y, -8.0 + h + 0.01), facing: (0.0, 1.0) },
            WallBuy { weapon_id: "m1carbine".into(), cost: 600, pos: V3::new(-inner, y, -5.0), facing: (1.0, 0.0) },
            WallBuy { weapon_id: "m1garand".into(), cost: 600, pos: V3::new(inner, y, 5.0), facing: (-1.0, 0.0) },
            WallBuy { weapon_id: "thompson".into(), cost: 1200, pos: V3::new(20.0 - h - 0.01, y, -4.0), facing: (-1.0, 0.0) },
            WallBuy { weapon_id: "doublebarrel".into(), cost: 1200, pos: V3::new(18.0, y, 6.0 - h - 0.01), facing: (0.0, -1.0) },
            WallBuy { weapon_id: "trenchgun".into(), cost: 1500, pos: V3::new(1.5, y, 16.0 - h - 0.01), facing: (0.0, -1.0) },
            WallBuy { weapon_id: "bar".into(), cost: 1800, pos: V3::new(4.0 - h - 0.01, y, 9.5), facing: (-1.0, 0.0) },
        ];
        l
    }

    /// Which interior area contains the point, if any.
    pub fn area_at(&self, x: f32, z: f32) -> Option<usize> {
        self.areas.iter().position(|a| a.contains_xz(x, z))
    }

    /// Combined XZ bounds of all interior areas.
    pub fn interior_bounds(&self) -> Aabb {
        let Some(first) = self.areas.first() else {
            return Aabb::new(V3::new(0.0, 0.0, 0.0), V3::new(1.0, WALL_H, 1.0));
        };
        let mut min = first.min;
        let mut max = first.max;
        for a in &self.areas {
            min = V3::new(min.x.min(a.min.x), 0.0, min.z.min(a.min.z));
            max = V3::new(max.x.max(a.max.x), WALL_H, max.z.max(a.max.z));
        }
        Aabb::new(min, max)
    }

    /// Static colliders for the player (walls + window fills + crate).
    pub fn player_colliders(&self) -> Vec<Aabb> {
        let mut v = self.walls.clone();
        v.extend(self.window_fills.iter().copied());
        if self.crate_solid {
            v.push(self.crate_box);
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bunker_shape() {
        let l = Level::bunker();
        assert_eq!(l.windows.len(), 10);
        assert_eq!(l.doors.len(), 2);
        assert_eq!(l.window_fills.len(), l.windows.len());
        // Every window's inside point is in its own area and the outside
        // spawn point is outside every area.
        for w in &l.windows {
            let (ix, iz) = w.inside_point();
            assert_eq!(l.area_at(ix, iz), Some(w.area), "{w:?}");
            let (sx, sz) = w.spawn_point();
            assert_eq!(l.area_at(sx, sz), None, "{w:?}");
        }
        // Start isn't inside a wall.
        let (px, pz) = l.player_start;
        assert!(l.player_colliders().iter().all(|c| c.push_circle(px, pz, 0.35).is_none()));
        // Wall buys sit inside their rooms.
        for wb in &l.wall_buys {
            let x = wb.pos.x + wb.facing.0 * 0.5;
            let z = wb.pos.z + wb.facing.1 * 0.5;
            assert!(l.area_at(x, z).is_some(), "{}", wb.weapon_id);
        }
    }
}
