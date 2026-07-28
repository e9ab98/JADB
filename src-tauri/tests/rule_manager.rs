use jadb_lib::services::apk_analyzer::ApkInfo;
use jadb_lib::services::rule_manager::{evaluate, load_all, Rule, RuleResult};
use serde_json::json;

fn info_with_perm(p: &str) -> ApkInfo {
    ApkInfo {
        permissions: vec![p.into()],
        ..ApkInfo::default()
    }
}

fn rule_permission(id: &str, pattern: &str, severity: &str) -> Rule {
    Rule {
        id: id.into(),
        description: None,
        severity: severity.into(),
        kind: "permission".into(),
        match_: json!({ "pattern": pattern }),
        metadata: None,
    }
}

#[test]
fn permission_glob_matches() {
    let r = rule_permission("r1", "android.permission.ACCESS_*LOCATION*", "info");
    let res = evaluate(&[r.clone()], "rs1", &info_with_perm("android.permission.ACCESS_FINE_LOCATION"));
    assert_eq!(res.len(), 1);
    assert!(res[0].matched);
    assert_eq!(res[0].rule_id, "r1");
    assert_eq!(res[0].rule_set_id, "rs1");
    assert_eq!(res[0].severity, "info");
    assert_eq!(res[0].evidence.as_deref(), Some("android.permission.ACCESS_FINE_LOCATION"));
}

#[test]
fn permission_glob_does_not_match_unrelated() {
    let r = rule_permission("r1", "android.permission.ACCESS_*LOCATION*", "warn");
    let res = evaluate(&[r], "rs1", &info_with_perm("android.permission.INTERNET"));
    assert_eq!(res.len(), 1);
    assert!(!res[0].matched);
}

#[test]
fn sdk_rule_fires_when_min_below_threshold() {
    let r = Rule {
        id: "old-sdk".into(),
        description: None,
        severity: "warn".into(),
        kind: "sdk".into(),
        match_: json!({ "field": "min_sdk", "lt": "26" }),
        metadata: None,
    };
    let mut info = ApkInfo::default();
    info.min_sdk = Some("24".into());
    let res = evaluate(&[r], "rs1", &info);
    assert_eq!(res.len(), 1);
    assert!(res[0].matched);
}

#[test]
fn sdk_rule_does_not_fire_when_min_at_or_above_threshold() {
    let r = Rule {
        id: "old-sdk".into(),
        description: None,
        severity: "warn".into(),
        kind: "sdk".into(),
        match_: json!({ "field": "min_sdk", "lt": "26" }),
        metadata: None,
    };
    let mut info = ApkInfo::default();
    info.min_sdk = Some("26".into());
    let res = evaluate(&[r], "rs1", &info);
    assert!(!res[0].matched);
}

#[test]
fn component_rule_fires_when_min_count_met() {
    let r = Rule {
        id: "many-activities".into(),
        description: None,
        severity: "info".into(),
        kind: "component".into(),
        match_: json!({ "type": "activity", "min_count": 2 }),
        metadata: None,
    };
    let mut info = ApkInfo::default();
    info.activities = vec!["a.A".into(), "b.B".into(), "c.C".into()];
    let res = evaluate(&[r], "rs1", &info);
    assert!(res[0].matched);
    assert_eq!(res[0].evidence.as_deref(), Some("activity=3"));
}

#[test]
fn manifest_stub_matches_via_contains() {
    let r = Rule {
        id: "manifest-contains".into(),
        description: None,
        severity: "info".into(),
        kind: "manifest".into(),
        match_: json!({ "contains": "android.intent.action.MAIN" }),
        metadata: None,
    };
    let mut info = ApkInfo::default();
    info.raw_badging = "launchable-activity: name='com.app.MainActivity' ... intent action android.intent.action.MAIN".into();
    let res = evaluate(&[r], "rs1", &info);
    assert!(res[0].matched);
}

#[test]
fn unknown_kind_returns_unmatched() {
    let r = Rule {
        id: "weird".into(),
        description: None,
        severity: "info".into(),
        kind: "unknown_kind".into(),
        match_: json!({}),
        metadata: None,
    };
    let res = evaluate(&[r], "rs1", &ApkInfo::default());
    assert_eq!(res.len(), 1);
    assert!(!res[0].matched);
}

#[test]
fn rule_set_loads_from_directory() {
    let tmp = tempdir();
    std::fs::write(
        tmp.join("libchecker.json"),
        r#"{
            "id": "libchecker",
            "name": "LibChecker",
            "rules": [
                { "id": "r1", "severity": "info", "kind": "permission", "match": { "pattern": "*" } }
            ]
        }"#,
    )
    .unwrap();
    let sets = load_all(&tmp).unwrap();
    assert_eq!(sets.len(), 1);
    assert_eq!(sets[0].id, "libchecker");
    assert_eq!(sets[0].rules.len(), 1);
}

#[test]
fn rule_set_loads_skips_invalid_files() {
    let tmp = tempdir();
    std::fs::write(tmp.join("bad.json"), "{not valid json}").unwrap();
    std::fs::write(
        tmp.join("good.json"),
        r#"{ "id": "g", "name": "Good", "rules": [] }"#,
    )
    .unwrap();
    let sets = load_all(&tmp).unwrap();
    // Invalid JSON file is silently skipped; valid one is returned.
    assert_eq!(sets.len(), 1);
    assert_eq!(sets[0].id, "g");
}

#[test]
fn rule_set_loads_returns_empty_when_dir_missing() {
    let mut tmp = tempdir();
    tmp.push("does-not-exist");
    let sets = load_all(&tmp).unwrap();
    assert!(sets.is_empty());
}

#[test]
fn result_message_uses_description_then_id() {
    let r = Rule {
        id: "rid".into(),
        description: Some("desc-text".into()),
        severity: "info".into(),
        kind: "permission".into(),
        match_: json!({ "pattern": "*" }),
        metadata: None,
    };
    let res: Vec<RuleResult> = evaluate(&[r], "rs1", &info_with_perm("android.permission.INTERNET"));
    assert_eq!(res[0].message, "desc-text");

    let r2 = Rule {
        id: "rid2".into(),
        description: None,
        severity: "info".into(),
        kind: "permission".into(),
        match_: json!({ "pattern": "*" }),
        metadata: None,
    };
    let res2 = evaluate(&[r2], "rs1", &info_with_perm("android.permission.INTERNET"));
    assert_eq!(res2[0].message, "rid2");
}

fn tempdir() -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("jadb-rules-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn native_library_rule_matches_filename() {
    let r = Rule {
        id: "uses-flutter".into(),
        description: None,
        severity: "info".into(),
        kind: "native_library".into(),
        match_: json!({ "file": "libflutter.so" }),
        metadata: None,
    };
    let mut info = ApkInfo::default();
    info.native_libs = vec!["libflutter.so".into(), "libc++_shared.so".into()];
    let res = evaluate(&[r], "rs1", &info);
    assert!(res[0].matched);
    assert_eq!(res[0].evidence.as_deref(), Some("libflutter.so"));
}

#[test]
fn native_library_rule_does_not_match_other_so() {
    let r = Rule {
        id: "uses-flutter".into(),
        description: None,
        severity: "info".into(),
        kind: "native_library".into(),
        match_: json!({ "file": "libflutter.so" }),
        metadata: None,
    };
    let mut info = ApkInfo::default();
    info.native_libs = vec!["libsqlite.so".into()];
    let res = evaluate(&[r], "rs1", &info);
    assert!(!res[0].matched);
    assert!(res[0].evidence.is_none());
}

#[test]
fn native_library_rule_with_empty_file_does_not_match() {
    let r = Rule {
        id: "uses-flutter".into(),
        description: None,
        severity: "info".into(),
        kind: "native_library".into(),
        match_: json!({}),
        metadata: None,
    };
    let res = evaluate(&[r], "rs1", &ApkInfo::default());
    assert!(!res[0].matched);
}

#[test]
fn component_class_rule_matches_activity_class() {
    let r = Rule {
        id: "wechat-entry".into(),
        description: None,
        severity: "info".into(),
        kind: "component_class".into(),
        match_: json!({ "type": "activity", "class": "com.tencent.mm.ui.LauncherUI" }),
        metadata: None,
    };
    let mut info = ApkInfo::default();
    info.activities = vec![
        "com.tencent.mm.SplashActivity".into(),
        "com.tencent.mm.ui.LauncherUI".into(),
    ];
    let res = evaluate(&[r], "rs1", &info);
    assert!(res[0].matched);
    assert_eq!(res[0].evidence.as_deref(), Some("com.tencent.mm.ui.LauncherUI"));
}

#[test]
fn component_class_rule_matches_service_class() {
    let r = Rule {
        id: "jpush-service".into(),
        description: None,
        severity: "info".into(),
        kind: "component_class".into(),
        match_: json!({ "type": "service", "class": "cn.jpush.android.service.DaemonService" }),
        metadata: None,
    };
    let mut info = ApkInfo::default();
    info.services = vec!["cn.jpush.android.service.DaemonService".into()];
    let res = evaluate(&[r], "rs1", &info);
    assert!(res[0].matched);
}

#[test]
fn component_class_rule_does_not_match_wrong_type() {
    let r = Rule {
        id: "x".into(),
        description: None,
        severity: "info".into(),
        kind: "component_class".into(),
        match_: json!({ "type": "activity", "class": "com.example.Foo" }),
        metadata: None,
    };
    let mut info = ApkInfo::default();
    // Same class name, but it's a service — should NOT match.
    info.services = vec!["com.example.Foo".into()];
    let res = evaluate(&[r], "rs1", &info);
    assert!(!res[0].matched);
}

#[test]
fn component_class_rule_does_not_match_unknown_type() {
    let r = Rule {
        id: "x".into(),
        description: None,
        severity: "info".into(),
        kind: "component_class".into(),
        match_: json!({ "type": "alien", "class": "x" }),
        metadata: None,
    };
    let res = evaluate(&[r], "rs1", &ApkInfo::default());
    assert!(!res[0].matched);
}

#[test]
fn action_rule_matches_intent_action() {
    let r = Rule {
        id: "boot-completed".into(),
        description: None,
        severity: "info".into(),
        kind: "action".into(),
        match_: json!({ "name": "android.intent.action.BOOT_COMPLETED" }),
        metadata: None,
    };
    let mut info = ApkInfo::default();
    info.intent_actions = vec![
        "android.intent.action.BOOT_COMPLETED".into(),
        "android.intent.action.PACKAGE_REPLACED".into(),
    ];
    let res = evaluate(&[r], "rs1", &info);
    assert!(res[0].matched);
    assert_eq!(
        res[0].evidence.as_deref(),
        Some("android.intent.action.BOOT_COMPLETED")
    );
}

#[test]
fn action_rule_does_not_match_unrelated() {
    let r = Rule {
        id: "x".into(),
        description: None,
        severity: "info".into(),
        kind: "action".into(),
        match_: json!({ "name": "android.intent.action.BOOT_COMPLETED" }),
        metadata: None,
    };
    let mut info = ApkInfo::default();
    info.intent_actions = vec!["android.intent.action.MAIN".into()];
    let res = evaluate(&[r], "rs1", &info);
    assert!(!res[0].matched);
}

#[test]
fn action_rule_with_empty_name_does_not_match() {
    let r = Rule {
        id: "x".into(),
        description: None,
        severity: "info".into(),
        kind: "action".into(),
        match_: json!({}),
        metadata: None,
    };
    let res = evaluate(&[r], "rs1", &ApkInfo::default());
    assert!(!res[0].matched);
}
