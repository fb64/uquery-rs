#!/usr/bin/env bash
# Smoke tests against a freshly built µQuery Docker image.
# Usage: ./tests/smoke.sh [image]
# Default image: fb64/uquery
set -euo pipefail

IMAGE="${1:-fb64/uquery}"
CONTAINER="uquery-smoke-test"
BASE_URL="http://localhost:8080"
TESTS_DIR="$(cd "$(dirname "$0")" && pwd)"
PASS=0
FAIL=0

GREEN='\033[0;32m'
RED='\033[0;31m'
BOLD='\033[1m'
NC='\033[0m'

pass() { echo -e "  ${GREEN}✓${NC} $1"; PASS=$((PASS + 1)); }
fail() { echo -e "  ${RED}✗${NC} $1"; FAIL=$((FAIL + 1)); }

cleanup() {
  docker rm -f "$CONTAINER" "${CONTAINER}-db" 2>/dev/null || true
}
trap cleanup EXIT
cleanup

echo -e "${BOLD}Image:${NC} $IMAGE"

# ── Helpers ───────────────────────────────────────────────────────────────────

wait_ready() {
  local name="$1"
  echo -n "Waiting for $name..."
  for i in $(seq 1 30); do
    if curl -sf "$BASE_URL/health" >/dev/null 2>&1; then echo " ready."; return; fi
    sleep 1
  done
  echo ""
  echo "Container did not become healthy in 30s."
  exit 1
}

check_status() {
  local name="$1" expected="$2"; shift 2
  local got
  got=$(curl -s -o /dev/null -w "%{http_code}" "$@")
  if [ "$got" -eq "$expected" ]; then
    pass "$name"
  else
    fail "$name (expected HTTP $expected, got $got)"
  fi
}

check_body() {
  local name="$1" pattern="$2"; shift 2
  local body
  body=$(curl -s "$@")
  if echo "$body" | grep -q "$pattern"; then
    pass "$name"
  else
    fail "$name (pattern '$pattern' not found in: $body)"
  fi
}

post_status() {
  local name="$1" expected="$2"; shift 2
  check_status "$name" "$expected" \
    -X POST "$BASE_URL" -H "Content-Type: text/plain" "$@"
}

post_body() {
  local name="$1" pattern="$2"; shift 2
  check_body "$name" "$pattern" \
    -X POST "$BASE_URL" -H "Content-Type: text/plain" "$@"
}

# ── Standard container ────────────────────────────────────────────────────────

docker run -d --name "$CONTAINER" \
  -p 8080:8080 \
  -v "$TESTS_DIR:/tmp/tests:ro" \
  "$IMAGE" >/dev/null

wait_ready "container"

echo ""
echo -e "${BOLD}Health${NC}"
check_status "GET /health returns 200" 200 "$BASE_URL/health"

echo ""
echo -e "${BOLD}Response formats${NC}"
post_status "JSON"                    200 -H "Accept: application/json"                    -d "SELECT 1 AS n"
post_status "CSV"                     200 -H "Accept: text/csv"                            -d "SELECT 1 AS n"
post_status "JSON Lines"              200 -H "Accept: application/jsonlines"               -d "SELECT 1 AS n"
post_status "Arrow IPC"               200 -H "Accept: application/vnd.apache.arrow.stream" -d "SELECT 1 AS n"
post_status "406 on unsupported type" 406 -H "Accept: application/xml"                     -d "SELECT 1"

echo ""
echo -e "${BOLD}Query correctness${NC}"
post_body "JSON value"       '"n":1'  -H "Accept: application/json"                    -d "SELECT 1 AS n"
post_body "CSV header+value" '^n'     -H "Accept: text/csv"                            -d "SELECT 1 AS n"
post_body "JSON Lines value" '"n":1'  -H "Accept: application/jsonlines"               -d "SELECT 1 AS n"
post_body "multi-row count"  '"c":3'  -H "Accept: application/json"                    -d "SELECT COUNT(*) AS c FROM (VALUES (1),(2),(3)) t(x)"

echo ""
echo -e "${BOLD}Error handling${NC}"
post_status "400 on invalid SQL" 400 -d "NOT VALID SQL"
post_status "400 on empty body"  400 -d ""

echo ""
echo -e "${BOLD}File queries${NC}"
post_status "CSV file"     200 -H "Accept: application/json" -d "SELECT * FROM '/tmp/tests/test.csv'            LIMIT 5"
post_status "JSONL file"   200 -H "Accept: application/json" -d "SELECT * FROM '/tmp/tests/test.jsonl'          LIMIT 5"
post_status "Parquet file" 200 -H "Accept: application/json" -d "SELECT * FROM '/tmp/tests/test.zstd.parquet'   LIMIT 5"

# ── Container with attached DB ────────────────────────────────────────────────

docker rm -f "$CONTAINER" 2>/dev/null || true

echo ""
echo -e "${BOLD}Custom database${NC}"
docker run -d --name "${CONTAINER}-db" \
  -p 8080:8080 \
  -v "$TESTS_DIR:/tmp/tests:ro" \
  "$IMAGE" -d /tmp/tests/test.db >/dev/null

wait_ready "container with DB"
post_status "starts with attached DB" 200 -H "Accept: application/json" -d "SELECT 1"

# ── Summary ───────────────────────────────────────────────────────────────────

echo ""
echo "────────────────────────────────"
if [ "$FAIL" -eq 0 ]; then
  echo -e "${GREEN}${BOLD}All ${PASS} tests passed.${NC}"
else
  echo -e "${BOLD}${PASS} passed, ${RED}${FAIL} failed${NC}."
fi
echo "────────────────────────────────"

[ "$FAIL" -eq 0 ]
