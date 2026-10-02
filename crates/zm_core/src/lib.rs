//! Engine-independent core of *Undead Rounds*.
//!
//! Everything in here is plain Rust with no Bevy dependency so it can be unit
//! tested quickly and reused by other front-ends:
//!
//! * [`wav`]        – RIFF/WAVE reader that turns MS-ADPCM / IMA-ADPCM into PCM16
//!                    (the sounds shipped in World at War's `.iwd` archives are MS-ADPCM).
//! * [`weaponfile`] – parser for the `WEAPONFILE\key\value\...` text format.
//! * [`weapons`]    – built-in weapon table, optionally overridden from weapon files.
//! * [`grenade`]    – hand grenade stats, allowance, radius damage and bounces.
//! * [`rules`]      – round / health / points / drop-rate maths.
//! * [`geom`]       – tiny 2D/3D maths helpers (AABBs, ray casts, circle push-out).
//! * [`level`]      – the bunker layout: walls, windows, debris doors, wall buys.
//! * [`nav`]        – grid navigation (A*) used by the zombies once inside.
//! * [`trimesh`]    – triangle-soup collision (ray casts, sphere push-out) for real maps.
//! * [`navgraph`]   – waypoint-graph navigation over a map's path nodes.

pub mod geom;
pub mod grenade;
pub mod level;
pub mod nav;
pub mod navgraph;
pub mod rules;
pub mod trimesh;
pub mod wav;
pub mod weaponfile;
pub mod weapons;
