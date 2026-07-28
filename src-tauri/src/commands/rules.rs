use crate::config::settings;
use crate::error::{AppError, AppResult};
use crate::services::apk_analyzer;
use crate::services::rule_manager::{self, ComponentMatches, RuleSet};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[tauri::command]
pub async fn list_rules(app: AppHandle) -> AppResult<Vec<RuleSet>> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    let path = s
        .rules_path
        .ok_or_else(|| AppError::InvalidInput("rulesPath not set".into()))?;
    rule_manager::load_all(&PathBuf::from(path)).map_err(AppError::Config)
}

/// Run libchecker rules against an APK and return a per-component
/// breakdown (native libs / activities / services / receivers /
/// providers). Each component carries the matching rule (if any) so
/// the UI can show "this APK has Flutter" by hovering the
/// `libflutter.so` row.
#[tauri::command]
pub async fn analyze_with_rules(
    app: AppHandle,
    apk_path: String,
    rule_set_ids: Vec<String>,
) -> AppResult<rule_manager::RuleReport> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    let rules_path = s
        .rules_path
        .clone()
        .ok_or_else(|| AppError::InvalidInput("rulesPath not set".into()))?;
    let apk = PathBuf::from(&apk_path);
    if !apk.exists() {
        return Err(AppError::InvalidInput(format!("file not found: {apk_path}")));
    }
    let all_sets = rule_manager::load_all(&PathBuf::from(&rules_path))
        .map_err(AppError::Config)?;
    let selected: Vec<&RuleSet> = all_sets
        .iter()
        .filter(|rs| rule_set_ids.contains(&rs.id))
        .collect();
    let info = apk_analyzer::analyze(&s, &apk).await?;
    let components = rule_manager::evaluate_components(&selected, &info);
    let total_matched = count_matched(&components);
    Ok(rule_manager::RuleReport {
        apk_path,
        components,
        total_matched,
    })
}

fn count_matched(c: &ComponentMatches) -> u32 {
    (c.native_libraries.iter().filter(|h| h.matched_rule.is_some()).count()
        + c.activities.iter().filter(|h| h.matched_rule.is_some()).count()
        + c.services.iter().filter(|h| h.matched_rule.is_some()).count()
        + c.receivers.iter().filter(|h| h.matched_rule.is_some()).count()
        + c.providers.iter().filter(|h| h.matched_rule.is_some()).count()) as u32
}

#[tauri::command]
pub async fn install_rule_packs(
    app: AppHandle,
) -> AppResult<crate::services::rule_pack::RulePackStatus> {
    crate::services::rule_pack::install_all(&app).await
}

#[tauri::command]
pub async fn uninstall_rule_packs(app: AppHandle) -> AppResult<()> {
    crate::services::rule_pack::uninstall(&app).await
}

#[tauri::command]
pub async fn get_rule_pack_status(
    app: AppHandle,
) -> AppResult<crate::services::rule_pack::RulePackStatus> {
    crate::services::rule_pack::status(&app).await
}

#[tauri::command]
pub async fn install_libchecker_rules(
    app: AppHandle,
) -> AppResult<crate::services::rule_pack::RulePackStatus> {
    crate::services::rule_pack::install_libchecker(&app).await
}
