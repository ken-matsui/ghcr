use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::Result;
use serde_json::{json, Value};

use crate::oci::schema::{
    Schema, IMAGE_CONFIG_SCHEMA_URI, IMAGE_INDEX_SCHEMA_URI, IMAGE_LAYOUT_SCHEMA_URI,
    IMAGE_MANIFEST_SCHEMA_URI,
};

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

    pub(crate) fn write_image_layout(&self, root: &Path) -> Result<()> {
        let image_layout = json!({ "imageLayoutVersion": "1.0.0" });
        self.schema
            .validate_schema(IMAGE_LAYOUT_SCHEMA_URI, &image_layout)?;
        Self::write_hash(root, &image_layout, Some("oci-layout".to_string()))?;
        Ok(())
    }

    /// # Args
    /// * upload_file: must be tar.gz'ed upload-target directory
    /// * blobs: ./blobs/sha256
    ///          https://github.com/opencontainers/image-spec/blob/main/image-layout.md#blobs
    ///
    /// # Description
    /// 1. Get sha256 digest of compressed upload-target directory
    /// 2. Copy the compressed upload-target directory to (as) ./blobs/sha256/{digest we got in (1)}
    pub(crate) fn write_tar_gz(upload_file: &Path, blobs: &Path) -> Result<String> {
        let tar_gz_sha256 = sha256::try_digest(upload_file)?;
        fs::copy(upload_file, blobs.join(tar_gz_sha256.clone()))?;
        Ok(tar_gz_sha256)
    }

    pub(crate) fn write_image_config(
        &self,
        arch: &str,
        os: &str,
        tar_sha256: &str,
        blobs: &Path,
    ) -> Result<(String, usize)> {
        let image_config = json!({
            "architecture": arch,
            "os": os,
            "rootfs": {
                "type": "layers",
                "diff_ids": [
                    format!("sha256:{tar_sha256}")
                ]
            }
        });
        self.schema
            .validate_schema(IMAGE_CONFIG_SCHEMA_URI, &image_config)?;
        Self::write_hash(blobs, &image_config, None)
    }

    pub(crate) fn write_image_manifest(
        &self,
        image_manifest: &Value,
        blobs: &Path,
    ) -> Result<(String, usize)> {
        self.schema
            .validate_schema(IMAGE_MANIFEST_SCHEMA_URI, image_manifest)?;
        Self::write_hash(blobs, image_manifest, None)
    }

    pub(crate) fn write_image_index(
        &self,
        manifests: &Vec<Value>,
        annotations: &HashMap<String, String>,
        blobs: &Path,
    ) -> Result<(String, usize)> {
        let image_index = json!({
            "schemaVersion": 2,
            "manifests": manifests,
            "annotations": annotations,
        });
        self.schema
            .validate_schema(IMAGE_INDEX_SCHEMA_URI, &image_index)?;
        Self::write_hash(blobs, &image_index, None)
    }

    pub(crate) fn write_index_json(
        &self,
        index_json_sha256: &str,
        index_json_size: usize,
        root: &Path,
        annotations: &HashMap<String, String>,
    ) -> Result<()> {
        let index_json = json!({
            "schemaVersion": 2,
            "manifests": [{
                "mediaType": "application/vnd.oci.image.index.v1+json",
                "digest": format!("sha256:{index_json_sha256}"),
                "size": index_json_size,
                "annotations": annotations,
            }],
        });
        self.schema
            .validate_schema(IMAGE_INDEX_SCHEMA_URI, &index_json)?;
        Self::write_hash(root, &index_json, Some("index.json".to_string()))?;
        Ok(())
    }
}
