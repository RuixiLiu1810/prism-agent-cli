use std::io::{self, Write};

use agent_core::{
    AgentCompletePayload, AgentEventEnvelope, AgentRuntimeConfig, EventSink,
    StaticConfigProvider,
};
use clap::Parser;

/// Claude Prism agent runtime (standalone CLI).
///
/// Runs the agent outside of the Tauri desktop app.
/// Events are emitted as JSON Lines on stdout.
#[derive(Parser, Debug)]
#[command(name = "agent-runtime", version)]
struct Args {
    /// API key for the LLM provider
    #[arg(long, env = "AGENT_API_KEY")]
    api_key: String,

    /// Provider name (e.g. openai, anthropic, deepseek, minimax)
    #[arg(long, env = "AGENT_PROVIDER", default_value = "openai")]
    provider: String,

    /// Model name (e.g. o4-mini, claude-sonnet-4-20250514)
    #[arg(long, env = "AGENT_MODEL")]
    model: String,

    /// Base URL for the provider API
    #[arg(long, env = "AGENT_BASE_URL")]
    base_url: Option<String>,

    /// Path to the project directory
    #[arg(long)]
    project_path: String,
}

/// EventSink that writes JSON Lines to stdout.
struct StdioEventSink;

impl EventSink for StdioEventSink {
    fn emit_event(&self, envelope: &AgentEventEnvelope) {
        if let Ok(json) = serde_json::to_string(envelope) {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            let _ = writeln!(handle, "{}", json);
        }
    }

    fn emit_complete(&self, payload: &AgentCompletePayload) {
        if let Ok(json) = serde_json::to_string(payload) {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            let _ = writeln!(handle, "{}", json);
        }
    }
}

fn default_base_url(provider: &str) -> &str {
    match provider {
        "openai" => "https://api.openai.com/v1",
        "anthropic" => "https://api.anthropic.com/v1",
        "deepseek" => "https://api.deepseek.com/v1",
        "minimax" => "https://api.minimax.chat/v1",
        _ => "https://api.openai.com/v1",
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let base_url = args
        .base_url
        .unwrap_or_else(|| default_base_url(&args.provider).to_string());

    let config = {
        let mut c = AgentRuntimeConfig::default_local_agent();
        c.provider = args.provider;
        c.model = args.model;
        c.api_key = Some(args.api_key);
        c.base_url = base_url;
        c
    };

    let config_provider = StaticConfigProvider {
        config,
        config_dir: std::env::temp_dir().join("agent-runtime"),
    };

    let _event_sink = StdioEventSink;

    // TODO: Wire up the turn execution loop once provider-specific
    // streaming functions (openai::run_turn_loop, chat_completions::run_turn_loop)
    // are migrated from the Tauri adapter to agent-core.
    //
    // For now the CLI binary validates arguments and configuration,
    // and the StdioEventSink + StaticConfigProvider are ready to plug
    // into the turn loop when it becomes available.
    eprintln!(
        "agent-runtime: ready (provider={}, project={})",
        config_provider.config.provider, args.project_path
    );
    eprintln!("agent-runtime: turn loop not yet wired — awaiting provider migration to agent-core");
}
