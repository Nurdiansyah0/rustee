#!/usr/bin/env bash
# ==============================================================================
# E2E Test Runner: Personal Finance PWA SaaS
# Supports: all, tier1, tier2, tier3, tier4
# Outputs: TAP (Test Anything Protocol) version 13
# Exit Code: 0 on 100% pass, non-zero on any failure
# ==============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

TIER="${1:-all}"
PORT="${PORT:-8089}"
BASE_URL="${BASE_URL:-}"
SERVER_PID=""

# Help message
if [[ "${TIER}" == "-h" || "${TIER}" == "--help" || "${TIER}" == "help" ]]; then
    echo "Personal Finance PWA - E2E Test Runner"
    echo ""
    echo "Usage:"
    echo "  bash e2e_tests/runner.sh [tier1|tier2|tier3|tier4|all] [--base-url URL]"
    echo ""
    echo "Tiers:"
    echo "  tier1 : Run Tier 1 Feature Coverage tests (145 tests across 29 features)"
    echo "  tier2 : Run Tier 2 Boundary & Corner tests (145 tests across 29 features)"
    echo "  tier3 : Run Tier 3 Pairwise Cross-Feature tests (29 tests)"
    echo "  tier4 : Run Tier 4 Real-World Application Scenarios (15 scenarios)"
    echo "  all   : Run all test tiers (334 total tests - default)"
    echo ""
    echo "Environment Variables:"
    echo "  BASE_URL  : Target server URL (defaults to auto-detecting 8080 or launching mock on 8089)"
    echo "  PORT      : Port for mock reference server if launched (default: 8089)"
    exit 0
fi

# Cleanup function on script exit
cleanup() {
    if [[ -n "${SERVER_PID}" ]]; then
        echo "# Stopping mock reference server (PID: ${SERVER_PID})..." >&2
        kill -TERM "${SERVER_PID}" 2>/dev/null || true
        wait "${SERVER_PID}" 2>/dev/null || true
    fi
}
trap cleanup EXIT INT TERM

# Determine target URL or launch reference harness
if [[ -n "${BASE_URL}" ]]; then
    echo "# Using configured BASE_URL: ${BASE_URL}" >&2
else
    # Check if a live server is already listening on 8080 or 8089
    if curl -s -f -m 1 "http://127.0.0.1:8080/health" >/dev/null 2>&1; then
        BASE_URL="http://127.0.0.1:8080"
        echo "# Detected running backend on ${BASE_URL}" >&2
    elif curl -s -f -m 1 "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1; then
        BASE_URL="http://127.0.0.1:${PORT}"
        echo "# Detected running reference server on ${BASE_URL}" >&2
    else
        echo "# Starting embedded reference mock server on port ${PORT}..." >&2
        python3 "${SCRIPT_DIR}/harness/server.py" "${PORT}" >/dev/null 2>&1 &
        SERVER_PID=$!

        # Wait for server to become ready (up to 5 seconds)
        READY=0
        for _ in {1..50}; do
            if curl -s -f -m 1 "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1; then
                READY=1
                break
            fi
            sleep 0.1
        done

        if [[ ${READY} -ne 1 ]]; then
            echo "# Error: Reference server failed to start on port ${PORT}" >&2
            exit 1
        fi
        BASE_URL="http://127.0.0.1:${PORT}"
        echo "# Reference server active on ${BASE_URL} (PID: ${SERVER_PID})" >&2
    fi
fi

# Execute tests via run_all.py
export BASE_URL
python3 "${SCRIPT_DIR}/run_all.py" "${TIER}" "${BASE_URL}"
