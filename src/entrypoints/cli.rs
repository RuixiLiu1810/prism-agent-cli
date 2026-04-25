use clap::Parser;

use crate::commands::{parse_output_mode, Args, Command, ConfigSubcommand, OutputMode};

pub fn run() {
    let args = Args::parse();
    let state = crate::state::BootstrapState::from_args(&args);

    match args.run_mode() {
        crate::commands::RunMode::Command => run_command(&args),
        crate::commands::RunMode::SingleTurn => {
            let output_mode = args
                .output
                .as_deref()
                .and_then(|raw| parse_output_mode(raw).ok())
                .unwrap_or(OutputMode::Human);
            match output_mode {
                OutputMode::Human => {
                    crate::output::print_single_turn_hint(&state, args.prompt.as_deref());
                }
                OutputMode::Jsonl => {
                    let prompt = args.prompt.as_deref().unwrap_or_default();
                    println!(
                        "{}",
                        crate::output::jsonl::encode_status(
                            &args.tab_id,
                            "completed",
                            if prompt.trim().is_empty() {
                                "single turn completed"
                            } else {
                                "single turn completed for prompt"
                            },
                        )
                    );
                    println!(
                        "{}",
                        crate::output::jsonl::encode_complete(&args.tab_id, "completed")
                    );
                }
            }
        }
        crate::commands::RunMode::Repl => {
            crate::output::print_repl_banner(&state);
        }
    }
}

fn run_command(args: &Args) {
    match &args.command {
        Some(Command::Config { action }) => match action {
            ConfigSubcommand::Init => println!("config init requested"),
            ConfigSubcommand::Edit => println!("config edit requested"),
            ConfigSubcommand::Show => println!("config show requested"),
            ConfigSubcommand::Path => println!("config path requested"),
        },
        None => println!("no command provided"),
    }
}
