use anyhow::{Context, Result};
use std::ffi::OsStr;
use std::process::{ExitStatus, Output};

pub(crate) fn ensure_available() -> Result<()> {
    which::which("container").context(
        "Apple 'container' not found. Install: brew install container (requires macOS 26+)",
    )?;
    Ok(())
}

pub(crate) fn system_start() {
    let _ = std::process::Command::new("container")
        .args(["system", "start"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

pub(crate) fn run_interactive<S: AsRef<OsStr>>(args: &[S]) -> Result<ExitStatus> {
    std::process::Command::new("container")
        .args(args)
        .status()
        .context("failed to launch container")
}

pub(crate) fn run_output(args: &[&str]) -> Result<Output> {
    std::process::Command::new("container")
        .args(args)
        .output()
        .context("failed to run container command")
}

/// Like `run_output`, but a non-zero exit is an error carrying the command's
/// stderr — so a failed `container` call can never be mistaken for empty output.
pub(crate) fn run_checked(args: &[&str]) -> Result<String> {
    let output = run_output(args)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.trim();
        anyhow::bail!(
            "`container {}` failed{}{}",
            args.join(" "),
            if detail.is_empty() { "" } else { ": " },
            detail
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub(crate) fn image_exists(image: &str) -> Result<bool> {
    let stdout = run_checked(&["image", "ls"])?;
    Ok(image_listed(&stdout, image))
}

pub(crate) fn builder_stop() {
    let _ = std::process::Command::new("container")
        .args(["builder", "stop"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// Split `name[:tag]`, taking care not to mistake a registry port for a tag
/// (`ghcr.io:5000/foo` has no tag).
fn split_image_ref(image: &str) -> (&str, Option<&str>) {
    match image.rsplit_once(':') {
        Some((name, tag)) if !tag.contains('/') => (name, Some(tag)),
        _ => (image, None),
    }
}

fn is_header(line: &&str) -> bool {
    let mut cols = line.split_whitespace();
    cols.next() == Some("NAME") && cols.next() == Some("TAG")
}

/// `container image ls` prints `NAME  TAG  DIGEST`, so an image referenced with an
/// explicit tag (`claude-box:latest`) never matches the NAME column on its own.
fn image_listed(stdout: &str, image: &str) -> bool {
    let (name, tag) = split_image_ref(image);
    stdout.lines().skip_while(is_header).any(|line| {
        let mut cols = line.split_whitespace();
        if cols.next() != Some(name) {
            return false;
        }
        match tag {
            None => true,
            Some(tag) => cols.next() == Some(tag),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LS: &str = "\
NAME                                          TAG      DIGEST
claude-box                                    latest   4f9d8d0eb9c9
node                                          22-slim  e21fc383b50d
ghcr.io/apple/container-builder-shim/builder  0.12.0   edf820e05c33
";

    #[test]
    fn image_listed_should_match_untagged_name() {
        assert!(image_listed(LS, "claude-box"));
        assert!(image_listed(LS, "node"));
    }

    #[test]
    fn image_listed_should_match_name_with_explicit_tag() {
        assert!(image_listed(LS, "claude-box:latest"));
        assert!(image_listed(LS, "node:22-slim"));
    }

    #[test]
    fn image_listed_should_reject_wrong_tag() {
        assert!(!image_listed(LS, "claude-box:v2"));
    }

    #[test]
    fn image_listed_should_reject_missing_image() {
        assert!(!image_listed(LS, "nope"));
        assert!(!image_listed("", "claude-box"));
    }

    #[test]
    fn image_listed_should_not_match_the_header_or_substrings() {
        assert!(!image_listed(LS, "NAME"));
        assert!(!image_listed(LS, "claude"));
    }

    #[test]
    fn split_image_ref_should_ignore_registry_ports() {
        assert_eq!(split_image_ref("claude-box"), ("claude-box", None));
        assert_eq!(
            split_image_ref("claude-box:latest"),
            ("claude-box", Some("latest"))
        );
        assert_eq!(
            split_image_ref("ghcr.io:5000/team/img"),
            ("ghcr.io:5000/team/img", None)
        );
    }
}
