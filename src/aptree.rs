//! Adélie Package Tree (aptree) management module.
//!
//! This module provides the `Aptree` struct and logic to interact with the
//! Adélie Linux package repository. It supports database synchronization,
//! package searching, and source retrieval via Git sparse-checkout,
//! specifically tailored for Adélie's repository structure.

use crate::settings::Settings;
use crate::{collect_args, concat_path, git_utils, parse_key_value};
use crate::{invalid_arg, missing_arg, utils};

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

        let sett = Settings::load_or_create();
        let mut rootfs_dir = sett.set_rootfs();
        let (mut search_pkg, mut get_pkg) = (Vec::new(), Vec::new());

        let mut output = if !sett.output_dir.is_empty() {
            sett.output_dir
        } else {
            Settings::set_output_dir()?
        };

        let (mut update, mut search, mut get, mut bk) = (false, false, false, false);

        while let Some(arg) = args.pop_front() {
            match arg {
                "-u" | "--update" => (update, bk) = (true, true),
                a if a.starts_with("--output=") => {
                    output = parse_key_value!("aptree", "directory", arg)?;
                }
                "-o" | "--output" => {
                    output = parse_key_value!("aptree", "directory", arg, args.pop_front())?;
                }
                a if a.starts_with("--search=") => {
                    (search, bk) = (true, true);
                    search_pkg.push(parse_key_value!("aptree", "package", arg)?);
                    collect_args!(args, search_pkg);
                }
                "-s" | "--search" => {
                    (search, bk) = (true, true);
                    search_pkg.push(parse_key_value!(
                        "aptree",
                        "package",
                        arg,
                        args.pop_front()
                    )?);
                    collect_args!(args, search_pkg);
                }
                a if a.starts_with("--get=") => {
                    (get, bk) = (true, true);
                    get_pkg.push(parse_key_value!("aptree", "package", arg)?);
                    collect_args!(args, get_pkg);
                }
                "-g" | "--get" => {
                    (get, bk) = (true, true);
                    get_pkg.push(parse_key_value!(
                        "aptree",
                        "package",
                        arg,
                        args.pop_front()
                    )?);
                    collect_args!(args, get_pkg);
                }
                a if a.starts_with("--rootfs=") => {
                    rootfs_dir = parse_key_value!("aptree", "directory", arg)?;
                }
                "-R" | "--rootfs" => {
                    rootfs_dir = parse_key_value!("aptree", "directory", arg, args.pop_front())?;
                }
                other => return invalid_arg!("aptree", other),
            }
        }

        if !bk {
            return missing_arg!("aptree", essential);
        }

        if update {
            git_utils::setup_repository(
                &rootfs_dir,
                "https://git.adelielinux.org/adelie/packages.git",
                "aptree",
                &["bootstrap", "experimental", "legacy", "system", "user"],
            )?;

            if !search && !get {
                return Ok(());
            }
        }

        utils::check_rootfs_exists(&rootfs_dir)?;
        let content = fs::read_to_string(concat_path!(rootfs_dir, "build", "aptree-database"))?;

        if search {
            git_utils::print_result(&search_pkg, &content)?;

            if !get {
                return Ok(());
            }
        }

        if get {
            git_utils::fetch_package_files(&rootfs_dir, "aptree", &get_pkg, &content, &output)?;
        }
        Ok(())
    }
}
