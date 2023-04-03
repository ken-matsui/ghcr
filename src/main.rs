use anyhow::Result;
use ghcr::Ghcr;

fn main() -> Result<()> {
    let ghcr = Ghcr::new("poac-dev".to_string(), "poac".to_string());
    ghcr.upload_oci_image("ken-matsui/ghcr", "1.0.0")?;

    Ok(())
}
