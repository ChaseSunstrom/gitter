use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::error::{AppError, R};

/// SQLite behind a mutex. Every call runs on the blocking pool so the async
/// runtime never waits on disk.
#[derive(Clone)]
pub struct Db(Arc<Mutex<Connection>>);

/// Each entry upgrades the schema by one version. Append only.
const MIGRATIONS: &[&str] = &[
    // v1: core
    r#"
    CREATE TABLE users (
        id INTEGER PRIMARY KEY,
        username TEXT NOT NULL UNIQUE COLLATE NOCASE,
        email TEXT NOT NULL DEFAULT '',
        pass_hash TEXT NOT NULL,
        display_name TEXT NOT NULL DEFAULT '',
        bio TEXT NOT NULL DEFAULT '',
        location TEXT NOT NULL DEFAULT '',
        website TEXT NOT NULL DEFAULT '',
        is_admin INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL
    );
    CREATE UNIQUE INDEX users_email ON users(email COLLATE NOCASE) WHERE email != '';
    CREATE TABLE sessions (
        token_hash TEXT PRIMARY KEY,
        user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        created_at INTEGER NOT NULL,
        expires_at INTEGER NOT NULL
    );
    CREATE TABLE tokens (
        id INTEGER PRIMARY KEY,
        user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        token_hash TEXT NOT NULL UNIQUE,
        created_at INTEGER NOT NULL,
        last_used INTEGER
    );
    CREATE TABLE repos (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        owner_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        name TEXT NOT NULL COLLATE NOCASE,
        description TEXT NOT NULL DEFAULT '',
        website TEXT NOT NULL DEFAULT '',
        is_private INTEGER NOT NULL DEFAULT 0,
        default_branch TEXT NOT NULL DEFAULT 'main',
        forked_from INTEGER REFERENCES repos(id) ON DELETE SET NULL,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        pushed_at INTEGER,
        language TEXT NOT NULL DEFAULT '',
        UNIQUE(owner_id, name)
    );
    CREATE TABLE stars (
        user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        created_at INTEGER NOT NULL,
        PRIMARY KEY(user_id, repo_id)
    );
    CREATE TABLE issues (
        id INTEGER PRIMARY KEY,
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        number INTEGER NOT NULL,
        author_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        title TEXT NOT NULL,
        body TEXT NOT NULL DEFAULT '',
        is_pr INTEGER NOT NULL DEFAULT 0,
        state TEXT NOT NULL DEFAULT 'open',
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        closed_at INTEGER,
        UNIQUE(repo_id, number)
    );
    CREATE INDEX issues_repo_state ON issues(repo_id, is_pr, state);
    CREATE TABLE pulls (
        issue_id INTEGER PRIMARY KEY REFERENCES issues(id) ON DELETE CASCADE,
        head_branch TEXT NOT NULL,
        base_branch TEXT NOT NULL,
        head_sha TEXT,
        base_sha TEXT,
        merged_sha TEXT,
        merged_by INTEGER REFERENCES users(id) ON DELETE SET NULL,
        merged_at INTEGER
    );
    CREATE TABLE comments (
        id INTEGER PRIMARY KEY,
        issue_id INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
        author_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        kind TEXT NOT NULL DEFAULT 'comment',
        body TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );
    CREATE INDEX comments_issue ON comments(issue_id);
    CREATE TABLE labels (
        id INTEGER PRIMARY KEY,
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        color TEXT NOT NULL,
        UNIQUE(repo_id, name)
    );
    CREATE TABLE issue_labels (
        issue_id INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
        label_id INTEGER NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
        PRIMARY KEY(issue_id, label_id)
    );
    CREATE TABLE events (
        id INTEGER PRIMARY KEY,
        user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        repo_id INTEGER REFERENCES repos(id) ON DELETE CASCADE,
        kind TEXT NOT NULL,
        title TEXT NOT NULL DEFAULT '',
        target TEXT NOT NULL DEFAULT '',
        count INTEGER NOT NULL DEFAULT 1,
        created_at INTEGER NOT NULL
    );
    CREATE INDEX events_user ON events(user_id, created_at);
    CREATE INDEX events_repo ON events(repo_id, created_at);
    "#,
    // v2: instance settings, suspended accounts
    r#"
    CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
    ALTER TABLE users ADD COLUMN suspended INTEGER NOT NULL DEFAULT 0;
    "#,
    // v3: two-factor auth, session details for the session list and re-auth, audit log
    r#"
    ALTER TABLE users ADD COLUMN totp_secret TEXT;
    ALTER TABLE users ADD COLUMN totp_pending TEXT;
    ALTER TABLE users ADD COLUMN totp_last_step INTEGER NOT NULL DEFAULT 0;
    CREATE TABLE recovery_codes (
        user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        code_hash TEXT NOT NULL,
        used_at INTEGER,
        PRIMARY KEY(user_id, code_hash)
    );
    ALTER TABLE sessions ADD COLUMN ip TEXT NOT NULL DEFAULT '';
    ALTER TABLE sessions ADD COLUMN agent TEXT NOT NULL DEFAULT '';
    ALTER TABLE sessions ADD COLUMN last_seen INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE sessions ADD COLUMN verified_at INTEGER NOT NULL DEFAULT 0;
    CREATE INDEX sessions_user ON sessions(user_id);
    -- No foreign keys: the log outlives the accounts and repos it mentions.
    CREATE TABLE audit (
        id INTEGER PRIMARY KEY,
        actor_id INTEGER,
        actor TEXT NOT NULL DEFAULT '',
        subject_id INTEGER,
        subject TEXT NOT NULL DEFAULT '',
        action TEXT NOT NULL,
        detail TEXT NOT NULL DEFAULT '',
        ip TEXT NOT NULL DEFAULT '',
        created_at INTEGER NOT NULL,
        hash TEXT NOT NULL
    );
    CREATE INDEX audit_subject ON audit(subject_id, id);
    CREATE INDEX audit_actor ON audit(actor_id, id);
    "#,
    // v4: token access levels, single-repository (deploy) tokens, expiry
    r#"
    ALTER TABLE tokens ADD COLUMN scope TEXT NOT NULL DEFAULT 'all';
    ALTER TABLE tokens ADD COLUMN repo_id INTEGER REFERENCES repos(id) ON DELETE CASCADE;
    ALTER TABLE tokens ADD COLUMN expires_at INTEGER;
    CREATE INDEX tokens_repo ON tokens(repo_id);
    "#,
    // v5: abuse protection: storage use, invite links, reports, moderator-hidden content
    r#"
    ALTER TABLE repos ADD COLUMN size_bytes INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE repos ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE issues ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE comments ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0;
    CREATE TABLE invites (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        code_hash TEXT NOT NULL UNIQUE,
        created_by INTEGER REFERENCES users(id) ON DELETE SET NULL,
        note TEXT NOT NULL DEFAULT '',
        max_uses INTEGER NOT NULL DEFAULT 1,
        uses INTEGER NOT NULL DEFAULT 0,
        expires_at INTEGER,
        created_at INTEGER NOT NULL
    );
    -- One target column per kind, so a report disappears with what it reports
    -- (and can never land on a later row that reuses the id).
    CREATE TABLE reports (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        reporter_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
        kind TEXT NOT NULL,
        repo_id INTEGER REFERENCES repos(id) ON DELETE CASCADE,
        issue_id INTEGER REFERENCES issues(id) ON DELETE CASCADE,
        comment_id INTEGER REFERENCES comments(id) ON DELETE CASCADE,
        user_id INTEGER REFERENCES users(id) ON DELETE CASCADE,
        reason TEXT NOT NULL,
        details TEXT NOT NULL DEFAULT '',
        status TEXT NOT NULL DEFAULT 'open',
        resolution TEXT NOT NULL DEFAULT '',
        resolved_by INTEGER REFERENCES users(id) ON DELETE SET NULL,
        created_at INTEGER NOT NULL,
        resolved_at INTEGER
    );
    CREATE INDEX reports_status ON reports(status, id);
    "#,
];

impl Db {
    pub fn open(path: &Path) -> anyhow::Result<Db> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn memory() -> anyhow::Result<Db> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> anyhow::Result<Db> {
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;
             PRAGMA busy_timeout=5000; PRAGMA temp_store=MEMORY;",
        )?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            let tx = conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.execute_batch(&format!("PRAGMA user_version={}", i + 1))?;
            tx.commit()?;
        }
        Ok(Db(Arc::new(Mutex::new(conn))))
    }

    /// Run `f` with the connection on the blocking pool.
    pub async fn call<T, F>(&self, f: F) -> R<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> R<T> + Send + 'static,
    {
        let db = self.0.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = db.lock().unwrap_or_else(|p| p.into_inner());
            f(&mut conn)
        })
        .await
        .map_err(AppError::internal)?
    }
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn migrates_and_queries() {
        let db = Db::memory().unwrap();
        let n: i64 = db
            .call(|c| Ok(c.query_row("SELECT count(*) FROM users", [], |r| r.get(0))?))
            .await
            .unwrap();
        assert_eq!(n, 0);
        let v: i64 = db
            .call(|c| Ok(c.query_row("PRAGMA user_version", [], |r| r.get(0))?))
            .await
            .unwrap();
        assert_eq!(v as usize, MIGRATIONS.len());
    }
}
