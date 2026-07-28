use jadb_lib::services::apk_repackager::build_apktool_b_args;

#[test]
fn builds_b_args() {
    let args = build_apktool_b_args("/tmp/src", "/tmp/out.apk");
    assert_eq!(args, vec!["b", "/tmp/src", "-o", "/tmp/out.apk"]);
}

#[test]
fn b_args_subcommand_is_first() {
    let args = build_apktool_b_args("in", "out.apk");
    assert_eq!(args[0], "b");
    // output path is the last argument
    assert_eq!(args[args.len() - 1], "out.apk");
}
