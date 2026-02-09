//! Environment setup orchestration.
//!
//! This module handles the initial preparation of the Alpine Linux environment,
//! including mirror selection, version discovery, rootfs extraction, and
//! provisioning of default packages.

use crate::command::Command;
use crate::mirror::Mirror;
use crate::settings::Settings;
use crate::utils::finish_msg_setup;
use crate::{concat_path, invalid_arg, parse_key_value, utils};

use flate2::read::GzDecoder;
use indicatif::{ProgressBar, ProgressStyle};
use regex::Regex;
use scraper::{Html, Selector};
use std::collections::VecDeque;
use std::error::Error;
use std::fs::File;
use std::io::{BufReader, Write};
use std::{fs, io};
use tar::Archive;

/// Default packages installed when minimal mode is disabled.
pub const DEF_PACKAGES: &str =
    "alpine-sdk autoconf automake cmake glib-dev glib-static libtool go xz";

/// Controller for setting up the Alpine Linux rootfs environment.
pub struct Setup {
    /// Command line arguments not consumed by the main parser.
    remaining_args: Vec<String>,
}

/// Structured version components for semantic comparison.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct VersionKey {
    major: u32,
    minor: u32,
    patch: u32,
    suffix: String,
}

impl Setup {
    /// Creates a new `Setup` instance.
    pub fn new(remaining_args: Vec<String>) -> Self {
        Setup { remaining_args }
    }

    /// Orchestrates the setup process including version discovery and installation.
    ///
    /// This method parses setup-specific flags, identifies the latest available
    /// minirootfs on the selected mirror, and executes the extraction and
    /// initial package setup via `apk`.
    ///
    /// # Returns
    /// - `Ok(())` on successful environment initialization.
    /// - `Err` if any stage (download, extraction, or execution) fails.
    pub fn run(&mut self) -> Result<(), Box<dyn Error>> {
        let mut args: VecDeque<&str> = self.remaining_args.iter().map(|s| s.as_str()).collect();
        let mut use_mirror: Option<String> = None;
        let (mut no_cache, mut reinstall, mut edge, mut minimal) = (false, false, false, false);

        let sett = Settings::load();
        let (mut cache_dir, mut rootfs_dir) = (sett.set_cache_dir(), sett.set_rootfs());
        let def_rootfs = rootfs_dir.clone();

        while let Some(arg) = args.pop_front() {
            match arg {
                "--edge" => edge = true,
                "--no-cache" => no_cache = true,
                "--minimal" => minimal = true,
                "-r" | "--reinstall" => reinstall = true,
                a if a.starts_with("--mirror=") => {
                    use_mirror = Some(parse_key_value!("setup", "url", arg)?);
                }
                "--mirror" => {
                    use_mirror = Some(parse_key_value!("setup", "url", arg, args.pop_front())?);
                }
                a if a.starts_with("--cache=") => {
                    cache_dir = parse_key_value!("setup", "directory", arg)?;
                }
                "--cache" => {
                    cache_dir = parse_key_value!("setup", "directory", arg, args.pop_front())?;
                }
                a if a.starts_with("--rootfs=") => {
                    rootfs_dir = parse_key_value!("setup", "directory", arg)?;
                }
                "-R" | "--rootfs" => {
                    rootfs_dir = parse_key_value!("setup", "directory", arg, args.pop_front())?;
                }
                _ => return invalid_arg!("setup", arg),
            }
        }

        if !reinstall {
            self.test_valid_directory(&rootfs_dir, &def_rootfs)?;
        }

        if no_cache {
            cache_dir = String::from("/tmp/ALPack_cache");
        }

        let mut mirror = Mirror::new(use_mirror, edge.then_some("edge".to_string()));
        mirror.run()?;

        let url = mirror.get_mirror();
        let res = ureq::get(url.as_str())
            .call()?
            .body_mut()
            .read_to_string()?;

        let document = Html::parse_document(&res);
        let selector = Selector::parse("a").unwrap();

        let pattern = format!(
            r"^alpine-minirootfs-([\w.\-]+)-{}\.tar\.gz$",
            utils::get_arch()
        );
        let re = Regex::new(&pattern).unwrap();

        let mut matches = vec![];
        for element in document.select(&selector) {
            if let Some(href) = element.value().attr("href") {
                if let Some(caps) = re.captures(href) {
                    let version_str = &caps[1];
                    if let Some(key) = self.parse_version_key(version_str) {
                        matches.push((key, version_str.to_string(), href.to_string()));
                    }
                }
            }
        }

        matches.sort_by(|a, b| a.0.cmp(&b.0));

        if let Some((_, version, link)) = matches.last() {
            println!("Latest version found: {version}");
            println!("Link: {url}{link}");
            let dest_dir = utils::download_file(&format!("{url}{link}"), &cache_dir, link)?;
            let dest_rootfs = self.extract_tar_gz(&format!("{dest_dir}/{link}"), &rootfs_dir)?;

            if no_cache {
                let _ = fs::remove_dir_all(&cache_dir);
            }

            let new_content = mirror.get_repository();
            let repo_path = concat_path!(&dest_rootfs, "etc/apk/repositories");
            let mut file = File::create(&repo_path)?;
            file.write_all(new_content.as_bytes())?;

            let apk_command = if minimal {
                "apk update".to_string()
            } else {
                format!("apk update && apk add {DEF_PACKAGES}")
            };

            Command::run(&dest_rootfs, None, Some(apk_command), true, true, false)?;
        } else {
            Err("No alpine-minirootfs files found")?;
        }

        finish_msg_setup();
        Ok(())
    }

    /// Extracts a `.tar.gz` archive to the specified destination directory.
    ///
    /// # Arguments
    /// * `file_path` - The path to the `.tar.gz` file to extract.
    /// * `destination` - The directory where the contents will be extracted.
    ///
    /// # Returns
    /// * `Ok(String)` containing the destination path on success.
    /// * `Err`: An `io::Error` if extraction fails.
    fn extract_tar_gz(&self, file_path: &str, destination: &str) -> io::Result<String> {
        let save_dest = utils::create_dir_with_fallback(destination)?;

        let file = File::open(file_path)?;
        let total_size = file.metadata()?.len();

        let bar = ProgressBar::new(total_size);
        bar.set_message("Extracting...");
        bar.set_style(
            ProgressStyle::with_template(utils::DOWNLOAD_TEMPLATE)
                .unwrap()
                .progress_chars("##-"),
        );

        let reader = bar.wrap_read(BufReader::with_capacity(64 * 1024, file));
        let decoder = GzDecoder::new(reader);
        let mut archive = Archive::new(decoder);

        archive.unpack(&save_dest)?;

        bar.finish_with_message("Extracted! ");
        Ok(save_dest)
    }

    /// Parses a version string into a `VersionKey` struct.
    ///
    /// # Arguments
    /// * `link_contain_version` - A string slice containing the version string to parse.
    ///
    /// # Returns
    /// * `Some(VersionKey)` if the string is successfully parsed.
    /// * `None` if the string does not match the expected version pattern.
    fn parse_version_key(&self, link_contain_version: &str) -> Option<VersionKey> {
        let re = Regex::new(r"^(\d+)\.(\d+)\.(\d+)(?:[_\-]?([a-zA-Z0-9]+))?$").ok()?;
        let caps = re.captures(link_contain_version)?;

        Some(VersionKey {
            major: caps.get(1)?.as_str().parse().ok()?,
            minor: caps.get(2)?.as_str().parse().ok()?,
            patch: caps.get(3)?.as_str().parse().ok()?,
            suffix: caps
                .get(4)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default(),
        })
    }

    /// Validates if the target directory can be used for rootfs installation.
    ///
    /// # Parameters
    /// - `target`: The requested installation path.
    /// - `def_rootfs`: The fallback default rootfs path.
    ///
    /// # Returns
    /// - `Ok(())` if the directory exists.
    /// - `Err` with an error message if the directory does not exist or is not accessible.
    fn test_valid_directory(&self, target: &str, def_rootfs: &str) -> Result<(), Box<dyn Error>> {
        if fs::metadata(target).map(|m| m.is_dir()).unwrap_or(false) {
            return Err(format!(
                "Rootfs directory {target} is already available.\nUse [-r|--reinstall] to reinstall it.",
            )
            .into());
        }

        let mut can_write = false;
        if let Some(pos) = target.rfind('/') {
            let parent = if pos == 0 { "/" } else { &target[..pos] };
            if let Ok(meta) = fs::metadata(parent) {
                if !meta.permissions().readonly() {
                    can_write = true;
                }
            }
        }

        if !can_write {
            if !target.is_empty() {
                eprintln!(
                    "\x1b[1;33mWarning\x1b[0m: Write access denied for '{target}'. Falling back to default...",
                );
            }

            if fs::metadata(def_rootfs)
                .map(|m| m.is_dir())
                .unwrap_or(false)
            {
                return Err(format!(
                    "Rootfs directory {def_rootfs} is already available.\nUse [-r|--reinstall] to reinstall it.",
                ).into());
            }
        }

        Ok(())
    }
}
