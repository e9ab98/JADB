use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("io: {0}")] Io(#[from] std::io::Error),
    #[error("tool missing: {0}")] ToolMissing(String),
    #[error("tool failed: {tool} (exit={code}): {msg}")] ToolFailed {
        tool: String,
        code: i32,
        msg: String,
    },
    #[error("config: {0}")] Config(String),
    #[error("parse: {0}")] Parse(String),
    #[error("task not found: {0}")] TaskNotFound(String),
    #[error("task cancelled")] TaskCancelled,
    #[error("invalid input: {0}")] InvalidInput(String),
    #[error("not found: {0}")] NotFound(String),
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
