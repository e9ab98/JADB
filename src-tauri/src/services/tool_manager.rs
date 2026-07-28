use crate::config::settings::{self, Settings, SettingsPatch};
use crate::config::tools::{load_all, ToolEntry, ToolName};
use crate::error::{AppError, AppResult};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};
use tokio::fs;
use tokio::io::AsyncWriteExt;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ToolSource {
    /// The binary lives under the managed `tools/` directory and was put
    /// there by JADB's installer.
    Bundled,
    /// The user pointed us at an existing binary on their machine via
    /// Settings.<tool>_path.
    Local,
    /// We have no configured path yet; `path` is the default location the
    /// install command would write to (the binary is not necessarily on
    /// disk yet — check `installed`).
    Fallback,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ToolStatus {
    pub name: String,
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    pub download_url: Option<String>,
    pub source: ToolSource,
}

#[derive(Clone, Debug)]
pub struct BuildToolsPaths {
    pub root: PathBuf,
    pub apksigner_jar: PathBuf,
    pub zipalign: PathBuf,
    pub version: String,
}

pub fn tool_root(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("tools")
}

pub fn tool_dir(app_data_dir: &Path, entry: &ToolEntry) -> PathBuf {
    tool_root(app_data_dir).join(format!("{}-{}", entry.name.as_str(), entry.version))
}

pub fn resolve_binary_path(entry: &ToolEntry, app_data_dir: &Path) -> AppResult<PathBuf> {
    let dir = tool_dir(app_data_dir, entry);
    match entry.name {
        ToolName::Apktool | ToolName::UberApkSigner => Ok(dir.join(&entry.file_name)),
        ToolName::AndroidBuildTools => {
            Ok(dir.join(entry.unzip_dir.as_deref().unwrap_or("build-tools")))
        }
        ToolName::Jadx => {
            let bin = if cfg!(target_os = "windows") {
                "bin/jadx.bat"
            } else {
                "bin/jadx"
            };
            let sub = entry.unzip_dir.as_deref().unwrap_or("").replace("{os}", os_token());
            Ok(dir.join(sub).join(bin))
        }
        ToolName::Aapt2 => {
            let sub = entry.unzip_dir.as_deref().unwrap_or("").replace("{os}", os_token());
            let bin = entry.binary_sub_path.as_deref().unwrap_or("aapt2");
            let bin = if cfg!(target_os = "windows") && bin == "aapt2" {
                "aapt2.exe"
            } else {
                bin
            };
            Ok(dir.join(sub).join(bin))
        }
        ToolName::Adb => {
            let sub = entry.unzip_dir.as_deref().unwrap_or("").replace("{os}", os_token());
            let bin = if cfg!(target_os = "windows") {
                "adb.exe"
            } else {
                "adb"
            };
            Ok(dir.join(sub).join(bin))
        }
    }
}

fn path_for_tool(s: &Settings, name: &ToolName) -> Option<String> {
    match name {
        ToolName::Apktool => s.apktool_path.clone(),
        ToolName::UberApkSigner => s.uber_apk_signer_path.clone(),
        ToolName::AndroidBuildTools => s.android_build_tools_dir.clone(),
        ToolName::Jadx => s.jadx_dir.clone(),
        ToolName::Aapt2 => s.aapt_path.clone(),
        ToolName::Adb => s.adb_path.clone(),
    }
}

pub fn resolve_android_build_tools_dir(path: &Path) -> AppResult<BuildToolsPaths> {
    if let Some(paths) = inspect_build_tools_dir(path) {
        return Ok(paths);
    }

    let versions_dir = path.join("build-tools");
    let entries = std::fs::read_dir(&versions_dir).map_err(|error| {
        AppError::Config(format!(
            "Android Build-Tools directory not found under {}: {error}",
            path.display()
        ))
    })?;
    let mut candidates = entries
        .flatten()
        .filter_map(|entry| inspect_build_tools_dir(&entry.path()))
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| compare_versions(&left.version, &right.version));
    candidates.pop().ok_or_else(|| {
        AppError::Config(format!(
            "no valid Android Build-Tools installation found under {}",
            versions_dir.display()
        ))
    })
}

fn inspect_build_tools_dir(path: &Path) -> Option<BuildToolsPaths> {
    if !path.is_dir() {
        return None;
    }
    let apksigner_jar = path.join("lib").join("apksigner.jar");
    let zipalign = path.join(if cfg!(target_os = "windows") {
        "zipalign.exe"
    } else {
        "zipalign"
    });
    let source_properties = path.join("source.properties");
    if !apksigner_jar.is_file() || !zipalign.is_file() || !source_properties.is_file() {
        return None;
    }
    let properties = std::fs::read_to_string(source_properties).ok()?;
    let version = properties.lines().find_map(|line| {
        line.strip_prefix("Pkg.Revision=")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })?;
    Some(BuildToolsPaths {
        root: path.to_path_buf(),
        apksigner_jar,
        zipalign,
        version,
    })
}

fn compare_versions(left: &str, right: &str) -> std::cmp::Ordering {
    let left = version_parts(left);
    let right = version_parts(right);
    left.cmp(&right)
}

fn version_parts(version: &str) -> Vec<u32> {
    version
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(|part| part.parse::<u32>().unwrap_or(0))
        .collect()
}

pub async fn status_all(app_data_dir: &Path, s: &Settings) -> Vec<ToolStatus> {
    let entries = load_all();
    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        if entry.name == ToolName::AndroidBuildTools {
            out.push(android_build_tools_status(&entry, app_data_dir, s));
            continue;
        }
        let configured = path_for_tool(s, &entry.name);
        let resolved = resolve_binary_path(&entry, app_data_dir).ok();
        // `installed` is true when either the bundled tool exists at its
        // default location OR the user pointed us at a binary they already
        // have on their machine via Settings.<tool>_path. Without the
        // configured-path check, the Tools UI would report an installed
        // tool as missing the moment a user replaced the download with a
        // local binary.
        let installed_default = resolved.as_ref().map(|p| p.exists()).unwrap_or(false);
        let installed_local = configured
            .as_deref()
            .map(|p| {
                let p = p.trim();
                !p.is_empty() && Path::new(p).is_file()
            })
            .unwrap_or(false);
        let installed = installed_default || installed_local;
        let resolved_str = resolved
            .as_ref()
            .map(|p| p.to_string_lossy().to_string());
        let path = configured.clone().or_else(|| resolved_str.clone());

        // `source` distinguishes "the user pointed us at a *different*
        // binary on their machine" from "the user clicked Install which
        // happens to write the bundled path into Settings". Both end up
        // with `settings.<tool>_path` populated; we only call it `local`
        // when that path differs from the default bundled location.
        let source = if installed_local
            && configured.as_deref() != resolved_str.as_deref()
        {
            ToolSource::Local
        } else if installed_default {
            ToolSource::Bundled
        } else {
            ToolSource::Fallback
        };
        let download_url = if entry.platforms.is_some() {
            entry
                .platforms
                .as_ref()
                .map(|p| p.url().to_string())
        } else if entry.download_url.is_empty() {
            None
        } else {
            Some(entry.download_url.clone())
        };
        out.push(ToolStatus {
            name: entry.name.as_str().into(),
            installed,
            version: Some(entry.version.clone()),
            path,
            download_url,
            source,
        });
    }
    out
}

fn android_build_tools_status(
    entry: &ToolEntry,
    app_data_dir: &Path,
    settings: &Settings,
) -> ToolStatus {
    let managed_path = resolve_binary_path(entry, app_data_dir).ok();
    let managed = managed_path
        .as_deref()
        .and_then(|path| resolve_android_build_tools_dir(path).ok());
    let configured = settings.android_build_tools_dir.as_deref().map(Path::new);
    let configured_is_managed = configured
        .zip(managed_path.as_deref())
        .map(|(configured, managed)| configured == managed)
        .unwrap_or(false);
    let selected = configured
        .and_then(|path| resolve_android_build_tools_dir(path).ok())
        .or_else(|| if configured.is_none() { managed.clone() } else { None });
    let installed = selected.is_some();
    let version = selected
        .as_ref()
        .map(|paths| paths.version.clone())
        .or_else(|| Some(entry.version.clone()));
    let path = selected
        .as_ref()
        .map(|paths| paths.root.to_string_lossy().into_owned())
        .or_else(|| {
            configured
                .map(|path| path.to_string_lossy().into_owned())
                .or_else(|| managed_path.map(|path| path.to_string_lossy().into_owned()))
        });
    let source = if configured.is_some() && !configured_is_managed {
        ToolSource::Local
    } else if managed.is_some() {
        ToolSource::Bundled
    } else {
        ToolSource::Fallback
    };
    let download_url = entry
        .platforms
        .as_ref()
        .map(|platforms| platforms.url().to_string());

    ToolStatus {
        name: entry.name.as_str().into(),
        installed,
        version,
        path,
        download_url,
        source,
    }
}

fn os_token() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "osx"
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

fn resolve_url_and_filename(entry: &ToolEntry) -> AppResult<(String, String)> {
    if let Some(platforms) = &entry.platforms {
        Ok((platforms.url().to_string(), entry.file_name.clone()))
    } else if entry.download_url.is_empty() {
        Err(AppError::Config(format!(
            "{} has no download URL or platforms",
            entry.name.as_str()
        )))
    } else {
        Ok((entry.download_url.clone(), entry.file_name.clone()))
    }
}

async fn download_with_progress(app: &AppHandle, name: &str, url: &str, dest: &Path) -> AppResult<()> {
    let client = reqwest::Client::new();
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::Config(format!("download failed: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Config(format!("download failed: {e}")))?;
    let total = resp.content_length().unwrap_or(0);
    let mut stream = resp.bytes_stream();
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).await?;
    }
    let mut file = fs::File::create(dest).await?;
    let mut downloaded: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| AppError::Config(e.to_string()))?;
        file.write_all(&bytes).await?;
        downloaded += bytes.len() as u64;
        let _ = app.emit(
            "tool://install-progress",
            serde_json::json!({ "name": name, "downloaded": downloaded, "total": total }),
        );
    }
    file.flush().await?;
    if downloaded == 0 {
        return Err(AppError::Config("download returned an empty file".into()));
    }
    Ok(())
}

pub async fn extract_zip(zip_path: &Path, dest_dir: &Path) -> AppResult<()> {
    let bytes = fs::read(zip_path).await?;
    let dest_dir = dest_dir.to_path_buf();
    tokio::task::spawn_blocking(move || -> AppResult<()> {
        let reader = std::io::Cursor::new(bytes);
        let mut zip =
            zip::ZipArchive::new(reader).map_err(|e| AppError::Config(e.to_string()))?;
        std::fs::create_dir_all(&dest_dir)?;

        // Detect "single shared top-level directory" archives (e.g. the
        // Google platform-tools zip, which has every entry nested under
        // `platform-tools/`). When the archive matches that pattern, we
        // strip the shared prefix so the binary lands directly inside
        // `dest_dir`. For archives that already start with multiple
        // top-level entries (jadx: `bin/`, `lib/`, `README.md`, ...) we
        // leave them alone.
        let strip_prefix: Option<std::path::PathBuf> = {
            let names: Vec<String> = zip.file_names().map(str::to_string).collect();
            (|| -> Option<std::path::PathBuf> {
                // Empty archive: nothing to strip and nothing to extract.
                let first = names.first()?;
                // First entry has no slash → there is no shared top-level
                // directory to strip (e.g. jadx's central directory starts
                // with `LICENSE` / `README.md`). Fall through and let the
                // extraction loop run verbatim. The previous incarnation
                // of this match did `return Ok(())` here, which silently
                // skipped extraction for exactly that archive shape.
                let (prefix, _) = first.split_once('/')?;
                let prefix_slash = format!("{prefix}/");
                let all_match = names
                    .iter()
                    .filter(|n| !n.is_empty())
                    .all(|n| *n == prefix || n.starts_with(&prefix_slash));
                all_match.then(|| std::path::PathBuf::from(prefix))
            })()
        };

        for i in 0..zip.len() {
            let mut file = zip
                .by_index(i)
                .map_err(|e| AppError::Config(e.to_string()))?;
            let raw = match file.enclosed_name() {
                Some(p) => p.to_path_buf(),
                None => continue,
            };
            // Drop the leading top-level segment when we detected a single
            // shared prefix; otherwise keep the entry verbatim.
            let stripped = match &strip_prefix {
                Some(prefix) => match raw.strip_prefix(prefix) {
                    Ok(rest) => rest.to_path_buf(),
                    Err(_) => raw,
                },
                None => raw,
            };
            // Skip the empty entry that corresponds to the archive root
            // itself.
            let out = if stripped.as_os_str().is_empty() {
                dest_dir.clone()
            } else {
                dest_dir.join(&stripped)
            };
            if file.is_dir() {
                std::fs::create_dir_all(&out)?;
            } else {
                if let Some(p) = out.parent() {
                    std::fs::create_dir_all(p)?;
                }
                let mut out_file = std::fs::File::create(&out)?;
                std::io::copy(&mut file, &mut out_file)?;
                #[cfg(unix)]
                if let Some(mode) = file.unix_mode() {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&out, std::fs::Permissions::from_mode(mode))?;
                }
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| AppError::Config(e.to_string()))??;
    Ok(())
}

async fn install_android_build_tools(
    app_data_dir: &Path,
    entry: &ToolEntry,
    archive_path: &Path,
) -> AppResult<PathBuf> {
    let tdir = tool_dir(app_data_dir, entry);
    let staging = tdir.join(format!(".build-tools-staging-{}", uuid::Uuid::new_v4()));
    let backup = tdir.join(format!(".build-tools-backup-{}", uuid::Uuid::new_v4()));
    let final_dir = resolve_binary_path(entry, app_data_dir)?;
    let _ = fs::remove_dir_all(&staging).await;
    let _ = fs::remove_dir_all(&backup).await;

    if let Err(error) = extract_zip(archive_path, &staging).await {
        let _ = fs::remove_dir_all(&staging).await;
        return Err(error);
    }

    let staged = match resolve_android_build_tools_dir(&staging) {
        Ok(paths) => paths,
        Err(error) => {
            let _ = fs::remove_dir_all(&staging).await;
            return Err(error);
        }
    };
    if staged.version != entry.version {
        let _ = fs::remove_dir_all(&staging).await;
        return Err(AppError::Config(format!(
            "downloaded Android Build-Tools version {}, expected {}",
            staged.version, entry.version
        )));
    }
    ensure_executable(&staged.zipalign)?;

    let had_existing = final_dir.exists();
    if had_existing {
        fs::rename(&final_dir, &backup).await?;
    }
    if let Err(error) = fs::rename(&staged.root, &final_dir).await {
        if had_existing {
            let _ = fs::rename(&backup, &final_dir).await;
        }
        let _ = fs::remove_dir_all(&staging).await;
        return Err(error.into());
    }

    let _ = fs::remove_dir_all(&backup).await;
    let _ = fs::remove_dir_all(&staging).await;
    let installed = resolve_android_build_tools_dir(&final_dir)?;
    ensure_executable(&installed.zipalign)?;
    Ok(installed.root)
}

fn ensure_executable(path: &Path) -> AppResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(path)?.permissions();
        permissions.set_mode(permissions.mode() | 0o755);
        std::fs::set_permissions(path, permissions)?;
    }
    // On non-unix (Windows) executables don't have a separate execute bit;
    // extension association handles launch, so this is a no-op. Silence
    // the unused-variable lint that triggers here.
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

pub async fn install(app: &AppHandle, name: ToolName) -> AppResult<ToolStatus> {
    let entries = load_all();
    let entry = entries
        .into_iter()
        .find(|e| e.name == name)
        .ok_or_else(|| AppError::NotFound(name.as_str().into()))?;
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let tdir = tool_dir(&dir, &entry);
    fs::create_dir_all(&tdir).await?;

    let (download_url, file_name) = resolve_url_and_filename(&entry)?;
    let file_name = file_name.replace("{os}", os_token());
    let dest = tdir.join(&file_name);

    download_with_progress(app, name.as_str(), &download_url, &dest).await?;

    let final_path = if entry.name == ToolName::AndroidBuildTools {
        install_android_build_tools(&dir, &entry, &dest).await?
    } else {
        if let Some(unzip) = &entry.unzip_dir {
            let unzip_name = unzip.replace("{os}", os_token());
            let unzip_into = tdir.join(&unzip_name);
            let _ = fs::remove_dir_all(&unzip_into).await;
            extract_zip(&dest, &unzip_into).await?;
        }
        let bin = resolve_binary_path(&entry, &dir)?;
        // `jadx_dir` is the parent of `bin/` — both the CLI decompiler
        // and the GUI launcher expect `bin/jadx` and `bin/jadx-gui` to
        // live under it. `resolve_binary_path` returns the CLI binary
        // itself, so we walk up two levels (binary → bin → its parent).
        if entry.name == ToolName::Jadx {
            bin.parent()
                .and_then(|p| p.parent())
                .map(|p| p.to_path_buf())
                .unwrap_or(bin)
        } else {
            bin
        }
    };
    let mut s = settings::read(&dir).await?;
    let patch = match entry.name {
        ToolName::Apktool => SettingsPatch {
            apktool_path: Some(Some(final_path.to_string_lossy().into())),
            ..Default::default()
        },
        ToolName::UberApkSigner => SettingsPatch {
            uber_apk_signer_path: Some(Some(final_path.to_string_lossy().into())),
            ..Default::default()
        },
        ToolName::AndroidBuildTools => SettingsPatch {
            android_build_tools_dir: Some(Some(final_path.to_string_lossy().into())),
            ..Default::default()
        },
        ToolName::Jadx => SettingsPatch {
            jadx_dir: Some(Some(final_path.to_string_lossy().into())),
            ..Default::default()
        },
        ToolName::Aapt2 => SettingsPatch {
            aapt_path: Some(Some(final_path.to_string_lossy().into())),
            ..Default::default()
        },
        ToolName::Adb => SettingsPatch {
            adb_path: Some(Some(final_path.to_string_lossy().into())),
            ..Default::default()
        },
    };
    s.apply(patch);
    settings::write(&dir, &s).await?;
    let _ = app.emit("settings://changed", &s);

    Ok(ToolStatus {
        name: entry.name.as_str().into(),
        installed: true,
        version: Some(entry.version),
        path: Some(final_path.to_string_lossy().to_string()),
        download_url: Some(download_url),
        source: ToolSource::Bundled,
    })
}

pub async fn remove(app: &AppHandle, name: ToolName) -> AppResult<()> {
    let entries = load_all();
    let entry = entries
        .into_iter()
        .find(|e| e.name == name)
        .ok_or_else(|| AppError::NotFound(name.as_str().into()))?;
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let tdir = tool_dir(&dir, &entry);
    if tdir.exists() {
        fs::remove_dir_all(&tdir).await?;
    }
    let mut s = settings::read(&dir).await?;
    let patch = match entry.name {
        ToolName::Apktool => SettingsPatch {
            apktool_path: Some(None),
            ..Default::default()
        },
        ToolName::UberApkSigner => SettingsPatch {
            uber_apk_signer_path: Some(None),
            ..Default::default()
        },
        ToolName::AndroidBuildTools => SettingsPatch {
            android_build_tools_dir: if s
                .android_build_tools_dir
                .as_deref()
                .map(Path::new)
                .map(|path| path.starts_with(&tdir))
                .unwrap_or(false)
            {
                Some(None)
            } else {
                None
            },
            ..Default::default()
        },
        ToolName::Jadx => SettingsPatch {
            jadx_dir: Some(None),
            ..Default::default()
        },
        ToolName::Aapt2 => SettingsPatch {
            aapt_path: Some(None),
            ..Default::default()
        },
        ToolName::Adb => SettingsPatch {
            adb_path: Some(None),
            ..Default::default()
        },
    };
    s.apply(patch);
    settings::write(&dir, &s).await?;
    let _ = app.emit("settings://changed", &s);
    Ok(())
}

#[cfg(test)]
mod extract_tests {
    use super::extract_zip;
    use std::io::Write as _;

    fn extract_to_set(entries: &[(&str, &[u8])]) -> std::collections::HashSet<String> {
        let tmp = std::env::temp_dir().join(format!(
            "jadb-extract-test-{}.zip",
            uuid::Uuid::new_v4()
        ));
        {
            let file = std::fs::File::create(&tmp).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
            for (name, bytes) in entries {
                zip.start_file(*name, opts).unwrap();
                zip.write_all(bytes).unwrap();
            }
            zip.finish().unwrap();
        }
        let dest = std::env::temp_dir().join(format!(
            "jadb-extract-test-{}",
            uuid::Uuid::new_v4()
        ));
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(extract_zip(&tmp, &dest)).unwrap();
        let mut found = std::collections::HashSet::new();
        for entry in walkdir::WalkDir::new(&dest).into_iter().flatten() {
            let rel = entry.path().strip_prefix(&dest).unwrap();
            if rel.as_os_str().is_empty() {
                continue;
            }
            let s = rel.to_string_lossy().replace('\\', "/");
            found.insert(s);
        }
        let _ = std::fs::remove_dir_all(&dest);
        let _ = std::fs::remove_file(&tmp);
        found
    }

    #[test]
    fn extract_zip_strips_single_shared_top_dir() {
        // Simulates the platform-tools layout: every entry is nested under
        // `platform-tools/`. After extraction, `dest_dir` should contain
        // `adb`, `source.properties`, etc. — never a `platform-tools/`
        // sub-directory.
        let entries: Vec<(&str, &[u8])> = vec![
            ("platform-tools/adb", b"adb-binary"),
            ("platform-tools/source.properties", b"PKG=platform-tools"),
            ("platform-tools/lib64/libc.so", b"libc"),
        ];
        let found = extract_to_set(&entries);
        assert!(found.contains("adb"), "missing adb: {:?}", found);
        assert!(found.contains("source.properties"), "{:?}", found);
        assert!(found.contains("lib64/libc.so"), "{:?}", found);
        assert!(
            !found.iter().any(|p| p.starts_with("platform-tools/")),
            "nested platform-tools/ leaked through: {:?}",
            found
        );
    }

    #[test]
    fn extract_zip_keeps_multi_root_layout_intact() {
        // Simulates the jadx layout: multiple top-level entries (`bin/`,
        // `lib/`, `README.md`) with no shared prefix. They should land in
        // `dest_dir` verbatim.
        let entries: Vec<(&str, &[u8])> = vec![
            ("bin/jadx", b"jadx-bin"),
            ("lib/foo.jar", b"jar"),
            ("README.md", b"readme"),
        ];
        let found = extract_to_set(&entries);
        assert!(found.contains("bin/jadx"), "{:?}", found);
        assert!(found.contains("lib/foo.jar"), "{:?}", found);
        assert!(found.contains("README.md"), "{:?}", found);
    }
}
