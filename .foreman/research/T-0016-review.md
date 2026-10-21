## Lens: intent

- **HIGH — src/api/snippets.rs:137,261 and src/api/wiki.rs:148 — snippets and wikis bypass the storage quota**
  - **Evidence:** `storage_left` is called only from `smart_http.rs:206`, `lfs.rs:69`, `repos.rs:430` and `code.rs:910`. `STORAGE_USED` (`repo.rs:148`) sums `repos.size_bytes` and LFS only. `size_bytes` is refreshed only by `hooks.rs:22` after a push, so a wiki or snippet never changes it.
  - **Scenario, snippets:** any signed-in account can `PUT /snippets/{id}` repeatedly with different 1 MB content. Each save is a new loose blob. The only brake is `write_rate_per_min`, default 120, so about 120 MB/min. The 30/hour limit applies only to creating snippets, not editing them.
  - **Scenario, wikis:** a repo owner can save pages of up to 512 KB, with no limit on page count or revisions.
  - **Result:** a throwaway account can fill the data disk.
  - **How bad:** snippets are HIGH (any account, unbounded). Wikis are MEDIUM (owner only, same mechanism).
  - **Smallest fix:**
    1. Add `snippets.size_bytes` and `repos.wiki_bytes`, set after each save with `admin::dir_size`.
    2. Add both to `STORAGE_USED`.
    3. Call `storage_left(owner)` before the git commit in `snippets::create`, `snippets::update` and `wiki::save`, and reject when the request's bytes exceed what is left.
  - **Confidence:** high.

- **LOW — src/validate.rs:28, src/lib.rs:118 — existing accounts named "snippets" are shadowed**
  - **Scenario:** the new reserved name blocks new signups only. An account called `snippets` that already exists loses its profile route to the SPA's `/snippets`.
  - **Fix:** mention it in the upgrade notes, or rename such accounts in a migration.
  - **Confidence:** medium.

- **LOW — src/api/snippets.rs:146-165 — a failed create briefly exposes a snippet with no git data**
  - **Scenario:** the DB row is inserted before `init_bare` and the commit. For that window a public snippet appears in `/snippets` while `show` returns 404. A crash in the window leaves the row for good, though `gitter doctor` will flag it.
  - **Fix:** create the git side first, or insert the row last.
  - **Confidence:** medium.

## Lens: adversary

I found no exploitable issue in the points below.

- **Secret snippets:**
  - **Ids:** 128 bits from `OsRng`, validated as 32 hex characters in `valid_id`.
  - **Lists:** `snippets.rs:191` shows a secret row only when `?1 IS NOT NULL AND owner_id = viewer`, and the viewer is -1 when signed out or using a deploy token.
  - **Search and feed:** nothing in `account.rs` touches snippets.
  - **Raw:** `inert(..., immutable = rev.len()==40, no_store = s.secret)` gives `private, no-store` for secrets.
  - **Not found vs forbidden:** `load` returns 404 for any missing or malformed id. 403 appears only after an id is known, and a known id already grants read access.
- **Hidden and private wikis:** every wiki handler starts with `Repo::readable`, which applies `access()` (private, hidden, and a deploy token for another repo). There is no wiki raw route.
- **Git injection:**
  - **Wiki names:** `page_name` allows alphanumerics, `-`, `_` and `.`, rejects a leading `.`, `-` or `_`, and rejects `..`. The name becomes `<name>.md` after `--` or in a `rev:path` stdin line.
  - **Snippet names:** `write_path` plus a `/` ban, with controls rejected, so no newline reaches `cat-file --batch-check`. A leading `-` is harmless because the name travels inside `--cacheinfo mode,oid,path` or `--index-info`.
  - **Revisions:** `rev` is hex-validated and goes through `--end-of-options`.
- **Limited tokens, snippets:** the barrier is structural. `token_may` (`mod.rs:231`) allows non-GET only when `segs[0]=="repos"`, so any limited token gets 403 on `/snippets` writes. Unlimited tokens get `limits=None` (`auth.rs:257`).
- **Limited tokens, wikis and discussions:**
  - Read-scoped tokens: `token_may` rejects writes outright.
  - Deploy tokens, other repos: `in_its_repo` fails closed, including for `%`-encoded names and `.git` suffixes. `access()` also sets `write=false` there.
  - Deploy tokens, own repo: a read-only deploy token cannot post a discussion (`token_may` false), and a write-scope one can in its own repo.
- **Defense-in-depth note:** the discussion handlers (create, comment, vote, edit, answer) check only `readable`, never `writable`. The read-scope block depends entirely on `scope_guard`. That is the same pattern as issues, so I'm not raising it as a finding.
- **XSS:**
  - `md.js` rewrites hrefs only after DOMPurify, and only for scheme-less relative links. The result always starts with `/${owner}/${repo}/...` with `encPath` applied.
  - `owner` and `repo` come from the server-validated meta, or from route params that already loaded successfully.
  - The other `innerHTML` uses (`discussions.js:238-245`, `snippets.js:95`) take only `html` template output (escaped) or earlier sanitized output.
- **Votes:** `discussion_votes_once` is a unique index on `(user_id, discussion_id, ifnull(comment_id,0))` and the insert is `INSERT OR IGNORE`. Comment ids start at 1, so 0 is a safe sentinel.
- **Answers:** only the thread author, or the owner or an admin, may mark one, and only on a `qa` thread. Moving a thread out of qa clears the answer.

## Lens: edge

- **Merged features:**
  - **Cascades:** user deletion cascades through snippets, discussions, comments and votes (`foreign_keys=ON`). A deleted answer comment becomes `answer_id` NULL.
  - **`delete_user`:** it gathers repo ids, snippet ids and LFS oids inside the one transaction, then removes the wiki, repo and snippet folders. `repos` uses AUTOINCREMENT, so a wiki folder is never inherited by a new repo.
  - **Hidden repos:** they go through `readable`.
- **Backup and restore:**
  - **Coverage:** wikis, snippets and `lfs` are included.
  - **Old backups:** `unpack` creates the missing folders (`ops.rs:278`), so they are accepted.
  - **Doctor:** wikis are `required:false`, so a missing folder is not counted. Their "orphans" check compares against repo ids, which is correct.
  - **Manifest:** it counts only repos and snippets, a cosmetic gap.
- **Deletion audit:** wiki, discussion and snippet deletions are recorded (`record`). Only the owner or an admin can delete a discussion, so the actor and subject are correct. The labels exist in `lib.js:289`.
- **LOW — src/api/discussions.rs:345 — moderator deletion of someone else's reply is not audited**
  - **Scenario:** the repo owner deletes another user's reply, which is a moderation action with no log entry.
  - **Fix:** `record` an event when `author != u.id`.
  - **Confidence:** medium.
- **LOW — src/api/discussions.rs:148 — numbers can be reused**
  - **Scenario:** discussion numbers use `max(number)+1`, so deleting the newest thread lets the next one reuse its number.
  - **Fix:** accept it, or keep a counter column.
  - **Confidence:** medium.

## Verdict

**Changes needed.** The quota bypass is the one blocker. Everything else is LOW and can ship or be captured.

**Not checked:** I did not run the tests or compile, and I did not read the full 2843-line diff. I did not review the markdown editor (`mdeditor.js`) or the moderation reports.

**Noticed:** none
