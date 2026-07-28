use jadb_lib::services::apk_signer::{build_uber_args, find_signed_apk, format_apk_signed_path};

#[test]
fn builds_args_with_passwords() {
    let args = build_uber_args(
        "/path/uber.jar",
        "/tmp/in.apk",
        "/tmp/ks.jks",
        "storepwd",
        "alias",
        "keypwd",
    );
    assert_eq!(
        args,
        vec![
            "-jar", "/path/uber.jar",
            "--apks", "/tmp/in.apk",
            "--ks", "/tmp/ks.jks",
            "--ksPass", "storepwd",
            "--ksAlias", "alias",
            "--ksKeyPass", "keypwd",
        ]
    );
}

#[test]
fn uber_jar_is_first_arg() {
    let args = build_uber_args("u.jar", "a.apk", "k.jks", "p", "al", "kp");
    assert_eq!(args[1], "u.jar");
    assert_eq!(args[args.len() - 1], "kp");
}

#[test]
fn format_signed_path_matches_uber_release_output() {
    // uber-apk-signer 1.3.0 with --ks writes `<input>-aligned-signed<ext>` next to the input.
    assert_eq!(
        format_apk_signed_path("/tmp/foo/bar.apk"),
        "/tmp/foo/bar-aligned-signed.apk"
    );
    assert_eq!(
        format_apk_signed_path("/tmp/foo/bar"),
        "/tmp/foo/bar-aligned-signed.apk"
    );
    assert_eq!(
        format_apk_signed_path("only.apk"),
        "./only-aligned-signed.apk"
    );
}

#[test]
fn find_signed_apk_returns_predicted_when_present() {
    let dir = tempdir_in_target("predicted");
    let input = dir.join("foo.apk");
    let signed = dir.join("foo-aligned-signed.apk");
    std::fs::write(&input, b"input").unwrap();
    std::fs::write(&signed, b"signed").unwrap();
    let found = find_signed_apk(input.to_str().unwrap());
    assert_eq!(found.as_deref(), Some(signed.to_str().unwrap()));
}

#[test]
fn find_signed_apk_falls_back_to_newest_signed_apk_in_parent() {
    let dir = tempdir_in_target("fallback");
    let input = dir.join("foo.apk");
    std::fs::write(&input, b"input").unwrap();
    // Predicted file is absent, so the fallback path runs.
    // A debug-variant signed file is later in the dir; its mtime is naturally
    // newer than the input's because it was created after.
    let fallback = dir.join("foo-aligned-debugSigned.apk");
    std::fs::write(&fallback, b"new").unwrap();

    let found = find_signed_apk(input.to_str().unwrap());
    assert_eq!(found.as_deref(), Some(fallback.to_str().unwrap()));
}

#[test]
fn find_signed_apk_returns_none_when_nothing_matches() {
    let dir = tempdir_in_target("nomatches");
    let input = dir.join("foo.apk");
    std::fs::write(&input, b"input").unwrap();
    assert_eq!(find_signed_apk(input.to_str().unwrap()), None);
}

// -- test helpers ---------------------------------------------------------

/// Create a unique temp directory inside the Cargo `CARGO_TARGET_TMPDIR` (or `/tmp`
/// fallback) with pid + nanos + label to guarantee uniqueness across parallel tests.
fn tempdir_in_target(label: &str) -> std::path::PathBuf {
    let target = std::env::var("CARGO_TARGET_TMPDIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"));
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    let p = target.join(format!("jadb-{pid}-{stamp}-{label}"));
    std::fs::create_dir_all(&p).unwrap();
    p
}
