use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::Path;

const DOCKERFILE: &str = include_str!("../assets/Dockerfile");
const SANDBOX_GUIDANCE: &str = include_str!("../assets/sandbox-guidance.md");

const DOCKERFILE_NAME: &str = "Dockerfile";
const GUIDANCE_NAME: &str = "sandbox-guidance.md";
const HASH_PLACEHOLDER: &str = "{{CSB_ASSETS_SHA256}}";
const HASH_LABEL: &str = "csb.assets.sha256";

const UNLABELLED_MESSAGE: &str = "Sandbox image predates csb's image tracking (built by the old \
     shell installer, or by hand). Run `csb build --force` to rebuild it.";
const STALE_MESSAGE: &str =
    "Sandbox image was built with an older version of csb. Run `csb build` to update.";

pub(crate) fn guidance_hash() -> String {
    let mut hasher = Sha256::new();
    hasher.update(SANDBOX_GUIDANCE.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(crate) fn write_build_context(dir: &Path) -> Result<()> {
    let hash = guidance_hash();
    let dockerfile = DOCKERFILE.replace(HASH_PLACEHOLDER, &hash);

    std::fs::write(dir.join(DOCKERFILE_NAME), dockerfile).context("failed to write Dockerfile")?;
    std::fs::write(dir.join(GUIDANCE_NAME), SANDBOX_GUIDANCE)
        .context("failed to write sandbox-guidance.md")?;

    Ok(())
}

pub(crate) fn build_image(image: &str) -> Result<()> {
    let dir = tempfile::tempdir().context("failed to create temp build directory")?;
    write_build_context(dir.path())?;

    let context_path = dir.path().to_str().context("invalid temp dir path")?;
    let status = crate::container::run_interactive(&["build", "-t", image, context_path]);

    // Always reclaim the builder VM's RAM, including after a failed build.
    crate::container::builder_stop();

    if !status?.success() {
        anyhow::bail!("image build failed");
    }

    Ok(())
}

/// Returns a warning when the image's baked-in assets differ from this binary's,
/// including the case where the image carries no csb label at all (pre-Rust images
/// built by the old `install.sh` — exactly the ones most likely to be out of date).
pub(crate) fn check_staleness(image: &str) -> Result<Option<String>> {
    let output = crate::container::run_output(&["image", "inspect", image])?;
    if !output.status.success() {
        // Image missing or uninspectable — nothing meaningful to compare against.
        return Ok(None);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(staleness_of(&stdout, &guidance_hash()))
}

fn staleness_of(inspect_json: &str, current_hash: &str) -> Option<String> {
    match find_label(inspect_json, HASH_LABEL) {
        None => Some(UNLABELLED_MESSAGE.into()),
        Some(hash) if hash != current_hash => Some(STALE_MESSAGE.into()),
        Some(_) => None,
    }
}

/// Pull a label value out of `container image inspect` output without taking on a
/// JSON dependency. Tolerates both `"k":"v"` and Apple's pretty-printed `"k" : "v"`,
/// and skips lookalike occurrences (e.g. the `LABEL ...` line echoed in `history`).
fn find_label<'a>(inspect_json: &'a str, key: &str) -> Option<&'a str> {
    let quoted_key = format!("\"{key}\"");
    let mut haystack = inspect_json;

    while let Some(idx) = haystack.find(&quoted_key) {
        let rest = &haystack[idx + quoted_key.len()..];
        haystack = rest;

        let Some(value_start) = rest.find('"') else {
            continue;
        };
        // Only whitespace and the colon may sit between a key and its value.
        if rest[..value_start]
            .chars()
            .any(|c| !c.is_whitespace() && c != ':')
        {
            continue;
        }
        let value = &rest[value_start + 1..];
        if let Some(end) = value.find('"') {
            return Some(&value[..end]);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const APPLE_STYLE: &str = r#"{ "Labels" : { "csb.assets.sha256" : "abc123" } }"#;
    const DOCKER_STYLE: &str = r#"{"Labels":{"csb.assets.sha256":"abc123"}}"#;
    const NO_LABEL: &str = r#"{ "config" : { "Env" : [ "IS_SANDBOX=1" ] } }"#;

    #[test]
    fn guidance_hash_should_be_deterministic() {
        let hash1 = guidance_hash();
        let hash2 = guidance_hash();
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64);
    }

    #[test]
    fn find_label_should_read_both_json_styles() {
        assert_eq!(find_label(APPLE_STYLE, HASH_LABEL), Some("abc123"));
        assert_eq!(find_label(DOCKER_STYLE, HASH_LABEL), Some("abc123"));
    }

    #[test]
    fn find_label_should_return_none_when_absent() {
        assert_eq!(find_label(NO_LABEL, HASH_LABEL), None);
    }

    #[test]
    fn staleness_should_be_none_when_hash_matches() {
        assert_eq!(staleness_of(APPLE_STYLE, "abc123"), None);
    }

    #[test]
    fn staleness_should_warn_when_hash_differs() {
        let warning = staleness_of(APPLE_STYLE, "different").expect("expected a warning");
        assert!(warning.contains("csb build"));
    }

    #[test]
    fn staleness_should_warn_when_image_has_no_label() {
        // Images built by the old shell installer carry no label at all.
        let warning = staleness_of(NO_LABEL, "abc123").expect("expected a warning");
        assert!(warning.contains("--force"));
    }

    #[test]
    fn write_build_context_should_create_correct_files() {
        let dir = tempfile::tempdir().unwrap();
        write_build_context(dir.path()).unwrap();

        assert!(dir.path().join(DOCKERFILE_NAME).exists());
        assert!(dir.path().join(GUIDANCE_NAME).exists());
    }

    #[test]
    fn write_build_context_should_replace_hash_placeholder() {
        let dir = tempfile::tempdir().unwrap();
        write_build_context(dir.path()).unwrap();

        let dockerfile = std::fs::read_to_string(dir.path().join(DOCKERFILE_NAME)).unwrap();
        assert!(
            !dockerfile.contains(HASH_PLACEHOLDER),
            "placeholder should be replaced"
        );
        assert!(
            dockerfile.contains(&guidance_hash()),
            "should contain the computed hash"
        );
    }

    #[test]
    fn write_build_context_should_emit_the_label_staleness_checks_read() {
        let dir = tempfile::tempdir().unwrap();
        write_build_context(dir.path()).unwrap();

        let dockerfile = std::fs::read_to_string(dir.path().join(DOCKERFILE_NAME)).unwrap();
        assert!(dockerfile.contains(&format!("LABEL {HASH_LABEL}=\"{}\"", guidance_hash())));
    }

    #[test]
    fn find_label_should_skip_the_history_echo_of_the_label_instruction() {
        // `container image inspect` repeats the Dockerfile instruction in `history`.
        let inspect = r#"{ "history" : [ { "created_by" : "LABEL csb.assets.sha256=\"abc123\"" } ],
                           "Labels" : { "csb.assets.sha256" : "abc123" } }"#;
        assert_eq!(find_label(inspect, HASH_LABEL), Some("abc123"));
    }

    #[test]
    fn write_build_context_should_copy_the_file_the_dockerfile_expects() {
        let dockerfile = DOCKERFILE.replace(HASH_PLACEHOLDER, "x");
        assert!(
            dockerfile.contains(&format!("COPY {GUIDANCE_NAME}")),
            "Dockerfile must COPY the filename the build context writes"
        );
    }

    #[test]
    fn write_build_context_should_preserve_guidance_content() {
        let dir = tempfile::tempdir().unwrap();
        write_build_context(dir.path()).unwrap();

        let guidance = std::fs::read_to_string(dir.path().join(GUIDANCE_NAME)).unwrap();
        assert_eq!(guidance, SANDBOX_GUIDANCE);
    }

    #[test]
    fn write_build_context_should_be_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        write_build_context(dir.path()).unwrap();
        write_build_context(dir.path()).unwrap();

        let dockerfile = std::fs::read_to_string(dir.path().join(DOCKERFILE_NAME)).unwrap();
        assert!(dockerfile.contains(&guidance_hash()));
    }
}
