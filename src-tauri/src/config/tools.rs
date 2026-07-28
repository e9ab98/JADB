use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ToolName {
    Apktool,
    UberApkSigner,
    AndroidBuildTools,
    Jadx,
    Aapt2,
    Adb,
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
            other => Err(format!("unknown tool: {other}")),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
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
