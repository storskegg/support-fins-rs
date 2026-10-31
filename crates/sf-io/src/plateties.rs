//! The plate-tie settings as JSON (issue #50, Q9): what the app saves with the session, what a printer
//! profile's `plateTies` overrides, and the document a tied plate's 3MF carries.
//!
//! Settings: `{ "contact": "edge", "maxGap": 8, "minRatio": 2, ..., "tunables": { "depth": 2, ... } }`,
//! the keys of `sf_engine::plateties::NUMBERS` (the shown ones at the top level, the hidden ones under
//! `tunables`). Reading is lenient about what is missing (it keeps the base: the profile's default)
//! and strict about everything else: an unknown key, a wrong type, a fraction of a tie or a number out
//! of range is an error naming the key.

use crate::json::{Json, parse};
use crate::limits::LoadOptions;
use crate::zip::unzip_with;
use sf_engine::plateties::{ContactMode, NUMBERS, Overrides, PlateTies};

/// Where a tied plate's 3MF keeps its plate-tie document (beside the session, `SESSION_PART`).
pub const PLATE_TIES_PART: &str = "Metadata/support_fins_plate_ties.json";

/// The settings as JSON.
pub fn settings_to_json(s: &PlateTies) -> Json {
    let mut top = vec![(
        "contact".to_string(),
        Json::Str(s.contact.as_str().to_string()),
    )];
    let mut hidden = Vec::new();
    for f in NUMBERS {
        let v = Json::Num((f.get)(s));
        if f.hidden {
            hidden.push((f.key.to_string(), v));
        } else {
            top.push((f.key.to_string(), v));
        }
    }
    top.push(("tunables".to_string(), Json::Obj(hidden)));
    Json::Obj(top)
}

/// Settings from JSON, starting from `base` (a key that is missing keeps `base`'s value). The result is
/// validated (`PlateTies::validate`).
pub fn settings_from_json(j: &Json, base: &PlateTies) -> Result<PlateTies, String> {
    let Json::Obj(kv) = j else {
        return Err("plate ties: the settings must be a JSON object".into());
    };
    let mut s = base.clone();
    let set = |s: &mut PlateTies, key: &str, v: &Json, hidden: bool| -> Result<(), String> {
        let Some(f) = NUMBERS.iter().find(|f| f.key == key && f.hidden == hidden) else {
            return Err(format!(
                "plate ties: unknown setting {}{key}",
                if hidden { "tunables." } else { "" }
            ));
        };
        let Json::Num(n) = v else {
            return Err(format!("plate ties: {key} must be a number"));
        };
        if key == "tiesPerPair" && n.fract() != 0.0 {
            return Err(format!(
                "plate ties: tiesPerPair must be a whole number, got {n}"
            ));
        }
        if !n.is_finite() || *n < f.min || *n > f.max {
            return Err(format!(
                "plate ties: {key} must be {}..{}, got {n}",
                f.min, f.max
            ));
        }
        (f.set)(s, *n);
        Ok(())
    };
    for (k, v) in kv {
        match (k.as_str(), v) {
            ("contact", Json::Str(m)) => {
                s.contact = ContactMode::parse(m).ok_or_else(|| {
                    format!("plate ties: contact must be edge, face or anywhere, got {m:?}")
                })?;
            }
            ("contact", _) => return Err("plate ties: contact must be text".into()),
            ("tunables", Json::Obj(t)) => {
                for (k, v) in t {
                    set(&mut s, k, v, true)?;
                }
            }
            ("tunables", _) => return Err("plate ties: tunables must be an object".into()),
            (k, v) => set(&mut s, k, v, false)?,
        }
    }
    s.validate()?;
    Ok(s)
}

/// Settings from JSON text (`""` is `{}`: the base as it is).
pub fn settings_from_text(text: &str, base: &PlateTies) -> Result<PlateTies, String> {
    if text.trim().is_empty() {
        base.validate()?;
        return Ok(base.clone());
    }
    let j = parse(text).ok_or("plate ties: the settings are not JSON")?;
    settings_from_json(&j, base)
}

/// What a tied plate saves: the printer profile, the settings and the user's pair overrides.
#[derive(Clone, Debug, PartialEq)]
pub struct PlateTiesDoc {
    pub profile: String,
    pub settings: PlateTies,
    pub overrides: Overrides,
}

fn pairs_json(p: &[(String, String)]) -> Json {
    Json::Arr(
        p.iter()
            .map(|(a, b)| Json::Arr(vec![Json::Str(a.clone()), Json::Str(b.clone())]))
            .collect(),
    )
}

fn pairs_from(j: Option<&Json>, key: &str) -> Result<Vec<(String, String)>, String> {
    match j {
        None => Ok(Vec::new()),
        Some(Json::Arr(a)) => a
            .iter()
            .map(|p| match p {
                Json::Arr(ab) => match &ab[..] {
                    [Json::Str(a), Json::Str(b)] => Ok((a.clone(), b.clone())),
                    _ => Err(format!("plate ties: {key} holds pairs of two model keys")),
                },
                _ => Err(format!("plate ties: {key} holds pairs of two model keys")),
            })
            .collect(),
        Some(_) => Err(format!("plate ties: {key} must be a list")),
    }
}

impl PlateTiesDoc {
    /// `{ "version": 1, "profile": id, "settings": {...}, "disabled": [[a, b]], "added": [[a, b]] }`
    pub fn to_json(&self) -> Json {
        Json::Obj(vec![
            ("version".into(), Json::Num(1.0)),
            ("profile".into(), Json::Str(self.profile.clone())),
            ("settings".into(), settings_to_json(&self.settings)),
            ("disabled".into(), pairs_json(&self.overrides.disabled)),
            ("added".into(), pairs_json(&self.overrides.added)),
        ])
    }

    /// Read one back. Settings the document lacks come from its profile's defaults (or the default
    /// profile's, when its printer is not one this build knows).
    pub fn from_json(j: &Json) -> Result<PlateTiesDoc, String> {
        if !matches!(j, Json::Obj(_)) {
            return Err("plate ties: the document must be a JSON object".into());
        }
        match j.get("version") {
            Some(Json::Num(v)) if *v == 1.0 => {}
            _ => return Err("plate ties: an unknown document version".into()),
        }
        let profile = match j.get("profile") {
            Some(Json::Str(p)) => p.clone(),
            _ => return Err("plate ties: the document names no printer profile".into()),
        };
        let base = crate::printer::profile(&profile)
            .unwrap_or_else(crate::printer::default_profile)
            .plate_ties
            .clone();
        let settings = match j.get("settings") {
            Some(s) => settings_from_json(s, &base)?,
            None => base,
        };
        Ok(PlateTiesDoc {
            profile,
            settings,
            overrides: Overrides {
                disabled: pairs_from(j.get("disabled"), "disabled")?,
                added: pairs_from(j.get("added"), "added")?,
            },
        })
    }
}

/// The plate-tie document of a 3MF, `None` when it has none.
pub fn read_plate_ties(bytes: &[u8], opts: &LoadOptions) -> Result<Option<PlateTiesDoc>, String> {
    let parts = unzip_with(bytes, opts).map_err(String::from)?;
    let Some(data) = parts.get(PLATE_TIES_PART) else {
        return Ok(None);
    };
    let text = std::str::from_utf8(data).map_err(|_| "plate ties: the document is not text")?;
    let j = parse(text).ok_or("plate ties: the document is not JSON")?;
    PlateTiesDoc::from_json(&j).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_keep_the_base() {
        let mut s = PlateTies {
            max_gap: 6.5,
            contact: ContactMode::Anywhere,
            ties_per_pair: 2,
            ..PlateTies::default()
        };
        s.tunables.arch = 30.0;
        let j = settings_to_json(&s);
        assert_eq!(settings_from_json(&j, &PlateTies::default()).unwrap(), s);
        // missing keys keep the base
        let base = s.clone();
        let t = settings_from_text(r#"{ "maxGap": 5 }"#, &base).unwrap();
        assert_eq!(t.max_gap, 5.0);
        assert_eq!(t.tunables.arch, 30.0);
        assert_eq!(settings_from_text("", &base).unwrap(), base);
        let text = j.stringify();
        assert!(
            text.starts_with(r#"{"contact":"anywhere","maxGap":6.5,"#),
            "{text}"
        );
        assert!(text.contains(r#""tunables":{"depth":2,"#), "{text}");
    }

    #[test]
    fn bad_settings_name_their_key() {
        let base = PlateTies::default();
        for (text, why) in [
            (r#"{ "maxGap": 70 }"#, "maxGap must be 0.5..50, got 70"),
            (r#"{ "maxGap": "8" }"#, "maxGap must be a number"),
            (r#"{ "tiesPerPair": 2.5 }"#, "whole number"),
            (r#"{ "depth": 2 }"#, "unknown setting depth"),
            (
                r#"{ "tunables": { "maxGap": 2 } }"#,
                "unknown setting tunables.maxGap",
            ),
            (r#"{ "contact": "edges" }"#, "edge, face or anywhere"),
            (r#"{ "heightFrom": 0.9, "heightTo": 0.5 }"#, "heightFrom"),
            ("[1]", "JSON object"),
            ("{ ", "not JSON"),
        ] {
            let e = settings_from_text(text, &base).unwrap_err();
            assert!(e.contains(why), "{text}: {e}");
        }
    }

    #[test]
    fn the_document_round_trips() {
        let doc = PlateTiesDoc {
            profile: "elegoo-saturn-4-ultra-16k".into(),
            settings: crate::printer::default_profile().plate_ties.clone(),
            overrides: Overrides {
                disabled: vec![("a".into(), "b".into())],
                added: vec![("c".into(), "d".into())],
            },
        };
        let j = doc.to_json();
        assert_eq!(PlateTiesDoc::from_json(&j).unwrap(), doc);
        let text = j.stringify();
        assert!(
            text.contains(r#""disabled":[["a","b"]],"added":[["c","d"]]"#),
            "{text}"
        );
        assert!(PlateTiesDoc::from_json(&parse(r#"{"version":2}"#).unwrap()).is_err());
    }
}
