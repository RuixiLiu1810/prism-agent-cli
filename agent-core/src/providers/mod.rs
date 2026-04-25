pub mod chat_completions;
pub mod openai;

use chrono::{DateTime, Utc};
use reqwest::header::{HeaderMap, RETRY_AFTER};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct AgentTurnOutcome {
    pub response_id: Option<String>,
    pub messages: Vec<serde_json::Value>,
    pub suspended: bool,
}

pub(super) fn retry_delay_from_headers(headers: &HeaderMap) -> Option<Duration> {
    let raw = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    if raw.is_empty() {
        return None;
    }
    if let Ok(secs) = raw.parse::<u64>() {
        return Some(Duration::from_secs(secs.max(1)));
    }

    let retry_at = DateTime::parse_from_rfc2822(raw).ok()?.with_timezone(&Utc);
    let now = Utc::now();
    let delta_secs = (retry_at - now).num_seconds();
    if delta_secs <= 0 {
        Some(Duration::from_secs(1))
    } else {
        Some(Duration::from_secs(delta_secs as u64))
    }
}

#[cfg(test)]
mod tests {
    use super::retry_delay_from_headers;
    use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};
    use std::time::Duration;

    #[test]
    fn retry_delay_prefers_seconds_header() {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("7"));
        assert_eq!(retry_delay_from_headers(&headers), Some(Duration::from_secs(7)));
    }

    #[test]
    fn retry_delay_returns_none_for_missing_or_invalid_header() {
        let headers = HeaderMap::new();
        assert_eq!(retry_delay_from_headers(&headers), None);

        let mut bad = HeaderMap::new();
        bad.insert(RETRY_AFTER, HeaderValue::from_static("not-a-date"));
        assert_eq!(retry_delay_from_headers(&bad), None);
    }
}
