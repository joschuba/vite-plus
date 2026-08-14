/// Error type for documentation-provider operations.
///
/// `UserMessage` carries a complete user-facing diagnostic; callers render
/// it behind their own `error:` prefix, matching the `vp_pm_cli` pattern.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    UserMessage(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("{0}")]
    Io(#[from] std::io::Error),
}

pub(crate) fn user_message(message: impl Into<String>) -> Error {
    Error::UserMessage(message.into())
}
