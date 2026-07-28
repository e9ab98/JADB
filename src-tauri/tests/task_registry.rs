use jadb_lib::services::task_registry::TaskRegistry;

#[test]
fn register_returns_a_token_that_is_not_cancelled() {
    let reg = TaskRegistry::new();
    let token = reg.register("t1");
    assert!(!token.is_cancelled());
    assert!(!reg.is_cancelled("t1"));
}

#[test]
fn cancel_marks_token_and_clears_registry() {
    let reg = TaskRegistry::new();
    let token = reg.register("t2");
    assert!(reg.cancel("t2"));
    assert!(token.is_cancelled());
    assert!(reg.is_cancelled("t2"));
}

#[test]
fn cancel_unknown_id_returns_false() {
    let reg = TaskRegistry::new();
    assert!(!reg.cancel("never-registered"));
}

#[test]
fn complete_removes_from_registry() {
    let reg = TaskRegistry::new();
    reg.register("t3");
    reg.complete("t3");
    assert!(!reg.cancel("t3"));
}
