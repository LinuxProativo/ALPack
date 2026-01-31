use crate::command::Command;
use crate::mirror::Mirror;
use crate::settings::Settings;
use crate::utils::{_parse_key_value, finish_msg_setup};
use crate::{parse_key_value, utils};

use flate2::read::GzDecoder;
use indicatif::{ProgressBar, ProgressStyle};
use regex::Regex;
use scraper::{Html, Selector};
use std::collections::VecDeque;
use std::error::Error;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::{fs, io};
use tar::Archive;

pub const DEF_PACKAGES: &str =
    "alpine-sdk autoconf automake cmake glib-dev glib-static libtool go xz";

pub struct Setup {
    name: String,
    remaining_args: Vec<String>,
    def_rootfs: Option<String>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct VersionKey {
    major: u32,
    minor: u32,
    patch: u32,
    suffix: String,
}

impl Setup {
    pub fn new(name: String, remaining_args: Vec<String>) -> Self {
        Setup {
            name,
            remaining_args,
            def_rootfs: None,
        }
    }

    pub fn run(&mut self) -> Result<(), Box<dyn Error>> {
        let mut args: VecDeque<_> = self.remaining_args.clone().into();
        let mut use_mirror: Option<String> = None;
        let (mut no_cache, mut reinstall, mut edge, mut minimal) = (false, false, false, false);

        let sett = Settings::load_or_create();
        let (mut cache_dir, mut rootfs_dir) = (sett.set_cache_dir(), sett.set_rootfs());
        self.def_rootfs = Some(sett.set_rootfs());

        while let Some(arg) = args.pop_front() {
            match arg.as_str() {
                "--no-cache" => {
                    no_cache = true;
                },
                "-r" | "--reinstall" => {
                    reinstall = true;
                },
                "--edge" => {
                    edge = true;
                },
                "--minimal" => {
                    minimal = true;
                },
                a if a.starts_with("--mirror=") => {
                    use_mirror = parse_key_value!("setup", "url", arg)?;
                }
                "--mirror" => {
                    use_mirror = parse_key_value!("setup", "url", arg, args.pop_front().unwrap_or_default())?;
                }
                a if a.starts_with("--cache=") => {
                    cache_dir = parse_key_value!("setup", "directory", arg)?.unwrap_or_default();
                }
                "--cache" => {
                    cache_dir = parse_key_value!("setup", "directory", arg, args.pop_front().unwrap_or_default())?.unwrap();
                }
                a if a.starts_with("--rootfs=") => {
                    rootfs_dir = parse_key_value!("setup", "directory", arg)?.unwrap_or_default();
                }
                "-R" | "--rootfs" => {
                    rootfs_dir = parse_key_value!("setup", "directory", arg, args.pop_front().unwrap_or_default())?.unwrap();
                }
                _ => {
                    return Err(format!("{c}: setup: invalid argument '{arg}'\nUse '{c} --help' to see available options.", c = self.name).into())
                }
            }
        }

        if !reinstall {
            self.test_valid_directory(&rootfs_dir)?;
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

        let document = Html::parse_document(res.as_str());
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
        let mut dest_rootfs = rootfs_dir.clone();

        if let Some((_, version, link)) = matches.last() {
            println!("Latest version found: {version}");
            println!("Link: {url}{link}");
            let dest_dir =
                utils::download_file(format!("{url}{link}"), cache_dir.clone(), link.to_string())?;
            dest_rootfs = self.extract_tar_gz(format!("{dest_dir}/{link}"), rootfs_dir)?;

            if no_cache {
                let path = Path::new(cache_dir.as_str());
                fs::remove_dir_all(path)?;
            }
        } else {
            Err("No alpine-minirootfs files found")?;
        }

        let new_content = mirror.get_repository();
        let repo_path = Path::new(dest_rootfs.as_str()).join("etc/apk/repositories");
        let mut file = File::create(&repo_path)?;
        file.write_all(new_content.as_bytes())?;

        Command::run(
            dest_rootfs.clone(),
            None,
            Some("apk update".to_string()),
            true,
            true,
            false,
        )?;

        if !minimal {
            Command::run(
                dest_rootfs,
                None,
                Some(format!("apk add {DEF_PACKAGES}")),
                true,
                true,
                false,
            )?;
        }

        finish_msg_setup(self.name.clone());
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
    ///
    /// # Examples
    /// ```
    /// let result = extract_tar_gz(String::from("archive.tar.gz"), String::from("/tmp/output"));
    /// assert!(result.is_ok());
    /// ```
    fn extract_tar_gz(&self, file_path: String, destination: String) -> io::Result<String> {
        let dest_ok = utils::create_dir_with_fallback(destination);
        let save_dest = dest_ok?.to_str().unwrap().to_string();
        let mut decoder = GzDecoder::new(File::open(file_path)?);

        let mut temp = Vec::new();
        decoder.read_to_end(&mut temp)?;

        let bar = ProgressBar::new(temp.len() as u64);
        bar.set_message("Extracting...");
        bar.set_style(
            ProgressStyle::with_template(utils::DOWNLOAD_TEMPLATE)
                .unwrap()
                .progress_chars("##-"),
        );

        let reader = bar.wrap_read(io::Cursor::new(temp));
        let mut archive = Archive::new(reader);
        archive.unpack(Path::new(save_dest.as_str()))?;

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
    ///
    /// # Examples
    /// ```
    /// let version = parse_version_key("3.23.0_alpha20250612");
    /// assert!(version.is_some());
    /// ```
    fn parse_version_key(&self, link_contain_version: &str) -> Option<VersionKey> {
        let re = Regex::new(r"^(\d+)\.(\d+)\.(\d+)(?:[_\-]?([a-zA-Z0-9]+))?$").ok()?;
        let caps = re.captures(link_contain_version)?;
        Some(VersionKey {
            major: caps[1].parse().ok()?,
            minor: caps[2].parse().ok()?,
            patch: caps[3].parse().ok()?,
            suffix: caps.get(4).map_or("", |m| m.as_str()).to_string(),
        })
    }

    /// Checks whether the given target directory exists and is valid.
    ///
    /// # Parameters
    /// - `target`: Path to the directory to be checked.
    ///
    /// # Returns
    /// - `Ok(())` if the directory exists.
    /// - `Err` with an error message if the directory does not exist or is not accessible.
    ///
    /// # Examples
    /// ```
    /// let result = test_valid_directory("/path/to/check");
    /// assert!(result.is_ok());
    /// ```
    fn test_valid_directory(&self, target: &str) -> Result<(), Box<dyn Error>> {
        let target_path = Path::new(target);

        if target_path.exists() && target_path.is_dir() {
            return Err(format!("Rootfs directory {target} is already available.\nUse [-r|--reinstall] to reinstall it.").into());
        }

        if let Some(parent) = target_path.parent() {
            if parent.exists() && fs::metadata(parent).unwrap().permissions().readonly() == false {
                let test_path = parent.join(".permission_test");
                match File::create(&test_path) {
                    Ok(_) => {
                        fs::remove_file(&test_path)?;
                        return Ok(());
                    }
                    Err(_) => {
                        eprintln!(
                            "\x1b[1;33mWarning\x1b[0m: Write access denied for '{}'. Falling back to the default location...",
                            target
                        );
                    }
                }
            }
        }

        let home = self.def_rootfs.clone().unwrap();
        let fallback_path = Path::new(&home);
        if fallback_path.exists() && fallback_path.is_dir() {
            return Err(format!("Rootfs directory {target} is already available.\nUse [-r|--reinstall] to reinstall it.").into());
        }
        Ok(())
    }
}
