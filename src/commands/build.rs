use anyhow::Result;

pub(crate) fn execute(force: bool) -> Result<()> {
    let config = crate::config::Config::from_env();

    crate::container::ensure_available()?;
    crate::container::system_start();

    if config.image_is_custom
        && crate::container::image_exists(&config.image)?
        && !crate::image::is_csb_built(&config.image)?
    {
        anyhow::bail!(
            "'{}' (from CSB_IMAGE) was not built by csb — refusing to overwrite it. \
             Unset CSB_IMAGE, or pick a name csb owns.",
            config.image
        );
    }

    if !force && crate::container::image_exists(&config.image)? {
        match crate::image::check_staleness(&config.image, !config.image_is_custom)? {
            None => {
                println!(
                    "Image '{}' is already up to date.\n\
                     Use `csb build --force` to rebuild it (that's how you update Claude Code).",
                    config.image
                );
                return build_overlay(&config.image);
            }
            Some(reason) => println!("{reason}\nRebuilding '{}'...", config.image),
        }
    }

    crate::image::build_image(&config.image)?;
    println!("Image '{}' built successfully.", config.image);

    build_overlay(&config.image)
}

/// Rebuild the user's `~/.config/csb/Dockerfile` layer, if they have one — a new
/// base means the layer on top of it is stale by definition.
fn build_overlay(base: &str) -> Result<()> {
    let Some(overlay) = crate::overlay::discover()? else {
        return Ok(());
    };

    let derived = crate::overlay::derived_image_name(base);
    println!("Building your overlay layer as '{derived}'...");
    crate::overlay::build(&overlay, base, &derived, &crate::image::assets_hash())?;
    println!("Overlay image '{derived}' built successfully.");

    Ok(())
}
