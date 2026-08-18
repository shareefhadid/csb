use anyhow::Result;

pub(crate) fn execute(force: bool) -> Result<()> {
    let config = crate::config::Config::from_env();

    crate::container::ensure_available()?;
    crate::container::system_start();

    if !force && crate::container::image_exists(&config.image)? {
        match crate::image::check_staleness(&config.image)? {
            None => {
                println!(
                    "Image '{}' is already up to date.\n\
                     Use `csb build --force` to rebuild it (that's how you update Claude Code).",
                    config.image
                );
                return Ok(());
            }
            Some(reason) => println!("{reason}\nRebuilding '{}'...", config.image),
        }
    }

    crate::image::build_image(&config.image)?;
    println!("Image '{}' built successfully.", config.image);

    Ok(())
}
