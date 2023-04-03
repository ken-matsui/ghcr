use std::env;
use std::fs::File;
use std::path::Path;

use anyhow::Result;
use debug_print::debug_println as dprintln;
use flate2::write::GzEncoder;
use flate2::Compression;
use ghcr::Ghcr;

fn main() -> Result<()> {
    dprintln!("Initializing Ghcr object ...");
    let ghcr = Ghcr::new("ken-matsui".to_string(), "ghcr".to_string())?;

    let target_file = Path::new("./src");
    let tar_gz_file = env::temp_dir().join("archive.tar.gz");
    let tar_gz = File::create(tar_gz_file.as_path())?;
    let enc = GzEncoder::new(tar_gz, Compression::default());
    let mut tar = tar::Builder::new(enc);

    dprintln!("Compressing {target_file:?} into {tar_gz_file:?} ...");
    tar.append_dir_all(Path::new("test-org/test"), target_file)?;
    tar.finish()?;
    dprintln!("Finished: {tar_gz_file:?}");

    ghcr.upload_oci_image(tar_gz_file.as_path(), "test-org/test", "0.1.0")
}
