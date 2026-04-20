//! Settings management for ALPack.
//!
//! Handles loading, saving, and displaying configuration using a thread-safe
//! global path and safe home directory fallbacks.

use sandbox_utils::{
    config_file, default_cache, default_rootfs, get_config_diff, render_table, safe_home,
    InodeMode, OverlayAction, USE_PROOT,
};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::{env, fs};

/// Application configuration settings.
#[derive(Serialize, Deserialize, Debug)]
pub struct Settings {
    /// The default Alpine Linux mirror URL.
    pub default_mirror: String,
    /// Directory used for caching downloaded files.
    pub cache_dir: PathBuf,
    /// Directory where the rootfs will be extracted/managed.
    pub rootfs_dir: PathBuf,
    /// The command used to run the rootfs (e.g., proot, chroot).
    pub cmd_rootfs: String,
    /// The target Alpine release version.
    pub release: String,
    /// Default output directory for build artifacts.
    pub output_dir: PathBuf,
    /// Whether to use an overlay filesystem (e.g., OverlayFS) for the rootfs.
    pub use_overlay: bool,
    /// The inode management mode for the overlay (e.g., Virtual or Persistent).
    pub overlay_inode_mode: InodeMode,
    /// The cleanup or preserve action to take on the overlay after execution.
    pub overlay_action: OverlayAction,
}

/// Global thread-safe storage for application settings.
static SETTINGS: OnceLock<Settings> = OnceLock::new();

impl Default for Settings {
    /// Provides default settings based on the safe home directory.
    fn default() -> Self {
        Self {
            default_mirror: "https://dl-cdn.alpinelinux.org/alpine/".to_string(),
            cache_dir: default_cache(),
            rootfs_dir: default_rootfs(),
            cmd_rootfs: USE_PROOT.to_string(),
            release: "latest-stable".to_string(),
            output_dir: PathBuf::new(),
            use_overlay: false,
            overlay_inode_mode: InodeMode::Virtual,
            overlay_action: OverlayAction::Preserve,
        }
    }
}

impl Settings {
    /// Provides global access to the loaded settings.
    ///
    /// If the settings haven't been loaded yet, it initializes them from the disk.
    ///
    /// # Returns
    /// A reference to the global `Settings` instance.
    pub fn global() -> &'static Settings {
        SETTINGS.get_or_init(Self::load)
    }

    /// Loads the configuration from the config file, or creates a default one.
    ///
    /// This method will attempt to read the TOML file from disk. If the file
    /// is missing or corrupted, it initializes a new one with default values.
    ///
    /// # Returns
    /// - A `Settings` struct populated from disk or defaults.
    pub fn load() -> Self {
        let path = config_file();

        match fs::read_to_string(path) {
            Ok(content) if content.is_empty() => Self::create(),
            Ok(content) => toml::from_str(&content).unwrap_or_else(|_| Self::create()),
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
        let path = config_file();
        let _ = fs::write(&path, toml::to_string_pretty(&default).unwrap_or_default());
        default
    }

    /// Saves the current configuration to the default config file path.
    ///
    /// # Returns
    /// - `Ok(())` if the file was successfully written.
    /// - `Err` if serialization or the write operation fails.
    pub fn save(&self) -> Result<(), Box<dyn Error>> {
        let toml_data = toml::to_string_pretty(self)?;
        fs::write(config_file(), toml_data)?;
        Ok(())
    }

    /// Displays the current configuration from the disk and compares it with in-memory settings.
    ///
    /// Fields that differ will be highlighted using ANSI color codes to show
    /// the transition from the old value to the new value.
    pub fn show_config_changes(&self) {
        let disk_config = fs::read_to_string(config_file())
            .ok()
            .and_then(|s| toml::from_str::<Settings>(&s).ok());

        let rows = match disk_config {
            Some(old) => get_config_diff(&old, self),
            None => get_config_diff(self, self),
        };

        render_table(rows);
    }
}

/// Returns the default Alpine Linux mirror URL.
///
/// This value is retrieved from the global settings initialized from the configuration file.
///
/// # Returns
/// A `String` containing the mirror URL (e.g., "https://dl-cdn.alpinelinux.org/alpine/").
pub fn settings_mirror() -> String {
    SETTINGS.wait().default_mirror.clone()
}

/// Returns the active root filesystem directory.
///
/// Resolution priority:
/// 1. `ALPACK_ROOTFS` environment variable.
/// 2. `rootfs_dir` value from the configuration file.
///
/// # Returns
/// A `PathBuf` pointing to the directory where the rootfs is managed.
pub fn settings_rootfs_dir() -> PathBuf {
    env::var("ALPACK_ROOTFS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| SETTINGS.wait().rootfs_dir.clone())
}

/// Returns the active cache directory for downloads.
///
/// Resolution priority:
/// 1. `ALPACK_CACHE` environment variable.
/// 2. `cache_dir` value from the configuration file.
///
/// # Returns
/// A `PathBuf` pointing to the location used for storing cached files.
pub fn settings_cache_dir() -> PathBuf {
    env::var("ALPACK_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| SETTINGS.wait().cache_dir.clone())
}

/// Returns the command used to execute the sandbox.
///
/// Common values include `"proot"` or `"bwrap"`.
///
/// # Returns
/// A `String` representing the binary name or command configured for the rootfs.
pub fn settings_cmd() -> String {
    SETTINGS.wait().cmd_rootfs.clone()
}

/// Returns the target Alpine Linux release version.
///
/// Usually defaults to `"latest-stable"` or a specific version like `"v3.18"`.
///
/// # Returns
/// A `String` containing the release identifier.
pub fn settings_release() -> String {
    SETTINGS.wait().release.clone()
}

/// Returns the output directory for build artifacts with fallback logic.
///
/// If no output directory is explicitly set in the configuration, it attempts to
/// return the current working directory, falling back to the user's safe home
/// directory if the current directory is inaccessible.
///
/// # Returns
/// A `PathBuf` representing the destination for generated files.
pub fn settings_output_dir() -> PathBuf {
    let out = &SETTINGS.wait().output_dir;
    if out.as_os_str().is_empty() {
        env::current_dir().unwrap_or_else(|_| safe_home())
    } else {
        out.clone()
    }
}

/// Returns whether the overlay filesystem is enabled.
///
/// # Returns
/// `true` if the sandbox should use an overlay layer over the rootfs.
pub fn settings_use_overlay() -> bool {
    SETTINGS.wait().use_overlay
}

/// Returns the configured action for the overlay after the session ends.
///
/// Common actions include preserving the changes or discarding them.
///
/// # Returns
/// An `OverlayAction` variant.
pub fn settings_overlay_action() -> OverlayAction {
    SETTINGS.wait().overlay_action.clone()
}

/// Returns the inode handling mode for the overlay filesystem.
///
/// # Returns
/// An `InodeMode` variant determining how file identifiers are managed.
pub fn settings_overlay_inode_mode() -> InodeMode {
    SETTINGS.wait().overlay_inode_mode.clone()
}
