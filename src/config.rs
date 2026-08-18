pub(crate) const DEFAULT_IMAGE: &str = "claude-box";
pub(crate) const DEFAULT_MEMORY: &str = "6g";

pub(crate) struct Config {
    pub image: String,
    pub memory: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Env lookup is injected so tests don't mutate process-global state — two
    /// tests doing `set_var` in parallel race, and `set_var` is `unsafe` from
    /// edition 2024 onward.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        Self {
            image: value(&lookup, "CSB_IMAGE", DEFAULT_IMAGE),
            memory: value(&lookup, "CSB_MEMORY", DEFAULT_MEMORY),
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
    }

    #[test]
    fn config_should_read_custom_env_vars() {
        let config =
            Config::from_lookup(lookup(&[("CSB_IMAGE", "my-image"), ("CSB_MEMORY", "8g")]));
        assert_eq!(config.image, "my-image");
        assert_eq!(config.memory, "8g");
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
}
