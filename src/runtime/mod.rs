use crate::commands::Args;

pub mod session_kernel;
pub mod turn_loop;

pub fn resolved_provider(args: &Args) -> String {
    crate::providers::resolve_provider(args.provider.as_deref())
}

pub fn resolved_output(args: &Args) -> String {
    args.output
        .as_deref()
        .map_or_else(|| "human".to_string(), |v| v.trim().to_ascii_lowercase())
}
