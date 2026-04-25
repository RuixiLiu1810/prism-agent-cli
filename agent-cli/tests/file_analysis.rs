//! 文件分析场景自动化验收测试

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;


    // 用例1.1：分析单个文件内容（如统计行数、查找关键字）
    #[test]
    fn test_single_file_line_count() {
        // 1. 创建测试文件
        let test_path = "test_single.txt";
        fs::write(test_path, "line1\nline2\nline3").unwrap();

        // 2. 调用 agent runtime CLI 进行行数统计（假设有 agent-runtime count-lines）
        let output = Command::new("../../target/debug/agent-runtime")
            .args(["count-lines", test_path])
            .output()
            .expect("failed to execute agent-runtime");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("3"), "输出应包含行数3，实际输出：{}", stdout);

        // 3. 清理
        fs::remove_file(test_path).unwrap();
    }

    // 用例1.3：异常输入（如不存在的文件）
    #[test]
    fn test_file_not_found() {
        let output = Command::new("../../target/debug/agent-runtime")
            .args(["count-lines", "not_exist.txt"])
            .output()
            .expect("failed to execute agent-runtime");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("not found") || stderr.contains("不存在"), "应提示文件不存在，实际输出：{}", stderr);
    }
}
