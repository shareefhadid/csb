//! User-supplied image layers.
//!
//! csb's own image is compiled into the binary, which makes it reproducible but
//! unextendable — adding a tool used to mean forking the repo. An overlay is a
//! Dockerfile at `~/.config/csb/Dockerfile` that csb builds *on top of* its base
//! image, so personal tooling lives in the user's config instead of in csb.

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const OVERLAY_NAME: &str = "Dockerfile";
pub(crate) const LOCAL_LABEL: &str = "csb.local.sha256";

pub(crate) struct Overlay {
    /// Build context: the config dir, so `COPY` can reach the user's own files.
    pub dir: PathBuf,
    pub content: String,
}

/// `$XDG_CONFIG_HOME/csb/Dockerfile`, else `~/.config/csb/Dockerfile`.
pub(crate) fn config_dir(lookup: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    if let Some(xdg) = lookup("XDG_CONFIG_HOME").filter(|v| !v.trim().is_empty()) {
        return Some(PathBuf::from(xdg).join("csb"));
    }
    lookup("HOME")
        .filter(|v| !v.trim().is_empty())
        .map(|home| PathBuf::from(home).join(".config").join("csb"))
}

pub(crate) fn discover() -> Result<Option<Overlay>> {
    let Some(dir) = config_dir(|key| std::env::var(key).ok()) else {
        return Ok(None);
    };
    let path = dir.join(OVERLAY_NAME);
    if !path.is_file() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read overlay {}", path.display()))?;
    Ok(Some(Overlay { dir, content }))
}

/// An overlay is usually just `RUN`/`ENV`/`COPY` lines, so csb supplies the
/// `FROM`. A `FROM` written by hand wins — that's the escape hatch for anyone
/// who wants a different base entirely.
pub(crate) fn compose(content: &str, base_image: &str) -> String {
    if has_from(content) {
        return content.to_string();
    }
    format!("FROM {base_image}\n\n{content}")
}

fn has_from(content: &str) -> bool {
    content.lines().any(|line| {
        let line = line.trim_start();
        line.len() >= 4 && line[..4].eq_ignore_ascii_case("from") && {
            let rest = &line[4..];
            rest.starts_with(char::is_whitespace)
        }
    })
}

/// Ties the built image to both the overlay's content and the base it was built
/// on, so upgrading csb invalidates a stale local layer too.
pub(crate) fn hash(composed: &str, base_hash: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(composed.as_bytes());
    hasher.update(b"\0");
    hasher.update(base_hash.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// `claude-box` -> `claude-box-local`, preserving any tag.
pub(crate) fn derived_image_name(image: &str) -> String {
    match image.rsplit_once(':') {
        Some((name, tag)) if !tag.contains('/') => format!("{name}-local:{tag}"),
        _ => format!("{image}-local"),
    }
}

pub(crate) fn build(
    overlay: &Overlay,
    base_image: &str,
    derived: &str,
    base_hash: &str,
) -> Result<()> {
    let composed = compose(&overlay.content, base_image);
    let label = format!("{LOCAL_LABEL}={}", hash(&composed, base_hash));

    let dir = tempfile::tempdir().context("failed to create temp build directory")?;
    let dockerfile = dir.path().join(OVERLAY_NAME);
    std::fs::write(&dockerfile, &composed)
        .context("failed to write composed overlay Dockerfile")?;

    let status = crate::container::run_interactive(&[
        "build",
        "-t",
        derived,
        "-f",
        path_str(&dockerfile)?,
        "--label",
        &label,
        path_str(&overlay.dir)?,
    ]);

    crate::container::builder_stop();

    if !status?.success() {
        anyhow::bail!("overlay image build failed");
    }
    Ok(())
}

fn path_str(path: &Path) -> Result<&str> {
    path.to_str()
        .with_context(|| format!("path is not valid UTF-8: {}", path.display()))
}

/// `Some(reason)` when the derived image needs rebuilding.
pub(crate) fn staleness(inspect_json: &str, expected: &str) -> Option<String> {
    match crate::image::find_label(inspect_json, LOCAL_LABEL) {
        Some(found) if found == expected => None,
        Some(_) => Some("Your ~/.config/csb/Dockerfile changed since this image was built.".into()),
        None => Some("Local image is missing its csb overlay label.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_should_supply_the_base_when_the_overlay_has_no_from() {
        let composed = compose("RUN npm install -g context-mode\n", "claude-box");
        assert!(composed.starts_with("FROM claude-box\n"));
        assert!(composed.contains("RUN npm install -g context-mode"));
    }

    #[test]
    fn compose_should_respect_a_hand_written_from() {
        let overlay = "FROM my-own-base\nRUN true\n";
        assert_eq!(compose(overlay, "claude-box"), overlay);
    }

    #[test]
    fn compose_should_detect_from_case_insensitively_and_indented() {
        assert_eq!(compose("  from x\n", "claude-box"), "  from x\n");
    }

    #[test]
    fn compose_should_not_mistake_other_directives_for_from() {
        // A word merely starting with "from" is not a FROM instruction.
        let overlay = "RUN echo fromage\nENV FROMAGE=1\n";
        assert!(compose(overlay, "claude-box").starts_with("FROM claude-box"));
    }

    #[test]
    fn hash_should_change_with_overlay_or_base() {
        let a = hash("FROM x\nRUN a", "base1");
        assert_eq!(a, hash("FROM x\nRUN a", "base1"));
        assert_ne!(a, hash("FROM x\nRUN b", "base1"));
        assert_ne!(a, hash("FROM x\nRUN a", "base2"));
    }

    #[test]
    fn derived_name_should_preserve_tags() {
        assert_eq!(derived_image_name("claude-box"), "claude-box-local");
        assert_eq!(derived_image_name("claude-box:v2"), "claude-box-local:v2");
        assert_eq!(derived_image_name("ghcr.io:5000/x"), "ghcr.io:5000/x-local");
    }

    #[test]
    fn config_dir_should_prefer_xdg_then_home() {
        let xdg = config_dir(|k| match k {
            "XDG_CONFIG_HOME" => Some("/x/cfg".into()),
            "HOME" => Some("/Users/me".into()),
            _ => None,
        });
        assert_eq!(xdg.unwrap(), PathBuf::from("/x/cfg/csb"));

        let home = config_dir(|k| (k == "HOME").then(|| "/Users/me".to_string()));
        assert_eq!(home.unwrap(), PathBuf::from("/Users/me/.config/csb"));

        assert!(config_dir(|_| None).is_none());
        assert!(config_dir(|k| (k == "XDG_CONFIG_HOME").then(|| "  ".to_string())).is_none());
    }

    #[test]
    fn staleness_should_flag_changed_and_unlabelled_images() {
        let current = r#"{"Labels":{"csb.local.sha256":"abc"}}"#;
        assert_eq!(staleness(current, "abc"), None);
        assert!(staleness(current, "def").is_some());
        assert!(staleness(r#"{"Labels":{}}"#, "abc").is_some());
    }
}
