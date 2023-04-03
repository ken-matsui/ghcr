use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::process::Command;
use std::{env, fs, str};

use anyhow::{bail, Context, Result};
use const_format::formatc;
use debug_print::debug_println as dprintln;
use flate2::read::GzDecoder;
use serde_json::json;
use which::which;

use crate::oci;

const DOMAIN: &str = "ghcr.io";
const URL_PREFIX: &str = formatc!("https://{DOMAIN}/v2/");
const DOCKER_PREFIX: &str = formatc!("docker://{DOMAIN}/");

const SKOPEO_BINARY_NAME: &str = "skopeo";
const GITHUB_PACKAGE_TYPE: &str = "container";

pub struct Ghcr {
    user: String,
    token: String,

    org: String,
    repo: String,
}

impl Ghcr {
    pub fn new(org: String, repo: String) -> Result<Self> {
        let (user, token) = Ghcr::precondition()?;
        Ok(Self {
            user,
            token,
            org,
            repo,
        })
    }

    fn precondition() -> Result<(String, String)> {
        // These environmental variables must be defined.
        let user =
            env::var("GITHUB_PACKAGES_USER").context("GITHUB_PACKAGES_USER must be defined")?;
        let token =
            env::var("GITHUB_PACKAGES_TOKEN").context("GITHUB_PACKAGES_TOKEN must be defined")?;

        // skopeo must be installed to upload an OCI image.
        which(SKOPEO_BINARY_NAME).context("skopeo must be installed")?;

        Ok((user, token))
    }

    fn root_url(prefix: &str, org: &str, repo: &str) -> String {
        // docker/skopeo insist on lowercase org ("repository name")
        let org = org.to_lowercase();

        format!("{prefix}{org}/{repo}")
    }

    fn check_existence(&self, name: &str, version: &str) -> Result<String> {
        let image_name = name;
        let image_tag = version;
        let image_uri_prefix = Ghcr::root_url(DOCKER_PREFIX, &self.org, &self.repo);
        let image_uri = format!("{image_uri_prefix}/{image_name}:{image_tag}");

        let mut inspect_args = vec!["inspect".to_string(), "--raw".to_string(), image_uri];
        inspect_args.push(format!("--creds={}:{}", self.user, self.token));
        let inspect_result = Command::new(SKOPEO_BINARY_NAME)
            .args(inspect_args)
            .output()
            .expect("skopeo command failed");

        if inspect_result.status.success() {
            bail!("package already exists: {image_name}:{image_tag}");
        }
        Ok(image_name.to_string())
    }

    pub fn upload_oci_image(&self, target_file: &Path, name: &str, version: &str) -> Result<()> {
        let image_name = self.check_existence(name, version)?;

        let dir_name = format!("{image_name}--{version}");
        let root = Path::new(&dir_name);
        fs::remove_dir_all(root)?;
        fs::create_dir(root)?;

        let oci_image = oci::Image::new()?;
        oci_image.write_image_layout(root)?;

        let blobs_buf = root.join("blobs").join("sha256");
        let blobs = blobs_buf.as_path();
        fs::create_dir_all(blobs)?;

        let mut package_annotations = HashMap::<String, String>::new();
        package_annotations.insert(
            "com.github.package.type".to_string(),
            GITHUB_PACKAGE_TYPE.to_string(),
        );
        // package_annotations.insert("org.opencontainers.image.created".to_string(), created_date);
        // package_annotations.insert(
        //     "org.opencontainers.image.description".to_string(),
        //     description,
        // );
        // package_annotations.insert(
        //     "org.opencontainers.image.documentation".to_string(),
        //     documentation,
        // );
        // package_annotations.insert(
        //     "org.opencontainers.image.license".to_string(),
        //     license,
        // );
        package_annotations.insert(
            "org.opencontainers.image.ref.name".to_string(),
            version.to_string(),
        );
        // package_annotations.insert(
        //     "org.opencontainers.image.revision".to_string(),
        //     git_revision,
        // );
        // package_annotations.insert("org.opencontainers.image.source".to_string(), source);
        package_annotations.insert("org.opencontainers.image.title".to_string(), image_name);
        // package_annotations.insert(
        //     "org.opencontainers.image.url".to_string(),
        //     homepage,
        // );
        package_annotations.insert(
            "org.opencontainers.image.vendor".to_string(),
            self.org.clone(),
        );
        package_annotations.insert(
            "org.opencontainers.image.version".to_string(),
            version.to_string(),
        );

        dprintln!("Uploading {target_file:?}");
        let tar_gz_sha256 = oci::Image::write_tar_gz(target_file, blobs)?;

        let arch = "amd64"; // package must be built at least on x86_64
        let os = "linux"; // package must be built at least on Linux

        // get decompressed sha256 digest
        let tar_gz = File::open(target_file)?;
        let tar_gz_size = tar_gz.metadata()?.len();
        let mut tar = GzDecoder::new(tar_gz);
        let mut data = String::new();
        tar.read_to_string(&mut data)?;
        let tar_sha256 = sha256::digest(data);

        let (config_json_sha256, config_json_size) =
            oci_image.write_image_config(arch, os, &tar_sha256, blobs)?;

        let mut descriptor_annotations = HashMap::<String, String>::new();
        descriptor_annotations.insert(
            "org.opencontainers.image.ref.name".to_string(),
            version.to_string(),
        );

        let image_manifest = json!({
            "schemaVersion": 2,
            "config": {
                "mediaType": "application/vnd.oci.image.config.v1+json",
                "digest": format!("sha256:{config_json_sha256}"),
                "size": config_json_size,
            },
            "layers": [{
                "mediaType": "application/vnd.oci.image.layer.v1.tar+gzip",
                "digest": format!("sha256:{tar_gz_sha256}"),
                "size": tar_gz_size,
                "annotations": {
                    "org.opencontainers.image.title": target_file.to_str().unwrap(),
                },
            }],
            "annotations": package_annotations,
        });
        let (manifest_json_sha256, manifest_json_size) =
            oci_image.write_image_manifest(&image_manifest, blobs)?;

        let manifests = vec![json!({
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "digest": format!("sha256:{manifest_json_sha256}"),
            "size": manifest_json_size,
            "platform": {
                "architecture": arch,
                "os": os,
            },
            "annotations": descriptor_annotations,
        })];
        let (index_json_sha256, index_json_size) =
            oci_image.write_image_index(&manifests, &package_annotations, blobs)?;
        oci_image.write_index_json(
            &index_json_sha256,
            index_json_size,
            root,
            &HashMap::from([(
                "org.opencontainers.image.ref.name".to_string(),
                version.to_string(),
            )]),
        )?;

        // TODO: --- upload_oci_image ---

        Ok(())
    }
}
