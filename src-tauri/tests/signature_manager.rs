use jadb_lib::commands::signatures::SignatureConfig;
use jadb_lib::services::signature_manager::{apply_patch, generate_id};

fn sample() -> SignatureConfig {
    SignatureConfig {
        id: "x".into(),
        label: "Old".into(),
        keystore_path: "/ks".into(),
        keystore_password: "p".into(),
        key_alias: "a".into(),
        key_password: "kp".into(),
        created_at: "2026-01-01T00:00:00Z".into(),
    }
}

#[test]
fn generate_id_is_uuid_like() {
    let a = generate_id();
    let b = generate_id();
    assert_ne!(a, b);
    assert!(a.contains('-'), "id should contain dashes: {a}");
    assert!(a.len() >= 32, "uuid v4 should be >=32 chars: {a}");
}

#[test]
fn apply_patch_updates_only_present_fields() {
    let mut s = sample();
    let patch = SignatureConfig {
        id: "x".into(),
        label: "New".into(),
        keystore_path: "/ks".into(),
        keystore_password: "p".into(),
        key_alias: "a".into(),
        key_password: "kp".into(),
        created_at: "2026-01-01T00:00:00Z".into(),
    };
    apply_patch(&mut s, &patch);
    assert_eq!(s.label, "New");
    assert_eq!(s.key_alias, "a");
    assert_eq!(s.keystore_path, "/ks");
}

#[test]
fn apply_patch_with_empty_string_keeps_existing() {
    let mut s = sample();
    let patch = SignatureConfig {
        id: "x".into(),
        label: "".into(), // empty → no change
        keystore_path: "/new".into(),
        keystore_password: "newp".into(),
        key_alias: "".into(),
        key_password: "".into(),
        created_at: "".into(),
    };
    apply_patch(&mut s, &patch);
    assert_eq!(s.label, "Old", "empty label should not overwrite");
    assert_eq!(s.key_alias, "a", "empty alias should not overwrite");
    assert_eq!(s.keystore_path, "/new", "non-empty path overwrites");
    assert_eq!(s.keystore_password, "newp");
    assert_eq!(s.key_password, "kp", "empty key_password should not overwrite");
}
