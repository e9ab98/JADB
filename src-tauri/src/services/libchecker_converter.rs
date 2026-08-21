//! Converter: LibChecker-Rules raw format → JADB RuleSet format.
//!
//! LibChecker stores one rule per file (e.g. `native-libs/libfoo.so.json`) with
//! a `{ data: [...locales], uuid }` body. JADB stores one RuleSet per file with
//! `{ id, name, rules: [...] }`. We group by LibChecker's top-level directory
//! (one RuleSet per directory) and translate each file into a single Rule
//! carrying `label`, `dev_team`, `source_link`, and the zh-Hans description in
//! `Rule.metadata` (the rule engine does not read it).
//!
//! v1 covers 7 categories:
//!   - native-libs        → kind=native_library
//!   - activities-libs    → kind=component_class, type=activity
//!   - services-libs      → kind=component_class, type=service
//!   - receivers-libs     → kind=component_class, type=receiver
//!   - providers-libs     → kind=component_class, type=provider
//!   - actions-libs       → kind=action
//!
//! `static-libs` is intentionally skipped: its native `match.contains`
//! needle is meaningless against `info.raw_badging` (the badging
//! output doesn't contain package paths), so the matcher fires
//! 0% of the time and just adds noise to the rule list. Proper
//! static-library detection needs `<uses-library>` /
//! `<uses-static-library>` parsing from the aapt2 xmltree output;
//! re-introduce the category once the analyzer exposes those
//! fields on `ApkInfo`.

use crate::services::rule_manager::{Rule, RuleSet};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const PRIMARY_LOCALES: &[&str] = &["en", "en-US", "zh-Hans", "zh-CN"];

/// Walk a LibChecker-Rules checkout rooted at `root` and produce one RuleSet
/// per known category. `commit_short` is the short git SHA of the checkout,
/// embedded in each pack description so users can see what they have installed.
///
/// Missing categories yield no RuleSet. A missing root yields no RuleSets
/// (returns Ok(vec![]) so the caller can fall back to bundled).
pub fn convert_dir(root: &Path, commit_short: &str) -> Result<Vec<RuleSet>, String> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    let version = read_cloud_version(root);

    let mut out = Vec::new();
    for (aliases, category_id, category_name, builder) in categories() {
        let dir = match pick_alias(root, &aliases) {
            Some(d) => d,
            None => continue,
        };
        let rules = builder(&dir);
        if rules.is_empty() {
            continue;
        }
        out.push(RuleSet {
            id: format!("libchecker.{category_id}"),
            name: format!("LibChecker {category_name}"),
            version: version.clone(),
            description: Some(format!(
                "Converted from LibChecker/LibChecker-Rules @ {commit_short}"
            )),
            rules,
        });
    }
    Ok(out)
}

type Builder = Box<dyn Fn(&Path) -> Vec<Rule>>;

/// Each category lists candidate top-level sub-directory names. The
/// converter picks the first one that exists in the LibChecker-Rules
/// checkout, so we keep working across upstream renames (the published
/// rules have shipped as both `native-libs/` and `native_libs/`
/// depending on the snapshot, and contributors have historically
/// shipped single-name variants). Order matters: list the
/// most-specific / newest alias first.
fn categories() -> Vec<(Vec<&'static str>, &'static str, &'static str, Builder)> {
    vec![
        (
            vec!["native-libs", "native_libs", "native_lib"],
            "native-libraries",
            "Native Libraries",
            Box::new(convert_native_libs),
        ),
        (
            vec!["activities-libs", "activities_libs", "activities_lib", "activities"],
            "activities",
            "Activities",
            Box::new(convert_components_in("activity")),
        ),
        (
            vec!["services-libs", "services_libs", "services_lib", "services"],
            "services",
            "Services",
            Box::new(convert_components_in("service")),
        ),
        (
            vec!["receivers-libs", "receivers_libs", "receivers_lib", "receivers"],
            "receivers",
            "Receivers",
            Box::new(convert_components_in("receiver")),
        ),
        (
            vec!["providers-libs", "providers_libs", "providers_lib", "providers"],
            "providers",
            "Providers",
            Box::new(convert_components_in("provider")),
        ),
        (
            vec!["actions-libs", "actions_libs", "actions_lib", "actions"],
            "intent-actions",
            "Intent Actions",
            Box::new(convert_actions),
        ),
    ]
}

/// Walk `aliases` in order and return the first path that resolves
/// to an existing directory under `root`. Returns `None` when no
/// alias matches, so the caller can simply `continue` past the
/// category.
fn pick_alias(root: &Path, aliases: &[&'static str]) -> Option<PathBuf> {
    for a in aliases {
        let d = root.join(a);
        if d.is_dir() {
            return Some(d);
        }
    }
    None
}

fn read_cloud_version(root: &Path) -> Option<String> {
    let p = root.join("cloud/md5/v4");
    let bytes = std::fs::read(&p).ok()?;
    let v: Value = serde_json::from_slice(&bytes).ok()?;
    v.get("version")
        .and_then(|x| x.as_u64())
        .map(|n| n.to_string())
}

fn convert_native_libs(dir: &Path) -> Vec<Rule> {
    let mut entries: Vec<std::path::PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.flatten().map(|e| e.path()).collect(),
        Err(_) => return Vec::new(),
    };
    entries.sort();
    let mut out = Vec::new();
    for path in entries {
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let file_stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let rule = match lc_to_rule(&bytes, &file_stem, |label, team, desc| Rule {
            id: format!("native-{}", uuid_short_from_path(&path)),
            description: Some(format!("{label} — {team}: {desc}")),
            severity: "info".into(),
            kind: "native_library".into(),
            match_: json!({ "file": file_stem }),
            metadata: None,
        }) {
            Some(r) => r,
            None => continue,
        };
        out.push(rule);
    }
    out
}

#[allow(clippy::type_complexity)]
fn convert_components_in(component_type: &'static str) -> Box<dyn Fn(&Path) -> Vec<Rule>> {
    let ct = component_type;
    Box::new(move |dir| {
        let mut out = Vec::new();
        walk_json(dir, &mut |path, bytes| {
            let class = match class_fqn_from_json(dir, path) {
                Some(c) => c,
                None => return,
            };
            if class.is_empty() {
                return;
            }
            let Some(mut rule) = lc_to_rule(bytes, &class, |label, team, desc| Rule {
                id: format!("{ct}-{}", uuid_short_from_path(path)),
                description: Some(format!("{label} — {team}: {desc}")),
                severity: "info".into(),
                kind: "component_class".into(),
                match_: json!({ "type": ct, "class": class.clone() }),
                metadata: None,
            }) else {
                return;
            };
            if let Some(d) = rule.description.as_mut() {
                *d = format!("[{class}] {d}");
            }
            out.push(rule);
        });
        out
    })
}

fn convert_actions(dir: &Path) -> Vec<Rule> {
    let mut entries: Vec<std::path::PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.flatten().map(|e| e.path()).collect(),
        Err(_) => return Vec::new(),
    };
    entries.sort();
    let mut out = Vec::new();
    for path in entries {
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let name = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let Some(rule) = lc_to_rule(&bytes, &name, |label, team, desc| Rule {
            id: format!("action-{}", uuid_short_from_path(&path)),
            description: Some(format!("{label} — {team}: {desc}")),
            severity: "info".into(),
            kind: "action".into(),
            match_: json!({ "name": name.clone() }),
            metadata: None,
        }) else {
            continue;
        };
        out.push(rule);
    }
    out
}


// ---- shared helpers ----

fn walk_json(dir: &Path, visit: &mut dyn FnMut(&Path, &[u8])) {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(p) = stack.pop() {
        let rd = match std::fs::read_dir(&p) {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(bytes) = std::fs::read(&path) {
                    visit(&path, &bytes);
                }
            }
        }
    }
}

/// Convert a JSON path under `root/<subdir>/<a>/<b>/<Foo>.json` into a
/// Java FQN `<a>.<b>.<Foo>`. Returns None if any segment can't be decoded.
fn class_fqn_from_json(root: &Path, json_path: &Path) -> Option<String> {
    let rel = json_path.strip_prefix(root).ok()?;
    let mut parts: Vec<String> = Vec::new();
    for c in rel.components() {
        let s = c.as_os_str().to_str()?;
        if s.ends_with(".json") {
            parts.push(s.trim_end_matches(".json").to_string());
        } else {
            parts.push(s.to_string());
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(parts.join("."))
}

fn uuid_short_from_path(path: &Path) -> String {
    // Stable short id from absolute path bytes — avoids needing to parse
    // JSON just for the id. 12 hex chars ≈ 48 bits, collision-free in
    // practice for a few thousand rules.
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    format!("{:012x}", h.finish())
}

fn lc_to_rule<F>(bytes: &[u8], _key_hint: &str, mut build: F) -> Option<Rule>
where
    F: FnMut(&str, &str, &str) -> Rule,
{
    let v: Value = match serde_json::from_slice(bytes) {
        Ok(v) => v,
        Err(_) => return None,
    };

    // The upstream LibChecker-Rules checkout has shipped in two
    // schemas. v1 (historical): `{ "data": [{ locale, data }], "uuid" }`
    // — the converter unpacks it via `pick_locales`. v2 (current
    // master): flat `{ label, team, iconUrl, contributors, description,
    // relativeUrl }`. Detect by the presence of `data` and route
    // accordingly so we keep working when users reinstall against an
    // older snapshot.
    let (label, team, desc, source_link, zh_desc) =
        if let Some(arr) = v.get("data").and_then(|x| x.as_array()) {
            // v1 wrapper schema.
            let (en, zh) = pick_locales(arr);
            let en = en?;
            let label = en.get("label").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let team = en.get("dev_team").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let desc = en.get("description").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let source_link = en.get("source_link").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let zh_desc = zh
                .and_then(|z| z.get("description").and_then(|x| x.as_str()).map(str::to_string))
                .unwrap_or_default();
            (label, team, desc, source_link, zh_desc)
        } else {
            // v2 flat schema. No zh description in this format — leave
            // empty so downstream UI shows "no Chinese description
            // available" rather than fabricating one.
            let label = v.get("label").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let team = v.get("team").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let desc = v.get("description").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let source_link = v.get("relativeUrl").and_then(|x| x.as_str()).unwrap_or("").to_string();
            (label, team, desc, source_link, String::new())
        };

    let mut rule = build(&label, &team, &desc);
    rule.metadata = Some(json!({
        "label": label,
        "dev_team": team,
        "source_link": source_link,
        "zh_description": zh_desc,
    }));
    Some(rule)
}

fn pick_locales(arr: &[Value]) -> (Option<&Value>, Option<&Value>) {
    let mut primary: Option<&Value> = None;
    let mut zh: Option<&Value> = None;
    for entry in arr {
        let locale = entry.get("locale").and_then(|x| x.as_str()).unwrap_or("");
        if PRIMARY_LOCALES.contains(&locale) {
            if primary.is_none() {
                primary = entry.get("data");
            }
            if locale == "zh-Hans" || locale == "zh-CN" {
                zh = entry.get("data");
            }
        }
    }
    (primary, zh)
}
