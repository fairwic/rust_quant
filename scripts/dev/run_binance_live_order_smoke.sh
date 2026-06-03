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

Use scripts/dev/run_binance_live_eth_micro_order_smoke.sh only after explicit
live-trading authorization and after Web has an existing v3 signed-ready
Binance credential. The legacy pending-close entrypoint must not write API
credentials or run live exchange mutations.
MSG

exit 2
