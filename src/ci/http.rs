//! The API `gitter runner` talks to. It sits outside /api: runners and jobs
//! carry their own tokens, and logs arrive far more often than the per-address
//! API rate limit allows.

use std::time::{Duration, Instant};

use axum::Json;
use axum::Router;
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{post, put};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::json;
use tokio::io::AsyncWriteExt;

use super::{jobs, queue};
use crate::AppState;
use crate::auth::sha256_hex;
use crate::error::{AppError, R};

/// Runner tokens (made in Site administration → Runners).
pub const RUNNER_TOKEN_PREFIX: &str = "grn_";
/// A claim waits this long for a job before answering 204.
const LONG_POLL: Duration = Duration::from_secs(25);
const LOG_CHUNK_CAP: usize = 1 << 20;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/ci-runner/claim", post(claim))
        .route("/ci-runner/jobs/{id}/heartbeat", post(heartbeat))
        .route("/ci-runner/jobs/{id}/log", post(log))
        .route("/ci-runner/jobs/{id}/steps/{index}/{status}", post(step))
        // Only the artifact upload streams big bodies; everything else keeps axum's 2 MB cap,
        // which applies before the token is checked.
        .route("/ci-runner/jobs/{id}/artifacts", put(artifacts).layer(DefaultBodyLimit::disable()))
        .route("/ci-runner/jobs/{id}/finish", post(finish))
}

fn bearer(h: &HeaderMap) -> Option<String> {
    h.get(header::AUTHORIZATION)?.to_str().ok()?.strip_prefix("Bearer ").map(|t| t.trim().to_string())
}

/// The runner a runner token belongs to.
async fn runner(st: &AppState, h: &HeaderMap) -> R<i64> {
    let token = bearer(h).filter(|t| t.starts_with(RUNNER_TOKEN_PREFIX)).ok_or_else(AppError::unauthorized)?;
    let hash = sha256_hex(&token);
    st.db
        .call(move |c| {
            Ok(rusqlite::OptionalExtension::optional(c.query_row(
                "SELECT id FROM ci_runners WHERE token_hash = ?1 AND builtin = 0",
                [hash],
                |r| r.get(0),
            ))?)
        })
        .await?
        .ok_or_else(AppError::unauthorized)
}

/// The job token, checked to be job `id`'s current attempt (a stale one is refused).
async fn job(st: &AppState, h: &HeaderMap, id: i64) -> R<String> {
    let token = bearer(h).ok_or_else(AppError::unauthorized)?;
    let t = token.clone();
    match st.db.call(move |c| Ok(queue::job_for_token(c, &t)?)).await? {
        Some(j) if j == id => Ok(token),
        _ => Err(AppError::new(StatusCode::FORBIDDEN, "This attempt no longer holds the job")),
    }
}

#[derive(Deserialize)]
struct ClaimReq {
    #[serde(default)]
    labels: Vec<String>,
}

async fn claim(State(st): State<AppState>, h: HeaderMap, Json(req): Json<ClaimReq>) -> R<Response> {
    let id = runner(&st, &h).await?;
    let labels: Vec<String> = req.labels.into_iter().filter(|l| l.len() <= 64).take(32).collect();
    let json = serde_json::to_string(&labels).unwrap_or_default();
    st.db
        .call(move |c| {
            Ok(c.execute(
                "UPDATE ci_runners SET labels = ?2, last_seen = ?3 WHERE id = ?1",
                rusqlite::params![id, json, crate::db::now()],
            )?)
        })
        .await?;
    let until = Instant::now() + LONG_POLL;
    loop {
        // Ask to be woken before looking, so a job queued in between isn't missed.
        let woken = st.ci_wake.notified();
        tokio::pin!(woken);
        woken.as_mut().enable();
        if let Some(lease) = jobs::claim(&st, id, labels.clone(), false).await? {
            return Ok(Json(lease).into_response());
        }
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Ok(StatusCode::NO_CONTENT.into_response());
        }
        let _ = tokio::time::timeout(left.min(Duration::from_secs(5)), woken).await;
    }
}

async fn heartbeat(State(st): State<AppState>, h: HeaderMap, Path(id): Path<i64>) -> R<Json<serde_json::Value>> {
    let token = job(&st, &h, id).await?;
    let ok = jobs::heartbeat(&st, id, &token).await?;
    Ok(Json(json!({ "ok": ok })))
}

async fn log(State(st): State<AppState>, h: HeaderMap, Path(id): Path<i64>, body: Body) -> R<StatusCode> {
    job(&st, &h, id).await?;
    let data = axum::body::to_bytes(body, LOG_CHUNK_CAP).await.map_err(|_| AppError::bad("Send log output in pieces of at most 1 MB"))?;
    jobs::append_log(&st, id, &data).await?;
    Ok(StatusCode::OK)
}

async fn step(State(st): State<AppState>, h: HeaderMap, Path((id, index, status)): Path<(i64, usize, String)>) -> R<StatusCode> {
    job(&st, &h, id).await?;
    jobs::step(&st, id, index, &status).await?;
    Ok(StatusCode::OK)
}

async fn artifacts(State(st): State<AppState>, h: HeaderMap, Path(id): Path<i64>, body: Body) -> R<StatusCode> {
    job(&st, &h, id).await?;
    let dir = st.cfg.data_dir.join("ci/tmp");
    tokio::fs::create_dir_all(&dir).await?;
    let tmp = dir.join(format!("{id}.{}", crate::db::now()));
    let res = async {
        let mut f = tokio::fs::File::create(&tmp).await?;
        let (mut stream, mut n) = (body.into_data_stream(), 0u64);
        while let Some(chunk) = tokio::time::timeout(Duration::from_secs(60), stream.next()).await.map_err(|_| AppError::bad("The upload stalled"))? {
            let chunk = chunk.map_err(|_| AppError::bad("The upload was interrupted"))?;
            n += chunk.len() as u64;
            if n > jobs::ARTIFACT_CAP {
                return Err(AppError::bad("Artifacts are limited to 1 GB per job"));
            }
            f.write_all(&chunk).await?;
        }
        f.sync_all().await?;
        jobs::store_artifact(&st, id, &tmp).await
    }
    .await;
    let _ = tokio::fs::remove_file(&tmp).await;
    res.map(|_| StatusCode::OK)
}

#[derive(Deserialize)]
struct FinishReq {
    success: bool,
    exit_code: Option<i32>,
}

async fn finish(State(st): State<AppState>, h: HeaderMap, Path(id): Path<i64>, body: Body) -> R<StatusCode> {
    let token = job(&st, &h, id).await?;
    let data = axum::body::to_bytes(body, 4096).await.map_err(|_| AppError::bad("Bad result"))?;
    let req: FinishReq = serde_json::from_slice(&data).map_err(|_| AppError::bad("Bad result"))?;
    jobs::finish(&st, id, &token, req.success, req.exit_code).await?;
    Ok(StatusCode::OK)
}
