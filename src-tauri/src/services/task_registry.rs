use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub struct TaskRegistry {
    map: Arc<Mutex<HashMap<String, CancellationToken>>>,
}

impl TaskRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, id: &str) -> CancellationToken {
        let token = CancellationToken::new();
        self.map
            .lock()
            .expect("TaskRegistry mutex poisoned")
            .insert(id.to_string(), token.clone());
        token
    }

    pub fn cancel(&self, id: &str) -> bool {
        if let Ok(map) = self.map.lock() {
            if let Some(t) = map.get(id) {
                t.cancel();
                return true;
            }
        }
        false
    }

    pub fn complete(&self, id: &str) {
        if let Ok(mut map) = self.map.lock() {
            map.remove(id);
        }
    }

    pub fn is_cancelled(&self, id: &str) -> bool {
        self.map
            .lock()
            .ok()
            .and_then(|m| m.get(id).map(|t| t.is_cancelled()))
            .unwrap_or(false)
    }
}
