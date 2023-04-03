use crate::oci::schema::{Schema, IMAGE_LAYOUT_SCHEMA_URI};

use std::fs;
use std::path::Path;

use anyhow::Result;
use serde_json::{json, Value};

pub(crate) struct Image {
    schema: Schema,
}

impl Image {
    pub(crate) fn new() -> Result<Self> {
        let mut schema = Schema::new();
        schema.load_schemas()?;
        Ok(Self { schema })
    }

    fn write_hash(
        directory: &Path,
        hash: &Value,
        filename: Option<String>,
    ) -> Result<(String, usize)> {
        let json = serde_json::to_string_pretty(&hash)?;
        let json_sha256 = sha256::digest(json.clone());
        let filename = filename.unwrap_or(json_sha256.clone());
        let path = directory.join(filename);
        fs::write(path, json.clone())?;

        Ok((json_sha256, json.len()))
    }

    pub(crate) fn write_image_layout(&self, root: &Path) -> Result<(String, i64)> {
        let image_layout = json!({ "imageLayoutVersion": "1.0.0" });
        self.schema
            .validate_schema(IMAGE_LAYOUT_SCHEMA_URI, &image_layout)?;
        let (config_json_sha256, config_json_size) =
            Image::write_hash(root, &image_layout, Some("oci-layout".to_string()))?;
        Ok((config_json_sha256, config_json_size as i64))
    }

    /// upload_file: must be tar.gz'ed upload-target directory
    /// blobs: ./blobs/sha256
    ///        https://github.com/opencontainers/image-spec/blob/main/image-layout.md#blobs
    pub(crate) fn write_tar_gz(upload_file: &Path, blobs: &Path) -> Result<String> {
        let tar_gz_sha256 = sha256::try_digest(upload_file)?;
        fs::copy(upload_file, blobs.join(tar_gz_sha256.clone()))?;
        Ok(tar_gz_sha256)
    }
}
