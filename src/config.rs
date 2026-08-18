pub(crate) const DEFAULT_IMAGE: &str = "claude-box";
pub(crate) const DEFAULT_MEMORY: &str = "6g";

pub(crate) struct Config {
    pub image: String,
    pub memory: String,
    /// True when `CSB_IMAGE` named the image. csb only rebuilds or warns about
    /// images it owns; a user-supplied one is theirs.
    pub image_is_custom: bool,
    /// Ready-to-pass `KEY=value` pairs for `container run -e`.
    pub env: Vec<String>,
}

impl Config {
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Env lookup is injected so tests don't mutate process-global state — two
    /// tests doing `set_var` in parallel race, and `set_var` is `unsafe` from
    /// edition 2024 onward.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let image = value(&lookup, "CSB_IMAGE", DEFAULT_IMAGE);
        Self {
            image_is_custom: image != DEFAULT_IMAGE,
            image,
            memory: value(&lookup, "CSB_MEMORY", DEFAULT_MEMORY),
            env: env_pairs(&lookup, "CSB_ENV"),
        }
    }
}

/// An empty or whitespace-only override is a mistake (`export CSB_MEMORY=`), not a
/// request to pass `-m ""` to `container`.
fn value(lookup: &impl Fn(&str) -> Option<String>, key: &str, default: &str) -> String {
    lookup(key)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.to_string())
}

/// `CSB_ENV` is a comma-separated list. `NAME=value` passes through literally;
/// a bare `NAME` forwards that variable's value from the host, and is dropped
/// when the host doesn't have it set.
fn env_pairs(lookup: &impl Fn(&str) -> Option<String>, key: &str) -> Vec<String> {
    let Some(raw) = lookup(key) else {
        return Vec::new();
    };

    raw.split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| match entry.split_once('=') {
            Some(_) => Some(entry.to_string()),
            None => lookup(entry).map(|v| format!("{entry}={v}")),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn config_should_use_defaults_when_env_vars_missing() {
        let config = Config::from_lookup(lookup(&[]));
        assert_eq!(config.image, "claude-box");
        assert_eq!(config.memory, "6g");
        assert!(!config.image_is_custom);
        assert!(config.env.is_empty());
    }

    #[test]
    fn config_should_read_custom_env_vars() {
        let config =
            Config::from_lookup(lookup(&[("CSB_IMAGE", "my-image"), ("CSB_MEMORY", "8g")]));
        assert_eq!(config.image, "my-image");
        assert_eq!(config.memory, "8g");
        assert!(config.image_is_custom);
    }

    #[test]
    fn config_should_not_treat_the_default_image_name_as_custom() {
        let config = Config::from_lookup(lookup(&[("CSB_IMAGE", "claude-box")]));
        assert!(!config.image_is_custom);
    }

    #[test]
    fn config_should_fall_back_when_env_vars_are_empty_or_whitespace() {
        let config = Config::from_lookup(lookup(&[("CSB_IMAGE", ""), ("CSB_MEMORY", "   ")]));
        assert_eq!(config.image, "claude-box");
        assert_eq!(config.memory, "6g");
    }

    #[test]
    fn config_should_trim_surrounding_whitespace() {
        let config = Config::from_lookup(lookup(&[("CSB_MEMORY", " 8g ")]));
        assert_eq!(config.memory, "8g");
    }

    #[test]
    fn env_should_pass_explicit_pairs_through() {
        let config = Config::from_lookup(lookup(&[(
            "CSB_ENV",
            "CONTEXT_MODE_DIR=/workspace/.context-mode, RTK_QUIET=1",
        )]));
        assert_eq!(
            config.env,
            vec!["CONTEXT_MODE_DIR=/workspace/.context-mode", "RTK_QUIET=1"]
        );
    }

    #[test]
    fn env_should_forward_bare_names_from_the_host() {
        let config = Config::from_lookup(lookup(&[
            ("CSB_ENV", "GH_TOKEN,MISSING_ONE"),
            ("GH_TOKEN", "secret"),
        ]));
        assert_eq!(config.env, vec!["GH_TOKEN=secret"]);
    }

    #[test]
    fn env_should_ignore_empty_entries() {
        let config = Config::from_lookup(lookup(&[("CSB_ENV", " , A=1 ,, ")]));
        assert_eq!(config.env, vec!["A=1"]);
    }
}
