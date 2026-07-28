#![allow(unused_imports)]
// Both `use`s feed into the #[ignore]'d debug helper below; suppress the
// unused warning so CI's lib-test target stays silent.
use std::io::Read;
use flate2::read::GzDecoder;

// Manual debug helper: prints the first 30 paths inside `/tmp/lb.tar.gz`.
// Requires the operator to manually drop a LibChecker tarball there.
// Not part of CI; ignored by default so `cargo test` stays green.
#[test]
#[ignore]
fn iterate_real_tarball() {
    let bytes = std::fs::read("/tmp/lb.tar.gz").expect("read tarball");
    let gz = GzDecoder::new(&bytes[..]);
    let mut archive = tar::Archive::new(gz);
    let mut count = 0;
    let mut paths = Vec::new();
    for entry in archive.entries().unwrap() {
        let entry = entry.unwrap();
        let path = entry.path().unwrap().to_path_buf();
        paths.push(path.to_string_lossy().to_string());
        count += 1;
        if count >= 30 {
            break;
        }
    }
    eprintln!("first 30 paths:");
    for p in &paths {
        eprintln!("  {}", p);
    }
    eprintln!("total iterated before break: {}", count);
}
