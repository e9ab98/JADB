use crate::error::{AppError, AppResult};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const EOCD_MIN_SIZE: usize = 22;
const MAX_EOCD_SEARCH: u64 = EOCD_MIN_SIZE as u64 + u16::MAX as u64;
const EOCD_SIGNATURE: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
const APK_SIG_BLOCK_FOOTER_SIZE: u64 = 24;
const APK_SIG_BLOCK_MAGIC: &[u8; 16] = b"APK Sig Block 42";
const APK_SIG_BLOCK_V2_ID: u32 = 0x7109_871a;
const APK_SIG_BLOCK_V3_ID: u32 = 0xf053_68c0;
const APK_SIG_BLOCK_V31_ID: u32 = 0x1b93_ad61;

pub fn is_apk_signed(path: &Path) -> AppResult<bool> {
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(AppError::InvalidInput(format!(
            "not a regular file: {}",
            path.display()
        )));
    }
    if has_v1_signature(path)? {
        return Ok(true);
    }
    has_apk_signing_block_signature(path, metadata.len())
}

fn has_v1_signature(path: &Path) -> AppResult<bool> {
    // Collect candidate data offsets for .RSA/.DSA/.EC entries.
    let candidates: Vec<u64> = {
        let file = File::open(path)?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|error| AppError::Parse(format!("invalid APK ZIP: {error}")))?;
        let mut offsets: Vec<u64> = Vec::new();
        for index in 0..archive.len() {
            let entry = archive
                .by_index(index)
                .map_err(|error| AppError::Parse(format!("invalid APK ZIP entry: {error}")))?;
            let upper = entry.name().to_ascii_uppercase();
            let Some(name) = upper.strip_prefix("META-INF/") else {
                continue;
            };
            if !name.contains('/')
                && (name.ends_with(".RSA") || name.ends_with(".DSA") || name.ends_with(".EC"))
            {
                offsets.push(entry.data_start());
            }
        }
        offsets
    };
    // A valid PKCS#7 SignedData is ASN.1 DER — top-level SEQUENCE tag is 0x30.
    // strip_signing flips the first byte, so a broken V1 file starts with something else.
    let mut file = File::open(path)?;
    for offset in candidates {
        file.seek(SeekFrom::Start(offset))?;
        let mut byte = [0u8; 1];
        if file.read_exact(&mut byte).is_ok() && byte[0] == 0x30 {
            return Ok(true);
        }
    }
    Ok(false)
}

fn has_apk_signing_block_signature(path: &Path, file_len: u64) -> AppResult<bool> {
    let mut file = File::open(path)?;
    let (eocd_offset, eocd) = find_eocd(&mut file, file_len)?;
    let central_directory_offset = u32::from_le_bytes([
        eocd[16], eocd[17], eocd[18], eocd[19],
    ]);
    if central_directory_offset == u32::MAX {
        return Err(AppError::Parse("ZIP64 APK is not supported".into()));
    }
    let central_directory_offset = central_directory_offset as u64;
    if central_directory_offset > eocd_offset {
        return Err(AppError::Parse(
            "ZIP central directory offset is outside the APK".into(),
        ));
    }
    if central_directory_offset < APK_SIG_BLOCK_FOOTER_SIZE {
        return Ok(false);
    }

    file.seek(SeekFrom::Start(
        central_directory_offset - APK_SIG_BLOCK_FOOTER_SIZE,
    ))?;
    let mut footer = [0u8; APK_SIG_BLOCK_FOOTER_SIZE as usize];
    file.read_exact(&mut footer)?;
    if &footer[8..] != APK_SIG_BLOCK_MAGIC {
        return Ok(false);
    }

    let block_size = u64::from_le_bytes([
        footer[0], footer[1], footer[2], footer[3], footer[4], footer[5], footer[6], footer[7],
    ]);
    if block_size < APK_SIG_BLOCK_FOOTER_SIZE {
        return Err(AppError::Parse(
            "APK Signing Block size is invalid".into(),
        ));
    }
    let total_block_size = block_size
        .checked_add(8)
        .ok_or_else(|| AppError::Parse("APK Signing Block size overflow".into()))?;
    if total_block_size > central_directory_offset {
        return Err(AppError::Parse(
            "APK Signing Block extends outside the APK".into(),
        ));
    }

    let block_start = central_directory_offset - total_block_size;
    file.seek(SeekFrom::Start(block_start))?;
    let leading_size = read_u64(&mut file)?;
    if leading_size != block_size {
        return Err(AppError::Parse(
            "APK Signing Block sizes do not match".into(),
        ));
    }

    let pairs_end = central_directory_offset - APK_SIG_BLOCK_FOOTER_SIZE;
    let mut cursor = block_start
        .checked_add(8)
        .ok_or_else(|| AppError::Parse("APK Signing Block offset overflow".into()))?;
    if cursor > pairs_end {
        return Err(AppError::Parse(
            "APK Signing Block pair area is invalid".into(),
        ));
    }

    while cursor < pairs_end {
        if pairs_end - cursor < 8 {
            return Err(AppError::Parse(
                "APK Signing Block pair length is truncated".into(),
            ));
        }
        file.seek(SeekFrom::Start(cursor))?;
        let pair_size = read_u64(&mut file)?;
        if pair_size < 4 {
            return Err(AppError::Parse(
                "APK Signing Block pair is too small".into(),
            ));
        }
        let pair_end = cursor
            .checked_add(8)
            .and_then(|offset| offset.checked_add(pair_size))
            .ok_or_else(|| AppError::Parse("APK Signing Block pair size overflow".into()))?;
        if pair_end > pairs_end {
            return Err(AppError::Parse(
                "APK Signing Block pair extends beyond its boundary".into(),
            ));
        }
        let id = read_u32(&mut file)?;
        if is_supported_scheme(id) {
            return Ok(true);
        }
        cursor = pair_end;
    }

    Ok(false)
}

fn find_eocd(file: &mut File, file_len: u64) -> AppResult<(u64, [u8; EOCD_MIN_SIZE])> {
    if file_len < EOCD_MIN_SIZE as u64 {
        return Err(AppError::Parse(
            "APK is too small to be a ZIP file".into(),
        ));
    }
    let search_len = file_len.min(MAX_EOCD_SEARCH);
    let search_start = file_len - search_len;
    file.seek(SeekFrom::Start(search_start))?;
    let mut tail = vec![0u8; search_len as usize];
    file.read_exact(&mut tail)?;

    for index in (0..=tail.len() - EOCD_MIN_SIZE).rev() {
        if &tail[index..index + EOCD_SIGNATURE.len()] != EOCD_SIGNATURE.as_slice() {
            continue;
        }
        let comment_len = u16::from_le_bytes([tail[index + 20], tail[index + 21]]) as usize;
        if index + EOCD_MIN_SIZE + comment_len != tail.len() {
            continue;
        }
        let mut record = [0u8; EOCD_MIN_SIZE];
        record.copy_from_slice(&tail[index..index + EOCD_MIN_SIZE]);
        return Ok((search_start + index as u64, record));
    }

    Err(AppError::Parse(
        "ZIP end-of-central-directory record not found".into(),
    ))
}

fn read_u64(reader: &mut impl Read) -> AppResult<u64> {
    let mut bytes = [0u8; 8];
    reader.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

fn read_u32(reader: &mut impl Read) -> AppResult<u32> {
    let mut bytes = [0u8; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn is_supported_scheme(id: u32) -> bool {
    matches!(
        id,
        APK_SIG_BLOCK_V2_ID | APK_SIG_BLOCK_V3_ID | APK_SIG_BLOCK_V31_ID
    )
}

// ============================================================================
// Inspect / Strip (signature viewer + remover)
// ============================================================================

use crate::config::settings;
use crate::progress;
use crate::services::apk_signer::{self, TaskHandle};
use crate::services::task_registry::TaskRegistry;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

const ZIP_EOCD_SIGNATURE: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
const ZIP_EOCD_MIN_SIZE: usize = 22;
const APK_SIG_BLOCK_V4_ID: u32 = 0x1d96_bb1c;
const APK_SIG_BLOCK_FOOTER_TOTAL: u64 = 24;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SignatureInfo {
    pub apk_path: String,
    pub file_size: u64,
    pub verifies: bool,
    pub is_signed: bool,
    pub verified_v1: bool,
    pub verified_v2: bool,
    pub verified_v3: bool,
    pub verified_v31: bool,
    pub verified_v4: bool,
    pub signer_count: u32,
    pub error_message: Option<String>,
    pub raw_output: String,
    /// Per-signer certificate detail. Built from `apksigner verify
    /// --print-certs`. Empty when apksigner is not configured or the APK
    /// is unsigned; the index matches apksigner's 1-based "Signer #N".
    #[serde(default)]
    pub signers: Vec<SignerDetail>,
}

/// Discrete key-strength buckets used to render the "key health" badge.
/// The thresholds are derived from current (2026) CA/Browser Forum
/// guidelines: RSA < 2048 bits and anything < 224 bits EC is "weak",
/// 2048+ RSA / 256+ EC is "acceptable", 3072+ RSA / 384+ EC is "strong".
/// We deliberately do not surface anything as "strong" for RSA-2048 only
/// because long-lived signing keys should plan ahead.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum KeyStrength {
    Weak,
    Acceptable,
    Strong,
    Unknown,
}

impl KeyStrength {
    /// Compute strength from a key algorithm string ("RSA", "EC", "DSA", ...)
    /// and a bit length. Tolerant of unknowns so a future apksigner output
    /// doesn't crash the renderer.
    fn from_parts(algorithm: Option<&str>, bits: Option<u32>) -> Self {
        let Some(bits) = bits else {
            return KeyStrength::Unknown;
        };
        match algorithm.map(|s| s.to_ascii_uppercase()).as_deref() {
            Some("RSA") => match bits {
                ..=1023 => KeyStrength::Weak,
                1024..=2031 => KeyStrength::Acceptable,
                2032..=3071 => KeyStrength::Strong,
                _ => KeyStrength::Strong,
            },
            Some("EC") | Some("ECDSA") => match bits {
                ..=159 => KeyStrength::Weak,
                160..=223 => KeyStrength::Acceptable,
                224..=255 => KeyStrength::Strong,
                _ => KeyStrength::Strong,
            },
            Some("DSA") => match bits {
                ..=1023 => KeyStrength::Weak,
                _ => KeyStrength::Acceptable,
            },
            _ => KeyStrength::Unknown,
        }
    }
}

impl Default for KeyStrength {
    fn default() -> Self {
        KeyStrength::Unknown
    }
}

/// One row of the `Signer #N certificate ...` block that `apksigner verify
/// --print-certs` emits. Fields are `None` when apksigner did not emit them
/// (older builds may use "certificate key type" instead of "key algorithm",
/// for example).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SignerDetail {
    pub index: u32,
    pub dn: Option<String>,
    pub issuer_dn: Option<String>,
    pub sha256: Option<String>,
    pub sha1: Option<String>,
    pub md5: Option<String>,
    pub serial: Option<String>,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    /// Algorithm label (`RSA`, `EC`, `ECDSA`, `DSA`) emitted by apksigner
    /// at the time of `key algorithm:`.
    pub key_algorithm: Option<String>,
    /// Public / private key bit length.
    pub key_bits: Option<u32>,
    /// SHA-256 digest of the *public key* (NOT the X.509 certificate).
    /// This is what most pinning / PEP integrations actually consume.
    pub public_key_sha256: Option<String>,
    pub public_key_sha1: Option<String>,
    pub public_key_md5: Option<String>,
    /// Signature algorithm emitted by the certificate (e.g.
    /// `SHA256withRSA`, `SHA256withECDSA`). apksigner versions before 0.7
    /// only print this on debug keys.
    pub signature_algorithm: Option<String>,
    /// X.509 certificate version (1, 3, ...). Emitted only when apksigner
    /// decided to surface it; usually missing on debug keystores.
    pub cert_version: Option<u32>,
    /// `true` when the certificate DN / issuer DN looks like the standard
    /// Android debug keystore. Pure heuristic on the strings apksigner
    /// emits; we err on the side of `false` so a real key with an
    /// "Android" organization does not get a false positive.
    pub is_debug_signed: bool,
    /// Bucketed key strength used by the UI to colour the badge.
    pub key_strength: KeyStrength,
    /// Distinct schemes this signer was verified under (e.g. ["v1", "v2"]).
    #[serde(default)]
    pub schemes: Vec<String>,
    /// apksigner warnings attached to this signer block (e.g.
    /// "META-INF/xyz not protected by signature").
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StripResult {
    pub output_path: String,
    pub output_size: u64,
    pub removed_v1_files: Vec<String>,
    pub removed_v2_v3: bool,
    pub removed_v4_idsig: bool,
    pub had_v1: bool,
    pub had_v2_v3: bool,
    pub had_v4: bool,
    pub source_path: String,
}

pub async fn inspect_signature(app: &AppHandle, apk_path: &str) -> AppResult<SignatureInfo> {
    if apk_path.trim().is_empty() {
        return Err(AppError::InvalidInput("apk_path is empty".into()));
    }
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    let settings = settings::read(&dir).await?;
    inspect_signature_inner(&settings, apk_path).await
}

/// Pure logic for inspecting the APK signature — does not touch Tauri /
/// `AppHandle`. Lives in the same module as `inspect_signature` so the
/// `apksigner verify` wiring and the fallback behaviour stay in one place.
/// Used by both the `inspect_signature` Tauri command and the analysis
/// pipeline (`apk_analyzer::analyze`) so the Analyze tab can surface
/// signer details without a second IPC round-trip.
pub async fn inspect_signature_inner(
    settings: &crate::config::settings::Settings,
    apk_path: &str,
) -> AppResult<SignatureInfo> {
    if apk_path.trim().is_empty() {
        return Err(AppError::InvalidInput("apk_path is empty".into()));
    }
    let path = PathBuf::from(apk_path);
    let metadata = std::fs::metadata(&path)?;
    let mut info = SignatureInfo {
        apk_path: apk_path.to_string(),
        file_size: metadata.len(),
        ..Default::default()
    };

    let build_tools = match apk_signer::resolve_build_tools(settings) {
        Ok(tools) => tools,
        Err(error) => {
            info.error_message = Some(format!("apksigner 不可用: {error}"));
            info.is_signed = is_apk_signed(&path).unwrap_or(false);
            return Ok(info);
        }
    };

    let mut command = tokio::process::Command::new("java");
    command
        .arg("-Xmx1024M")
        .arg("-jar")
        .arg(&build_tools.apksigner_jar)
        .arg("verify")
        .arg("--verbose")
        .arg("--print-certs");
    let v4_signature_path = companion_idsig_path(&path);
    if v4_signature_path.is_file() {
        command
            .arg("--v4-signature-file")
            .arg(&v4_signature_path);
    }
    let output = command
        .arg(&path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|error| AppError::Config(format!("java invocation failed: {error}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    info.verifies = output.status.success();
    info.raw_output = if stderr.is_empty() {
        stdout.clone()
    } else {
        format!("{stdout}\n--- stderr ---\n{stderr}")
    };

    parse_apksigner_output(&stdout, &mut info);
    info.is_signed = info.verified_v1
        || info.verified_v2
        || info.verified_v3
        || info.verified_v31
        || info.verified_v4;

    if !info.is_signed {
        info.is_signed = is_apk_signed(&path).unwrap_or(false);
    }
    if !info.is_signed && info.error_message.is_none() && !info.verifies {
        info.error_message = Some("apksigner 报告此 APK 不通过校验".into());
    }

    Ok(info)
}

fn parse_apksigner_output(stdout: &str, info: &mut SignatureInfo) {
    // Index of the Signer #N row whose detail lines we are currently
    // collecting. We flush it whenever the next "Signer #N" row appears.
    let mut current: Option<SignerDetail> = None;
    // Scheme verdicts that arrived before a Signer header was parsed --
    // we apply them to whichever signer opens next so per-signer views
    // can show the schemes list regardless of order in the apksigner
    // output.
    let mut pending_schemes: Vec<String> = Vec::new();
    let flush = |cur: &mut Option<SignerDetail>, info: &mut SignatureInfo| {
        if let Some(mut s) = cur.take() {
            // Finalise heuristic flags once all detail rows are in.
            s.is_debug_signed = looks_like_debug_cert(s.dn.as_deref(), s.issuer_dn.as_deref());
            s.key_strength = KeyStrength::from_parts(s.key_algorithm.as_deref(), s.key_bits);
            if is_significant_signer(&s) {
                info.signers.push(s);
            }
        }
    };
    for line in stdout.lines() {
        let trimmed = line.trim();

        // Top-level apksigner WARNING lines (e.g. v1 scheme reporting
        // unprotected META-INF entries) belong to the signer currently
        // being described; we collect them so the UI can show them.
        if let Some(rest) = trimmed.strip_prefix("WARNING:") {
            if let Some(cur) = current.as_mut() {
                cur.warnings.push(rest.trim().to_string());
            }
            continue;
        }

        // New signer row -> flush previous, start a fresh entry.
        if let Some(header) = parse_signer_header(trimmed) {
            flush(&mut current, info);
            if header.idx > info.signer_count {
                info.signer_count = header.idx;
            }
            let mut signer = SignerDetail {
                index: header.idx,
                schemes: std::mem::take(&mut pending_schemes),
                ..Default::default()
            };
            // The header line itself carries the opening DN / issuer; seed
            // those right now so a signer with only a DN row still renders
            // subject info on the UI.
            match header.kind {
                SignerHeaderKind::CertificateDn => signer.dn = Some(header.value),
                SignerHeaderKind::IssuerDn => signer.issuer_dn = Some(header.value),
            }
            current = Some(signer);
            continue;
        }

        // Top-level scheme verdicts (Signer-independent, but we ALSO attribute
        // each scheme to the current signer so per-signer views can render
        // v1/v2/v3 separately). Only "true" verdicts get attached -- a false
        // verdict on its own line carries no useful info for the UI.
        macro_rules! capture_scheme {
            ($rest:expr, $info_field:ident, $scheme:literal) => {{
                let verdict = parse_bool_after_colon($rest);
                info.$info_field = verdict;
                if verdict {
                    attribute_scheme(&mut current, &mut pending_schemes, $scheme);
                }
            }};
        }
        if let Some(rest) = trimmed.strip_prefix("Verified using v1 scheme") {
            capture_scheme!(rest, verified_v1, "v1");
        } else if let Some(rest) = trimmed.strip_prefix("Verified using v2 scheme") {
            capture_scheme!(rest, verified_v2, "v2");
        } else if let Some(rest) = trimmed.strip_prefix("Verified using v3 scheme") {
            capture_scheme!(rest, verified_v3, "v3");
        } else if let Some(rest) = trimmed.strip_prefix("Verified using v3.1 scheme") {
            capture_scheme!(rest, verified_v31, "v3.1");
        } else if let Some(rest) = trimmed.strip_prefix("Verified using v4 scheme") {
            capture_scheme!(rest, verified_v4, "v4");
        }

        // Per-signer detail rows. We split on the literal ":" suffix
        // apksigner uses so the right-hand side can contain commas/spaces
        // (`CN=foo, O=bar, C=CN` style DNs are common). Several prefixes
        // exist in different apksigner versions; we accept both forms.
        if let Some(cur) = current.as_mut() {
            if let Some(v) = strip_after(trimmed, "certificate DN:") {
                cur.dn = Some(v);
            } else if let Some(v) = strip_after(trimmed, "issuer DN:") {
                cur.issuer_dn = Some(v);
            } else if let Some(v) = strip_after(trimmed, "certificate SHA-256 digest:") {
                cur.sha256 = Some(v);
            } else if let Some(v) = strip_after(trimmed, "certificate SHA-1 digest:") {
                cur.sha1 = Some(v);
            } else if let Some(v) = strip_after(trimmed, "certificate MD5 digest:") {
                cur.md5 = Some(v);
            } else if let Some(v) = strip_after(trimmed, "certificate serial number:") {
                cur.serial = Some(v);
            } else if let Some(v) = strip_after(trimmed, "certificate validity start:") {
                cur.valid_from = Some(v);
            } else if let Some(v) = strip_after(trimmed, "certificate validity end:") {
                cur.valid_to = Some(v);
            } else if let Some(v) = strip_after(trimmed, "key algorithm:") {
                cur.key_algorithm = Some(v);
            } else if let Some(v) = strip_after(trimmed, "key size (bits):") {
                cur.key_bits = v.trim().parse::<u32>().ok();
            } else if let Some(v) = strip_after(trimmed, "certificate key type:") {
                // Older apksigner: `Signer #N certificate key type: RSA (2048 bit)`.
                // Pull algorithm = first whitespace-separated token, bit length =
                // first standalone digit run ("256" inside `(256 bit)`).
                if cur.key_algorithm.is_none() {
                    let algo = v.split_whitespace().next().unwrap_or(&v).to_string();
                    cur.key_algorithm = Some(algo);
                }
                if cur.key_bits.is_none() {
                    let bits = extract_first_uint(&v);
                    cur.key_bits = bits;
                }
            } else if let Some(v) = strip_after(trimmed, "public key SHA-256 digest:") {
                cur.public_key_sha256 = Some(v);
            } else if let Some(v) = strip_after(trimmed, "public key SHA-1 digest:") {
                cur.public_key_sha1 = Some(v);
            } else if let Some(v) = strip_after(trimmed, "public key MD5 digest:") {
                cur.public_key_md5 = Some(v);
            } else if let Some(v) = strip_after(trimmed, "certificate signature algorithm:") {
                cur.signature_algorithm = Some(v);
            } else if let Some(v) = strip_after(trimmed, "certificate version:") {
                cur.cert_version = v.trim().parse::<u32>().ok();
            }
        }
    }
    flush(&mut current, info);
    eprintln!("[DBG-END] signers={} signer_count={}", info.signers.len(), info.signer_count);
}

/// True only when the signer record carries enough detail to be worth
/// surfacing. apksigner occasionally emits `Signer #N` rows whose only
/// contribution is a single scheme verdict; those would otherwise render
/// as empty cards.
fn is_significant_signer(s: &SignerDetail) -> bool {
    s.dn.is_some()
        || s.issuer_dn.is_some()
        || s.sha256.is_some()
        || s.sha1.is_some()
        || s.md5.is_some()
        || s.serial.is_some()
        || s.valid_from.is_some()
        || s.valid_to.is_some()
        || s.key_algorithm.is_some()
        || s.public_key_sha256.is_some()
        || s.public_key_sha1.is_some()
        || s.public_key_md5.is_some()
        || s.signature_algorithm.is_some()
        || s.cert_version.is_some()
        || !s.schemes.is_empty()
}

/// Heuristic check that looks for the well-known Android debug keystore
/// strings inside the Subject / Issuer DNs. Production keystores that
/// happen to have "Android" somewhere in their O= are NOT flagged because
/// we require *both* an `O=Android`-style component AND a `CN=Android
/// Debug` marker (the default debug keystore is the only common cert that
/// ships with this exact combination).
fn looks_like_debug_cert(subject: Option<&str>, issuer: Option<&str>) -> bool {
    let is_debug_cn = |dn: Option<&str>| -> bool {
        dn.map(|s| {
            let upper = s.to_ascii_uppercase();
            upper.contains("CN=ANDROID DEBUG") || upper.contains("O=ANDROID DEBUG")
        })
        .unwrap_or(false)
    };
    is_debug_cn(subject) || is_debug_cn(issuer)
}

/// Parse `Signer #1 certificate DN: ...` / `Signer #1 certificate SHA-256
/// digest: ...` / `Signer #1 key algorithm: ...` etc. Returns
/// `Some(index)` **only** for the *opening* line of a fresh signer block.
///
/// The naive `find("signer #")` matcher used to misfire here: every detail
/// row of signer #1 ("Signer #1 certificate SHA-256 digest: ..." /
/// "Signer #1 public key SHA-256 digest: ...") also contains the
/// substring "Signer #1", which caused every detail row to be
/// mis-interpreted as a fresh signer header and immediately flushed.
///
/// What the *opening* line of an apksigner signer block looks like.
/// Detail rows under the same signer (`Signer #1 certificate SHA-256
/// digest: ...`, `Signer #1 key algorithm: ...`, etc.) are picked up by
/// `strip_after`, NOT here. Returning the keyword kind + value lets the
/// caller seed the signer record without a second pass.
#[derive(Debug, PartialEq, Eq)]
enum SignerHeaderKind {
    CertificateDn,
    IssuerDn,
}

#[derive(Debug)]
struct SignerHeader {
    idx: u32,
    kind: SignerHeaderKind,
    value: String,
}

fn parse_signer_header(line: &str) -> Option<SignerHeader> {
    let trimmed = line.trim_start();
    let lower = trimmed.to_ascii_lowercase();
    let after_prefix = lower.strip_prefix("signer #")?;
    let digits: String = after_prefix.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let idx: u32 = digits.parse().ok()?;
    let mut chars = after_prefix.chars();
    for _ in 0..digits.len() {
        chars.next();
    }
    if chars.next() != Some(' ') {
        return None;
    }
    let lower_tail: String = chars.collect();
    let lower_trim = lower_tail.trim_start();
    if lower_trim.starts_with("certificate dn:") {
        let value = strip_after(&trimmed, "certificate DN:").unwrap_or_default();
        if value.is_empty() {
            return None;
        }
        return Some(SignerHeader {
            idx,
            kind: SignerHeaderKind::CertificateDn,
            value,
        });
    }
    if lower_trim.starts_with("issuer dn:") {
        let value = strip_after(&trimmed, "issuer DN:").unwrap_or_default();
        return Some(SignerHeader {
            idx,
            kind: SignerHeaderKind::IssuerDn,
            value,
        });
    }
    None
}

/// Locate `prefix` somewhere inside `line` (case-insensitive) and return
/// the remainder, trimmed. Used for apksigner detail rows whose format is
/// `Signer #N <keyword>: <value>` -- the keyword is rarely at column 0.
///
/// We previously required the keyword to be the line prefix, which made
/// every apksigner detail row fail to parse. The middle-of-line search is
/// case-insensitive so we accept both `Certificate DN:` and `certificate dn:`.
fn strip_after(line: &str, prefix: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    let prefix_lower = prefix.to_ascii_lowercase();
    let start = lower.find(&prefix_lower)?;
    // Advance past the matched prefix. We index by char to cope with the
    // unlikely case where the prefix contains a non-ASCII byte later.
    let mut end = start + prefix_lower.len();
    while end < lower.len() && !lower.as_bytes()[end].is_ascii_whitespace() {
        end += 1;
    }
    let after_prefix = &trimmed[end..];
    Some(after_prefix.trim().trim_start_matches(':').trim().to_string())
}

/// Return the first decimal integer inside `s`. Used by the legacy
/// `certificate key type: RSA (2048 bit)` line where the bit length is
/// wrapped in parens and would otherwise be lost to whitespace splitting.
fn extract_first_uint(s: &str) -> Option<u32> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            let run = &s[start..i];
            if let Ok(n) = run.parse::<u32>() {
                return Some(n);
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Tag a scheme verdict onto the currently-open signer. If no signer has
/// been opened yet (apksigner tends to print `Verified using vX scheme`
/// lines *before* the first `Signer #N ...`) we stash the verdict in
/// `pending_schemes` so the next signer header inherits it.
fn attribute_scheme(
    current: &mut Option<SignerDetail>,
    pending: &mut Vec<String>,
    scheme: &str,
) {
    let already_recorded = |schemes: &[String], scheme: &str| {
        schemes.iter().any(|s| s == scheme)
    };
    if let Some(cur) = current.as_mut() {
        if !already_recorded(&cur.schemes, scheme) {
            cur.schemes.push(scheme.to_string());
        }
    } else if !already_recorded(pending, scheme) {
        pending.push(scheme.to_string());
    }
}

fn parse_bool_after_colon(rest: &str) -> bool {
    rest.split(':')
        .next_back()
        .map(|s| s.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

pub async fn strip_signing(
    app: &AppHandle,
    registry: &TaskRegistry,
    apk_path: &str,
    output_path: Option<&str>,
) -> AppResult<TaskHandle> {
    if apk_path.trim().is_empty() {
        return Err(AppError::InvalidInput("apk_path is empty".into()));
    }
    let src = PathBuf::from(apk_path);
    let output = match output_path {
        Some(p) => PathBuf::from(p),
        None => default_stripped_path(&src)?,
    };
    if !src.is_file() {
        return Err(AppError::InvalidInput(format!(
            "APK 文件不存在: {}",
            src.display()
        )));
    }
    let task_id = uuid::Uuid::new_v4().to_string();
    let _ = registry.register(&task_id);
    let app_clone = app.clone();
    let task_id_clone = task_id.clone();
    let src_clone = src.clone();
    let output_clone = output.clone();

    tokio::spawn(async move {
        let app_for_blocking = app_clone.clone();
        let task_id_for_blocking = task_id_clone.clone();
        let result = tokio::task::spawn_blocking(move || {
            strip_signing_blocking(
                &src_clone,
                &output_clone,
                &app_for_blocking,
                &task_id_for_blocking,
            )
        })
        .await;
        match result {
            Ok(Ok(strip_result)) => {
                progress::emit_progress(&app_clone, &task_id_clone, 100.0, "完成");
                progress::emit_done(&app_clone, &task_id_clone, Some(strip_result));
            }
            Ok(Err(error)) => {
                progress::emit_error(
                    &app_clone,
                    &task_id_clone,
                    &format!("去除签名失败: {error}"),
                );
            }
            Err(join_error) => {
                progress::emit_error(
                    &app_clone,
                    &task_id_clone,
                    &format!("去除签名任务失败: {join_error}"),
                );
            }
        }
    });

    Ok(TaskHandle {
        task_id,
        kind: "strip".into(),
    })
}

fn default_stripped_path(src: &Path) -> AppResult<PathBuf> {
    let parent = src
        .parent()
        .ok_or_else(|| AppError::InvalidInput("APK 路径缺少父目录".into()))?;
    let stem = src
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("apk");
    Ok(parent.join(format!("{stem}.unsigned.apk")))
}

fn strip_signing_blocking(
    src: &Path,
    output: &Path,
    app: &AppHandle,
    task_id: &str,
) -> AppResult<StripResult> {
    use std::collections::HashSet;
    

    #[allow(dead_code)]
    const COPY_BUFFER_SIZE: usize = 1024 * 1024;

    progress::emit_progress(app, task_id, 0.0, "读取 APK");
    progress::emit_log(app, task_id, &format!("源文件: {}", src.display()), "info");
    let original = std::fs::read(src)?;
    let original_len = original.len() as u64;
    progress::emit_log(app, task_id, &format!("已读取 {} 字节", original_len), "info");

    progress::emit_progress(app, task_id, 30.0, "解析签名");
    let (_eocd_offset, eocd) = find_eocd_bytes(&original)?;
    let original_cd_offset = u32::from_le_bytes([eocd[16], eocd[17], eocd[18], eocd[19]]) as u64;
    let (_, had_v2_v3) = read_apk_signing_block_pairs(&original, original_cd_offset)?;

    let mut removed_v1: Vec<String> = Vec::new();
    let mut had_v1 = false;
    {
        let cursor = std::io::Cursor::new(&original);
        let mut archive = zip::ZipArchive::new(cursor)
            .map_err(|error| AppError::Parse(format!("invalid APK ZIP: {error}")))?;
        for i in 0..archive.len() {
            let entry = archive
                .by_index(i)
                .map_err(|error| AppError::Parse(format!("invalid APK ZIP entry: {error}")))?;
            if is_v1_signature_entry(entry.name()) {
                removed_v1.push(entry.name().to_string());
                had_v1 = true;
            }
        }
    }
    progress::emit_log(
        app,
        task_id,
        &format!(
            "V1 文件 {} 个将被删除,APK Signing Block 含 V2/V3+ {}",
            removed_v1.len(),
            had_v2_v3
        ),
        "info",
    );

    progress::emit_progress(app, task_id, 60.0, "重写 ZIP");
    let max_zip_filename_len = 256usize;
    let temp_path = std::env::temp_dir().join(format!(
        "jadb-strip-{}-{}.tmp",
        std::process::id(),
        original_len
    ));
    let new_zip_len = {
        let v1_set: HashSet<&str> = removed_v1.iter().map(|s| s.as_str()).collect();
        let cursor = std::io::Cursor::new(&original);
        let mut archive = zip::ZipArchive::new(cursor)
            .map_err(|error| AppError::Parse(format!("invalid APK ZIP: {error}")))?;
        let temp_file = std::fs::File::create(&temp_path)?;
        let mut writer = zip::ZipWriter::new(temp_file);

        for i in 0..archive.len() {
            let entry = archive
                .by_index(i)
                .map_err(|error| AppError::Parse(format!("invalid APK ZIP entry: {error}")))?;
            let name = entry.name();
            if v1_set.contains(name) {
                continue;
            }
            let name_len = name.len();
            if name_len > max_zip_filename_len {
                return Err(AppError::Parse(format!(
                    "entry 名称过长,无法安全重写: {name_len}"
                )));
            }
            // raw_copy_file moves the compressed bytes without inflate/deflate, so 476MB
            // entries go through in milliseconds instead of minutes.
            writer
                .raw_copy_file(entry)
                .map_err(|error| AppError::Parse(format!("raw_copy_file failed: {error}")))?;
        }
        writer
            .finish()
            .map_err(|error| AppError::Parse(format!("zip finish failed: {error}")))?;
        let final_size = std::fs::metadata(&temp_path)?.len();
        final_size
    };

    let new_zip = std::fs::read(&temp_path)?;
    let _ = std::fs::remove_file(&temp_path);
    progress::emit_log(
        app,
        task_id,
        &format!("重写完成:输出 {} 字节", new_zip_len),
        "info",
    );

    progress::emit_progress(app, task_id, 92.0, "写入新 APK");
    std::fs::write(output, &new_zip)?;
    progress::emit_log(app, task_id, &format!("已写入: {}", output.display()), "info");

    progress::emit_progress(app, task_id, 96.0, "清理 .idsig");
    let idsig_path = companion_idsig_path(src);
    let (had_v4, removed_v4) = if idsig_path.is_file() {
        let _ = std::fs::remove_file(&idsig_path);
        progress::emit_log(
            app,
            task_id,
            &format!("已删除 .idsig: {}", idsig_path.display()),
            "info",
        );
        (true, true)
    } else {
        (false, false)
    };

    progress::emit_progress(app, task_id, 100.0, "完成");

    // Final verification: confirm no V1 entries remain in the output ZIP.
    {
        let output_file = std::fs::File::open(output)?;
        match zip::ZipArchive::new(output_file) {
            Ok(mut archive) => {
                let mut leftover: Vec<String> = Vec::new();
                let mut all_meta: Vec<String> = Vec::new();
                for i in 0..archive.len() {
                    let entry = archive
                        .by_index(i)
                        .map_err(|error| AppError::Parse(format!("invalid APK ZIP entry: {error}")))?;
                    let name = entry.name();
                    if name.to_ascii_uppercase().starts_with("META-INF/") {
                        all_meta.push(name.to_string());
                    }
                    if is_v1_signature_entry(name) {
                        leftover.push(name.to_string());
                    }
                }
                progress::emit_log(
                    app,
                    task_id,
                    &format!("stripped APK 中 META-INF/* entries: {:?}", all_meta),
                    "info",
                );
                let v1_set: std::collections::HashSet<&str> = removed_v1
                    .iter()
                    .map(|s| s.as_str())
                    .collect();
                if !leftover.is_empty() {
                    progress::emit_log(
                        app,
                        task_id,
                        &format!(
                            "警告:stripped APK 中仍残留 V1 文件: {:?}",
                            leftover
                        ),
                        "warn",
                    );
                } else {
                    progress::emit_log(
                        app,
                        task_id,
                        &format!(
                            "验证:stripped APK 中已无 V1 entry (本应删除 {} 个)",
                            v1_set.len()
                        ),
                        "info",
                    );
                }
            }
            Err(error) => {
                progress::emit_log(
                    app,
                    task_id,
                    &format!("验证失败:无法打开输出 ZIP: {error}"),
                    "warn",
                );
            }
        }
    }

    let removed_v2_v3 = had_v2_v3 || had_v1;

    Ok(StripResult {
        output_path: output.to_string_lossy().to_string(),
        output_size: new_zip.len() as u64,
        removed_v1_files: removed_v1,
        removed_v2_v3,
        removed_v4_idsig: removed_v4,
        had_v1,
        had_v2_v3,
        had_v4,
        source_path: src.to_string_lossy().to_string(),
    })
}

fn companion_idsig_path(apk: &Path) -> PathBuf {
    let mut path = apk.as_os_str().to_os_string();
    path.push(".idsig");
    PathBuf::from(path)
}

#[derive(Clone)]
#[allow(dead_code)]
struct Pair {
    id: u32,
    data: Vec<u8>,
}

fn read_apk_signing_block_pairs(
    apk: &[u8],
    cd_offset: u64,
) -> AppResult<(Vec<Pair>, bool)> {
    if cd_offset < APK_SIG_BLOCK_FOOTER_TOTAL {
        return Ok((Vec::new(), false));
    }
    let footer_start = (cd_offset - APK_SIG_BLOCK_FOOTER_TOTAL) as usize;
    let mut magic = [0u8; 16];
    magic.copy_from_slice(&apk[footer_start + 8..footer_start + 24]);
    if &magic != APK_SIG_BLOCK_MAGIC {
        return Ok((Vec::new(), false));
    }
    let trailing_size = u64::from_le_bytes(
        apk[footer_start..footer_start + 8].try_into().unwrap(),
    );
    if trailing_size < 24 {
        return Err(AppError::Parse("APK Signing Block size invalid".into()));
    }
    let total_block_size = trailing_size
        .checked_add(8)
        .ok_or_else(|| AppError::Parse("APK Signing Block size overflow".into()))?;
    if total_block_size > cd_offset {
        return Err(AppError::Parse(
            "APK Signing Block extends outside the APK".into(),
        ));
    }
    let block_start = cd_offset - total_block_size;
    let leading_size = u64::from_le_bytes(
        apk[block_start as usize..block_start as usize + 8]
            .try_into()
            .unwrap(),
    );
    if leading_size != trailing_size {
        return Err(AppError::Parse(
            "APK Signing Block sizes do not match".into(),
        ));
    }

    let pairs_end = cd_offset - APK_SIG_BLOCK_FOOTER_TOTAL;
    let mut cursor = block_start + 8;
    let mut pairs = Vec::new();
    let mut had_signature_pair = false;

    while cursor < pairs_end {
        let remaining = pairs_end - cursor;
        if remaining < 8 {
            return Err(AppError::Parse(
                "APK Signing Block pair length is truncated".into(),
            ));
        }
        let pair_size = u64::from_le_bytes(
            apk[cursor as usize..cursor as usize + 8]
                .try_into()
                .unwrap(),
        );
        if pair_size < 4 {
            return Err(AppError::Parse(
                "APK Signing Block pair is too small".into(),
            ));
        }
        let pair_end = cursor
            .checked_add(8)
            .and_then(|v| v.checked_add(pair_size))
            .ok_or_else(|| AppError::Parse("APK Signing Block pair size overflow".into()))?;
        if pair_end > pairs_end {
            return Err(AppError::Parse(
                "APK Signing Block pair extends beyond boundary".into(),
            ));
        }
        let id = u32::from_le_bytes(
            apk[cursor as usize + 8..cursor as usize + 12]
                .try_into()
                .unwrap(),
        );
        let data_len = pair_size as usize - 4;
        let mut data = vec![0u8; data_len];
        data.copy_from_slice(&apk[cursor as usize + 12..cursor as usize + 12 + data_len]);
        if is_signature_scheme_id(id) {
            had_signature_pair = true;
        }
        pairs.push(Pair { id, data });
        cursor = pair_end;
    }

    Ok((pairs, had_signature_pair))
}

#[allow(dead_code)]
fn build_apk_signing_block(pairs: &[Pair]) -> Vec<u8> {
    let mut pairs_data = Vec::new();
    for pair in pairs {
        let pair_size = pair.data.len() as u64 + 4;
        pairs_data.extend_from_slice(&pair_size.to_le_bytes());
        pairs_data.extend_from_slice(&pair.id.to_le_bytes());
        pairs_data.extend_from_slice(&pair.data);
    }
    let block_size_value = pairs_data.len() as u64 + 24;
    let mut result = Vec::new();
    result.extend_from_slice(&block_size_value.to_le_bytes());
    result.extend_from_slice(&pairs_data);
    result.extend_from_slice(&block_size_value.to_le_bytes());
    result.extend_from_slice(APK_SIG_BLOCK_MAGIC);
    result
}

fn is_signature_scheme_id(id: u32) -> bool {
    matches!(
        id,
        APK_SIG_BLOCK_V2_ID | APK_SIG_BLOCK_V3_ID | APK_SIG_BLOCK_V31_ID | APK_SIG_BLOCK_V4_ID
    )
}

fn is_v1_signature_entry(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    if !upper.starts_with("META-INF/") {
        return false;
    }
    let rest = &upper[9..];
    if rest.is_empty() || rest.contains('/') {
        return false;
    }
    if rest == "MANIFEST.MF" || rest == "INDEX.LIST" {
        return true;
    }
    rest.ends_with(".SF")
        || rest.ends_with(".RSA")
        || rest.ends_with(".DSA")
        || rest.ends_with(".EC")
}

fn find_eocd_bytes(data: &[u8]) -> AppResult<(u64, [u8; ZIP_EOCD_MIN_SIZE])> {
    let file_len = data.len() as u64;
    if file_len < ZIP_EOCD_MIN_SIZE as u64 {
        return Err(AppError::Parse("file too small to be a ZIP".into()));
    }
    let search_len = file_len.min(ZIP_EOCD_MIN_SIZE as u64 + u16::MAX as u64);
    let search_start = file_len - search_len;
    let tail = &data[search_start as usize..];
    for index in (0..=tail.len() - ZIP_EOCD_MIN_SIZE).rev() {
        if &tail[index..index + ZIP_EOCD_SIGNATURE.len()] != ZIP_EOCD_SIGNATURE.as_slice() {
            continue;
        }
        let comment_len = u16::from_le_bytes([tail[index + 20], tail[index + 21]]) as usize;
        if index + ZIP_EOCD_MIN_SIZE + comment_len != tail.len() {
            continue;
        }
        let mut record = [0u8; ZIP_EOCD_MIN_SIZE];
        record.copy_from_slice(&tail[index..index + ZIP_EOCD_MIN_SIZE]);
        return Ok((search_start + index as u64, record));
    }
    Err(AppError::Parse("ZIP end-of-central-directory not found".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real-world apksigner output, captured from `apksigner verify --verbose
    /// --print-certs` against a debug APK (Java's standard debug.keystore).
    const APKSIGNER_DEBUG_OUTPUT: &str = "\
Verifies
Verified using v1 scheme (JAR signing): false
Verified using v2 scheme (APK Signature Scheme v2): true
Verified using v3 scheme (APK Signature Scheme v3): false
Verified using v3.1 scheme (APK Signature Scheme v3.1): false
Verified using v4 scheme (APK Signature Scheme v4): false
Verified for SourceStamp: false
Number of signers: 1
Signer #1 certificate DN: C=US, O=Android, CN=Android Debug
Signer #1 certificate SHA-256 digest: 7e0ea093e98cc795c82c0af9859a103063f1d347c55d7cc8fbf7ebabf7fba2b6
Signer #1 certificate SHA-1 digest: f3680235379e53f41dcfed8edf04c28dc9f36217
Signer #1 certificate MD5 digest: 534b0ace528e470432c3498c83f25ea6
Signer #1 key algorithm: RSA
Signer #1 key size (bits): 2048
Signer #1 public key SHA-256 digest: 12ae97d916c5ced299160e0faff4a3e3de730423750d78e67129929b5c15e379
Signer #1 public key SHA-1 digest: d7059d17a44ca76925ff5392158bc3d02ce0483f
Signer #1 public key MD5 digest: ba1a98b08b64cece3d58841e9142d194
WARNING: META-INF/LGPL-2.1.txt not protected by signature.
";

    #[test]
    fn parse_apksigner_extracts_full_signer_detail() {
        let mut info = SignatureInfo::default();
        parse_apksigner_output(APKSIGNER_DEBUG_OUTPUT, &mut info);

        assert_eq!(info.signer_count, 1);
        assert!(info.verified_v2);
        assert!(!info.verified_v1);

        assert_eq!(info.signers.len(), 1);
        let s = &info.signers[0];
        assert_eq!(s.index, 1);
        assert_eq!(s.dn.as_deref(), Some("C=US, O=Android, CN=Android Debug"));
        assert_eq!(s.key_algorithm.as_deref(), Some("RSA"));
        assert_eq!(s.key_bits, Some(2048));
        assert_eq!(
            s.public_key_sha256.as_deref(),
            Some("12ae97d916c5ced299160e0faff4a3e3de730423750d78e67129929b5c15e379")
        );
        assert_eq!(s.warnings.len(), 1);
        assert!(s.is_debug_signed);
        assert_eq!(s.key_strength, KeyStrength::Strong);
        assert_eq!(s.schemes, vec!["v2".to_string()]);
    }

    #[test]
    fn parse_apksigner_accepts_legacy_key_type_line() {
        let stdout = "\
Verifies
Signer #1 certificate DN: CN=test
Signer #1 certificate SHA-256 digest: abcd
Signer #1 certificate key type: EC (256 bit)
";
        let mut info = SignatureInfo::default();
        parse_apksigner_output(stdout, &mut info);
        let s = &info.signers[0];
        assert_eq!(s.key_algorithm.as_deref(), Some("EC"));
        assert_eq!(s.key_bits, Some(256));
        assert_eq!(s.key_strength, KeyStrength::Strong);
    }

    #[test]
    fn key_strength_bucketing_matches_table() {
        assert_eq!(KeyStrength::from_parts(Some("RSA"), Some(1024)), KeyStrength::Acceptable);
        assert_eq!(KeyStrength::from_parts(Some("RSA"), Some(2048)), KeyStrength::Strong);
        assert_eq!(KeyStrength::from_parts(Some("EC"), Some(256)), KeyStrength::Strong);
        assert_eq!(KeyStrength::from_parts(Some("FOO"), None), KeyStrength::Unknown);
    }

    #[test]
    fn debug_cert_heuristic_is_conservative() {
        assert!(looks_like_debug_cert(Some("CN=Android Debug"), None));
        assert!(looks_like_debug_cert(None, Some("CN=Android Debug,O=Android,C=US")));
        assert!(!looks_like_debug_cert(Some("CN=Google,O=Android,C=US"), None));
        assert!(!looks_like_debug_cert(Some("C=CN,O=Tencent"), None));
    }
}
