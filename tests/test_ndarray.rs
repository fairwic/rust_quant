// use ndarray::{Array1, Array2, arr1, arr2, ArrayView1, s};
//
// // 基本数组操作示例
// pub fn basic_array_operations() {
//     // 1. 创建一维数组
//     let a = Array1::from(vec![1.0, 2.0, 3.0, 4.0, 5.0]);
//     let b = arr1(&[1.0, 2.0, 3.0, 4.0, 5.0]); // 另一种创建方式
//
//     // 2. 创建二维数组
//     let c = Array2::from_shape_vec((2, 3), vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]).unwrap();
//
//     // 3. 基本运算
//     let sum = &a + &b;  // 数组加法
//     let product = &a * &b;  // 元素级乘法
//     let scaled = &a * 2.0;  // 标量乘法
//
//     // 4. 数组切片
//     let slice = a.slice(s![1..4]);  // 获取下标1到3的元素
//
//     println!("Sum: {:?}", sum);
//     println!("Product: {:?}", product);
//     println!("Scaled: {:?}", scaled);
//     println!("Slice: {:?}", slice);
// }
//
// // 技术分析中的应用示例
// pub struct TechnicalIndicators {
//     data: Array1<f64>,
// }
//
// impl TechnicalIndicators {
//     pub fn new(data: Vec<f64>) -> Self {
//         TechnicalIndicators {
//             data: Array1::from(data)
//         }
//     }
//
//     // 计算简单移动平均
//     pub fn sma(&self, period: usize) -> Array1<f64> {
//         let n = self.data.len();
//         let mut result = Array1::zeros(n);
//
//         for i in period-1..n {
//             let window = self.data.slice(s![i-period+1..=i]);
//             result[i] = window.mean().unwrap_or(0.0);
//         }
//
//         result
//     }
//
//     // 计算指数移动平均
//     pub fn ema(&self, period: usize) -> Array1<f64> {
//         let alpha = 2.0 / (period + 1) as f64;
//         let mut result = Array1::zeros(self.data.len());
//         result[0] = self.data[0];
//
//         for i in 1..self.data.len() {
//             result[i] = alpha * self.data[i] + (1.0 - alpha) * result[i-1];
//         }
//
//         result
//     }
//
//     // 计算标准差
//     pub fn std_dev(&self, period: usize) -> Array1<f64> {
//         let n = self.data.len();
//         let mut result = Array1::zeros(n);
//
//         for i in period-1..n {
//             let window = self.data.slice(s![i-period+1..=i]);
//             result[i] = window.std(0.0);
//         }
//
//         result
//     }
// }
//
// // 矩阵运算示例
// pub fn matrix_operations() {
//     // 1. 创建矩阵
//     let matrix = arr2(&[
//         [1.0, 2.0, 3.0],
//         [4.0, 5.0, 6.0],
//         [7.0, 8.0, 9.0]
//     ]);
//
//     // 2. 矩阵转置
//     let transposed = matrix.t();
//
//     // 3. 矩阵乘法
//     let product = matrix.dot(&transposed);
//
//     // 4. 矩阵分解（需要启用相关特性）
//     #[cfg(feature = "linear-algebra")]
//     let _decomp = matrix.qr();
//
//     println!("Matrix:\n{:?}", matrix);
//     println!("Transposed:\n{:?}", transposed);
//     println!("Product:\n{:?}", product);
// }
//
// // 在数据分析中的应用
// pub struct DataAnalysis {
//     data: Array2<f64>,
// }
//
// impl DataAnalysis {
//     // 计算相关系数矩阵
//     pub fn correlation_matrix(&self) -> Array2<f64> {
//         let n_cols = self.data.ncols();
//         let mut corr = Array2::zeros((n_cols, n_cols));
//
//         for i in 0..n_cols {
//             for j in 0..n_cols {
//                 let col_i = self.data.column(i);
//                 let col_j = self.data.column(j);
//                 corr[[i, j]] = Self::correlation(col_i, col_j);
//             }
//         }
//
//         corr
//     }
//
//     // 计算两个序列的相关系数
//     fn correlation(x: ArrayView1<f64>, y: ArrayView1<f64>) -> f64 {
//         let mean_x = x.mean().unwrap_or(0.0);
//         let mean_y = y.mean().unwrap_or(0.0);
//
//         let cov_xy = (&x - mean_x).dot(&(&y - mean_y));
//         let var_x = (&x - mean_x).dot(&(&x - mean_x));
//         let var_y = (&y - mean_y).dot(&(&y - mean_y));
//
//         cov_xy / (var_x * var_y).sqrt()
//     }
// }
//
// #[cfg(test)]
// mod tests {
//     use super::*;
//
//     #[test]
//     fn test_technical_indicators() {
//         let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
//         let indicators = TechnicalIndicators::new(data);
//
//         let sma = indicators.sma(3);
//         let ema = indicators.ema(3);
//
//         println!("SMA: {:?}", sma);
//         println!("EMA: {:?}", ema);
//     }
// }
