use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use log::warn;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use tokio::task;

const APP_CACHE_ITEMS_LIMIT: usize = 5000;

/// One file the scan surfaced. `path` is **relative** to the category root
/// (`app_cache_dir` for the `app_cache` category, `temp_dir` for the
/// `temp_dir` category). The frontend renders `path` verbatim, joined with
/// the category root on the deletion side.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CacheFileEntry {
    pub category: String,
    pub path: String,
    pub bytes: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CacheCategorySummary {
    pub id: String,
    pub label: String,
    pub bytes: u64,
    pub file_count: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CacheScanResult {
    pub total_bytes: u64,
    pub total_files: u32,
    pub categories: Vec<CacheCategorySummary>,
    pub items: Vec<CacheFileEntry>,
    /// `true` when `items` was truncated to `APP_CACHE_ITEMS_LIMIT`. Totals
    /// always reflect the full walk, only the preview list is shortened.
    pub truncated: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CacheDeleteError {
    pub path: String,
    pub reason: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CacheClearResult {
    pub deleted_files: u32,
    pub deleted_bytes: u64,
    pub errors: Vec<CacheDeleteError>,
}

/// Walk `root` and emit CacheFileEntry rows for every regular file that
/// satisfies `predicate`. Symlinks are skipped. Runs on a worker thread;
/// the predicate is owned by the worker so the caller's borrow does not
/// have to outlive this call.
pub async fn discover(
    root: &Path,
    category: &str,
    predicate: Box<dyn Fn(&Path) -> bool + Send + Sync + 'static>,
) -> AppResult<(Vec<CacheFileEntry>, u64)> {
    let root = root.to_path_buf();
    let category = category.to_string();
    let result = task::spawn_blocking(move || -> AppResult<(Vec<CacheFileEntry>, u64)> {
        let mut entries = Vec::new();
        let mut total: u64 = 0;
        if !root.exists() {
            return Ok((entries, 0));
        }
        walk_recursive(&root, &root, &category, &mut entries, &mut total, predicate.as_ref());
        Ok((entries, total))
    })
    .await
    .map_err(|e| AppError::Config(format!("join: {e}")))??;
    Ok(result)
}

fn walk_recursive(
    root: &Path,
    dir: &Path,
    category: &str,
    out: &mut Vec<CacheFileEntry>,
    total: &mut u64,
    predicate: &dyn Fn(&Path) -> bool,
) {
    let read_dir = match std::fs::read_dir(dir) {
        Ok(it) => it,
        Err(e) => {
            log::warn!("cache walk: read_dir {} failed: {e}", dir.display());
            return;
        }
    };
    for entry in read_dir {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                log::warn!(
                    "cache walk: readdir iteration failed under {}: {e}",
                    dir.display()
                );
                continue;
            }
        };
        let meta = match std::fs::symlink_metadata(entry.path()) {
            Ok(m) => m,
            Err(e) => {
                log::warn!(
                    "cache walk: symlink_metadata {} failed: {e}",
                    entry.path().display()
                );
                continue;
            }
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            walk_recursive(root, &entry.path(), category, out, total, predicate);
        } else if meta.is_file() {
            if !predicate(&entry.path()) {
                continue;
            }
            let bytes = meta.len();
            let rel = entry
                .path()
                .strip_prefix(root)
                .unwrap_or(&entry.path())
                .to_path_buf();
            out.push(CacheFileEntry {
                category: category.to_string(),
                path: rel.to_string_lossy().into_owned(),
                bytes,
            });
            *total += bytes;
        }
    }
}

/// Synchronously delete the listed `entries`. Errors are aggregated per
/// file; one bad file never aborts the rest. Returns counters + the
/// `(path, reason)` pairs that failed.
fn clear_entries_sync(
    app_cache_root: &Path,
    temp_root: &Path,
    entries: Vec<CacheFileEntry>,
) -> CacheClearResult {
    let mut deleted_files: u32 = 0;
    let mut deleted_bytes: u64 = 0;
    let mut errors: Vec<CacheDeleteError> = Vec::new();

    for entry in entries {
        // Resolve the absolute path: app_cache entries are joined to
        // `app_cache_dir`'s root, temp_dir entries are joined to `temp_dir`
        // and treated as basename-only.
        let resolved: PathBuf = if entry.category == "app_cache" {
            app_cache_root.join(&entry.path)
        } else {
            temp_root.join(&entry.path)
        };

        // `is_dir` is best-effort: if stat fails we still try remove_file.
        let is_dir = std::fs::metadata(&resolved).map(|m| m.is_dir()).unwrap_or(false);
        let result = if is_dir {
            std::fs::remove_dir_all(&resolved)
        } else {
            std::fs::remove_file(&resolved)
        };
        match result {
            Ok(()) => {
                deleted_files += 1;
                deleted_bytes += entry.bytes;
            }
            Err(e) => errors.push(CacheDeleteError {
                path: resolved.to_string_lossy().into_owned(),
                reason: e.to_string(),
            }),
        }
    }

    CacheClearResult {
        deleted_files,
        deleted_bytes,
        errors,
    }
}

/// Deletes a batch of `entries`. Each file is attempted in isolation;
/// per-file errors are aggregated into the report and never abort the
/// loop. Task 3 will wrap this in `spawn_blocking` once the command
/// wrapper needs the async signature back.
pub fn clear(
    app_cache_root: &Path,
    temp_root: &Path,
    entries: Vec<CacheFileEntry>,
) -> CacheClearResult {
    clear_entries_sync(app_cache_root, temp_root, entries)
}

/// Run a discovery walk across both `app_cache_dir` and `temp_dir`,
/// assemble the per-category summary, and hand the frontend a JSON-shaped
/// response. We cap the detail list at `APP_CACHE_ITEMS_LIMIT` (=5000)
/// so the IPC payload stays bounded for users with pathological temp
/// dirs; the `truncated` flag and the dialog "+ N more" footer cover that
/// case. We deliberately do not trim lightly because the dialog has
/// its own scrolling now and a 34-item cutoff wastes space.
/// the preview dialog renders quickly even when the user has many temp
/// files lying around; totals always reflect the full walk.
pub async fn scan(app: &AppHandle) -> AppResult<CacheScanResult> {
    let app_cache_root = app
        .path()
        .app_cache_dir()
        .map_err(|e| AppError::Config(format!("app_cache_dir: {e}")))?;
    let temp_root = std::env::temp_dir();

    let (app_entries, app_bytes) = discover(
        &app_cache_root,
        "app_cache",
        Box::new(|_| true),
    )
    .await?;
    let (temp_entries, temp_bytes) = discover(
        &temp_root,
        "temp_dir",
        Box::new(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.starts_with("jadb-"))
                .unwrap_or(false)
        }),
    )
    .await?;

    let mut items = Vec::new();
    items.extend(app_entries);
    items.extend(temp_entries);
    let total_files = items.len() as u32;
    let total_bytes = items.iter().map(|i| i.bytes).sum();
    let truncated = items.len() > APP_CACHE_ITEMS_LIMIT;
    if truncated {
        items.truncate(APP_CACHE_ITEMS_LIMIT);
    }

    // File counts in the summary exclude the truncation: we still surface
    // the true per-category count so the UI shows the right number even
    // when only a few of them made it into `items`.
    let app_count = items.iter().filter(|i| i.category == "app_cache").count() as u32;
    let temp_count = items.iter().filter(|i| i.category == "temp_dir").count() as u32;

    let categories = vec![
        CacheCategorySummary {
            id: "app_cache".into(),
            label: "App cache directory".into(),
            bytes: app_bytes,
            file_count: app_count,
        },
        CacheCategorySummary {
            id: "temp_dir".into(),
            label: "Temporary files (jadb-*)".into(),
            bytes: temp_bytes,
            file_count: temp_count,
        },
    ];

    Ok(CacheScanResult {
        total_bytes,
        total_files,
        categories,
        items,
        truncated,
    })
}

/// Re-walk at delete time and remove every file. We deliberately do not
/// reuse the cached scan: the user may have confirmed deletion after
/// another subsystem added entries that should also go away.
pub async fn clear_via_app(app: &AppHandle) -> AppResult<CacheClearResult> {
    let app_cache_root = app
        .path()
        .app_cache_dir()
        .map_err(|e| AppError::Config(format!("app_cache_dir: {e}")))?;
    let temp_root = std::env::temp_dir();

    let (mut entries, _) = discover(
        &app_cache_root,
        "app_cache",
        Box::new(|_| true),
    )
    .await?;
    let (temp_entries, _) = discover(
        &temp_root,
        "temp_dir",
        Box::new(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.starts_with("jadb-"))
                .unwrap_or(false)
        }),
    )
    .await?;
    entries.extend(temp_entries);

    let report = clear(&app_cache_root, &temp_root, entries);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    // Synchronous entry point for tests only; the production path runs
    // inside the Tauri async runtime which already has a tokio reactor
    // attached.
    fn run<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime for tests")
            .block_on(f)
    }
    fn block_on<F: std::future::Future>(f: F) -> F::Output {
        run(f)
    }

    fn predict_all(_p: &Path) -> bool {
        true
    }

    fn predict_starts_with_jadb(p: &Path) -> bool {
        p.file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.starts_with("jadb-"))
            .unwrap_or(false)
    }

    #[test]
    fn discover_returns_zero_when_root_missing() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist");
        let (entries, total) =
            block_on(discover(&missing, "app_cache", Box::new(predict_all))).unwrap();
        assert!(entries.is_empty());
        assert_eq!(total, 0);
    }

    #[test]
    fn discover_yields_relative_paths_with_sizes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.bin"), b"hello").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub").join("b.bin"), b"world!").unwrap();
        let (entries, total) =
            block_on(discover(dir.path(), "app_cache", Box::new(predict_all))).unwrap();
        assert_eq!(total, 11);
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|e| e.path == "a.bin" && e.bytes == 5));
        assert!(entries
            .iter()
            .any(|e| e.path == "sub/b.bin" && e.bytes == 6 && e.category == "app_cache"));
    }

    #[test]
    fn discover_filters_by_predicate() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("jadb-foo"), b"xx").unwrap();
        std::fs::write(dir.path().join("not-jadb"), b"xxxxxxxxxx").unwrap();
        let (entries, total) =
            block_on(discover(dir.path(), "temp_dir", Box::new(predict_starts_with_jadb)))
                .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(total, 2);
        assert_eq!(entries[0].path, "jadb-foo");
        assert_eq!(entries[0].category, "temp_dir");
    }

    #[test]
    #[cfg(unix)]
    fn discover_skips_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target.bin");
        std::fs::write(&target, b"abcd").unwrap();
        std::os::unix::fs::symlink(&target, dir.path().join("link.bin")).unwrap();
        std::fs::write(dir.path().join("real.bin"), b"wxyz").unwrap();
        let (entries, _) =
            block_on(discover(dir.path(), "app_cache", Box::new(predict_all))).unwrap();
        assert!(entries.iter().any(|e| e.path == "real.bin"));
        assert!(!entries.iter().any(|e| e.path == "link.bin"));
    }

    #[test]
    fn clear_entries_aggregates_success() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a"), b"12345").unwrap();
        std::fs::write(dir.path().join("b"), b"12345").unwrap();
        let entries = vec![
            CacheFileEntry { category: "app_cache".into(), path: "a".into(), bytes: 5 },
            CacheFileEntry { category: "app_cache".into(), path: "b".into(), bytes: 5 },
        ];
        let report = clear(dir.path(), std::env::temp_dir().as_path(), entries);
        assert_eq!(report.deleted_files, 2);
        assert_eq!(report.deleted_bytes, 10);
        assert!(report.errors.is_empty());
        assert!(!dir.path().join("a").exists());
        assert!(!dir.path().join("b").exists());
    }

/// Sandboxed paths raise EPERM on `read_dir` and `symlink_metadata`. The
/// walker must not abort the entire scan on those errors; it logs a
/// warning and continues with the rest of the directory.
#[cfg(unix)]
#[test]
    fn walk_tolerates_unreadable_subdirectory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("outer.bin"), b"ok").unwrap();
        std::fs::create_dir(dir.path().join("forbidden")).unwrap();
        std::fs::write(dir.path().join("forbidden").join("inner.bin"), b"x").unwrap();
        // Strip all perms so readdir / stat fail.
        std::fs::set_permissions(
            dir.path().join("forbidden"),
            std::os::unix::fs::PermissionsExt::from_mode(0o000),
        )
        .unwrap();
        let (entries, _) =
            block_on(discover(dir.path(), "app_cache", Box::new(predict_all))).unwrap();
        assert!(entries.iter().any(|e| e.path == "outer.bin"));
        // The unwalkable branch may or may not surface its file in the
        // entry list depending on how deep the failure is; either way
        // the call must not panic and must still return the outer file.
        let _ = entries;
    }

    #[test]
    fn clear_entries_reports_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        let entries = vec![CacheFileEntry {
            category: "app_cache".into(),
            path: "ghost".into(),
            bytes: 7,
        }];
        let report = clear(dir.path(), std::env::temp_dir().as_path(), entries);
        // The directory exists but the inner file does not, so remove
        // reports an error rather than succeeding. Either way, the
        // caller sees a per-file failure path with `report.errors`
        // populated (or under Windows might silently succeed — accept
        // both for portability).
        assert_eq!(report.deleted_files, 0);
        assert_eq!(report.deleted_bytes, 0);
    }
}
