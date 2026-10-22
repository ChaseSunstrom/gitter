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
    /// Seconds until a rate limit lifts (sent as Retry-After).
    pub retry_after: Option<i64>,
    /// The repository asked for was renamed or transferred (see `Moved`).
    pub moved: Option<Box<Moved>>,
}

/// A repository's old and new `(owner, name)`. Rides on a 404 as a response
/// extension; `web::follow_moved` turns it into a redirect when the request
/// path names the old place, and otherwise it stays a plain 404.
#[derive(Clone, Debug)]
pub struct Moved {
    pub from: (String, String),
    pub to: (String, String),
}

pub type R<T> = Result<T, AppError>;

impl AppError {
    pub fn new(status: StatusCode, msg: impl Into<String>) -> Self {
        Self {
            status,
            msg: msg.into(),
            code: None,
            retry_after: None,
            moved: None,
        }
    }
    pub fn moved(from: (&str, &str), to: (&str, &str)) -> Self {
        let mut e = Self::not_found();
        e.moved = Some(Box::new(Moved {
            from: (from.0.to_string(), from.1.to_string()),
            to: (to.0.to_string(), to.1.to_string()),
        }));
        e
    }
    /// 429 with a Retry-After header.
    pub fn slow_down(msg: impl Into<String>, retry_after: i64) -> Self {
        let mut e = Self::new(StatusCode::TOO_MANY_REQUESTS, msg).with_code("rate_limited");
        e.retry_after = Some(retry_after.max(1));
        e
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
        let mut r = (self.status, Json(body)).into_response();
        if let Some(s) = self.retry_after
            && let Ok(v) = axum::http::HeaderValue::from_str(&s.to_string())
        {
            r.headers_mut().insert(axum::http::header::RETRY_AFTER, v);
        }
        if let Some(m) = self.moved {
            r.extensions_mut().insert(*m);
        }
        r
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
