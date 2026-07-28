use jadb_lib::config::settings::{Language, Settings, SettingsPatch, ThemeMode};
use jadb_lib::config::settings_test_helpers::{read_settings_from, write_settings_to};

#[test]
fn default_settings_returns_zhcn_system() {
    let s = Settings::default();
    assert_eq!(s.language, Language::ZhCn);
    assert_eq!(s.theme, ThemeMode::System);
    assert!(s.aapt_path.is_none());
    assert!(s.apktool_path.is_none());
}

#[test]
fn round_trip_settings_preserves_all_fields() {
    let tmp = tempdir_in_target();
    let path = tmp.join("settings.json");
    let original = Settings {
        aapt_path: Some("/opt/aapt2".into()),
        adb_path: None,
        apktool_path: Some("/opt/apktool.jar".into()),
        uber_apk_signer_path: None,
        apksigner_path: None,
        android_build_tools_dir: None,
        jadx_dir: Some("/opt/jadx".into()),
        rules_path: None,
        rules_download_url: None,
        language: Language::En,
        theme: ThemeMode::Dark,
    };
    write_settings_to(&path, &original).unwrap();
    let loaded = read_settings_from(&path).unwrap();
    assert_eq!(loaded.aapt_path, original.aapt_path);
    assert_eq!(loaded.language, Language::En);
    assert_eq!(loaded.theme, ThemeMode::Dark);
    assert_eq!(loaded.jadx_dir, original.jadx_dir);
}

#[test]
fn patch_applies_only_non_none_fields() {
    let mut s = Settings::default();
    let patch = SettingsPatch {
        language: Some(Language::En),
        theme: None,
        aapt_path: Some(Some("/usr/bin/aapt2".into())),
        adb_path: None,
        apktool_path: None,
        uber_apk_signer_path: None,
        apksigner_path: None,
        android_build_tools_dir: None,
        jadx_dir: None,
        rules_path: None,
        rules_download_url: None,
    };
    s.apply(patch);
    assert_eq!(s.language, Language::En);
    assert_eq!(s.theme, ThemeMode::System);
    assert_eq!(s.aapt_path.as_deref(), Some("/usr/bin/aapt2"));
}

#[test]
fn patch_can_clear_a_field() {
    let mut s = Settings { aapt_path: Some("/x".into()), ..Settings::default() };
    let patch = SettingsPatch { aapt_path: Some(None), ..SettingsPatch::default() };
    s.apply(patch);
    assert!(s.aapt_path.is_none());
}

fn tempdir_in_target() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("jadb-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
