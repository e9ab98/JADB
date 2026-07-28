use crate::config::settings;
use crate::error::{AppError, AppResult};
use crate::services::libchecker_converter;
use futures_util::StreamExt;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;

/// `<app_data_dir>/rules/` — default destination for installed rule packs.
pub fn rules_root(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("rules")
}

/// Files bundled into the binary via `include_str!`. Used as fallback when
/// no download URL is configured or the download yields no JSON files.
const BUNDLED: &[(&str, &str)] = &[
    (
        "libchecker-baseline.json",
        include_str!("../../rules/libchecker-baseline.json"),
    ),
    (
        "sdk-checks.json",
        include_str!("../../rules/sdk-checks.json"),
    ),
];

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RulePackSource {
    Bundled,
    Server,
    /// Installed from LibChecker/LibChecker-Rules via codeload tarball.
    Libchecker,
}

#[derive(Serialize, serde::Deserialize, Clone, Debug)]
pub struct InstalledRulePack {
    pub id: String,
    pub name: String,
    pub rule_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct RulePackStatus {
    pub installed: bool,
    pub path: Option<String>,
    pub packs: Vec<InstalledRulePack>,
    pub total_rules: usize,
    pub source: Option<RulePackSource>,
    pub download_url: Option<String>,
    /// LibChecker-Rules `cloud/md5/v4.version` — only set when `source == Libchecker`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub libchecker_version: Option<u32>,
    /// Short git SHA baked into pack descriptions. Best-effort, may be "unknown".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub libchecker_commit: Option<String>,
    /// Human-readable reason for an install/refresh fallback. Set only when
    /// the last attempted run couldn't deliver an upstream (Libchecker /
    /// server) pack and instead installed bundled content — the UI uses
    /// this to surface *why* the user only sees the bundled starter
    /// instead of the full Libchecker rule set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

/// Install (or refresh) the rule packs.
///
/// If `settings.rules_download_url` is set, attempt to download a zip from it
/// and extract `*.json` files into the rules directory. Falls back to bundled
/// content when the URL is missing, the download fails, or no JSON files are
/// produced.
pub async fn install_all(app: &AppHandle) -> AppResult<RulePackStatus> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let root = rules_root(&dir);
    tokio::fs::create_dir_all(&root).await?;

    let s = settings::read(&dir).await?;
    let url = s.rules_download_url.clone();

    // Track *why* we fell back to bundled so the UI can tell the user
    // that this only got them the starter set instead of full upstream
    // rules. `last_error` stays None for the happy path and the
    // "no URL configured" branch (that's a deliberate choice, not a
    // failure).
    let (source, last_error) = if let Some(url) = url.as_deref() {
        match download_and_extract(app, url, &root).await {
            Ok(n) if n > 0 => {
                let _ = app.emit(
                    "rules://installed",
                    serde_json::json!({ "source": "server", "files": n }),
                );
                (RulePackSource::Server, None)
            }
            Ok(_) => {
                // Download succeeded but extracted zero JSON files — fall back.
                install_bundled(&root).await?;
                (
                    RulePackSource::Bundled,
                    Some("server url downloaded an archive with no rule JSON files".into()),
                )
            }
            Err(e) => {
                // Network or zip error — fall back without surfacing an
                // error, since bundled content keeps the feature working
                // offline. We DO surface the failure reason in
                // status.last_error so the UI can show it on demand.
                install_bundled(&root).await?;
                (RulePackSource::Bundled, Some(format!("download failed: {e}")))
            }
        }
    } else {
        install_bundled(&root).await?;
        (RulePackSource::Bundled, None)
    };

    // Point rules_path at the directory so list_rules picks them up — unless
    // the user has explicitly set a custom path elsewhere.
    let mut next = s;
    let auto_path = root.to_string_lossy().to_string();
    if next.rules_path.is_none() {
        next.rules_path = Some(auto_path);
    }
    settings::write(&dir, &next).await?;
    let _ = app.emit("settings://changed", &next);

    let mut status = status_inner(&root).await;
    status.source = Some(source);
    status.download_url = next.rules_download_url;
    status.last_error = last_error;
    Ok(status)
}

pub async fn uninstall(app: &AppHandle) -> AppResult<()> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let root = rules_root(&dir);
    if root.exists() {
        tokio::fs::remove_dir_all(&root).await?;
    }
    let mut s = settings::read(&dir).await?;
    let auto_path = root.to_string_lossy().to_string();
    if s.rules_path.as_deref() == Some(auto_path.as_str()) {
        s.rules_path = None;
        settings::write(&dir, &s).await?;
        let _ = app.emit("settings://changed", &s);
    }
    Ok(())
}

pub async fn status(app: &AppHandle) -> AppResult<RulePackStatus> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let s = settings::read(&dir).await?;
    let mut st = status_inner(&rules_root(&dir)).await;
    st.source = None; // status() does not attempt install, so source is unknown.
    st.download_url = s.rules_download_url;
    Ok(st)
}

async fn install_bundled(root: &Path) -> AppResult<()> {
    for (filename, content) in BUNDLED {
        let dest = root.join(filename);
        tokio::fs::write(&dest, content).await?;
    }
    Ok(())
}

/// Download `url` into a temp file, then unzip `*.json` members into `dest_dir`.
/// Returns the number of JSON files extracted.
async fn download_and_extract(_app: &AppHandle, url: &str, dest_dir: &Path) -> AppResult<usize> {
    let tmp = std::env::temp_dir().join(format!("jadb-rules-{}.zip", uuid::Uuid::new_v4()));
    let client = reqwest::Client::new();
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::Config(format!("download failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Config(format!(
            "download returned HTTP {}",
            resp.status()
        )));
    }
    let mut file = tokio::fs::File::create(&tmp)
        .await
        .map_err(AppError::Io)?;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| AppError::Config(e.to_string()))?;
        file.write_all(&bytes).await.map_err(AppError::Io)?;
    }
    file.flush().await.map_err(AppError::Io)?;
    drop(file);

    let extracted = extract_json_from_zip(&tmp, dest_dir).await;
    let _ = tokio::fs::remove_file(&tmp).await;
    extracted
}

async fn extract_json_from_zip(zip_path: &Path, dest_dir: &Path) -> AppResult<usize> {
    let bytes = tokio::fs::read(zip_path).await.map_err(AppError::Io)?;
    let dest_dir = dest_dir.to_path_buf();
    tokio::task::spawn_blocking(move || -> AppResult<usize> {
        let reader = std::io::Cursor::new(bytes);
        let mut zip = zip::ZipArchive::new(reader).map_err(|e| AppError::Config(e.to_string()))?;
        let mut count = 0usize;
        for i in 0..zip.len() {
            let mut file = zip
                .by_index(i)
                .map_err(|e| AppError::Config(e.to_string()))?;
            if !file.name().to_lowercase().ends_with(".json") {
                continue;
            }
            let out = match file.enclosed_name() {
                Some(p) => dest_dir.join(p),
                None => continue,
            };
            if let Some(p) = out.parent() {
                std::fs::create_dir_all(p)?;
            }
            let mut out_file = std::fs::File::create(&out)?;
            std::io::copy(&mut file, &mut out_file)?;
            count += 1;
        }
        Ok(count)
    })
    .await
    .map_err(|e| AppError::Config(e.to_string()))?
}

async fn status_inner(root: &Path) -> RulePackStatus {
    let installed = root.exists();
    let mut packs = Vec::new();
    if installed {
        if let Ok(mut rd) = tokio::fs::read_dir(root).await {
            while let Ok(Some(entry)) = rd.next_entry().await {
                let p = entry.path();
                if p.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                if let Ok(bytes) = tokio::fs::read(&p).await {
                    if let Ok(set) =
                        serde_json::from_slice::<crate::services::rule_manager::RuleSet>(&bytes)
                    {
                        packs.push(InstalledRulePack {
                            id: set.id,
                            name: set.name,
                            rule_count: set.rules.len(),
                            version: set.version,
                        });
                    }
                }
            }
        }
    }
    let mut libchecker_version: Option<u32> = None;
    for p in &packs {
        if p.id.starts_with("libchecker.") {
            if let Some(v) = p.version.as_deref().and_then(|s| s.parse::<u32>().ok()) {
                libchecker_version = Some(v);
            }
            break;
        }
    }
    RulePackStatus {
        installed,
        path: if installed {
            Some(root.to_string_lossy().to_string())
        } else {
            None
        },
        total_rules: packs.iter().map(|p| p.rule_count).sum(),
        packs,
        source: None,
        download_url: None,
        libchecker_version,
        libchecker_commit: None,
        last_error: None,
    }
}

/// Public list of upstream branches we try in order when fetching the
/// LibChecker-Rules tarball. `master` is the historical long-lived
/// branch; if the upstream repo renames its default branch to `main`
/// (or archives `master`) the installer automatically rolls over so
/// the UI never silently keeps showing the bundled starter.
pub const LIBCHECKER_CODELOAD_BRANCHES: &[&str] = &["master", "main"];

/// Build a codeload URL for a given branch.
pub fn libchecker_codeload_url(branch: &str) -> String {
    format!(
        "https://codeload.github.com/LibChecker/LibChecker-Rules/tar.gz/refs/heads/{}",
        branch
    )
}

/// Backwards-compatible constant for the original single-branch URL.
pub const LIBCHECKER_CODELOAD_URL: &str =
    "https://codeload.github.com/LibChecker/LibChecker-Rules/tar.gz/refs/heads/master";

/// Build a `reqwest::Client` configured with reasonable timeouts. The
/// defaults ship with no timeouts, which means a stalled GitHub
/// connection (e.g. codeload.github.com is unreachable from China)
/// hangs the install for tens of seconds before falling back to
/// bundled. 10s connect / 10s read keeps slow networks from making
/// the UI feel broken.
fn libchecker_http_client() -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .user_agent(concat!("JADB/", env!("CARGO_PKG_VERSION"), " libchecker-installer"))
        .build()
        .map_err(|e| AppError::Config(format!("reqwest build: {e}")))
}

/// Stream the body of an in-flight `reqwest::Response` into `tmp` and
/// return the decoded gzipped bytes. Returns immediately on the first
/// I/O error so the caller can record the branch as failed and try the
/// next one.
async fn download_libchecker_body(
    resp: reqwest::Response,
    tmp: &Path,
) -> Result<Vec<u8>, String> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::File::create(tmp)
        .await
        .map_err(|e| format!("create tmp: {e}"))?;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let bytes = match chunk {
            Ok(b) => b,
            Err(_) => break,
        };
        if file.write_all(&bytes).await.is_err() {
            break;
        }
    }
    let _ = file.flush().await;
    drop(file);

    let gz_bytes = tokio::fs::read(tmp).await.map_err(|_| "read tmp".to_string())?;
    tokio::task::spawn_blocking(move || -> std::io::Result<Vec<u8>> {
        let mut decoder = flate2::read::GzDecoder::new(&gz_bytes[..]);
        let mut out = Vec::new();
        std::io::Read::read_to_end(&mut decoder, &mut out)?;
        Ok(out)
    })
    .await
    .map_err(|_| "gunzip join".to_string())?
    .map_err(|e| format!("gunzip: {e}"))
}

/// Strip leading `pax_global_header` (typeflag `g`) and `pax extended`
/// (typeflag `x`) entries from a tar byte slice. GitHub's codeload
/// prepends a `pax_global_header` entry whose body contains extra
/// records (`comment=…`, GNU sparse cksums, etc.) that the `tar` crate
/// refuses to decode as utf-8 — failing the whole iteration on
/// "numeric field did not have utf-8 text when getting cksum". Since
/// we read paths from the static `ustar` `name[100]` field via
/// `path_bytes()` and never consult PAX extension keys, we can safely
/// drop these synthetic entries without losing any real file data.
///
/// Each iteration: read the 512-byte header at the current cursor,
/// check `typeflag[156]`, and if it's `g` / `x`, skip `512 +
/// padded(content_size)` bytes. Stops at the first non-header entry
/// (typically `0x30` `'0'` for an old-style plain file or `0x35` for a
/// directory) or at two consecutive zero blocks (end of archive).
fn strip_pax_global_headers(bytes: &[u8]) -> Vec<u8> {
    fn is_zero(buf: &[u8]) -> bool {
        buf.iter().all(|&b| b == 0)
    }

    let mut offset = 0usize;
    loop {
        // Need at least a full header to even peek at it.
        if bytes.len() < offset + 512 {
            return bytes[offset..].to_vec();
        }
        let header = &bytes[offset..offset + 512];
        // Two consecutive zero blocks mean end of archive; return
        // whatever's left (shouldn't normally happen mid-prefix).
        if is_zero(header) {
            return bytes[offset..].to_vec();
        }
        let typeflag = header[156];
        if typeflag != b'g' && typeflag != b'x' {
            // Reached a real entry (file / dir / link / etc.).
            return bytes[offset..].to_vec();
        }
        // Parse size: 12-byte octal field at offset 124, NUL-padded.
        let size_field = &header[124..136];
        let size_str = match std::str::from_utf8(size_field) {
            Ok(s) => s.trim_end_matches('\0').trim(),
            Err(_) => "0",
        };
        let size = usize::from_str_radix(size_str, 8).unwrap_or(0);
        // Skip 512-byte header + size rounded up to the next 512.
        offset += 512 + ((size + 511) / 512) * 512;
    }
}

/// Install (or refresh) the LibChecker-Rules rules: download the codeload
/// tarball, extract into a temp staging directory, run the converter, and
/// write the 7 RuleSet JSONs into `dest`. `dest` is typically
/// `<app_data_dir>/rules/`. Old `libchecker.*.json` files are removed first
/// so the result is idempotent.
///
/// Pure function — no I/O outside the provided `bytes` and `dest`. Network
/// download lives in [`install_libchecker`] which delegates here.
pub fn install_libchecker_from_archive(
    bytes: &[u8],
    dest: &Path,
) -> AppResult<RulePackStatus> {
        let staging = std::env::temp_dir().join(format!("jadb-libchecker-staging-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&staging)?;

    // Stream the tarball from memory. tar::Archive needs Read + Seek, so we
    // wrap the byte slice in a Cursor. Pre-strip the leading
    // `pax_global_header` entries (GitHub codeload emits these on
    // every tarball) — see `strip_pax_global_headers` for why.
    let cleaned = strip_pax_global_headers(bytes);
    let cursor = std::io::Cursor::new(cleaned);
    let mut archive = tar::Archive::new(cursor);

    // We must NOT collect entries into a Vec first — tar::Entry's Read impl
    // shares the underlying reader's cursor, so reading entry[N] after we
    // advance past it returns 0 bytes. Iterate inline; lock in the shared
    // top-level dir from the first entry as we go.
    let raw_iter = archive
        .entries()
        .map_err(|e| AppError::Config(format!("tar entries: {e}")))?;

    let mut prefix: Option<std::ffi::OsString> = None;

    for entry_result in raw_iter {
        let mut entry = entry_result.map_err(|e| AppError::Config(format!("tar entry: {e}")))?;
        // IMPORTANT: use `path_bytes()` (raw ustar name[100]) instead of
        // `path()`. `path()` walks PAX extended headers, and GitHub
        // codeload's leading `pax_global_header` entry carries GNU-style
        // binary `cksum` / `sparse.offset` fields that the tar crate
        // refuses to decode as utf-8 text — causing
        //   numeric field did not have utf-8 text when getting cksum
        // for every subsequent entry. Reading the raw bytes sidesteps
        // PAX parsing entirely.
        // `path_bytes()` reads only the static ustar name[100] field, so
        // it can never trip PAX extension parsing. Returns a Cow that
        // may be borrowed from the underlying buffer or owned when the
        // name is longer than 100 bytes (in which case tar stores it
        // in a `ustar` extension block we still don't try to parse).
        let raw_bytes = entry.path_bytes();
        let raw_path = std::path::PathBuf::from(String::from_utf8_lossy(&raw_bytes).into_owned());

        // pax_global_header is a tar-side metadata record. Reading it as
        // a regular file consumes bytes from the underlying stream and
        // breaks subsequent entries. Skip entirely.
        if raw_path == std::path::PathBuf::from("pax_global_header")
            || raw_path
                .components()
                .next()
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                == Some("pax_global_header".to_string())
        {
            continue;
        }

        let first_component = raw_path
            .components()
            .next()
            .map(|c| c.as_os_str().to_owned());

        // GitHub codeload tarballs include a `pax_global_header` entry as
        // the very first record; skip it (and any other non-LibChecker
        // top-level dirs) when locking in the prefix.
        let is_pax = first_component
            .as_ref()
            .map(|c| c == "pax_global_header")
            .unwrap_or(false);
        if prefix.is_none() && !is_pax {
            prefix = first_component.clone();
        }

        let stripped = match (&prefix, &first_component) {
            (Some(p), Some(_)) => match raw_path.strip_prefix(p) {
                Ok(rest) => rest.to_path_buf(),
                Err(_) => continue,
            },
            (None, _) => raw_path.clone(),
            (Some(_), None) => continue,
        };
        if stripped.as_os_str().is_empty() {
            continue;
        }
        let out_path = staging.join(&stripped);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let entry_type = entry.header().entry_type();
        if entry_type.is_dir() {
            std::fs::create_dir_all(&out_path)?;
            continue;
        }
        let mut buf = Vec::new();
        use std::io::Read;
        if entry.read_to_end(&mut buf).is_err() {
            continue;
        }
        std::fs::write(&out_path, &buf)?;
    }

    // Extract the short SHA / branch name from the wrapper dir name when
    // present. Format is `LibChecker-Rules-<sha>` or `LibChecker-Rules-<branch>`.
    let commit_short = prefix
        .as_ref()
        .and_then(|p| p.to_str())
        .and_then(|s| s.strip_prefix("LibChecker-Rules-"))
        .unwrap_or("unknown")
        .to_string();

    let sets = libchecker_converter::convert_dir(&staging, &commit_short)
        .map_err(|e| AppError::Config(e.to_string()))?;
    if sets.is_empty() {
        eprintln!(
            "libchecker install: convert_dir returned 0 sets (commit={}, staging={:?});              upstream layout unrecognised, caller will fall back to bundled",
            commit_short, staging
        );
    }

    // Idempotency: wipe any previous libchecker.*.json before writing new ones.
    if dest.exists() {
        for entry in std::fs::read_dir(dest)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("libchecker.") && name.ends_with(".json") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    } else {
        std::fs::create_dir_all(dest)?;
    }

    let mut status = RulePackStatus {
        installed: true,
        path: Some(dest.to_string_lossy().to_string()),
        packs: Vec::new(),
        total_rules: 0,
        source: Some(RulePackSource::Libchecker),
        download_url: None,
        libchecker_version: None,
        libchecker_commit: if commit_short == "unknown" {
            None
        } else {
            Some(commit_short)
        },
        last_error: None,
    };
    for set in sets {
        let file = dest.join(format!("{}.json", set.id));
        let bytes = serde_json::to_vec_pretty(&set).map_err(|e| AppError::Config(e.to_string()))?;
        // Atomic-ish write: write to a sibling then rename.
        let tmp = file.with_extension("json.tmp");
        std::fs::write(&tmp, &bytes)?;
        std::fs::rename(&tmp, &file)?;
        status.packs.push(InstalledRulePack {
            id: set.id.clone(),
            name: set.name.clone(),
            rule_count: set.rules.len(),
            version: set.version.clone(),
        });
        status.total_rules += set.rules.len();
        if status.libchecker_version.is_none() {
            if let Some(v) = set.version.as_deref().and_then(|s| s.parse::<u32>().ok()) {
                status.libchecker_version = Some(v);
            }
        }
    }

    // Cleanup the staging tree; ignore errors so a stuck temp dir doesn't
    // abort the install.
    let _ = std::fs::remove_dir_all(&staging);

    Ok(status)
}
/// Async wrapper: fetch the LibChecker-Rules codeload tarball, gunzip it,
/// then hand the plain tar bytes to [`install_libchecker_from_archive`].
/// On any network or decompression failure — or when the upstream
/// checkout uses a directory layout the converter doesn't yet recognise
/// — falls back to bundled content and reports `RulePackSource::Bundled`
/// so the UI never breaks.
///
/// CRITICAL: every code path that materialises a rules directory on
/// disk MUST write `settings.rules_path` (when not already set) before
/// returning. Otherwise the Rules view reads `rulesPath not set` even
/// though `get_rule_pack_status` reports `installed=true`. To make this
/// hard to forget, the routine funnels every fallback through a single
/// convergence point at the bottom: install_bundled -> settings write ->
/// emit -> status.
pub async fn install_libchecker(app: &AppHandle) -> AppResult<RulePackStatus> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let root = rules_root(&dir);

    // Outcome of the install attempt. `Ok(status)` carries the real
    // Libchecker-produced packs; `Fallback(reason)` means we couldn't
    // get usable data and will install bundled content instead.
    enum Outcome {
        Ok(RulePackStatus),
        Fallback(String),
    }

    // We must download to a tmp file because gunzip needs to seek.
    let tmp = std::env::temp_dir().join(format!("jadb-libchecker-{}.tar.gz", uuid::Uuid::new_v4()));
    let client = libchecker_http_client()?;
    let outcome: Outcome = async {
        // Try each upstream branch in order. The exact failure of the
        // last branch becomes the user-facing reason; intermediate
        // 404s are kept as debug-only "skipped branch X" notes so we
        // don't mis-attribute a transient branch rename as a network
        // outage.
        let mut last_err: Option<String> = None;
        let mut tar_bytes: Option<Vec<u8>> = None;
        for branch in LIBCHECKER_CODELOAD_BRANCHES {
            let url = libchecker_codeload_url(branch);
            eprintln!("libchecker install: GET {url}");
            let resp = match client.get(&url).send().await {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("libchecker install: branch {branch} send failed: {e}");
                    last_err = Some(format!("{branch}: {e}"));
                    continue;
                }
            };
            if !resp.status().is_success() {
                eprintln!(
                    "libchecker install: branch {branch} HTTP {}",
                    resp.status()
                );
                last_err = Some(format!("{branch}: HTTP {}", resp.status()));
                continue;
            }
            match download_libchecker_body(resp, &tmp).await {
                Ok(bytes) => {
                    tar_bytes = Some(bytes);
                    last_err = None;
                    break;
                }
                Err(e) => {
                    eprintln!("libchecker install: branch {branch} body failed: {e}");
                    last_err = Some(format!("{branch}: {e}"));
                }
            }
        }
        let tar_bytes = match tar_bytes {
            Some(b) => b,
            None => {
                return Err::<Outcome, String>(last_err.unwrap_or_else(|| "no branch succeeded".into()));
            }
        };
        let _ = tokio::fs::remove_file(&tmp).await;

        let status = install_libchecker_from_archive(&tar_bytes, &root)
            .map_err(|e| format!("install archive: {e}"))?;
        if status.packs.is_empty() {
            // Tarball parsed but the converter produced no rule sets
            // (upstream checkout uses a directory layout we don't yet
            // recognise). Fall back to bundled rather than ship an
            // empty Libchecker install.
            return Err::<Outcome, String>("libchecker upstream layout unrecognised".into());
        }
        Ok::<Outcome, String>(Outcome::Ok(status))
    }
    .await
    .unwrap_or_else(Outcome::Fallback);

    let status: RulePackStatus = match outcome {
        Outcome::Ok(s) => {
            let _ = app.emit(
                "rules://installed",
                serde_json::json!({ "source": "libchecker", "files": s.packs.len() }),
            );
            s
        }
        Outcome::Fallback(reason) => {
            eprintln!("libchecker install: falling back to bundled ({reason})");
            install_bundled(&root).await?;
            let _ = app.emit(
                "rules://installed",
                serde_json::json!({ "source": "bundled", "reason": reason }),
            );
            // Mirror `status(app)` — that helper hydrates `download_url`
            // from settings and clears `source` (which the install
            // response deliberately leaves as the source the UI saw at
            // request-time, not the post-fallback one). The UI surfaces
            // "bundled" through the rules://installed event instead.
            let mut st = status(app).await?;
            st.last_error = Some(reason);
            st
        }
    };

    // Single convergence point: ensure settings.rules_path points at the
    // (now-populated) install root whenever the user hasn't already
    // picked a custom directory. Without this, `list_rules` returns
    // "rulesPath not set" and Rules view is empty even though
    // `installed=true`. Reads as a separate await to avoid borrowing
    // borrows.
    let mut current = settings::read(&dir).await?;
    let auto_path = root.to_string_lossy().to_string();
    if current.rules_path.is_none() {
        current.rules_path = Some(auto_path);
        settings::write(&dir, &current).await?;
        let _ = app.emit("settings://changed", &current);
    }

    Ok(status)
}
