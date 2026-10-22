//! CI test reports in the API: a run's test results and coverage, a commit's
//! failures as annotations (for a pull request's files), and flaky tests.

use axum::Json;
use axum::extract::{Path, Query, State};
use rusqlite::{Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{Value, json};

use super::J;
use crate::AppState;
use crate::auth::Viewer;
use crate::error::AppError;
use crate::repo::Repo;
use crate::validate;

/// Covered share of all coverable lines, in percent (2 decimals), if any job reported coverage.
fn percent(hit: f64, total: f64) -> Value {
    if total > 0.0 { json!((hit / total * 10000.0).round() / 100.0) } else { Value::Null }
}

/// Per-job counts and the run's coverage.
fn summary(c: &Connection, run: i64) -> rusqlite::Result<(Vec<Value>, Value)> {
    let mut q = c.prepare(
        "SELECT j.id, j.name, p.tests, p.failed, p.skipped, p.flaky, p.cov_hit, p.cov_total
         FROM ci_jobs j JOIN ci_job_reports p ON p.job_id = j.id WHERE j.run_id = ?1 ORDER BY j.id",
    )?;
    let (mut hit, mut total) = (0f64, 0f64);
    let jobs = q
        .query_map([run], |r| {
            let (h, t): (Option<f64>, Option<f64>) = (r.get(6)?, r.get(7)?);
            hit += h.unwrap_or(0.0);
            total += t.unwrap_or(0.0);
            Ok(json!({
                "id": r.get::<_, i64>(0)?, "name": r.get::<_, String>(1)?, "tests": r.get::<_, i64>(2)?, "failed": r.get::<_, i64>(3)?,
                "skipped": r.get::<_, i64>(4)?, "flaky": r.get::<_, i64>(5)?, "coverage": percent(h.unwrap_or(0.0), t.unwrap_or(0.0)),
            }))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok((jobs, percent(hit, total)))
}

/// A run's current failed and flaky tests (at most 500).
fn results(c: &Connection, run: i64) -> rusqlite::Result<Vec<Value>> {
    let mut q = c.prepare(
        "SELECT t.job_id, j.name, t.suite, t.name, t.status, t.message, t.file, t.line, t.flaky
         FROM ci_test_results t JOIN ci_jobs j ON j.id = t.job_id WHERE j.run_id = ?1 AND t.current = 1 ORDER BY t.id LIMIT 500",
    )?;
    q.query_map([run], |r| {
        Ok(json!({
            "job_id": r.get::<_, i64>(0)?, "job": r.get::<_, String>(1)?, "suite": r.get::<_, String>(2)?, "name": r.get::<_, String>(3)?,
            "status": r.get::<_, String>(4)?, "message": r.get::<_, String>(5)?, "file": r.get::<_, Option<String>>(6)?,
            "line": r.get::<_, Option<i64>>(7)?, "flaky": r.get::<_, bool>(8)?,
        }))
    })?
    .collect()
}

/// GET …/ci/runs/{n}/tests: `{ coverage, jobs: [counts…], results: [failed and flaky tests] }`.
pub async fn run_tests(State(st): State<AppState>, Viewer(v): Viewer, Path((o, n, number)): Path<(String, String, i64)>) -> J<Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let id = r.id;
    let out = st
        .db
        .call(move |c| {
            let Some(run) = c.query_row("SELECT id FROM ci_runs WHERE repo_id = ?1 AND number = ?2", [id, number], |r| r.get::<_, i64>(0)).optional()? else {
                return Ok(None);
            };
            let (jobs, coverage) = summary(c, run)?;
            Ok(Some(json!({ "coverage": coverage, "jobs": jobs, "results": results(c, run)? })))
        })
        .await?;
    out.map(Json).ok_or_else(AppError::not_found)
}

#[derive(Deserialize)]
pub struct ReportQ {
    sha: String,
}

/// GET …/ci/report?sha=: the newest run of a commit: its counts, coverage, and
/// the failed tests that name a file and line (annotations for its diff).
pub async fn report(State(st): State<AppState>, Viewer(v): Viewer, Path((o, n)): Path<(String, String)>, Query(q): Query<ReportQ>) -> J<Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    if !validate::sha(&q.sha) {
        return Err(AppError::bad("That isn't a commit"));
    }
    let id = r.id;
    let out = st
        .db
        .call(move |c| {
            let run: Option<(i64, i64)> = c
                .query_row("SELECT id, number FROM ci_runs WHERE repo_id = ?1 AND sha = ?2 ORDER BY id DESC LIMIT 1", rusqlite::params![id, q.sha], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?;
            let Some((run, number)) = run else { return Ok(json!({ "run": null, "coverage": null, "tests": 0, "failed": 0, "flaky": 0, "annotations": [] })) };
            let (jobs, coverage) = summary(c, run)?;
            let sum = |k: &str| jobs.iter().map(|j| j[k].as_i64().unwrap_or(0)).sum::<i64>();
            let annotations: Vec<Value> = results(c, run)?
                .into_iter()
                .filter(|t| t["status"] == "failed" && t["file"].is_string() && t["line"].is_i64())
                .take(200)
                .map(|t| {
                    let title = if t["suite"].as_str().unwrap_or("").is_empty() { t["name"].clone() } else { json!(format!("{}::{}", t["suite"].as_str().unwrap_or(""), t["name"].as_str().unwrap_or(""))) };
                    json!({ "path": t["file"], "line": t["line"], "title": title, "message": t["message"], "job": t["job"], "job_id": t["job_id"] })
                })
                .collect();
            Ok(json!({ "run": number, "coverage": coverage, "tests": sum("tests"), "failed": sum("failed"), "flaky": sum("flaky"), "annotations": annotations }))
        })
        .await?;
    Ok(Json(out))
}

/// GET …/ci/flaky: tests that failed and then passed on the same commit, newest first.
pub async fn flaky(State(st): State<AppState>, Viewer(v): Viewer, Path((o, n)): Path<(String, String)>) -> J<Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let id = r.id;
    let tests: Vec<Value> = st
        .db
        .call(move |c| {
            let mut q = c.prepare("SELECT name, flips, last_sha, last_run, last_seen FROM ci_flaky_tests WHERE repo_id = ?1 ORDER BY last_seen DESC, name LIMIT 100")?;
            Ok(q.query_map([id], |r| {
                Ok(json!({ "name": r.get::<_, String>(0)?, "flips": r.get::<_, i64>(1)?, "last_sha": r.get::<_, String>(2)?, "last_run": r.get::<_, i64>(3)?, "last_seen": r.get::<_, i64>(4)? }))
            })?
            .collect::<Result<_, _>>()?)
        })
        .await?;
    Ok(Json(json!({ "tests": tests })))
}
