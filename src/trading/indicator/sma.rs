use ndarray::Array1;
use ta::indicators::SimpleMovingAverage;
use ta::{DataItem, Next};

pub fn calculate(data: &Array1<f64>, length: usize) -> Array1<f64> {
    let vec_data: Vec<DataItem> = data
        .iter()
        .map(|&x| {
            DataItem::builder()
                .close(x)
                .open(x)
                .high(x)
                .low(x)
                .volume(0.0)
                .build()
                .unwrap()
        })
        .collect();
    let mut sma_indicator = SimpleMovingAverage::new(length).unwrap();
    let result = vec_data
        .iter()
        .map(|x| sma_indicator.next(x))
        .collect::<Vec<f64>>();
    Array1::from(result)
}
