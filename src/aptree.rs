//! Adélie Package Tree (aptree) management module.
//!
//! This module provides the `Aptree` struct and logic to interact with the
//! Adélie Linux package repository. It supports database synchronization,
//! package searching, and source retrieval via Git sparse-checkout,
//! specifically tailored for Adélie's repository structure.

use crate::settings::{settings_output_dir, settings_rootfs_dir};
use crate::utils;
use crate::utils::collect_args;
use sandbox_utils::{app_name, invalid_arg, missing_arg, parse_value};
use std::collections::VecDeque;
use std::error::Error;
use std::fs;

/// Controller for Adélie Linux repository operations.
pub struct Aptree {
    /// Arguments passed from the CLI for processing.
    remaining_args: Vec<String>,
}

impl Aptree {
    /// Creates a new `Aptree` instance with the given context and arguments.
    pub fn new(remaining_args: Vec<String>) -> Self {
        Aptree { remaining_args }
    }

    /// Executes the aptree command logic based on the provided arguments.
    ///
    /// Manages the full lifecycle of Adélie package interactions, including
    /// updating the local index from the official Adélie Git mirror and
    /// performing optimized searches.
    ///
    /// # Performance
    /// - Uses `VecDeque<&str>` for zero-allocation argument parsing.
    /// - Leverages lazy loading for the database content to minimize memory footprint.
    ///
    /// # Returns
    /// - `Ok(())` on success.
    /// - `Err` if any operation fails, including network or filesystem errors.
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        let mut args: VecDeque<&str> = self.remaining_args.iter().map(|s| s.as_str()).collect();

        if args.is_empty() {
            return missing_arg!("aptree");
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
                    output_dir = parse_value!("aptree", "directory", arg)?.into();
                }
                "-o" | "--output" => {
                    output_dir = parse_value!("aptree", "directory", arg, args.pop_front())?.into();
                }
                a if a.starts_with("--search=") => {
                    (search, bk) = (true, true);
                    s_pkg.push(parse_value!("aptree", "package", arg)?);
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
                    get_pkg.push(parse_value!("aptree", "package", arg)?);
                    collect_args(&mut args, &mut get_pkg);
                }
                "-g" | "--get" => {
                    (get, bk) = (true, true);
                    get_pkg.push(parse_value!("aptree", "package", arg, args.pop_front())?);
                    collect_args(&mut args, &mut get_pkg);
                }
                a if a.starts_with("--rootfs=") => {
                    rootfs_dir = parse_value!("aptree", "directory", arg)?.into();
                }
                "-R" | "--rootfs" => {
                    rootfs_dir = parse_value!("aptree", "directory", arg, args.pop_front())?.into();
                }
                other => return invalid_arg!("aptree", other),
            }
        }

        if !bk {
            return missing_arg!("aptree", essential);
        }

        if update {
            utils::update_git_repository(
                rootfs_dir.clone(),
                "https://git.adelielinux.org/adelie/packages.git",
                "aptree",
                &["bootstrap", "experimental", "legacy", "system", "user"],
            )?;

            if !search && !get {
                return Ok(());
            }
        }

        utils::check_rootfs_exists(rootfs_dir.clone())?;

        let db_path = rootfs_dir.join("build/aptree-database");

        if !db_path.exists() {
            return Err(format!(
                "The aptree database was not found at: {}\nPlease run '{} aptree -u' first to initialize the repository.",
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
                rootfs_dir, "aptree", &get_pkg, &content, output_dir,
            )?;
        }
        Ok(())
    }
}
