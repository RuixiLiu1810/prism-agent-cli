use std::collections::HashMap;

use crate::runtime::turn_loop::TurnOutcome;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingTurn {
    pub tab_id: String,
    pub prompt: String,
    pub approval_tool: String,
}

#[derive(Debug, Default)]
pub struct RuntimeState {
    pending_turns: HashMap<String, PendingTurn>,
    approvals: HashMap<String, HashMap<String, String>>,
}

impl RuntimeState {
    pub async fn run_turn(&mut self, tab_id: &str, prompt: &str) -> Result<TurnOutcome, String> {
        if self.pending_turns.contains_key(tab_id) {
            return Err(format!("tab '{tab_id}' already has a suspended turn"));
        }

        let pending = PendingTurn {
            tab_id: tab_id.to_string(),
            prompt: prompt.to_string(),
            approval_tool: "shell".to_string(),
        };
        self.pending_turns.insert(tab_id.to_string(), pending);

        Ok(TurnOutcome::suspended(
            tab_id,
            "Approval required before continuing this turn.",
        ))
    }

    pub async fn approve_and_resume(
        &mut self,
        tab_id: &str,
        tool: &str,
        scope: &str,
    ) -> Result<TurnOutcome, String> {
        let Some(pending) = self.pending_turns.remove(tab_id) else {
            return Err(format!("no suspended turn found for tab '{tab_id}'"));
        };

        if pending.approval_tool != tool {
            return Err(format!(
                "approval tool mismatch: expected '{}', got '{}'",
                pending.approval_tool, tool
            ));
        }

        self.approvals
            .entry(tab_id.to_string())
            .or_default()
            .insert(tool.to_string(), scope.to_string());

        Ok(TurnOutcome::completed(
            tab_id,
            format!("Turn resumed with {tool}:{scope} approval."),
        ))
    }

    pub fn pending_turn_for(&self, tab_id: &str) -> Option<&PendingTurn> {
        self.pending_turns.get(tab_id)
    }

    pub fn approval_scope_for(&self, tab_id: &str, tool: &str) -> Option<&str> {
        self.approvals
            .get(tab_id)
            .and_then(|records| records.get(tool))
            .map(String::as_str)
    }
}
