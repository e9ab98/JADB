use jadb_lib::commands::system::latest_file_in;
use std::fs;
use std::time::Duration;

#[test]
fn picks_newest_file() {
    let dir = std::env::temp_dir().join(format!("jadb-log-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    // Write older first, sleep briefly, then newer so its mtime is strictly later.
    let older = dir.join("jadb-2026-01-01.log");
    fs::write(&older, b"old").unwrap();
    std::thread::sleep(Duration::from_millis(50));
    let newer = dir.join("jadb-2026-07-21.log");
    fs::write(&newer, b"new").unwrap();

    let picked = latest_file_in(&dir).unwrap();
    assert!(picked.ends_with("jadb-2026-07-21.log"), "got {}", picked.display());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn empty_dir_returns_itself() {
    let dir = std::env::temp_dir().join(format!("jadb-log-empty-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let picked = latest_file_in(&dir).unwrap();
    assert_eq!(picked.to_string_lossy(), dir.to_string_lossy());
    let _ = fs::remove_dir_all(&dir);
}
