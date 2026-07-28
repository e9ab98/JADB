use std::io::Read;
use flate2::read::GzDecoder;

#[test]
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
