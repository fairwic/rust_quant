use rust_quant::time_util;
#[tokio::test]
async fn main() {
    println!("{}", 0.15588235294117647 > 0.00);
    println!("{}", 0.0100 > 0.00);
    println!("{}", 0.0100 > 0.012);

    time_util::millis_time_diff("4H", 1736884800000, 1714478400000);
    time_util::millis_time_diff("4H", 1736884800000, 1722499200000);
}
