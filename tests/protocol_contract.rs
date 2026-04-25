use prism_agent_cli::output::jsonl::{encode_complete, encode_status};

#[test]
fn jsonl_status_line_uses_stable_contract() {
    let line = encode_status("tab-1", "streaming", "Connected");
    let value: serde_json::Value = serde_json::from_str(&line)
        .unwrap_or_else(|err| panic!("status line should be valid json: {err}"));

    assert_eq!(value["tabId"], "tab-1");
    assert_eq!(value["payload"]["type"], "status");
    assert_eq!(value["payload"]["stage"], "streaming");
    assert_eq!(value["payload"]["message"], "Connected");
}

#[test]
fn jsonl_complete_line_uses_stable_contract() {
    let line = encode_complete("tab-9", "completed");
    let value: serde_json::Value = serde_json::from_str(&line)
        .unwrap_or_else(|err| panic!("complete line should be valid json: {err}"));

    assert_eq!(value["tabId"], "tab-9");
    assert_eq!(value["payload"]["type"], "complete");
    assert_eq!(value["payload"]["outcome"], "completed");
    assert_eq!(value["protocolVersion"], 1);
}
