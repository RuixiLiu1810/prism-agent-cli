#[derive(Debug, Clone, Default)]
pub struct AppContext {
    pub turn_count: usize,
}

pub async fn run_turn(ctx: &mut AppContext, _prompt: String) -> Result<(), String> {
    ctx.turn_count = ctx.turn_count.saturating_add(1);
    Ok(())
}
