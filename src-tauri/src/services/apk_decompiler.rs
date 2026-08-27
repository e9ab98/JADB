use crate::config::settings::Settings;
use crate::error::{AppError, AppResult};
use crate::progress;
use crate::services::java_runtime;
use crate::services::task_registry::TaskRegistry;
use serde::Serialize;
use std::process::Stdio;
use tauri::AppHandle;
use tauri::Manager;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[derive(Serialize, Clone, Debug)]
pub struct TaskHandle {
    pub task_id: String,
    pub kind: String,
}

pub fn build_apktool_args(apk: &str, out: &str, force: bool) -> Vec<String> {
    let mut a = vec!["d".into(), apk.into(), "-o".into(), out.into()];
    if force {
        a.push("-f".into());
    }
    a
}

pub fn parse_apktool_progress(line: &str) -> Option<f32> {
    if let Some(idx) = line.find(']') {
        let after = &line[idx + 1..];
        if let Some(rest) = after.trim().strip_suffix('%') {
            if let Ok(v) = rest.trim().parse::<f32>() {
                return Some(v.clamp(0.0, 100.0));
            }
        }
    }
    None
}

pub async fn decompile(
    app: &AppHandle,
    registry: &TaskRegistry,
    settings: &Settings,
    apk_path: &str,
    out_dir: &str,
    force: bool,
) -> AppResult<TaskHandle> {
    let apktool = settings
        .apktool_path
        .clone()
        .ok_or_else(|| AppError::ToolMissing("apktool".into()))?;

    let task_id = uuid::Uuid::new_v4().to_string();
    let token = registry.register(&task_id);

    // apktool ships only as a JAR (see tool_manager::resolve_binary_path),
    // so spawning it requires `java -jar`. We resolve the runtime here
    // (bundled Temurin JDK -> system java on PATH) so the spawn below
    // can use it instead of failing with EACCES on a non-executable
    // .jar (errno 13 / "Permission denied").
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let runtime = java_runtime::resolve(settings, Some(&dir))?;
    let java_bin = runtime.java_bin.clone();

    let apk = apk_path.to_string();
    let out = out_dir.to_string();
    let app_clone = app.clone();
    let task_id_clone = task_id.clone();
    let token_clone = token.clone();

    tokio::spawn(async move {
        let args = build_apktool_args(&apk, &out, force);
        // Bundled apktool is a .jar without +x; launching it directly
        // returns EACCES. Route through `java -jar` instead. If a user
        // has pointed `apktool_path` at a wrapper script (non-.jar),
        // fall back to direct exec so custom setups keep working.
        let mut cmd = if apktool.to_ascii_lowercase().ends_with(".jar") {
            let mut c = Command::new(&java_bin);
            c.arg("-jar").arg(&apktool).args(&args);
            c
        } else {
            let mut c = Command::new(&apktool);
            c.args(&args);
            c
        };
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        progress::emit_progress(&app_clone, &task_id_clone, 0.0, "启动 apktool");

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                progress::emit_error(&app_clone, &task_id_clone, &format!("spawn apktool failed: {e}"));
                return;
            }
        };

        let stdout = match child.stdout.take() {
            Some(s) => s,
            None => {
                progress::emit_error(&app_clone, &task_id_clone, "apktool stdout unavailable");
                return;
            }
        };
        let stderr = match child.stderr.take() {
            Some(s) => s,
            None => {
                progress::emit_error(&app_clone, &task_id_clone, "apktool stderr unavailable");
                return;
            }
        };

        let mut stdout_lines = BufReader::new(stdout).lines();
        let mut stderr_lines = BufReader::new(stderr).lines();

        let mut exited = false;
        let mut got_progress = false;
        let mut last_percent: f32 = -1.0;
        let mut stdout_done = false;
        let mut stderr_done = false;

        while !(stdout_done && stderr_done && exited) {
            tokio::select! {
                _ = token_clone.cancelled() => {
                    let _ = child.kill().await;
                    progress::emit_error(&app_clone, &task_id_clone, "任务已取消");
                    return;
                }
                line = stdout_lines.next_line(), if !stdout_done => match line {
                    Ok(Some(l)) => {
                        if let Some(pct) = parse_apktool_progress(&l) {
                            if (pct - last_percent).abs() > 0.5 {
                                progress::emit_progress(&app_clone, &task_id_clone, pct, "反编译中");
                                last_percent = pct;
                                got_progress = true;
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
                        if !got_progress && s.success() {
                            progress::emit_progress(&app_clone, &task_id_clone, 100.0, "完成");
                            got_progress = true;
                        }
                        if s.success() {
                            progress::emit_progress(&app_clone, &task_id_clone, 100.0, "完成");
                            progress::emit_done(
                                &app_clone,
                                &task_id_clone,
                                Some(serde_json::Value::String(out.clone())),
                            );
                        } else {
                            let code = s.code().unwrap_or(-1);
                            progress::emit_error(
                                &app_clone,
                                &task_id_clone,
                                &format!("apktool 退出码 {code}"),
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
        kind: "decompile".into(),
    })
}
