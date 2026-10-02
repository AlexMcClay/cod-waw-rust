//! Dumps every effect, impact table and weapon effect of a zone as text.
//!
//! `cargo run --release -p waw_assets --example fxdump -- [zone] [out.txt]`
//! (output is derived from the game's data: keep it in `research/vfx/local/`).

use std::fmt::Write as _;
use waw_assets::t4::{self, fx, ZoneData};
use waw_assets::zone::decompress;
use waw_assets::Install;

fn r(x: &fx::Range) -> String {
    if x.amp == 0.0 {
        format!("{}", x.base)
    } else {
        format!("{}+{}", x.base, x.amp)
    }
}

fn mat_desc(zd: &ZoneData, i: u32) -> String {
    let m = &zd.materials[i as usize];
    let tex = m.color_map().map(|t| zd.image_name(t).to_string()).unwrap_or_default();
    format!("{} [{}] tex={} atlas={:?}", m.name, m.techset.as_deref().unwrap_or("?"), tex, m.atlas)
}

fn dump_effect(o: &mut String, zd: &ZoneData, e: &fx::Effect) {
    let _ = writeln!(
        o,
        "== {}  flags={:#x} loopLife={} looping={} oneshot={} emission={} prio={}",
        e.name, e.flags, e.msec_looping_life, e.looping, e.oneshot, e.emission, e.priority
    );
    for (i, el) in e.elems.iter().enumerate() {
        let group = if i < e.looping { "loop" } else if i < e.looping + e.oneshot { "once" } else { "emit" };
        let _ = writeln!(
            o,
            "  [{i}] {group} {} flags={:#010x} spawn={:?} delay={}+{} life={}+{} sort={} lit={}",
            fx::elem_type::name(el.elem_type),
            el.flags,
            el.spawn,
            el.spawn_delay_msec.base,
            el.spawn_delay_msec.amp,
            el.life_span_msec.base,
            el.life_span_msec.amp,
            el.sort_order,
            el.lighting_frac
        );
        let _ = writeln!(
            o,
            "      origin=({}, {}, {}) offR={} offH={} angles=({}, {}, {}) angVel=({}, {}, {}) rot0={} grav={} refl={}",
            r(&el.spawn_origin[0]),
            r(&el.spawn_origin[1]),
            r(&el.spawn_origin[2]),
            r(&el.spawn_offset_radius),
            r(&el.spawn_offset_height),
            r(&el.spawn_angles[0]),
            r(&el.spawn_angles[1]),
            r(&el.spawn_angles[2]),
            r(&el.angular_velocity[0]),
            r(&el.angular_velocity[1]),
            r(&el.angular_velocity[2]),
            r(&el.initial_rotation),
            r(&el.gravity),
            r(&el.reflection_factor)
        );
        let _ = writeln!(
            o,
            "      atlas={:?} wind={} spawnRange={} fadeIn={} fadeOut={} cull={} emitDist={} emitVar={}",
            el.atlas,
            el.wind_influence,
            r(&el.spawn_range),
            r(&el.fade_in_range),
            r(&el.fade_out_range),
            el.spawn_frustum_cull_radius,
            r(&el.emit_dist),
            r(&el.emit_dist_variance)
        );
        for v in &el.visuals {
            let d = match v {
                fx::Visual::Material(m) => format!("material {}", mat_desc(zd, *m)),
                fx::Visual::Model(m) => format!("model {}", zd.xmodels[*m as usize].name),
                fx::Visual::Effect(n) => format!("effect {n}"),
                fx::Visual::Sound(n) => format!("sound {n}"),
                fx::Visual::Mark(ms) => format!("mark {:?}", ms.map(|m| m.map(|m| zd.materials[m as usize].name.clone()))),
            };
            let _ = writeln!(o, "      visual: {d}");
        }
        for (k, n) in [("onImpact", &el.effect_on_impact), ("onDeath", &el.effect_on_death), ("emitted", &el.effect_emitted)] {
            if let Some(n) = n {
                let _ = writeln!(o, "      {k}: {n}");
            }
        }
        for (k, s) in el.vel.iter().enumerate() {
            let _ = writeln!(
                o,
                "      vel[{k}] local v={:?}+{:?} d={:?}+{:?} | world v={:?}+{:?} d={:?}+{:?}",
                s.local_velocity.base,
                s.local_velocity.amp,
                s.local_delta.base,
                s.local_delta.amp,
                s.world_velocity.base,
                s.world_velocity.amp,
                s.world_delta.base,
                s.world_delta.amp
            );
        }
        for (k, s) in el.vis.iter().enumerate() {
            let _ = writeln!(
                o,
                "      vis[{k}] color={:?}+{:?} rotD={}+{} rotT={}+{} size={:?}+{:?} scale={}+{}",
                s.base.color,
                s.amp.color,
                s.base.rotation_delta,
                s.amp.rotation_delta,
                s.base.rotation_total,
                s.amp.rotation_total,
                s.base.size,
                s.amp.size,
                s.base.scale,
                s.amp.scale
            );
        }
        if let Some(t) = &el.trail {
            let _ = writeln!(o, "      trail scroll={} repeat={} split={} verts={:?} inds={:?}", t.scroll_time_msec, t.repeat_dist, t.split_dist, t.verts, t.inds);
        }
    }
}

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "nazi_zombie_prototype".into());
    let out = std::env::args().nth(2);
    let install = Install::locate(&[]).expect("install");
    let data = decompress(&std::fs::read(install.fastfile(&name)).expect("read")).expect("decompress");
    let zd = t4::walk(data);
    let mut o = String::new();
    let _ = writeln!(o, "zone {name}: complete={} stopped={:?} fx={} impact tables={}", zd.complete(), zd.stopped, zd.fx.len(), zd.impact_fx.len());
    for w in &zd.weapons {
        let _ = writeln!(o, "weapon {}: {:?}", w.name, w.fx);
        if !w.bounce_sounds.is_empty() {
            let _ = writeln!(o, "  bounce sounds by surface: {:?}", w.bounce_sounds);
        }
    }
    for t in &zd.impact_fx {
        let _ = writeln!(o, "impact table {}", t.name);
        for (i, e) in t.entries.iter().enumerate() {
            let _ = writeln!(o, "  type {i}: flesh {:?}", e.flesh);
            for (s, f) in e.nonflesh.iter().enumerate() {
                if let Some(f) = f {
                    let _ = writeln!(o, "    {:>2} {:<13} {f}", s, fx::SURFACE_TYPES.get(s).unwrap_or(&"?"));
                }
            }
        }
    }
    // Surface types of the world's materials (what bullets and grenades hit).
    if let Some(w) = &zd.world {
        let mut seen = std::collections::BTreeMap::new();
        for s in &w.surfaces {
            if let Some(m) = s.material.map(|m| &zd.materials[m as usize]) {
                let types: Vec<&str> = (0..31).filter(|b| m.surface_type_bits & (1 << b) != 0).map(|b| fx::SURFACE_TYPES[b]).collect();
                seen.insert(m.name.clone(), format!("{:#010x} {types:?}", m.surface_type_bits));
            }
        }
        for (n, t) in seen {
            let _ = writeln!(o, "world material {n}: {t}");
        }
    }
    for e in &zd.fx {
        if e.is_stub() {
            let _ = writeln!(o, "== {} (other zone)", e.name);
        } else {
            dump_effect(&mut o, &zd, e);
        }
    }
    match out {
        Some(p) => std::fs::write(&p, o).expect("write"),
        None => print!("{o}"),
    }
}
