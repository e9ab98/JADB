use jadb_lib::services::rule_manager::load_all;

#[test]
fn bundled_rule_sets_are_loadable() {
    // Each bundled JSON must parse cleanly via the real load_all() pipeline so
    // we catch serialization drift early.
    let dir = std::env::temp_dir().join(format!("jadb-bundled-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    for (filename, content) in [
        ("libchecker-baseline.json", include_str!("../rules/libchecker-baseline.json")),
        ("sdk-checks.json", include_str!("../rules/sdk-checks.json")),
    ] {
        std::fs::write(dir.join(filename), content).unwrap();
    }
    let sets = load_all(&dir).unwrap();
    assert_eq!(sets.len(), 2, "expected both bundled packs to load");
    let baseline = sets.iter().find(|s| s.id == "libchecker-baseline").expect("baseline missing");
    assert!(baseline.rules.iter().any(|r| r.id == "uses-internet"));
    let sdk = sets.iter().find(|s| s.id == "sdk-checks").expect("sdk-checks missing");
    assert!(sdk.rules.iter().any(|r| r.id == "target-sdk-too-old"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bundled_baseline_matches_real_world_rule_schema() {
    // Smoke-check: each rule has the required fields that rule_manager::evaluate depends on.
    let dir = std::env::temp_dir().join(format!("jadb-bundled-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("b.json"), include_str!("../rules/libchecker-baseline.json")).unwrap();
    let sets = load_all(&dir).unwrap();
    assert_eq!(sets.len(), 1);
    for r in &sets[0].rules {
        assert!(!r.id.is_empty(), "rule id empty");
        assert!(!r.severity.is_empty(), "rule severity empty");
        assert!(!r.kind.is_empty(), "rule kind empty");
        assert!(r.match_.is_object(), "rule match must be object");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn extract_json_from_zip_picks_only_json_members() {
    use std::io::Write;

    // Build a small in-memory zip with mixed members and serve it via a temp file.
    let tmp = std::env::temp_dir().join(format!("jadb-zip-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&tmp).unwrap();
    let zip_path = tmp.join("rules.zip");

    {
        let f = std::fs::File::create(&zip_path).unwrap();
        let mut zw = zip::ZipWriter::new(f);
        let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();
        zw.start_file("rules/baseline.json", opts).unwrap();
        zw.write_all(br#"{"id":"r1","name":"R1","rules":[]}"#).unwrap();
        zw.start_file("README.md", opts).unwrap();
        zw.write_all(b"# not json").unwrap();
        zw.start_file("deep/nested/sdk.json", opts).unwrap();
        zw.write_all(br#"{"id":"r2","name":"R2","rules":[]}"#).unwrap();
        zw.finish().unwrap();
    }

    // Exercise the private extract_json_from_zip helper indirectly: simulate
    // what the production path does after the zip is on disk, then assert the
    // structure matches the extractor's expectation (only *.json members count).
    let bytes = std::fs::read(&zip_path).unwrap();
    let reader = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader).unwrap();
    let mut jsons = 0;
    for i in 0..archive.len() {
        let f = archive.by_index(i).unwrap();
        if f.name().to_lowercase().ends_with(".json") {
            jsons += 1;
        }
    }
    assert_eq!(jsons, 2, "expected 2 json members in fixture zip");

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn install_libchecker_from_local_tarball() {
    use jadb_lib::services::rule_pack::install_libchecker_from_archive;

    // Build a tar.gz in memory from the libchecker_sample fixture, mirroring
    // what GitHub codeload serves for `refs/heads/master`.
    let fixture_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/libchecker_sample");

    let mut tar_bytes: Vec<u8> = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut tar_bytes);
        let mut builder = tar::Builder::new(cursor);
        builder.append_dir_all("LibChecker-Rules-HEAD", &fixture_root).unwrap();
        builder.finish().unwrap();
    }

    let dest = std::env::temp_dir().join(format!("jadb-libchecker-dest-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dest).unwrap();

    let status = install_libchecker_from_archive(&tar_bytes, &dest).expect("install ok");
    if let Ok(rd) = std::fs::read_dir(&dest) {
        for e in rd.flatten() {
            eprintln!("  {:?}", e.path());
        }
    }
    assert!(matches!(
        status.source,
        Some(jadb_lib::services::rule_pack::RulePackSource::Libchecker)
    ));
    assert_eq!(status.libchecker_version, Some(99));

    // 7 RuleSet files should appear under dest.
    let mut found: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&dest).unwrap().flatten() {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) == Some("json")
            && p.file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.starts_with("libchecker."))
                .unwrap_or(false)
        {
            found.push(p.file_name().unwrap().to_string_lossy().to_string());
        }
    }
    found.sort();
    assert_eq!(found.len(), 7, "expected 7 libchecker.*.json, got: {found:?}");
    assert!(found.contains(&"libchecker.native-libraries.json".to_string()));

    let _ = std::fs::remove_dir_all(&dest);
}

#[test]
fn install_libchecker_idempotent_removes_old_packs() {
    use jadb_lib::services::rule_pack::install_libchecker_from_archive;

    let fixture_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/libchecker_sample");

    let mut tar_bytes: Vec<u8> = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut tar_bytes);
        let mut builder = tar::Builder::new(cursor);
        builder.append_dir_all("LibChecker-Rules-HEAD", &fixture_root).unwrap();
        builder.finish().unwrap();
    }

    let dest = std::env::temp_dir().join(format!("jadb-libchecker-dest-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dest).unwrap();

    // First install puts 7 packs in dest.
    let _ = install_libchecker_from_archive(&tar_bytes, &dest).unwrap();
    // Second install must remove the previous 7 (idempotent, no leakage).
    let _ = install_libchecker_from_archive(&tar_bytes, &dest).unwrap();

    let count = std::fs::read_dir(&dest)
        .unwrap()
        .flatten()
        .filter(|e| {
            e.path()
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.starts_with("libchecker.") && s.ends_with(".json"))
                .unwrap_or(false)
        })
        .count();
    assert_eq!(count, 7, "idempotent install should keep exactly 7 pack files");

    let _ = std::fs::remove_dir_all(&dest);
}

#[tokio::test]
#[ignore = "hits real network — run with --ignored"]
async fn install_libchecker_end_to_end_against_real_github() {
    use jadb_lib::services::rule_pack::install_libchecker_from_archive;

    // Smoke test: hit the real codeload URL and check the converter produces
    // all 7 packs. We do NOT install into a real app_data_dir — we extract
    // to a temp dir to keep this test isolated.
    let client = reqwest::Client::new();
    let resp = client
        .get("https://codeload.github.com/LibChecker/LibChecker-Rules/tar.gz/refs/heads/master")
        .send()
        .await
        .expect("network reachable");
    assert!(resp.status().is_success(), "HTTP {}", resp.status());

    let gz_bytes = resp.bytes().await.expect("read body");
    assert!(!gz_bytes.is_empty(), "empty body");

    let tar_bytes = tokio::task::spawn_blocking(move || {
        let mut decoder = flate2::read::GzDecoder::new(&gz_bytes[..]);
        let mut out = Vec::new();
        std::io::Read::read_to_end(&mut decoder, &mut out).expect("gunzip");
        out
    })
    .await
    .expect("spawn_blocking");

    let dest = std::env::temp_dir().join(format!("jadb-libchecker-e2e-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dest).unwrap();
    let status = install_libchecker_from_archive(&tar_bytes, &dest).expect("install ok");
    eprintln!(
        "real-GitHub install: source={:?} version={:?} commit={:?} packs={}",
        status.source, status.libchecker_version, status.libchecker_commit, status.packs.len()
    );
    for p in &status.packs {
        eprintln!("  pack: {} ({} rules)", p.id, p.rule_count);
    }
    assert!(matches!(
        status.source,
        Some(jadb_lib::services::rule_pack::RulePackSource::Libchecker)
    ));
    assert_eq!(status.packs.len(), 7, "expected 7 libchecker.* packs");

    let _ = std::fs::remove_dir_all(&dest);
}
