mod cli;
mod commands;
mod config;
mod container;
mod image;

use anyhow::Result;
use clap::Parser;

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("csb: {e:#}");
            std::process::exit(1);
        }
    }
}

fn run() -> Result<i32> {
    // Parse errors (unknown flags on a real subcommand, bad usage) must surface as
    // errors — never as a silent `claude` launch. clap exits with its own message.
    let cli = cli::Cli::parse();

    match cli.command {
        Some(cli::Commands::Run { claude_args }) => commands::run::execute(&claude_args),
        Some(cli::Commands::Doctor) => {
            commands::doctor::execute()?;
            Ok(0)
        }
        Some(cli::Commands::Build { force }) => {
            commands::build::execute(force)?;
            Ok(0)
        }
        None => commands::run::execute(&cli.claude_args),
    }
}
