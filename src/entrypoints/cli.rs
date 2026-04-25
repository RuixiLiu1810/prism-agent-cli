use clap::Parser;

use crate::commands::{Args, Command, ConfigSubcommand};

pub fn run() {
    let args = Args::parse();
    let state = crate::state::BootstrapState::from_args(&args);

    match args.run_mode() {
        crate::commands::RunMode::Command => run_command(&args),
        crate::commands::RunMode::SingleTurn => {
            crate::output::print_single_turn_hint(&state, args.prompt.as_deref());
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
