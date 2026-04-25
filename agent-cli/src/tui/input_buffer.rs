#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InputBuffer {
    current: String,
    undo_stack: Vec<String>,
}

impl InputBuffer {
    pub fn with_text(text: impl Into<String>) -> Self {
        Self {
            current: text.into(),
            undo_stack: Vec::new(),
        }
    }

    pub fn current(&self) -> &str {
        &self.current
    }

    pub fn is_empty(&self) -> bool {
        self.current.is_empty()
    }

    pub fn insert_char(&mut self, ch: char) {
        self.push_undo();
        self.current.push(ch);
    }

    pub fn backspace(&mut self) {
        if self.current.is_empty() {
            return;
        }
        self.push_undo();
        self.current.pop();
    }

    pub fn replace(&mut self, value: impl Into<String>) {
        self.push_undo();
        self.current = value.into();
    }

    pub fn clear(&mut self) {
        if self.current.is_empty() {
            return;
        }
        self.push_undo();
        self.current.clear();
    }

    pub fn submit_trimmed(&mut self) -> Option<String> {
        let prompt = self.current.trim().to_string();
        self.current.clear();
        self.undo_stack.clear();
        if prompt.is_empty() {
            None
        } else {
            Some(prompt)
        }
    }

    pub fn undo(&mut self) -> bool {
        if let Some(previous) = self.undo_stack.pop() {
            self.current = previous;
            true
        } else {
            false
        }
    }

    fn push_undo(&mut self) {
        self.undo_stack.push(self.current.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::InputBuffer;

    #[test]
    fn submit_returns_trimmed_prompt_and_clears_buffer() {
        let mut buf = InputBuffer::with_text("  hello world  ");
        let submitted = buf.submit_trimmed();
        assert_eq!(submitted.as_deref(), Some("hello world"));
        assert!(buf.is_empty());
    }

    #[test]
    fn undo_restores_previous_snapshot() {
        let mut buf = InputBuffer::default();
        buf.insert_char('a');
        buf.insert_char('b');
        assert_eq!(buf.current(), "ab");
        assert!(buf.undo());
        assert_eq!(buf.current(), "a");
        assert!(buf.undo());
        assert_eq!(buf.current(), "");
    }

    #[test]
    fn clear_empties_the_buffer() {
        let mut buf = InputBuffer::with_text("hello");
        buf.clear();
        assert!(buf.is_empty());
    }
}
