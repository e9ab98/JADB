//! Generic helpers for parsing `adb shell` / `dumpsys` output.
//!
//! Designed for the system-info snapshot in `adb_manager::system_info`
//! but reusable for any caller that needs to pull a single value out
//! of a multi-line command output. Each helper is strict at the line
//! level (no alphanumeric char before the key) so we don't
//! accidentally pick up identifiers like `EVENT_BT_RSSI_EVENT` as an
//! `RSSI: ...` value. Placeholder values (`null`, `unknown`,
//! `<none>`, `<unknown ssid>`, etc.) are filtered so callers can
//! chain `?` without manual filtering.

use crate::config::settings::Settings;
use crate::services::adb_manager::run_adb_shell;

/// Run `getprop <name>` and return the trimmed value, or `None` when
/// the call fails or the value is empty/`null`/`unknown`. Centralises
/// the standard placeholder filter so every caller handles the same
/// edge cases.
pub(crate) async fn prop_or_none(
    settings: &Settings,
    device: &str,
    name: &str,
) -> Option<String> {
    match run_adb_shell(settings, device, &["getprop", name]).await {
        Ok(value) => {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() || trimmed == "null" || trimmed == "unknown" {
                None
            } else {
                Some(trimmed)
            }
        }
        Err(_) => None,
    }
}

/// Find the first line containing `<key>` followed by `sep` (with `=`
/// also accepted as a fallback separator) and return the first
/// whitespace-delimited token after the separator. Strips surrounding
/// `"` / `'` quotes. Returns `None` if no match or the value is empty.
///
/// The match is strict at the line level: the key must be preceded by
/// line start, whitespace, or a brace/comma/bracket — never by an
/// alphanumeric character (so `EVENT_BT_RSSI_EVENT` is not picked up
/// as `SSID:`). Case-sensitive (use [`key_value_block`] for
/// case-insensitive matching).
pub(crate) fn first_match(out: &str, key: &str, sep: char) -> Option<String> {
    for line in out.lines() {
        let trimmed = line.trim();
        let mut search_from = 0;
        while let Some(rel_idx) = trimmed[search_from..].find(key) {
            let abs_idx = search_from + rel_idx;
            // Strict boundary check: prev must be line start, whitespace,
            // `{`, `,`, or `[`. Mirrors the original `parse_ssid_line`
            // so we don't accidentally match tokens like `BSSID` whose
            // name contains `SSID` as a substring.
            let prev_ok = abs_idx == 0
                || {
                    let prev = trimmed[..abs_idx].chars().last().unwrap_or(' ');
                    prev.is_whitespace() || prev == '{' || prev == ',' || prev == '['
                };
            if !prev_ok {
                search_from = abs_idx + key.len();
                continue;
            }
            let after = &trimmed[abs_idx + key.len()..];
            let after = after.trim_start();
            let after = after
                .strip_prefix(sep)
                .or_else(|| after.strip_prefix('='))
                .unwrap_or(after)
                .trim_start();
            let v = after
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_matches(|c: char| c == '"' || c == '\'')
                .to_string();
            if !v.is_empty() {
                return Some(v);
            }
            search_from = abs_idx + key.len();
        }
    }
    None
}

/// Strip surrounding `{`, `}`, `"`, `'` from the input and return the
/// trimmed inner value. Returns `None` if empty. Designed for
/// structured outputs like `mSimOperatorAlpha={"foo"}` /
/// `mNetworkOperatorName="foo"` where the value may be wrapped in
/// braces or quotes depending on the dumpsys flavour.
pub(crate) fn quoted_value(s: &str) -> Option<String> {
    let trimmed = s.trim();
    let v = trimmed.trim_matches(|c: char| c == '{' || c == '}' || c == '"' || c == '\'');
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

/// Strict key-value extraction. Case-insensitive (covers both
/// `SSID: ...` and `ssid=...`), accepts both `:` and `=` as
/// separators, handles quoted values (`"TP-LINK"`), filters placeholder
/// values (`<unknown ssid>`, `null`, `<none>`, `unknown`, etc.). Use
/// this for fields where false positives are a concern (e.g. SSID:
/// `EVENT_WIFI_SSID_EVENT` would otherwise match a naive substring
/// search).
pub(crate) fn key_value_block(out: &str, key: &str) -> Option<String> {
    let key_lower = key.to_ascii_lowercase();
    for line in out.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();
        let mut search_from = 0;
        while let Some(rel_idx) = lower[search_from..].find(&key_lower) {
            let abs_idx = search_from + rel_idx;
            // Strict boundary check: prev must be line start, whitespace,
            // `{`, `,`, or `[`. Mirrors the original `parse_ssid_line`
            // so we don't accidentally match tokens like `BSSID` whose
            // name contains `SSID` as a substring.
            let prev_ok = abs_idx == 0
                || {
                    let prev = trimmed[..abs_idx].chars().last().unwrap_or(' ');
                    prev.is_whitespace() || prev == '{' || prev == ',' || prev == '['
                };
            if !prev_ok {
                search_from = abs_idx + key.len();
                continue;
            }
            let after = &trimmed[abs_idx + key.len()..];
            let after = after.trim_start();
            let after = after
                .strip_prefix(':')
                .or_else(|| after.strip_prefix('='))
                .unwrap_or(after)
                .trim_start();
            // Quoted value: take everything up to the next quote.
            let value = if let Some(stripped) = after.strip_prefix('"') {
                match stripped.find('"') {
                    Some(end) => &stripped[..end],
                    None => after.split_whitespace().next().unwrap_or(""),
                }
            } else {
                after.split_whitespace().next().unwrap_or("")
            };
            let v = value.trim_matches(|c: char| c == '"' || c == '\'');
            // Reject placeholders + values that look like another key's
            // name (e.g. `"bssid:"` when we're looking for SSID). On
            // some OEM ROMs the privacy-masked SSID line collapses to
            // `SSID: BSSID: aa:bb:cc:dd:ee:ff` and without this filter
            // the SSID slot would pick up the literal string "bssid:"
            // as the value. Rejecting trailing `:` / `=` is a cheap
            // heuristic for "looks like another key". (SSID values that
            // legitimately end in `:` or `=` are exceedingly rare.)
            // Also reject values that contain no alphanumeric character at
            // all — these are always punctuation bleed-through from a
            // nearby placeholder (e.g. `>` from `<unknown ssid>`
            // closing, `]`, `}`, etc.). A real SSID always has at
            // least one letter or digit.
            let has_alnum = v.chars().any(|c| c.is_alphanumeric());
            if !v.is_empty()
                && v != "<unknown ssid>"
                && v != "null"
                && v != "<none>"
                && v != "unknown"
                && !v.starts_with('_')
                && !v.starts_with('[')
                && !v.starts_with('{')
                && !v.starts_with('<')
                && !v.ends_with(':')
                && !v.ends_with('=')
                && has_alnum
            {
                return Some(v.to_string());
            }
            search_from = abs_idx + key.len();
        }
    }
    None
}

/// Parse a single line from `df -h` output. Returns `(Size, Avail)`
/// for lines whose mount is `/data` or `/storage`. `None` otherwise.
///
/// toybox `df -h` layout used by Android:
///   Filesystem Size Used Avail Use% Mounted
/// (size columns are one word — `df` on GNU coreutils can use two
/// words for "Available", but toybox always uses one.)
pub(crate) fn df_columns(line: &str) -> Option<(String, String)> {
    let cols: Vec<&str> = line.split_whitespace().collect();
    let mount = cols.last()?;
    if !mount.contains("/data") && !mount.contains("/storage") {
        return None;
    }
    if cols.len() >= 5 {
        return Some((cols[1].to_string(), cols[3].to_string()));
    }
    None
}
