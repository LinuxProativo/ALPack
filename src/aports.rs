//! Alpine Package Ports (aports) management module.
//!
//! This module provides the `Aports` struct and logic to interact with the
//! Alpine Linux aports repository, allowing for database updates,
//! package searching, and source file retrieval via sparse-checkout.

use crate::settings::{settings_output_dir, settings_rootfs_dir};
use crate::{collect_args, invalid_arg, missing_arg, parse_key_value, utils};

use std::collections::VecDeque;
use std::error::Error;
use std::fs;

/// Controller for Alpine Linux repository operations.
pub struct Aports {
    /// Arguments passed from the CLI for processing.
    remaining_args: Vec<String>,
}

impl Aports {
    /// Creates a new `Aports` instance with the given context and arguments.
    pub fn new(remaining_args: Vec<String>) -> Self {
        Aports { remaining_args }
    }

    /// Executes the aports command logic based on the provided arguments.
    ///
    /// The flow includes parsing arguments, optionally updating the local
    /// repository index, and performing search or fetch operations.
    ///
    /// # Performance
    /// - Uses `VecDeque<&str>` to avoid heap allocations during argument parsing.
    /// - Implements lazy loading for the database content.
    ///
    /// # Returns
    /// - `Ok(())` on success.
    /// - `Err` if argument validation, repository setup, or file operations fail.
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        let mut args: VecDeque<&str> = self.remaining_args.iter().map(|s| s.as_str()).collect();

        if args.is_empty() {
            return missing_arg!("aports");
        }

        let mut rootfs_dir = settings_rootfs_dir();
        let mut output_dir = settings_output_dir();
        let (mut search_pkg, mut get_pkg) = (Vec::new(), Vec::new());
        let (mut update, mut search, mut get, mut bk) = (false, false, false, false);

        while let Some(arg) = args.pop_front() {
            match arg {
                "-u" | "--update" => (update, bk) = (true, true),
                a if a.starts_with("--output=") => {
                    output_dir = parse_key_value!("aports", "directory", arg)?.into();
                }
                "-o" | "--output" => {
                    output_dir =
                        parse_key_value!("aports", "directory", arg, args.pop_front())?.into();
                }
                a if a.starts_with("--search=") => {
                    (search, bk) = (true, true);
                    search_pkg.push(parse_key_value!("aports", "package", arg)?);
                    collect_args!(args, search_pkg);
                }
                "-s" | "--search" => {
                    (search, bk) = (true, true);
                    search_pkg.push(parse_key_value!(
                        "aports",
                        "package",
                        arg,
                        args.pop_front()
                    )?);
                    collect_args!(args, search_pkg);
                }
                a if a.starts_with("--get=") => {
                    (get, bk) = (true, true);
                    get_pkg.push(parse_key_value!("aports", "package", arg)?);
                    collect_args!(args, get_pkg);
                }
                "-g" | "--get" => {
                    (get, bk) = (true, true);
                    get_pkg.push(parse_key_value!(
                        "aports",
                        "package",
                        arg,
                        args.pop_front()
                    )?);
                    collect_args!(args, get_pkg);
                }
                a if a.starts_with("--rootfs=") => {
                    rootfs_dir = parse_key_value!("aports", "directory", arg)?.into();
                }
                "-R" | "--rootfs" => {
                    rootfs_dir =
                        parse_key_value!("aports", "directory", arg, args.pop_front())?.into();
                }
                other => return invalid_arg!("aports", other),
            }
        }

        if !bk {
            return missing_arg!("aports", essential);
        }

        if update {
            utils::update_git_repository(
                rootfs_dir.clone(),
                "https://github.com/alpinelinux/aports.git",
                "aports",
                &["main", "community", "testing"],
            )?;

            if !search && !get {
                return Ok(());
            }
        }

        utils::check_rootfs_exists(rootfs_dir.clone())?;
        let content: String =
            fs::read_to_string(rootfs_dir.join("build/aports-database")).unwrap_or_default();

        if search {
            utils::print_result(&search_pkg, &content)?;
            if !get {
                return Ok(());
            }
        }

        if get {
            utils::download_git_sources_files(
                rootfs_dir, "aports", &get_pkg, &content, output_dir,
            )?;
        }
        Ok(())
    }
}
