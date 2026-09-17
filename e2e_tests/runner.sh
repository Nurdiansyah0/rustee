#!/usr/bin/env bash
# ==============================================================================
# Invinite Personal Finance v3.1.0 — Master E2E Acceptance Test Suite Runner
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
    cat << 'EOF'
Invinite Personal Finance v3.1.0 — Master E2E Acceptance Test Suite Runner

Usage:
  bash e2e_tests/runner.sh [TIER] [BASE_URL]

Tiers:
  all     Run all 4 test tiers (625 assertions total) [Default]
  tier1   Tier 1: Feature Coverage (275 tests across 55 features)
  tier2   Tier 2: Boundary & Corner Cases (275 tests across 55 features)
  tier3   Tier 3: Cross-Feature Integration (55 pairwise tests)
  tier4   Tier 4: Real-World End-to-End Scenarios (20 comprehensive workflows)

Options:
  BASE_URL   Base URL of running service (e.g. http://127.0.0.1:8080).
             If omitted, runner checks port 8080, port 8089, or launches
             the specification reference mock server automatically.

Examples:
  bash e2e_tests/runner.sh all
  bash e2e_tests/runner.sh tier1
  bash e2e_tests/runner.sh all http://127.0.0.1:8080
EOF
    exit 0
fi

TARGET="${1:-all}"
if [[ -n "${2:-}" && "${2:-}" =~ ^https?:// ]]; then
    BASE_URL="${2}"
elif [[ "${TARGET}" =~ ^https?:// ]]; then
    BASE_URL="${TARGET}"
    TARGET="all"
fi

cleanup() {
    if [[ -n "${SERVER_PID}" ]]; then
        echo "# Shutting down test reference server (PID: ${SERVER_PID})..." >&2
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
        echo "# Starting Invinite v3.1.0 reference server on port ${PORT}..." >&2
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

START_TIME=$(date +%s)
FAILED_TIERS=()

run_tier() {
    local tier_name="$1"
    local script_name="$2"
    echo "=============================================================================="
    echo "# Executing ${tier_name} against ${BASE_URL}..."
    echo "=============================================================================="
    if ! bash "${SCRIPT_DIR}/${script_name}" "${BASE_URL}"; then
        echo "# FAILED: ${tier_name}" >&2
        FAILED_TIERS+=("${tier_name}")
    fi
}

case "${TARGET}" in
    all)
        run_tier "Tier 1: Feature Coverage (275 tests)" "tier1_feature_coverage.sh"
        run_tier "Tier 2: Boundary & Corner Cases (275 tests)" "tier2_boundary_corner.sh"
        run_tier "Tier 3: Cross-Feature Integration (55 tests)" "tier3_cross_feature.sh"
        run_tier "Tier 4: Real-World Scenarios (20 scenarios)" "tier4_real_world.sh"
        ;;
    tier1)
        run_tier "Tier 1: Feature Coverage (275 tests)" "tier1_feature_coverage.sh"
        ;;
    tier2)
        run_tier "Tier 2: Boundary & Corner Cases (275 tests)" "tier2_boundary_corner.sh"
        ;;
    tier3)
        run_tier "Tier 3: Cross-Feature Integration (55 tests)" "tier3_cross_feature.sh"
        ;;
    tier4)
        run_tier "Tier 4: Real-World Scenarios (20 scenarios)" "tier4_real_world.sh"
        ;;
    *)
        echo "Unknown target: ${TARGET}. Valid targets: all, tier1, tier2, tier3, tier4" >&2
        exit 1
        ;;
esac

END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))

echo "=============================================================================="
echo "# MASTER SUITE EXECUTION SUMMARY"
echo "=============================================================================="
echo "# Target: ${TARGET}"
echo "# Base URL: ${BASE_URL}"
echo "# Total Duration: ${DURATION}s"

if [[ ${#FAILED_TIERS[@]} -gt 0 ]]; then
    echo "# Status: FAILED"
    echo "# Failed Tiers: ${FAILED_TIERS[*]}"
    exit 1
else
    echo "# Status: PASSED (All tiers passed cleanly with 0 failures)"
    exit 0
fi
