//! Character and AI-type scripts: which models make up an AI and how it
//! comes apart, as the game's own scripts define it.
//!
//! * `aitype/<type>.gsc` picks one of its characters at random
//!   (`character\<name>::main()`).
//! * `character/<name>.gsc` sets the body (`setModelFromArray`), the head
//!   (`self.headModel`), and the gib models: `torsoDmg1..5` (upper body:
//!   clean, right arm off, left arm off, guts, beheaded), `legDmg1..4`
//!   (lower body: clean, right leg off, left leg off, both off) and
//!   `gibSpawn1..5` with `gibSpawnTag1..5` (the severed part and the joint
//!   it flies from). Values are model names or random picks from
//!   `xmodelalias/<name>.gsc` lists.

/// One character's models.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CharacterDef {
    pub name: String,
    pub bodies: Vec<String>,
    pub heads: Vec<String>,
    /// `torsoDmg1..5`: each a list to pick from (empty if unset).
    pub torso_dmg: [Vec<String>; 5],
    /// `legDmg1..4`.
    pub leg_dmg: [Vec<String>; 4],
    /// `gibSpawn1..5`: the severed part and the joint it leaves from.
    pub gib_spawn: [Option<(Vec<String>, String)>; 5],
}

/// The script path of an AI type for a spawner classname
/// (`actor_axis_zombie_ger_ber_sshonor` -> `aitype/axis_zombie_ger_ber_sshonor.gsc`).
pub fn aitype_script(classname: &str) -> Option<String> {
    classname.strip_prefix("actor_").map(|t| format!("aitype/{t}.gsc"))
}

/// The character scripts an AI type picks from.
pub fn aitype_characters(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in code_lines(text) {
        if let Some(i) = line.find("character\\") {
            let rest = &line[i + "character\\".len()..];
            if let Some(end) = rest.find("::main") {
                let path = format!("character/{}.gsc", &rest[..end]);
                if !out.contains(&path) {
                    out.push(path);
                }
            }
        }
    }
    out
}

/// The models of an `xmodelalias/<name>.gsc` list (`a[i] = "model";`).
pub fn alias_models(text: &str) -> Vec<String> {
    code_lines(text).filter(|l| l.trim_start().starts_with("a[")).filter_map(quoted).collect()
}

/// Reads a character script; `alias` returns the text of
/// `xmodelalias/<name>.gsc` given `<name>`.
pub fn parse_character(name: &str, text: &str, alias: impl Fn(&str) -> Option<String>) -> CharacterDef {
    let mut c = CharacterDef { name: name.to_string(), ..Default::default() };
    // Only `main()`: `precache()` lists every model again.
    let main = text.split("precache()").next().unwrap_or(text);
    let value = |rhs: &str| -> Vec<String> {
        if let Some(i) = rhs.find("xmodelalias\\") {
            let rest = &rhs[i + "xmodelalias\\".len()..];
            let n = rest.split("::").next().unwrap_or("");
            alias(n).map(|t| alias_models(&t)).unwrap_or_default()
        } else {
            quoted(rhs).into_iter().collect()
        }
    };
    let mut tags: [Option<String>; 5] = Default::default();
    let mut spawns: [Vec<String>; 5] = Default::default();
    for line in code_lines(main) {
        let l = line.trim();
        if l.contains("setModelFromArray") {
            c.bodies = value(l);
            continue;
        }
        let Some((lhs, rhs)) = l.split_once('=') else { continue };
        let key = lhs.trim().trim_start_matches("self.").to_ascii_lowercase();
        let num = |prefix: &str| key.strip_prefix(prefix).and_then(|n| n.parse::<usize>().ok()).filter(|n| (1..=5).contains(n)).map(|n| n - 1);
        if key == "headmodel" {
            c.heads = value(rhs);
        } else if let Some(i) = num("torsodmg") {
            c.torso_dmg[i] = value(rhs);
        } else if let Some(i) = num("legdmg").filter(|&i| i < 4) {
            c.leg_dmg[i] = value(rhs);
        } else if let Some(i) = num("gibspawntag") {
            tags[i] = quoted(rhs);
        } else if let Some(i) = num("gibspawn") {
            spawns[i] = value(rhs);
        }
    }
    for i in 0..5 {
        if let (false, Some(tag)) = (spawns[i].is_empty(), tags[i].take()) {
            c.gib_spawn[i] = Some((std::mem::take(&mut spawns[i]), tag.to_ascii_lowercase()));
        }
    }
    c
}

/// Lines without `//` comments.
fn code_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines().map(|l| l.split("//").next().unwrap_or(""))
}

/// The first double-quoted string in `s`.
fn quoted(s: &str) -> Option<String> {
    let a = s.find('"')?;
    let b = s[a + 1..].find('"')?;
    Some(s[a + 1..a + 1 + b].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHAR: &str = r#"
main()
{
	codescripts\character::setModelFromArray(xmodelalias\bodies::main());
	self.headModel = codescripts\character::randomElement(xmodelalias\heads::main());
	self attach(self.headModel, "", true);
	self.torsoDmg1 = "body_g_upclean";
	self.torsoDmg2 = codescripts\character::randomElement(xmodelalias\rarmoff::main());
	self.legDmg1 = "body_g_lowclean";
	self.gibSpawn1 = "body_g_rarmspawn";
	self.gibSpawnTag1 = "J_Elbow_RI";
}

precache()
{
	precacheModel("ignored");
}
"#;

    #[test]
    fn reads_a_character_and_its_aliases() {
        let alias = |n: &str| match n {
            "bodies" => Some("a[0] = \"body1\";\na[1] = \"body2\";".to_string()),
            "heads" => Some("a[0] = \"head1\";".to_string()),
            "rarmoff" => Some("a[0] = \"rarmoff_1\";\na[1] = \"rarmoff_2\";".to_string()),
            _ => None,
        };
        let c = parse_character("c", CHAR, alias);
        assert_eq!(c.bodies, vec!["body1", "body2"]);
        assert_eq!(c.heads, vec!["head1"]);
        assert_eq!(c.torso_dmg[0], vec!["body_g_upclean"]);
        assert_eq!(c.torso_dmg[1], vec!["rarmoff_1", "rarmoff_2"]);
        assert_eq!(c.leg_dmg[0], vec!["body_g_lowclean"]);
        assert_eq!(c.gib_spawn[0], Some((vec!["body_g_rarmspawn".to_string()], "j_elbow_ri".to_string())));
        assert!(c.gib_spawn[1].is_none());
        assert_eq!(aitype_characters("case 0:\n\tcharacter\\char_a::main();\ncase 1:\n\tcharacter\\char_b::main();\n\tcharacter\\char_a::precache();"), vec!["character/char_a.gsc", "character/char_b.gsc"]);
        assert_eq!(aitype_script("actor_axis_zombie_x").as_deref(), Some("aitype/axis_zombie_x.gsc"));
    }
}
