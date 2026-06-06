#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

: "${RUSTUP_TOOLCHAIN:=1.91.1}"
: "${PRE_MAJOR_LISTING_ANNOUNCEMENT_INPUT:?set PRE_MAJOR_LISTING_ANNOUNCEMENT_INPUT to an announcement JSON file}"

cd "${REPO_ROOT}"

if [[ -n "${PRE_MAJOR_LISTING_CAPTURE_REQUEST_OUTPUT:-}" ]]; then
    cargo run --bin pre_major_listing_announcement_to_capture -- \
        --input "${PRE_MAJOR_LISTING_ANNOUNCEMENT_INPUT}" \
        | jq '.request' \
        > "${PRE_MAJOR_LISTING_CAPTURE_REQUEST_OUTPUT}"
    echo "capture request written: ${PRE_MAJOR_LISTING_CAPTURE_REQUEST_OUTPUT}"
else
    cargo run --bin pre_major_listing_announcement_to_capture -- \
        --input "${PRE_MAJOR_LISTING_ANNOUNCEMENT_INPUT}"
fi

echo "live trading: disabled"
