//! The Security tab: secrets push protection refused (and allowing one once),
//! the on-demand secret scan, dependencies, vulnerabilities, SBOM downloads,
//! and the site admin's OSV import.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};

use super::admin::Admin;
use super::{ClientIp, J};
use crate::AppState;
use crate::auth::{Authed, Viewer};
use crate::db::now;
use crate::error::AppError;
use crate::repo::Repo;
use crate::scan::{ALLOW_FOR, Finding};
use crate::security::{Event, record};

const REASONS: &[(&str, &str)] = &[("false_positive", "a false positive"), ("test", "a test value"), ("later", "will be revoked")];

/// What the Security tab shows the viewer.
pub async fn overview(State(st): State<AppState>, Viewer(v): Viewer, Path((o, n)): Path<(String, String)>) -> J<Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let (id, uid, admin) = (r.id, v.as_ref().map(|u| u.id), r.can_admin);
    let (blocked, scan) = st
        .db
        .call(move |c| {
            let mut blocked = Vec::new();
            if let Some(uid) = uid {
                // The latest refusal of each finding this week, and whether it's allowed now.
                let mut s = c.prepare(
                    "SELECT b.fingerprint, b.kind, b.path, b.line, b.masked, b.commit_sha, max(b.created_at),
                            EXISTS (SELECT 1 FROM secret_bypasses p WHERE p.repo_id = b.repo_id AND p.user_id = b.user_id AND p.fingerprint = b.fingerprint AND p.used_at IS NULL AND p.expires_at > ?3)
                     FROM secret_blocks b WHERE b.repo_id = ?1 AND b.user_id = ?2 AND b.created_at > ?3 - 7 * 86400
                     GROUP BY b.fingerprint ORDER BY max(b.created_at) DESC LIMIT 100",
                )?;
                blocked = s
                    .query_map(rusqlite::params![id, uid, now()], |r| {
                        Ok(json!({
                            "fingerprint": r.get::<_, String>(0)?, "kind": r.get::<_, String>(1)?, "path": r.get::<_, String>(2)?,
                            "line": r.get::<_, i64>(3)?, "masked": r.get::<_, String>(4)?, "commit": r.get::<_, String>(5)?,
                            "created_at": r.get::<_, i64>(6)?, "allowed": r.get::<_, bool>(7)?,
                        }))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
            }
            let scan = if admin { last_scan(c, id)? } else { Value::Null };
            Ok((blocked, scan))
        })
        .await?;
    Ok(Json(json!({ "can_admin": r.can_admin, "can_write": r.can_write, "blocked": blocked, "scan": scan, "reasons": REASONS.iter().map(|(k, l)| json!({ "id": k, "label": l })).collect::<Vec<_>>() })))
}

fn last_scan(c: &rusqlite::Connection, repo_id: i64) -> rusqlite::Result<Value> {
    use rusqlite::OptionalExtension;
    Ok(c.query_row("SELECT commit_sha, scanned_at, truncated, findings FROM secret_scans WHERE repo_id = ?1", [repo_id], |r| {
        let findings: Value = serde_json::from_str(&r.get::<_, String>(3)?).unwrap_or_else(|_| json!([]));
        Ok(json!({ "commit": r.get::<_, String>(0)?, "scanned_at": r.get::<_, i64>(1)?, "truncated": r.get::<_, bool>(2)?, "findings": findings }))
    })
    .optional()?
    .unwrap_or(Value::Null))
}

/// Scan the default branch for secrets now (repository admins).
pub async fn scan(State(st): State<AppState>, Authed(u): Authed, Path((o, n)): Path<(String, String)>) -> J<Value> {
    let r = Repo::administrable(&st, Some(&u), &o, &n).await?;
    let sha = st
        .git
        .resolve(&r.path, &format!("refs/heads/{}", r.default_branch))
        .await?
        .ok_or_else(|| AppError::bad("This repository has no commits to scan yet"))?;
    let (findings, more, cut) = crate::scan::scan_commit(&st, &r, &sha).await?;
    let id = r.id;
    let text = serde_json::to_string(&findings).map_err(AppError::internal)?;
    let scan = st
        .db
        .call(move |c| {
            c.execute(
                "INSERT INTO secret_scans (repo_id, commit_sha, scanned_at, truncated, findings) VALUES (?1,?2,?3,?4,?5)
                 ON CONFLICT(repo_id) DO UPDATE SET commit_sha = ?2, scanned_at = ?3, truncated = ?4, findings = ?5",
                rusqlite::params![id, sha, now(), cut || more > 0, text],
            )?;
            Ok(last_scan(c, id)?)
        })
        .await?;
    Ok(Json(json!({ "scan": scan })))
}

#[derive(Deserialize)]
pub struct AllowReq {
    fingerprints: Vec<String>,
    reason: String,
}

/// Allow secrets push protection refused, once each, for the next push or
/// commit by the same person (within ALLOW_FOR).
pub async fn allow(State(st): State<AppState>, Authed(u): Authed, ip: ClientIp, Path((o, n)): Path<(String, String)>, Json(req): Json<AllowReq>) -> J<Value> {
    let r = Repo::writable(&st, Some(&u), &o, &n).await?;
    if u.agent().is_some() {
        return Err(AppError::new(axum::http::StatusCode::FORBIDDEN, "A person has to allow a secret, not an agent"));
    }
    let Some((reason, label)) = REASONS.iter().find(|(k, _)| *k == req.reason) else {
        return Err(AppError::bad("Say why: false_positive, test or later"));
    };
    if req.fingerprints.is_empty() || req.fingerprints.len() > crate::scan::MAX_FINDINGS || req.fingerprints.iter().any(|f| f.len() != 20 || !f.bytes().all(|b| b.is_ascii_hexdigit())) {
        return Err(AppError::bad("Name the findings to allow"));
    }
    let (id, uid, fps, reason) = (r.id, u.id, req.fingerprints, reason.to_string());
    let found: Vec<Finding> = st
        .db
        .call(move |c| {
            let tx = c.transaction()?;
            let t = now();
            let mut found = Vec::new();
            for fp in &fps {
                let f = tx.query_row(
                    "SELECT kind, path, line, masked, commit_sha FROM secret_blocks WHERE repo_id = ?1 AND user_id = ?2 AND fingerprint = ?3 AND created_at > ?4 ORDER BY id DESC LIMIT 1",
                    rusqlite::params![id, uid, fp, t - 7 * 86400],
                    |r| Ok(Finding { kind: r.get(0)?, path: r.get(1)?, line: r.get::<_, i64>(2)? as usize, masked: r.get(3)?, fingerprint: fp.clone(), commit: r.get(4)? }),
                );
                let f = match f {
                    Ok(f) => f,
                    Err(rusqlite::Error::QueryReturnedNoRows) => return Err(AppError::not_found().with_code("no_such_finding")),
                    Err(e) => return Err(e.into()),
                };
                tx.execute(
                    "INSERT INTO secret_bypasses (repo_id, user_id, fingerprint, reason, created_at, expires_at, used_at) VALUES (?1,?2,?3,?4,?5,?6,NULL)
                     ON CONFLICT(repo_id, user_id, fingerprint) DO UPDATE SET reason = ?4, created_at = ?5, expires_at = ?6, used_at = NULL",
                    rusqlite::params![id, uid, fp, reason, t, t + ALLOW_FOR],
                )?;
                found.push(f);
            }
            tx.commit()?;
            Ok(found)
        })
        .await?;
    for f in &found {
        let detail = format!("{}: {} in {}:{} ({}), {label}", r.full_name(), f.kind, f.path, f.line, f.masked);
        record(&st.db, Event { actor: Some(&u), subject: Some(r.owner_id), action: "secret_allowed", detail, ip: &ip.0 }).await;
    }
    Ok(Json(json!({ "allowed": found.len(), "expires_in": ALLOW_FOR })))
}

/// The default branch's dependencies and the vulnerabilities known for them.
pub async fn dependencies(State(st): State<AppState>, Viewer(v): Viewer, Path((o, n)): Path<(String, String)>) -> J<Value> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let advisories: i64 = st.db.call(|c| Ok(c.query_row("SELECT count(*) FROM osv_vulns", [], |r| r.get(0))?)).await?;
    let Some(sha) = st.git.resolve(&r.path, &format!("refs/heads/{}", r.default_branch)).await? else {
        return Ok(Json(json!({ "commit": null, "branch": r.default_branch, "files": [], "dependencies": [], "vulnerabilities": [], "osv": { "advisories": advisories } })));
    };
    let d = crate::deps::of_commit(&st, &r, &sha).await?;
    let vulns = if advisories > 0 { crate::deps::vulnerabilities(&st, d.deps.clone()).await? } else { Vec::new() };
    Ok(Json(json!({
        "commit": sha, "branch": r.default_branch, "files": d.files, "dependencies": d.deps,
        "vulnerabilities": vulns, "osv": { "advisories": advisories },
    })))
}

#[derive(Deserialize)]
pub struct SbomQ {
    format: Option<String>,
}

/// The default branch's SBOM as a download: `?format=spdx` (SPDX 2.3) or `cyclonedx` (1.5), JSON.
pub async fn sbom(State(st): State<AppState>, Viewer(v): Viewer, Path((o, n)): Path<(String, String)>, Query(q): Query<SbomQ>) -> Result<Response, AppError> {
    let r = Repo::readable(&st, v.as_ref(), &o, &n).await?;
    let spdx = match q.format.as_deref().unwrap_or("spdx") {
        "spdx" => true,
        "cyclonedx" => false,
        _ => return Err(AppError::bad("format is spdx or cyclonedx")),
    };
    let sha = st
        .git
        .resolve(&r.path, &format!("refs/heads/{}", r.default_branch))
        .await?
        .ok_or_else(|| AppError::bad("This repository has no commits yet"))?;
    let d = crate::deps::of_commit(&st, &r, &sha).await?;
    let full = r.full_name();
    let (doc, ext) = if spdx {
        (crate::deps::spdx(&st.cfg.url, &full, &sha, &d.deps, now()), "spdx.json")
    } else {
        (crate::deps::cyclonedx(&full, &sha, &d.deps, now()), "cdx.json")
    };
    let file = format!("{}-{}-{}.{ext}", r.owner, r.name, &sha[..7]);
    Ok((
        [(header::CONTENT_TYPE, "application/json".to_string()), (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{file}\""))],
        serde_json::to_vec_pretty(&doc).map_err(AppError::internal)?,
    )
        .into_response())
}

/// How many OSV advisories are loaded.
pub async fn osv_status(State(st): State<AppState>, Admin(_): Admin) -> J<Value> {
    let (n, newest): (i64, Option<String>) = st.db.call(|c| Ok(c.query_row("SELECT count(*), max(modified) FROM osv_vulns", [], |r| Ok((r.get(0)?, r.get(1)?)))?)).await?;
    Ok(Json(json!({ "advisories": n, "newest": newest })))
}

/// Import an OSV snapshot: the request body is one OSV JSON record, a JSON array of them, or a zip of them.
pub async fn osv_import(State(st): State<AppState>, Admin(u): Admin, ip: ClientIp, body: Bytes) -> J<Value> {
    let list = tokio::task::spawn_blocking(move || crate::deps::read_snapshot(&body)).await.map_err(AppError::internal)?.map_err(AppError::bad)?;
    let (imported, withdrawn) = crate::deps::import(&st, list).await?;
    let detail = format!("{imported} imported, {withdrawn} withdrawn");
    record(&st.db, Event { actor: Some(&u), subject: None, action: "osv_imported", detail, ip: &ip.0 }).await;
    Ok(Json(json!({ "imported": imported, "withdrawn": withdrawn })))
}
