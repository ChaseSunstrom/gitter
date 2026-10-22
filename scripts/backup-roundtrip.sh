#!/usr/bin/env bash
# Back up a running instance, restore it into an empty data dir, boot the
# restore and check that accounts, repositories and git history all survived.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -q
BIN=./target/debug/gitter
TMP=$(mktemp -d)
PORT=$((20000 + RANDOM % 20000)); PORT2=$((PORT + 1))
B="http://127.0.0.1:$PORT"; B2="http://127.0.0.1:$PORT2"
cleanup() { kill "${P1:-}" "${P2:-}" 2>/dev/null || true; rm -rf "$TMP"; }
trap cleanup EXIT
fail() { echo "✗ $*"; exit 1; }
wait_up() { for _ in $(seq 50); do curl -sf "$1/healthz" >/dev/null && return 0; sleep 0.1; done; fail "server at $1 did not start"; }

GITTER_DATA=$TMP/a GITTER_ADDR=127.0.0.1:$PORT GITTER_URL=$B $BIN >"$TMP/a.log" 2>&1 & P1=$!
wait_up "$B"
SETUP=$(grep -o 'setup=[A-Za-z0-9_-]*' "$TMP/a.log" | head -1 | cut -d= -f2)
c() { curl -sf -b "$TMP/jar" -c "$TMP/jar" -H "Origin: $B" -H 'Content-Type: application/json' "$@"; }
c -X POST "$B/api/auth/signup" --data "{\"username\":\"alice\",\"email\":\"a@example.com\",\"password\":\"correct horse\",\"setup\":\"$SETUP\"}" >/dev/null
c -X POST "$B/api/repos" --data '{"name":"keep","readme":true}' >/dev/null
c -X POST "$B/api/repos/alice/keep/issues" --data '{"title":"Remember me"}' >/dev/null
c -X POST "$B/api/repos/alice/keep/wiki" --data '{"name":"Home","content":"Wiki survives"}' >/dev/null
c -X POST "$B/api/repos/alice/keep/discussions" --data '{"title":"Talk survives","category":"general"}' >/dev/null
c -X PUT "$B/api/repos/alice/keep/ci/secrets/DEPLOY_KEY" --data '{"value":"kept-secret"}' >/dev/null
cp "$TMP/a/secret.key" "$TMP/key.orig"
SNIP=$(c -X POST "$B/api/snippets" --data '{"title":"Snip","secret":true,"files":[{"name":"a.txt","content":"snippet survives"}]}' | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')
SHA=$(curl -sf "$B/api/repos/alice/keep/commits" | python3 -c 'import json,sys; print(json.load(sys.stdin)["commits"][0]["sha"])')
# A Git LFS file, uploaded the way git-lfs does it (batch, then PUT).
printf 'large file %.0s' $(seq 2000) >"$TMP/asset.bin"
OID=$(sha256sum "$TMP/asset.bin" | cut -d' ' -f1); SIZE=$(wc -c <"$TMP/asset.bin")
AUTH=(-u 'alice:correct horse')  # pragma: allowlist secret
curl -sf "${AUTH[@]}" -H 'Content-Type: application/vnd.git-lfs+json' -X POST "$B/alice/keep.git/info/lfs/objects/batch" \
  --data "{\"operation\":\"upload\",\"objects\":[{\"oid\":\"$OID\",\"size\":$SIZE}]}" | grep -q '"upload"' || fail "LFS batch upload"
curl -sf "${AUTH[@]}" -X PUT --data-binary @"$TMP/asset.bin" "$B/alice/keep.git/info/lfs/objects/$OID" >/dev/null || fail "LFS upload"
# A deleted repository waits in the trash; the backup carries it so it can still be restored.
c -X POST "$B/api/repos" --data '{"name":"gone","readme":true}' >/dev/null
c -X DELETE "$B/api/repos/alice/gone" --data '{"confirm":"alice/gone"}' >/dev/null || fail "delete into the trash"
GONE=$(c "$B/api/me/trash" | python3 -c 'import json,sys; print(json.load(sys.stdin)["repos"][0]["id"])')
# A release with a file.
RID=$(c -X POST "$B/api/repos/alice/keep/releases" --data '{"tag":"v1","title":"Kept release"}' | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')
AID=$(curl -sf -b "$TMP/jar" -H "Origin: $B" --data-binary @"$TMP/asset.bin" "$B/api/repos/alice/keep/releases/$RID/assets?name=asset.bin" \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])') || fail "release file upload"

GITTER_DATA=$TMP/a $BIN backup "$TMP/backup.tar.gz" || fail "backup"
[ -s "$TMP/backup.tar.gz" ] || fail "backup file is empty"
echo "✓ backup of a running server"
GITTER_DATA=$TMP/a $BIN backup - >"$TMP/stream.tar.gz" 2>/dev/null || fail "backup to stdout"
GITTER_DATA=$TMP/a $BIN verify "$TMP/backup.tar.gz" >/dev/null || fail "verify a good backup"
GITTER_DATA=$TMP/a $BIN verify - <"$TMP/stream.tar.gz" >/dev/null || fail "verify from stdin"
head -c 2000 "$TMP/backup.tar.gz" >"$TMP/torn.tar.gz"
if GITTER_DATA=$TMP/a $BIN verify "$TMP/torn.tar.gz" >/dev/null 2>&1; then fail "verify must reject a truncated backup"; fi
echo "✓ backup to stdout, verify good and truncated backups"

if GITTER_DATA=$TMP/a $BIN restore "$TMP/backup.tar.gz" 2>/dev/null; then fail "restore must refuse to overwrite existing data"; fi
echo "✓ restore refuses a non-empty data dir"

cp "$TMP/backup.tar.gz" "$TMP/a/restore.tar.gz"
kill "$P1"; wait "$P1" 2>/dev/null || true
GITTER_DATA=$TMP/a $BIN restore "$TMP/a/restore.tar.gz" --force >/dev/null || fail "restore --force in place"
ls -d "$TMP"/a/before-restore-* >/dev/null 2>&1 || fail "--force should keep the old data in before-restore-*"
GITTER_DATA=$TMP/a $BIN doctor >/dev/null || fail "doctor after in-place restore"
echo "✓ restore --force in place keeps the old data aside"

GITTER_DATA=$TMP/b $BIN restore - <"$TMP/stream.tar.gz" || fail "restore from stdin"
GITTER_DATA=$TMP/b $BIN doctor || fail "doctor on restored data"
cmp -s "$TMP/b/secret.key" "$TMP/key.orig" && [ "$(stat -c %a "$TMP/b/secret.key")" = 600 ] || fail "the server key (CI secrets) must be restored, owner-only"
echo "✓ restore + doctor"

GITTER_DATA=$TMP/b GITTER_ADDR=127.0.0.1:$PORT2 GITTER_URL=$B2 $BIN >"$TMP/b.log" 2>&1 & P2=$!
wait_up "$B2"
[ "$(curl -sf "$B2/api/repos/alice/keep/commits" | python3 -c 'import json,sys; print(json.load(sys.stdin)["commits"][0]["sha"])')" = "$SHA" ] || fail "history differs after restore"
curl -sf "$B2/api/repos/alice/keep/issues/1" | grep -q "Remember me" || fail "issue missing after restore"
curl -sf "$B2/api/repos/alice/keep/wiki/Home" | grep -q "Wiki survives" || fail "wiki page missing after restore"
curl -sf "$B2/api/repos/alice/keep/discussions/1" | grep -q "Talk survives" || fail "discussion missing after restore"
curl -sf "$B2/raw/snippets/$SNIP/latest/a.txt" | grep -q "snippet survives" || fail "snippet missing after restore"
git clone -q "$B2/alice/keep.git" "$TMP/clone" && [ -f "$TMP/clone/README.md" ] || fail "clone from restored server"
curl -sf -H "Origin: $B2" -H 'Content-Type: application/json' -X POST "$B2/api/auth/login" --data '{"login":"alice","password":"correct horse"}' >/dev/null || fail "login after restore"  # pragma: allowlist secret
curl -sf "${AUTH[@]}" "$B2/alice/keep.git/info/lfs/objects/$OID" | cmp -s - "$TMP/asset.bin" || fail "LFS file missing or different after restore"
c2() { curl -sf -b "$TMP/jar2" -c "$TMP/jar2" -H "Origin: $B2" -H 'Content-Type: application/json' "$@"; }
c2 -X POST "$B2/api/auth/login" --data '{"login":"alice","password":"correct horse"}' >/dev/null || fail "login after restore"  # pragma: allowlist secret
c2 -X POST "$B2/api/me/trash/$GONE/restore" >/dev/null || fail "restore from the trash after a backup restore"
git clone -q "$B2/alice/gone.git" "$TMP/gone" && [ -f "$TMP/gone/README.md" ] || fail "a trashed repository's files must survive the backup"
curl -sf "$B2/api/repos/alice/keep/releases/$RID/assets/$AID" | cmp -s - "$TMP/asset.bin" || fail "release file missing or different after restore"
echo "✓ restored server has the same history, issues, wiki, discussions, snippets, trash, logins, clones, LFS files, release files and the key for CI secrets"
echo "Backup round-trip passed."
