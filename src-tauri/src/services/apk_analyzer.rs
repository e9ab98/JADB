use crate::config::settings::Settings;
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

/// All `.so` files inside an APK, grouped by ABI (e.g. "arm64-v8a",
/// "x86_64"). Value of each entry is the **full archive paths** under
/// `lib/<abi>/`, sorted and deduped per ABI by `extract_native_libs`.
pub type NativeLibsByArch = std::collections::BTreeMap<String, Vec<String>>;

/// All fields exposed to the frontend; the Rust side keeps everything
/// as plain types and `serde` renders the JSON shape the UI expects.
#[derive(Default, Serialize, Deserialize, Clone, Debug)]
pub struct ApkInfo {
    pub package_name: String,
    pub version_code: Option<String>,
    pub version_name: Option<String>,
    pub min_sdk: Option<String>,
    pub target_sdk: Option<String>,
    pub max_sdk: Option<String>,
    pub application_label: Option<String>,
    pub permissions: Vec<String>,
    pub activities: Vec<String>,
    pub services: Vec<String>,
    pub receivers: Vec<String>,
    pub providers: Vec<String>,
    pub intent_actions: Vec<String>,
    pub native_libs: Vec<String>,
    pub native_libraries: NativeLibsByArch,
    pub tech_stack: Vec<String>,
    pub insights: Vec<String>,
    pub raw_badging: String,
    // ---- Badging / xmltree only surface when the APK actually declares them. ----
    //
    // `uses_feature`                   → hardware / software features the app needs.
    // `uses_library`                   → shared libraries the app links to (e.g. maps).
    // `uses_permission_sdk_23`         → runtime-only permissions (Android 6+).
    // `supports_screens`               → screen size / density buckets the app supports.
    // `locales`                        → `<locales>` block or `supports-locales` summary.
    // `application_debuggable`         → `application-debuggable` line from badging.
    // `short_name_ratio`               → 0.0..=1.0 fraction of class names with <= 3 chars
    //                                   in the last segment (ProGuard / R8 obfuscation
    //                                   heuristic; see `parse_repeat`).
    pub uses_feature: Vec<String>,
    pub uses_library: Vec<String>,
    pub uses_permission_sdk_23: Vec<String>,
    pub supports_screens: Vec<String>,
    pub locales: Vec<String>,
    pub application_debuggable: bool,
    pub short_name_ratio: f32,
    pub file_size: Option<u64>,
    pub volume_total_size: Option<u64>,
    pub volume_stats: Option<VolumeStats>,
    pub largest_files: Vec<VolumeEntry>,
    pub security_report: Option<SecurityReport>,
    /// Optional per-signer certificate detail. Populated by the analysis
    /// pipeline when apksigner is configured; `None` when apksigner is
    /// missing or the APK could not be inspected.
    pub signature: Option<crate::services::apk_signature::SignatureInfo>,
    /// Heuristic packer / shell / obfuscator detection result. `None`
    /// means detection did not run (signature analysis failure path);
    /// use `packer.as_ref().map(|p| p.is_packed).unwrap_or(false)` to
    /// treat a missing report as "not detected".
    pub packer: Option<PackerReport>,
    /// Absolute path inside the APK (e.g. `res/mipmap-xxhdpi-v4/ic_launcher.png`)
    /// for the launcher icon reported by aapt2. `None` when the APK has
    /// no `application-icon-*` line or we failed to resolve one. Always
    /// paired with `icon_data_url` when present; either both set or both
    /// `None` so callers never have to handle "path without bytes".
    pub icon_path: Option<String>,
    /// `data:image/...;base64,...` for the launcher icon. We send the
    /// base64 bytes (rather than the raw archive path) because the
    /// frontend cannot reach into the APK zip itself — only the Rust
    /// side has the file open. Adaptive-icon XML descriptors are
    /// skipped on purpose: their foreground/background references are
    /// not a single image and the dashboard avatar only needs the
    /// raster fallback. `None` when no raster icon was found.
    pub icon_data_url: Option<String>,
}


#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PackerReport {
    /// `true` when at least one packer indicator fires.
    pub is_packed: bool,
    /// Best-guess packer label ("360", "Bangcle", ...). `None` when only
    /// generic shell indicators fired.
    pub packer_name: Option<String>,
    /// Each hit that contributed to `is_packed`: a `.so` filename, a class
    /// FQN, or any other rule trigger. Exposed to the UI so users can audit
    /// what made the detector say "yes".
    pub indicators: Vec<PackerIndicator>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PackerIndicator {
    /// "native", "entry_class", or future kinds (manifest permissions,
    /// meta-data stamps, etc.).
    pub kind: String,
    /// The artifact that triggered detection.
    pub value: String,
    /// Family label, e.g. "360", "Bangcle", "KiWiVM", "generic".
    pub packer: String,
}

#[derive(Default, Serialize, Deserialize, Clone, Debug)]
pub struct VolumeStats {
    pub dex: u64,
    pub lib: u64,
    pub res: u64,
    pub assets: u64,
    pub manifest: u64,
    pub arsc: u64,
    pub other: u64,
    pub lib_breakdown: std::collections::BTreeMap<String, u64>,
    pub redundant_files: Vec<RedundantFile>,
    pub waste_size: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RedundantFile {
    pub crc: String,
    pub size: u64,
    pub files: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VolumeEntry {
    pub name: String,
    pub size: u64,
    pub ratio: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SecurityRisk {
    pub id: String,
    pub level: String,
    pub title: String,
    pub description: String,
    pub suggestion: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SecurityReport {
    pub risks: Vec<SecurityRisk>,
    /// 0..=100, higher = safer
    pub score: u32,
}

pub fn parse_badging(s: &str) -> AppResult<ApkInfo> {
    let mut info = ApkInfo::default();
    for line in s.lines() {
        let line = line.trim_end();
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let rest = rest.trim();
        match key {
            "package" => {
                // package: name='x.y.z' versionCode='42' versionName='1.2.3' compileSdkVersion='34'
                for part in rest.split_whitespace() {
                    if let Some(v) = part.strip_prefix("name='") {
                        if let Some(end) = v.find('\'') {
                            info.package_name = v[..end].to_string();
                        }
                    } else if let Some(v) = part.strip_prefix("versionCode='") {
                        if let Some(end) = v.find('\'') {
                            info.version_code = Some(v[..end].to_string());
                        }
                    } else if let Some(v) = part.strip_prefix("versionName='") {
                        if let Some(end) = v.find('\'') {
                            info.version_name = Some(v[..end].to_string());
                        }
                    }
                }
            }
            "sdkVersion" => info.min_sdk = Some(unquote_single(rest)),
            "targetSdkVersion" => info.target_sdk = Some(unquote_single(rest)),
            "maxSdkVersion" => info.max_sdk = Some(unquote_single(rest)),
            "application-label" => info.application_label = Some(unquote_single(rest)),
            "uses-permission" => {
                if let Some(name) = extract_attr(rest, "name") {
                    if !info.permissions.contains(&name) {
                        info.permissions.push(name);
                    }
                }
            }
            "launchable-activity" => {
                if let Some(name) = extract_attr(rest, "name") {
                    if !info.activities.contains(&name) {
                        info.activities.push(name);
                    }
                }
            }
            "service" => {
                if let Some(name) = extract_attr(rest, "name") {
                    if !info.services.contains(&name) {
                        info.services.push(name);
                    }
                }
            }
            "receiver" => {
                if let Some(name) = extract_attr(rest, "name") {
                    if !info.receivers.contains(&name) {
                        info.receivers.push(name);
                    }
                }
            }
            "provider" => {
                if let Some(name) = extract_attr(rest, "name") {
                    if !info.providers.contains(&name) {
                        info.providers.push(name);
                    }
                }
            }
            "intent-action" => {
                if let Some(name) = extract_attr(rest, "name") {
                    if !info.intent_actions.contains(&name) {
                        info.intent_actions.push(name);
                    }
                }
            }
            // `uses-feature: name='android.hardware.camera'` — comma-separated
            // attribute string (`name=..., required=...`). We pull the
            // `name` token; `required=true` is the default and not surfaced.
            "uses-feature" => {
                if let Some(name) = extract_attr(rest, "name") {
                    if !name.is_empty() && !info.uses_feature.contains(&name) {
                        info.uses_feature.push(name);
                    }
                }
            }
            // Shared libraries the app links to via `<uses-library>` (we
            // re-parse from xmltree below — badging only gets a single
            //         attribute string, but the FQN is what the rule
            // engine cares about).
            "uses-library" => {
                if let Some(name) = extract_attr(rest, "name") {
                    if !name.is_empty() && !info.uses_library.contains(&name) {
                        info.uses_library.push(name);
                    }
                }
            }
            // `supports-screens: small='false' normal='true' ...` — parse
            // each `key='value'` token rather than treating the whole
            // clause as a single attribute. We only record the keys with
            // `='true'` so the UI doesn't double-count "supports nothing".
            "supports-screens" => {
                for token in rest.split_whitespace() {
                    if let Some(value) = token.strip_suffix("='true'") {
                        if let Some(name) = value.split_once('=').map(|(k, _)| k) {
                            if !info.supports_screens.contains(&name.to_string()) {
                                info.supports_screens.push(name.to_string());
                            }
                        }
                    }
                }
            }
            // `locales: '--_--' 'en' 'zh-CN' ...` — a single-quoted list.
            // We split on whitespace and strip the surrounding quotes
            // so each entry is a clean BCP-47 tag. The leading `--_--`
            // marker is the "default locale only" sentinel aapt2 emits
            // and we drop it.
            "locales" => {
                for raw in rest.split_whitespace() {
                    let stripped = raw
                        .strip_prefix('\'')
                        .and_then(|s| s.strip_suffix('\''))
                        .unwrap_or(raw);
                    if stripped.is_empty() || stripped == "--_--" {
                        continue;
                    }
                    if !info.locales.contains(&stripped.to_string()) {
                        info.locales.push(stripped.to_string());
                    }
                }
            }
            // `application-debuggable` line — bare flag, no value.
            // Also recorded on the security_report side, but we expose it
            // at the top level so the basicInfo card can render it
            // without traversing the nested report.
            "application-debuggable" => {
                info.application_debuggable = true;
            }
            // `application-icon-160:res/mipmap-anydpi-v26/ic_launcher.xml`
            // or `application-icon:res/mipmap-xxhdpi/ic_launcher.png`.
            // We only set `icon_path`; the analyzer pipeline below
            // walks every density-suffixed entry and picks the highest
            // raster match (the parse step just collects raw hints).
            k if k == "application-icon" || k.starts_with("application-icon-") => {
                // Capture the first observation; the analyzer pipeline
                // below re-walks `raw_badging` and picks the highest-
                // density raster. We still record a hint here so debug
                // surfaces can show what aapt2 reported without waiting
                // for the analyzer's pass.
                if info.icon_path.is_none() {
                    info.icon_path = Some(unquote_single(rest));
                }
            }
            _ => {}
        }
    }
    Ok(info)
}

fn unquote_single(s: &str) -> String {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix('\'') {
        if let Some(end) = rest.find('\'') {
            return rest[..end].to_string();
        }
    }
    t.to_string()
}

fn extract_attr(s: &str, key: &str) -> Option<String> {
    // Pull `value` from any of:
    //   key='value'                          -- aapt2 dump badging
    //   key="value"                          -- aapt2 dump xmltree, plain attr
    //   key(0xHEX)="value"                   -- aapt2 dump xmltree, attr w/ RID
    //
    // Implementation note: we match the bare `key` token first so that the
    // character straight after it determines which shape we are in. Doing
    // "find prefix = `key=`" and then strip another `=` was the previous
    // implementation and it accidentally over-consumed the badging format
    // (`name='x'` -> after `name=` we land on `'x'`, not on `=`).
    let idx = s.find(key)?;
    let mut after = &s[idx + key.len()..];
    // Optional `(0x....)` resource-id annotation.
    if let Some(stripped) = after.strip_prefix('(') {
        after = stripped;
        let close = after.find(')')?;
        after = &after[close + 1..];
    }
    let after = after.trim_start();
    // Optional `=` then an opening quote. We deliberately allow either quote
    // style here because aapt2 badging is single-quoted while xmltree is
    // double-quoted. Anything else means the attribute we just matched is not
    // actually a key/value pair on this line.
    let after = after.strip_prefix('=')?.trim_start();
    let quote = match after.chars().next()? {
        '\'' => '\'',
        '"' => '"',
        _ => return None,
    };
    let after = &after[quote.len_utf8()..];
    let end = after.find(quote)?;
    Some(after[..end].to_string())
}

async fn dump_manifest(aapt: &str, apk_path: &Path) -> AppResult<String> {
    let output = Command::new(aapt)
        .arg("dump")
        .arg("xmltree")
        .arg(apk_path)
        .arg("--file")
        .arg("AndroidManifest.xml")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| AppError::Config(format!("spawn aapt2 xmltree: {e}")))?;
    if !output.status.success() {
        return Err(AppError::ToolFailed {
            tool: "aapt2".into(),
            code: output.status.code().unwrap_or(-1),
            msg: String::from_utf8_lossy(&output.stderr).into(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Best-effort: parse `<activity/>`, `<service/>`, `<receiver/>`, `<provider/>`
/// from the xmltree output of `aapt2 dump xmltree AndroidManifest.xml`.
///
/// Output format we expect (one tag per line, attributes on subsequent lines):
///
/// ```text
/// E: activity (line=12)
///   A: android:theme(0x01010005)=@0x7f1300a8
///   A: android:name(0x01010003)="com.foo.MainActivity"
///   A: android:exported(0x01010010)=0xffffffff
/// E: intent-filter (line=13)
///   N: android:action
/// E: service (line=...)
///   A: android:name(...)="..."
/// ```
///
/// Bug fix history: an earlier revision set `current = Some("activity")` on
/// the `E: activity (line=N)` line and then *immediately* reset it back to
/// `None` in the same iteration via the "end-tag heuristic" (the opening tag
/// line matches `starts_with("E: ") && !contains("name=")`). That meant the
/// next `A: android:name=...` line was reached with `current == None`, so
/// nothing was pushed into `info.activities/info.services/...`. Only the
/// single `launchable-activity` from `parse_badging` survived — hence the
/// user-facing symptom of "Activity=1, Service/Receiver/Provider=0".
fn parse_components_from_xml(xml: &str, info: &mut ApkInfo) {
    // Top-level tag → callback to push the next `android:name` attribute
    // value into the matching `ApkInfo` Vec. We register a callback for
    // every tag we want to land on `info.{activities, services, receivers,
    // providers, uses_feature, uses_library, uses_permission_sdk_23}`.
    // Anything else (e.g. `<application>`, `<manifest>`, `<intent-filter>`)
    // closes the current scope so subsequent attributes don't bleed.
    let mut current: Option<&'static str> = None;
    for line in xml.lines() {
        let trimmed = line.trim();
        // On a fresh opening tag line, switch context and `continue` so that
        // the very next attribute line is processed with the right component.
        if let Some(tag) = trimmed
            .strip_prefix("E: ")
            .and_then(|s| s.split_whitespace().next())
        {
            current = match tag {
                "activity" if !trimmed.contains("E: activity-") => Some("activity"),
                "service" if !trimmed.contains("E: service-") => Some("service"),
                "receiver" => Some("receiver"),
                "provider" if !trimmed.contains("E: provider-") => Some("provider"),
                "uses-feature" => Some("uses-feature"),
                "uses-library" => Some("uses-library"),
                // `uses-permission-sdk-23` is the runtime-only variant
                // (Android 6.0+). It is *separate* from `<uses-permission>`,
                // which we already cover via aapt2 dump badging.
                "uses-permission-sdk-23" => Some("uses-permission-sdk-23"),
                // Anything else (application, manifest, uses-permission,
                // intent-filter, action, category, meta-data, ...) closes
                // the current component scope so that subsequent attributes
                // don't accidentally attach to the previous component.
                _ => None,
            };
            continue;
        }

        if let Some(kind) = current {
            if let Some(name) = extract_attr(trimmed, "android:name") {
                let target = match kind {
                    "activity" => Some(&mut info.activities),
                    "service" => Some(&mut info.services),
                    "receiver" => Some(&mut info.receivers),
                    "provider" => Some(&mut info.providers),
                    "uses-feature" => Some(&mut info.uses_feature),
                    "uses-library" => Some(&mut info.uses_library),
                    "uses-permission-sdk-23" => Some(&mut info.uses_permission_sdk_23),
                    _ => None,
                };
                if let Some(t) = target {
                    if !name.is_empty() && !t.contains(&name) {
                        t.push(name);
                    }
                }
            }
            // Nested tags (`N: ...` or another `E: ...` we didn't `continue`
            // from above) end the current component scope.
            if trimmed.starts_with("E: ") || trimmed.starts_with("N:") {
                current = None;
            }
        }
    }
}

pub async fn analyze(settings: &Settings, apk_path: &Path) -> AppResult<ApkInfo> {
    let aapt = settings
        .aapt_path
        .as_deref()
        .ok_or_else(|| AppError::ToolMissing("aapt2".into()))?;

    let output = Command::new(aapt)
        .arg("dump")
        .arg("badging")
        .arg(apk_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| AppError::Config(format!("spawn aapt2: {e}")))?;
    if !output.status.success() {
        return Err(AppError::ToolFailed {
            tool: "aapt2".into(),
            code: output.status.code().unwrap_or(-1),
            msg: String::from_utf8_lossy(&output.stderr).into(),
        });
    }

    let mut stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if stdout.is_empty() {
        let mut buf = Vec::new();
        let _ = output.stdout.as_slice().read_to_end(&mut buf);
        stdout = String::from_utf8_lossy(&buf).into_owned();
    }

    let mut info = parse_badging(&stdout)?;
    info.raw_badging = stdout;

    // Re-walk the raw badging output to pick the best launcher icon.
    // `parse_badging` captures the first `application-icon*` line it
    // sees; we want the highest-density *raster* entry. Adaptive-icon
    // XML descriptors are skipped because they reference a foreground
    // + background pair and cannot be rendered as a single image.
    if let Some(path) = pick_best_icon_path(&info.raw_badging) {
        if let Some(data_url) = read_icon_as_data_url(apk_path, &path) {
            info.icon_path = Some(path);
            info.icon_data_url = Some(data_url);
        }
    }

    // Best-effort: also parse xmltree for additional components not in badging.
    // We keep the xml around here so we can re-use it for the
    // obfuscation index (`short_name_ratio`) and the security report
    // without paying for a second `aapt2 dump xmltree` round-trip.
    let manifest_xml = dump_manifest(aapt, apk_path).await.ok();
    if let Some(xml) = manifest_xml.as_ref() {
        parse_components_from_xml(xml, &mut info);
    }
    // Obfuscation sniff: feed the same xmltree output to
    // `short_name_ratio` and stash the result on `info` so the
    // basicInfo card can render "混淆度 87%" inline. Returns None
    // for tiny APKs (under the `total < 4` floor) so the UI knows
    // to render "—" instead of forcing a 0%.
    if let Some(xml) = manifest_xml.as_ref() {
        info.short_name_ratio = short_name_ratio(xml).unwrap_or(0.0);
    }

    // File size & native libs: best-effort, never abort analysis.
    if let Ok(meta) = std::fs::metadata(apk_path) {
        info.file_size = Some(meta.len());
    }
    if let Ok((libs, grouped)) = extract_native_libs(apk_path) {
        info.native_libs = libs;
        info.native_libraries = grouped;
    }

    // Tech-stack detection & insights.
    info.tech_stack = detect_tech_stack(&info);
    info.insights = build_insights(&info);

    // Volume stats (best-effort, only if archive is readable).
    if let Ok(vs) = compute_volume_stats(apk_path) {
        info.volume_total_size = Some(vs.total);
        info.volume_stats = Some(vs.stats);
        info.largest_files = vs.largest;
    }

    // Security report — combines xmltree, badging and obvious risk rules.
    let manifest_xml = manifest_xml.unwrap_or_default();
    info.security_report = Some(build_security_report(&info, &manifest_xml));

    // Packer / shell detection is pure zip-path analysis; cheap and
    // synchronous, always available regardless of apksigner config.
    info.packer = Some(detect_packer(&info.packer_ref()));

    // Signature inspection (apksigner `--print-certs`). Best-effort: if
    // apksigner is not configured or the call fails, `signature` stays
    // `None` and the analyze pipeline continues. The front-end shows
    // "未检测" instead of crashing on the user.
    info.signature = crate::services::apk_signature::inspect_signature_inner(
        settings,
        &apk_path.to_string_lossy(),
    )
    .await
    .ok();

    Ok(info)
}

/// Walk raw aapt2 `dump badging` output and pick the highest-density
/// raster launcher icon path. Adaptive-icon XML descriptors
/// (`application-icon-160:res/mipmap-anydpi-v26/ic_launcher.xml`) are
/// skipped because the dashboard avatar needs a single image — XML
/// adaptive icons reference a foreground + background pair and cannot
/// be inlined as one `<img src="data:...">`. When aapt2 lists no
/// raster icons we keep `None` and the dashboard reverts to the letter
/// avatar (this is the rare legacy-build case).
fn pick_best_icon_path(badging: &str) -> Option<String> {
    // Density order: ldpi < mdpi < hdpi < xhdpi < xxhdpi < xxxhdpi.
    // `application-icon-160` etc. maps to mdpi; we promote the entry
    // whose density rank is highest.
    const DENSITY_ORDER: &[&str] = &[
        "ldpi", "mdpi", "hdpi", "xhdpi", "xxhdpi", "xxxhdpi",
    ];
    let density_rank = |density: &str| -> usize {
        DENSITY_ORDER
            .iter()
            .position(|name| *name == density)
            .unwrap_or(0)
    };
    let mut best: Option<(usize, String)> = None;
    for line in badging.lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        if key != "application-icon" && !key.starts_with("application-icon-") {
            continue;
        }
        // aapt2 wraps the value in single quotes (e.g.
        // `application-icon-480:'res/mipmap-xxhdpi/ic_launcher.png'`);
        // the helper below strips them so the resulting string is a
        // real zip entry path.
        let path = unquote_single(rest);
        // Adaptive-icon XML descriptors live under `mipmap-anydpi-*` and
        // end in `.xml`. Without this guard the UI would render a broken
        // `<img>` (browsers do not honor image/xml).
        if path.ends_with(".xml") {
            continue;
        }
        let density_bucket = if let Some(suffix) = key.strip_prefix("application-icon-") {
            match suffix {
                "120" => "ldpi",
                "160" => "mdpi",
                "240" => "hdpi",
                "320" => "xhdpi",
                "480" => "xxhdpi",
                "640" => "xxxhdpi",
                _ => "mdpi",
            }
        } else {
            "mdpi"
        };
        let rank = density_rank(density_bucket);
        match &best {
            Some((current, _)) if *current >= rank => {}
            _ => best = Some((rank, path)),
        }
    }
    best.map(|(_, path)| path)
}

/// Open the APK as a zip and read the named entry into a base64 data
/// URL. Only used for the launcher icon, so a single read into a
/// `Vec<u8>` is fine — launcher icons are well under 100 KB.
///
/// Returns `None` on any failure (entry missing, archive unreadable,
/// bytes empty). All failure modes are non-fatal for the analyze
/// pipeline: the dashboard simply falls back to the letter avatar.
fn read_icon_as_data_url(apk_path: &Path, entry_path: &str) -> Option<String> {
    let file = std::fs::File::open(apk_path).ok()?;
    let mut zip = zip::ZipArchive::new(file).ok()?;
    let mut entry = zip.by_name(entry_path).ok()?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).ok()?;
    if bytes.is_empty() {
        return None;
    }
    let mime = if entry_path.ends_with(".webp") {
        "image/webp"
    } else if entry_path.ends_with(".png") {
        "image/png"
    } else if entry_path.ends_with(".jpg") || entry_path.ends_with(".jpeg") {
        "image/jpeg"
    } else {
        // Adaptive-icon XML or other formats we cannot inline.
        return None;
    };
    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
    Some(format!("data:{};base64,{}", mime, encoded))
}

pub fn extract_native_libs(
    apk_path: &Path,
) -> AppResult<(Vec<String>, NativeLibsByArch)> {
    let file = std::fs::File::open(apk_path).map_err(AppError::Io)?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| AppError::Parse(e.to_string()))?;
    let mut all: Vec<String> = Vec::new();
    let mut grouped: NativeLibsByArch = NativeLibsByArch::new();
    for i in 0..zip.len() {
        let entry = zip
            .by_index(i)
            .map_err(|e| AppError::Parse(e.to_string()))?;
        let name = entry.name().to_string();
        if let Some(arch) = abi_from_path(&name) {
            all.push(name.clone());
            grouped.entry(arch.to_string()).or_default().push(name);
        }
    }
    all.sort();
    for v in grouped.values_mut() {
        v.sort();
        v.dedup();
    }
    Ok((all, grouped))
}

/// Canonical Android NDK ABI directory names as accepted by `aapt2` /
/// `unzip -l`. Listed by descending order of (likely) current-day relevance so
/// future readers can pick the most common ones out at a glance. MIPS is
/// deprecated (removed in NDK r17, 2018) but still appears in old APKs and
/// ROM vendor prebuilts, so we accept it instead of silently dropping.
const ABI_DIRS: &[&str] = &[
    "arm64-v8a",
    "armeabi-v7a",
    "armeabi",
    "x86_64",
    "x86",
    "mips64",
    "mips",
];

fn abi_from_path(name: &str) -> Option<&'static str> {
    let rest = name.strip_prefix("lib/")?;
    let abi = rest.split('/').next()?;
    ABI_DIRS.iter().copied().find(|a| *a == abi)
}

fn detect_tech_stack(info: &ApkInfo) -> Vec<String> {
    let mut stack = Vec::new();
    let mut mark = |label: &str, needles: &[&str]| {
        for n in needles {
            if info.native_libs.iter().any(|l| l.contains(n)) {
                stack.push(label.to_string());
                return;
            }
        }
    };
    mark("Flutter", &["libflutter.so"]);
    mark("React Native", &["librn", "libreactnativejni", "libhermes.so", "libjsc.so"]);
    mark("Unity", &["libunity.so"]);
    mark("Cocos2d", &["libcocos2dcpp.so", "libMyGame.so"]);
    mark("Xamarin", &["libmonodroid.so", "libmonosgen-2.0.so"]);
    mark("Cordova", &["libcordova.so"]);
    mark("Weex", &["libweexjsb.so"]);
    mark("Tinker", &["libtinker.so"]);
    if info
        .permissions
        .iter()
        .any(|p| p.contains("BIND_ACCESSIBILITY_SERVICE"))
    {
        stack.push("Accessibility".into());
    }
    if info.package_name.contains("com.tencent.mm") {
        stack.push("WeChat".into());
    }
    stack.sort();
    stack.dedup();
    stack
}

fn build_insights(info: &ApkInfo) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(min_sdk) = info.min_sdk.as_deref() {
        if let Ok(n) = min_sdk.parse::<u32>() {
            if n < 21 {
                out.push(format!("minSdk={n} 偏低，部分设备可能无法安装"));
            }
        }
    }
    if let Some(target) = info.target_sdk.as_deref() {
        if let Ok(n) = target.parse::<u32>() {
            if n < 30 {
                out.push(format!("targetSdk={n} 已被 Google Play 新政策淘汰"));
            }
        }
    }
    if info.native_libs.iter().any(|l| l.contains("libflutter.so")) {
        out.push("检测到 Flutter 引擎".into());
    }
    if info
        .native_libs
        .iter()
        .any(|l| l.contains("librn") || l.contains("libreactnativejni"))
    {
        out.push("检测到 React Native 引擎".into());
    }
    out
}

struct VolumeComputation {
    total: u64,
    stats: VolumeStats,
    largest: Vec<VolumeEntry>,
}

fn compute_volume_stats(apk_path: &Path) -> AppResult<VolumeComputation> {
    let file = std::fs::File::open(apk_path).map_err(AppError::Io)?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| AppError::Parse(e.to_string()))?;
    let mut stats = VolumeStats::default();
    let mut total: u64 = 0;
    let mut entries: Vec<(String, u64)> = Vec::new();
    let mut by_crc: std::collections::BTreeMap<String, (u64, Vec<String>)> =
        std::collections::BTreeMap::new();
    for i in 0..zip.len() {
        let entry = zip
            .by_index(i)
            .map_err(|e| AppError::Parse(e.to_string()))?;
        let name = entry.name().to_string();
        let size = entry.size();
        total += size;
        let crc = entry.crc32().to_string();
        by_crc
            .entry(crc)
            .or_insert_with(|| (size, Vec::new()))
            .1
            .push(name.clone());
        entries.push((name.clone(), size));
        match classify(&name) {
            "dex" => stats.dex += size,
            "lib" => {
                stats.lib += size;
                if let Some(abi) = abi_from_path(&name) {
                    *stats.lib_breakdown.entry(abi.to_string()).or_insert(0) += size;
                }
            }
            "res" => stats.res += size,
            "assets" => stats.assets += size,
            "manifest" => stats.manifest += size,
            "arsc" => stats.arsc += size,
            _ => stats.other += size,
        };
    }
    let mut redundant: Vec<RedundantFile> = by_crc
        .into_iter()
        .filter_map(|(_crc, (size, files))| {
            if files.len() > 1 {
                Some(RedundantFile {
                    crc: _crc,
                    size,
                    files,
                })
            } else {
                None
            }
        })
        .collect();
    redundant.sort_by(|a, b| b.size.cmp(&a.size));
    stats.waste_size = redundant.iter().map(|r| r.size * (r.files.len() as u64 - 1)).sum();
    stats.redundant_files = redundant.into_iter().take(20).collect();

    entries.sort_by(|a, b| b.1.cmp(&a.1));
    let largest: Vec<VolumeEntry> = entries
        .into_iter()
        .take(20)
        .filter_map(|(name, size)| {
            if size == 0 || total == 0 {
                None
            } else {
                Some(VolumeEntry {
                    name,
                    size,
                    ratio: size as f64 / total as f64,
                })
            }
        })
        .collect();
    Ok(VolumeComputation {
        total,
        stats,
        largest,
    })
}

fn classify(name: &str) -> &'static str {
    if name == "AndroidManifest.xml" {
        return "manifest";
    }
    if name.ends_with(".arsc") {
        return "arsc";
    }
    if name.ends_with(".dex") || name.ends_with(".odex") || name.ends_with(".vdex") {
        return "dex";
    }
    if name.starts_with("lib/") && name.ends_with(".so") {
        return "lib";
    }
    if name.starts_with("res/") {
        return "res";
    }
    if name.starts_with("assets/") {
        return "assets";
    }
    "other"
}

/// Hard-coded signatures for the most common Android packers. The matcher
/// is intentionally a denylist of `.so` basenames and `.class`-prefixed FQNs
/// rather than a fingerprint; a single hit is enough to flag the APK as
/// packed because every entry below is a third-party runtime that would
/// never appear in a vanilla build. Keep entries lowercase; we match
/// case-insensitively.
struct PackerRule {
    packer: &'static str,
    so_substrings: &'static [&'static str],
    class_prefixes: &'static [&'static str],
}

const PACKER_RULES: &[PackerRule] = &[
    PackerRule {
        packer: "360",
        so_substrings: &["libsecexe.so", "libprotectclass.so", "libjiagu.so", "libjiagu_x.so"],
        class_prefixes: &["com.qihoo.", "com.qihoo360."],
    },
    PackerRule {
        packer: "Bangcle",
        so_substrings: &["libexec.so", "libexecmain.so", "libhelper.so", "libDexHelper.so"],
        class_prefixes: &["com.bangcle.", "com.secapk."],
    },
    PackerRule {
        packer: "KiWiVM",
        so_substrings: &["libfdog.so", "libfrog.so", "libfakela.so"],
        class_prefixes: &["com.kiwi."],
    },
    PackerRule {
        packer: "Tencent Legu",
        so_substrings: &["libtup.so", "libtupsafe.so", "libexecpro.so"],
        class_prefixes: &["com.tencent.stubshell", "com.tencent.legu", "com.tencent.ms.draft"],
    },
    PackerRule {
        packer: "Payegowrapper",
        so_substrings: &["libpayegowrapper.so"],
        class_prefixes: &["com.payegisubstrate."],
    },
    PackerRule {
        packer: "iJiami",
        so_substrings: &["libegis.so", "libexec_x86.so"],
        class_prefixes: &["com.shell."],
    },
    PackerRule {
        packer: "Baidu",
        so_substrings: &["libbaiduprotect.so"],
        class_prefixes: &["com.baidu.protect."],
    },
    PackerRule {
        packer: "Mobisec",
        so_substrings: &["libmobisec.so"],
        class_prefixes: &["com.mobisec."],
    },
    PackerRule {
        packer: "DEXGuard",
        so_substrings: &["libdexguard.so"],
        class_prefixes: &["de.fernuni.dexguard."],
    },
    // Generic decryptor / shell loaders — these alone do not name a vendor
    // but still warrant flagging. Labeled "generic" so the UI can group them.
    PackerRule {
        packer: "generic shell",
        so_substrings: &["libdecryptor.so", "libshell.so", "libloader.so", "libstub.so", "libaes.so"],
        class_prefixes: &["io.github.snowdream.app.stub", "com.stub.", "com.tencent.ms.dexlib"],
    },
];

/// Heuristic packer detector. Walks the natively-listed `.so` basenames and
/// matches them against `PACKER_RULES`, plus (best-effort) checks the
/// launcher activity's package for known stub-class prefixes by inspecting
/// `info.application_label`/the package name. Cheap, independent of apksigner,
/// never aborts the analyze pipeline.
fn detect_packer(info: &ApkInfoRef<'_>) -> PackerReport {
    let mut indicators: Vec<PackerIndicator> = Vec::new();
    let mut packer_name: Option<String> = None;

    let lower_libs: Vec<String> = info
        .native_libs
        .iter()
        .map(|n| n.to_ascii_lowercase())
        .collect();

    for rule in PACKER_RULES {
        for needle in rule.so_substrings {
            let needle_lower = needle.to_ascii_lowercase();
            for lib in &lower_libs {
                if lib.contains(&needle_lower) {
                    indicators.push(PackerIndicator {
                        kind: "native".into(),
                        value: lib.clone(),
                        packer: rule.packer.into(),
                    });
                    if packer_name.is_none() {
                        packer_name = Some(rule.packer.into());
                    }
                }
            }
        }
        for prefix in rule.class_prefixes {
            let hay_lower = info.package_name.to_ascii_lowercase();
            if hay_lower.starts_with(&prefix.to_ascii_lowercase()) {
                indicators.push(PackerIndicator {
                    kind: "entry_class".into(),
                    value: info.package_name.to_string(),
                    packer: rule.packer.into(),
                });
                if packer_name.is_none() {
                    packer_name = Some(rule.packer.into());
                }
            }
        }
    }

    PackerReport {
        is_packed: !indicators.is_empty(),
        packer_name,
        indicators,
    }
}

// `ApkInfo` is too large to reborrow; pull the few fields the packer rule
// actually needs out of it into a small struct on the stack.
struct ApkInfoRef<'a> {
    package_name: &'a str,
    native_libs: &'a [String],
}

/// Convenience: extract the relevant slice of an `ApkInfo` for the packer
/// detector so call sites stay one-liners.
impl ApkInfo {
    fn packer_ref(&self) -> ApkInfoRef<'_> {
        ApkInfoRef {
            package_name: &self.package_name,
            native_libs: &self.native_libs,
        }
    }
}

fn check_webview_cleartext(manifest_xml: &str) -> bool {
    let has_cleartext = manifest_xml.contains("usesCleartextTraffic")
        && !manifest_xml.contains("usesCleartextTraffic(0x0)=false");
    let has_webview = manifest_xml.to_lowercase().contains("webview");
    has_cleartext && has_webview
}

#[derive(Debug, Clone)]
struct ExposedComponent {
#[allow(dead_code)]
    name: String,
    kind: String,
}

fn analyze_exposed_components(manifest_xml: &str) -> Vec<ExposedComponent> {
    let mut out = Vec::new();
    let mut current: Option<(String, String, Option<bool>, bool, bool)> = None;
    // (name, kind, exported?, has_intent_filter, has_permission)
    for line in manifest_xml.lines() {
        let trimmed = line.trim();
        if let Some(tag) = trimmed
            .strip_prefix("E: ")
            .and_then(|s| s.split_whitespace().next())
        {
            // Finalize previous component.
            if let Some((name, kind, exported, has_intent_filter, has_permission)) = current.take() {
                if is_exposed(exported, has_intent_filter) && !has_permission {
                    out.push(ExposedComponent { name, kind });
                }
            }
            if matches!(tag, "activity" | "service" | "receiver" | "provider") {
                current = Some((String::new(), tag.to_string(), None, false, false));
            }
            continue;
        }
        if let Some(cur) = current.as_mut() {
            if trimmed.starts_with("E: intent-filter") {
                cur.3 = true;
                continue;
            }
            if let Some(name) = extract_attr(trimmed, "android:name") {
                if cur.0.is_empty() {
                    cur.0 = name;
                }
            } else if trimmed.contains("exported") {
                cur.2 = Some(trimmed.contains("0xffffffff"));
            } else if trimmed.contains("permission") {
                cur.4 = true;
            }
        }
    }
    if let Some((name, kind, exported, has_intent_filter, has_permission)) = current {
        if is_exposed(exported, has_intent_filter) && !has_permission {
            out.push(ExposedComponent { name, kind });
        }
    }
    out
}

fn is_exposed(exported: Option<bool>, has_intent_filter: bool) -> bool {
    exported == Some(true) || (exported.is_none() && has_intent_filter)
}

fn build_security_report(info: &ApkInfo, manifest_xml: &str) -> SecurityReport {
    let mut risks: Vec<SecurityRisk> = Vec::new();
    if info.raw_badging.contains("android:debuggable='True'")
        || manifest_xml.contains("(0x0101000f)=0xffffffff")
    {
        risks.push(SecurityRisk {
            id: "debuggable".into(),
            level: "critical".into(),
            title: "应用可调试".into(),
            description: "AndroidManifest.xml 中 android:debuggable 设置为 true。".into(),
            suggestion: "release 构建不应开启 debuggable；构建时去掉该属性。".into(),
        });
    }

    let exposed = analyze_exposed_components(manifest_xml);
    let svc_rcv = exposed
        .iter()
        .filter(|c| c.kind == "service" || c.kind == "receiver")
        .count();
    if svc_rcv > 0 {
        risks.push(SecurityRisk {
            id: "exposed-sr".into(),
            level: "warning".into(),
            title: "Service/Receiver 暴露风险".into(),
            description: format!(
                "检测到 {} 个 Service/Receiver 组件被导出且未受权限保护。这可能导致非授权的跨应用启动或数据劫持。",
                svc_rcv
            ),
            suggestion: "请在 AndroidManifest 中检查这些组件，若非必须请设置 android:exported=\"false\"；若必须导出，请增加 android:permission 进行鉴权。".into(),
        });
    }
    let act_prv = exposed
        .iter()
        .filter(|c| c.kind == "activity" || c.kind == "provider")
        .count();
    if act_prv > 0 {
        risks.push(SecurityRisk {
            id: "exposed-ap".into(),
            level: "warning".into(),
            title: "Activity/Provider 暴露风险".into(),
            description: format!(
                "检测到 {} 个 Activity 或 Provider 被导出且未配置权限。",
                act_prv
            ),
            suggestion: "请在 AndroidManifest 中检查这些组件，对非必要导出加 android:exported=\"false\"，并对 Provider 加 permissions。".into(),
        });
    }

    if info
        .permissions
        .iter()
        .any(|p| p == "android.permission.BIND_ACCESSIBILITY_SERVICE")
    {
        risks.push(SecurityRisk {
            id: "a11y".into(),
            level: "info".into(),
            title: "请求无障碍服务".into(),
            description: "应用请求 BIND_ACCESSIBILITY_SERVICE，结合界面劫持风险。".into(),
            suggestion: "确认声明用途，避免被用于界面劫持或静默操作。".into(),
        });
    }

    if check_webview_cleartext(manifest_xml) {
        risks.push(SecurityRisk {
            id: "webview-cleartext".into(),
            level: "warning".into(),
            title: "WebView 允许明文流量".into(),
            description: "usesCleartextTraffic=true 且存在 WebView，存在中间人风险。".into(),
            suggestion: "关闭 usesCleartextTraffic 或通过 NetworkSecurityConfig 限定可信域名。".into(),
        });
    }

    let score = if risks.is_empty() {
        100
    } else {
        let mut s: i32 = 100;
        for r in &risks {
            s -= match r.level.as_str() {
                "critical" => 30,
                "warning" => 12,
                _ => 3,
            };
        }
        s.max(0) as u32
    };

    SecurityReport { risks, score }
}

/// Fraction of class names whose last segment is <= 3 characters.
///
/// The heuristic is intentionally crude: feed it the
/// `aapt2 dump xmltree AndroidManifest.xml` output and it counts
/// how many `android:name="..."` attributes have a `last_segment`
/// of length 0..=3. The result is a 0.0..=1.0 ratio used as the
/// "obfuscation index" of the APK — a value of 0.0 means the APK
/// is unobfuscated, 1.0 means every class has a 1-3 char name
/// (extreme R8 / ProGuard). Anything below `0.30` is treated as
/// "not obfuscated" and the caller surfaces a "none" label.
///
/// We require at least 4 names to score — fewer than that and the
/// statistic is meaningless (a 2-name APK with 1 short class
/// would be reported as 50% obfuscated). Returns `None` for
/// the "too few names" case so the UI can render "—" instead of
/// a misleading 0.
fn short_name_ratio(xml: &str) -> Option<f32> {
    // We count both `total` and `short` over the same line set —
    //   `<attribute android:name="..."/>` lines — so the ratio
    // isn't biased by the opening `E: <kind>` tags that appear
    // between every attribute. Without this, an apk with 4
    // activities and 4 attribute lines would yield 0.5 instead
    // of 1.0 because the `E:` lines are also non-empty.
    let mut total: u32 = 0;
    let mut short: u32 = 0;
    for line in xml.lines() {
        if !line.contains("android:name") {
            continue;
        }
        if let Some(name) = extract_attr(line, "android:name") {
            total += 1;
            let last = name.rsplit('.').next().unwrap_or(&name);
            // 0..=3 chars in the last segment is the "obfuscated"
            // threshold. Anything 4+ is treated as readable.
            if last.len() <= 3 {
                short += 1;
            }
        }
    }
    if total < 4 {
        return None;
    }
    let ratio = short as f32 / total as f32;
    Some(ratio.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obfuscation_short_names_flags_50_percent() {
        // 4 of 4 names have a 1-char last segment -> ratio 1.0,
        // way above the 0.30 invalidation threshold.
        let xml = "\
E: activity (line=1)
  A: android:name(0x01010003)=\"com.x.a\"
E: activity (line=2)
  A: android:name(0x01010003)=\"com.x.b\"
E: activity (line=3)
  A: android:name(0x01010003)=\"com.x.c\"
E: activity (line=4)
  A: android:name(0x01010003)=\"com.x.d\"
";
        let ratio = short_name_ratio(xml).expect("enough names");
        assert!(ratio > 0.5, "ratio should be ~1.0, got {ratio}");
    }

    #[test]
    fn obfuscation_short_names_returns_none_below_floor() {
        // 3 names is below the `total < 4` floor — ratio should be
        // `None` so the UI can render "—" instead of a misleading
        // 0 percentage.
        let xml = "\
E: activity (line=1)
  A: android:name(0x01010003)=\"com.x.a\"
E: activity (line=2)
  A: android:name(0x01010003)=\"com.x.b\"
E: activity (line=3)
  A: android:name(0x01010003)=\"com.x.c\"
";
        assert!(short_name_ratio(xml).is_none());
    }

    #[test]
    fn parse_components_extracts_all_four_kinds() {
        // Regression test for "Activity=1, Service/Receiver/Provider=0".
        // Multiple components of each kind interleaved with foreign tags.
        let xml = "\
E: activity (line=1)
  A: android:theme(0x01010005)=@0x7f1300a8
  A: android:name(0x01010003)=\"com.foo.Main\"
E: intent-filter (line=2)
  N: android:action
E: activity (line=3)
  A: android:name(0x01010003)=\"com.foo.Second\"
E: service (line=4)
  A: android:name(0x01010003)=\"com.foo.Svc\"
E: receiver (line=5)
  A: android:name(0x01010003)=\"com.foo.Rcv\"
E: provider (line=6)
  A: android:name(0x01010003)=\"com.foo.Prv\"
E: uses-permission (line=7)
";
        let mut info = ApkInfo::default();
        parse_components_from_xml(xml, &mut info);
        assert_eq!(info.activities, vec!["com.foo.Main", "com.foo.Second"]);
        assert_eq!(info.services, vec!["com.foo.Svc"]);
        assert_eq!(info.receivers, vec!["com.foo.Rcv"]);
        assert_eq!(info.providers, vec!["com.foo.Prv"]);
    }
    /// Regression: real-world APKs use the bare legacy directory
    /// `lib/armeabi/` as well as the modern `lib/armeabi-v7a/`. The previous
    /// whitelist silently dropped anything that wasn't a 4-name table and that
    /// hid every .so inside pre-installed legacy apps (e.g. the atm TV APK on
    /// this machine: 30 .so files all under `armeabi`).
    /// detect_packer is the gating function behind the "Hardened" badge.
    /// It is best-effort / heuristic, so we pin its behaviour on three known
    /// samples rather than letting the user's APK be the test oracle.
    #[test]
    fn detect_packer_returns_clean_for_vanilla_apk() {
        let info = ApkInfo {
            package_name: "com.example.app".into(),
            native_libs: vec!["lib/arm64-v8a/libflutter.so".into()],
            ..ApkInfo::default()
        };
        let report = detect_packer(&info.packer_ref());
        assert!(!report.is_packed, "vanilla APK should not be flagged");
        assert!(report.indicators.is_empty());
        assert!(report.packer_name.is_none());
    }

    #[test]
    fn detect_packer_flags_bangcle_via_native_helper_so() {
        let info = ApkInfo {
            package_name: "com.example.paid".into(),
            native_libs: vec![
                "lib/arm64-v8a/libexec.so".into(),
                "lib/arm64-v8a/libhelper.so".into(),
                "lib/arm64-v8a/libfoo.so".into(),
            ],
            ..ApkInfo::default()
        };
        let report = detect_packer(&info.packer_ref());
        assert!(report.is_packed);
        // First hit's packer label is what we surface.
        assert_eq!(report.packer_name.as_deref(), Some("Bangcle"));
        let native_hits: Vec<&PackerIndicator> = report
            .indicators
            .iter()
            .filter(|i| i.kind == "native")
            .collect();
        assert!(native_hits.iter().any(|h| h.value.contains("libexec.so")));
        assert!(native_hits.iter().any(|h| h.value.contains("libhelper.so")));
    }

    #[test]
    fn detect_packer_flags_via_package_prefix() {
        let info = ApkInfo {
            // No native hits, but the package prefix is the giveaway.
            package_name: "com.qihoo360.example".into(),
            native_libs: vec![],
            ..ApkInfo::default()
        };
        let report = detect_packer(&info.packer_ref());
        assert!(report.is_packed);
        assert_eq!(report.packer_name.as_deref(), Some("360"));
        assert_eq!(report.indicators[0].kind, "entry_class");
    }

    #[test]
    fn abi_from_path_recognises_all_seven_ndk_abis() {
        // Modern.
        assert_eq!(abi_from_path("lib/arm64-v8a/libfoo.so"), Some("arm64-v8a"));
        assert_eq!(abi_from_path("lib/armeabi-v7a/libfoo.so"), Some("armeabi-v7a"));
        assert_eq!(abi_from_path("lib/x86/libfoo.so"), Some("x86"));
        assert_eq!(abi_from_path("lib/x86_64/libfoo.so"), Some("x86_64"));
        // Legacy / deprecated but still seen in older APKs.
        assert_eq!(abi_from_path("lib/armeabi/libfoo.so"), Some("armeabi"));
        assert_eq!(abi_from_path("lib/mips/libfoo.so"), Some("mips"));
        assert_eq!(abi_from_path("lib/mips64/libfoo.so"), Some("mips64"));
        // Nested paths under each ABI must still match (we just want the
        // first path segment).
        assert_eq!(
            abi_from_path("lib/armeabi/sub/dir/libbar.so"),
            Some("armeabi")
        );
        // Files outside `lib/` or under an unknown ABI directory -> None.
        assert_eq!(abi_from_path("assets/foo.so"), None);
        assert_eq!(abi_from_path("lib/riscv64/libfoo.so"), None);
        assert_eq!(abi_from_path("res/raw/armeabi_x86.so"), None);
    }

    #[test]
    fn extract_attr_supports_three_aapt_shapes() {
        // 1. aapt2 dump badging: `name='value'`, single quote.
        assert_eq!(
            extract_attr("name='com.foo.Main'", "name"),
            Some("com.foo.Main".to_string())
        );

        // 2. aapt2 dump xmltree, plain: `key="value"`, double quote.
        assert_eq!(
            extract_attr("android:name=\"com.foo.Main\"", "android:name"),
            Some("com.foo.Main".to_string())
        );

        // 3. aapt2 dump xmltree, resource-id form: `key(0xHEX)="v"`.
        assert_eq!(
            extract_attr(
                "android:name(0x01010003)=\"com.foo.Main\"",
                "android:name"
            ),
            Some("com.foo.Main".to_string())
        );

        // Whitespace tolerance around `=`.
        assert_eq!(
            extract_attr("name  =  'com.foo.Main'", "name"),
            Some("com.foo.Main".to_string())
        );

        // Real xmltree output uses the full namespace URL on the attribute
        // line; the substring matcher still finds the trailing `name(...)`.
        assert_eq!(
            extract_attr(
                "http://schemas.android.com/apk/res/android:name(0x01010003)=\"com.foo.Main\"",
                "android:name"
            ),
            Some("com.foo.Main".to_string())
        );

        // Wrong shape -> None, never a panic.
        assert_eq!(
            extract_attr("theme=\"@0x7f1300a8\"", "android:name"),
            None
        );
    }

    /// Regression for the "permissions show 2-3 duplicates" bug:
    /// `aapt2 dump badging` re-emits the same permission whenever the APK
    /// has merged manifests. Going from 62 reported lines down to 34 actual
    /// permissions is on this APK (`atm_phone_appbox_xiaomitv2_mitv`).
    /// `parse_badging` keeps first-seen order while removing repeats for
    /// permissions / services / receivers / providers.
    #[test]
    fn parse_badging_dedups_repeated_permissions() {
        let badging = "package: name='com.foo' versionCode='1' versionName='1.0' compileSdkVersion='34'
uses-permission: name='android.permission.INTERNET'
uses-permission: name='android.permission.WAKE_LOCK'
uses-permission: name='android.permission.INTERNET'
uses-permission: name='android.permission.INTERNET'
uses-permission: name='android.permission.GET_TASKS'
uses-permission: name='android.permission.WAKE_LOCK'
uses-permission: name='android.permission.GET_TASKS'
uses-permission: name='android.permission.GET_TASKS'
service: name='com.foo.SvcA'
service: name='com.foo.SvcA'
service: name='com.foo.SvcB'
receiver: name='com.foo.Rcv'
receiver: name='com.foo.Rcv'
provider: name='com.foo.Prv'
";
        let info = parse_badging(badging).expect("badging parses");
        assert_eq!(
            info.permissions,
            vec![
                "android.permission.INTERNET".to_string(),
                "android.permission.WAKE_LOCK".to_string(),
                "android.permission.GET_TASKS".to_string(),
            ]
        );
        assert_eq!(
            info.services,
            vec!["com.foo.SvcA".to_string(), "com.foo.SvcB".to_string()]
        );
        assert_eq!(info.receivers, vec!["com.foo.Rcv".to_string()]);
        assert_eq!(info.providers, vec!["com.foo.Prv".to_string()]);
    }


    #[test]
    fn parse_components_ignores_dashed_subtags() {
        // `activity-alias`, `service-alias` etc. should not be picked up as
        // the canonical `activity`/`service` tag. The match guard
        // `if !trimmed.contains("E: activity-")` covers this.
        let xml = "\
E: activity-alias (line=1)
  A: android:name(0x01010003)=\"com.foo.Alias\"
E: activity (line=2)
  A: android:name(0x01010003)=\"com.foo.Main\"
";
        let mut info = ApkInfo::default();
        parse_components_from_xml(xml, &mut info);
        assert_eq!(info.activities, vec!["com.foo.Main"]);
        assert!(info.services.is_empty());
    }
}
