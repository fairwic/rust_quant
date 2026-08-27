use super::consolidation_quality::strict_visual_parent_for_range;
use super::{
    Candle, StrictVisualBreakoutResearchVariant, StrictVisualConsolidationState,
    StrictVisualLongEntryEvent, StrictVisualLongEntryState, StrictVisualRangeEvent,
    StrictVisualRangeEvidence,
};

/// 子区间候选只能读取已经冻结的父身份和父视觉外壳。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct StrictVisualNestedRangeScope {
    pub(crate) parent: StrictVisualRangeEvidence,
    pub(crate) visual_upper: f64,
    pub(crate) visual_lower: f64,
}

impl StrictVisualLongEntryState {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn update_nested(
        &mut self,
        candles: &[Candle],
        index: usize,
        tick_size: f64,
        source_atr: Option<f64>,
        volume_event: bool,
        volume_ratio: Option<f64>,
        take_profit_atr: Option<f64>,
        variant: StrictVisualBreakoutResearchVariant,
        scope: StrictVisualNestedRangeScope,
    ) -> Option<StrictVisualLongEntryEvent> {
        self.update_with_scope(
            candles,
            index,
            tick_size,
            source_atr,
            volume_event,
            volume_ratio,
            take_profit_atr,
            variant,
            Some(scope),
        )
    }
}

impl StrictVisualConsolidationState {
    pub(crate) fn update_nested_for_variant(
        &mut self,
        candles: &[Candle],
        index: usize,
        tick_size: f64,
        variant: StrictVisualBreakoutResearchVariant,
        scope: StrictVisualNestedRangeScope,
    ) -> Option<StrictVisualRangeEvent> {
        self.update_scoped(candles, index, tick_size, variant, Some(scope))
    }
}

/// 原 V31 父状态机之外的单活动子区间；父事件始终优先并负责重置子生命周期。
#[derive(Debug, Default)]
pub(crate) struct StrictVisualNestedChildState {
    scope: Option<StrictVisualNestedRangeScope>,
    child: StrictVisualLongEntryState,
}

impl StrictVisualNestedChildState {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn update(
        &mut self,
        candles: &[Candle],
        index: usize,
        tick_size: f64,
        source_atr: Option<f64>,
        volume_event: bool,
        volume_ratio: Option<f64>,
        take_profit_atr: Option<f64>,
        variant: StrictVisualBreakoutResearchVariant,
        parent_event: Option<StrictVisualLongEntryEvent>,
    ) -> Option<StrictVisualLongEntryEvent> {
        if let Some(event) = parent_event {
            self.apply_parent_event(candles, event);
            return None;
        }
        let scope = self.scope?;
        self.child.update_nested(
            candles,
            index,
            tick_size,
            source_atr,
            volume_event,
            volume_ratio,
            take_profit_atr,
            variant,
            scope,
        )
    }

    fn apply_parent_event(&mut self, candles: &[Candle], event: StrictVisualLongEntryEvent) {
        match event {
            StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::Confirmed(range))
            | StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::ParentUpgraded(range)) => {
                self.scope = strict_visual_parent_for_range(candles, range).map(|visual| {
                    StrictVisualNestedRangeScope {
                        parent: range,
                        visual_upper: visual.upper,
                        visual_lower: visual.lower,
                    }
                });
                self.child.reset();
            }
            StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::WeakDeparturePending(_)) => {
                self.child.reset();
            }
            StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::WeakDepartureReturned(_)) => {
            }
            StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::UpperBreak(_))
            | StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::LowerBreak(_))
            | StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::WeakDepartureConsumed(_))
            | StrictVisualLongEntryEvent::Range(StrictVisualRangeEvent::InternalImpulseReset(_))
            | StrictVisualLongEntryEvent::AcceptanceArmed(_)
            | StrictVisualLongEntryEvent::DirectSignal(_) => self.clear_parent(),
            StrictVisualLongEntryEvent::AcceptanceConfirmed(_)
            | StrictVisualLongEntryEvent::AcceptanceBodyMidpointRejected(_)
            | StrictVisualLongEntryEvent::AcceptanceMarginRejected(_)
            | StrictVisualLongEntryEvent::ExternalStructureRejected(_)
            | StrictVisualLongEntryEvent::AcceptanceInvalidated(_)
            | StrictVisualLongEntryEvent::AcceptanceExpired(_) => {}
        }
    }

    fn clear_parent(&mut self) {
        self.scope = None;
        self.child.reset();
    }
}
