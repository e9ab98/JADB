use jadb_lib::config::tools::{Platforms, ToolEntry, ToolName};
use jadb_lib::services::tool_manager::resolve_binary_path;

fn fixture_apktool() -> ToolEntry {
    ToolEntry {
        name: ToolName::Apktool,
        version: "2.12.1".into(),
        download_url: "https://example.com/apktool.jar".into(),
        file_name: "apktool_2.12.1.jar".into(),
        config_name: "apktool_path".into(),
        unzip_dir: None,
        platforms: None,
        binary_sub_path: None,
    }
}

fn fixture_jadx() -> ToolEntry {
    ToolEntry {
        name: ToolName::Jadx,
        version: "1.5.6".into(),
        download_url: "https://example.com/jadx.zip".into(),
        file_name: "jadx-1.5.6.zip".into(),
        config_name: "jadx_dir".into(),
        unzip_dir: Some("jadx-1.5.6".into()),
        platforms: None,
        binary_sub_path: None,
    }
}

fn fixture_aapt2() -> ToolEntry {
    ToolEntry {
        name: ToolName::Aapt2,
        version: "7.2.2-7984345".into(),
        download_url: "".into(),
        file_name: "aapt2-7.2.2-7984345-{os}.jar".into(),
        config_name: "aapt_path".into(),
        unzip_dir: Some("aapt2-{os}".into()),
        platforms: Some(Platforms {
            macos: "macos-url".into(),
            linux: "linux-url".into(),
            windows: "windows-url".into(),
        }),
        binary_sub_path: Some("aapt2".into()),
    }
}

#[test]
fn apktool_entry_resolves_to_jar_path() {
    let e = fixture_apktool();
    let resolved = resolve_binary_path(&e, std::path::Path::new("/tmp/jadb")).unwrap();
    assert!(resolved.ends_with("apktool-2.12.1/apktool_2.12.1.jar"));
}

#[test]
fn jadx_entry_resolves_to_jadx_binary() {
    let e = fixture_jadx();
    let resolved = resolve_binary_path(&e, std::path::Path::new("/tmp/jadb")).unwrap();
    let expected_suffix = if cfg!(target_os = "windows") {
        "jadx-1.5.6/jadx-1.5.6/bin/jadx.bat"
    } else {
        "jadx-1.5.6/jadx-1.5.6/bin/jadx"
    };
    assert!(resolved.ends_with(expected_suffix));
}

#[test]
fn aapt2_entry_resolves_to_inner_binary() {
    let e = fixture_aapt2();
    let resolved = resolve_binary_path(&e, std::path::Path::new("/tmp/jadb")).unwrap();
    // After the {os} placeholder is substituted, the path should contain the per-OS subdir.
    let expected_os_subdir = if cfg!(target_os = "macos") { "aapt2-osx" } else if cfg!(target_os = "linux") { "aapt2-linux" } else { "aapt2-windows" };
    let s = resolved.to_string_lossy();
    assert!(!s.contains("aapt2-{os}"), "path still has literal {{os}}: {s}");
    assert!(s.contains(expected_os_subdir), "got {s}");
}

#[test]
fn load_all_matches_tools_json_names() {
    use jadb_lib::config::tools::{load_all, ToolName};
    let entries = load_all();
    let names: Vec<ToolName> = entries.into_iter().map(|e| e.name).collect();
    assert!(names.contains(&ToolName::Apktool));
    assert!(names.contains(&ToolName::UberApkSigner));
    assert!(names.contains(&ToolName::Jadx));
    assert!(names.contains(&ToolName::Aapt2));
}

#[test]
fn toolname_kebab_case_roundtrip() {
    use jadb_lib::config::tools::ToolName;
    for (variant, expected) in [
        (ToolName::Apktool, "apktool"),
        (ToolName::UberApkSigner, "uber-apk-signer"),
        (ToolName::Jadx, "jadx"),
        (ToolName::Aapt2, "aapt2"),
    ] {
        let s = serde_json::to_string(&variant).unwrap();
        assert_eq!(s, format!("\"{expected}\""), "serialize {variant:?}");
        let back: ToolName = serde_json::from_str(&s).unwrap();
        assert_eq!(back, variant, "deserialize {expected}");
    }
}

fn fixture_aapt2_with_os_placeholder() -> ToolEntry {
    // Mirrors tools.json entry for aapt2 verbatim.
    ToolEntry {
        name: ToolName::Aapt2,
        version: "7.2.2-7984345".into(),
        download_url: "".into(),
        file_name: "aapt2-7.2.2-7984345-{os}.jar".into(),
        config_name: "aapt_path".into(),
        unzip_dir: Some("aapt2-{os}".into()),
        platforms: Some(Platforms {
            macos: "macos-url".into(),
            linux: "linux-url".into(),
            windows: "windows-url".into(),
        }),
        binary_sub_path: Some("aapt2".into()),
    }
}

#[test]
fn aapt2_resolved_path_has_no_os_placeholder() {
    // Regression: resolve_binary_path used to leave "{os}" literal in the path,
    // causing status_all.installed to always be false even after a successful install.
    let e = fixture_aapt2_with_os_placeholder();
    let resolved = resolve_binary_path(&e, std::path::Path::new("/tmp/jadb")).unwrap();
    let s = resolved.to_string_lossy();
    assert!(!s.contains("{os}"), "path still contains literal {{os}}: {s}");
    // On macOS/Linux the resolved binary should end with "aapt2" (no .exe).
    #[cfg(not(target_os = "windows"))]
    assert!(s.ends_with("aapt2"), "unexpected tail: {s}");
}

#[tokio::test]
async fn aapt2_status_installed_after_extract() {
    // Simulate what install() does: extract the jar into tdir/<unzip_dir with {os} replaced>,
    // then verify status_all reports installed=true via the same resolve_binary_path.
    use jadb_lib::services::tool_manager::status_all;
    use jadb_lib::config::settings::Settings;

    let e = fixture_aapt2_with_os_placeholder();
    let dir = std::path::Path::new("/tmp/jadb-aapt2-status-test");
    let _ = std::fs::remove_dir_all(dir);
    // Mirror tool_root + tool_dir: <app_data>/tools/<name>-<version>
    let tdir = dir.join("tools").join(format!("{}-{}", e.name.as_str(), e.version));
    let os_dir_name = e.unzip_dir.as_deref().unwrap().replace("{os}", {
        #[cfg(target_os = "macos")] {"osx"}
        #[cfg(target_os = "linux")] {"linux"}
        #[cfg(target_os = "windows")] {"windows"}
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))] {""}
    });
    let unzip_into = tdir.join(&os_dir_name);
    std::fs::create_dir_all(&unzip_into).unwrap();
    // Drop a fake binary where the real install would put it.
    std::fs::write(unzip_into.join("aapt2"), b"fake").unwrap();

    let status = status_all(dir, &Settings::default()).await;
    let aapt2 = status.iter().find(|s| s.name == "aapt2").expect("aapt2 status missing");
    assert!(aapt2.installed, "aapt2 should report installed; got path={:?}", aapt2.path);
    assert!(aapt2.path.as_deref().unwrap_or("").ends_with(&format!("{os_dir_name}/aapt2")), "path={:?}", aapt2.path);

    let _ = std::fs::remove_dir_all(dir);
}

fn fixture_adb() -> ToolEntry {
    ToolEntry {
        name: ToolName::Adb,
        version: "r35.0.0".into(),
        download_url: "".into(),
        file_name: "platform-tools_r35.0.0-{os}.zip".into(),
        config_name: "adb_path".into(),
        unzip_dir: Some("platform-tools".into()),
        platforms: Some(Platforms {
            macos: "macos-url".into(),
            linux: "linux-url".into(),
            windows: "windows-url".into(),
        }),
        binary_sub_path: Some("adb".into()),
    }
}

#[test]
fn adb_entry_resolves_to_binary() {
    let e = fixture_adb();
    let resolved = resolve_binary_path(&e, std::path::Path::new("/tmp/jadb")).unwrap();
    let s = resolved.to_string_lossy();
    // platform-tools has no {os} placeholder, so the resolved path should land directly there.
    let expected_tail = if cfg!(target_os = "windows") {
        "platform-tools/adb.exe"
    } else {
        "platform-tools/adb"
    };
    assert!(s.ends_with(expected_tail), "got {s}");
}
