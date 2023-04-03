use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::Command;
use std::{env, fs, str};

use anyhow::{bail, Context as _, Result};
use const_format::formatc;
use data_encoding::HEXUPPER;
use debug_print::debug_println as dprintln;
use flate2::bufread::GzDecoder;
use ring::digest::{Context, Digest, SHA256};
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

    fn sha256_digest<R: BufRead>(mut reader: GzDecoder<R>) -> Result<Digest> {
        let mut context = Context::new(&SHA256);
        let mut buffer = [0; 1024];

        loop {
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            context.update(&buffer[..count]);
        }
        Ok(context.finish())
    }

    pub fn upload_oci_image(&self, target_file: &Path, name: &str, version: &str) -> Result<()> {
        let image_name = self.check_existence(name, version)?;

        let dir_name = format!("{}--{version}", image_name.replace("/", "-"));
        let root = Path::new(&dir_name);
        if root.exists() {
            fs::remove_dir_all(root)?;
        }
        fs::create_dir(root)?;

        let oci_image = oci::Image::new()?;
        oci_image.write_image_layout(root)?;

        let blobs_buf = root.join("blobs").join("sha256");
        let blobs = blobs_buf.as_path();
        fs::create_dir_all(blobs)?;

        let package_annotations = HashMap::<String, String>::from([
            (
                "com.github.package.type".to_string(),
                GITHUB_PACKAGE_TYPE.to_string(),
            ),
            // ("org.opencontainers.image.created".to_string(), created_date),
            // (
            //     "org.opencontainers.image.description".to_string(),
            //     description,
            // ),
            // (
            //     "org.opencontainers.image.documentation".to_string(),
            //     documentation,
            // ),
            // ("org.opencontainers.image.license".to_string(), license),
            (
                "org.opencontainers.image.ref.name".to_string(),
                version.to_string(),
            ),
            // (
            //     "org.opencontainers.image.revision".to_string(),
            //     git_revision,
            // ),
            // ("org.opencontainers.image.source".to_string(), source),
            ("org.opencontainers.image.title".to_string(), image_name),
            // ("org.opencontainers.image.url".to_string(), homepage),
            (
                "org.opencontainers.image.vendor".to_string(),
                self.org.clone(),
            ),
            (
                "org.opencontainers.image.version".to_string(),
                version.to_string(),
            ),
        ]);

        dprintln!("Uploading {target_file:?} ...");
        let tar_gz_sha256 = oci::Image::write_tar_gz(target_file, blobs)?;

        let arch = "amd64"; // package must be built at least on x86_64
        let os = "linux"; // package must be built at least on Linux

        // get decompressed sha256 digest
        let tar_gz = File::open(target_file)?;
        let tar_gz_size = tar_gz.metadata()?.len();
        let tar = GzDecoder::new(BufReader::new(tar_gz));
        let tar_sha256 = Self::sha256_digest(tar)?;

        dprintln!("Creating image config ...");
        let (config_json_sha256, config_json_size) =
            oci_image.write_image_config(arch, os, &HEXUPPER.encode(tar_sha256.as_ref()), blobs)?;
        dprintln!("Done: creating image config");

        let descriptor_annotations = HashMap::<String, String>::from([(
            "org.opencontainers.image.ref.name".to_string(),
            version.to_string(),
        )]);

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
        dprintln!("Creating image manifest ...");
        let (manifest_json_sha256, manifest_json_size) =
            oci_image.write_image_manifest(&image_manifest, blobs)?;
        dprintln!("Done: creating image manifest");

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
        dprintln!("Creating image index ...");
        let (index_json_sha256, index_json_size) =
            oci_image.write_image_index(&manifests, &package_annotations, blobs)?;
        dprintln!("Done: creating image index");

        dprintln!("Creating index json ...");
        oci_image.write_index_json(
            &index_json_sha256,
            index_json_size,
            root,
            &HashMap::from([(
                "org.opencontainers.image.ref.name".to_string(),
                version.to_string(),
            )]),
        )?;
        dprintln!("Done: creating index json");

        // TODO: --- upload_oci_image ---

        Ok(())
    }
}
