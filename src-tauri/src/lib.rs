pub mod commands;
pub mod config;
pub mod error;
pub mod progress;
pub mod services;

use crate::services::task_registry::TaskRegistry;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_log::Builder::new().level(log::LevelFilter::Info).build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(TaskRegistry::new())
        .invoke_handler(tauri::generate_handler![
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::tools::get_tool_status,
            commands::tools::install_tool,
            commands::tools::remove_tool,
            commands::analyze::analyze_apk,
            commands::analyze::cancel_task,
            commands::analyze::file_size,
            commands::decompile::decompile_apk,
            commands::decompile::open_path,
            commands::repackage::repackage_apk,
            commands::sign::check_apk_signed,
            commands::sign::sign_apk,
            commands::sign::inspect_signature,
            commands::sign::strip_apk_signing,
            commands::signatures::list_signatures,
            commands::signatures::create_signature,
            commands::signatures::update_signature,
            commands::signatures::delete_signature,
            commands::signatures::import_keystore,
            commands::signatures::create_new_keystore,
            commands::signatures::export_signature,
            commands::lineages::list_lineages,
            commands::lineages::create_lineage,
            commands::lineages::import_lineage,
            commands::lineages::delete_lineage,
            commands::lineages::export_lineage,
            commands::jadx::jadx_decompile,
            commands::jadx::launch_jadx_gui,
            commands::rules::list_rules,
            commands::rules::analyze_with_rules,
            commands::rules::install_rule_packs,
            commands::rules::uninstall_rule_packs,
            commands::rules::get_rule_pack_status,
            commands::rules::install_libchecker_rules,
            commands::system::get_log_path,
            commands::adb::adb_devices,
            commands::adb::adb_connect,
            commands::adb::adb_disconnect,
            commands::adb::adb_list_packages,
            commands::adb::adb_app_info,
            commands::adb::adb_app_icon,
            commands::adb::adb_uninstall,
            commands::adb::adb_export_apks,
            commands::adb::adb_force_stop,
            commands::adb::adb_launch_app,
            commands::adb::adb_clear_cache,
            commands::cache::scan_cache,
            commands::cache::clear_cache,
            commands::adb::adb_shell,
            commands::window::open_apps_window,
            commands::window::open_analyze_window,
            commands::window::open_decompile_window,
            commands::window::open_repackage_window,
            commands::window::open_data_dir_window,
            commands::report::export_apk_report,
            commands::adb::list_remote_dir,
            commands::adb::resolve_app_data_dir,
            commands::adb::delete_remote_file,
            commands::adb::push_file,
            commands::adb::pull_file,
            commands::adb::is_device_rooted,
        ])
        .setup(|_app| {
            log::info!("JADB started");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running JADB application");
}
