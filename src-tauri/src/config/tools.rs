use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ToolName {
    #[default]
    Apktool,
    UberApkSigner,
    AndroidBuildTools,
    Jadx,
    Aapt2,
    Adb,
    Java,
}

impl ToolName {
    pub fn as_str(&self) -> &'static str {
        match self {
            ToolName::Apktool => "apktool",
            ToolName::UberApkSigner => "uber-apk-signer",
            ToolName::AndroidBuildTools => "android-build-tools",
            ToolName::Jadx => "jadx",
            ToolName::Aapt2 => "aapt2",
            ToolName::Adb => "adb",
            ToolName::Java => "java",
        }
    }
}

impl FromStr for ToolName {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "apktool" => Ok(ToolName::Apktool),
            "uber-apk-signer" => Ok(ToolName::UberApkSigner),
            "android-build-tools" => Ok(ToolName::AndroidBuildTools),
            "jadx" => Ok(ToolName::Jadx),
            "aapt2" => Ok(ToolName::Aapt2),
            "adb" => Ok(ToolName::Adb),
            "java" => Ok(ToolName::Java),
            other => Err(format!("unknown tool: {other}")),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PerOsNames {
    #[serde(default)]
    pub macos: Option<String>,
    #[serde(default)]
    pub linux: Option<String>,
    #[serde(default)]
    pub windows: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ToolEntry {
    pub name: ToolName,
    pub version: String,
    #[serde(default)]
    pub download_url: String,
    pub file_name: String,
    pub config_name: String,
    #[serde(default)]
    pub unzip_dir: Option<String>,
    #[serde(default)]
    pub platforms: Option<Platforms>,
    #[serde(default)]
    pub binary_sub_path: Option<String>,
    /// Optional override of `file_name` per OS. Used when the upstream
    /// distribution ships different archive formats per platform (e.g.
    /// Adoptium ships `.tar.gz` on mac/linux and `.zip` on Windows).
    /// `{os_token}` placeholders are substituted at install time using
    /// the same `os_token()` helper that already substitutes `{os}`.
    #[serde(default)]
    pub file_name_per_os: Option<PerOsNames>,
}

impl ToolEntry {
    /// Resolve the on-disk filename for the current OS. Prefers the
    /// per-OS override if present; otherwise falls back to `file_name`.
    ///
    /// Note: `os_token` is duplicated here (also defined in
    /// `services::tool_manager`) because putting it in either shared
    /// location would create a `config -> services -> config` import
    /// cycle. The two implementations must stay in sync.
    pub fn file_name_for_current_os(&self) -> String {
        let token: &'static str = {
            #[cfg(target_os = "macos")]
            { "osx" }
            #[cfg(target_os = "linux")]
            { "linux" }
            #[cfg(target_os = "windows")]
            { "windows" }
        };
        let key = match token {
            "osx" => "macos",
            other => other,
        };
        if let Some(per_os) = &self.file_name_per_os {
            let candidate = match key {
                "macos" => per_os.macos.as_ref(),
                "linux" => per_os.linux.as_ref(),
                "windows" => per_os.windows.as_ref(),
                _ => None,
            };
            if let Some(name) = candidate {
                return name.replace("{os_token}", token);
            }
        }
        self.file_name.replace("{os_token}", token)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Platforms {
    pub macos: String,
    pub linux: String,
    pub windows: String,
}

impl Platforms {
    pub fn for_current_os(&self) -> &'static str {
        #[cfg(target_os = "macos")]
        {
            "macos"
        }
        #[cfg(target_os = "linux")]
        {
            "linux"
        }
        #[cfg(target_os = "windows")]
        {
            "windows"
        }
    }
    pub fn url(&self) -> &str {
        match self.for_current_os() {
            "macos" => &self.macos,
            "linux" => &self.linux,
            "windows" => &self.windows,
            _ => unreachable!(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct ToolsFile {
    tools: Vec<ToolEntry>,
}

pub fn load_all() -> Vec<ToolEntry> {
    let raw = include_str!("tools.json");
    let parsed: ToolsFile = serde_json::from_str(raw).expect("tools.json must be valid");
    parsed.tools
}
