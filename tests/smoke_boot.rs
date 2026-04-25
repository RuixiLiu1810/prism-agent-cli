#[test]
fn binary_boots_with_help() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_agent-runtime"))
        .arg("--help")
        .output()
        .expect("spawn agent-runtime");
    assert!(
        out.status.success(),
        "expected success, stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}
