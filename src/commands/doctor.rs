use anyhow::Result;

pub(crate) fn execute() -> Result<()> {
    let config = crate::config::Config::from_env();

    crate::container::ensure_available()?;

    check_shell_shadow();

    println!("== containers ==");
    match crate::container::run_checked(&["ls"]) {
        Ok(stdout) => print!("{stdout}"),
        Err(e) => println!("  could not list containers: {e:#}"),
    }

    println!("\n== live CPU/MEM (MEM near the cap => OOM, the usual freeze cause) ==");
    match crate::container::run_checked(&["stats", "--no-stream"]) {
        Ok(stdout) if !stdout.trim().is_empty() => print!("{stdout}"),
        _ => println!("  (none running)"),
    }

    println!("\n== image ==");
    match crate::container::image_exists(&config.image) {
        Ok(true) => match crate::image::check_staleness(&config.image, !config.image_is_custom)? {
            Some(warning) => println!("  {warning}"),
            None => println!("  '{}' present and up to date.", config.image),
        },
        Ok(false) => println!(
            "  Image '{}' not found. Run `csb build` to create it.",
            config.image
        ),
        Err(e) => println!("  could not check image: {e:#}"),
    }

    report_overlay(&config)?;

    Ok(())
}

fn report_overlay(config: &crate::config::Config) -> Result<()> {
    println!("\n== overlay ==");

    let Some(overlay) = crate::overlay::discover()? else {
        println!("  none — add one at ~/.config/csb/Dockerfile to extend the image.");
        return Ok(());
    };

    let derived = crate::overlay::derived_image_name(&config.image);
    let expected = crate::overlay::hash(
        &crate::overlay::compose(&overlay.content, &config.image),
        &crate::image::assets_hash(),
    );

    println!("  {}/Dockerfile -> '{derived}'", overlay.dir.display());

    if !crate::container::image_exists(&derived)? {
        println!("  not built yet. Run `csb build` (or just `csb`) to build it.");
        return Ok(());
    }

    let output = crate::container::run_output(&["image", "inspect", &derived])?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    match crate::overlay::staleness(&stdout, &expected) {
        Some(reason) => println!("  {reason} It rebuilds on the next run."),
        None => println!("  built and up to date."),
    }

    Ok(())
}

/// Anything in a shell rc that defines `csb` shadows this binary entirely — a
/// shell function or alias always wins over a PATH lookup.
fn check_shell_shadow() {
    let Ok(home) = std::env::var("HOME") else {
        return;
    };

    for rc in [format!("{home}/.zshrc"), format!("{home}/.bashrc")] {
        if let Ok(content) = std::fs::read_to_string(&rc) {
            if let Some(what) = shadow_reason(&content) {
                eprintln!(
                    "Warning: {what} in {rc} shadows this binary — a shell function or alias \
                     always wins over the PATH. Remove it to use the csb CLI."
                );
            }
        }
    }
}

/// Users of the pre-1.0 shell version either sourced `csb.sh` or pasted the
/// function straight into their rc, so matching the filename alone misses most
/// of them.
fn shadow_reason(content: &str) -> Option<&'static str> {
    let mut reason = None;

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        if line.contains("csb.sh") {
            return Some("'source .../csb.sh'");
        }
        if is_function_def(line) {
            reason = Some("a 'csb()' shell function");
        } else if line.starts_with("alias csb=") {
            reason = reason.or(Some("an 'alias csb='"));
        }
    }

    reason
}

/// Matches `csb()`, `csb ()`, and `function csb`.
fn is_function_def(line: &str) -> bool {
    let line = line.strip_prefix("function ").unwrap_or(line).trim_start();
    let Some(rest) = line.strip_prefix("csb") else {
        return false;
    };
    let rest = rest.trim_start();
    rest.starts_with("()") || rest.starts_with('{')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shadow_should_detect_a_sourced_script() {
        assert!(shadow_reason("source \"/x/csb/csb.sh\"\n").is_some());
    }

    #[test]
    fn shadow_should_detect_an_inlined_function() {
        // The pre-1.0 README told people to paste this straight into their rc.
        let rc = "export PATH=/x:$PATH\ncsb() {\n  container run --rm x\n}\n";
        assert_eq!(shadow_reason(rc), Some("a 'csb()' shell function"));
    }

    #[test]
    fn shadow_should_detect_other_function_spellings() {
        assert!(shadow_reason("csb () {\n}\n").is_some());
        assert!(shadow_reason("function csb {\n}\n").is_some());
    }

    #[test]
    fn shadow_should_detect_an_alias() {
        assert_eq!(
            shadow_reason("alias csb='container run x'\n"),
            Some("an 'alias csb='")
        );
    }

    #[test]
    fn shadow_should_ignore_comments_and_unrelated_names() {
        assert_eq!(shadow_reason("# csb() { } -- old version\n"), None);
        assert_eq!(shadow_reason("csb-doctor() {\n}\n"), None);
        assert_eq!(shadow_reason("csbx() {\n}\n"), None);
        assert_eq!(shadow_reason("echo csb\n"), None);
        assert_eq!(shadow_reason(""), None);
    }
}
