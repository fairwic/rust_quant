#!/usr/bin/env bash
# Deprecated Binance ETH micro live validation entrypoint.
#
# Live validation is Rust-native only. Keep this filename as a fail-fast guard
# for old notes and shell history.
set -euo pipefail

cat >&2 <<'MSG'
ERROR: scripts/dev/run_binance_live_eth_micro_order_smoke.sh is deprecated and disabled.

Use the Rust-native entrypoint instead:
  cargo run -q -p rust-quant-cli --bin binance_eth_micro_live_validation

Required live-mutation guard:
  BINANCE_ETH_MICRO_LIVE_APPLY=true
  BINANCE_ETH_MICRO_LIVE_ORDER_CONFIRM=I_UNDERSTAND_TINY_ETH_LIVE_ORDER

This shell file must not perform signed preflight, write Web execution tasks,
run execution workers, or place/cancel/close live exchange orders.
MSG

exit 2
