use prism_agent_cli::runtime::session_kernel::SessionKernel;

#[tokio::test]
async fn suspended_turn_can_resume_in_same_session() {
    let mut kernel = SessionKernel::for_test();

    let first = kernel
        .run_prompt("tab-1", "run shell command")
        .await
        .unwrap_or_else(|err| panic!("run_prompt should suspend: {err}"));
    assert!(first.suspended);
    assert_eq!(first.stage, "awaiting_approval");

    let resumed = kernel
        .approve_and_resume("tab-1", "shell", "once")
        .await
        .unwrap_or_else(|err| panic!("approve_and_resume should finish turn: {err}"));
    assert!(!resumed.suspended);
    assert_eq!(resumed.stage, "completed");
}
