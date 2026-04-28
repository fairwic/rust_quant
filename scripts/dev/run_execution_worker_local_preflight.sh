#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
DRY_RUN_SCRIPT="${REPO_ROOT}/scripts/dev/run_execution_worker_dry_run.sh"
TARGET_BINARY="${REPO_ROOT}/target/debug/rust_quant"

: "${RUSTUP_TOOLCHAIN:=1.91.1}"
: "${QUANT_CORE_DATABASE_URL:=postgres://postgres:postgres123@localhost:5432/quant_core}"
: "${QUANT_DATABASE_URL:="${QUANT_CORE_DATABASE_URL}"}"
: "${SQLX_OFFLINE:=true}"

SELECTED_RUSTC=""
if command -v rustup >/dev/null 2>&1; then
    SELECTED_RUSTC="$(rustup which --toolchain "${RUSTUP_TOOLCHAIN}" rustc 2>/dev/null || true)"
    if [[ -n "${SELECTED_RUSTC}" ]]; then
        export RUSTC="${SELECTED_RUSTC}"
    fi
fi

echo "Execution worker local preflight"
echo "  repo: ${REPO_ROOT}"
echo "  dry_run_script: ${DRY_RUN_SCRIPT}"
echo "  downstream_entrypoint: ./scripts/dev/run_execution_worker_dry_run.sh"
echo "  target_binary: ${TARGET_BINARY}"
echo "  quant_core_database_url: ${QUANT_CORE_DATABASE_URL}"
echo "  sqlx_offline: ${SQLX_OFFLINE}"
if [[ -n "${RUSTC:-}" ]]; then
    echo "  selected_rustc: ${RUSTC}"
fi

if [[ -x "${TARGET_BINARY}" ]]; then
    echo "Stable path available: existing binary already built."
    echo "Delegating to dry-run launcher with EXECUTION_WORKER_USE_EXISTING_BINARY=true."
    exec env \
        EXECUTION_WORKER_USE_EXISTING_BINARY=true \
        RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN}" \
        QUANT_CORE_DATABASE_URL="${QUANT_CORE_DATABASE_URL}" \
        QUANT_DATABASE_URL="${QUANT_DATABASE_URL}" \
        SQLX_OFFLINE="${SQLX_OFFLINE}" \
        RUSTC="${RUSTC:-}" \
        "${DRY_RUN_SCRIPT}" "$@"
fi

CARGO_PATH="$(command -v cargo 2>/dev/null || true)"
RUSTC_PATH="$(command -v rustc 2>/dev/null || true)"
if [[ "${CARGO_PATH}" == "/opt/homebrew/bin/cargo" || "${RUSTC_PATH}" == "/opt/homebrew/bin/rustc" ]]; then
    echo "Detected Homebrew cargo/rustc on PATH." >&2
    echo "Homebrew cargo/rustc may resolve rustc 1.89 even when rustup has 1.91.1 installed." >&2
    echo "This launcher exports RUSTC from rustup before falling back to cargo." >&2
fi

if [[ "${SQLX_OFFLINE}" =~ ^(true|TRUE|1|yes|YES)$ ]] && [[ ! -d "${REPO_ROOT}/.sqlx" ]]; then
    echo "SQLX_OFFLINE=true but ${REPO_ROOT}/.sqlx is missing." >&2
    echo "Compile paths that still touch old sqlx compile-time macros can fail until cache is restored." >&2
    echo "Prefer an existing target/debug/rust_quant binary for local dry-run worker loops." >&2
fi

if [[ -z "${RUSTC:-}" ]]; then
    echo "rustup toolchain ${RUSTUP_TOOLCHAIN} was not resolved." >&2
    echo "Fallback cargo run may reuse the PATH rustc and hit the alloy rustc >= 1.91 requirement." >&2
else
    echo "No existing binary found; fallback compile path will use RUSTC=${RUSTC}."
fi

echo "Delegating to dry-run launcher with EXECUTION_WORKER_USE_EXISTING_BINARY=false."
exec env \
    EXECUTION_WORKER_USE_EXISTING_BINARY=false \
    RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN}" \
    QUANT_CORE_DATABASE_URL="${QUANT_CORE_DATABASE_URL}" \
    QUANT_DATABASE_URL="${QUANT_DATABASE_URL}" \
    SQLX_OFFLINE="${SQLX_OFFLINE}" \
    RUSTC="${RUSTC:-}" \
    "${DRY_RUN_SCRIPT}" "$@"
