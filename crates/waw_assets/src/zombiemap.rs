//! Reads the gameplay layout of a Zombies map from its entities: windows and
//! their boards, zombie spawners, buyable doors/debris, wall weapons, the
//! mystery box and path nodes. Positions are in game units (inches, Z up).
//!
//! The entity conventions (targetnames such as `exterior_goal` or
//! `zombie_spawner_init`) are those the map itself uses; the game logic that
//! consumes this data is our own.

use crate::mapents::{Entity, EntityList};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Board {
    pub submodel: usize,
    pub origin: [f32; 3],
}

#[derive(Debug, Clone)]
pub struct Window {
    /// Where zombies stand outside before tearing boards.
    pub outside: [f32; 3],
    /// The struct at the window itself (floor level, inside face).
    pub at: [f32; 3],
    pub boards: Vec<Board>,
    /// Player clip brush for the opening.
    pub clip: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpawnGroup {
    /// Active from the start.
    Init,
    /// Unlocked by the door (`zombie_spawner_door`).
    Door,
    /// Unlocked by the upstairs debris (`zombie_spawner_upstairs`).
    Upstairs,
    Other,
}

impl SpawnGroup {
    fn from_targetname(t: &str) -> SpawnGroup {
        match t {
            "zombie_spawner_init" => SpawnGroup::Init,
            "zombie_spawner_door" => SpawnGroup::Door,
            "zombie_spawner_upstairs" => SpawnGroup::Upstairs,
            _ => SpawnGroup::Other,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Spawner {
    pub origin: [f32; 3],
    pub group: SpawnGroup,
}

/// A buyable door or debris pile (possibly with several use triggers).
#[derive(Debug, Clone)]
pub struct Door {
    pub name: String,
    pub cost: u32,
    pub is_debris: bool,
    /// Origins of the triggers the player uses.
    pub triggers: Vec<[f32; 3]>,
    /// Brush submodels that block the way (hidden/moved when bought).
    pub blockers: Vec<usize>,
    /// Static props that are part of the blocker (e.g. a couch).
    pub props: Vec<(String, [f32; 3], [f32; 3])>,
    /// Spawner group this purchase unlocks.
    pub unlocks: Option<SpawnGroup>,
}

#[derive(Debug, Clone)]
pub struct WallBuy {
    pub weapon: String,
    pub cost: u32,
    pub origin: [f32; 3],
    pub trigger: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct ZombieMap {
    pub player_start: [f32; 3],
    pub player_yaw: f32,
    pub start_points: Vec<[f32; 3]>,
    pub windows: Vec<Window>,
    pub spawners: Vec<Spawner>,
    pub doors: Vec<Door>,
    pub wall_buys: Vec<WallBuy>,
    pub chest: Option<WallBuy>,
    pub path_nodes: Vec<[f32; 3]>,
    /// Traversal links (begin, end) such as window vaults.
    pub traversals: Vec<([f32; 3], [f32; 3])>,
    pub lights: Vec<[f32; 3]>,
    /// Static props placed as script models: (model, origin, angles).
    pub props: Vec<(String, [f32; 3], [f32; 3])>,
    /// Ambient sound emitters (structs with `script_sound`).
    pub ambient: Vec<AmbientEmitter>,
}

/// How an ambient emitter plays (the `script_label` of its struct).
#[derive(Debug, Clone, PartialEq)]
pub enum AmbientKind {
    /// A one-shot every `min..max` seconds.
    Random { min: f32, max: f32 },
    /// A loop that never stops.
    Looper,
    /// A loop whose source slides along the segment to `end`, kept at the
    /// point closest to the listener.
    Line { end: [f32; 3] },
}

#[derive(Debug, Clone)]
pub struct AmbientEmitter {
    pub alias: String,
    pub origin: [f32; 3],
    pub kind: AmbientKind,
}

fn cost(e: &Entity) -> u32 {
    e.f32("zombie_cost").map(|c| c.max(0.0) as u32).unwrap_or(0)
}

impl ZombieMap {
    pub fn from_entities(ents: &[Entity]) -> ZombieMap {
        let mut by_name: HashMap<&str, Vec<&Entity>> = HashMap::new();
        for e in ents {
            if !e.targetname().is_empty() {
                by_name.entry(e.targetname()).or_default().push(e);
            }
        }
        let targets = |e: &Entity| -> Vec<&Entity> { by_name.get(e.target()).cloned().unwrap_or_default() };

        let start = ents.by_class("info_player_start").next();
        let player_start = start.map(|e| e.origin()).unwrap_or([0.0; 3]);
        let player_yaw = start.map(|e| e.angles()[1]).unwrap_or(0.0);
        let start_points = ents.by_targetname("initial_spawn_points").map(|e| e.origin()).collect();

        let mut windows = Vec::new();
        for goal in ents.by_targetname("exterior_goal") {
            let mut w = Window { outside: goal.origin(), at: goal.origin(), boards: Vec::new(), clip: None };
            for t in targets(goal) {
                match t.classname() {
                    "script_struct" => w.at = t.origin(),
                    "script_brushmodel" if t.get("script_noteworthy") == Some("clip") => w.clip = t.submodel(),
                    "script_brushmodel" => {
                        if let Some(m) = t.submodel() {
                            w.boards.push(Board { submodel: m, origin: t.origin() });
                        }
                    }
                    _ => {}
                }
            }
            // Bottom board first: zombies tear from the top down.
            w.boards.sort_by(|a, b| a.origin[2].total_cmp(&b.origin[2]));
            windows.push(w);
        }

        let spawners = ents
            .iter()
            .filter(|e| e.classname().starts_with("actor_") && e.targetname().starts_with("zombie_spawner"))
            .map(|e| Spawner { origin: e.origin(), group: SpawnGroup::from_targetname(e.targetname()) })
            .collect();

        // Doors/debris: triggers sharing a target are one purchase.
        let mut doors: Vec<Door> = Vec::new();
        for trig in ents.iter().filter(|e| matches!(e.targetname(), "zombie_door" | "zombie_debris")) {
            if let Some(d) = doors.iter_mut().find(|d| d.name == trig.target()) {
                d.triggers.push(trig.origin());
                continue;
            }
            let mut d = Door {
                name: trig.target().to_string(),
                cost: cost(trig),
                is_debris: trig.targetname() == "zombie_debris",
                triggers: vec![trig.origin()],
                blockers: Vec::new(),
                props: Vec::new(),
                unlocks: None,
            };
            for t in targets(trig) {
                if t.classname() == "script_brushmodel" {
                    d.blockers.extend(t.submodel());
                } else if t.classname() == "script_model" {
                    d.props.push((t.get("model").unwrap_or("").to_string(), t.origin(), t.angles()));
                }
                if !t.target().is_empty() {
                    let g = SpawnGroup::from_targetname(t.target());
                    if g != SpawnGroup::Other {
                        d.unlocks = Some(g);
                    }
                }
            }
            doors.push(d);
        }

        let buy = |e: &Entity| WallBuy {
            weapon: e.get("zombie_weapon_upgrade").unwrap_or("").to_string(),
            cost: cost(e),
            origin: e.origin(),
            trigger: e.submodel(),
        };
        let wall_buys = ents
            .iter()
            .filter(|e| matches!(e.targetname(), "weapon_upgrade" | "weapon_cabinet_use"))
            .map(buy)
            .collect();
        let chest = ents.by_targetname("treasure_chest_use").next().map(buy);

        let path_nodes = ents.by_class("node_pathnode").map(|e| e.origin()).collect();
        let mut traversals = Vec::new();
        for b in ents.by_class("node_negotiation_begin") {
            if let Some(end) = by_name.get(b.target()).and_then(|v| v.iter().find(|e| e.classname() == "node_negotiation_end")) {
                traversals.push((b.origin(), end.origin()));
            }
        }
        let lights = ents.by_class("light").map(|e| e.origin()).collect();
        let props = ents
            .by_class("script_model")
            .filter(|e| e.get("model").is_some())
            .map(|e| (e.get("model").unwrap_or("").to_string(), e.origin(), e.angles()))
            .collect();

        let ambient = ents
            .iter()
            .filter_map(|e| {
                let alias = e.get("script_sound")?.to_string();
                let kind = match e.get("script_label")? {
                    "random" => AmbientKind::Random { min: e.f32("script_wait_min").unwrap_or(1.0), max: e.f32("script_wait_max").unwrap_or(3.0) },
                    "looper" => AmbientKind::Looper,
                    "line_emitter" => AmbientKind::Line { end: targets(e).first().map(|t| t.origin())? },
                    _ => return None,
                };
                Some(AmbientEmitter { alias, origin: e.origin(), kind })
            })
            .collect();
        ZombieMap {
            ambient,
            player_start,
            player_yaw,
            start_points,
            windows,
            spawners,
            doors,
            wall_buys,
            chest,
            path_nodes,
            traversals,
            lights,
            props,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapents;

    const TEXT: &str = r#"
{
"classname" "info_player_start"
"origin" "-37 202 57"
"angles" "0 180 0"
}
{
"target" "w1"
"targetname" "exterior_goal"
"origin" "-266 -191 22"
"classname" "script_struct"
}
{
"targetname" "w1"
"classname" "script_brushmodel"
"origin" "-217 -193 90"
"model" "*2"
}
{
"targetname" "w1"
"classname" "script_brushmodel"
"origin" "-217 -191 41"
"model" "*5"
}
{
"targetname" "w1"
"classname" "script_struct"
"origin" "-218 -191 22"
}
{
"targetname" "w1"
"classname" "script_brushmodel"
"script_noteworthy" "clip"
"origin" "-219 -190 52"
"model" "*8"
}
{
"zombie_cost" "1000"
"targetname" "zombie_door"
"classname" "trigger_use"
"target" "auto34"
"origin" "178 574 51"
"model" "*46"
}
{
"zombie_cost" "1000"
"targetname" "zombie_door"
"classname" "trigger_use"
"target" "auto34"
"origin" "176 607 51"
"model" "*51"
}
{
"classname" "script_brushmodel"
"targetname" "auto34"
"target" "zombie_spawner_door"
"origin" "210 587 47"
"model" "*88"
}
{
"script_noteworthy" "zombie_spawner"
"targetname" "zombie_spawner_door"
"origin" "-992.5 -51.5 9.8"
"classname" "actor_axis_zombie_ger_ber_sshonor"
}
{
"zombie_cost" "200"
"zombie_weapon_upgrade" "kar98k"
"targetname" "weapon_upgrade"
"classname" "trigger_use"
"origin" "-206 242 60"
"model" "*52"
}
{
"classname" "node_pathnode"
"origin" "188.4 151.2 173"
}
"#;

    #[test]
    fn reads_layout() {
        let m = ZombieMap::from_entities(&mapents::parse(TEXT));
        assert_eq!(m.player_start, [-37.0, 202.0, 57.0]);
        assert_eq!(m.player_yaw, 180.0);
        assert_eq!(m.windows.len(), 1);
        let w = &m.windows[0];
        assert_eq!(w.at, [-218.0, -191.0, 22.0]);
        assert_eq!(w.clip, Some(8));
        assert_eq!(w.boards.iter().map(|b| b.submodel).collect::<Vec<_>>(), vec![5, 2]);
        assert_eq!(m.doors.len(), 1);
        assert_eq!(m.doors[0].triggers.len(), 2);
        assert_eq!(m.doors[0].blockers, vec![88]);
        assert_eq!(m.doors[0].unlocks, Some(SpawnGroup::Door));
        assert!(!m.doors[0].is_debris);
        assert_eq!(m.spawners[0].group, SpawnGroup::Door);
        assert_eq!(m.wall_buys[0].weapon, "kar98k");
        assert_eq!(m.wall_buys[0].cost, 200);
        assert_eq!(m.path_nodes.len(), 1);
    }

    #[test]
    #[ignore]
    fn real_nacht_layout() {
        let root = std::env::var("UNDEAD_WAW").expect("set UNDEAD_WAW");
        let zone = crate::zone::Zone::load(&std::path::Path::new(&root).join("zone/english/nazi_zombie_prototype.ff")).unwrap();
        let ents = mapents::parse(mapents::find_in_zone(&zone.data).unwrap());
        assert_eq!(ents.len(), 1037);
        let m = ZombieMap::from_entities(&ents);
        assert_eq!(m.windows.len(), 12);
        assert!(m.windows.iter().all(|w| (5..=8).contains(&w.boards.len()) && w.clip.is_some()), "{:?}", m.windows);
        assert_eq!(m.wall_buys.len(), 9);
        assert!(m.chest.is_some());
        assert_eq!(m.path_nodes.len(), 634);
        assert_eq!(m.traversals.len(), 12);
        assert_eq!(m.doors.len(), 3);
        assert_eq!(m.spawners.len(), 43);
        // 43 random amb_spooky, 8 light + 5 fire loopers, 2 zombie line emitters.
        let count = |k: fn(&AmbientKind) -> bool| m.ambient.iter().filter(|a| k(&a.kind)).count();
        assert_eq!(count(|k| matches!(k, AmbientKind::Random { .. })), 43);
        assert_eq!(count(|k| matches!(k, AmbientKind::Looper)), 13);
        assert_eq!(count(|k| matches!(k, AmbientKind::Line { .. })), 2);
    }
}
