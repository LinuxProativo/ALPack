//! Global configuration management for ALPack.
//!
//! This module handles the `config` subcommand, allowing users to modify
//! persistent settings such as rootfs isolation tools, release channels,
//! and directory paths via CLI arguments.

use crate::settings::Settings;
use crate::{invalid_arg, parse_key_value};

use std::collections::VecDeque;
use std::error::Error;

/// Configuration manager for updating application settings.
pub struct Config {
    /// List of command-line arguments to be parsed.
    remaining_args: Vec<String>,
}

impl Config {
    /// Creates a new `Config` instance with a vector of string arguments passed to the config.
    pub fn new(remaining_args: Vec<String>) -> Self {
        Config { remaining_args }
    }

    /// Parses arguments and updates the persistent settings.
    ///
    /// Processes flags for isolation tools (`--use-proot`, `--use-bwrap`),
    /// release channels, and directory configurations. Changes are displayed
    /// to the user and saved to the configuration file if modifications occur.
    ///
    /// # Returns
    /// * `Ok(())` - If configuration was successfully updated and saved.
    /// * `Err` - If an invalid argument is provided or parsing fails.
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        let mut args: VecDeque<&str> = self.remaining_args.iter().map(|s| s.as_str()).collect();
        let mut sett = Settings::load_or_create();

        while let Some(arg) = args.pop_front() {
            match arg {
                "--use-proot" => sett.cmd_rootfs = "proot".to_string(),
                "--use-bwrap" => sett.cmd_rootfs = "bwrap".to_string(),
                "--use-latest-stable" => sett.release = "latest-stable".to_string(),
                "--use-edge" => sett.release = "edge".to_string(),
                a if a.starts_with("--cache-dir=") => {
                    sett.cache_dir = parse_key_value!("config", "directory", arg)?;
                }
                "--cache-dir" => {
                    sett.cache_dir =
                        parse_key_value!("config", "directory", arg, args.pop_front())?;
                }
                a if a.starts_with("--rootfs-dir=") => {
                    sett.rootfs_dir = parse_key_value!("config", "directory", arg)?;
                }
                "--rootfs-dir" => {
                    sett.rootfs_dir =
                        parse_key_value!("config", "directory", arg, args.pop_front())?;
                }
                a if a.starts_with("--output-dir=") => {
                    sett.output_dir = parse_key_value!("config", "directory", arg)?;
                }
                "--output-dir" => {
                    sett.output_dir =
                        parse_key_value!("config", "directory", arg, args.pop_front())?;
                }
                a if a.starts_with("--default-mirror=") => {
                    sett.default_mirror = parse_key_value!("config", "mirror", arg)?;
                }
                "--default-mirror" => {
                    sett.default_mirror =
                        parse_key_value!("config", "mirror", arg, args.pop_front())?;
                }
                _ => return invalid_arg!("config", arg),
            }
        }

        sett.show_config_changes();
        if !self.remaining_args.is_empty() {
            sett.save()?;
        }
        Ok(())
    }
}
