use crate::config::settings::Settings;
use crate::error::{AppError, AppResult};
use std::path::{Path, PathBuf};

/// Minimum Java major version JADB requires (JADX 1.5.x and the
/// single-file `java <file.java>` invocation in `commands::lineages`
/// both need >= 11). When a system `java` is found with a lower major
/// version we ignore it and continue the search so the bundled JDK is
/// still used.
const MIN_JAVA_MAJOR: u32 = 11;

/// Result of locating a usable Java runtime. `java_bin` is what we
/// invoke (`java` or `java.exe`); `java_home` is what we set as
/// `JAVA_HOME` for spawned children that care (jadx-gui launcher,
/// etc.).
#[derive(Debug, Clone)]
pub struct JavaRuntime {
    pub java_home: PathBuf,
    pub java_bin: PathBuf,
    /// Whether this runtime came from a bundled download (vs the
    /// host PATH). Surfaced in error messages so the user knows
    /// whether to install or fix PATH.
    pub bundled: bool,
}

/// Resolve the best available Java runtime. Order of preference:
///   1. Bundled JDK under `settings.java_dir` (if the user installed
///      it via Settings → Tools and the directory still exists).
///   2. Host PATH: scan `$PATH` (or `PATH` + Windows-specific dirs)
///      for `java` binaries, accept the first one whose `-version`
///      reports major >= `MIN_JAVA_MAJOR`.
///   3. Error: `ToolMissing("java")`.
///
/// This function is sync; the only side-effecting lookup it does is
/// spawning `java -version`, which is bounded by PATH length.
pub fn resolve(settings: &Settings, app_data_dir: Option<&Path>) -> AppResult<JavaRuntime> {
    if let Some(runtime) = resolve_bundled(settings, app_data_dir) {
        return Ok(runtime);
    }
    if let Some(runtime) = resolve_system() {
        return Ok(runtime);
    }
    Err(AppError::ToolMissing("java".into()))
}

/// Look for the bundled JDK that the tool installer wrote into
/// `settings.java_dir`. `app_data_dir` is used as a fallback in case
/// `java_dir` is empty but the install directory exists.
fn resolve_bundled(settings: &Settings, app_data_dir: Option<&Path>) -> Option<JavaRuntime> {
    // Prefer the explicit settings entry. Fall back to the default
    // managed location so a freshly-installed JDK is found even before
    // settings.json has been written back (race during install).
    let candidates: Vec<PathBuf> = settings
        .java_dir
        .as_ref()
        .map(|p| vec![PathBuf::from(p)])
        .unwrap_or_default();
    let mut candidates = candidates;
    if let Some(root) = app_data_dir {
        let default_path = root
            .join("tools")
            .join(format!("java-{}", settings_java_version_tag(settings)));
        if default_path.is_dir() {
            candidates.push(default_path);
        }
    }
    for dir in candidates {
        if let Some(runtime) = probe_jdk_at(&dir) {
            return Some(JavaRuntime {
                java_home: runtime.0,
                java_bin: runtime.1,
                bundled: true,
            });
        }
    }
    None
}

/// Walk a candidate root and find both `java_home` and the absolute
/// `java` binary. Returns `(java_home, java_bin)` on success.
///
/// Adoptium's tarball/zip layout:
///   linux/windows: `<root>/bin/java`            (JAVA_HOME = <root>)
///   macOS:          `<root>/Contents/Home/bin/java` (JAVA_HOME = <root>/Contents/Home)
fn probe_jdk_at(root: &Path) -> Option<(PathBuf, PathBuf)> {
    let bin_name = if cfg!(target_os = "windows") {
        "java.exe"
    } else {
        "java"
    };
    // Linux/Windows layout first — it is also a valid subset of any
    // macOS JDK extracted without the `Contents/Home` prefix.
    let direct = root.join("bin").join(bin_name);
    if direct.is_file() {
        return Some((root.to_path_buf(), direct));
    }
    let mac = root.join("Contents").join("Home").join("bin").join(bin_name);
    if mac.is_file() {
        let home = root.join("Contents").join("Home");
        return Some((home, mac));
    }
    None
}

/// Heuristic to compute the directory name JADB uses for managed
/// installs. Mirrors `tool_manager::tool_dir`'s `name-version` shape.
fn settings_java_version_tag(_settings: &Settings) -> &'static str {
    // tools.json pins the bundled JDK to major 21; the directory is
    // named `java-21` regardless of micro version.
    "21"
}

/// Scan the host PATH for any usable `java`. We accept the first hit
/// whose `-version` output reports major >= `MIN_JAVA_MAJOR`.
fn resolve_system() -> Option<JavaRuntime> {
    let path_var = std::env::var_os("PATH")?;
    let bin_name = if cfg!(target_os = "windows") {
        "java.exe"
    } else {
        "java"
    };
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(bin_name);
        if !candidate.is_file() {
            continue;
        }
        let output = match std::process::Command::new(&candidate)
            .arg("-version")
            .output()
        {
            Ok(o) => o,
            Err(_) => continue,
        };
        // `java -version` writes to stderr on every JDK I know of.
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.is_empty() && java_major_from_version_output(&stderr) >= Some(MIN_JAVA_MAJOR) {
            // For system installs we don't always have a clean
            // JAVA_HOME; the convention is `<java_bin>/..` for
            // linux/windows or `../..` for the macOS layout. Fall
            // back to walking up until we find `release`.
            let home = candidate
                .parent()
                .and_then(|p| p.parent())
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| candidate.parent().unwrap_or(&candidate).to_path_buf());
            return Some(JavaRuntime {
                java_home: home,
                java_bin: candidate,
                bundled: false,
            });
        }
    }
    None
}

/// Parse the major version from `java -version` stderr. Recognised
/// shapes:
///   `openjdk version "21.0.1" 2025-10-15`
///   `openjdk version "1.8.0_392"`
///   `java version "17.0.10" 2025-...`
/// Returns None on any unparseable input.
fn java_major_from_version_output(text: &str) -> Option<u32> {
    let quoted = text.split('"').nth(1)?;
    let mut parts = quoted.split('.');
    let first = parts.next()?.parse::<u32>().ok()?;
    // The old 1.x.x layout encodes major as the *second* component.
    if first == 1 {
        let second = parts.next()?.parse::<u32>().ok()?;
        Some(second)
    } else {
        Some(first)
    }
}

#[cfg(test)]
mod tests {
    use super::java_major_from_version_output;

    #[test]
    fn parses_modern_jdk_output() {
        let s = r#"openjdk version "21.0.1" 2025-10-15
OpenJDK Runtime Environment Temurin-21.0.1+12 (build 21.0.1+12)
OpenJDK 64-Bit Server VM Temurin-21.0.1+12 (build 21.0.1+12, mixed mode, sharing)
"#;
        assert_eq!(java_major_from_version_output(s), Some(21));
    }

    #[test]
    fn parses_legacy_1x_output() {
        let s = r#"java version "1.8.0_392"
Java(TM) SE Runtime Environment (build 1.8.0_392-b08)
"#;
        assert_eq!(java_major_from_version_output(s), Some(8));
    }

    #[test]
    fn rejects_unparseable() {
        assert_eq!(java_major_from_version_output("nope"), None);
    }
}
