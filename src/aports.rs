//! Alpine Package Ports (aports) management module.
//!
//! This module provides the `Aports` struct and logic to interact with the
//! Alpine Linux aports repository, allowing for database updates,
//! package searching, and source file retrieval via sparse-checkout.

use crate::settings::{settings_output_dir, settings_rootfs_dir};
use crate::utils;
use crate::utils::collect_args;
use sandbox_utils::{app_name, invalid_arg, missing_arg, parse_value};
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
        let (mut s_pkg, mut get_pkg) = (Vec::new(), Vec::new());
        let (mut update, mut search, mut get, mut generic) = (false, false, false, false);
        let mut bk = false;

        while let Some(arg) = args.pop_front() {
            match arg {
                "-u" | "--update" => (update, bk) = (true, true),
                a if a.starts_with("--output=") => {
                    output_dir = parse_value!("aports", "directory", arg)?.into();
                }
                "-o" | "--output" => {
                    output_dir = parse_value!("aports", "directory", arg, args.pop_front())?.into();
                }
                a if a.starts_with("--search=") => {
                    (search, bk) = (true, true);
                    s_pkg.push(parse_value!("aports", "package", arg)?);
                    collect_args(&mut args, &mut s_pkg);
                }
                "-s" | "--search" => {
                    (search, bk, generic) = (true, true, true);
                    s_pkg.push(parse_value!("aports", "package", arg, args.pop_front())?);
                    collect_args(&mut args, &mut s_pkg);
                }
                "-S" | "--strict-search" => {
                    (search, bk) = (true, true);
                    s_pkg.push(parse_value!("aports", "package", arg, args.pop_front())?);
                    collect_args(&mut args, &mut s_pkg);
                }
                a if a.starts_with("--get=") => {
                    (get, bk) = (true, true);
                    get_pkg.push(parse_value!("aports", "package", arg)?);
                    collect_args(&mut args, &mut get_pkg);
                }
                "-g" | "--get" => {
                    (get, bk) = (true, true);
                    get_pkg.push(parse_value!("aports", "package", arg, args.pop_front())?);
                    collect_args(&mut args, &mut get_pkg);
                }
                a if a.starts_with("--rootfs=") => {
                    rootfs_dir = parse_value!("aports", "directory", arg)?.into();
                }
                "-R" | "--rootfs" => {
                    rootfs_dir = parse_value!("aports", "directory", arg, args.pop_front())?.into();
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

        let db_path = rootfs_dir.join("build/aports-database");

        if !db_path.exists() {
            return Err(format!(
                "The aports database was not found at: {}\nPlease run '{} aports -u' first to initialize the repository.",
                db_path.display(), app_name()
            ).into());
        }

        let content = fs::read_to_string(&db_path)?;

        if search {
            utils::print_result(&s_pkg, &content, generic)?;
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
