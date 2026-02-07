//! Alpine Package Ports (aports) management module.
//!
//! This module provides the `Aports` struct and logic to interact with the
//! Alpine Linux aports repository, allowing for database updates,
//! package searching, and source file retrieval via sparse-checkout.

use crate::settings::Settings;
use crate::{collect_args, concat_path, git_utils, parse_key_value};
use crate::{invalid_arg, missing_arg, utils};

use std::collections::VecDeque;
use std::error::Error;
use std::fs;

/// Controller for Alpine Linux repository operations.
pub struct Aports<'a> {
    /// The name of the current execution context.
    name: &'a str,
    /// Arguments passed from the CLI for processing.
    remaining_args: Vec<String>,
}

impl<'a> Aports<'a> {
    /// Creates a new `Aports` instance with the given context and arguments.
    pub fn new(name: &'a str, remaining_args: Vec<String>) -> Self {
        Aports {
            name,
            remaining_args,
        }
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
            return missing_arg!(self.name, "aports");
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
                    output = parse_key_value!("aports", "directory", arg)?;
                }
                "-o" | "--output" => {
                    output = parse_key_value!("aports", "directory", arg, args.pop_front())?;
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
                    rootfs_dir = parse_key_value!("aports", "directory", arg)?;
                }
                "-R" | "--rootfs" => {
                    rootfs_dir = parse_key_value!("aports", "directory", arg, args.pop_front())?;
                }
                other => return invalid_arg!(self.name, "aports", other),
            }
        }

        if !bk {
            return missing_arg!(self.name, "aports", essential);
        }

        if update {
            git_utils::setup_repository(
                &rootfs_dir,
                "https://github.com/alpinelinux/aports.git",
                "aports",
                &["main", "community", "testing"],
            )?;

            if !search && !get {
                return Ok(());
            }
        }

        utils::check_rootfs_exists(self.name, &rootfs_dir)?;
        let content = fs::read_to_string(concat_path!(rootfs_dir, "build", "aports-database"))?;

        if search {
            git_utils::print_result(&search_pkg, &content)?;

            if !get {
                return Ok(());
            }
        }

        if get {
            git_utils::fetch_package_files(&rootfs_dir, "aports", &get_pkg, &content, &output)?;
        }
        Ok(())
    }
}
