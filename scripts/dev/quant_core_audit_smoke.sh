#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

: "${RUSTUP_TOOLCHAIN:="1.91.1"}"
: "${QUANT_CORE_DATABASE_URL:="postgres://postgres:postgres123@localhost:5432/quant_core"}"
: "${EXECUTION_WORKER_DRY_RUN:="true"}"

case "${EXECUTION_WORKER_DRY_RUN}" in
    true | TRUE | 1 | yes | YES) ;;
    *)
        echo "Refusing to run: quant_core audit smoke only supports dry-run execution." >&2
        echo "Set EXECUTION_WORKER_DRY_RUN=true." >&2
        exit 2
        ;;
esac

export QUANT_CORE_DATABASE_URL
export QUANT_CORE_AUDIT_SMOKE=1
export EXECUTION_WORKER_DRY_RUN=true

cd "${REPO_ROOT}"

echo "Preparing quant_core DDL"
./scripts/dev/ddl_smoke.sh

echo
echo "Running quant_core audit write smoke"
echo "  quant_core db: ${QUANT_CORE_DATABASE_URL}"
echo "  dry_run: ${EXECUTION_WORKER_DRY_RUN}"

if command -v rustup >/dev/null 2>&1; then
    : "${RUSTC:="$(rustup which --toolchain "${RUSTUP_TOOLCHAIN}" rustc)"}"
    export RUSTC
    rustup run "${RUSTUP_TOOLCHAIN}" cargo test -p rust-quant-services --test quant_core_audit_postgres_smoke -- --nocapture
else
    cargo test -p rust-quant-services --test quant_core_audit_postgres_smoke -- --nocapture
fi

echo
echo "quant_core audit smoke complete."
