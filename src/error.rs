use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};

/// An error that maps to an HTTP status and a short JSON message.
#[derive(Debug)]
pub struct AppError {
    pub status: StatusCode,
    pub msg: String,
    /// A machine-readable reason the web app acts on (e.g. "reauth").
    pub code: Option<&'static str>,
}

pub type R<T> = Result<T, AppError>;

impl AppError {
    pub fn new(status: StatusCode, msg: impl Into<String>) -> Self {
        Self {
            status,
            msg: msg.into(),
            code: None,
        }
    }
    pub fn with_code(mut self, code: &'static str) -> Self {
        self.code = Some(code);
        self
    }
    /// The action needs a recent password (and 2FA) confirmation.
    pub fn reauth() -> Self {
        Self::new(StatusCode::FORBIDDEN, "Confirm your password to continue").with_code("reauth")
    }
    pub fn bad(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, msg)
    }
    pub fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "Not found")
    }
    pub fn unauthorized() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "Sign in required")
    }
    pub fn forbidden() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "You don't have permission to do that",
        )
    }
    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, msg)
    }
    pub fn internal(msg: impl std::fmt::Display) -> Self {
        eprintln!("internal error: {msg}");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "Internal error")
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let body = match self.code {
            Some(c) => serde_json::json!({ "error": self.msg, "code": c }),
            None => serde_json::json!({ "error": self.msg }),
        };
        (self.status, Json(body)).into_response()
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        if let rusqlite::Error::SqliteFailure(f, _) = &e
            && f.code == rusqlite::ErrorCode::ConstraintViolation
        {
            return AppError::conflict("That already exists");
        }
        if matches!(e, rusqlite::Error::QueryReturnedNoRows) {
            return AppError::not_found();
        }
        AppError::internal(e)
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::internal(e)
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::internal(e)
    }
}
