use crate::config::settings::Settings;
use crate::error::{AppError, AppResult};
use crate::progress;
use crate::services::apk_signer;
use crate::services::signature_manager;
use crate::services::task_registry::TaskRegistry;
use serde::Serialize;
use std::process::Stdio;
use tauri::AppHandle;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[derive(Serialize, Clone, Debug)]
pub struct TaskHandle {
    pub task_id: String,
    pub kind: String,
}

pub fn build_apktool_b_args(src: &str, out: &str) -> Vec<String> {
    vec!["b".into(), src.into(), "-o".into(), out.into()]
}

pub async fn repackage(
    app: &AppHandle,
    registry: &TaskRegistry,
    settings: &Settings,
    src_dir: &str,
    out_apk: &str,
    sign: bool,
    signature_id: Option<String>,
    schemes: apk_signer::SigningSchemes,
) -> AppResult<TaskHandle> {
    let apktool = settings
        .apktool_path
        .clone()
        .ok_or_else(|| AppError::ToolMissing("apktool".into()))?;
    let build_tools = if sign {
        schemes.validate()?;
        Some(apk_signer::resolve_build_tools(settings)?)
    } else {
        None
    };

    let task_id = uuid::Uuid::new_v4().to_string();
    let _ = registry.register(&task_id);

    let src = src_dir.to_string();
    let out = out_apk.to_string();
    let apktool_clone = apktool.clone();
    let app_clone = app.clone();
    let task_id_clone = task_id.clone();

    tokio::spawn(async move {
        let mut cmd = Command::new(&apktool_clone);
        cmd.args(build_apktool_b_args(&src, &out))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        progress::emit_progress(&app_clone, &task_id_clone, 0.0, "重打包中");

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                progress::emit_error(&app_clone, &task_id_clone, &format!("spawn apktool b: {e}"));
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

        let mut out_reader = BufReader::new(stdout).lines();
        let mut err_reader = BufReader::new(stderr).lines();

        let mut exited = false;
        let mut stdout_done = false;
        let mut stderr_done = false;
        let mut apktool_success = false;

        while !(stdout_done && stderr_done && exited) {
            tokio::select! {
                line = out_reader.next_line(), if !stdout_done => match line {
                    Ok(Some(l)) => {
                        progress::emit_log(&app_clone, &task_id_clone, &l, "info");
                        if l.contains('%') {
                            progress::emit_progress(&app_clone, &task_id_clone, 50.0, "重打包中");
                        }
                    }
                    Ok(None) => stdout_done = true,
                    Err(e) => {
                        progress::emit_log(&app_clone, &task_id_clone, &format!("stdout err: {e}"), "warn");
                        stdout_done = true;
                    }
                },
                line = err_reader.next_line(), if !stderr_done => match line {
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
                        apktool_success = s.success();
                        exited = true;
                    }
                    Err(e) => {
                        progress::emit_error(&app_clone, &task_id_clone, &format!("wait failed: {e}"));
                        exited = true;
                    }
                },
            }
        }

        if !apktool_success {
            progress::emit_error(
                &app_clone,
                &task_id_clone,
                "apktool b 失败，未尝试签名",
            );
            return;
        }

        progress::emit_progress(&app_clone, &task_id_clone, 80.0, "完成重打包");

        // Optional: invoke signer inline using the same task_id, sharing the TaskPanel.
        if sign {
            if let Some(sid) = &signature_id {
                let sig_result = signature_manager::find(&app_clone, sid).await;
                match sig_result {
                    Ok(Some(sig)) => {
                        if let Some(build_tools) = &build_tools {
                            progress::emit_log(&app_clone, &task_id_clone, "开始签名", "info");
                            match apk_signer::run_signing(
                                &app_clone,
                                &task_id_clone,
                                build_tools,
                                &out,
                                &sig,
                                schemes,
                                80.0,
                            )
                            .await
                            {
                                Ok(Some(output)) => {
                                    progress::emit_done(
                                        &app_clone,
                                        &task_id_clone,
                                        Some(output.apk_path.as_str()),
                                    );
                                }
                                Ok(None) => {}
                                Err(e) => {
                                    progress::emit_error(&app_clone, &task_id_clone, &format!("signer error: {e}"));
                                }
                            }
                        } else {
                            progress::emit_error(
                                &app_clone,
                                &task_id_clone,
                                "Android Build-Tools 未配置",
                            );
                        }
                    }
                    Ok(None) => {
                        progress::emit_log(&app_clone, &task_id_clone, &format!("跳过签名：未找到签名 {sid}"), "warn");
                        progress::emit_done(&app_clone, &task_id_clone, Some(out.as_str()));
                    }
                    Err(e) => {
                        progress::emit_error(&app_clone, &task_id_clone, &format!("signature load failed: {e}"));
                    }
                }
            } else {
                progress::emit_log(&app_clone, &task_id_clone, "跳过签名：未指定签名 id", "warn");
                progress::emit_done(&app_clone, &task_id_clone, Some(out.as_str()));
            }
        } else {
            progress::emit_done(&app_clone, &task_id_clone, Some(out.as_str()));
        }

    });

    Ok(TaskHandle {
        task_id,
        kind: "repackage".into(),
    })
}
