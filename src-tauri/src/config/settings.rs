use crate::error::AppError;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

#[derive(Default, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    ZhCn,
    En,
}

#[derive(Default, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Default, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aapt_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adb_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apktool_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uber_apk_signer_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apksigner_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub android_build_tools_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jadx_dir: Option<String>,
    /// Path to the bundled JDK root directory (the parent of `bin/java`
    /// on linux/windows, or `Contents/Home` on macOS). Set by the
    /// tool installer after `Install` succeeds in Settings → Tools.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules_path: Option<String>,
    /// Optional remote URL pointing at a zip that contains LibChecker-style JSON rule files.
    /// When set, `install_rule_packs` downloads from here first; falls back to bundled on failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules_download_url: Option<String>,
    /// Optional URL of a remote JADB license-server. When set, the local
    /// LicenseService will call `/api/v1/license/verify` on each status
    /// refresh; a server-side `revoked` will take precedence over the
    /// local offline signature check. Network failure falls back to the
    /// existing offline validation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license_server_url: Option<String>,
    #[serde(default)]
    pub language: Language,
    #[serde(default)]
    pub theme: ThemeMode,
}

/// Deserialise a `Option<Option<T>>` so that JSON `null` is treated as
/// `Some(None)` (explicit clear) and JSON strings are treated as
/// `Some(Some(value))`. A missing key still maps to `None` (no-op).
///
/// Without this, the default serde-json behaviour maps both missing keys
/// and explicit JSON `null` to `Option::None`, which means the UI cannot
/// tell "leave this field alone" from "clear this field". In practice that
/// manifested as the "Clear local path" button in Settings → Tools being
/// silently ignored.
fn double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    use serde::de::{IntoDeserializer, Error};

    // Materialise into a generic JSON value first so we can distinguish
    // `null` from any other shape.
    let v = serde_json::Value::deserialize(deserializer)?;
    match v {
        serde_json::Value::Null => Ok(Some(None)),
        other => Ok(Some(Some(
            T::deserialize(other.into_deserializer()).map_err(Error::custom)?,
        ))),
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "double_option")]
    pub aapt_path: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "double_option")]
    pub adb_path: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "double_option")]
    pub apktool_path: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "double_option")]
    pub uber_apk_signer_path: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "double_option")]
    pub apksigner_path: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "double_option")]
    pub android_build_tools_dir: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "double_option")]
    pub jadx_dir: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "double_option")]
    pub java_dir: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "double_option")]
    pub rules_path: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "double_option")]
    pub rules_download_url: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "double_option")]
    pub license_server_url: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<Language>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeMode>,
}

impl Settings {
    pub fn apply(&mut self, patch: SettingsPatch) {
        if let Some(v) = patch.aapt_path { self.aapt_path = v; }
        if let Some(v) = patch.adb_path { self.adb_path = v; }
        if let Some(v) = patch.apktool_path { self.apktool_path = v; }
        if let Some(v) = patch.uber_apk_signer_path { self.uber_apk_signer_path = v; }
        if let Some(v) = patch.apksigner_path { self.apksigner_path = v; }
        if let Some(v) = patch.android_build_tools_dir { self.android_build_tools_dir = v; }
        if let Some(v) = patch.jadx_dir { self.jadx_dir = v; }
        if let Some(v) = patch.java_dir { self.java_dir = v; }
        if let Some(v) = patch.rules_path { self.rules_path = v; }
        if let Some(v) = patch.rules_download_url { self.rules_download_url = v; }
        if let Some(v) = patch.license_server_url { self.license_server_url = v; }
        if let Some(v) = patch.language { self.language = v; }
        if let Some(v) = patch.theme { self.theme = v; }
    }
}

pub fn settings_path(app_data_dir: &Path) -> std::path::PathBuf {
    app_data_dir.join("settings.json")
}

pub async fn read(app_data_dir: &Path) -> Result<Settings, AppError> {
    let path = settings_path(app_data_dir);
    if !path.exists() {
        return Ok(Settings::default());
    }
    let bytes = fs::read(&path).await?;
    let s: Settings = serde_json::from_slice(&bytes).map_err(|e| AppError::Config(e.to_string()))?;
    Ok(s)
}

pub async fn write(app_data_dir: &Path, settings: &Settings) -> Result<(), AppError> {
    let path = settings_path(app_data_dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    let json = serde_json::to_vec_pretty(settings).map_err(|e| AppError::Config(e.to_string()))?;
    fs::write(&path, json).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&path)?.permissions();
        perm.set_mode(0o600);
        std::fs::set_permissions(&path, perm)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_patch_explicit_null_clears_path() {
        // Sending `{"adbPath": null}` should clear the setting, not be
        // silently dropped. Without `double_option`, the default serde-json
        // behaviour maps both "field absent" and "field == null" to
        // `Option::None`, which meant the UI's "Clear local path" button
        // couldn't actually clear anything.
        let json = r#"{"adbPath": null}"#;
        let patch: SettingsPatch = serde_json::from_str(json).unwrap();
        assert_eq!(patch.adb_path, Some(None));
    }

    #[test]
    fn settings_patch_string_path_wraps_in_some_some() {
        let json = r#"{"adbPath": "/usr/local/bin/adb"}"#;
        let patch: SettingsPatch = serde_json::from_str(json).unwrap();
        assert_eq!(patch.adb_path, Some(Some("/usr/local/bin/adb".into())));
    }

    #[test]
    fn settings_patch_missing_field_means_noop() {
        // `{}` should leave the field at its default (None).
        let patch: SettingsPatch = serde_json::from_str("{}").unwrap();
        assert_eq!(patch.adb_path, None);
        assert_eq!(patch.jadx_dir, None);
    }

    #[test]
    fn settings_patch_round_trip_preserves_clear() {
        // Build a patch via the public surface (frontend payload) and
        // verify the apply() semantics: an explicit clear wins.
        let mut s = Settings {
            adb_path: Some("/old/path".into()),
            ..Default::default()
        };
        let patch: SettingsPatch = serde_json::from_str(r#"{"adbPath": null}"#).unwrap();
        s.apply(patch);
        assert_eq!(s.adb_path, None, "explicit null must clear the path");
    }

    #[test]
    fn settings_patch_round_trip_ignores_unrelated_fields() {
        // A patch that only touches `theme` should not touch `adb_path`.
        let mut s = Settings {
            adb_path: Some("/kept".into()),
            ..Default::default()
        };
        let patch: SettingsPatch = serde_json::from_str(r#"{"theme": "dark"}"#).unwrap();
        s.apply(patch);
        assert_eq!(s.adb_path, Some("/kept".into()));
        assert_eq!(s.theme, ThemeMode::Dark);
    }
}
