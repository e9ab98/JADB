use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub fn emit_progress(app: &AppHandle, task_id: &str, percent: f32, stage: &str) {
    let _ = app.emit(
        "task://progress",
        serde_json::json!({ "task_id": task_id, "percent": percent.clamp(0.0, 100.0), "stage": stage }),
    );
}

pub fn emit_log(app: &AppHandle, task_id: &str, line: &str, level: &str) {
    let _ = app.emit(
        "task://log",
        serde_json::json!({ "task_id": task_id, "line": line, "level": level }),
    );
}

pub fn emit_done<V: Serialize>(app: &AppHandle, task_id: &str, result: Option<V>) {
    let payload = match result {
        Some(r) => serde_json::json!({ "task_id": task_id, "result": r }),
        None => serde_json::json!({ "task_id": task_id }),
    };
    let _ = app.emit("task://done", payload);
}

pub fn emit_error(app: &AppHandle, task_id: &str, error: &str) {
    let _ = app.emit(
        "task://error",
        serde_json::json!({ "task_id": task_id, "error": error }),
    );
}
