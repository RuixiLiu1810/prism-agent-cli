pub fn resolve_provider(raw: Option<&str>) -> String {
    raw.map_or_else(|| "chat_completions".to_string(), |v| v.trim().to_string())
}
