//! Map entities: the plain-text `{ "key" "value" ... }` block a compiled map
//! carries (spawners, triggers, path nodes, script structs...).

use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct Entity {
    pub kv: HashMap<String, String>,
}

impl Entity {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.kv.get(key).map(String::as_str)
    }
    pub fn classname(&self) -> &str {
        self.get("classname").unwrap_or("")
    }
    pub fn targetname(&self) -> &str {
        self.get("targetname").unwrap_or("")
    }
    pub fn target(&self) -> &str {
        self.get("target").unwrap_or("")
    }
    pub fn vec3(&self, key: &str) -> Option<[f32; 3]> {
        let mut it = self.get(key)?.split_whitespace().map(|s| s.parse::<f32>().ok());
        Some([it.next()??, it.next()??, it.next()??])
    }
    /// Game-unit origin (Z up), zero if missing.
    pub fn origin(&self) -> [f32; 3] {
        self.vec3("origin").unwrap_or([0.0; 3])
    }
    /// Pitch, yaw, roll in degrees.
    pub fn angles(&self) -> [f32; 3] {
        self.vec3("angles").unwrap_or([0.0; 3])
    }
    pub fn f32(&self, key: &str) -> Option<f32> {
        self.get(key)?.trim().parse().ok()
    }
    /// Brush submodel index for `"model" "*N"`.
    pub fn submodel(&self) -> Option<usize> {
        self.get("model")?.strip_prefix('*')?.parse().ok()
    }
}

/// Parses the entity text.
pub fn parse(text: &str) -> Vec<Entity> {
    let mut out = Vec::new();
    let mut cur: Option<Entity> = None;
    for line in text.lines() {
        let line = line.trim();
        if line == "{" {
            cur = Some(Entity::default());
        } else if line == "}" {
            if let Some(e) = cur.take() {
                out.push(e);
            }
        } else if let Some(e) = cur.as_mut() {
            let parts: Vec<&str> = line.split('"').collect();
            // `"key" "value"` splits into ["", key, " ", value, ""].
            if parts.len() >= 5 {
                e.kv.insert(parts[1].to_string(), parts[3].to_string());
            }
        }
    }
    out
}

/// Finds the entity text inside a zone: the null-terminated block that
/// contains the worldspawn entity.
pub fn find_in_zone(zone: &[u8]) -> Option<&str> {
    let anchor = crate::zone::find_all(zone, b"\"classname\" \"worldspawn\"").into_iter().next()?;
    let start = zone[..anchor].iter().rposition(|&b| b == 0).map(|p| p + 1).unwrap_or(0);
    let end = anchor + zone[anchor..].iter().position(|&b| b == 0)?;
    std::str::from_utf8(&zone[start..end]).ok()
}

/// Convenience queries over a parsed entity list.
pub trait EntityList {
    fn by_class<'a>(&'a self, class: &'a str) -> Box<dyn Iterator<Item = &'a Entity> + 'a>;
    fn by_targetname<'a>(&'a self, name: &'a str) -> Box<dyn Iterator<Item = &'a Entity> + 'a>;
}

impl EntityList for [Entity] {
    fn by_class<'a>(&'a self, class: &'a str) -> Box<dyn Iterator<Item = &'a Entity> + 'a> {
        Box::new(self.iter().filter(move |e| e.classname() == class))
    }
    fn by_targetname<'a>(&'a self, name: &'a str) -> Box<dyn Iterator<Item = &'a Entity> + 'a> {
        Box::new(self.iter().filter(move |e| e.targetname() == name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "{\n\"classname\" \"worldspawn\"\n\"sunlight\" \"0.75\"\n}\n{\n\"origin\" \"-206 242 60\"\n\"zombie_cost\" \"200\"\n\"targetname\" \"weapon_upgrade\"\n\"classname\" \"trigger_use\"\n\"model\" \"*52\"\n}\n";

    #[test]
    fn parses_entities() {
        let ents = parse(TEXT);
        assert_eq!(ents.len(), 2);
        assert_eq!(ents[0].classname(), "worldspawn");
        let wb = ents.by_targetname("weapon_upgrade").next().unwrap();
        assert_eq!(wb.origin(), [-206.0, 242.0, 60.0]);
        assert_eq!(wb.f32("zombie_cost"), Some(200.0));
        assert_eq!(wb.submodel(), Some(52));
        assert_eq!(ents.by_class("trigger_use").count(), 1);
    }

    #[test]
    fn finds_text_in_zone() {
        let mut zone = vec![1u8, 2, 0];
        zone.extend_from_slice(TEXT.as_bytes());
        zone.extend_from_slice(&[0, 9, 9]);
        assert_eq!(find_in_zone(&zone), Some(TEXT));
    }
}
