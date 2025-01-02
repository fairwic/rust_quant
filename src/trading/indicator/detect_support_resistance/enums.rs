/// 支撑或阻力类型
#[derive(Debug, Clone, Copy)]
pub enum LevelType {
    Support,
    Resistance,
}

impl PartialEq for LevelType {
    fn eq(&self, other: &Self) -> bool {
        // 可以使用 matches! 宏简化写法
        matches!(
            (self, other),
            (LevelType::Support, LevelType::Support)
                | (LevelType::Resistance, LevelType::Resistance)
        )
    }
}

/// BOS/CHoCH 类型（示例：暂时只有这两种）
#[derive(Debug, Clone, Copy)]
pub enum BreakoutType {
    BOS,
    CHoCH,
}

/// 用于存储标记出的关键信息
#[derive(Debug, Clone)]
pub struct SupportResistance {
    pub index: usize,                   // 在 Candle 列表中的索引
    pub ts: i64,                        // 时间戳
    pub price: f64,                     // 该支撑/阻力所处价位
    pub level_type: LevelType,          // 支撑 or 阻力
    pub breakout: Option<BreakoutType>, // 是否被突破，以及是 BOS 还是 CHoCH
}

/// 简单趋势状态（示例）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrendState {
    Up,
    Down,
    Unknown,
}
