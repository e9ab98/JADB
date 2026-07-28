use jadb_lib::services::libchecker_converter::convert_dir;

fn fixture_root() -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/fixtures/libchecker_sample");
    p
}

#[test]
fn convert_dir_produces_seven_rule_sets() {
    let sets = convert_dir(&fixture_root(), "abc1234").expect("convert should succeed");
    assert_eq!(sets.len(), 7, "expected 7 RuleSets, got: {:?}", sets.iter().map(|s| &s.id).collect::<Vec<_>>());

    let ids: Vec<&str> = sets.iter().map(|s| s.id.as_str()).collect();
    for expected in [
        "libchecker.native-libraries",
        "libchecker.activities",
        "libchecker.services",
        "libchecker.receivers",
        "libchecker.providers",
        "libchecker.intent-actions",
        "libchecker.static-libraries",
    ] {
        assert!(ids.contains(&expected), "missing RuleSet: {expected}");
    }
}

#[test]
fn native_libraries_pack_contains_kind_native_library() {
    let sets = convert_dir(&fixture_root(), "abc1234").unwrap();
    let pack = sets
        .iter()
        .find(|s| s.id == "libchecker.native-libraries")
        .expect("native-libraries pack missing");
    assert_eq!(pack.rules.len(), 2);
    let test_rule = pack
        .rules
        .iter()
        .find(|r| r.match_.get("file").and_then(|v| v.as_str()) == Some("libtest.so"))
        .expect("libtest.so rule missing");
    assert_eq!(test_rule.kind, "native_library");
    assert!(
        test_rule.description.as_deref().unwrap_or_default().starts_with("TestNative"),
        "description should embed label, got: {:?}",
        test_rule.description
    );
    let meta = test_rule.metadata.as_ref().expect("metadata present");
    assert_eq!(meta.get("label").and_then(|v| v.as_str()), Some("TestNative"));
    assert_eq!(meta.get("dev_team").and_then(|v| v.as_str()), Some("Acme"));
    assert_eq!(meta.get("source_link").and_then(|v| v.as_str()), Some("https://example.com/test"));
    assert!(meta.get("zh_description").and_then(|v| v.as_str()).unwrap_or_default().contains("测试"));
}

#[test]
fn activities_pack_uses_component_class_kind() {
    let sets = convert_dir(&fixture_root(), "abc1234").unwrap();
    let pack = sets
        .iter()
        .find(|s| s.id == "libchecker.activities")
        .expect("activities pack missing");
    assert_eq!(pack.rules.len(), 1);
    let r = &pack.rules[0];
    assert_eq!(r.kind, "component_class");
    assert_eq!(r.match_.get("type").and_then(|v| v.as_str()), Some("activity"));
    assert_eq!(
        r.match_.get("class").and_then(|v| v.as_str()),
        Some("com.example.FooActivity")
    );
}

#[test]
fn services_pack_uses_service_type() {
    let sets = convert_dir(&fixture_root(), "abc1234").unwrap();
    let pack = sets.iter().find(|s| s.id == "libchecker.services").unwrap();
    assert_eq!(pack.rules[0].kind, "component_class");
    assert_eq!(
        pack.rules[0].match_.get("type").and_then(|v| v.as_str()),
        Some("service")
    );
    assert_eq!(
        pack.rules[0].match_.get("class").and_then(|v| v.as_str()),
        Some("com.example.FooService")
    );
}

#[test]
fn actions_pack_uses_action_kind() {
    let sets = convert_dir(&fixture_root(), "abc1234").unwrap();
    let pack = sets.iter().find(|s| s.id == "libchecker.intent-actions").unwrap();
    assert_eq!(pack.rules[0].kind, "action");
    assert_eq!(
        pack.rules[0].match_.get("name").and_then(|v| v.as_str()),
        Some("com.example.ACTION_X")
    );
}

#[test]
fn static_libraries_pack_uses_manifest_kind() {
    let sets = convert_dir(&fixture_root(), "abc1234").unwrap();
    let pack = sets.iter().find(|s| s.id == "libchecker.static-libraries").unwrap();
    assert_eq!(pack.rules[0].kind, "manifest");
    assert!(
        pack.rules[0]
            .match_
            .get("contains")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .contains("trichrome"),
        "static heuristic should embed path fragment"
    );
}

#[test]
fn pack_version_picks_up_cloud_md5_version() {
    let sets = convert_dir(&fixture_root(), "abc1234").unwrap();
    // Every pack should carry the rulesVersion read from cloud/md5/v4.
    for s in &sets {
        assert_eq!(s.version.as_deref(), Some("99"), "pack {} wrong version", s.id);
    }
}

#[test]
fn pack_description_includes_commit_short_sha() {
    let sets = convert_dir(&fixture_root(), "deadbee").unwrap();
    let s = &sets[0];
    assert!(
        s.description.as_deref().unwrap_or_default().contains("deadbee"),
        "description should mention commit, got: {:?}",
        s.description
    );
}

#[test]
fn convert_dir_on_missing_root_returns_empty() {
    let sets = convert_dir(
        &std::env::temp_dir().join("jadb-nonexistent-libchecker-xyz"),
        "x",
    )
    .expect("missing root should be tolerated, not error");
    assert!(sets.is_empty(), "got: {:?}", sets.iter().map(|s| &s.id).collect::<Vec<_>>());
}
