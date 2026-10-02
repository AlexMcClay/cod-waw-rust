//! A map's "look" as its own data defines it: the fog set by its art script
//! (`maps/createart/<map>_art.gsc`) and the vision set it switches to
//! (`vision/<name>.vision`: the film grade). Nothing here is specific to one
//! map; any World at War level that ships these files works.

use std::collections::HashMap;

/// `SetVolFog` / `SetExpFog` parameters, in game units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fog {
    /// Distance where fog starts.
    pub start: f32,
    /// Distance where fog reaches 50 %.
    pub halfway: f32,
    /// Height over which density halves (0 = no height falloff).
    pub half_height: f32,
    /// Height below which density is full.
    pub base_height: f32,
    pub color: [f32; 3],
}

/// The film grade of a vision set (`r_film*`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Film {
    pub enabled: bool,
    pub contrast: f32,
    pub brightness: f32,
    pub desaturation: f32,
    pub invert: bool,
    pub light_tint: [f32; 3],
    pub dark_tint: [f32; 3],
}

impl Default for Film {
    fn default() -> Self {
        Film { enabled: false, contrast: 1.0, brightness: 0.0, desaturation: 0.0, invert: false, light_tint: [1.0; 3], dark_tint: [1.0; 3] }
    }
}

/// A vision set file: its film grade and glow switch.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Vision {
    pub film: Film,
    pub glow: bool,
}

/// Splits a numeric argument list like `165, 835, 0.5` (variables resolved
/// through `vars`).
fn args(text: &str, vars: &HashMap<String, f32>) -> Vec<Option<f32>> {
    text.split(',')
        .map(|a| {
            let a = a.trim();
            a.parse::<f32>().ok().or_else(|| vars.get(&a.to_ascii_lowercase()).copied())
        })
        .collect()
}

/// The text between the parentheses of the first call to `func` (case
/// insensitive) that is not commented out.
fn call_args<'a>(script: &'a str, func: &str) -> Option<&'a str> {
    let lower = script.to_ascii_lowercase();
    let func = func.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(&func) {
        let at = from + i;
        from = at + func.len();
        let line_start = lower[..at].rfind('\n').map_or(0, |n| n + 1);
        if lower[line_start..at].contains("//") {
            continue;
        }
        let rest = &script[at + func.len()..];
        let open = rest.find('(')?;
        if !rest[..open].trim().is_empty() {
            continue;
        }
        let close = rest[open..].find(')')?;
        return Some(&rest[open + 1..open + close]);
    }
    None
}

/// Numeric `name = value;` assignments (fog scripts keep their values in
/// locals before calling `SetVolFog`). The first assignment wins: later
/// ones override them for split screen only.
fn assignments(script: &str) -> HashMap<String, f32> {
    let mut vars = HashMap::new();
    for line in script.lines() {
        let line = line.split("//").next().unwrap_or("");
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            let v = v.trim().trim_end_matches(';').trim();
            if !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                if let Ok(n) = v.parse::<f32>() {
                    vars.entry(k.to_ascii_lowercase()).or_insert(n);
                }
            }
        }
    }
    vars
}

/// The fog an art script sets, from `SetVolFog(start, halfway, halfheight,
/// baseheight, r, g, b, time)` or `SetExpFog(start, halfway, r, g, b, time)`.
pub fn fog_from_script(script: &str) -> Option<Fog> {
    let vars = assignments(script);
    if let Some(a) = call_args(script, "setvolfog") {
        let v = args(a, &vars);
        if v.len() >= 7 {
            return Some(Fog {
                start: v[0]?,
                halfway: v[1]?,
                half_height: v[2]?,
                base_height: v[3]?,
                color: [v[4]?, v[5]?, v[6]?],
            });
        }
    }
    if let Some(a) = call_args(script, "setexpfog") {
        let v = args(a, &vars);
        if v.len() >= 5 {
            return Some(Fog { start: v[0]?, halfway: v[1]?, half_height: 0.0, base_height: 0.0, color: [v[2]?, v[3]?, v[4]?] });
        }
    }
    None
}

/// The vision set an art script switches to (`set_all_players_visionset`,
/// `VisionSetNaked`, ...): the first quoted name after "visionset".
pub fn vision_name_from_script(script: &str) -> Option<String> {
    let lower = script.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find("visionset") {
        let at = from + i;
        from = at + 9;
        let line_start = lower[..at].rfind('\n').map_or(0, |n| n + 1);
        if lower[line_start..at].contains("//") {
            continue;
        }
        let rest = &script[at..];
        let line = rest.lines().next().unwrap_or("");
        if let Some(q) = line.find('"') {
            if let Some(e) = line[q + 1..].find('"') {
                return Some(line[q + 1..q + 1 + e].to_string());
            }
        }
    }
    None
}

/// Parses a `.vision` file (`dvar "value"` lines).
pub fn parse_vision(text: &str) -> Vision {
    let mut kv: HashMap<String, String> = HashMap::new();
    for line in text.lines() {
        let mut parts = line.trim().splitn(2, char::is_whitespace);
        let (Some(k), Some(v)) = (parts.next(), parts.next()) else { continue };
        kv.insert(k.to_ascii_lowercase(), v.trim().trim_matches('"').to_string());
    }
    let f = |k: &str, d: f32| kv.get(k).and_then(|v| v.parse().ok()).unwrap_or(d);
    let v3 = |k: &str| {
        kv.get(k)
            .map(|v| {
                let n: Vec<f32> = v.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                [n.first().copied().unwrap_or(1.0), n.get(1).copied().unwrap_or(1.0), n.get(2).copied().unwrap_or(1.0)]
            })
            .unwrap_or([1.0; 3])
    };
    Vision {
        film: Film {
            enabled: f("r_filmenable", 0.0) != 0.0,
            contrast: f("r_filmcontrast", 1.0),
            brightness: f("r_filmbrightness", 0.0),
            desaturation: f("r_filmdesaturation", 0.0),
            invert: f("r_filminvert", 0.0) != 0.0,
            light_tint: v3("r_filmlighttint"),
            dark_tint: v3("r_filmdarktint"),
        },
        glow: f("r_glow", 0.0) != 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NACHT_ART: &str = r#"
	setdvar("scr_fog_exp_halfplane", "835");
	level thread fog_settings();
	level thread maps\_utility::set_all_players_visionset( "zombie", 0.1 );
}
fog_settings()
{
	start_dist 			= 165;
	halfway_dist 		= 835;
	halfway_height 		= 200;
	base_height 		= 75;
	red 				= 0.5;
	green 				= 0.5;
	blue		 		= 0.5;
	trans_time			= 0;
	if( IsSplitScreen() )
	{
		start_dist 			= 112;
		halfway_height 		= 100;
		maps\_utility::set_splitscreen_fog( start_dist, halfway_dist, halfway_height, base_height, red, green, blue, trans_time, cull_dist );
	}
	else
	{
		SetVolFog( start_dist, halfway_dist, halfway_height, base_height, red, green, blue, trans_time );
	}
}"#;

    #[test]
    fn reads_fog_and_vision_from_an_art_script() {
        let fog = fog_from_script(NACHT_ART).unwrap();
        assert_eq!(fog, Fog { start: 165.0, halfway: 835.0, half_height: 200.0, base_height: 75.0, color: [0.5; 3] });
        assert_eq!(vision_name_from_script(NACHT_ART).as_deref(), Some("zombie"));
        assert_eq!(fog_from_script("// SetVolFog(1, 2, 3, 4, 5, 6, 7, 0);\nSetExpFog(100, 900, 0.2, 0.3, 0.4, 0);").unwrap().halfway, 900.0);
    }

    #[test]
    fn parses_vision_files() {
        let v = parse_vision("r_glow \"0\"\nr_filmEnable \"1\"\nr_filmContrast \"1\"\nr_filmBrightness \"0.0055\"\nr_filmDesaturation \"0.4\"\nr_filmLightTint \"2 2 2\"\nr_filmDarkTint \"0.84 0.92 1.10\"\n");
        assert!(v.film.enabled && !v.glow);
        assert_eq!(v.film.dark_tint, [0.84, 0.92, 1.10]);
        assert_eq!(v.film.light_tint, [2.0; 3]);
        assert_eq!(v.film.desaturation, 0.4);
    }
}
