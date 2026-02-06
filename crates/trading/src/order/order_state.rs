#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderState {
    New,
    Submitted,
    PartiallyFilled,
    Filled,
    CancelRequested,
    Canceled,
    Rejected,
}

pub struct OrderStateMachine {
    state: OrderState,
}

impl OrderStateMachine {
    pub fn new() -> Self {
        Self { state: OrderState::New }
    }

    pub fn state(&self) -> OrderState {
        self.state
    }

    pub fn submit(&mut self) -> Result<(), String> {
        if self.state != OrderState::New {
            return Err(format!("invalid transition: {:?} -> Submitted", self.state));
        }
        self.state = OrderState::Submitted;
        Ok(())
    }

    pub fn fill(&mut self) -> Result<(), String> {
        match self.state {
            OrderState::Submitted | OrderState::PartiallyFilled => {
                self.state = OrderState::Filled;
                Ok(())
            }
            _ => Err(format!("invalid transition: {:?} -> Filled", self.state)),
        }
    }

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
    fn order_state_transitions() {
        let mut sm = OrderStateMachine::new();
        assert_eq!(sm.state(), OrderState::New);
        sm.submit().unwrap();
        assert_eq!(sm.state(), OrderState::Submitted);
        sm.fill().unwrap();
        assert_eq!(sm.state(), OrderState::Filled);
    }
}
