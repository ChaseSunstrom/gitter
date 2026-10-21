#!/usr/bin/env bash
# End-to-end check: boots a real server on a temp data dir and drives it with
# curl, the git CLI and (if available) a headless browser.
set -euo pipefail
cd "$(dirname "$0")/.."

PORT=${PORT:-$((20000 + RANDOM % 20000))}
BASE="http://127.0.0.1:$PORT"
TMP=$(mktemp -d)
LOG="$TMP/server.log"
JAR="$TMP/jar"
pass=0
fail() { echo "✗ $*"; echo "--- server log"; tail -30 "$LOG"; exit 1; }
ok() { pass=$((pass + 1)); echo "✓ $*"; }

cargo build -q
GITTER_DATA="$TMP/data" GITTER_ADDR="127.0.0.1:$PORT" GITTER_URL="$BASE" ./target/debug/gitter >"$LOG" 2>&1 &
PID=$!
trap 'kill $PID 2>/dev/null; rm -rf "$TMP"' EXIT
for _ in $(seq 50); do curl -sf "$BASE/robots.txt" >/dev/null && break; sleep 0.1; done
SETUP=$(grep -o 'setup=[A-Za-z0-9_-]*' "$LOG" | head -1 | cut -d= -f2)
[ -n "$SETUP" ] || fail "first boot should print a setup link"
code=$(curl -s -o /dev/null -w '%{http_code}' -H "Origin: $BASE" -H 'Content-Type: application/json' -X POST "$BASE/api/auth/signup" --data '{"username":"mallory","email":"m@example.com","password":"correct horse"}')
[ "$code" = 403 ] || fail "first account without the setup code should be refused, got $code"
ok "first admin account needs the one-time setup link"
curl -sf "$BASE/healthz" | grep -q '"ok":true' || fail "healthz"
ok "healthz"

api() { # method path [json] -> body; fails on HTTP >= 400 unless EXPECT is set
  local m=$1 p=$2 d=${3:-}
  local args=(-s -b "$JAR" -c "$JAR" -X "$m" -H "Origin: $BASE" -w '\n%{http_code}')
  [ -n "$d" ] && args+=(-H 'Content-Type: application/json' --data "$d")
  local out code
  out=$(curl "${args[@]}" "$BASE/api$p")
  code=${out##*$'\n'}
  BODY=${out%$'\n'*}
  if [ -n "${EXPECT:-}" ]; then
    [ "$code" = "$EXPECT" ] || fail "$m $p expected $EXPECT got $code: $BODY"
  elif [ "$code" -ge 400 ]; then
    fail "$m $p -> $code: $BODY"
  fi
}
jq_() { python3 -c "import json,sys; d=json.load(sys.stdin); print($1)" <<<"$BODY"; }

# --- accounts
api POST /auth/signup "{\"username\":\"alice\",\"email\":\"alice@example.com\",\"password\":\"correct horse\",\"setup\":\"$SETUP\"}"
[ "$(jq_ 'd["user"]["is_admin"]')" = "True" ] || fail "first user should be admin"
ok "signup (first user is admin)"
api GET /me
[ "$(jq_ 'd["user"]["username"]')" = alice ] || fail "me"
ok "session cookie works"
curl -s -o /dev/null -w '%{http_code}' -b "$JAR" -X POST -H 'Origin: https://evil.example' -H 'Content-Type: application/json' \
  --data '{"name":"evil"}' "$BASE/api/repos" | grep -q 403 || fail "cross-site POST not blocked"
ok "cross-site write blocked"

# --- repos
api POST /repos '{"name":"hello","description":"First repo","readme":true}'
api GET /repos/alice/hello
[ "$(jq_ 'd["default_branch"]')" = main ] || fail "default branch"
[ "$(jq_ 'd["empty"]')" = False ] || fail "readme repo should not be empty"
ok "create repo with README"
api POST /me/tokens '{"name":"cli"}'
TOKEN=$(jq_ 'd["token"]')

# --- git over HTTP
W="$TMP/work"
git -c init.defaultBranch=main clone -q "$BASE/alice/hello.git" "$W" || fail "anonymous clone of public repo"
ok "anonymous clone"
cd "$W"
git config user.name Alice; git config user.email alice@example.com
mkdir -p src
printf 'fn main() {\n    println!("hi");\n}\n' > src/main.rs
git add -A && git commit -qm "Add main"
git checkout -qb feature
printf 'pub fn add(a: i32, b: i32) -> i32 { a + b }\n' > src/lib.rs
git add -A && git commit -qm "Add lib"
git checkout -q main
if git push -q origin main 2>/dev/null; then fail "anonymous push should be refused"; fi
ok "anonymous push refused"
git push -q "http://alice:$TOKEN@127.0.0.1:$PORT/alice/hello.git" main feature || fail "push with token"  # pragma: allowlist secret
ok "push with token"
git push -q "http://alice:correct%20horse@127.0.0.1:$PORT/alice/hello.git" HEAD:refs/heads/pw-branch || fail "push with password"  # pragma: allowlist secret
ok "push with password"
cd - >/dev/null
git clone -q "$BASE/alice/hello.git" "$TMP/clone2" && [ -f "$TMP/clone2/src/main.rs" ] || fail "re-clone"
ok "clone sees pushed commits"
(cd "$W" && echo "more" >> README.md && git commit -qam "More docs" && git push -q "http://alice:$TOKEN@127.0.0.1:$PORT/alice/hello.git" main) || fail "second push"  # pragma: allowlist secret
(cd "$TMP/clone2" && git pull -q && grep -q more README.md) || fail "pull"
ok "pull sees a later push"

# --- read API
api GET "/repos/alice/hello/tree?rev=main"
jq_ '[e["name"] for e in d["entries"]]' | grep -q src || fail "tree"
[ "$(jq_ 'd["readme"]["name"]')" = README.md ] || fail "readme"
api GET "/repos/alice/hello/blob?rev=main&path=src/main.rs"
jq_ 'd["content"]' | grep -q println || fail "blob"
[ "$(jq_ 'd["language"]')" = Rust ] || fail "language"
api GET "/repos/alice/hello/commits?rev=main"
[ "$(jq_ 'len(d["commits"])')" = 3 ] || fail "commit log"
SHA=$(jq_ '[c["sha"] for c in d["commits"] if c["subject"]=="Add main"][0]')
api GET "/repos/alice/hello/commit/$SHA"
[ "$(jq_ 'd["diff"]["files"][0]["new_path"]')" = src/main.rs ] || fail "commit diff"
api GET "/repos/alice/hello/compare?base=main&head=feature"
[ "$(jq_ 'd["ahead"]')" = 1 ] || fail "compare"
api GET "/repos/alice/hello/blame?rev=main&path=src/main.rs"
[ "$(jq_ 'len(d["chunks"])')" -ge 1 ] || fail "blame"
api GET "/repos/alice/hello/tree-commits?rev=main"
jq_ 'd["commits"]["src"]["subject"]' | grep -q "Add main" || fail "tree-commits"
api GET "/repos/alice/hello/grep?q=println"
[ "$(jq_ 'd["results"][0]["path"]')" = src/main.rs ] || fail "grep"
api GET "/repos/alice/hello/branches"
[ "$(jq_ 'len(d["branches"])')" = 3 ] || fail "branches"
api GET /repos/alice/hello/languages
[ "$(jq_ 'd["languages"][0]["name"]')" = Rust ] || fail "languages"
curl -sf "$BASE/raw/alice/hello/main/src/main.rs" | grep -q println || fail "raw"
curl -sI "$BASE/raw/alice/hello/main/src/main.rs" | grep -qi 'content-security-policy:.*sandbox' || fail "raw sandbox CSP"
curl -sf "$BASE/archive/alice/hello/main.zip" -o "$TMP/a.zip" && python3 -c "import zipfile,sys; zipfile.ZipFile(sys.argv[1]).namelist()" "$TMP/a.zip" || fail "archive"
ok "tree, blob, log, diff, compare, blame, grep, branches, languages, raw, archive"

# --- web editor commit
api GET /repos/alice/hello/branches
HEAD=$(jq_ '[b["sha"] for b in d["branches"] if b["name"]=="main"][0]')
api POST /repos/alice/hello/contents "{\"branch\":\"main\",\"expected_sha\":\"$HEAD\",\"message\":\"Edit README\",\"changes\":[{\"path\":\"README.md\",\"content\":\"# hello\\n\\nEdited on the web.\\n\"}]}"
EXPECT=409 api POST /repos/alice/hello/contents "{\"branch\":\"main\",\"expected_sha\":\"$HEAD\",\"message\":\"stale\",\"changes\":[{\"path\":\"x\",\"content\":\"y\"}]}"
EXPECT=400 api POST /repos/alice/hello/contents '{"branch":"main","message":"bad","changes":[{"path":".git/config","content":"x"}]}'
ok "web commit, stale-edit conflict, .git path refused"

# --- issues and pull requests
api POST /repos/alice/hello/issues '{"title":"Crash on start","body":"Steps: run it"}'
[ "$(jq_ 'd["number"]')" = 1 ] || fail "issue number"
api POST /repos/alice/hello/issues/1/comments '{"body":"Looking into it","state":"closed"}'
api GET /repos/alice/hello/issues/1
[ "$(jq_ 'd["issue"]["state"]')" = closed ] || fail "close via comment"
api POST /repos/alice/hello/pulls '{"title":"Add lib","head":"feature","base":"main"}'
[ "$(jq_ 'd["number"]')" = 2 ] || fail "PR number shares sequence"
api GET /repos/alice/hello/issues/2
[ "$(jq_ 'd["pull"]["mergeable"]')" = True ] || fail "mergeable"
api GET /repos/alice/hello/pulls/2/diff
[ "$(jq_ 'd["diff"]["files"][0]["new_path"]')" = src/lib.rs ] || fail "PR diff"
api POST /repos/alice/hello/pulls/2/merge '{"method":"merge","delete_branch":true}'
api GET /repos/alice/hello/issues/2
[ "$(jq_ 'd["issue"]["state"]')" = merged ] || fail "merged state"
api GET "/repos/alice/hello/blob?rev=main&path=src/lib.rs"
ok "issues, comments, PR create/diff/merge"

# --- wiki, discussions, snippets
api POST /repos/alice/hello/wiki '{"name":"Home","content":"# Hello wiki\n\nSee [Setup](Setup)."}'
api POST /repos/alice/hello/wiki '{"name":"Setup","content":"Run cargo build"}'
api GET /repos/alice/hello/wiki/Home
OID=$(jq_ 'd["page"]["oid"]')
api PUT /repos/alice/hello/wiki/Home "{\"name\":\"Home\",\"content\":\"# Hello wiki\\n\\nSee [Setup](Setup) to start.\",\"expected_oid\":\"$OID\"}"
EXPECT=409 api PUT /repos/alice/hello/wiki/Home "{\"name\":\"Home\",\"content\":\"stale\",\"expected_oid\":\"$OID\"}"
api GET /repos/alice/hello/wiki/Home/history
[ "$(jq_ 'len(d["commits"])')" = 2 ] || fail "wiki history"
api POST /repos/alice/hello/discussions '{"title":"How do I run it?","body":"Any tips?","category":"qa"}'
api POST /repos/alice/hello/discussions/1/comments '{"body":"Use cargo run"}'
CID=$(jq_ 'd["id"]')
api POST /repos/alice/hello/discussions/1/answer "{\"comment_id\":$CID}"
api POST /repos/alice/hello/discussions/1/vote '{"up":true}'
api GET /repos/alice/hello/discussions/1
[ "$(jq_ 'd["discussion"]["answer_id"]')" = "$CID" ] || fail "discussion answer"
api POST /snippets '{"title":"Greeting","files":[{"name":"hello.py","content":"print(\"hi\")\n"}]}'
SNIP=$(jq_ 'd["id"]')
curl -sf "$BASE/raw/snippets/$SNIP/latest/hello.py" | grep -q 'print' || fail "snippet raw"
ok "wiki pages with history and conflict check, discussion answer and vote, snippet raw file"

# --- second user, privacy, stars, forks
JAR_A="$JAR"; JAR="$TMP/jar2"
api POST /auth/signup '{"username":"bob","email":"bob@example.com","password":"another horse"}'
[ "$(jq_ 'd["user"]["is_admin"]')" = False ] || fail "second user not admin"
api POST /repos/alice/hello/star
api POST /repos/alice/hello/fork '{}'
api GET /repos/bob/hello
[ "$(jq_ 'd["fork_of"]["name"]')" = hello ] || fail "fork"
EXPECT=403 api PATCH /repos/alice/hello '{"description":"pwned"}'
ok "second user: star, fork, can't edit others' repos"
JAR="$JAR_A"
api POST /repos '{"name":"secret","private":true,"readme":true}'
JAR="$TMP/jar2"
EXPECT=404 api GET /repos/alice/secret
code=$(curl -s -o /dev/null -w '%{http_code}' "$BASE/alice/secret.git/info/refs?service=git-upload-pack")
[ "$code" = 401 ] || fail "private clone anon should 401, got $code"
[ "$(curl -s -o /dev/null -w '%{http_code}' -b "$JAR" "$BASE/raw/alice/secret/main/README.md")" = 404 ] || fail "private raw"
api GET "/explore?q=secret"
[ "$(jq_ 'len(d["repos"])')" = 0 ] || fail "private repo leaked in explore"
ok "private repos hidden from others (API, git, raw, explore)"
JAR="$JAR_A"
git clone -q "http://alice:$TOKEN@127.0.0.1:$PORT/alice/secret.git" "$TMP/secret" || fail "owner clones private"  # pragma: allowlist secret
ok "owner clones private repo with token"

api GET /admin/overview
[ "$(jq_ 'd["users"]')" = 2 ] || fail "admin overview"
JAR_KEEP="$JAR"; JAR="$TMP/jar2"
EXPECT=403 api GET /admin/overview
JAR="$JAR_KEEP"
ok "admin overview for admins only"
api GET /users/alice
[ "$(jq_ 'len(d["repos"])')" = 2 ] || fail "profile repos"
api GET /feed
api GET "/search?q=hel"
[ "$(jq_ 'd["repos"][0]["name"]')" = hello ] || fail "search"
ok "profile, feed, search"

# --- web pages
code=$(curl -s -o "$TMP/index.html" -w '%{http_code}' "$BASE/alice/hello/blob/main/src/main.rs")
[ "$code" = 200 ] && grep -q '<html' "$TMP/index.html" || fail "SPA route"
curl -sI "$BASE/" | grep -qi "content-security-policy: default-src 'self'" || fail "CSP header"
ok "app shell served with CSP"

if [ "${SKIP_UI:-}" = 1 ]; then
  echo "· UI check skipped (SKIP_UI=1)"
elif [ -d scripts/node_modules/playwright-core ]; then
  BASE="$BASE" GITTER_DB="$TMP/data/gitter.db" node scripts/ui-check.mjs || fail "UI check"
  ok "UI pages render without console errors"
else
  fail "UI check needs Playwright: run 'cd scripts && npm install' (or SKIP_UI=1 to skip)"
fi

echo "All $pass checks passed."
