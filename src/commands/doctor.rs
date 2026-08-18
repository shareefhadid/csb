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

/// Users of the pre-1.0 shell version have `source .../csb.sh` in their rc, which
/// defines a `csb` function that shadows this binary entirely.
fn check_shell_shadow() {
    let Ok(home) = std::env::var("HOME") else {
        return;
    };

    for rc in [format!("{home}/.zshrc"), format!("{home}/.bashrc")] {
        if let Ok(content) = std::fs::read_to_string(&rc) {
            if content.contains("csb.sh") {
                eprintln!(
                    "Warning: 'source .../csb.sh' found in {rc} — the shell function shadows \
                     this binary. Remove that line to use the csb CLI."
                );
            }
        }
    }
}
