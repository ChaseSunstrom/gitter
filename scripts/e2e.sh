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
trap 'kill $PID ${HOOK_PID:-} 2>/dev/null; rm -rf "$TMP"' EXIT
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
api GET "/repos/alice/hello/graph"
[ "$(jq_ '"Add lib" in [c["subject"] for c in d["commits"]] and all("lane" in c for c in d["commits"])')" = True ] || fail "commit graph: $BODY"
api GET "/repos/alice/hello/history?rev=main&path=README.md"
[ "$(jq_ '[v["commit"]["subject"] for v in d["versions"]][0]')" = "More docs" ] || fail "file history: $BODY"
ok "commit graph spans branches; file history lists a file's commits"
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

# --- releases
api POST /repos/alice/hello/releases '{"tag":"v1.0","target":"main","title":"Version one","notes":"First **stable** release"}'
RID=$(jq_ 'd["id"]')
printf 'hello release\n' >"$TMP/hello.txt"
AID=$(curl -sf -b "$JAR" -H "Origin: $BASE" -H 'Content-Type: text/plain' --data-binary @"$TMP/hello.txt" \
  "$BASE/api/repos/alice/hello/releases/$RID/assets?name=hello.txt" | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])') || fail "release file upload"
curl -sf -D "$TMP/rel.h" "$BASE/api/repos/alice/hello/releases/$RID/assets/$AID" | cmp -s - "$TMP/hello.txt" || fail "release file download"
grep -qi '^content-disposition: attachment; filename="hello.txt"' "$TMP/rel.h" || fail "release files should download as attachments"
git ls-remote "$BASE/alice/hello.git" refs/tags/v1.0 | grep -q . || fail "the release's new tag is missing"
api POST /repos/alice/hello/releases '{"tag":"v1.1","title":"Next up","draft":true}'
curl -sf "$BASE/api/repos/alice/hello/releases" | python3 -c 'import json,sys; sys.exit([r["tag"] for r in json.load(sys.stdin)["releases"]] != ["v1.0"])' || fail "drafts must be hidden from readers"
api GET /repos/alice/hello/releases/latest
[ "$(jq_ 'd["tag"]')" = v1.0 ] && [ "$(jq_ 'd["assets"][0]["downloads"]')" = 1 ] || fail "latest release and its download count: $BODY"
ok "releases: a new tag at a branch, a file uploaded and downloaded (as an attachment, counted), latest, drafts hidden"

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
api POST /repos/alice/hello/pulls/2/comments '{"path":"src/lib.rs","side":"new","line":1,"body":"Looks good here","pending":true}'
api POST /repos/alice/hello/pulls/2/reviews '{"event":"approve","body":"Ship it"}'
api GET /repos/alice/hello/pulls/2/reviews
[ "$(jq_ 'd["summary"]["approved"]')" = "['bob']" ] || fail "review: $BODY"
ok "code review: a line comment in a review, submitted as an approval"
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

# --- collaborators and organizations
api PUT /repos/alice/secret/collaborators/bob '{"role":"write"}'
BOB_AUTH="Authorization: Basic $(printf '%s' 'bob:another horse' | base64)" # pragma: allowlist secret
git clone -q -c http.extraHeader="$BOB_AUTH" "http://127.0.0.1:$PORT/alice/secret.git" "$TMP/bob-secret" || fail "collaborator clones private repo"
(cd "$TMP/bob-secret" && echo hi >bob.txt && git add bob.txt && git -c user.name=bob -c user.email=bob@example.com commit -qm "From bob" \
  && git -c http.extraHeader="$BOB_AUTH" push -q origin HEAD) || fail "collaborator with write pushes"
JAR="$TMP/jar2"
api GET /repos/alice/secret
[ "$(jq_ 'd["can_write"], d["can_admin"]')" = "True False" ] || fail "collaborator role: $BODY"
EXPECT=403 api PATCH /repos/alice/secret '{"description":"x"}'
JAR="$JAR_A"
api GET /repos/alice/secret/collaborators
jq_ 'd["access"]' | grep -q bob || fail "access list names the collaborator"
api POST /orgs '{"name":"acme","display_name":"Acme Inc"}'
api POST /repos '{"name":"tools","owner":"acme","private":true,"readme":true}'
JAR="$TMP/jar2"
EXPECT=404 api GET /repos/acme/tools
JAR="$JAR_A"
api PUT /orgs/acme/members/bob '{"role":"member"}'
JAR="$TMP/jar2"
api GET /repos/acme/tools
[ "$(jq_ 'd["can_write"]')" = False ] || fail "org members get the base role (read)"
JAR="$JAR_A"
api GET /users/acme
[ "$(jq_ 'd["is_org"]')" = True ] || fail "org profile"
ok "collaborators push with their role; organizations own repos and share them with members"

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

# --- signed commits: an SSH-signed push by a registered key shows as verified
if command -v ssh-keygen >/dev/null; then
  ssh-keygen -q -t ed25519 -N '' -C e2e -f "$TMP/sign"
  api POST /me/signing-keys "{\"title\":\"e2e laptop\",\"key\":\"$(cat "$TMP/sign.pub")\"}"
  (cd "$W" && git pull -q --ff-only && echo "Signed line" >> README.md \
    && git -c gpg.format=ssh -c user.signingkey="$TMP/sign" commit -qS -am "Signed commit" \
    && git push -q "http://alice:$TOKEN@127.0.0.1:$PORT/alice/hello.git" main) || fail "signed commit push"  # pragma: allowlist secret
  api GET "/repos/alice/hello/commits?rev=main"
  [ "$(jq_ 'd["commits"][0]["verification"]["status"]')" = verified ] || fail "signed commit should be verified: $BODY"
  [ "$(jq_ 'd["commits"][1]["verification"]["status"]')" = unsigned ] || fail "unsigned commit: $BODY"
  ok "SSH-signed commit by a registered key is verified"
else
  echo "· ssh-keygen not installed: signed commit check skipped"
fi

# --- Git LFS with the real client (the official static build, cached outside the repo)
LFS_VER=v3.8.0
LFS_SUM=e455e00f15d9b95661b8d53498ffb0c3367962cf1ec73c31ab7369516cd6ab8d
LFS_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/gitter-test/git-lfs-$LFS_VER"
if ! command -v git-lfs >/dev/null && [ ! -x "$LFS_DIR/git-lfs" ] && [ "$(uname -sm)" = "Linux x86_64" ]; then
  mkdir -p "$LFS_DIR"
  if curl -sfL -m 120 -o "$LFS_DIR/dl.tar.gz" "https://github.com/git-lfs/git-lfs/releases/download/$LFS_VER/git-lfs-linux-amd64-$LFS_VER.tar.gz" \
    && echo "$LFS_SUM  $LFS_DIR/dl.tar.gz" | sha256sum -c --quiet; then
    tar -xzf "$LFS_DIR/dl.tar.gz" -C "$LFS_DIR" --strip-components=1 --wildcards '*/git-lfs' || rm -f "$LFS_DIR/git-lfs"
  fi
  rm -f "$LFS_DIR/dl.tar.gz"
fi
PATH="$LFS_DIR:$PATH"
if command -v git-lfs >/dev/null; then
  # A throwaway global config, so `git lfs install` never touches ~/.gitconfig; never prompt.
  export GIT_CONFIG_GLOBAL="$TMP/lfs.gitconfig" GIT_TERMINAL_PROMPT=0
  api POST /repos '{"name":"media"}'
  L="$TMP/lfs-work"
  git init -q -b main "$L"
  (cd "$L" && git config user.name Alice && git config user.email alice@example.com && git lfs install >/dev/null \
    && git lfs track '*.bin' >/dev/null && head -c 300000 /dev/urandom >big.bin && git add .gitattributes big.bin \
    && git commit -qm "Add a big file" && git push -q "http://alice:$TOKEN@127.0.0.1:$PORT/alice/media.git" main 2>"$TMP/lfs-push.log") \
    || { cat "$TMP/lfs-push.log"; fail "push with git-lfs"; }  # pragma: allowlist secret
  api GET "/repos/alice/media/blob?rev=main&path=big.bin"
  [ "$(jq_ 'd["lfs"]["stored"]')" = True ] || fail "pushed LFS object not stored: $BODY"
  git clone -q "$BASE/alice/media.git" "$TMP/lfs-clone" 2>"$TMP/lfs-clone.log" || { cat "$TMP/lfs-clone.log"; fail "clone with git-lfs"; }
  cmp -s "$L/big.bin" "$TMP/lfs-clone/big.bin" || fail "cloned LFS file differs from the pushed one"
  curl -sf "$BASE/raw/alice/media/main/big.bin" | cmp -s - "$L/big.bin" || fail "raw should serve the LFS file"
  unset GIT_CONFIG_GLOBAL GIT_TERMINAL_PROMPT
  ok "git-lfs $(git-lfs version | cut -d' ' -f1 | cut -d/ -f2) pushes and clones LFS files; raw serves them"
else
  echo "· git-lfs not installed and the download failed: LFS client check skipped"
fi

# --- container registry: the API with curl, then a real client (docker, podman or skopeo) if installed
REG="127.0.0.1:$PORT"
RA=(-u "alice:$TOKEN")  # pragma: allowlist secret
[ "$(curl -s -o /dev/null -w '%{http_code}' "$BASE/v2/")" = 401 ] || fail "the registry ping should ask for credentials"
printf 'e2e config %.0s' $(seq 300) >"$TMP/blob.bin"
LD="sha256:$(sha256sum "$TMP/blob.bin" | cut -d' ' -f1)"
curl -sf "${RA[@]}" -X POST --data-binary @"$TMP/blob.bin" "$BASE/v2/alice/hello/blobs/uploads/?digest=$LD" >/dev/null || fail "registry blob push"
printf '{"schemaVersion":2,"mediaType":"application/vnd.oci.image.manifest.v1+json","config":{"mediaType":"application/vnd.oci.image.config.v1+json","digest":"%s","size":%s},"layers":[]}' \
  "$LD" "$(wc -c <"$TMP/blob.bin")" >"$TMP/manifest.json"
curl -sf "${RA[@]}" -X PUT -H 'Content-Type: application/vnd.oci.image.manifest.v1+json' --data-binary @"$TMP/manifest.json" \
  "$BASE/v2/alice/hello/manifests/v1" >/dev/null || fail "registry manifest push"
curl -sf "$BASE/v2/alice/hello/manifests/v1" | cmp -s - "$TMP/manifest.json" || fail "registry manifest pull"
curl -sf "$BASE/v2/alice/hello/blobs/$LD" | cmp -s - "$TMP/blob.bin" || fail "registry blob pull"
curl -sf "$BASE/v2/alice/hello/tags/list" | grep -q '"v1"' || fail "registry tags list"
api GET /repos/alice/hello/packages
[ "$(jq_ 'd["images"][0]["tags"][0]["tag"]')" = v1 ] || fail "Packages listing: $BODY"
ok "registry: blob and manifest pushed and pulled with curl, tags listed, Packages tab data"
CLIENT_LOG="$TMP/registry-client.log"
if command -v docker >/dev/null && timeout 10 docker info >/dev/null 2>&1; then
  # A throwaway client config, so the login never touches ~/.docker; an image built offline FROM scratch.
  export DOCKER_CONFIG="$TMP/docker"
  IMG="$REG/alice/hello/e2e:1"
  mkdir -p "$TMP/img" && echo "hello from the registry" >"$TMP/img/hello.txt" && printf 'FROM scratch\nCOPY hello.txt /\n' >"$TMP/img/Dockerfile"
  { docker build -q -t "$IMG" "$TMP/img" && echo "$TOKEN" | docker login -u alice --password-stdin "$REG" && docker push -q "$IMG" \
    && docker rmi -f "$IMG" && docker pull -q "$IMG" && docker rmi -f "$IMG" && docker logout "$REG"; } >"$CLIENT_LOG" 2>&1 \
    || { cat "$CLIENT_LOG"; docker rmi -f "$IMG" >/dev/null 2>&1; fail "docker push and pull"; }
  unset DOCKER_CONFIG
  api GET /repos/alice/hello/packages
  echo "$BODY" | grep -q '"alice/hello/e2e"' || fail "the pushed image should be listed: $BODY"
  ok "docker $(docker version -f '{{.Client.Version}}' 2>/dev/null) logs in, pushes and pulls an image"
elif command -v podman >/dev/null; then
  IMG="$REG/alice/hello/e2e:1"
  mkdir -p "$TMP/img" && echo "hello from the registry" >"$TMP/img/hello.txt" && printf 'FROM scratch\nCOPY hello.txt /\n' >"$TMP/img/Containerfile"
  { podman build -q -t "$IMG" "$TMP/img" && podman push --tls-verify=false --creds "alice:$TOKEN" "$IMG" \
    && podman rmi -f "$IMG" && podman pull -q --tls-verify=false --creds "alice:$TOKEN" "$IMG" && podman rmi -f "$IMG"; } >"$CLIENT_LOG" 2>&1 \
    || { cat "$CLIENT_LOG"; fail "podman push and pull"; }  # pragma: allowlist secret
  ok "podman pushes and pulls an image"
elif command -v skopeo >/dev/null; then
  { skopeo inspect --raw --tls-verify=false --creds "alice:$TOKEN" "docker://$REG/alice/hello:v1" >"$TMP/sk.json" \
    && skopeo copy -q --src-tls-verify=false --dest-tls-verify=false --src-creds "alice:$TOKEN" --dest-creds "alice:$TOKEN" \
      "docker://$REG/alice/hello:v1" "docker://$REG/alice/hello/copy:v1"; } >"$CLIENT_LOG" 2>&1 || { cat "$CLIENT_LOG"; fail "skopeo"; }  # pragma: allowlist secret
  cmp -s "$TMP/sk.json" "$TMP/manifest.json" || fail "skopeo saw a different manifest"
  ok "skopeo reads and copies an image"
else
  echo "· no docker, podman or skopeo: real registry client check skipped"
fi

# --- CI: a push runs the pipeline on the built-in runner
CI_YML='jobs:\n  build:\n    steps:\n      - name: Say hello\n        run: echo hello from ci && ls src\n    artifacts: [\"src/**\"]\n  test:\n    needs: [build]\n    steps:\n      - run: echo testing\n'
api POST /repos/alice/hello/contents "{\"branch\":\"main\",\"message\":\"Add CI\",\"changes\":[{\"path\":\".gitter/ci.yml\",\"content\":\"$CI_YML\"}]}"
ci_status=none
for _ in $(seq 150); do
  api GET /repos/alice/hello/ci/runs/1
  ci_status=$(jq_ 'd["status"]')
  case "$ci_status" in success|failure|cancelled) break ;; esac
  sleep 0.2
done
[ "$ci_status" = success ] || fail "CI run on the built-in runner ended as $ci_status: $BODY"
JOB=$(jq_ 'd["jobs"][0]["id"]')
curl -sf -b "$JAR" "$BASE/api/repos/alice/hello/ci/jobs/$JOB/log" | grep -q 'hello from ci' || fail "CI log"
curl -sf -b "$JAR" "$BASE/api/repos/alice/hello/ci/jobs/$JOB/artifacts" -o "$TMP/art.tgz" && tar -tzf "$TMP/art.tgz" | grep -q 'src/main.rs' || fail "CI artifacts"
curl -sf -N -m 5 -b "$JAR" "$BASE/api/repos/alice/hello/ci/jobs/$JOB/log/stream" | grep -q 'event: status' || fail "CI log stream"
ok "CI: a push runs jobs in order on the built-in runner, with log, live stream and artifacts"

# --- rename and transfer with redirects, trash with restore, data export, self-delete
moved() { curl -s -o /dev/null -w '%{http_code} %{redirect_url}' "$BASE$1"; }
api POST /repos '{"name":"old-name","readme":true}'
git clone -q "$BASE/alice/old-name.git" "$TMP/moved" || fail "clone before the rename"
api PATCH /repos/alice/old-name '{"name":"new-name"}'
[ "$(moved /api/repos/alice/old-name/issues)" = "301 $BASE/api/repos/alice/new-name/issues" ] || fail "a renamed repo's API path should 301"
[ "$(moved /alice/old-name)" = "301 $BASE/alice/new-name" ] || fail "a renamed repo's page should 301"
(cd "$TMP/moved" && git config user.name Alice && git config user.email alice@example.com && echo moved >>README.md && git commit -qam "After the rename" \
  && git push -q "http://alice:$TOKEN@127.0.0.1:$PORT/alice/old-name.git" main) || fail "push to the old URL after a rename"  # pragma: allowlist secret
git clone -q "$BASE/alice/old-name.git" "$TMP/moved2" && grep -q moved "$TMP/moved2/README.md" || fail "clone from the old URL after a rename"
ok "a renamed repo redirects (web, API) and still clones and pushes at its old URL"
JAR="$TMP/jar2"
api POST /repos '{"name":"gift","readme":true}'
api POST /repos/bob/gift/transfer '{"to":"alice","confirm":"bob/gift"}'
api POST /repos '{"name":"present"}'
api POST /repos/bob/present/transfer '{"to":"alice","confirm":"bob/present"}'
JAR="$JAR_A"
EXPECT=404 api GET /repos/alice/gift
api GET /me/transfers
GIFT=$(jq_ '[t["id"] for t in d["incoming"] if t["repo"]=="bob/gift"][0]')
api POST "/me/transfers/$GIFT/accept"
api GET /repos/alice/gift
[ "$(jq_ 'd["is_owner"]')" = True ] || fail "accepted transfer"
[ "$(moved /api/repos/bob/gift)" = "301 $BASE/api/repos/alice/gift" ] || fail "a transferred repo should 301"
ok "a transfer waits for the recipient, then moves the repo and redirects"
api POST /repos '{"name":"scratch","readme":true}'
api DELETE /repos/alice/scratch '{"confirm":"alice/scratch"}'
EXPECT=404 api GET /repos/alice/scratch
api POST /repos '{"name":"undo","readme":true}'
api DELETE /repos/alice/undo '{"confirm":"alice/undo"}'
api GET /me/trash
UNDO=$(jq_ '[r["id"] for r in d["repos"] if r["name"]=="undo"][0]')
api POST "/me/trash/$UNDO/restore"
git clone -q "$BASE/alice/undo.git" "$TMP/undo" && [ -f "$TMP/undo/README.md" ] || fail "a restored repo clones"
ok "deleted repos go to the trash and can be restored"
api POST /me/export
for _ in $(seq 100); do api GET /me/export; [ "$(jq_ 'd["export"]["status"]')" = running ] || break; sleep 0.1; done
[ "$(jq_ 'd["export"]["status"]')" = ready ] || fail "export: $BODY"
mkdir "$TMP/export"
curl -sf -b "$JAR" "$BASE/api/me/export/download" | tar -xz -C "$TMP/export" || fail "export download"
grep -q 'Crash on start' "$TMP/export/issues.json" || fail "export has the issues you wrote"
git clone -q -b main "$TMP/export/repositories/hello.bundle" "$TMP/from-bundle" && [ -f "$TMP/from-bundle/src/main.rs" ] || fail "clone from the exported bundle"
EXPECT=400 api DELETE /me '{"confirm":"bob"}'
ok "data export (JSON and git bundles); self-delete needs the typed username"
# --- webhooks: a local receiver gets a signed ping and an issue event (private addresses need the admin's say-so)
HOOK_LOG="$TMP/hook.log"
python3 -c '
import http.server, json, sys
log = open(sys.argv[1], "a")
class H(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        log.write(json.dumps({"headers": {k.lower(): v for k, v in self.headers.items()}, "body": body.decode()}) + "\n"); log.flush()
        self.send_response(200); self.send_header("Content-Length", "2"); self.end_headers(); self.wfile.write(b"ok")
    def log_message(self, *a): pass
s = http.server.HTTPServer(("127.0.0.1", 0), H)
open(sys.argv[2], "w").write(str(s.server_port))
s.serve_forever()
' "$HOOK_LOG" "$TMP/hook.port" &
HOOK_PID=$!
for _ in $(seq 50); do [ -s "$TMP/hook.port" ] && break; sleep 0.1; done
HOOK_URL="http://127.0.0.1:$(cat "$TMP/hook.port")/hook"
api POST /repos/alice/hello/hooks "{\"url\":\"$HOOK_URL\",\"secret\":\"e2e-secret\",\"events\":[\"issues\"],\"allow_private\":true}"
HOOK_ID=$(jq_ 'd["id"]')
api POST /repos/alice/hello/issues '{"title":"Hooked","body":"Sent to a webhook"}'
for _ in $(seq 100); do [ "$(grep -c . "$HOOK_LOG" 2>/dev/null)" -ge 2 ] && break; sleep 0.1; done
python3 - "$HOOK_LOG" <<'PY' || fail "webhook deliveries: $(cat "$HOOK_LOG")"
import hashlib, hmac, json, sys
rows = [json.loads(line) for line in open(sys.argv[1])]
assert sorted(r["headers"]["x-gitter-event"] for r in rows) == ["issues", "ping"], rows
for r in rows:
    want = "sha256=" + hmac.new(b"e2e-secret", r["body"].encode(), hashlib.sha256).hexdigest()
    assert r["headers"]["x-gitter-signature-256"] == want, r["headers"]
issue = next(json.loads(r["body"]) for r in rows if r["headers"]["x-gitter-event"] == "issues")
assert issue["action"] == "opened" and issue["issue"]["title"] == "Hooked", issue
PY
for _ in $(seq 50); do
  api GET "/repos/alice/hello/hooks/$HOOK_ID"
  [ "$(jq_ '[x["status"] for x in d["deliveries"]]')" = "['ok', 'ok']" ] && break
  sleep 0.1
done
[ "$(jq_ '[x["status"] for x in d["deliveries"]]')" = "['ok', 'ok']" ] || fail "webhook delivery log: $BODY"
ok "webhooks: a local receiver gets a signed ping and issue event, and the delivery log shows both"

# --- MCP server and agent tokens
api POST /me/tokens '{"name":"e2e agent","kind":"agent","repos":["alice/hello"],"tools":["read_file","create_issue"],"expires_days":1}'
AGENT=$(jq_ 'd["token"]')
mcp() { curl -s -X POST -H "Authorization: Bearer $AGENT" -H 'Content-Type: application/json' -H 'Accept: application/json, text/event-stream' --data "$1" "$BASE/mcp"; }
out=$(mcp '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"e2e","version":"1"}}}')
grep -q '"protocolVersion":"2025-06-18"' <<<"$out" || fail "mcp initialize: $out"
out=$(mcp '{"jsonrpc":"2.0","id":2,"method":"tools/list"}')
python3 -c 'import json,sys; t=sorted(x["name"] for x in json.loads(sys.argv[1])["result"]["tools"]); assert t == ["create_issue","list_repos","read_file"], t' "$out" || fail "mcp tools/list: $out"
out=$(mcp '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"read_file","arguments":{"repo":"alice/hello","path":"src/main.rs"}}}')
grep -q 'println' <<<"$out" || fail "mcp read_file: $out"
out=$(mcp '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"create_issue","arguments":{"repo":"alice/hello","title":"Filed by an agent"}}}')
N=$(python3 -c 'import json,sys; r=json.loads(sys.argv[1])["result"]; assert not r["isError"], r; print(json.loads(r["content"][0]["text"])["number"])' "$out") || fail "mcp create_issue: $out"
out=$(mcp '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"comment","arguments":{"repo":"alice/hello","number":1,"body":"x"}}}')
grep -q '"code":-32602' <<<"$out" || fail "a tool the agent wasn't given should be refused: $out"
api GET "/repos/alice/hello/issues/$N"
[ "$(jq_ 'd["issue"]["agent"]')" = "e2e agent" ] || fail "the issue should be marked as the agent's: $BODY"
api GET /me/audit
grep -q "e2e agent: create_issue alice/hello #$N" <<<"$BODY" || fail "the audit log should name the agent token: $BODY"
code=$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $AGENT" "$BASE/api/me")
[ "$code" = 403 ] || fail "agent token on the REST API should be refused, got $code"
code=$(curl -s -o /dev/null -w '%{http_code}' -u "x:$AGENT" "$BASE/alice/hello.git/info/refs?service=git-receive-pack")
[ "$code" = 403 ] || fail "agent token should not work for git pushes, got $code"
ok "MCP: initialize, tools/list and tool calls with an agent token; its issue is marked and audited; no REST or git"
# --- traffic: the clones above counted; a page view with an outside referrer
api POST /repos/alice/hello/traffic/view '{"path":"/","referrer":"https://example.org/post"}'
api GET /repos/alice/hello/traffic
[ "$(jq_ 'd["clones"] >= 3 and d["views"] >= 1 and d["referrers"][0]["name"] == "example.org"')" = True ] || fail "traffic: $BODY"
code=$(curl -s -o /dev/null -w '%{http_code}' "$BASE/api/repos/alice/hello/traffic")
[ "$code" = 401 ] || fail "traffic should be for admins only, anonymous got $code"
ok "traffic: clones and views per day for the repository's admins only"

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
