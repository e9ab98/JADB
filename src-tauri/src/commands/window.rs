use crate::error::{AppError, AppResult};
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};
use crate::services::license::{LicenseService, FEATURE_ADB_MULTI_DEVICE};

/// Prefix used for every apps window so we can detect / focus them by label.
const APPS_WINDOW_PREFIX: &str = "apps-";

/// Tauri window labels are restricted to `[a-zA-Z0-9_-]`. Map anything else to
/// `-` so a serial like `10.0.0.5:5555` becomes `apps-10-0-0-5-5555`.
fn sanitize_label(serial: &str) -> String {
    serial
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

fn window_label(serial: &str) -> String {
    format!("{}{}", APPS_WINDOW_PREFIX, sanitize_label(serial))
}

/// Minimal percent-encoder for path components. Only the chars that appear in
/// adb serials need to round-trip — `:` and `.` are the common ones — so a
/// hand-rolled encoder keeps the dependency footprint small.
fn url_path_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{:02X}", byte)),
        }
    }
    out
}


/// Navigate the main application window to the unified license page and focus it.
/// Standalone child windows call this instead of navigating inside themselves.
#[tauri::command]
pub async fn open_license_center(app: AppHandle) -> AppResult<()> {
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| AppError::NotFound("main window".into()))?;
    let mut url = main.url().map_err(|e| AppError::Config(e.to_string()))?;
    url.set_path("/index.html");
    url.set_query(None);
    url.set_fragment(Some("/settings?tab=license"));
    main.navigate(url)
        .map_err(|e| AppError::Config(e.to_string()))?;
    main.show().map_err(|e| AppError::Config(e.to_string()))?;
    main.unminimize().map_err(|e| AppError::Config(e.to_string()))?;
    main.set_focus().map_err(|e| AppError::Config(e.to_string()))?;
    Ok(())
}

/// Open (or focus) the apps window for a given device serial. One window per
/// device so multiple devices can sit side-by-side.
#[tauri::command]
pub async fn open_apps_window(
    app: AppHandle,
    license: State<'_, LicenseService>,
    serial: String,
) -> AppResult<()> {
    let label = window_label(&serial);

    // If a window already exists for this device, just bring it forward.
    if let Some(window) = app.get_webview_window(&label) {
        window
            .set_focus()
            .map_err(|e| AppError::Config(e.to_string()))?;
        return Ok(());
    }

    // Free users may manage two devices side-by-side. Existing windows are
    // counted rather than currently connected adb devices, so discovery and
    // other adb clients are never disturbed.
    let open_device_windows = app
        .webview_windows()
        .keys()
        .filter(|window_label| window_label.starts_with(APPS_WINDOW_PREFIX))
        .count();
    if open_device_windows >= 2 {
        license.require_feature(&app, FEATURE_ADB_MULTI_DEVICE).await?;
    }

    let url_path = format!(
        "index.html#/apps?serial={}",
        url_path_encode(&serial)
    );
    let title = format!("应用 — {}", serial);

    WebviewWindowBuilder::new(&app, &label, WebviewUrl::App(url_path.into()))
        .title(title)
        .inner_size(1100.0, 760.0)
        .min_inner_size(800.0, 600.0)
        .resizable(true)
        .build()
        .map_err(|e| AppError::Config(e.to_string()))?;

    Ok(())
}

/// Open (or focus) the per-app data-dir file manager window for a given
/// device + package. One window per (device, pkg) pair.
#[tauri::command]
pub async fn open_data_dir_window(
    app: AppHandle,
    device: String,
    pkg: String,
    debuggable: bool,
    use_root: bool,
    root_path: String,
) -> AppResult<()> {
    let label = format!(
        "data-{}-{}",
        sanitize_label(&device),
        sanitize_label(&pkg),
    );

    if let Some(window) = app.get_webview_window(&label) {
        window
            .set_focus()
            .map_err(|e| AppError::Config(e.to_string()))?;
        return Ok(());
    }

    // Embed `as` query param when the package is debuggable so the window
    // uses `run-as <pkg>` for every subsequent operation. Rooted release
    // root-backed sessions carry `root=1` so filesystem commands are
    // elevated via `su`, including debug apps whose `run-as` probe failed.
    let as_query = if debuggable {
        format!("&as={}", url_path_encode(&pkg))
    } else if use_root {
        "&root=1".to_string()
    } else {
        String::new()
    };
    let url_path = format!(
        "index.html#/data-dir?device={}&pkg={}&path={}{}",
        url_path_encode(&device),
        url_path_encode(&pkg),
        url_path_encode(&root_path),
        as_query,
    );
    let title = format!("数据 — {}", pkg);

    WebviewWindowBuilder::new(&app, &label, WebviewUrl::App(url_path.into()))
        .title(title)
        .inner_size(900.0, 600.0)
        .min_inner_size(640.0, 480.0)
        .resizable(true)
        .build()
        .map_err(|e| AppError::Config(e.to_string()))?;

    Ok(())
}

/// Open (or focus) the standalone 反编译 window. Lives at its own URL
/// route so it renders without the main sidebar (see `App.tsx`).
#[tauri::command]
pub async fn open_decompile_window(app: AppHandle, apk_path: Option<String>) -> AppResult<()> {
    const LABEL: &str = "decompile";
    let title = "反编译";

    if let Some(window) = app.get_webview_window(LABEL) {
        if let Some(path) = apk_path {
            let mut url = window.url().map_err(|e| AppError::Config(e.to_string()))?;
            url.set_path("/index.html");
            url.set_query(None);
            url.set_fragment(Some(&format!("/decompile?apk={}", url_path_encode(&path))));
            window.navigate(url).map_err(|e| AppError::Config(e.to_string()))?;
        }
        window.set_focus().map_err(|e| AppError::Config(e.to_string()))?;
        return Ok(());
    }

    WebviewWindowBuilder::new(&app, LABEL, WebviewUrl::App(match apk_path {
        Some(path) => format!("index.html#/decompile?apk={}", url_path_encode(&path)).into(),
        None => "index.html#/decompile".into(),
    }))
        .title(title)
        .inner_size(900.0, 620.0)
        .min_inner_size(640.0, 480.0)
        .resizable(true)
        .build()
        .map_err(|e| AppError::Config(e.to_string()))?;

    Ok(())
}

/// Open (or focus) the standalone 重打包 window.
#[tauri::command]
pub async fn open_repackage_window(app: AppHandle) -> AppResult<()> {
    const LABEL: &str = "repackage";
    let title = "重打包";

    if let Some(window) = app.get_webview_window(LABEL) {
        window
            .set_focus()
            .map_err(|e| AppError::Config(e.to_string()))?;
        return Ok(());
    }

    WebviewWindowBuilder::new(&app, LABEL, WebviewUrl::App("index.html#/repackage".into()))
        .title(title)
        .inner_size(900.0, 620.0)
        .min_inner_size(640.0, 480.0)
        .resizable(true)
        .build()
        .map_err(|e| AppError::Config(e.to_string()))?;

    Ok(())
}

/// Open (or focus) the standalone 分析 window. One window per app — the
/// analyzer workflow can take a while (aapt2 + rule packs), so we keep it
/// out of the main shell like 反编译 / 重打包.
#[tauri::command]
pub async fn open_analyze_window(app: AppHandle, apk_path: Option<String>) -> AppResult<()> {
    const LABEL: &str = "analyze";
    let title = "分析";

    if let Some(window) = app.get_webview_window(LABEL) {
        if let Some(path) = apk_path {
            let mut url = window.url().map_err(|e| AppError::Config(e.to_string()))?;
            url.set_path("/index.html");
            url.set_query(None);
            url.set_fragment(Some(&format!("/analyze?apk={}", url_path_encode(&path))));
            window.navigate(url).map_err(|e| AppError::Config(e.to_string()))?;
        }
        window.set_focus().map_err(|e| AppError::Config(e.to_string()))?;
        return Ok(());
    }

    WebviewWindowBuilder::new(&app, LABEL, WebviewUrl::App(match apk_path {
        Some(path) => format!("index.html#/analyze?apk={}", url_path_encode(&path)).into(),
        None => "index.html#/analyze".into(),
    }))
        .title(title)
        .inner_size(900.0, 720.0)
        .min_inner_size(640.0, 480.0)
        .resizable(true)
        .build()
        .map_err(|e| AppError::Config(e.to_string()))?;

    Ok(())
}
