# Changelog

All notable changes to JADB are documented in this file.

## [0.1.0] - 2026-07-20

### Added
- Initial release (macOS only)
- APK information analysis (aapt2 dump badging)
- Apktool decompile + repackage
- uber-apk-signer signing + signature management (plaintext JSON, chmod 600)
- JADX decompilation
- LibChecker rule analysis (permission / component / sdk / manifest)
- Bilingual UI (zh-CN / en) following system theme
- Toolchain auto-download (apktool / uber-apk-signer / jadx / aapt2)
- Long-running task panel with progress + cancel

## Unreleased

### Added
- **LibChecker-Rules GitHub integration**: Settings → Tools card now has an
  "Install LibChecker Rules (GitHub)" button that fetches the latest rules
  from `LibChecker/LibChecker-Rules`, converts them to JADB rule-pack format,
  and installs 7 packs (native libraries, activities, services, receivers,
  providers, intent actions, static libraries) covering 2000+ known Android
  libraries and SDKs.
- New rule kinds: `native_library` (matches `.so` filenames), `component_class`
  (matches Activity/Service/Receiver/Provider FQNs), `action` (matches intent
  actions). The rules engine now inspects APK contents beyond the manifest.
- `ApkInfo` exposes `native_libs` (basename list from APK `lib/**/*.so`) and
  `intent_actions` (parsed from `aapt2 dump badging`).
- `Rule` accepts optional `metadata` for richer UI rendering (label, dev team,
  source link, zh description).

### Changed
- Rule pack status now includes `libcheckerVersion` and `libcheckerCommit`
  when installed from GitHub. Source can be `bundled`, `server`, or `libchecker`.
- Toolchain: bumped bundled JADX from 1.5.3 to 1.5.6 (latest release on
  `skylot/jadx`). Download URL, file name, `unzip_dir`, doc comments in
  `commands/jadx.rs`, Rust test fixtures in `tests/tool_manager.rs`, the
  Vitest mock in `__tests__/toolsStore.test.ts`, and the design spec were
  updated to match.

### Fixed
- Toolchain: `extract_zip` (in `services/tool_manager.rs`) silently
  aborted extraction for any archive whose first central-directory entry
  did not contain a `/`. The `strip_prefix` pre-check used
  `return Ok(())` to exit the spawn-blocking closure, which skipped the
  actual file-write loop. JADX 1.5.3 / 1.5.6 ship a flat layout
  (`LICENSE`, `README.md`, `bin/`, `lib/` at the top), so this branch
  always fired and JADX installation appeared to "succeed" but left the
  install dir empty. Replaced with an immediately-invoked closure that
  returns `Option<PathBuf>`; the extraction loop now runs regardless of
  whether a shared top-level prefix was detected. The two existing
  `extract_zip_*` unit tests still pass.

### Changed
- `launch_jadx_gui` (in `commands/jadx.rs`) now:
  - Spawns `bin/jadx-gui` under `nohup` on macOS / Linux so closing
    JADB does not hand SIGHUP to jadx-gui's Swing runtime. Windows
    keeps the direct `Command::new(&bin)` path because the .bat
    launcher already detaches on its own.
  - Forwards `-Pdex-input.verify-checksum=no` to the launcher so
    re-packed / re-signed APKs (whose original dex header CRC no
    longer matches) decode cleanly instead of erroring out at the
    dex-input validator.
  Stdio / `JADX_GUI_OPTS` / locale env vars are unchanged.
