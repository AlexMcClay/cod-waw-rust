//! Parser for the plain-text weapon definition format used by the IW3-era
//! engines: `WEAPONFILE\key\value\key\value...` (backslash separated).
//!
//! Only the parser lives here; no game data is embedded. The game looks for
//! these files in the user's own extracted install at runtime.

use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct WeaponFile {
    pub fields: HashMap<String, String>,
}

impl WeaponFile {
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split('\\');
        let magic = parts.next()?.trim();
        if !magic.eq_ignore_ascii_case("WEAPONFILE") {
            return None;
        }
        let rest: Vec<&str> = parts.collect();
        let fields = rest
            .chunks(2)
            .filter(|kv| kv.len() == 2 && !kv[0].is_empty())
            .map(|kv| (kv[0].to_string(), kv[1].trim_end_matches(['\r', '\n', '\0']).to_string()))
            .collect();
        Some(Self { fields })
    }

    pub fn parse_bytes(bytes: &[u8]) -> Option<Self> {
        // Files are 8-bit; decode as latin-1 so nothing is ever rejected.
        let text: String = bytes.iter().map(|&b| b as char).collect();
        Self::parse(&text)
    }

    /// From `(key, value)` pairs, such as the stats of a zone WeaponDef.
    pub fn from_pairs<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        Self { fields: pairs.into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect() }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields.get(key).map(String::as_str).filter(|s| !s.is_empty())
    }

    pub fn f32(&self, key: &str) -> Option<f32> {
        self.get(key)?.trim().parse().ok()
    }

    pub fn u32(&self, key: &str) -> Option<u32> {
        self.f32(key).filter(|v| *v >= 0.0).map(|v| v as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pairs() {
        let wf = WeaponFile::parse("WEAPONFILE\\damage\\40\\fireType\\Full Auto\\clipSize\\30\\empty\\").unwrap();
        assert_eq!(wf.f32("damage"), Some(40.0));
        assert_eq!(wf.get("fireType"), Some("Full Auto"));
        assert_eq!(wf.u32("clipSize"), Some(30));
        assert_eq!(wf.get("empty"), None);
        assert_eq!(wf.get("missing"), None);
    }

    #[test]
    fn from_pairs() {
        let wf = WeaponFile::from_pairs([("damage", "100"), ("fireType", "Single Shot")]);
        assert_eq!(wf.f32("damage"), Some(100.0));
        assert_eq!(wf.get("fireType"), Some("Single Shot"));
    }

    #[test]
    fn rejects_other_files() {
        assert!(WeaponFile::parse("hello\\world").is_none());
    }
}
