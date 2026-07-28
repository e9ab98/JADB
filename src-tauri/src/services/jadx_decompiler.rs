use crate::config::settings::Settings;
use crate::error::{AppError, AppResult};
use crate::progress;
use crate::services::java_runtime;
use crate::services::task_registry::TaskRegistry;
use tauri::Manager;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tauri::AppHandle;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct JadxOptions {
    #[serde(rename = "showInGradle")]
    pub show_in_gradle: bool,
    #[serde(rename = "decompileResources")]
    pub decompile_resources: bool,
    #[serde(rename = "debugInfo")]
    pub debug_info: bool,
    #[serde(rename = "exportAsGradle")]
    pub export_as_gradle: bool,
    #[serde(rename = "threadsCount")]
    pub threads_count: Option<u32>,
}

impl Default for JadxOptions {
    fn default() -> Self {
        Self {
            show_in_gradle: false,
            decompile_resources: true,
            debug_info: true,
            export_as_gradle: false,
            threads_count: None,
        }
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct TaskHandle {
    pub task_id: String,
    pub kind: String,
}

pub fn build_args(_jadx_bin: &str, apk: &str, out: &str, opts: &JadxOptions) -> Vec<String> {
    let mut a: Vec<String> = vec!["-d".into(), out.into(), apk.into()];
    if !opts.decompile_resources {
        a.push("--no-res".into());
    }
    if !opts.debug_info {
        a.push("--no-debug-info".into());
    }
    if opts.export_as_gradle {
        a.push("--export-gradle".into());
    }
    if opts.show_in_gradle {
        a.push("--show-gradle-root".into());
    }
    if let Some(t) = opts.threads_count {
        a.push("--threads-count".into());
        a.push(t.to_string());
    }
    a
}

fn resolve_bin(jadx_dir: &Path) -> PathBuf {
    let bin = if cfg!(target_os = "windows") {
        "bin/jadx.bat"
    } else {
        "bin/jadx"
    };
    jadx_dir.join(bin)
}

pub async fn decompile(
    app: &AppHandle,
    registry: &TaskRegistry,
    settings: &Settings,
    apk_path: &str,
    out_dir: &str,
    options: JadxOptions,
) -> AppResult<TaskHandle> {
    let jadx_dir = settings
        .jadx_dir
        .clone()
        .ok_or_else(|| AppError::ToolMissing("jadx".into()))?;
    let bin = resolve_bin(Path::new(&jadx_dir));

    // Resolve the Java runtime so the jadx shell wrapper can find it.
    // Failure is non-fatal: if the host has a usable `java` we cannot
    // detect (e.g. via a launcher we don't model), let the user see
    // the wrapper's own error message instead of swallowing it here.
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let java_runtime = java_runtime::resolve(settings, Some(&dir)).ok();

    let task_id = uuid::Uuid::new_v4().to_string();
    let token = registry.register(&task_id);

    let app_clone = app.clone();
    let task_id_clone = task_id.clone();
    let token_clone = token.clone();
    let apk_path = apk_path.to_string();
    let out_dir = out_dir.to_string();
    let bin_clone = bin.clone();
    let java_clone = java_runtime;

    tokio::spawn(async move {
        let args = build_args(bin_clone.to_string_lossy().as_ref(), &apk_path, &out_dir, &options);
        let mut cmd = Command::new(&bin_clone);
        cmd.args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(ref runtime) = java_clone {
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

        progress::emit_progress(&app_clone, &task_id_clone, 0.0, "启动 jadx");

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                progress::emit_error(
                    &app_clone,
                    &task_id_clone,
                    &format!("spawn jadx failed: {e}"),
                );
                return;
            }
        };

        let stdout = match child.stdout.take() {
            Some(s) => s,
            None => {
                progress::emit_error(&app_clone, &task_id_clone, "jadx stdout unavailable");
                return;
            }
        };
        let stderr = match child.stderr.take() {
            Some(s) => s,
            None => {
                progress::emit_error(&app_clone, &task_id_clone, "jadx stderr unavailable");
                return;
            }
        };

        let mut stdout_lines = BufReader::new(stdout).lines();
        let mut stderr_lines = BufReader::new(stderr).lines();

        let mut exited = false;
        let mut stdout_done = false;
        let mut stderr_done = false;
        let mut last_percent: f32 = -1.0;

        while !(stdout_done && stderr_done && exited) {
            tokio::select! {
                _ = token_clone.cancelled() => {
                    let _ = child.kill().await;
                    progress::emit_error(&app_clone, &task_id_clone, "任务已取消");
                    return;
                }
                line = stdout_lines.next_line(), if !stdout_done => match line {
                    Ok(Some(l)) => {
                        if let Some(pct) = parse_jadx_percent(&l) {
                            if (pct - last_percent).abs() > 0.5 {
                                progress::emit_progress(&app_clone, &task_id_clone, pct, "反编译中");
                                last_percent = pct;
                            }
                        }
                        progress::emit_log(&app_clone, &task_id_clone, &l, "info");
                    }
                    Ok(None) => stdout_done = true,
                    Err(e) => {
                        progress::emit_log(&app_clone, &task_id_clone, &format!("stdout err: {e}"), "warn");
                        stdout_done = true;
                    }
                },
                line = stderr_lines.next_line(), if !stderr_done => match line {
                    Ok(Some(l)) => {
                        let level = if l.to_lowercase().contains("error") { "error" } else { "warn" };
                        progress::emit_log(&app_clone, &task_id_clone, &l, level);
                    }
                    Ok(None) => stderr_done = true,
                    Err(e) => {
                        progress::emit_log(&app_clone, &task_id_clone, &format!("stderr err: {e}"), "warn");
                        stderr_done = true;
                    }
                },
                status = child.wait(), if !exited => match status {
                    Ok(s) => {
                        if s.success() {
                            progress::emit_progress(&app_clone, &task_id_clone, 100.0, "完成");
                            progress::emit_done(
                                &app_clone,
                                &task_id_clone,
                                Some(serde_json::Value::String(out_dir.clone())),
                            );
                        } else {
                            let code = s.code().unwrap_or(-1);
                            progress::emit_error(
                                &app_clone,
                                &task_id_clone,
                                &format!("jadx 退出码 {code}"),
                            );
                        }
                        exited = true;
                    }
                    Err(e) => {
                        progress::emit_error(&app_clone, &task_id_clone, &format!("wait failed: {e}"));
                        exited = true;
                    }
                },
            }
        }
    });

    Ok(TaskHandle {
        task_id,
        kind: "jadx".into(),
    })
}

/// Best-effort percentage parser for jadx progress lines.
/// jadx prints lines like `... 42% ...`; we strip any prefix and parse the trailing `N%`.
pub fn parse_jadx_percent(line: &str) -> Option<f32> {
    let trimmed = line.trim().trim_start_matches('[');
    let pct = trimmed.split_whitespace().find_map(|tok| {
        let s = tok.trim_start_matches('[').trim_end_matches(']');
        s.strip_suffix('%').and_then(|n| n.parse::<f32>().ok())
    })?;
    Some(pct.clamp(0.0, 100.0))
}
