#!/bin/bash
# ==============================================================================
# Invinite Personal Finance PWA — Backend Daemon Starter
#
# All credentials, keys, and configurations are loaded dynamically from .env
# or system environment variables. NO SECRETS ARE STORED IN THIS SCRIPT.
# ==============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# 1. Load environment from .env if present
if [ -f "$SCRIPT_DIR/.env" ]; then
  echo "[start.sh] Loading configuration from .env..."
  set -a
  # shellcheck source=/dev/null
  source "$SCRIPT_DIR/.env"
  set +a
fi

# 2. Operational Defaults
export HOST="${HOST:-127.0.0.1}"
export PORT="${PORT:-8080}"
export DATABASE_URL="${DATABASE_URL:-sqlite://data/personal_finance.db?mode=rwc}"
export WEB_DIST="${WEB_DIST:-$SCRIPT_DIR/dist}"
export COOKIE_SECURE="${COOKIE_SECURE:-true}"

# 3. Security Assertions: Guard against unconfigured or insecure defaults
if [ -z "${JWT_SECRET:-}" ] || [ "${JWT_SECRET}" = "default_insecure_jwt_secret_change_in_production_32_bytes" ] || [ "${JWT_SECRET}" = "replace_with_secure_random_jwt_secret_minimum_32_characters" ]; then
  echo "[FATAL ERROR] JWT_SECRET is not configured with a secure private key!" >&2
  echo "[FATAL ERROR] Please define a random JWT_SECRET in your .env file." >&2
  exit 1
fi

if [ "${#JWT_SECRET}" -lt 32 ]; then
  echo "[FATAL ERROR] JWT_SECRET must be at least 32 characters long for cryptographic safety!" >&2
  exit 1
fi

# 4. Ensure data directory exists
mkdir -p "$SCRIPT_DIR/data"

# 5. Launch backend daemon
echo "[start.sh] Launching Invinite backend on http://${HOST}:${PORT}..."
nohup ./backend > /tmp/personal_finance_pwa.log 2>&1 &
BACKEND_PID=$!
echo "[start.sh] Backend daemon running with PID: ${BACKEND_PID}"
