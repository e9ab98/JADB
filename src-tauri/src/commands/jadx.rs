use crate::config::settings;
use crate::error::{AppError, AppResult};
use crate::services::jadx_decompiler::{self, JadxOptions, TaskHandle};
use crate::services::java_runtime;
use crate::services::task_registry::TaskRegistry;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tauri::{AppHandle, Manager, State};

#[tauri::command]
pub async fn jadx_decompile(
    app: AppHandle,
    registry: State<'_, TaskRegistry>,
    path: String,
    out_dir: String,
    options: JadxOptions,
) -> AppResult<TaskHandle> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    jadx_decompiler::decompile(&app, &registry, &s, &path, &out_dir, options).await
}

/// Name of the JADX GUI launcher for the current OS.
fn gui_bin_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "jadx-gui.bat"
    } else {
        "jadx-gui"
    }
}

/// Locate the JADX GUI launcher. `jadx_dir` can be in one of several
/// shapes depending on how it was set:
///
/// 1. Canonical — the parent of `bin/`, e.g.
///    `<root>/tools/jadx-1.5.6/jadx-1.5.6/`. The GUI is at
///    `<root>/tools/jadx-1.5.6/jadx-1.5.6/bin/jadx-gui`.
/// 2. Legacy auto-install — the CLI binary itself, e.g.
///    `<root>/tools/jadx-1.5.6/jadx-1.5.6/bin/jadx`. The GUI is its
///    sibling under `bin/`. (Pre-existing install bug stored the
///    binary path instead of the parent dir.)
/// 3. The `bin/` directory itself, e.g.
///    `<root>/tools/jadx-1.5.6/jadx-1.5.6/bin/`. The GUI is a direct
///    child.
/// 4. Already pointing at the GUI binary (direct hit).
fn resolve_gui_bin(jadx_dir: &str) -> Option<PathBuf> {
    let base = Path::new(jadx_dir);
    let bin = gui_bin_name();

    // (4) Already pointing at the GUI binary.
    if base.is_file()
        && base
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|stem| stem == "jadx-gui")
            .unwrap_or(false)
    {
        return Some(base.to_path_buf());
    }

    // (1) Canonical: `<dir>/bin/jadx-gui`.
    if base.is_dir() {
        let candidate = base.join("bin").join(bin);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    // (2) Legacy: `<dir>/bin/jadx` → sibling `<dir>/bin/jadx-gui`.
    if base.is_file()
        && base
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|stem| stem == "jadx")
            .unwrap_or(false)
    {
        if let Some(parent) = base.parent() {
            let candidate = parent.join(bin);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    // (3) Manual pick: `<dir>/jadx-gui`.
    if base.is_dir() {
        let candidate = base.join(bin);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

/// Launch the JADX GUI in a detached child process so closing JADB does
/// not tear it down. Used by the sidebar "应用 → JADX" entry.
#[tauri::command]
pub async fn launch_jadx_gui(app: AppHandle, apk_path: Option<String>) -> AppResult<()> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    let jadx_dir = s
        .jadx_dir
        .clone()
        .ok_or_else(|| AppError::ToolMissing("jadx".into()))?;

    let bin = resolve_gui_bin(&jadx_dir).ok_or_else(|| {
        let bin = gui_bin_name();
        let expected = Path::new(&jadx_dir).join("bin").join(bin);
        AppError::Config(format!(
            "JADX-GUI 可执行文件不存在:\n  当前 jadx 目录: {}\n  期望路径: {}\n请在 Settings → Tools 重新安装 jadx,或把 jadx 目录指向包含 bin/jadx-gui 的父目录(例如 jadx-1.5.6/)。",
            jadx_dir,
            expected.display(),
        ))
    })?;

    let mut cmd = {
        // Detach from Tauri's controlling session so that closing JADB
        // does not hand SIGHUP to jadx-gui. `nohup` flips SIGHUP to
        // SIG_IGN before exec'ing the launcher; on Windows the .bat
        // launcher already runs in its own console-less process and
        // does not need the wrapper, so we only pay the extra fork
        // cost on macOS / Linux.
        let cmd = if cfg!(target_os = "windows") {
            let mut cmd = Command::new(&bin);
            cmd.arg("-Pdex-input.verify-checksum=no");
            cmd
        } else {
            let mut cmd = Command::new("nohup");
            cmd.arg(&bin).arg("-Pdex-input.verify-checksum=no");
            cmd
        };
        cmd
    };
    // Make sure the bundled (or system) Java is what the jadx-gui
    // launcher script sees. Without these, on a host with no `java`
    // in PATH the wrapper exits with "java: command not found"
    // before even loading jadx-gui.jar. We:
    //   1. set JAVA_HOME (read by the launcher and any JVM it spawns)
    //   2. prepend <JAVA_HOME>/bin to PATH so any plain `java`
    //      invocation (in the launcher, or in child tools) finds ours.
    // If `java` cannot be resolved at all, fall back to the original
    // behaviour and let the launcher report its own error — we do not
    // want to block launching when the user has a known-good
    // system-wide Java that just happens to elude our detector.
    if let Ok(runtime) = java_runtime::resolve(&s, Some(&dir)) {
        cmd.env("JAVA_HOME", &runtime.java_home);
        let mut path = std::env::var_os("PATH").unwrap_or_default();
        let bin_dir = runtime.java_home.join("bin");
        if bin_dir.is_dir() {
            let mut combined = std::ffi::OsString::from(bin_dir.as_os_str());
            if !path.is_empty() {
                combined.push(if cfg!(target_os = "windows") { ";" } else { ":" });
                combined.push(path);
            }
            path = combined;
            cmd.env("PATH", path);
        }
    }
    if let Some(apk_path) = apk_path.as_deref() {
        if apk_path.trim().is_empty() || !Path::new(apk_path).exists() {
            return Err(AppError::InvalidInput("JADX APK path is invalid".into()));
        }
        cmd.arg(apk_path);
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // `JADX_GUI_OPTS` is the jadx-gui script's own knob for
        // JADX-GUI-specific JVM options. The script interpolates it
        // via `eval set -- $DEFAULT_JVM_OPTS $JAVA_OPTS $JADX_GUI_OPTS
        // -jar ...`, so the `-Dfile.encoding` flag lands in front
        // of the main class and is parsed as a JVM option (rather
        // than as a positional argument to the JAR). The script's
        // top comment also documents this knob.
        .env("JADX_GUI_OPTS", "-Dfile.encoding=UTF-8")
        // `JAVA_OPTS` is parsed by the same script as a fallback.
        .env("JAVA_OPTS", "-Dfile.encoding=UTF-8")
        // `JAVA_TOOL_OPTIONS` is read by the JVM at startup as a
        // last-resort channel in case the user's jadx-gui script ever
        // ignores the above two.
        .env("JAVA_TOOL_OPTIONS", "-Dfile.encoding=UTF-8")
        // Force the JVM's locale to UTF-8 even if the user's macOS
        // locale is something other than UTF-8 (e.g., GB18030).
        .env("LANG", "en_US.UTF-8")
        .env("LC_ALL", "en_US.UTF-8");
    cmd.spawn()
        .map_err(|e| AppError::Config(format!("启动 JADX-GUI 失败: {e}")))?;

    Ok(())
}
