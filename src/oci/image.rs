use crate::oci::schema::{Schema, IMAGE_LAYOUT_SCHEMA_URI};

use std::fs;
use std::path::Path;

use anyhow::Result;
use serde_json::{json, Value};

pub(crate) struct Image {
    schema: Schema,
}

fn write_hash(directory: &Path, hash: &Value, filename: Option<String>) -> Result<(String, usize)> {
    let json = serde_json::to_string_pretty(&hash)?;
    let json_sha256 = sha256::digest(json.clone());
    let filename = filename.unwrap_or(sha256digest.clone());
    let path = directory.join(filename);
    fs::write(path, json.clone())?;

    Ok((json_sha256, json.len()))
}

impl Image {
    pub(crate) fn new() -> Result<Self> {
        let mut schema = Schema::new();
        schema.load_schemas()?;
        Ok(Self { schema })
    }

    pub(crate) fn write_image_layout(&self, root: &Path) -> Result<(String, i64)> {
        let image_layout = json!({ "imageLayoutVersion": "1.0.0" });
        self.schema
            .validate_schema(IMAGE_LAYOUT_SCHEMA_URI, &image_layout)?;
        let (config_json_sha256, config_json_size) =
            write_hash(root, &image_layout, Some("oci-layout".to_string()))?;
        Ok((config_json_sha256, config_json_size as i64))
    }
}
