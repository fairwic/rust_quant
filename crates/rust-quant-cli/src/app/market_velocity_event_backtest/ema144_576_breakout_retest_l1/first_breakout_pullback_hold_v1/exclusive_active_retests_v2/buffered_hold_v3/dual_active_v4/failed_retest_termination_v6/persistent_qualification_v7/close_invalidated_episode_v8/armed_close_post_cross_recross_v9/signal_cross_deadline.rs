use super::V9ActiveDirection;

/// 首个交叉前交易信号启动的单次 EMA144/576 方向确认期限。
#[derive(Debug, Clone, Copy)]
pub(super) struct V9SignalCrossDeadline {
    /// 首个交叉前交易信号所属的 episode 快照。
    pub(super) origin_active: V9ActiveDirection,
    /// 首个交易信号完成 K 的索引；后续 episode 替换不得重置。
    pub(super) first_signal_idx: usize,
    /// 首个交易信号完成 K 的 Unix 毫秒时间戳。
    pub(super) first_signal_ts: i64,
}

/// 启动、确认或超时事件及其发生时间，用于输出完整生命周期证据。
#[derive(Debug, Clone, Copy)]
pub(super) struct V9SignalCrossDeadlineEvent {
    /// 首个信号计时上下文。
    pub(super) deadline: V9SignalCrossDeadline,
    /// 当前事件完成 K 的 Unix 毫秒时间戳。
    pub(super) ts: i64,
}
