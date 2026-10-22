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
    // v6: Git LFS objects. The files are shared by content (GITTER_DATA/lfs); a row
    // says a repository holds one, which is what lets it be downloaded from there.
    r#"
    CREATE TABLE lfs_objects (
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        oid TEXT NOT NULL,
        size INTEGER NOT NULL,
        created_at INTEGER NOT NULL,
        PRIMARY KEY(repo_id, oid)
    );
    CREATE INDEX lfs_objects_oid ON lfs_objects(oid);
    "#,
    // v7: discussions (wikis live in GITTER_DATA/wikis/<repo id>.git), snippets
    r#"
    CREATE TABLE discussions (
        id INTEGER PRIMARY KEY,
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        number INTEGER NOT NULL,
        category TEXT NOT NULL,
        author_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        title TEXT NOT NULL,
        body TEXT NOT NULL DEFAULT '',
        answer_id INTEGER REFERENCES discussion_comments(id) ON DELETE SET NULL,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        UNIQUE(repo_id, number)
    );
    CREATE INDEX discussions_repo ON discussions(repo_id, category, updated_at);
    CREATE TABLE discussion_comments (
        id INTEGER PRIMARY KEY,
        discussion_id INTEGER NOT NULL REFERENCES discussions(id) ON DELETE CASCADE,
        author_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        body TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );
    CREATE INDEX discussion_comments_d ON discussion_comments(discussion_id);
    -- comment_id NULL is a vote for the thread itself.
    CREATE TABLE discussion_votes (
        user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        discussion_id INTEGER NOT NULL REFERENCES discussions(id) ON DELETE CASCADE,
        comment_id INTEGER REFERENCES discussion_comments(id) ON DELETE CASCADE
    );
    CREATE UNIQUE INDEX discussion_votes_once ON discussion_votes(user_id, discussion_id, ifnull(comment_id, 0));
    CREATE INDEX discussion_votes_d ON discussion_votes(discussion_id, comment_id);
    -- id is random hex (secret links must be unguessable) and names the folder snippets/<id>.git.
    CREATE TABLE snippets (
        id TEXT PRIMARY KEY,
        owner_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        title TEXT NOT NULL DEFAULT '',
        is_secret INTEGER NOT NULL DEFAULT 0,
        files TEXT NOT NULL DEFAULT '',
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );
    CREATE INDEX snippets_owner ON snippets(owner_id, updated_at);
    CREATE INDEX snippets_listed ON snippets(is_secret, updated_at);
    "#,
    // v8: wiki and snippet sizes, for storage quotas
    r#"
    ALTER TABLE repos ADD COLUMN wiki_bytes INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE snippets ADD COLUMN size_bytes INTEGER NOT NULL DEFAULT 0;
    "#,
    // v9: built-in CI
    r#"
    CREATE TABLE ci_runners (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL,
        token_hash TEXT UNIQUE,
        labels TEXT NOT NULL DEFAULT '[]',
        builtin INTEGER NOT NULL DEFAULT 0,
        last_seen INTEGER,
        created_at INTEGER NOT NULL
    );
    CREATE TABLE ci_runs (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        number INTEGER NOT NULL,
        event TEXT NOT NULL,
        ref TEXT NOT NULL,
        sha TEXT NOT NULL,
        pr_number INTEGER,
        actor_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
        title TEXT NOT NULL DEFAULT '',
        status TEXT NOT NULL DEFAULT 'queued',
        error TEXT NOT NULL DEFAULT '',
        created_at INTEGER NOT NULL,
        started_at INTEGER,
        finished_at INTEGER,
        UNIQUE(repo_id, number)
    );
    CREATE INDEX ci_runs_sha ON ci_runs(repo_id, sha);
    CREATE TABLE ci_jobs (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        run_id INTEGER NOT NULL REFERENCES ci_runs(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        key TEXT NOT NULL,
        spec TEXT NOT NULL,
        status TEXT NOT NULL DEFAULT 'waiting',
        runner_id INTEGER REFERENCES ci_runners(id) ON DELETE SET NULL,
        attempt INTEGER NOT NULL DEFAULT 0,
        lease_until INTEGER,
        token_hash TEXT,
        exit_code INTEGER,
        steps TEXT NOT NULL DEFAULT '[]',
        -- Why a job was skipped: 'paths' (no matching change) or 'needs' (a job it needs didn't pass).
        reason TEXT NOT NULL DEFAULT '',
        created_at INTEGER NOT NULL,
        started_at INTEGER,
        finished_at INTEGER
    );
    CREATE INDEX ci_jobs_queue ON ci_jobs(status, id);
    CREATE INDEX ci_jobs_run ON ci_jobs(run_id);
    CREATE TABLE ci_secrets (
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        value BLOB NOT NULL,
        updated_at INTEGER NOT NULL,
        PRIMARY KEY(repo_id, name)
    );
    CREATE TABLE ci_artifacts (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        job_id INTEGER NOT NULL REFERENCES ci_jobs(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        size INTEGER NOT NULL,
        created_at INTEGER NOT NULL,
        UNIQUE(job_id, name)
    );
    CREATE TABLE ci_schedules (
        repo_id INTEGER PRIMARY KEY REFERENCES repos(id) ON DELETE CASCADE,
        crons TEXT NOT NULL,
        last_minute INTEGER NOT NULL DEFAULT 0
    );
    "#,
    // v10: organizations and collaborators. An org is a users row that can't sign in.
    r#"
    ALTER TABLE users ADD COLUMN is_org INTEGER NOT NULL DEFAULT 0;
    CREATE TABLE orgs (
        id INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
        -- What members get on every org repository: none, read or write.
        base_role TEXT NOT NULL DEFAULT 'read',
        created_by INTEGER REFERENCES users(id) ON DELETE SET NULL,
        created_at INTEGER NOT NULL
    );
    CREATE TABLE org_members (
        org_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        role TEXT NOT NULL DEFAULT 'member',
        created_at INTEGER NOT NULL,
        PRIMARY KEY (org_id, user_id)
    );
    CREATE INDEX org_members_user ON org_members(user_id);
    CREATE TABLE repo_collaborators (
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        role TEXT NOT NULL,
        added_by INTEGER REFERENCES users(id) ON DELETE SET NULL,
        created_at INTEGER NOT NULL,
        PRIMARY KEY (repo_id, user_id)
    );
    CREATE INDEX repo_collaborators_user ON repo_collaborators(user_id);
    "#,
    // v11: releases. Asset files live in GITTER_DATA/releases/<release id>/<asset id>;
    // AUTOINCREMENT so a new row never reuses the folder of a deleted one.
    r#"
    CREATE TABLE releases (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        tag TEXT NOT NULL,
        -- A draft's new tag is made at this branch or commit when it's published ('' once the tag exists).
        target TEXT NOT NULL DEFAULT '',
        title TEXT NOT NULL DEFAULT '',
        notes TEXT NOT NULL DEFAULT '',
        draft INTEGER NOT NULL DEFAULT 0,
        prerelease INTEGER NOT NULL DEFAULT 0,
        author_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
        created_at INTEGER NOT NULL,
        published_at INTEGER,
        UNIQUE(repo_id, tag)
    );
    CREATE TABLE release_assets (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        release_id INTEGER NOT NULL REFERENCES releases(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        size INTEGER NOT NULL,
        content_type TEXT NOT NULL DEFAULT 'application/octet-stream',
        sha256 TEXT NOT NULL DEFAULT '',
        downloads INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL,
        UNIQUE(release_id, name)
    );
    "#,
    // v12: webhooks
    r#"
    CREATE TABLE webhooks (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        url TEXT NOT NULL,
        -- Sealed with GITTER_DATA/secret.key.
        secret BLOB NOT NULL,
        -- Comma-separated event names, or '*' for all.
        events TEXT NOT NULL,
        active INTEGER NOT NULL DEFAULT 1,
        -- Set by a site admin only: may reach loopback and private addresses.
        allow_private INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );
    CREATE INDEX webhooks_repo ON webhooks(repo_id);
    -- The queue and the delivery log in one: next_at is when to try next (NULL once delivered or given up).
    CREATE TABLE webhook_deliveries (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        hook_id INTEGER NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
        guid TEXT NOT NULL,
        event TEXT NOT NULL,
        payload TEXT NOT NULL,
        redelivery INTEGER NOT NULL DEFAULT 0,
        status TEXT NOT NULL DEFAULT 'pending',
        attempts INTEGER NOT NULL DEFAULT 0,
        next_at INTEGER,
        request_headers TEXT NOT NULL DEFAULT '{}',
        response_code INTEGER,
        response_body TEXT NOT NULL DEFAULT '',
        error TEXT NOT NULL DEFAULT '',
        duration_ms INTEGER,
        created_at INTEGER NOT NULL,
        tried_at INTEGER
    );
    CREATE INDEX webhook_deliveries_due ON webhook_deliveries(next_at) WHERE next_at IS NOT NULL;
    CREATE INDEX webhook_deliveries_hook ON webhook_deliveries(hook_id, id);
    -- A finished run whose finished_at differs from hooked_at hasn't been announced yet
    -- (a re-run finishes again with a new time).
    ALTER TABLE ci_runs ADD COLUMN hooked_at INTEGER;
    UPDATE ci_runs SET hooked_at = finished_at;
    CREATE INDEX ci_runs_unhooked ON ci_runs(id) WHERE finished_at IS NOT hooked_at;
    "#,
    // v13: redirects, transfers, trash, exports
    r#"
    -- A trashed repository keeps its row (and its folder in GITTER_DATA/trash) for a week.
    -- Its name moves to deleted_name and `name` becomes '~<id>', which no real name can
    -- equal, so the name is free for a new repository meanwhile.
    ALTER TABLE repos ADD COLUMN deleted_at INTEGER;
    ALTER TABLE repos ADD COLUMN deleted_name TEXT;
    -- Old names of renamed or transferred repositories. Ids, not names, so a redirect
    -- follows the repository through later renames and leaves with a deleted account.
    CREATE TABLE repo_redirects (
        owner_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        name TEXT NOT NULL COLLATE NOCASE,
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        created_at INTEGER NOT NULL,
        PRIMARY KEY(owner_id, name)
    );
    CREATE INDEX repo_redirects_repo ON repo_redirects(repo_id);
    -- One open offer per repository.
    CREATE TABLE repo_transfers (
        repo_id INTEGER PRIMARY KEY REFERENCES repos(id) ON DELETE CASCADE,
        from_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        to_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        created_at INTEGER NOT NULL,
        expires_at INTEGER NOT NULL
    );
    CREATE INDEX repo_transfers_to ON repo_transfers(to_id);
    -- One account data export per account: GITTER_DATA/exports/<user id>-<id>.tar.gz.
    CREATE TABLE exports (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        user_id INTEGER NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
        status TEXT NOT NULL DEFAULT 'running',
        error TEXT NOT NULL DEFAULT '',
        size INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL,
        finished_at INTEGER,
        expires_at INTEGER
    );
    "#,
    // v14: signing keys. kind is 'ssh' or 'gpg'. A fingerprint is unique per account, not
    // across accounts: registering someone else's public key gains nothing (a signature
    // only verifies for the account whose email is the committer's), and can't lock them out.
    r#"
    CREATE TABLE signing_keys (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        kind TEXT NOT NULL,
        title TEXT NOT NULL DEFAULT '',
        key TEXT NOT NULL,
        fingerprint TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        UNIQUE(user_id, fingerprint)
    );
    CREATE INDEX signing_keys_fingerprint ON signing_keys(fingerprint);
    CREATE INDEX signing_keys_user ON signing_keys(user_id);
    "#,
    // v15: code review. A comment with no review_id was posted on its own (public at once);
    // others belong to a review, hidden from everyone but its author while it's pending.
    // original is the text of the lines commented on, so a later change marks it outdated.
    r#"
    CREATE TABLE reviews (
        id INTEGER PRIMARY KEY,
        issue_id INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
        author_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        state TEXT NOT NULL DEFAULT 'pending',
        body TEXT NOT NULL DEFAULT '',
        commit_sha TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        submitted_at INTEGER
    );
    CREATE INDEX reviews_issue ON reviews(issue_id);
    CREATE TABLE review_comments (
        id INTEGER PRIMARY KEY,
        issue_id INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
        review_id INTEGER REFERENCES reviews(id) ON DELETE CASCADE,
        author_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        path TEXT NOT NULL,
        side TEXT NOT NULL,
        line INTEGER NOT NULL,
        start_line INTEGER,
        commit_sha TEXT NOT NULL,
        original TEXT NOT NULL,
        in_reply_to INTEGER REFERENCES review_comments(id) ON DELETE CASCADE,
        body TEXT NOT NULL,
        resolved_by INTEGER REFERENCES users(id) ON DELETE SET NULL,
        resolved_at INTEGER,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );
    CREATE INDEX review_comments_issue ON review_comments(issue_id);
    "#,
    // v16: traffic, per repository and UTC day (unix time / 86400). Kind 'view' or
    // 'clone' with name '' is the day's count; 'path' (a page) and 'ref' (a referring
    // site) rows are the top lists, capped per day. Visitors are hashes of the address
    // with a random salt held in memory for that day only, so no address is stored.
    r#"
    CREATE TABLE traffic (
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        day INTEGER NOT NULL,
        kind TEXT NOT NULL,
        name TEXT NOT NULL DEFAULT '',
        count INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY(repo_id, day, kind, name)
    ) WITHOUT ROWID;
    CREATE TABLE traffic_visitors (
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        day INTEGER NOT NULL,
        kind TEXT NOT NULL,
        hash TEXT NOT NULL,
        PRIMARY KEY(repo_id, day, kind, hash)
    ) WITHOUT ROWID;
    "#,
    // v17: agent tokens (kind 'agent': only through /mcp, to the repositories in
    // token_repos and the comma-separated MCP tools in `tools`), and what agents did
    // (`agent` names the token, kept when the token is deleted). A CI run an agent
    // started gets no secrets.
    r#"
    ALTER TABLE tokens ADD COLUMN kind TEXT NOT NULL DEFAULT 'user';
    ALTER TABLE tokens ADD COLUMN tools TEXT NOT NULL DEFAULT '';
    CREATE TABLE token_repos (
        token_id INTEGER NOT NULL REFERENCES tokens(id) ON DELETE CASCADE,
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        PRIMARY KEY(token_id, repo_id)
    );
    ALTER TABLE events ADD COLUMN agent TEXT;
    ALTER TABLE issues ADD COLUMN agent TEXT;
    ALTER TABLE comments ADD COLUMN agent TEXT;
    ALTER TABLE ci_runs ADD COLUMN agent TEXT;
    -- Branches an agent created: the only existing branches agents may commit to, and a
    -- run on one is an agent run (no secrets, never the built-in runner) whoever pushed.
    CREATE TABLE agent_branches (
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        branch TEXT NOT NULL,
        agent TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        PRIMARY KEY(repo_id, branch)
    );
    "#,
    // v18: the container registry and generic packages. Blob files live once by content in
    // GITTER_DATA/registry/sha256/ab/<hex>; registry_blobs records which repositories hold
    // each (like lfs_objects). A manifest's own bytes are a blob too.
    r#"
    CREATE TABLE registry_blobs (
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        digest TEXT NOT NULL,
        size INTEGER NOT NULL,
        created_at INTEGER NOT NULL,
        PRIMARY KEY (repo_id, digest)
    );
    CREATE INDEX registry_blobs_digest ON registry_blobs(digest);
    -- name is the image's path under the repository ('' for owner/repo itself);
    -- size is the manifest plus the blobs or manifests it names.
    CREATE TABLE registry_manifests (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        digest TEXT NOT NULL,
        media_type TEXT NOT NULL,
        size INTEGER NOT NULL,
        created_at INTEGER NOT NULL,
        UNIQUE(repo_id, name, digest)
    );
    -- What each manifest names (layers, config, child manifests), so the sweep keeps them.
    CREATE TABLE registry_refs (
        manifest_id INTEGER NOT NULL REFERENCES registry_manifests(id) ON DELETE CASCADE,
        digest TEXT NOT NULL,
        PRIMARY KEY (manifest_id, digest)
    );
    CREATE TABLE registry_tags (
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        tag TEXT NOT NULL,
        digest TEXT NOT NULL,
        updated_at INTEGER NOT NULL,
        PRIMARY KEY (repo_id, name, tag)
    );
    -- Uploads in progress: the bytes so far are GITTER_DATA/registry/tmp/<id>.
    CREATE TABLE registry_uploads (
        id TEXT PRIMARY KEY,
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        size INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL
    );
    CREATE INDEX registry_uploads_user ON registry_uploads(user_id);
    CREATE TABLE packages (
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        version TEXT NOT NULL,
        file TEXT NOT NULL,
        digest TEXT NOT NULL,
        size INTEGER NOT NULL,
        created_at INTEGER NOT NULL,
        PRIMARY KEY (repo_id, name, version, file)
    );
    "#,
    // v19: CI trust and branch rules. A run started by an agent's changes waits for a
    // person's approval (approval 'needed', then 'approved'). branch_pushers: who last
    // moved each branch (a scheduled run counts as theirs). Commit statuses come from
    // outside systems. Branch rules guard the branches matching a pattern.
    r#"
    ALTER TABLE ci_runs ADD COLUMN approval TEXT;
    ALTER TABLE ci_runs ADD COLUMN approved_by INTEGER REFERENCES users(id) ON DELETE SET NULL;
    CREATE TABLE branch_pushers (
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        branch TEXT NOT NULL,
        user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        at INTEGER NOT NULL,
        PRIMARY KEY (repo_id, branch)
    );
    CREATE TABLE commit_statuses (
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        sha TEXT NOT NULL,
        context TEXT NOT NULL,
        state TEXT NOT NULL,
        description TEXT NOT NULL DEFAULT '',
        target_url TEXT NOT NULL DEFAULT '',
        creator_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        PRIMARY KEY (repo_id, sha, context)
    );
    CREATE TABLE branch_rules (
        id INTEGER PRIMARY KEY,
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        pattern TEXT NOT NULL,
        require_pr INTEGER NOT NULL DEFAULT 1,
        approvals INTEGER NOT NULL DEFAULT 0,
        checks TEXT NOT NULL DEFAULT '[]',
        allow_force_push INTEGER NOT NULL DEFAULT 0,
        allow_deletion INTEGER NOT NULL DEFAULT 0,
        enforce_admins INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL,
        UNIQUE (repo_id, pattern)
    );
    ALTER TABLE pulls ADD COLUMN auto_merge TEXT;
    ALTER TABLE pulls ADD COLUMN auto_merge_by INTEGER REFERENCES users(id) ON DELETE SET NULL;
    "#,
    // v20: milestones (numbered per repository; due_on is a YYYY-MM-DD date) and
    // assignees on issues and pull requests.
    r#"
    CREATE TABLE milestones (
        id INTEGER PRIMARY KEY,
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        number INTEGER NOT NULL,
        title TEXT NOT NULL,
        description TEXT NOT NULL DEFAULT '',
        due_on TEXT,
        state TEXT NOT NULL DEFAULT 'open',
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        closed_at INTEGER,
        UNIQUE(repo_id, number),
        UNIQUE(repo_id, title)
    );
    ALTER TABLE issues ADD COLUMN milestone_id INTEGER REFERENCES milestones(id) ON DELETE SET NULL;
    CREATE INDEX issues_milestone ON issues(milestone_id);
    CREATE TABLE issue_assignees (
        issue_id INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
        user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        PRIMARY KEY (issue_id, user_id)
    );
    CREATE INDEX issue_assignees_user ON issue_assignees(user_id);
    "#,
    // v21: auto-merge is armed for one head commit; who last pushed each branch, from the
    // push history kept so far (so scheduled runs keep their author after the upgrade).
    r#"
    ALTER TABLE pulls ADD COLUMN auto_merge_sha TEXT;
    INSERT OR IGNORE INTO branch_pushers (repo_id, branch, user_id, at)
        SELECT repo_id, title, user_id, max(created_at) FROM events WHERE kind = 'push' AND repo_id IS NOT NULL AND title != '' GROUP BY repo_id, title;
    "#,
    // v22: SSH keys that sign in to git over SSH (apart from signing_keys, like GitHub).
    // A key belongs to one account, so the fingerprint alone says whose it is.
    r#"
    CREATE TABLE ssh_keys (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        title TEXT NOT NULL,
        key TEXT NOT NULL,
        fingerprint TEXT NOT NULL UNIQUE,
        created_at INTEGER NOT NULL,
        last_used INTEGER
    );
    CREATE INDEX ssh_keys_user ON ssh_keys(user_id);
    "#,
    // v23: security scanning. A push or web commit that adds a secret is refused and the
    // finding kept for its author (masked, never the secret; a week) in secret_blocks, so
    // they can allow it once (secret_bypasses, used up by the next push or commit that has
    // it). secret_scans holds the latest on-demand scan of a default branch (findings as
    // JSON). osv_* is the vulnerability database the site admin imports (OSV format);
    // ranges are the OSV SEMVER/ECOSYSTEM ranges as JSON.
    r#"
    CREATE TABLE secret_blocks (
        id INTEGER PRIMARY KEY,
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        fingerprint TEXT NOT NULL,
        kind TEXT NOT NULL,
        path TEXT NOT NULL,
        line INTEGER NOT NULL,
        masked TEXT NOT NULL,
        commit_sha TEXT NOT NULL DEFAULT '',
        created_at INTEGER NOT NULL
    );
    CREATE INDEX secret_blocks_who ON secret_blocks(repo_id, user_id, fingerprint);
    CREATE INDEX secret_blocks_age ON secret_blocks(created_at);
    CREATE TABLE secret_bypasses (
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        fingerprint TEXT NOT NULL,
        reason TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        expires_at INTEGER NOT NULL,
        used_at INTEGER,
        PRIMARY KEY (repo_id, user_id, fingerprint)
    );
    CREATE TABLE secret_scans (
        repo_id INTEGER PRIMARY KEY REFERENCES repos(id) ON DELETE CASCADE,
        commit_sha TEXT NOT NULL,
        scanned_at INTEGER NOT NULL,
        truncated INTEGER NOT NULL DEFAULT 0,
        findings TEXT NOT NULL
    );
    CREATE TABLE osv_vulns (
        id TEXT PRIMARY KEY,
        summary TEXT NOT NULL DEFAULT '',
        severity TEXT NOT NULL DEFAULT '',
        aliases TEXT NOT NULL DEFAULT '[]',
        modified TEXT NOT NULL DEFAULT ''
    );
    CREATE TABLE osv_affected (
        vuln_id TEXT NOT NULL REFERENCES osv_vulns(id) ON DELETE CASCADE,
        ecosystem TEXT NOT NULL,
        name TEXT NOT NULL,
        versions TEXT NOT NULL DEFAULT '[]',
        ranges TEXT NOT NULL DEFAULT '[]'
    );
    CREATE INDEX osv_affected_pkg ON osv_affected(ecosystem, name);
    CREATE INDEX osv_affected_vuln ON osv_affected(vuln_id);
    "#,
    // v24: the merge queue. An entry is tested on a commit made on top of the base
    // (kept at refs/gitter-queue/<id>); it lands only if that exact commit passes.
    r#"
    ALTER TABLE branch_rules ADD COLUMN merge_queue INTEGER NOT NULL DEFAULT 0;
    CREATE TABLE merge_queue (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        issue_id INTEGER NOT NULL UNIQUE REFERENCES issues(id) ON DELETE CASCADE,
        base TEXT NOT NULL,
        method TEXT NOT NULL,
        added_by INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        head_sha TEXT NOT NULL,
        state TEXT NOT NULL DEFAULT 'queued',
        test_sha TEXT,
        tested_base TEXT,
        started_at INTEGER,
        created_at INTEGER NOT NULL
    );
    CREATE INDEX merge_queue_repo ON merge_queue(repo_id, base, id);
    "#,
    // v25: notifications, one row per thread per person (`thread` is "issue:<id>",
    // "discussion:<id>" or "run:<id>"; the id columns make rows go with their thread, so a
    // reused id never inherits them). thread_subscriptions are explicit choices
    // (subscribed 0 = unsubscribed); repo_watches levels are 'watching' or 'ignoring'
    // or 'participating' (the owner's default is watching, everyone else's participating).
    r#"
    CREATE TABLE notifications (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        thread TEXT NOT NULL,
        issue_id INTEGER REFERENCES issues(id) ON DELETE CASCADE,
        discussion_id INTEGER REFERENCES discussions(id) ON DELETE CASCADE,
        run_id INTEGER REFERENCES ci_runs(id) ON DELETE CASCADE,
        reason TEXT NOT NULL,
        actor_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
        unread INTEGER NOT NULL DEFAULT 1,
        done INTEGER NOT NULL DEFAULT 0,
        updated_at INTEGER NOT NULL,
        UNIQUE(user_id, thread)
    );
    CREATE INDEX notifications_inbox ON notifications(user_id, done, unread, updated_at);
    -- The inbox sorts read and unread together.
    CREATE INDEX notifications_recent ON notifications(user_id, done, updated_at);
    -- For the cascades: without them every deleted repo, issue, discussion, run or account scans the table.
    CREATE INDEX notifications_repo ON notifications(repo_id);
    CREATE INDEX notifications_issue ON notifications(issue_id);
    CREATE INDEX notifications_discussion ON notifications(discussion_id);
    CREATE INDEX notifications_run ON notifications(run_id);
    CREATE INDEX notifications_actor ON notifications(actor_id);
    CREATE TABLE thread_subscriptions (
        user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        thread TEXT NOT NULL,
        issue_id INTEGER REFERENCES issues(id) ON DELETE CASCADE,
        discussion_id INTEGER REFERENCES discussions(id) ON DELETE CASCADE,
        subscribed INTEGER NOT NULL,
        PRIMARY KEY(user_id, thread)
    );
    CREATE INDEX thread_subscriptions_thread ON thread_subscriptions(thread);
    CREATE INDEX thread_subscriptions_issue ON thread_subscriptions(issue_id);
    CREATE INDEX thread_subscriptions_discussion ON thread_subscriptions(discussion_id);
    CREATE TABLE repo_watches (
        user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        level TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        PRIMARY KEY(user_id, repo_id)
    );
    CREATE INDEX repo_watches_repo ON repo_watches(repo_id, level);
    "#,
    // v26: repository mirrors. A pull mirror (at most one per repository, which is then
    // read-only here) fetches its remote's branches and tags every interval_secs; push
    // mirrors push this repository's to their remote after each change. The row is its own queue:
    // next_at is when to sync next (NULL: nothing waiting), running_until a lease on a
    // sync in progress, so a burst of pushes is one mirror push.
    r#"
    CREATE TABLE mirrors (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        direction TEXT NOT NULL CHECK (direction IN ('pull', 'push')),
        url TEXT NOT NULL,
        username TEXT NOT NULL DEFAULT '',
        -- The password or token, sealed with GITTER_DATA/secret.key; NULL for none.
        secret BLOB,
        -- Set by a site admin only: may reach loopback and private addresses.
        allow_private INTEGER NOT NULL DEFAULT 0,
        interval_secs INTEGER NOT NULL DEFAULT 28800,
        next_at INTEGER,
        running_until INTEGER,
        failures INTEGER NOT NULL DEFAULT 0,
        last_ok_at INTEGER,
        last_tried_at INTEGER,
        last_error TEXT NOT NULL DEFAULT '',
        created_at INTEGER NOT NULL
    );
    CREATE UNIQUE INDEX mirrors_one_pull ON mirrors(repo_id) WHERE direction = 'pull';
    CREATE INDEX mirrors_repo ON mirrors(repo_id);
    CREATE INDEX mirrors_due ON mirrors(next_at);
    "#,
    // v27: CI caches (trust: 0 an agent's run wrote it, 1 a person's, 2 an admin's), test reports, flaky tests.
    r#"
    CREATE TABLE ci_caches (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        ref TEXT NOT NULL,
        trust INTEGER NOT NULL,
        key TEXT NOT NULL,
        size INTEGER NOT NULL,
        created_at INTEGER NOT NULL,
        used_at INTEGER NOT NULL,
        UNIQUE (repo_id, ref, trust, key)
    );
    CREATE TABLE ci_job_reports (
        job_id INTEGER PRIMARY KEY REFERENCES ci_jobs(id) ON DELETE CASCADE,
        tests INTEGER NOT NULL,
        failed INTEGER NOT NULL,
        skipped INTEGER NOT NULL,
        flaky INTEGER NOT NULL,
        cov_hit REAL,
        cov_total REAL
    );
    CREATE TABLE ci_test_results (
        id INTEGER PRIMARY KEY,
        job_id INTEGER NOT NULL REFERENCES ci_jobs(id) ON DELETE CASCADE,
        current INTEGER NOT NULL DEFAULT 1,
        repo_id INTEGER NOT NULL,
        sha TEXT NOT NULL,
        job TEXT NOT NULL,
        suite TEXT NOT NULL,
        name TEXT NOT NULL,
        status TEXT NOT NULL,
        message TEXT NOT NULL DEFAULT '',
        file TEXT,
        line INTEGER,
        flaky INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX ci_test_results_job ON ci_test_results(job_id);
    CREATE INDEX ci_test_results_sha ON ci_test_results(repo_id, sha, name);
    CREATE TABLE ci_flaky_tests (
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        flips INTEGER NOT NULL,
        last_sha TEXT NOT NULL,
        last_run INTEGER NOT NULL,
        last_seen INTEGER NOT NULL,
        PRIMARY KEY (repo_id, name)
    );
    "#,
    // v28: one-time imports: a pull mirror that removes itself after its first successful sync.
    r#"
    ALTER TABLE mirrors ADD COLUMN once INTEGER NOT NULL DEFAULT 0;
    "#,
    // v29: signing in with GitHub, GitLab or OpenID Connect. `secret` is sealed with the
    // server key (a new key leaves it unreadable: re-enter it). An identity is the
    // provider's stable id for a person, never their email. Accounts made through a
    // provider have pass_hash '!', which no password matches.
    r#"
    CREATE TABLE oauth_providers (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        slug TEXT NOT NULL UNIQUE,
        kind TEXT NOT NULL,
        name TEXT NOT NULL,
        base_url TEXT NOT NULL,
        client_id TEXT NOT NULL,
        secret TEXT NOT NULL,
        allow_private INTEGER NOT NULL DEFAULT 0,
        enabled INTEGER NOT NULL DEFAULT 1,
        allow_signup INTEGER NOT NULL DEFAULT 1,
        created_at INTEGER NOT NULL
    );
    CREATE TABLE identities (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        provider_id INTEGER NOT NULL REFERENCES oauth_providers(id) ON DELETE CASCADE,
        subject TEXT NOT NULL,
        user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        login TEXT NOT NULL DEFAULT '',
        created_at INTEGER NOT NULL,
        UNIQUE (provider_id, subject),
        UNIQUE (provider_id, user_id)
    );
    CREATE INDEX identities_user ON identities(user_id);
    "#,
    // v30: bulk imports from GitHub and GitLab. A job's token is sealed and set to NULL
    // when the job ends. Each item is one repository working through its stages; import_map
    // ties what was imported to the remote's ids, so doing a page twice adds nothing twice.
    r#"
    CREATE TABLE import_jobs (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        owner_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        kind TEXT NOT NULL,
        base_url TEXT NOT NULL,
        token TEXT,
        allow_private INTEGER NOT NULL DEFAULT 0,
        state TEXT NOT NULL DEFAULT 'running',
        error TEXT NOT NULL DEFAULT '',
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );
    CREATE INDEX import_jobs_user ON import_jobs(user_id, state);
    CREATE UNIQUE INDEX import_jobs_one_running ON import_jobs(user_id) WHERE state = 'running';
    CREATE TABLE import_items (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        job_id INTEGER NOT NULL REFERENCES import_jobs(id) ON DELETE CASCADE,
        source TEXT NOT NULL,
        remote_id TEXT NOT NULL DEFAULT '',
        name TEXT NOT NULL,
        private INTEGER NOT NULL DEFAULT 0,
        description TEXT NOT NULL DEFAULT '',
        repo_id INTEGER REFERENCES repos(id) ON DELETE SET NULL,
        stage TEXT NOT NULL DEFAULT 'create',
        cursor TEXT NOT NULL DEFAULT '',
        number_offset INTEGER NOT NULL DEFAULT -1,
        pages INTEGER NOT NULL DEFAULT 0,
        tries INTEGER NOT NULL DEFAULT 0,
        skipped INTEGER NOT NULL DEFAULT 0,
        error TEXT NOT NULL DEFAULT '',
        next_at INTEGER NOT NULL DEFAULT 0,
        running_until INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX import_items_due ON import_items(stage, next_at);
    CREATE TABLE import_map (
        repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
        kind TEXT NOT NULL,
        remote_id TEXT NOT NULL,
        local_id INTEGER NOT NULL,
        PRIMARY KEY (repo_id, kind, remote_id)
    );
    "#,
    // v31: pull requests from forks. Their head lives in this repository as refs/pull/N/head,
    // copied from the fork's branch (head_repo_id, NULL once that fork is gone).
    r#"
    ALTER TABLE pulls ADD COLUMN from_fork INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE pulls ADD COLUMN head_repo_id INTEGER REFERENCES repos(id) ON DELETE SET NULL;
    ALTER TABLE pulls ADD COLUMN maintainer_can_modify INTEGER NOT NULL DEFAULT 0;
    CREATE INDEX pulls_head_repo ON pulls(head_repo_id, head_branch);
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
