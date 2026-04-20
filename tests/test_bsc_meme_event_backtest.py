import importlib.util
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[1] / "scripts" / "bsc_meme_event_backtest.py"
spec = importlib.util.spec_from_file_location("bsc_meme_event_backtest", MODULE_PATH)
bt = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bt)


def candle(ts, close, volume=1000.0):
    return bt.Candle(ts=ts, open=close, high=close, low=close, close=close, volume=volume)


class BscMemeEventBacktestTests(unittest.TestCase):
    def test_event_replay_captures_second_leg_with_positive_r(self):
        candles = []
        ts = 0
        for _ in range(24):
            candles.append(candle(ts, 100.0, 1000.0))
            ts += 300
        prices = [100, 102, 108, 121, 130, 150, 196, 180, 160, 145, 138]
        for price in prices:
            candles.append(candle(ts, price, 30_000.0))
            ts += 300

        result = bt.run_price_volume_replay("TEST", candles, bt.BacktestConfig())

        self.assertTrue(result.entered)
        self.assertEqual(result.exit_reason, "TRAILING_STOP")
        self.assertGreater(result.net_r, 2.0)

    def test_event_replay_caps_loser_near_one_r(self):
        candles = []
        ts = 0
        for _ in range(24):
            candles.append(candle(ts, 100.0, 1000.0))
            ts += 300
        for price in [100, 103, 109, 122, 111, 108]:
            candles.append(candle(ts, price, 30_000.0))
            ts += 300

        result = bt.run_price_volume_replay("TEST", candles, bt.BacktestConfig())

        self.assertTrue(result.entered)
        self.assertEqual(result.exit_reason, "STOP_LOSS")
        self.assertGreaterEqual(result.net_r, -1.25)
        self.assertLessEqual(result.net_r, -1.0)

    def test_validation_requires_positive_edge_after_removing_largest_winner(self):
        results = [
            bt.BacktestResult(symbol="A", entered=True, net_r=4.0),
            bt.BacktestResult(symbol="B", entered=True, net_r=-1.0),
            bt.BacktestResult(symbol="C", entered=True, net_r=-1.0),
        ]

        summary = bt.summarize_results(results)

        self.assertEqual(summary["net_r"], 2.0)
        self.assertEqual(summary["net_r_without_largest_winner"], -2.0)
        self.assertFalse(summary["passes_proof_gate"])

    def test_goplus_parser_extracts_security_and_concentration(self):
        contract = "0xabc"
        payload = {
            "result": {
                contract: {
                    "buy_tax": "0.01",
                    "sell_tax": "0.02",
                    "is_honeypot": "0",
                    "cannot_sell_all": "0",
                    "cannot_buy": "0",
                    "is_blacklisted": "0",
                    "transfer_pausable": "0",
                    "is_mintable": "0",
                    "dex": [{"liquidity": "60000"}, {"liquidity": "5000"}],
                    "holders": [{"percent": "0.12"}, {"percent": "0.03"}],
                }
            }
        }

        event = bt.parse_goplus_security("TEST", contract, payload)

        self.assertTrue(event.security_checked)
        self.assertTrue(event.sell_simulation_passed)
        self.assertEqual(event.buy_tax_pct, 1.0)
        self.assertEqual(event.sell_tax_pct, 2.0)
        self.assertEqual(event.dex_liquidity_usd, 65000)
        self.assertEqual(event.top1_holder_pct, 12.0)
        self.assertEqual(event.top10_holder_pct, 15.0)

    def test_full_event_replay_rejects_missing_oi_and_cex_flow(self):
        candles = self._triggering_candles()
        event = bt.EventSnapshot(
            symbol="TEST",
            contract_address="0xabc",
            event_tags=["cex_listing"],
            security_checked=True,
            sell_simulation_passed=True,
            dex_liquidity_usd=100000,
            derivatives_checked=True,
            funding_rate=-0.01,
        )

        result = bt.run_full_event_replay("TEST", candles, event, bt.BacktestConfig())

        self.assertFalse(result.entered)
        self.assertIn("HISTORICAL_OI_GROWTH_MISSING", result.data_warning)
        self.assertIn("CEX_FLOW_DATA_MISSING", result.data_warning)

    def test_full_event_replay_enters_when_all_event_fields_confirm(self):
        candles = self._triggering_candles()
        event = bt.EventSnapshot(
            symbol="TEST",
            contract_address="0xabc",
            event_tags=["cex_listing"],
            security_checked=True,
            sell_simulation_passed=True,
            dex_liquidity_usd=100000,
            derivatives_checked=True,
            funding_rate=-0.01,
            historical_oi_available=True,
            oi_growth_1h_pct=40.0,
            cex_flow_checked=True,
            cex_net_inflow_usd=0.0,
        )

        result = bt.run_full_event_replay("TEST", candles, event, bt.BacktestConfig())

        self.assertTrue(result.entered)
        self.assertGreater(result.net_r, 2.0)

    def test_coinalyze_market_picker_prefers_matching_perp(self):
        sample = {"symbol": "rave_usdt", "perp_symbols": ["RAVEUSDT"]}
        markets = [
            {
                "symbol": "RAVEUSD_PERP.X",
                "symbol_on_exchange": "RAVEUSD",
                "base_asset": "RAVE",
                "quote_asset": "USD",
                "is_perpetual": True,
            },
            {
                "symbol": "RAVEUSDT_PERP.G",
                "symbol_on_exchange": "RAVEUSDT",
                "base_asset": "RAVE",
                "quote_asset": "USDT",
                "is_perpetual": True,
                "has_long_short_ratio_data": True,
            },
        ]

        market = bt.pick_coinalyze_market(sample, markets)

        self.assertEqual(market["symbol"], "RAVEUSDT_PERP.G")

    def test_coinalyze_summary_extracts_squeeze_fields(self):
        market = {"symbol": "RAVEUSDT_PERP.G"}
        oi_history = [{"t": i, "c": 100.0} for i in range(12)]
        oi_history.append({"t": 12, "c": 140.0})
        funding_history = [{"t": 10, "c": 0.001}, {"t": 11, "c": -0.002}]
        ratio_history = [{"t": 10, "l": 35.0, "s": 65.0}]

        summary = bt.build_coinalyze_summary(
            "RAVEUSDT_PERP.G",
            market,
            [{"symbol": "RAVEUSDT_PERP.G", "history": oi_history}],
            [{"symbol": "RAVEUSDT_PERP.G", "history": funding_history}],
            [{"symbol": "RAVEUSDT_PERP.G", "history": ratio_history}],
        )

        self.assertTrue(summary["available"])
        self.assertAlmostEqual(summary["oi_growth_1h_pct"], 40.0)
        self.assertEqual(summary["funding_rate"], -0.002)
        self.assertEqual(summary["short_crowding_score"], 0.65)

    def test_cex_flow_summarizes_labeled_token_transfers(self):
        labels = {
            "0xcex": "binance_hot_wallet",
            "0xgate": "gate_hot_wallet",
        }
        transfers = [
            {
                "from": "0xuser",
                "to": "0xCEX",
                "value": "1000000000000000000000",
                "tokenDecimal": "18",
            },
            {
                "from": "0xgate",
                "to": "0xuser",
                "value": "250000000000000000000",
                "tokenDecimal": "18",
            },
        ]

        summary = bt.summarize_cex_flow(transfers, labels, reference_price_usd=0.5)

        self.assertEqual(summary["inflow_tokens"], 1000.0)
        self.assertEqual(summary["outflow_tokens"], 250.0)
        self.assertEqual(summary["net_inflow_usd"], 375.0)
        self.assertEqual(summary["inflow_count"], 1)
        self.assertEqual(summary["outflow_count"], 1)
        self.assertEqual(summary["labels"], ["binance_hot_wallet", "gate_hot_wallet"])

    def test_counterparty_candidates_rank_by_transfer_volume(self):
        transfers = [
            {
                "from": "0xsmall",
                "to": "0xlarge",
                "value": "1000000000000000000000",
                "tokenDecimal": "18",
            },
            {
                "from": "0xlarge",
                "to": "0xother",
                "value": "400000000000000000000",
                "tokenDecimal": "18",
            },
        ]

        rows = bt.summarize_counterparty_candidates(transfers, reference_price_usd=2.0)

        self.assertEqual(rows[0]["address"], "0xlarge")
        self.assertEqual(rows[0]["received_tokens"], 1000.0)
        self.assertEqual(rows[0]["sent_tokens"], 400.0)
        self.assertEqual(rows[0]["volume_usd"], 2800.0)

    def test_transfer_log_decoder_extracts_addresses_and_value(self):
        log = {
            "topics": [
                "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef",
                "0x0000000000000000000000001111111111111111111111111111111111111111",
                "0x0000000000000000000000002222222222222222222222222222222222222222",
            ],
            "data": "0x" + hex(123456789)[2:].rjust(64, "0"),
            "blockNumber": "0x10",
            "transactionHash": "0xabc",
            "logIndex": "0x2",
        }

        transfer = bt.decode_transfer_log(log)

        self.assertEqual(transfer["from"], "0x1111111111111111111111111111111111111111")
        self.assertEqual(transfer["to"], "0x2222222222222222222222222222222222222222")
        self.assertEqual(transfer["value"], "123456789")
        self.assertEqual(transfer["blockNumber"], 16)
        self.assertEqual(transfer["logIndex"], 2)

    def _triggering_candles(self):
        candles = []
        ts = 0
        for _ in range(24):
            candles.append(candle(ts, 100.0, 1000.0))
            ts += 300
        for price in [100, 102, 108, 121, 130, 150, 196, 180, 160, 145, 138]:
            candles.append(candle(ts, price, 30_000.0))
            ts += 300
        return candles


if __name__ == "__main__":
    unittest.main()
