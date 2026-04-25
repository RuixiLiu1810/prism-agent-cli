use crate::runtime::turn_loop::TurnOutcome;
use crate::state::runtime_state::RuntimeState;

#[derive(Debug, Default)]
pub struct SessionKernel {
    state: RuntimeState,
}

impl SessionKernel {
    pub fn for_test() -> Self {
        Self {
            state: RuntimeState::default(),
        }
    }

    pub async fn run_prompt(&mut self, tab_id: &str, prompt: &str) -> Result<TurnOutcome, String> {
        self.state.run_turn(tab_id, prompt).await
    }

    pub async fn approve_and_resume(
        &mut self,
        tab_id: &str,
        tool: &str,
        scope: &str,
    ) -> Result<TurnOutcome, String> {
        self.state.approve_and_resume(tab_id, tool, scope).await
    }
}
