#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OrderState {
    #[default]
    New,
    Submitted,
    PartiallyFilled,
    Filled,
    CancelRequested,
    Canceled,
    Rejected,
}
#[derive(Default)]
pub struct OrderStateMachine {
    /// 当前状态。
    state: OrderState,
}
impl OrderStateMachine {
    /// 构建 交易执行与风控 所需实例，并集中初始化依赖和默认状态。
    pub fn new() -> Self {
        Self {
            state: OrderState::New,
        }
    }
    pub fn state(&self) -> OrderState {
        self.state
    }
    /// 执行提交步骤，串起交易执行需要的状态推进和错误处理。
    pub fn submit(&mut self) -> Result<(), String> {
        if self.state != OrderState::New {
            return Err(format!("invalid transition: {:?} -> Submitted", self.state));
        }
        self.state = OrderState::Submitted;
        Ok(())
    }
    /// 提供fill的集中实现，避免交易执行调用方重复处理相同细节。
    pub fn fill(&mut self) -> Result<(), String> {
        match self.state {
            OrderState::Submitted | OrderState::PartiallyFilled => {
                self.state = OrderState::Filled;
                Ok(())
            }
            _ => Err(format!("invalid transition: {:?} -> Filled", self.state)),
        }
    }
    /// 判断cancel，给交易执行流程提供布尔结果。
    pub fn cancel(&mut self) -> Result<(), String> {
        match self.state {
            OrderState::New
            | OrderState::Submitted
            | OrderState::PartiallyFilled
            | OrderState::CancelRequested => {
                self.state = OrderState::Canceled;
                Ok(())
            }
            _ => Err(format!("invalid transition: {:?} -> Canceled", self.state)),
        }
    }
    /// 提供reject的集中实现，避免交易执行调用方重复处理相同细节。
    pub fn reject(&mut self) -> Result<(), String> {
        match self.state {
            OrderState::New | OrderState::Submitted => {
                self.state = OrderState::Rejected;
                Ok(())
            }
            _ => Err(format!("invalid transition: {:?} -> Rejected", self.state)),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::{OrderState, OrderStateMachine};
    #[test]
    /// 提供订单状态transitions的集中实现，避免交易执行调用方重复处理相同细节。
    fn order_state_transitions() {
        let mut sm = OrderStateMachine::new();
        assert_eq!(sm.state(), OrderState::New);
        sm.submit().unwrap();
        assert_eq!(sm.state(), OrderState::Submitted);
        sm.fill().unwrap();
        assert_eq!(sm.state(), OrderState::Filled);
    }
}
