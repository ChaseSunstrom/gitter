#!/usr/bin/env bash
# Keeps docker-compose.yml simple and safe: one service, one volume, a
# healthcheck, GITTER_URL as the only required setting, nothing privileged.
# With --smoke it also builds the image, boots it and checks it serves.
set -euo pipefail
cd "$(dirname "$0")/.."
fail() { echo "✗ $*"; exit 1; }

cfg=$(docker compose -f docker-compose.yml config --format json) || fail "docker compose config failed"
python3 - "$cfg" <<'PY' || exit 1
import json, sys
c = json.loads(sys.argv[1])
errs = []
svcs = c.get("services", {})
if list(svcs) != ["gitter"]:
    errs.append(f"expected exactly one service named gitter, got {list(svcs)}")
vols = c.get("volumes") or {}
if len(vols) != 1:
    errs.append(f"expected exactly one named volume, got {list(vols)}")
s = svcs.get("gitter", {})
mounts = [m for m in s.get("volumes", [])]
if not any(m.get("target") == "/data" and m.get("type") == "volume" for m in mounts):
    errs.append("the volume must be mounted at /data")
if any("docker.sock" in str(m.get("source", "")) for m in mounts):
    errs.append("never mount the Docker socket")
if s.get("privileged"):
    errs.append("the service must not be privileged")
if "healthcheck" not in s:
    errs.append("add a healthcheck")
env = s.get("environment") or {}
if "GITTER_URL" not in env:
    errs.append("GITTER_URL must be set")
lines = [l for l in open("docker-compose.yml") if l.strip() and not l.strip().startswith("#")]
if len(lines) > 30:
    errs.append(f"keep the compose file short (it has {len(lines)} non-comment lines)")
for e in errs:
    print("✗", e)
if errs:
    sys.exit(1)
print("✓ docker-compose.yml: one service, one volume at /data, healthcheck, GITTER_URL, nothing privileged")
PY

if [ "${1:-}" = "--smoke" ]; then
  PORT=$((20000 + RANDOM % 20000))
  P=gitter-smoke-$$
  trap 'docker compose -p $P down -v >/dev/null 2>&1 || true' EXIT
  GITTER_PORT=$PORT GITTER_URL="http://127.0.0.1:$PORT" docker compose -p $P up -d --build --wait --wait-timeout 120 >/dev/null || fail "compose up"
  curl -sf "http://127.0.0.1:$PORT/healthz" | grep -q '"ok":true' || fail "healthz"
  curl -sf "http://127.0.0.1:$PORT/" | grep -q '<html' || fail "web app"
  docker compose -p $P exec -T gitter gitter doctor >/dev/null || fail "gitter doctor in the container"
  # A real clone and push over HTTP proves git and git-http-backend are in the image.
  B="http://127.0.0.1:$PORT"
  SETUP=$(docker compose -p $P logs gitter 2>/dev/null | grep -o 'setup=[A-Za-z0-9_-]*' | head -1 | cut -d= -f2)
  W=$(mktemp -d)
  curl -sf -c "$W/jar" -H "Origin: $B" -H 'Content-Type: application/json' -X POST "$B/api/auth/signup" \
    --data "{\"username\":\"boss\",\"email\":\"boss@example.com\",\"password\":\"correct horse\",\"setup\":\"$SETUP\"}" >/dev/null || fail "admin signup via setup link"
  curl -sf -b "$W/jar" -H "Origin: $B" -H 'Content-Type: application/json' -X POST "$B/api/repos" --data '{"name":"smoke","readme":true}' >/dev/null || fail "create repo"
  git clone -q "$B/boss/smoke.git" "$W/c" || fail "clone from the container"
  (cd "$W/c" && git -c user.name=a -c user.email=a@b.c commit -q --allow-empty -m smoke && git push -q "http://boss:correct%20horse@127.0.0.1:$PORT/boss/smoke.git" HEAD) || fail "push to the container"  # pragma: allowlist secret
  # The README's backup, verify and restore commands, exactly as documented.
  docker compose -p $P exec -T gitter gitter backup - >"$W/b.tar.gz" 2>/dev/null || fail "gitter backup - in the container"
  docker compose -p $P exec -T gitter gitter verify - <"$W/b.tar.gz" >/dev/null || fail "gitter verify - in the container"
  docker compose -p $P stop gitter >/dev/null 2>&1
  docker compose -p $P run --rm -T gitter restore - --force <"$W/b.tar.gz" >/dev/null 2>&1 || fail "gitter restore - in the container"
  docker compose -p $P start gitter >/dev/null 2>&1
  for _ in $(seq 60); do curl -sf "$B/healthz" >/dev/null && break; sleep 0.5; done
  git clone -q "$B/boss/smoke.git" "$W/c2" && [ "$(git -C "$W/c2" log -1 --format=%s)" = smoke ] || fail "clone after restore"
  rm -rf "$W"
  size=$(docker image inspect "$(docker compose -p $P images -q gitter | head -1)" --format '{{.Size}}')
  echo "✓ image builds, boots healthy, serves the app; setup link, clone, push, doctor, backup, verify and restore work ($((size / 1024 / 1024)) MB image)"
fi
