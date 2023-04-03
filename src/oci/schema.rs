use std::collections::HashMap;

use anyhow::{bail, Context, Result};
use serde_json::Value;
use valico::json_schema;

pub const IMAGE_CONFIG_SCHEMA_URI: &str = "https://opencontainers.org/schema/image/config";
pub const IMAGE_INDEX_SCHEMA_URI: &str = "https://opencontainers.org/schema/image/index";
pub const IMAGE_LAYOUT_SCHEMA_URI: &str = "https://opencontainers.org/schema/image/layout";
pub const IMAGE_MANIFEST_SCHEMA_URI: &str = "https://opencontainers.org/schema/image/manifest";

pub(crate) struct Schema {
    pub(crate) schema_json: HashMap<&'static str, Value>,
}

impl Schema {
    pub(crate) fn new() -> Self {
        Self {
            schema_json: HashMap::new(),
        }
    }

    fn schema_uri(&mut self, basename: &str, uris: Vec<&'static str>) -> Result<()> {
        // The current `main` version has an invalid JSON schema.
        // Going forward, this should probably be pinned to tags.
        // We currently use features newer than the last one (v1.0.2).
        let url = format!("https://raw.githubusercontent.com/opencontainers/image-spec/170393e57ed656f7f81c3070bfa8c3346eaa0a5a/schema/{basename}.json");
        let json = reqwest::blocking::get(url)?.json::<Value>()?;

        for uri in uris {
            self.schema_json.insert(uri, json.clone());
        }
        Ok(())
    }

    pub(crate) fn load_schemas(&mut self) -> Result<()> {
        self.schema_uri(
            "content-descriptor",
            vec!["https://opencontainers.org/schema/image/content-descriptor.json"],
        )?;
        self.schema_uri(
            "defs",
            vec![
                "https://opencontainers.org/schema/defs.json",
                "https://opencontainers.org/schema/descriptor/defs.json",
                "https://opencontainers.org/schema/image/defs.json",
                "https://opencontainers.org/schema/image/descriptor/defs.json",
                "https://opencontainers.org/schema/image/index/defs.json",
                "https://opencontainers.org/schema/image/manifest/defs.json",
            ],
        )?;
        self.schema_uri(
            "defs-descriptor",
            vec![
                "https://opencontainers.org/schema/descriptor.json",
                "https://opencontainers.org/schema/defs-descriptor.json",
                "https://opencontainers.org/schema/descriptor/defs-descriptor.json",
                "https://opencontainers.org/schema/image/defs-descriptor.json",
                "https://opencontainers.org/schema/image/descriptor/defs-descriptor.json",
                "https://opencontainers.org/schema/image/index/defs-descriptor.json",
                "https://opencontainers.org/schema/image/manifest/defs-descriptor.json",
                "https://opencontainers.org/schema/index/defs-descriptor.json",
            ],
        )?;
        self.schema_uri("config-schema", vec![IMAGE_CONFIG_SCHEMA_URI])?;
        self.schema_uri("image-index-schema", vec![IMAGE_INDEX_SCHEMA_URI])?;
        self.schema_uri("image-layout-schema", vec![IMAGE_LAYOUT_SCHEMA_URI])?;
        self.schema_uri("image-manifest-schema", vec![IMAGE_MANIFEST_SCHEMA_URI])?;
        Ok(())
    }

    pub(crate) fn validate_schema(&self, schema_url: &str, json: &Value) -> Result<()> {
        let schema_json = self
            .schema_json
            .get(schema_url)
            .context("unknown schema url")?;
        let mut scope = json_schema::Scope::new();
        let schema = scope.compile_and_return(schema_json.clone(), false)?;
        let result = schema.validate(&json);
        if !result.is_valid() {
            for error in result.errors {
                eprintln!("Validation error: {}", error.get_title());
                eprintln!("Instance path: {}", error.get_path());
            }
            bail!("JSON schema validation failed");
        }
        Ok(())
    }
}
