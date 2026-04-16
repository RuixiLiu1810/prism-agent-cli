use std::future::Future;
use std::io::{self, BufRead, Write};
use std::pin::Pin;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplAction {
    Submit(String),
    Exit,
    Ignore,
}

pub fn classify_input(line: &str) -> ReplAction {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return ReplAction::Ignore;
    }
    if trimmed.eq_ignore_ascii_case("exit") || trimmed.eq_ignore_ascii_case("quit") {
        return ReplAction::Exit;
    }
    ReplAction::Submit(trimmed.to_string())
}

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

pub async fn run_repl<R, W, H>(
    mut reader: R,
    writer: &mut W,
    mut on_submit: H,
) -> Result<(), String>
where
    R: BufRead,
    W: Write,
    H: FnMut(String) -> BoxFuture<Result<(), String>>,
{
    let mut line = String::new();
    loop {
        line.clear();
        write!(writer, "> ").map_err(|e| format!("failed to write prompt: {}", e))?;
        writer
            .flush()
            .map_err(|e| format!("failed to flush prompt: {}", e))?;

        let bytes = reader
            .read_line(&mut line)
            .map_err(|e| format!("failed to read input: {}", e))?;
        if bytes == 0 {
            break;
        }

        match classify_input(&line) {
            ReplAction::Ignore => continue,
            ReplAction::Exit => break,
            ReplAction::Submit(prompt) => on_submit(prompt).await?,
        }
    }
    Ok(())
}

pub fn stdin_reader() -> io::StdinLock<'static> {
    Box::leak(Box::new(io::stdin())).lock()
}

#[cfg(test)]
mod tests {
    use super::{classify_input, ReplAction};

    #[test]
    fn classifies_exit_and_quit() {
        assert_eq!(classify_input("exit"), ReplAction::Exit);
        assert_eq!(classify_input("quit"), ReplAction::Exit);
    }

    #[test]
    fn classifies_empty_as_ignore() {
        assert_eq!(classify_input("   "), ReplAction::Ignore);
    }

    #[test]
    fn classifies_normal_text_as_submit() {
        assert_eq!(
            classify_input("hello"),
            ReplAction::Submit("hello".to_string())
        );
    }

    #[tokio::test]
    async fn repl_submits_non_empty_lines_until_exit() {
        use std::io::Cursor;
        use std::sync::{Arc, Mutex};

        let input = Cursor::new("hello\n\nworld\nexit\n");
        let mut output = Vec::new();
        let seen = Arc::new(Mutex::new(Vec::<String>::new()));
        let seen_ref = Arc::clone(&seen);

        super::run_repl(input, &mut output, move |prompt| {
            let seen_ref = Arc::clone(&seen_ref);
            Box::pin(async move {
                if let Ok(mut guard) = seen_ref.lock() {
                    guard.push(prompt);
                }
                Ok(())
            })
        })
        .await
        .unwrap_or_else(|e| panic!("repl should finish: {e}"));

        let recorded = match seen.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        assert_eq!(recorded.as_slice(), ["hello", "world"]);
    }
}
