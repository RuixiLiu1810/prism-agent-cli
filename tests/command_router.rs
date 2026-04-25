use prism_agent_cli::commands::registry;
use prism_agent_cli::services::turn_service::AppContext;

#[test]
fn command_registry_contains_help_and_status() {
    let reg = registry();
    assert!(reg.contains_key("/help"));
    assert!(reg.contains_key("/status"));
}

#[test]
fn command_handlers_execute_without_error() {
    let reg = registry();
    let mut ctx = AppContext::default();
    let help = reg
        .get("/help")
        .unwrap_or_else(|| panic!("missing /help handler"));
    let status = reg
        .get("/status")
        .unwrap_or_else(|| panic!("missing /status handler"));

    assert!(help(&mut ctx, &[]).is_ok());
    assert!(status(&mut ctx, &[]).is_ok());
}
