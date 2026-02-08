//! Settings management for ALPack.
//!
//! Handles loading, saving, and displaying configuration using a thread-safe
//! global path and safe home directory fallbacks.

use crate::concat_path;
use crate::utils::SAFE_HOME;

use serde::{Deserialize, Serialize};
use std::string::ToString;
use std::sync::LazyLock;
use std::{env, fs, io};

/// Absolute path to the configuration directory.
static CONFIG_DIR: LazyLock<String> =
    LazyLock::new(|| concat_path!(SAFE_HOME.wait(), ".config/ALPack"));

/// Absolute path to the config.toml file.
static CONFIG_FILE: LazyLock<String> = LazyLock::new(|| concat_path!(&*CONFIG_DIR, "config.toml"));

/// Application configuration settings.
#[derive(Serialize, Deserialize, Debug)]
pub struct Settings {
    /// The default Alpine Linux mirror URL.
    pub default_mirror: String,
    /// Directory used for caching downloaded files.
    pub cache_dir: String,
    /// Directory where the rootfs will be extracted/managed.
    pub rootfs_dir: String,
    /// The command used to run the rootfs (e.g., proot, chroot).
    pub cmd_rootfs: String,
    /// The target Alpine release version.
    pub release: String,
    /// Default output directory for build artifacts.
    pub output_dir: String,
}

impl Default for Settings {
    /// Provides default settings based on the safe home directory.
    fn default() -> Self {
        let home = SAFE_HOME.wait();
        Self {
            default_mirror: "https://dl-cdn.alpinelinux.org/alpine/".to_string(),
            cache_dir: concat_path!(home, ".cache/ALPack"),
            rootfs_dir: concat_path!(home, ".ALPack"),
            cmd_rootfs: "proot".to_string(),
            release: "latest-stable".to_string(),
            output_dir: String::new(),
        }
    }
}

impl Settings {
    /// Loads the configuration from the config file, or creates a default one.
    ///
    /// This method will attempt to read the TOML file from disk. If the file
    /// is missing or corrupted, it initializes a new one with default values.
    ///
    /// # Returns
    /// - A `Settings` struct populated from disk or defaults.
    pub fn load() -> Self {
        let path = &*CONFIG_FILE;

        match fs::read_to_string(path) {
            Ok(content) if content.is_empty() => {
                eprintln!("\x1b[1;33mWarning\x1b[0m: Config file is empty. Using defaults.");
                Self::create()
            }
            Ok(content) => toml::from_str(&content).unwrap_or_else(|_| {
                eprintln!("\x1b[1;33mWarning\x1b[0m: Failed to parse config. Using defaults.");
                Self::create()
            }),
            Err(_) => Self::create(),
        }
    }

    /// Creates a new configuration file with default values.
    ///
    /// Ensures the parent directory exists before writing the serialized
    /// default settings to the disk.
    ///
    /// # Returns
    /// - A `Settings` struct containing default values.
    fn create() -> Self {
        let default = Settings::default();
        let _ = fs::create_dir_all(&*CONFIG_DIR);

        if let Err(e) = fs::write(
            &*CONFIG_FILE,
            toml::to_string_pretty(&default).unwrap_or_default(),
        ) {
            eprintln!("\x1b[1;33mWarning\x1b[0m: Failed to write default config file: {e}");
        }

        default
    }

    /// Saves the current configuration to the default config file path.
    ///
    /// # Returns
    /// - `Ok(())` if the file was successfully written.
    /// - `Err` if serialization or the write operation fails.
    pub fn save(&self) -> io::Result<()> {
        let _ = fs::create_dir_all(&*CONFIG_DIR);

        let toml_data = toml::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        fs::write(&*CONFIG_FILE, toml_data)
    }

    /// Displays the current configuration from the disk and compares it with in-memory settings.
    ///
    /// Fields that differ will be highlighted using ANSI color codes to show
    /// the transition from the old value to the new value.
    #[allow(unused_variables, unused_mut)]
    pub fn show_config_changes(&self) {
        let disk_config = fs::read_to_string(&*CONFIG_FILE)
            .ok()
            .and_then(|s| toml::from_str::<Settings>(&s).ok());
        let mut rows: Vec<(String, String)> = Vec::new();

        macro_rules! show_field {
            ($field:ident) => {
                let name = stringify!($field).to_string();
                let mut new_v = self.$field.clone();

                if name == "output_dir" && new_v.is_empty() {
                    new_v = "Current Directory or Home Fallback".to_string();
                }

                let value_str = if let Some(old) = &disk_config {
                    let mut old_v = old.$field.clone();
                    if name == "output_dir" && old_v.is_empty() {
                        old_v = "Current Directory or Home Fallback".to_string();
                    }

                    if old_v != new_v {
                        format!("\x1b[1;31m{old_v}\x1b[0m -> \x1b[1;32m{new_v}\x1b[0m")
                    } else {
                        new_v
                    }
                } else {
                    new_v
                };
                rows.push((name, value_str));
            };
        }

        show_field!(default_mirror);
        show_field!(cache_dir);
        show_field!(rootfs_dir);
        show_field!(cmd_rootfs);
        show_field!(release);
        show_field!(output_dir);

        self.render_table(rows);
    }

    /// Renders a formatted table for configuration display.
    ///
    /// This method calculates the necessary column widths and draws a terminal-based
    /// table using Unicode box-drawing characters. It specifically handles ANSI
    /// escape sequences (used for coloring) by compensating for their invisible
    /// length to maintain border alignment.
    ///
    /// # Parameters
    /// - `rows`: A vector of tuples containing the field name and its formatted value.
    fn render_table(&self, rows: Vec<(String, String)>) {
        let key_width = rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
        let val_width = rows
            .iter()
            .map(|(_, v)| {
                if v.contains("->") {
                    v.len().saturating_sub(22)
                } else {
                    v.len()
                }
            })
            .max()
            .unwrap_or(0);

        println!(
            "╔═{}═══╦═{}═══╗",
            "═".repeat(key_width),
            "═".repeat(val_width)
        );
        for (k, v) in rows {
            let padding = if v.contains("->") {
                val_width + 22
            } else {
                val_width
            };
            println!("║ {:<key_width$}   ║ {:<padding$}   ║", k, v);
        }
        println!(
            "╚═{}═══╩═{}═══╝",
            "═".repeat(key_width),
            "═".repeat(val_width)
        );
    }

    /// Determines the output directory for the application.
    ///
    /// # Returns
    /// - `Ok(String)` containing the path, or a home fallback on permission error.
    pub fn set_output_dir() -> io::Result<String> {
        let current = env::current_dir()?.display().to_string();

        if fs::read_dir(&current).is_ok() {
            Ok(current)
        } else {
            Ok(SAFE_HOME.wait().to_string())
        }
    }

    /// Determines the root filesystem directory for the application.
    ///
    /// # Returns
    /// - `String` containing the path, prioritized by environment variables.
    pub fn set_rootfs(&self) -> String {
        env::var("ALPACK_ROOTFS").unwrap_or_else(|_| self.rootfs_dir.clone())
    }

    /// Determines the cache directory for the application.
    ///
    /// # Returns
    /// - `String` containing the path, prioritized by environment variables.
    pub fn set_cache_dir(&self) -> String {
        env::var("ALPACK_CACHE").unwrap_or_else(|_| self.cache_dir.clone())
    }
}
