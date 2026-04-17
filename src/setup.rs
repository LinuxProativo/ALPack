//! Environment setup orchestration.
//!
//! This module handles the initial preparation of the Alpine Linux environment,
//! including mirror selection, version discovery, rootfs extraction, and
//! provisioning of default packages.

use crate::mirror::Mirror;
use crate::settings::{settings_cache_dir, settings_rootfs_dir};
use crate::utils::map_result;
use regex::Regex;
use sandbox_utils::{
    app_arch, app_name, invalid_arg, parse_value, success_finish_setup, temp_cache, SandBox,
    SandBoxConfig,
};
use scraper::{Html, Selector};
use std::collections::VecDeque;
use std::error::Error;
use std::fs;

/// Structured version components for semantic comparison.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct VersionKey {
    major: u32,
    minor: u32,
    patch: u32,
    suffix: String,
}

/// Default packages installed when minimal mode is disabled.
pub const DEF_PACKAGES: &str =
    "alpine-sdk autoconf automake cmake glib-dev glib-static libtool go xz";

/// Controller for setting up the Alpine Linux rootfs environment.
pub struct Setup {
    /// Command line arguments not consumed by the main parser.
    remaining_args: Vec<String>,
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
        let (mut cache_dir, mut rootfs) = (settings_cache_dir(), settings_rootfs_dir());

        while let Some(arg) = args.pop_front() {
            match arg {
                "--edge" => edge = true,
                "--no-cache" => no_cache = true,
                "--minimal" => minimal = true,
                "-r" | "--reinstall" => reinstall = true,
                a if a.starts_with("--mirror=") => {
                    use_mirror = Some(parse_value!("setup", "url", arg)?);
                }
                "--mirror" => {
                    use_mirror = Some(parse_value!("setup", "url", arg, args.pop_front())?);
                }
                a if a.starts_with("--cache=") => {
                    cache_dir = parse_value!("setup", "directory", arg)?.into();
                }
                "--cache" => {
                    cache_dir = parse_value!("setup", "directory", arg, args.pop_front())?.into();
                }
                a if a.starts_with("--rootfs=") => {
                    rootfs = parse_value!("setup", "directory", arg)?.into();
                }
                "-R" | "--rootfs" => {
                    rootfs = parse_value!("setup", "directory", arg, args.pop_front())?.into();
                }
                _ => return invalid_arg!("setup", arg),
            }
        }

        if !reinstall && rootfs.exists() && rootfs.is_dir() {
            return Err(format!(
                "Rootfs directory '{}' is already available.\nUse [-r|--reinstall] to reinstall it.",
                rootfs.display()
            ).into());
        }

        if reinstall && rootfs.exists() {
            fs::remove_dir_all(&rootfs)?;
        }

        if no_cache {
            cache_dir = temp_cache();
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

        let pattern = format!(r"^alpine-minirootfs-([\w.\-]+)-{}\.tar\.gz$", app_arch());
        let re = Regex::new(&pattern).unwrap();

        let mut matches = vec![];
        for element in document.select(&selector) {
            if let Some(href) = element.value().attr("href") {
                if let Some(caps) = re.captures(href) {
                    let version_str = &caps[1];
                    if let Some(key) = Self::parse_version_key(version_str) {
                        matches.push((key, version_str.to_string(), href.to_string()));
                    }
                }
            }
        }

        matches.sort_by(|a, b| a.0.cmp(&b.0));

        if let Some((_, version, link)) = matches.last() {
            println!("Latest version found: {version}");
            println!("Link: {url}{link}");
            sandbox_utils::download_file(&format!("{url}{link}"), cache_dir.clone(), link)?;
            sandbox_utils::extract_bootstrap(cache_dir.join(link), rootfs.clone())?;

            if no_cache {
                let _ = fs::remove_dir_all(&cache_dir);
            }

            let repo_path = rootfs.join("etc/apk/repositories");
            fs::write(&repo_path, mirror.get_repository())?;

            let apk_command = if minimal {
                "apk update".to_string()
            } else {
                format!("apk update && apk add {DEF_PACKAGES}")
            };

            let config = SandBoxConfig {
                rootfs,
                run_cmd: apk_command,
                use_root: true,
                ignore_extra_bind: true,
                ..Default::default()
            };

            map_result(SandBox::run(config))?;
        } else {
            Err("No alpine-minirootfs files found")?;
        }

        success_finish_setup(format!("{} run", app_name()).as_str())
    }

    /// Parses a version string into a `VersionKey` struct.
    ///
    /// # Arguments
    /// * `link_contain_version` - A string slice containing the version string to parse.
    ///
    /// # Returns
    /// * `Some(VersionKey)` if the string is successfully parsed.
    /// * `None` if the string does not match the expected version pattern.
    pub fn parse_version_key(link_contain_version: &str) -> Option<VersionKey> {
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
}
