use jadb_lib::services::apk_decompiler::build_apktool_args;

#[test]
fn builds_args_with_force_flag() {
    let args = build_apktool_args("/tmp/in.apk", "/tmp/out", true);
    assert_eq!(args, vec!["d", "/tmp/in.apk", "-o", "/tmp/out", "-f"]);
}

#[test]
fn builds_args_without_force() {
    let args = build_apktool_args("/tmp/in.apk", "/tmp/out", false);
    assert_eq!(args, vec!["d", "/tmp/in.apk", "-o", "/tmp/out"]);
}

#[test]
fn args_in_expected_order() {
    let args = build_apktool_args("a.apk", "out", false);
    // First arg is "d" (subcommand), then input, then -o, then out.
    assert_eq!(args[0], "d");
    assert_eq!(args[args.len() - 1], "out");
}
