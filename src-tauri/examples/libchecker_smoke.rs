//! One-off smoke binary: feed a real LibChecker-Rules codeload tarball
//! through `install_libchecker_from_archive` and print what the converter
//! produces, plus the staging/output tree the function actually wrote.
//!
//! Built only via `cargo run --example libchecker_smoke -- <path.tar.gz>`
//! from `src-tauri/`. Useful for diagnosing "I clicked Install LibChecker
//! Rules (GitHub) and only got the bundled starter" — the smoke binary
//! bypasses Tauri + the network + the bundle so we can isolate whether
//! the archive-to-RuleSet pipeline is at fault.
//!
//! NOT a unit test; this is a developer-facing diagnostic. Safe to delete
//! once the upstream layout is verified end-to-end.

use std::io::Read;
use std::path::Path;

use jadb_lib::services::rule_pack::install_libchecker_from_archive;

fn walk_tree(label: &str, root: &Path, max_depth: usize) {
    fn inner(p: &Path, depth: usize, max_depth: usize) {
        if depth > max_depth {
            return;
        }
        let Ok(rd) = std::fs::read_dir(p) else {
            return;
        };
        let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        for path in entries {
            let tag = if path.is_dir() { "DIR " } else { "FILE" };
            println!("    {}{}", "  ".repeat(depth), tag);
            println!("    {}{}", "  ".repeat(depth), path.display());
            if path.is_dir() {
                inner(&path, depth + 1, max_depth);
            }
        }
    }
    println!("=== {label}: {} ===", root.display());
    if !root.exists() {
        println!("    (does not exist — install_libchecker_from_archive did not populate it)");
        return;
    }
    inner(root, 0, max_depth);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: libchecker_smoke <tarball.tar.gz>");
        std::process::exit(2);
    }
    // Read the .tar.gz file and gunzip it before feeding the bytes to
    // install_libchecker_from_archive. The production `install_libchecker`
    // does the same gunzip step inside its async block; this smoke
    // binary mirrors that path so we can debug the archive stage in
    // isolation.
    let gz = match std::fs::read(&args[1]) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("read {} failed: {}", args[1], e);
            std::process::exit(2);
        }
    };
    let mut decoder = flate2::read::GzDecoder::new(&gz[..]);
    let mut bytes = Vec::new();
    if let Err(e) = decoder.read_to_end(&mut bytes) {
        eprintln!("gunzip failed: {e}");
        std::process::exit(2);
    }
    println!(
        "tarball: {} ({} compressed -> {} raw tar)",
        args[1],
        gz.len(),
        bytes.len()
    );

    let out = std::env::temp_dir().join("libchecker-smoke-out");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).expect("mkdir out");
    println!();
    match install_libchecker_from_archive(&bytes, &out) {
        Ok(status) => {
            println!("install_libchecker_from_archive OK");
            println!("  packs: {}", status.packs.len());
            for p in &status.packs {
                println!("    - {:<30} ({} rules)", p.id, p.rule_count);
            }
            println!("  total_rules: {}", status.total_rules);
            println!("  source: {:?}", status.source);
            println!("  commit_short: {:?}", status.libchecker_commit);
            println!("  version: {:?}", status.libchecker_version);
        }
        Err(e) => println!("install_libchecker_from_archive ERROR: {e}"),
    }
    println!();

    // The staging temp dir is named jadb-libchecker-staging-<uuid>; print
    // whichever one is left over so we can sanity check the prefix-locked
    // wrapper dir + whether native-libs/etc ever landed there.
    let mut stagings: Vec<_> = std::fs::read_dir(std::env::temp_dir())
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|s| s.to_str())
                        .map(|s| s.starts_with("jadb-libchecker-staging-"))
                        .unwrap_or(false)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    stagings.sort();
    for st in &stagings {
        walk_tree("staging", st, 3);
        println!();
    }
    walk_tree("output (dest)", &out, 3);
}
