#!/usr/bin/env bash
# Legacy Binance pending-close live entrypoint.
#
# This path predates Web v3 credential envelopes, signed read-only
# reconciliation gates, and protection outcome validation. Keep the filename as
# a hard fail-fast guard for operators who may still have old notes or shell
# history pointing here.
set -euo pipefail

cat >&2 <<'MSG'
ERROR: scripts/dev/run_binance_live_order_smoke.sh is deprecated and disabled.

Use the Rust-native ETH micro validation only after explicit live-trading
authorization and after Web has an existing signed-ready Binance credential:
  cargo run -q -p rust-quant-cli --bin binance_eth_micro_live_validation

The legacy pending-close entrypoint must not write API credentials or run live
exchange mutations.
MSG

exit 2
