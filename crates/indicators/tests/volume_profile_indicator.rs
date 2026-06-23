use rust_quant_common::CandleItem;
use rust_quant_indicators::volume::VolumeProfileIndicator;
fn candle(o: f64, h: f64, l: f64, c: f64, v: f64, ts: i64) -> CandleItem {
    CandleItem {
        o,
        h,
        l,
        c,
        v,
        ts,
        confirm: 1,
    }
}
#[test]
fn volume_profile_builds_poc_and_value_area_from_price_ranges() {
    let mut indicator = VolumeProfileIndicator::new(4, 4, 0.70);
    indicator.next(&candle(100.0, 110.0, 100.0, 108.0, 10.0, 1));
    indicator.next(&candle(108.0, 120.0, 108.0, 118.0, 30.0, 2));
    indicator.next(&candle(118.0, 130.0, 118.0, 128.0, 80.0, 3));
    let value = indicator.next(&candle(126.0, 132.0, 124.0, 130.0, 40.0, 4));
    assert!(value.point_of_control >= 124.0);
    assert!(value.point_of_control <= 132.0);
    assert!(value.value_area_low <= value.point_of_control);
    assert!(value.value_area_high >= value.point_of_control);
    assert!(value.close_inside_value_area || value.close_above_value_area);
    assert!(value.total_volume > 0.0);
    assert_eq!(value.price_bin_count, 4);
}
#[test]
fn volume_profile_rolls_lookback_window() {
    let mut indicator = VolumeProfileIndicator::new(2, 4, 0.70);
    indicator.next(&candle(90.0, 100.0, 90.0, 95.0, 200.0, 1));
    indicator.next(&candle(120.0, 130.0, 120.0, 126.0, 30.0, 2));
    let value = indicator.next(&candle(124.0, 132.0, 122.0, 130.0, 40.0, 3));
    assert!(value.point_of_control >= 120.0);
    assert!(value.point_of_control <= 132.0);
    assert!(value.value_area_low >= 120.0);
}
