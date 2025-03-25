use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ta::{Close, DataItem, High, Low, Volume};
use tracing::{debug, info};

use crate::trading::model::market::candles::CandlesEntity;

#[derive(Debug, Clone)]
pub struct PriceLevel {
    pub price: f64,
    pub strength: f64,  // 0.0-1.0 表示强度
    pub type_name: String,  // "支撑"或"压力"
    pub confirmation_count: u32,  // 确认次数
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KeyLevelIndicator {
    // 配置参数
    pub lookback_period: usize,           // 回溯周期
    pub swing_threshold: f64,             // 识别摆动点的阈值
    pub volume_weight: f64,               // 成交量权重
    pub price_cluster_threshold: f64,     // 价格聚类阈值
    pub fibonacci_levels: Vec<f64>,       // 斐波那契水平
    pub ma_periods: Vec<usize>,           // 移动平均周期
    
    // 提高精度的额外配置
    pub atr_multiplier: f64,              // ATR倍数，用于动态阈值
    pub recent_levels_boost: f64,         // 最近水平的增强因子
    pub psychological_levels_enabled: bool, // 是否启用心理关卡(整数、半数等)
}

impl Default for KeyLevelIndicator {
    fn default() -> Self {
        Self {
            lookback_period: 1000,         // 默认回溯200根K线
            swing_threshold: 0.03,        // 默认1%波动识别为摆动点
            volume_weight: 0.4,           // 默认成交量权重30%
            price_cluster_threshold: 0.005, // 价格聚类阈值0.5%
            fibonacci_levels: vec![0.236, 0.382, 0.5, 0.618, 0.786, 1.0, 1.618, 2.618],
            ma_periods: vec![20, 50, 100, 200],
            atr_multiplier: 1.5,
            recent_levels_boost: 1.2,
            psychological_levels_enabled: true,
        }
    }
}

impl KeyLevelIndicator {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn calculate_key_levels(&self, data: &[DataItem]) -> (Vec<PriceLevel>, Vec<PriceLevel>) {
        // 确保有足够数据
        if data.len() < self.lookback_period {
            return (Vec::new(), Vec::new());
        }
        
        // 1. 识别历史高低点
        let swing_points = self.identify_swing_points(data);
        
        // 2. 计算斐波那契水平
        let fib_levels = self.calculate_fibonacci_levels(data);
        
        // 3. 识别移动平均线水平
        let ma_levels = self.identify_ma_levels(data);
        
        // 4. 识别成交量轮廓关键区域
        let volume_levels = self.identify_volume_profile_levels(data);
        
        // 5. 识别心理价格水平(如整数关卡)
        let psychological_levels = if self.psychological_levels_enabled {
            self.identify_psychological_levels(data)
        } else {
            Vec::new()
        };
        
        // 6. 整合所有水平并聚类
        let mut all_levels = Vec::new();
        all_levels.extend(swing_points);
        all_levels.extend(fib_levels);
        all_levels.extend(ma_levels);
        all_levels.extend(volume_levels);
        all_levels.extend(psychological_levels);
        
        // 7. 增强最近的价格水平 (新增)
        let enhanced_levels = self.enhance_recent_levels(all_levels, data);
        
        // 8. 聚类相近的价格水平
        let clustered_levels = self.cluster_price_levels(enhanced_levels);
        
        // 9. 按类型和强度分离支撑位和压力位
        let (support_levels, resistance_levels) = self.separate_levels(clustered_levels, data);
        
        // 10. 返回排序后的支撑位和压力位
        (
            self.sort_levels_by_strength(support_levels),
            self.sort_levels_by_strength(resistance_levels)
        )
    }
    
    // 识别价格摆动点(高点和低点)
    fn identify_swing_points(&self, data: &[DataItem]) -> Vec<PriceLevel> {
        let mut swing_points = Vec::new();
        let window_size = 5; // 左右各考虑2个点
        
        // 动态阈值：使用ATR计算
        let atr = self.calculate_atr(data, 14) * self.atr_multiplier;
        
        // 从第window_size个到倒数第window_size个
        for i in window_size..(data.len() - window_size) {
            let current = data[i].close();
            
            // 判断是否为局部高点
            let is_high = (0..window_size).all(|j| data[i - j - 1].high() <= data[i].high())
                && (0..window_size).all(|j| data[i + j + 1].high() <= data[i].high());
                
            // 判断是否为局部低点
            let is_low = (0..window_size).all(|j| data[i - j - 1].low() >= data[i].low())
                && (0..window_size).all(|j| data[i + j + 1].low() >= data[i].low());
            
            if is_high {
                // 高点作为潜在压力位
                let strength = self.calculate_point_strength(data, i, true);
                swing_points.push(PriceLevel {
                    price: data[i].high(),
                    strength,
                    type_name: "压力".to_string(),
                    confirmation_count: self.count_confirmations(data, data[i].high(), true),
                });
            }
            
            if is_low {
                // 低点作为潜在支撑位
                let strength = self.calculate_point_strength(data, i, false);
                swing_points.push(PriceLevel {
                    price: data[i].low(),
                    strength,
                    type_name: "支撑".to_string(),
                    confirmation_count: self.count_confirmations(data, data[i].low(), false),
                });
            }
        }
        
        swing_points
    }
    
    // 计算点的强度(基于价格波动、成交量和确认次数)
    fn calculate_point_strength(&self, data: &[DataItem], index: usize, is_high: bool) -> f64 {
        let price_impact = if is_high {
            (data[index].high() - data[index-1].high()).abs() / data[index-1].high()
        } else {
            (data[index].low() - data[index-1].low()).abs() / data[index-1].low()
        };
        
        // 考虑成交量因素
        let avg_volume = data[index-5..index].iter().map(|x| x.volume()).sum::<f64>() / 5.0;
        let volume_factor = data[index].volume() / avg_volume;
        
        // 考虑时间因素(越近的点越重要)
        let recency_factor = 1.0 + (index as f64 / data.len() as f64) 
                                  * (self.recent_levels_boost - 1.0);
                                  
        // 计算确认次数因素
        let price_to_check = if is_high { data[index].high() } else { data[index].low() };
        let confirmation_count = self.count_confirmations(data, price_to_check, is_high);
        let confirmation_factor = 1.0 + (confirmation_count as f64 * 0.05); // 每次确认增加5%强度
        
        // 综合计算强度，限制在0.0-1.0范围内
        let strength = (price_impact * (1.0 - self.volume_weight) + 
                       (volume_factor - 1.0).max(0.0) * self.volume_weight) *
                       recency_factor * confirmation_factor;
                       
        (strength * 10.0).min(1.0)
    }
    
    // 计算价格水平被确认的次数
    fn count_confirmations(&self, data: &[DataItem], price: f64, is_resistance: bool) -> u32 {
        let threshold = price * self.price_cluster_threshold;
        let mut count = 0;
        
        for item in data {
            if is_resistance {
                // 对于压力位，高点接近但不超过此价位算一次确认
                if (item.high() - price).abs() < threshold && item.high() <= price {
                    count += 1;
                }
            } else {
                // 对于支撑位，低点接近但不低于此价位算一次确认
                if (item.low() - price).abs() < threshold && item.low() >= price {
                    count += 1;
                }
            }
        }
        
        count
    }
    
    // 计算斐波那契水平
    fn calculate_fibonacci_levels(&self, data: &[DataItem]) -> Vec<PriceLevel> {
        let mut fib_levels = Vec::new();
        
        // 找出区间内的最高点和最低点
        let mut max_price = f64::MIN;
        let mut min_price = f64::MAX;
        let mut max_index = 0;
        let mut min_index = 0;
        
        for (i, item) in data.iter().enumerate() {
            if item.high() > max_price {
                max_price = item.high();
                max_index = i;
            }
            if item.low() < min_price {
                min_price = item.low();
                min_index = i;
            }
        }
        
        // 确定趋势方向
        let is_uptrend = max_index > min_index;
        
        // 计算价格范围
        let price_range = max_price - min_price;
        
        // 为每个斐波那契水平创建价格水平
        for &level in &self.fibonacci_levels {
            let fib_price = if is_uptrend {
                // 上升趋势的回调水平
                max_price - price_range * level
            } else {
                // 下降趋势的反弹水平
                min_price + price_range * level
            };
            
            // 获取当前价格，判断是否已经突破/跌破该位置
            let current_price = data.last().unwrap().close();
            let last_low = data.last().unwrap().low();
            let last_high = data.last().unwrap().high();
            
            // 确定类型 - 考虑突破/跌破情况
            let type_name = if is_uptrend {
                // 上涨趋势中，斐波那契通常作为支撑
                // 但如果价格已经跌破，则变为压力
                if fib_price <= last_low {
                    "压力".to_string()
                } else {
                    "支撑".to_string()
                }
            } else {
                // 下跌趋势中，斐波那契通常作为压力
                // 但如果价格已经突破，则变为支撑
                if fib_price >= last_high {
                    "支撑".to_string()
                } else {
                    "压力".to_string()
                }
            };
            
            // 斐波那契水平的基础强度
            let base_strength = match level {
                0.382 | 0.618 => 0.7, // 黄金分割比例
                0.5 => 0.65,          // 50%回调
                0.236 => 0.5,
                0.786 => 0.5,
                _ => 0.4,
            };
            
            // 计算确认次数
            let confirmation_count = self.count_confirmations(data, fib_price, !is_uptrend);
            
            // 根据确认次数调整强度
            let strength = (base_strength + confirmation_count as f64 * 0.05).min(1.0);
            
            fib_levels.push(PriceLevel {
                price: fib_price,
                strength,
                type_name,
                confirmation_count,
            });
        }
        
        fib_levels
    }
    
    // 识别移动平均线水平
    fn identify_ma_levels(&self, data: &[DataItem]) -> Vec<PriceLevel> {
        let mut ma_levels = Vec::new();
        
        for &period in &self.ma_periods {
            if data.len() <= period {
                continue;
            }
            
            // 计算最后一个MA值
            let ma_value = data[(data.len() - period)..].iter()
                .map(|x| x.close())
                .sum::<f64>() / period as f64;
                
            // 确定支撑或压力
            let last_price = data.last().unwrap().close();
            let is_support = last_price > ma_value;
            
            // 根据周期确定强度
            let base_strength = match period {
                200 => 0.8,  // 长期均线有更强的支撑/压力作用
                100 => 0.7,
                50 => 0.6,
                _ => 0.5,
            };
            
            // 计算该均线作为支撑/压力的确认次数
            let confirmation_count = self.count_ma_confirmations(data, period, is_support);
            
            // 根据确认次数调整强度
            let strength = (base_strength + confirmation_count as f64 * 0.05).min(1.0);
            
            ma_levels.push(PriceLevel {
                price: ma_value,
                strength,
                type_name: if is_support { "支撑".to_string() } else { "压力".to_string() },
                confirmation_count,
            });
        }
        
        ma_levels
    }
    
    // 计算均线作为支撑/压力的确认次数
    fn count_ma_confirmations(&self, data: &[DataItem], period: usize, is_support: bool) -> u32 {
        let mut count = 0;
        
        // 只在有足够数据的情况下计算
        if data.len() <= period {
            return 0;
        }
        
        // 从第一个可以计算均线的点开始
        for i in period..data.len() {
            // 计算当前点的移动平均
            let ma = data[(i-period)..i].iter()
                .map(|x| x.close())
                .sum::<f64>() / period as f64;
                
            if is_support {
                // 检查价格是否接近MA并从上方反弹
                if data[i].low() <= ma * 1.01 && data[i].close() > ma {
                    count += 1;
                }
            } else {
                // 检查价格是否接近MA并从下方回落
                if data[i].high() >= ma * 0.99 && data[i].close() < ma {
                    count += 1;
                }
            }
        }
        
        count
    }
    
    // 识别成交量轮廓关键区域
    fn identify_volume_profile_levels(&self, data: &[DataItem]) -> Vec<PriceLevel> {
        // 简化的成交量轮廓实现
        let mut price_volume_map: HashMap<u64, f64> = HashMap::new();
        let min_price = data.iter().map(|x| x.low()).fold(f64::INFINITY, f64::min);
        let max_price = data.iter().map(|x| x.high()).fold(f64::NEG_INFINITY, f64::max);
        let price_range = max_price - min_price;
        
        // 创建价格区间
        const DIVISIONS: usize = 100;
        let interval = price_range / DIVISIONS as f64;
        
        // 累积每个价格区间的成交量
        for item in data {
            let avg_price = (item.high() + item.low()) / 2.0;
            let bucket = ((avg_price - min_price) / interval) as u64;
            *price_volume_map.entry(bucket).or_insert(0.0) += item.volume();
        }
        
        // 找出成交量最高的区域
        let mut high_volume_areas = Vec::new();
        let total_volume: f64 = price_volume_map.values().sum();
        
        for (bucket, volume) in price_volume_map {
            // 只考虑成交量占比较高的区域
            if volume / total_volume > 0.05 {
                let price = min_price + bucket as f64 * interval + interval / 2.0;
                
                // 根据成交量占比计算强度
                let strength = (volume / total_volume * 3.0).min(0.9);
                
                // 确定当前价格相对于该区域的位置，以判断是支撑还是压力
                let last_price = data.last().unwrap().close();
                let is_support = price < last_price;
                
                high_volume_areas.push(PriceLevel {
                    price,
                    strength,
                    type_name: if is_support { "支撑".to_string() } else { "压力".to_string() },
                    confirmation_count: 1, // 这里简化处理
                });
            }
        }
        
        high_volume_areas
    }
    
    // 识别心理价格水平
    fn identify_psychological_levels(&self, data: &[DataItem]) -> Vec<PriceLevel> {
        let mut psych_levels = Vec::new();
        
        // 确定价格范围
        let min_price = data.iter().map(|x| x.low()).fold(f64::INFINITY, f64::min);
        let max_price = data.iter().map(|x| x.high()).fold(f64::NEG_INFINITY, f64::max);
        
        // 根据价格范围确定步长
        let step = self.determine_psychological_step(min_price, max_price);
        let half_step = step / 2.0;
        
        // 当前价格
        let current_price = data.last().unwrap().close();
        
        // 生成心理价格水平
        let mut price = (min_price / step).floor() * step;
        while price <= max_price * 1.05 { // 稍微超出最高价，为未来波动预留空间
            // 整数关卡
            if self.is_psychological_level(price) {
                let is_support = price < current_price;
                let distance_factor = 1.0 - ((price - current_price).abs() / (max_price - min_price)).min(1.0);
                
                // 检查是否为圆整数(如10.0, 100.0)，这些通常有更强的心理效应
                let is_round_number = price.log10().fract() < 0.001;
                let base_strength = if is_round_number { 0.7 } else { 0.5 };
                
                psych_levels.push(PriceLevel {
                    price,
                    strength: base_strength * (0.5 + distance_factor / 2.0),
                    type_name: if is_support { "支撑".to_string() } else { "压力".to_string() },
                    confirmation_count: self.count_confirmations(data, price, !is_support),
                });
            }
            
            // 半数关卡
            price += half_step;
            if price <= max_price * 1.05 {
                let is_support = price < current_price;
                let distance_factor = 1.0 - ((price - current_price).abs() / (max_price - min_price)).min(1.0);
                
                psych_levels.push(PriceLevel {
                    price,
                    strength: 0.4 * (0.5 + distance_factor / 2.0),
                    type_name: if is_support { "支撑".to_string() } else { "压力".to_string() },
                    confirmation_count: self.count_confirmations(data, price, !is_support),
                });
            }
            
            price += half_step;
        }
        
        psych_levels
    }
    
    // 确定心理价格水平的步长
    fn determine_psychological_step(&self, min_price: f64, max_price: f64) -> f64 {
        let range = max_price - min_price;
        
        // 根据价格范围确定合适的步长
        if max_price >= 10000.0 { return 1000.0; }
        if max_price >= 1000.0 { return 100.0; }
        if max_price >= 100.0 { return 10.0; }
        if max_price >= 10.0 { return 1.0; }
        if max_price >= 1.0 { return 0.1; }
        
        0.01
    }
    
    // 判断是否为心理价格水平
    fn is_psychological_level(&self, price: f64) -> bool {
        // 整数值
        if price.fract() < 0.001 {
            return true;
        }
        
        // 检查是否为其他常见心理价格模式
        false
    }
    
    // 聚类相近的价格水平
    fn cluster_price_levels(&self, levels: Vec<PriceLevel>) -> Vec<PriceLevel> {
        if levels.is_empty() {
            return Vec::new();
        }
        
        let mut clustered = Vec::new();
        let mut clusters: Vec<Vec<PriceLevel>> = Vec::new();
        
        // 对价格进行排序
        let mut sorted_levels = levels;
        sorted_levels.sort_by(|a, b| a.price.partial_cmp(&b.price).unwrap());
        
        // 初始化第一个聚类
        let mut current_cluster = vec![sorted_levels[0].clone()];
        let mut current_base = sorted_levels[0].price;
        
        // 聚类过程
        for level in sorted_levels.into_iter().skip(1) {
            // 如果价格与当前基准价格足够接近，则加入当前聚类
            if (level.price - current_base).abs() / current_base <= self.price_cluster_threshold {
                current_cluster.push(level);
            } else {
                // 否则结束当前聚类，开始新的聚类
                clusters.push(current_cluster);
                current_cluster = vec![level.clone()];
                current_base = level.price;
            }
        }
        
        // 处理最后一个聚类
        if !current_cluster.is_empty() {
            clusters.push(current_cluster);
        }
        
        // 处理每个聚类，合并为一个强化的水平
        for cluster in clusters {
            if cluster.is_empty() {
                continue;
            }
            
            // 计算加权平均价格
            let total_weight: f64 = cluster.iter().map(|l| l.strength).sum();
            let weighted_price: f64 = cluster.iter()
                .map(|l| l.price * l.strength)
                .sum::<f64>() / total_weight;
                
            // 确定支撑/压力类型(按多数决定)
            let support_count = cluster.iter()
                .filter(|l| l.type_name == "支撑")
                .count();
            let resistance_count = cluster.len() - support_count;
            
            let type_name = if support_count >= resistance_count {
                "支撑".to_string()
            } else {
                "压力".to_string()
            };
            
            // 合并强度(取最大值并增加聚类奖励)
            let max_strength = cluster.iter().map(|l| l.strength).fold(0.0, f64::max);
            let cluster_bonus = (cluster.len() as f64 * 0.05).min(0.2); // 最多增加20%
            let strength = (max_strength + cluster_bonus).min(1.0);
            
            // 合并确认次数
            let total_confirmations = cluster.iter().map(|l| l.confirmation_count).sum();
            
            // 创建合并后的水平
            clustered.push(PriceLevel {
                price: weighted_price,
                strength,
                type_name,
                confirmation_count: total_confirmations,
            });
        }
        
        clustered
    }
    
    // 按类型分离支撑位和压力位
    fn separate_levels(
        &self, 
        levels: Vec<PriceLevel>, 
        data: &[DataItem]
    ) -> (Vec<PriceLevel>, Vec<PriceLevel>) {
        let current_price = data.last().unwrap().close();
        
        let mut support_levels = Vec::new();
        let mut resistance_levels = Vec::new();
        
        for mut level in levels {
            // 严格按照当前价格的关系划分支撑位和压力位
            if level.price < current_price {
                level.type_name = "支撑".to_string();
                support_levels.push(level);
            } else {
                level.type_name = "压力".to_string();
                resistance_levels.push(level);
            }
        }
        
        // 增加日志，便于调试
        info!(
            "分析完成: 当前价格 {:.4}, 识别出 {} 个支撑位, {} 个压力位",
            current_price, support_levels.len(), resistance_levels.len()
        );
        
        (support_levels, resistance_levels)
    }
    
    // 按强度排序价格水平
    fn sort_levels_by_strength(&self, mut levels: Vec<PriceLevel>) -> Vec<PriceLevel> {
        levels.sort_by(|a, b| b.strength.partial_cmp(&a.strength).unwrap());
        levels
    }
    
    // 计算ATR (Average True Range)
    fn calculate_atr(&self, data: &[DataItem], period: usize) -> f64 {
        if data.len() < period + 1 {
            return 0.0;
        }
        
        let mut tr_sum = 0.0;
        
        for i in 1..=period {
            let idx = data.len() - i;
            let prev_idx = idx - 1;
            
            // True Range 计算：前一根收盘价到当前高点、前一根收盘价到当前低点、当前高点到当前低点的最大值
            let tr = f64::max(
                f64::max(
                    data[idx].high() - data[idx].low(),
                    (data[prev_idx].close() - data[idx].high()).abs()
                ),
                (data[prev_idx].close() - data[idx].low()).abs()
            );
            
            tr_sum += tr;
        }
        
        tr_sum / period as f64
    }
    
    // 辅助方法：将CandlesEntity转换为DataItem
    pub fn convert_candles_to_data_items(&self, candles: &[CandlesEntity]) -> Vec<DataItem> {
        candles
            .iter()
            .map(|candle| {
                DataItem::builder()
                    .open(candle.o.parse().unwrap())
                    .high(candle.h.parse().unwrap())
                    .low(candle.l.parse().unwrap())
                    .close(candle.c.parse().unwrap())
                    .volume(candle.vol.parse().unwrap())
                    .build()
                    .unwrap()
            })
            .collect()
    }
    
    // 增加一个增强处理最近历史点位的方法
    fn enhance_recent_levels(&self, mut levels: Vec<PriceLevel>, data: &[DataItem]) -> Vec<PriceLevel> {
        // 获取全部数据的长度作为参考
        let total_length = data.len();
        
        // 逆序增强：对最近的N个交易日的高低点给予更高权重
        // 对每个价格水平，检查它是否在最近的交易中出现过
        for level in &mut levels {
            // 检查最近30%的数据
            let recent_data_start = (total_length as f64 * 0.7) as usize;
            let recent_data = &data[recent_data_start..];
            
            // 计算该价格水平在最近数据中的确认次数
            let mut recent_confirmations = 0;
            for item in recent_data {
                // 判断是否接近该价格水平
                let is_close_to_level = (item.high() - level.price).abs() < level.price * 0.01 || 
                                       (item.low() - level.price).abs() < level.price * 0.01;
                
                if is_close_to_level {
                    recent_confirmations += 1;
                }
            }
            
            // 根据最近确认次数增强强度
            if recent_confirmations > 0 {
                let boost_factor = 1.0 + (recent_confirmations as f64 * 0.02).min(0.3);
                level.strength = (level.strength * boost_factor).min(1.0);
                level.confirmation_count += recent_confirmations;
            }
        }
        
        levels
    }
    
    // 添加新方法，提供明确的获取最强支撑位和压力位的功能
    pub fn get_strongest_support<'a>(&self, support_levels: &'a [PriceLevel]) -> Option<&'a PriceLevel> {
        if support_levels.is_empty() {
            return None;
        }
        // 按照强度排序列表中的第一个是最强的
        Some(&support_levels[0])
    }
    
    pub fn get_strongest_resistance<'a>(&self, resistance_levels: &'a [PriceLevel]) -> Option<&'a PriceLevel> {
        if resistance_levels.is_empty() {
            return None;
        }
        // 按照强度排序列表中的第一个是最强的
        Some(&resistance_levels[0])
    }
    
    // 添加方法获取最接近当前价格的高强度支撑位/压力位
    // 这是可选功能，如果有需要的话
    pub fn get_closest_significant_support<'a>(&self, support_levels: &'a [PriceLevel], current_price: f64, min_strength: f64) -> Option<&'a PriceLevel> {
        support_levels.iter()
            .filter(|level| level.strength >= min_strength)
            .min_by(move |a, b| {
                let a_distance = (a.price - current_price).abs();
                let b_distance = (b.price - current_price).abs();
                a_distance.partial_cmp(&b_distance).unwrap()
            })
    }
    
    pub fn get_closest_significant_resistance<'a>(&self, resistance_levels: &'a [PriceLevel], current_price: f64, min_strength: f64) -> Option<&'a PriceLevel> {
        resistance_levels.iter()
            .filter(|level| level.strength >= min_strength)
            .min_by(move |a, b| {
                let a_distance = (a.price - current_price).abs();
                let b_distance = (b.price - current_price).abs();
                a_distance.partial_cmp(&b_distance).unwrap()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ta::DataItem;
    
    // 创建模拟的历史数据
    fn create_mock_data() -> Vec<DataItem> {
        let mut data = Vec::new();
        
        // 模拟一个上升-下降-上升的价格序列，共250个数据点
        let mut price = 100.0;
        
        // 初始上升趋势
        for i in 0..80 {
            let volatility = (i % 10) as f64 * 0.2;
            let high = price * (1.0 + 0.01 + volatility * 0.005);
            let low = price * (1.0 - 0.005 - volatility * 0.003);
            let volume = 1000.0 + volatility * 200.0;
            
            data.push(
                DataItem::builder()
                    .open(price)
                    .high(high)
                    .low(low)
                    .close(price * 1.005)
                    .volume(volume)
                    .build()
                    .unwrap()
            );
            
            price *= 1.005; // 每次上涨0.5%
        }
        
        // 下降趋势
        for i in 0..100 {
            let volatility = (i % 10) as f64 * 0.2;
            let high = price * (1.0 + 0.005 + volatility * 0.003);
            let low = price * (1.0 - 0.01 - volatility * 0.005);
            let volume = 1000.0 + (i % 5) as f64 * 500.0; // 间歇性放量
            
            data.push(
                DataItem::builder()
                    .open(price)
                    .high(high)
                    .low(low)
                    .close(price * 0.995)
                    .volume(volume)
                    .build()
                    .unwrap()
            );
            
            price *= 0.995; // 每次下跌0.5%
        }
        
        // 再次上升趋势
        for i in 0..70 {
            let volatility = (i % 10) as f64 * 0.2;
            let high = price * (1.0 + 0.012 + volatility * 0.005);
            let low = price * (1.0 - 0.004 - volatility * 0.002);
            let volume = 1000.0 + volatility * 300.0;
            
            data.push(
                DataItem::builder()
                    .open(price)
                    .high(high)
                    .low(low)
                    .close(price * 1.008)
                    .volume(volume)
                    .build()
                    .unwrap()
            );
            
            price *= 1.008; // 每次上涨0.8%
        }
        
        data
    }
    
    #[test]
    fn test_key_level_indicator() {
        // 创建模拟数据
        let data = create_mock_data();
        
        // 创建指标实例
        let indicator = KeyLevelIndicator::new();
        
        // 计算支撑位和压力位
        let (support_levels, resistance_levels) = indicator.calculate_key_levels(&data);
        
        // 打印顶部支撑位
        println!("====== 重要支撑位 (按强度排序) ======");
        for (i, level) in support_levels.iter().take(5).enumerate() {
            println!(
                "支撑位 #{}: 价格 = {:.2}, 强度 = {:.2}, 确认次数 = {}",
                i + 1, level.price, level.strength, level.confirmation_count
            );
        }
        
        // 打印顶部压力位
        println!("\n====== 重要压力位 (按强度排序) ======");
        for (i, level) in resistance_levels.iter().take(5).enumerate() {
            println!(
                "压力位 #{}: 价格 = {:.2}, 强度 = {:.2}, 确认次数 = {}",
                i + 1, level.price, level.strength, level.confirmation_count
            );
        }
        
        // 基本验证
        assert!(!support_levels.is_empty(), "应该找到至少一些支撑位");
        assert!(!resistance_levels.is_empty(), "应该找到至少一些压力位");
        
        // 验证支撑位都低于当前价格
        let current_price = data.last().unwrap().close();
        for level in &support_levels {
            assert!(
                level.price <= current_price,
                "支撑位 ({:.2}) 应该低于或等于当前价格 ({:.2})",
                level.price, current_price
            );
        }
        
        // 验证压力位都高于当前价格
        for level in &resistance_levels {
            assert!(
                level.price > current_price,
                "压力位 ({:.2}) 应该高于当前价格 ({:.2})",
                level.price, current_price
            );
        }
        
        // 验证强度排序
        for i in 1..support_levels.len() {
            assert!(
                support_levels[i-1].strength >= support_levels[i].strength,
                "支撑位应该按强度降序排序"
            );
        }
        
        for i in 1..resistance_levels.len() {
            assert!(
                resistance_levels[i-1].strength >= resistance_levels[i].strength,
                "压力位应该按强度降序排序"
            );
        }
    }
} 