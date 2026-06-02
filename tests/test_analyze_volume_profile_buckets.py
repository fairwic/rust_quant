import importlib.util
import unittest
from pathlib import Path


MODULE_PATH = (
    Path(__file__).resolve().parents[1] / "scripts" / "analyze_volume_profile_buckets.py"
)
spec = importlib.util.spec_from_file_location("analyze_volume_profile_buckets", MODULE_PATH)
vp = importlib.util.module_from_spec(spec)
spec.loader.exec_module(vp)


def entry(open_time, option_type, profile=None):
    signal_value = None
    if profile is not None:
        signal_value = vp.json.dumps({"volume_profile_value": profile})
    return {
        "open_position_time": open_time,
        "option_type": option_type,
        "profit_loss": "0",
        "signal_value": signal_value,
    }


def close(open_time, profit_loss):
    return {
        "open_position_time": open_time,
        "option_type": "close",
        "profit_loss": str(profit_loss),
        "signal_value": None,
    }


class AnalyzeVolumeProfileBucketsTests(unittest.TestCase):
    def test_latest_back_test_query_is_case_insensitive_for_vegas(self):
        sql = vp.latest_vegas_back_test_sql()

        self.assertIn("lower(strategy_type) = 'vegas'", sql)

    def test_analyze_rows_pairs_entry_signal_with_close_profit(self):
        rows = [
            entry(
                "2021-01-01 00:00:00",
                "long",
                {
                    "close_inside_value_area": True,
                    "close_on_high_volume_node": True,
                    "distance_to_poc_pct": 0.002,
                },
            ),
            close("2021-01-01 00:00:00", 10),
            entry(
                "2021-01-02 00:00:00",
                "long",
                {
                    "close_inside_value_area": True,
                    "close_on_high_volume_node": True,
                    "distance_to_poc_pct": 0.003,
                },
            ),
            close("2021-01-02 00:00:00", -5),
            entry(
                "2021-01-03 00:00:00",
                "short",
                {
                    "close_above_value_area": True,
                    "close_on_low_volume_node": True,
                    "distance_to_poc_pct": -0.03,
                },
            ),
            close("2021-01-03 00:00:00", -7),
            close("2021-01-99 00:00:00", 1),
        ]

        report = vp.analyze_rows(rows, min_samples=1)

        self.assertEqual(report["summary"]["analyzed_trades"], 3)
        self.assertEqual(report["summary"]["skipped_rows"], 1)
        self.assertEqual(report["summary"]["missing_close_trades"], 0)
        self.assertEqual(report["summary"]["missing_profile_trades"], 0)

        inside = report["buckets"]["position:inside_value_area"]
        self.assertEqual(inside["count"], 2)
        self.assertEqual(inside["wins"], 1)
        self.assertEqual(inside["losses"], 1)
        self.assertEqual(inside["total_profit"], 5.0)
        self.assertEqual(inside["avg_profit"], 2.5)

        self.assertEqual(
            report["buckets"]["direction_position:long_inside_value_area"]["count"],
            2,
        )
        self.assertEqual(report["buckets"]["node:high_volume_node"]["count"], 2)
        self.assertEqual(report["buckets"]["poc_distance:near_poc"]["count"], 2)
        self.assertEqual(report["buckets"]["position:above_value_area"]["total_profit"], -7.0)

    def test_analyze_rows_counts_entry_trades_without_volume_profile(self):
        rows = [
            entry("2021-01-01 00:00:00", "long", None),
            close("2021-01-01 00:00:00", -3),
            entry("2021-01-02 00:00:00", "short", None),
            close("2021-01-02 00:00:00", 4),
        ]

        report = vp.analyze_rows(rows, min_samples=1)

        self.assertEqual(report["summary"]["analyzed_trades"], 0)
        self.assertEqual(report["summary"]["missing_profile_trades"], 2)
        self.assertEqual(report["buckets"], {})

    def test_analyze_rows_can_use_backfilled_profile_by_entry_time(self):
        rows = [
            entry("2021-01-01 00:00:00", "long", None),
            close("2021-01-01 00:00:00", 6),
        ]
        profile_by_open_time = {
            "2021-01-01 00:00:00": {
                "close_below_value_area": True,
                "close_on_low_volume_node": True,
                "distance_to_poc_pct": 0.04,
            }
        }

        report = vp.analyze_rows(
            rows,
            min_samples=1,
            profile_by_open_time=profile_by_open_time,
        )

        self.assertEqual(report["summary"]["analyzed_trades"], 1)
        self.assertEqual(report["summary"]["backfilled_profile_trades"], 1)
        self.assertEqual(report["buckets"]["position:below_value_area"]["total_profit"], 6.0)

    def test_build_profile_lookup_indexes_profiles_by_utc_candle_time(self):
        candle_rows = [
            {"ts": "1609459200000", "h": "110", "l": "100", "c": "105", "vol": "100"},
            {"ts": "1609473600000", "h": "120", "l": "110", "c": "115", "vol": "200"},
        ]

        lookup = vp.build_profile_lookup(
            candle_rows,
            lookback=2,
            price_bins=4,
            value_area_ratio=0.70,
        )

        self.assertIn("2021-01-01 04:00:00", lookup)
        profile = lookup["2021-01-01 04:00:00"]
        self.assertEqual(profile["price_bin_count"], 4)
        self.assertEqual(profile["total_volume"], 300.0)
        self.assertGreater(profile["point_of_control"], 0.0)


if __name__ == "__main__":
    unittest.main()
