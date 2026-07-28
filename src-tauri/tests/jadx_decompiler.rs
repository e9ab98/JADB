use jadb_lib::services::jadx_decompiler::{build_args, JadxOptions};

#[test]
fn builds_minimal_args() {
    let opts = JadxOptions::default();
    let args = build_args("/opt/jadx", "/tmp/in.apk", "/tmp/out", &opts);
    assert!(args.contains(&"-d".to_string()));
    assert!(args.contains(&"/tmp/in.apk".to_string()));
    assert!(args.contains(&"/tmp/out".to_string()));
    assert!(!args.contains(&"--no-res".to_string()));
    assert!(!args.contains(&"--no-debug-info".to_string()));
    assert!(!args.contains(&"--export-gradle".to_string()));
}

#[test]
fn builds_args_with_options() {
    let opts = JadxOptions {
        show_in_gradle: true,
        decompile_resources: false,
        debug_info: false,
        export_as_gradle: true,
        threads_count: Some(4),
    };
    let args = build_args("/opt/jadx", "/tmp/in.apk", "/tmp/out", &opts);
    assert!(args.contains(&"--no-res".to_string()));
    assert!(args.contains(&"--no-debug-info".to_string()));
    assert!(args.contains(&"--export-gradle".to_string()));
    assert!(args.contains(&"--show-gradle-root".to_string()));
    assert!(args.contains(&"--threads-count".to_string()));
    assert!(args.contains(&"4".to_string()));
    assert_eq!(args[0], "-d");
    assert_eq!(args[1], "/tmp/out");
    assert_eq!(args[2], "/tmp/in.apk");
}
