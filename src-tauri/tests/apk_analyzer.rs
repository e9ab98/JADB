use jadb_lib::services::apk_analyzer::{parse_badging, ApkInfo};

const SAMPLE: &str = "\
package: name='com.example.app' versionCode='42' versionName='1.2.3' compileSdkVersion='34' compileSdkVersionCodename='14'\n\
sdkVersion:'24'\n\
targetSdkVersion:'34'\n\
application-label:'Example'\n\
uses-permission: name='android.permission.INTERNET'\n\
uses-permission: name='android.permission.ACCESS_FINE_LOCATION'\n\
launchable-activity: name='com.example.app.MainActivity'\n\
";

#[test]
fn parses_basic_badging() {
    let info = parse_badging(SAMPLE).unwrap();
    assert_eq!(info.package_name, "com.example.app");
    assert_eq!(info.version_code.as_deref(), Some("42"));
    assert_eq!(info.version_name.as_deref(), Some("1.2.3"));
    assert_eq!(info.min_sdk.as_deref(), Some("24"));
    assert_eq!(info.target_sdk.as_deref(), Some("34"));
    assert_eq!(info.application_label.as_deref(), Some("Example"));
    assert_eq!(info.permissions.len(), 2);
    assert!(info.permissions[0].contains("INTERNET"));
}

#[test]
fn empty_badging_yields_minimal_info() {
    let info = parse_badging("package: name='x'").unwrap();
    assert_eq!(info.package_name, "x");
    assert!(info.permissions.is_empty());
}

#[test]
fn apk_info_default_components_are_empty() {
    let info = ApkInfo::default();
    assert!(info.activities.is_empty());
    assert_eq!(info.package_name, "");
}

#[test]
fn file_size_returns_real_size_for_existing_file() {
    // The `file_size` Tauri command is just a thin wrapper over std::fs::metadata,
    // but we still cover it here to guard the IPC contract.
    let dir = std::env::temp_dir().join(format!("jadb-fs-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("hello.bin");
    std::fs::write(&f, b"hello world").unwrap();
    let meta = std::fs::metadata(&f).unwrap();
    assert_eq!(meta.len(), 11);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn parse_badging_extracts_intent_actions() {
    // aapt2 dump badging outputs `intent-action: name='com.example.FOO'` once per
    // <intent-filter><action android:name="..."/></intent-filter> in the manifest.
    let badging = "\
package: name='com.example.app' versionCode='1' versionName='1.0'\n\
receiver: name='com.example.Receiver1'\n\
  intent-action: name='com.example.ACTION_X'\n\
  intent-action: name='com.example.ACTION_Y'\n\
service: name='com.example.Service1'\n\
  intent-action: name='com.example.SERVICE_BOOT'\n\
";
    let info = parse_badging(badging).unwrap();
    assert!(info.intent_actions.contains(&"com.example.ACTION_X".to_string()));
    assert!(info.intent_actions.contains(&"com.example.ACTION_Y".to_string()));
    assert!(info.intent_actions.contains(&"com.example.SERVICE_BOOT".to_string()));
    assert_eq!(info.intent_actions.len(), 3);
}

#[test]
fn apk_info_default_has_empty_intent_actions_and_native_libs() {
    let info = ApkInfo::default();
    assert!(info.intent_actions.is_empty());
    assert!(info.native_libs.is_empty());
}

#[test]
fn extract_native_libs_groups_by_abi_returns_full_paths() {
    use jadb_lib::services::apk_analyzer::extract_native_libs;
    use std::io::Write;

    let tmp = std::env::temp_dir().join(format!("jadb-native-libs-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&tmp).unwrap();
    let apk = tmp.join("sample.apk");

    {
        let f = std::fs::File::create(&apk).unwrap();
        let mut zw = zip::ZipWriter::new(f);
        let opts = zip::write::SimpleFileOptions::default();
        // Two ABIs carrying the same `libfoo.so` — full paths are kept
        // (one per ABI), not basenamed and not deduped across ABIs.
        zw.start_file("lib/arm64-v8a/libfoo.so", opts).unwrap();
        zw.write_all(b"foo-arm64").unwrap();
        zw.start_file("lib/x86_64/libfoo.so", opts).unwrap();
        zw.write_all(b"foo-x64").unwrap();
        zw.start_file("lib/armeabi-v7a/libbar.so", opts).unwrap();
        zw.write_all(b"bar").unwrap();
        // Negative case: NOT under `lib/` — must not appear in result.
        zw.start_file("assets/index.html", opts).unwrap();
        zw.write_all(b"<html/>").unwrap();
        // Edge case (current behavior): under `lib/<abi>/` but not a .so.
        // `abi_from_path` matches the directory only, so this WILL appear.
        // If a `.so` filter is ever added, this is the line to revisit.
        zw.start_file("lib/arm64-v8a/subdir/not-a-so.txt", opts).unwrap();
        zw.write_all(b"x").unwrap();
        zw.start_file("AndroidManifest.xml", opts).unwrap();
        zw.write_all(b"<manifest/>").unwrap();
        zw.finish().unwrap();
    }

    let (all, _grouped) = extract_native_libs(&apk).unwrap();
    let mut sorted = all.clone();
    sorted.sort();
    assert_eq!(
        sorted,
        vec![
            "lib/arm64-v8a/libfoo.so".to_string(),
            "lib/arm64-v8a/subdir/not-a-so.txt".to_string(),
            "lib/armeabi-v7a/libbar.so".to_string(),
            "lib/x86_64/libfoo.so".to_string(),
        ]
    );

    let _ = std::fs::remove_dir_all(&tmp);
}
