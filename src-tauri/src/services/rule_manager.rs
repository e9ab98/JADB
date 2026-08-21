use crate::services::apk_analyzer::ApkInfo;
use glob::Pattern;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RuleSet {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub rules: Vec<Rule>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Rule {
    pub id: String,
    #[serde(default)]
    pub description: Option<String>,
    pub severity: String,
    pub kind: String,
    #[serde(rename = "match")]
    pub match_: Value,
    /// Optional metadata bag — e.g. LibChecker's `label`, `dev_team`,
    /// `source_link`, `zh_description`. The rule engine does not read this;
    /// it's forwarded to the UI for richer reporting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

/// A single component entry (one APK-side artifact — a `.so` basename,
/// an Activity/Service/Receiver/Provider class FQN) paired with the
/// libchecker rule that recognised it. `matched_rule` is `None` when
/// the component exists in the APK but no installed rule pack covers
/// it; the UI surfaces those as plain rows.
#[derive(Serialize, Clone, Debug)]
pub struct ComponentHit {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_rule: Option<MatchedRule>,
}

/// LibChecker metadata for a single component match. Mirrors the
/// `RuleResultMetadata` shape the previous rule report emitted but
/// without the `matched` / `evidence` booleans — those are implicit
/// here (a row is "matched" iff `matched_rule.is_some()`).
#[derive(Serialize, Clone, Debug)]
pub struct MatchedRule {
    pub rule_set_id: String,
    pub rule_id: String,
    pub severity: String,
    pub description: String,
    /// Same forward-from-`Rule.metadata` semantics as the old
    /// `RuleResult.metadata` (LibChecker label / dev_team /
    /// source_link / zh_description). Optional because the bundled
    /// starter rules do not carry metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// APK-side component breakdown. The UI renders one section per
/// category so users read "this APK has these native libs / activities
/// / services / receivers / providers" rather than a flat list of
/// rule ids.
#[derive(Serialize, Clone, Debug, Default)]
pub struct ComponentMatches {
    pub native_libraries: Vec<ComponentHit>,
    pub activities: Vec<ComponentHit>,
    pub services: Vec<ComponentHit>,
    pub receivers: Vec<ComponentHit>,
    pub providers: Vec<ComponentHit>,
}

/// Per-APK rule report. `total_matched` is the count of components
/// across all 5 categories that have a `matched_rule` — the UI uses
/// it for the dashboard hit-count badge.
#[derive(Serialize, Clone, Debug)]
pub struct RuleReport {
    pub apk_path: String,
    pub components: ComponentMatches,
    pub total_matched: u32,
}

/// Load every `*.json` rule set from `rules_path`. Invalid files are skipped silently.
/// Returns an empty Vec if the directory does not exist.
pub fn load_all(rules_path: &Path) -> Result<Vec<RuleSet>, String> {
    if !rules_path.exists() {
        return Ok(vec![]);
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(rules_path).map_err(|e| e.to_string())? {
        let entry = match entry { Ok(e) => e, Err(_) => continue };
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let bytes = match std::fs::read(&path) { Ok(b) => b, Err(_) => continue };
        match serde_json::from_slice::<RuleSet>(&bytes) {
            Ok(set) => out.push(set),
            Err(_) => continue,
        }
    }
    Ok(out)
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum ComponentKind {
    Native,
    Activity,
    Service,
    Receiver,
    Provider,
}

impl ComponentKind {
    fn matches_str(self, s: &str) -> bool {
        matches!(
            (self, s),
            (ComponentKind::Activity, "activity")
                | (ComponentKind::Service, "service")
                | (ComponentKind::Receiver, "receiver")
                | (ComponentKind::Provider, "provider"),
        )
    }
}

/// For every APK-side component (each `.so` basename, every
/// Activity/Service/Receiver/Provider FQN), find the first
/// `native_library` / `component_class` rule that matches it. The
/// first matching rule wins; later rule packs in the same selection
/// are skipped. This matches the libchecker-Rules convention where a
/// single class has a single canonical owner.
pub fn evaluate_components(rule_sets: &[&RuleSet], info: &ApkInfo) -> ComponentMatches {
    fn match_one(rule_sets: &[&RuleSet], name: &str, kind: ComponentKind) -> Option<MatchedRule> {
        for rs in rule_sets {
            for r in &rs.rules {
                let hit = match (r.kind.as_str(), kind) {
                    ("native_library", ComponentKind::Native) => {
                        r.match_.get("file").and_then(|v| v.as_str()) == Some(name)
                    }
                    ("component_class", k) if k != ComponentKind::Native => {
                        let cls = r.match_.get("class").and_then(|v| v.as_str()).unwrap_or("");
                        let ty = r.match_.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        k.matches_str(ty) && cls == name
                    }
                    _ => false,
                };
                if hit {
                    return Some(MatchedRule {
                        rule_set_id: rs.id.clone(),
                        rule_id: r.id.clone(),
                        severity: r.severity.clone(),
                        description: r
                            .description
                            .clone()
                            .unwrap_or_else(|| r.id.clone()),
                        metadata: r.metadata.clone(),
                    });
                }
            }
        }
        None
    }

    let mut out = ComponentMatches::default();
    for name in &info.native_libs {
        out.native_libraries.push(ComponentHit {
            name: name.clone(),
            matched_rule: match_one(rule_sets, name, ComponentKind::Native),
        });
    }
    for name in &info.activities {
        out.activities.push(ComponentHit {
            name: name.clone(),
            matched_rule: match_one(rule_sets, name, ComponentKind::Activity),
        });
    }
    for name in &info.services {
        out.services.push(ComponentHit {
            name: name.clone(),
            matched_rule: match_one(rule_sets, name, ComponentKind::Service),
        });
    }
    for name in &info.receivers {
        out.receivers.push(ComponentHit {
            name: name.clone(),
            matched_rule: match_one(rule_sets, name, ComponentKind::Receiver),
        });
    }
    for name in &info.providers {
        out.providers.push(ComponentHit {
            name: name.clone(),
            matched_rule: match_one(rule_sets, name, ComponentKind::Provider),
        });
    }
    out
}

fn eval_permission(rule: &Rule, info: &ApkInfo) -> (bool, Option<String>) {
    let pattern = rule
        .match_
        .get("pattern")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let pat = match Pattern::new(pattern) {
        Ok(p) => p,
        Err(_) => return (false, None),
    };
    for p in &info.permissions {
        if pat.matches(p) {
            return (true, Some(p.clone()));
        }
    }
    (false, None)
}

fn eval_component(rule: &Rule, info: &ApkInfo) -> (bool, Option<String>) {
    let kind = rule
        .match_
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let count = match kind {
        "activity" => info.activities.len(),
        "service" => info.services.len(),
        "receiver" => info.receivers.len(),
        "provider" => info.providers.len(),
        _ => return (false, None),
    };
    let threshold = rule
        .match_
        .get("min_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if threshold > 0 && (count as u64) >= threshold {
        (true, Some(format!("{kind}={count}")))
    } else {
        (false, None)
    }
}

fn eval_sdk(rule: &Rule, info: &ApkInfo) -> (bool, Option<String>) {
    let field = rule
        .match_
        .get("field")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let value = match field {
        "min_sdk" => info.min_sdk.clone(),
        "target_sdk" => info.target_sdk.clone(),
        "max_sdk" => info.max_sdk.clone(),
        _ => None,
    };
    let n: i64 = match value.as_deref().and_then(|s| s.parse().ok()) {
        Some(n) => n,
        None => return (false, value),
    };
    let lt = numeric_from(rule.match_.get("lt"));
    let gt = numeric_from(rule.match_.get("gt"));
    let matched = match (lt, gt) {
        (Some(l), _) if n < l => true,
        (_, Some(g)) if n > g => true,
        _ => false,
    };
    (matched, value)
}

fn eval_manifest_stub(rule: &Rule, info: &ApkInfo) -> (bool, Option<String>) {
    let needle = rule
        .match_
        .get("contains")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if needle.is_empty() {
        return (false, None);
    }
    if info.raw_badging.contains(needle) {
        (true, Some(needle.to_string()))
    } else {
        (false, None)
    }
}

fn numeric_from(v: Option<&Value>) -> Option<i64> {
    v.and_then(|x| x.as_i64().or_else(|| x.as_str().and_then(|s| s.parse().ok())))
}


fn eval_native_library(rule: &Rule, info: &ApkInfo) -> (bool, Option<String>) {
    let needle = rule
        .match_
        .get("file")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if needle.is_empty() {
        return (false, None);
    }
    for lib in &info.native_libs {
        if lib == needle {
            return (true, Some(needle.to_string()));
        }
    }
    (false, None)
}

fn eval_component_class(rule: &Rule, info: &ApkInfo) -> (bool, Option<String>) {
    let kind = rule
        .match_
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let class = rule
        .match_
        .get("class")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if class.is_empty() {
        return (false, None);
    }
    let list: &[String] = match kind {
        "activity" => &info.activities,
        "service" => &info.services,
        "receiver" => &info.receivers,
        "provider" => &info.providers,
        _ => return (false, None),
    };
    if list.iter().any(|c| c == class) {
        (true, Some(class.to_string()))
    } else {
        (false, None)
    }
}

fn eval_action(rule: &Rule, info: &ApkInfo) -> (bool, Option<String>) {
    let needle = rule
        .match_
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if needle.is_empty() {
        return (false, None);
    }
    for a in &info.intent_actions {
        if a == needle {
            return (true, Some(needle.to_string()));
        }
    }
    (false, None)
}

/// `<uses-feature>` matcher. Supports the same `name` field as
/// `eval_action` — `match.name` is matched against every entry in
/// `info.uses_feature`. Accepts glob-style wildcards (`*`) so a
/// rule can pin all camera family features with
/// `match.name = "android.hardware.camera*"`.
fn eval_uses_feature(rule: &Rule, info: &ApkInfo) -> (bool, Option<String>) {
    let needle = rule
        .match_
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if needle.is_empty() {
        return (false, None);
    }
    let pat = match Pattern::new(needle) {
        Ok(p) => p,
        Err(_) => return (false, None),
    };
    for f in &info.uses_feature {
        if pat.matches(f) {
            return (true, Some(f.clone()));
        }
    }
    (false, None)
}

/// `<uses-library>` matcher. Same shape as `eval_uses_feature`
/// but walks `info.uses_library`. Useful for catching SDK
/// integrations that ship as a shared library (e.g. Google Maps
/// Android, ML Kit) without having to enumerate every class.
fn eval_uses_library(rule: &Rule, info: &ApkInfo) -> (bool, Option<String>) {
    let needle = rule
        .match_
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if needle.is_empty() {
        return (false, None);
    }
    let pat = match Pattern::new(needle) {
        Ok(p) => p,
        Err(_) => return (false, None),
    };
    for lib in &info.uses_library {
        if pat.matches(lib) {
            return (true, Some(lib.clone()));
        }
    }
    (false, None)
}

/// Result of evaluating a single `Rule` against an `ApkInfo`. Mirrors what
/// the legacy rule report UI expected per-rule (matched + evidence); the
/// newer `RuleReport` / `evaluate_components` API is per-component.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleResult {
    pub matched: bool,
    pub rule_id: String,
    pub rule_set_id: String,
    pub severity: String,
    pub evidence: Option<String>,
    /// Human-readable label: `rule.description` if present, else `rule.id`.
    pub message: String,
}

/// Per-rule evaluator dispatcher.
///
/// For each `Rule` in `rules`, dispatch to the matching `eval_*` by
/// `rule.kind`. Unknown kinds fall through to `matched=false, evidence=None`
/// rather than panicking — rule packs can carry experimental `kind`s that
/// older engines just ignore.
///
/// `rule_set_id` is stamped onto every produced `RuleResult` so the UI can
/// attribute a hit to the specific rule pack that emitted it.
pub fn evaluate(rules: &[Rule], rule_set_id: &str, info: &ApkInfo) -> Vec<RuleResult> {
    rules
        .iter()
        .map(|r| {
            let (matched, evidence) = match r.kind.as_str() {
                "permission" => eval_permission(r, info),
                "component" => eval_component(r, info),
                "sdk" => eval_sdk(r, info),
                "manifest" | "manifest_stub" => eval_manifest_stub(r, info),
                "native_library" => eval_native_library(r, info),
                "component_class" => eval_component_class(r, info),
                "action" => eval_action(r, info),
                "uses_feature" => eval_uses_feature(r, info),
                "uses_library" => eval_uses_library(r, info),
                _ => (false, None),
            };
            RuleResult {
                matched,
                rule_id: r.id.clone(),
                rule_set_id: rule_set_id.to_string(),
                severity: r.severity.clone(),
                evidence,
                message: r.description.clone().unwrap_or_else(|| r.id.clone()),
            }
        })
        .collect()
}
