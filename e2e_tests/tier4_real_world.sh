#!/usr/bin/env bash
# ==============================================================================
# Tier 4: Real-World End-to-End User Scenarios (20 comprehensive journeys)
# Specification: Master Specification v3.1.0 & PROJECT.md § Feature Inventory
# Output: TAP (Test Anything Protocol) version 13
# Exit Code: 0 on 100% pass, non-zero on failure
# ==============================================================================

set -euo pipefail

export NO_PROXY="localhost,127.0.0.1"
export no_proxy="localhost,127.0.0.1"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PORT="${PORT:-8089}"
BASE_URL="${BASE_URL:-}"
SERVER_PID=""

# Help message
if [[ "${1:-}" == "-h" || "${1:-}" == "--help" || "${1:-}" == "help" ]]; then
    echo "Tier 4: Real-World End-to-End User Scenarios Test Runner (20 scenarios)"
    echo "Usage: bash e2e_tests/tier4_real_world.sh [BASE_URL]"
    exit 0
fi

if [[ -n "${1:-}" && "${1:-}" =~ ^https?:// ]]; then
    BASE_URL="${1}"
fi

cleanup() {
    if [[ -n "${SERVER_PID}" ]]; then
        kill -TERM "${SERVER_PID}" 2>/dev/null || true
        wait "${SERVER_PID}" 2>/dev/null || true
    fi
}
trap cleanup EXIT INT TERM

# Detect running reference server or launch reference server
if [[ -z "${BASE_URL}" ]]; then
    if curl -s -f -m 1 "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1; then
        BASE_URL="http://127.0.0.1:${PORT}"
        echo "# Detected running reference server on ${BASE_URL}" >&2
    else
        echo "# Launching reference mock server on port ${PORT}..." >&2
        python3 "${SCRIPT_DIR}/harness/server.py" "${PORT}" >/dev/null 2>&1 &
        SERVER_PID=$!

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

export BASE_URL
python3 "${SCRIPT_DIR}/tier4/test_tier4.py" "${BASE_URL}"
