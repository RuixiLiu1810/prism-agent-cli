#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HistorySearch {
    entries: Vec<String>,
    cursor: Option<usize>,
    draft: Option<String>,
    search_query: Option<String>,
    search_cursor: Option<usize>,
}

impl HistorySearch {
    pub fn record(&mut self, prompt: impl Into<String>) {
        self.entries.push(prompt.into());
        self.cursor = None;
        self.draft = None;
        self.search_query = None;
        self.search_cursor = None;
    }

    pub fn up(&mut self, current_input: &str) -> Option<String> {
        if self.entries.is_empty() {
            return None;
        }
        if self.cursor.is_none() {
            self.draft = Some(current_input.to_string());
        }
        let next = match self.cursor {
            Some(cursor) if cursor > 0 => cursor - 1,
            Some(cursor) => cursor,
            None => self.entries.len().saturating_sub(1),
        };
        self.cursor = Some(next);
        self.entries.get(next).cloned()
    }

    pub fn down(&mut self) -> Option<String> {
        if self.entries.is_empty() {
            return None;
        }
        match self.cursor {
            Some(cursor) => {
                let next = cursor.saturating_add(1);
                if next >= self.entries.len() {
                    self.cursor = None;
                    self.draft.take()
                } else {
                    self.cursor = Some(next);
                    self.entries.get(next).cloned()
                }
            }
            None => None,
        }
    }

    pub fn start_reverse_search(&mut self, query: impl Into<String>) -> Option<String> {
        let query = query.into();
        if query.is_empty() {
            return None;
        }
        self.search_query = Some(query);
        self.search_cursor = Some(self.entries.len());
        self.search_prev()
    }

    pub fn search_prev(&mut self) -> Option<String> {
        let query = self.search_query.as_deref()?;
        let start = self.search_cursor.unwrap_or(self.entries.len());
        for idx in (0..start).rev() {
            if self.entries[idx].contains(query) {
                self.search_cursor = Some(idx);
                return self.entries.get(idx).cloned();
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::HistorySearch;

    #[test]
    fn history_navigation_restores_draft_after_down() {
        let mut history = HistorySearch::default();
        history.record("first");
        history.record("second");

        assert_eq!(history.up("draft").as_deref(), Some("second"));
        assert_eq!(history.up("ignored").as_deref(), Some("first"));
        assert_eq!(history.down().as_deref(), Some("second"));
        assert_eq!(history.down().as_deref(), Some("draft"));
    }

    #[test]
    fn reverse_search_finds_previous_matching_entry() {
        let mut history = HistorySearch::default();
        history.record("read Cargo.toml");
        history.record("read src/main.rs");
        history.record("run tests");

        assert_eq!(
            history.start_reverse_search("read").as_deref(),
            Some("read src/main.rs")
        );
        assert_eq!(
            history.search_prev().as_deref(),
            Some("read Cargo.toml")
        );
    }
}
