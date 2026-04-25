use crate::state::BootstrapState;

pub fn print_repl_banner(state: &BootstrapState) {
    println!(
        "[bootstrap] provider={} model={} project={} output={} protocol=v{}",
        state.provider,
        state.model,
        state.project_path,
        state.output_mode,
        crate::protocol::version()
    );
}

pub fn print_single_turn_hint(state: &BootstrapState, prompt: Option<&str>) {
    let prompt_text = prompt.unwrap_or_default().trim();
    println!(
        "[single-turn] provider={} prompt={} tool_mode={}",
        state.provider,
        if prompt_text.is_empty() {
            "<empty>"
        } else {
            prompt_text
        },
        state.tool_mode
    );
}
