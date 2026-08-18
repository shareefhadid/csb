use crate::config::Config;
use anyhow::{Context, Result};
use std::io::IsTerminal;
use std::path::Path;

/// Which of `-i` / `-t` to give `container run`.
///
/// `-i` is what forwards stdin, so it must be passed even when stdin is a pipe
/// (`echo "prompt" | csb -p ...`) — without it the container sees empty stdin.
/// `-t` allocates a pseudo-TTY, which translates `\n` to `\r\n`, so it may only be
/// used when *both* ends are terminals — otherwise `csb -p "..." | grep` gets
/// carriage returns in its output.
#[derive(Clone, Copy)]
pub(crate) struct Tty {
    pub stdin: bool,
    pub stdout: bool,
}

impl Tty {
    fn detect() -> Self {
        Self {
            stdin: std::io::stdin().is_terminal(),
            stdout: std::io::stdout().is_terminal(),
        }
    }
}

pub(crate) fn execute(claude_args: &[String]) -> Result<i32> {
    let config = Config::from_env();

    crate::container::ensure_available()?;
    crate::container::system_start();

    if !crate::container::image_exists(&config.image)? {
        eprintln!(
            "Sandbox image '{}' not found — building now...",
            config.image
        );
        crate::image::build_image(&config.image)?;
    } else if let Some(warning) = crate::image::check_staleness(&config.image)? {
        eprintln!("{warning}");
    }

    let pwd = std::env::current_dir().context("failed to get current directory")?;
    let home = std::env::var("HOME").context("could not determine home directory")?;

    let args = build_run_args(&config, &pwd, &home, Tty::detect(), claude_args)?;

    let status = crate::container::run_interactive(&args)?;
    Ok(status.code().unwrap_or(1))
}

pub(crate) fn build_run_args(
    config: &Config,
    pwd: &Path,
    home: &str,
    tty: Tty,
    claude_args: &[String],
) -> Result<Vec<String>> {
    let pwd = pwd.to_str().context("project path is not valid UTF-8")?;
    let claude_dir = format!("{home}/.claude");

    // `-v` splits host and container paths on ':', so a colon anywhere in either
    // path silently produces a bogus mount.
    check_mountable(pwd, "project directory")?;
    check_mountable(&claude_dir, "Claude config directory")?;

    let mut args: Vec<String> = vec!["run".into()];

    args.push("-i".into());
    if tty.stdin && tty.stdout {
        args.push("-t".into());
    }

    args.extend([
        "--rm".into(),
        "-m".into(),
        config.memory.clone(),
        "-v".into(),
        format!("{pwd}:/workspace"),
        "-w".into(),
        "/workspace".into(),
        "-v".into(),
        format!("{claude_dir}:{claude_dir}"),
        "-e".into(),
        format!("CLAUDE_CONFIG_DIR={claude_dir}"),
        config.image.clone(),
        "claude".into(),
        "--dangerously-skip-permissions".into(),
    ]);

    args.extend(claude_args.iter().cloned());

    Ok(args)
}

fn check_mountable(path: &str, label: &str) -> Result<()> {
    anyhow::ensure!(
        !path.contains(':'),
        "{label} path contains ':' ({path}), which cannot be bind-mounted into the sandbox"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        Config::from_lookup(|_| None)
    }

    fn args_for(tty: Tty, claude_args: &[String]) -> Vec<String> {
        build_run_args(
            &config(),
            Path::new("/Users/me/proj"),
            "/Users/me",
            tty,
            claude_args,
        )
        .unwrap()
    }

    const BOTH: Tty = Tty {
        stdin: true,
        stdout: true,
    };
    const PIPED_STDOUT: Tty = Tty {
        stdin: true,
        stdout: false,
    };
    const PIPED_STDIN: Tty = Tty {
        stdin: false,
        stdout: false,
    };

    #[test]
    fn run_args_should_request_a_tty_only_when_both_ends_are_terminals() {
        assert!(args_for(BOTH, &[]).contains(&"-t".to_string()));
        assert!(!args_for(PIPED_STDOUT, &[]).contains(&"-t".to_string()));
        assert!(!args_for(PIPED_STDIN, &[]).contains(&"-t".to_string()));
    }

    #[test]
    fn run_args_should_always_forward_stdin() {
        for tty in [BOTH, PIPED_STDOUT, PIPED_STDIN] {
            assert!(
                args_for(tty, &[]).contains(&"-i".to_string()),
                "-i must be present or piped input is dropped"
            );
        }
    }

    #[test]
    fn run_args_should_mount_project_and_claude_dir_at_host_path() {
        let args = args_for(BOTH, &[]);
        assert!(args.contains(&"/Users/me/proj:/workspace".to_string()));
        assert!(args.contains(&"/Users/me/.claude:/Users/me/.claude".to_string()));
        assert!(args.contains(&"CLAUDE_CONFIG_DIR=/Users/me/.claude".to_string()));
        assert!(args.contains(&"-w".to_string()));
        assert!(args.contains(&"/workspace".to_string()));
    }

    #[test]
    fn run_args_should_apply_memory_cap_and_image() {
        let config = Config::from_lookup(|key| match key {
            "CSB_MEMORY" => Some("8g".into()),
            "CSB_IMAGE" => Some("custom-box".into()),
            _ => None,
        });
        let args = build_run_args(&config, Path::new("/p"), "/h", BOTH, &[]).unwrap();
        let m = args.iter().position(|a| a == "-m").unwrap();
        assert_eq!(args[m + 1], "8g");
        assert!(args.contains(&"custom-box".to_string()));
    }

    #[test]
    fn run_args_should_pass_claude_flags_after_skip_permissions() {
        let args = args_for(BOTH, &["-p".into(), "hello world".into()]);
        let skip = args
            .iter()
            .position(|a| a == "--dangerously-skip-permissions")
            .unwrap();
        assert_eq!(args[skip + 1..], ["-p", "hello world"]);
    }

    #[test]
    fn run_args_should_use_rm_so_nothing_persists() {
        assert!(args_for(BOTH, &[]).contains(&"--rm".to_string()));
    }

    #[test]
    fn run_args_should_reject_paths_containing_a_colon() {
        assert!(build_run_args(
            &config(),
            Path::new("/Users/me/a:b"),
            "/Users/me",
            BOTH,
            &[]
        )
        .is_err());
        assert!(build_run_args(&config(), Path::new("/p"), "/Users/od:d", BOTH, &[]).is_err());
    }
}
