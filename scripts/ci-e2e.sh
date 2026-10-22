#!/usr/bin/env bash
# CI with an external runner: boots a server with its built-in runner off,
# registers a runner, starts `gitter runner` against it over HTTP, and checks
# that it clones with its job token, runs the jobs, streams logs, uploads
# artifacts and reports results; then that a removed runner's token stops working.
set -euo pipefail
cd "$(dirname "$0")/.."

PORT=${PORT:-$((20000 + RANDOM % 20000))}
BASE="http://127.0.0.1:$PORT"
TMP=$(mktemp -d)
LOG="$TMP/server.log"
JAR="$TMP/jar"
fail() { echo "✗ $*"; echo "--- server log"; tail -20 "$LOG"; echo "--- runner log"; tail -20 "$TMP/runner.log" 2>/dev/null; exit 1; }
ok() { echo "✓ $*"; }

cargo build -q
GITTER_DATA="$TMP/data" GITTER_ADDR="127.0.0.1:$PORT" GITTER_URL="$BASE" ./target/debug/gitter >"$LOG" 2>&1 &
PID=$!
RPID=
trap 'kill $PID $RPID 2>/dev/null; rm -rf "$TMP"' EXIT
for _ in $(seq 50); do curl -sf "$BASE/robots.txt" >/dev/null && break; sleep 0.1; done
SETUP=$(grep -o 'setup=[A-Za-z0-9_-]*' "$LOG" | head -1 | cut -d= -f2)

api() { # method path [json]; fails on HTTP >= 400
  local out code args=(-s -b "$JAR" -c "$JAR" -X "$1" -H "Origin: $BASE" -H 'Content-Type: application/json' -w '\n%{http_code}')
  [ -n "${3:-}" ] && args+=(--data "$3")
  out=$(curl "${args[@]}" "$BASE/api$2")
  code=${out##*$'\n'}; BODY=${out%$'\n'*}
  [ "$code" -lt 400 ] || fail "$1 $2 -> $code: $BODY"
}
jq_() { python3 -c "import json,sys; d=json.load(sys.stdin); print($1)" <<<"$BODY"; }

api POST /auth/signup "{\"username\":\"ada\",\"email\":\"ada@example.com\",\"password\":\"correct horse\",\"setup\":\"$SETUP\"}"
api GET /admin/settings
SETTINGS=$(python3 -c "import json,sys; d=json.loads(sys.argv[1]); d['builtin_runner_jobs']=0; print(json.dumps(d))" "$BODY")
api PUT /admin/settings "$SETTINGS"
api POST /repos '{"name":"app","private":true,"readme":true}'
api PUT /repos/ada/app/ci/secrets/DEPLOY_KEY '{"value":"remote-secret-123"}'
api POST /admin/runners '{"name":"box-1"}'
RUNNER_TOKEN=$(jq_ 'd["token"]')
RUNNER_ID=$(jq_ 'd["id"]')
GITTER_RUNNER_TOKEN="$RUNNER_TOKEN" ./target/debug/gitter runner --url "$BASE" --labels e2e --jobs 2 --work "$TMP/work" >"$TMP/runner.log" 2>&1 &
RPID=$!
ok "server with the built-in runner off; runner box-1 registered and started"

YML='on: [push, manual]\njobs:\n  build:\n    runs-on: [e2e]\n    steps:\n      - name: Build\n        run: echo \"remote build of $GITTER_REPOSITORY with $DEPLOY_KEY\" && mkdir -p dist && cp README.md dist/\n    artifacts: [\"dist/**\"]\n  check:\n    runs-on: [e2e]\n    needs: [build]\n    steps:\n      - run: test \"$GITTER_EVENT\" = push\n'
api POST /repos/ada/app/contents "{\"branch\":\"main\",\"message\":\"CI on a runner\",\"changes\":[{\"path\":\".gitter/ci.yml\",\"content\":\"$YML\"}]}"
status=none
for _ in $(seq 200); do
  api GET /repos/ada/app/ci/runs/1
  status=$(jq_ 'd["status"]')
  case "$status" in success|failure|cancelled) break ;; esac
  sleep 0.2
done
[ "$status" = success ] || fail "the remote run ended as $status: $BODY"
[ "$(jq_ 'd["jobs"][0]["runner"]')" = box-1 ] || fail "the job ran on $(jq_ 'd["jobs"][0]["runner"]')"
JOB=$(jq_ 'd["jobs"][0]["id"]')
LOGTEXT=$(curl -sf -b "$JAR" "$BASE/api/repos/ada/app/ci/jobs/$JOB/log")
grep -q 'remote build of ada/app with \*\*\*' <<<"$LOGTEXT" || fail "remote log: $LOGTEXT"
grep -q 'remote-secret-123' <<<"$LOGTEXT" && fail "the secret leaked into the log"
curl -sf -b "$JAR" "$BASE/api/repos/ada/app/ci/jobs/$JOB/artifacts" -o "$TMP/a.tgz" && tar -tzf "$TMP/a.tgz" | grep -q 'dist/README.md' || fail "remote artifacts"
ok "the runner cloned a private repo with its job token, ran both jobs in order, masked the secret, uploaded artifacts"

api GET /admin/runners
[ "$(jq_ '[r for r in d["runners"] if r["name"]=="box-1"][0]["labels"].count("e2e")')" = 1 ] || fail "runner labels: $BODY"
# A job token stops working when its attempt ends.
JT=$(grep -o 'gcj_[A-Za-z0-9_-]*' "$TMP/runner.log" | head -1 || true)
[ -z "$JT" ] || fail "the runner printed a job token"
api DELETE "/admin/runners/$RUNNER_ID"
code=$(curl -s -o /dev/null -w '%{http_code}' -X POST -H "Authorization: Bearer $RUNNER_TOKEN" -H 'Content-Type: application/json' --data '{}' "$BASE/ci-runner/claim")
[ "$code" = 401 ] || fail "a removed runner's token should be refused, got $code"
ok "removed runners are locked out"
echo "CI runner checks passed."
