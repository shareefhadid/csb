use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "csb",
    version,
    about = "Sandboxed Claude Code on Apple container"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Arguments passed through to claude (when no subcommand is given)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub claude_args: Vec<String>,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// Launch Claude Code in the sandbox (same as bare `csb`)
    Run {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        claude_args: Vec<String>,
    },
    /// Host-side diagnostics for a running sandbox
    Doctor,
    /// Build the sandbox image (also how you update Claude Code)
    Build {
        /// Rebuild even when the image is already present and up to date
        #[arg(long)]
        force: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn claude_args_of(cli: Cli) -> Vec<String> {
        match cli.command {
            Some(Commands::Run { claude_args }) => claude_args,
            None => cli.claude_args,
            _ => panic!("expected a run dispatch"),
        }
    }

    #[test]
    fn cli_should_dispatch_to_run_when_no_args() {
        let cli = Cli::parse_from(["csb"]);
        assert!(cli.command.is_none());
        assert!(cli.claude_args.is_empty());
    }

    #[test]
    fn cli_should_dispatch_to_doctor_subcommand() {
        let cli = Cli::parse_from(["csb", "doctor"]);
        assert!(matches!(cli.command, Some(Commands::Doctor)));
    }

    #[test]
    fn cli_should_dispatch_to_build_with_force() {
        let cli = Cli::parse_from(["csb", "build", "--force"]);
        assert!(matches!(cli.command, Some(Commands::Build { force: true })));
    }

    #[test]
    fn cli_should_dispatch_to_build_without_force() {
        let cli = Cli::parse_from(["csb", "build"]);
        assert!(matches!(
            cli.command,
            Some(Commands::Build { force: false })
        ));
    }

    #[test]
    fn cli_should_passthrough_flags_when_no_subcommand() {
        let cli = Cli::parse_from(["csb", "--continue"]);
        assert!(cli.command.is_none());
        assert_eq!(claude_args_of(cli), vec!["--continue"]);
    }

    #[test]
    fn cli_should_passthrough_flags_with_explicit_run() {
        let cli = Cli::parse_from(["csb", "run", "--continue", "--verbose"]);
        assert_eq!(claude_args_of(cli), vec!["--continue", "--verbose"]);
    }

    #[test]
    fn cli_should_passthrough_prompt_flag_with_explicit_run() {
        let cli = Cli::parse_from(["csb", "run", "-p", "hello world"]);
        assert_eq!(claude_args_of(cli), vec!["-p", "hello world"]);
    }

    #[test]
    fn cli_should_passthrough_prompt_flag_when_no_subcommand() {
        let cli = Cli::parse_from(["csb", "-p", "hello world"]);
        assert_eq!(claude_args_of(cli), vec!["-p", "hello world"]);
    }

    #[test]
    fn cli_should_passthrough_help_after_separator() {
        let cli = Cli::parse_from(["csb", "--", "--help"]);
        assert_eq!(claude_args_of(cli), vec!["--help"]);
    }

    #[test]
    fn cli_should_passthrough_help_after_separator_with_run() {
        let cli = Cli::parse_from(["csb", "run", "--", "--help"]);
        assert_eq!(claude_args_of(cli), vec!["--help"]);
    }

    #[test]
    fn cli_should_passthrough_subcommand_name_after_separator() {
        let cli = Cli::parse_from(["csb", "--", "doctor"]);
        assert!(cli.command.is_none());
        assert_eq!(claude_args_of(cli), vec!["doctor"]);
    }

    #[test]
    fn cli_should_error_on_unknown_flag_for_a_subcommand() {
        // Must NOT silently fall through to launching claude.
        assert!(Cli::try_parse_from(["csb", "doctor", "--bogus"]).is_err());
        assert!(Cli::try_parse_from(["csb", "build", "--forc"]).is_err());
    }
}
