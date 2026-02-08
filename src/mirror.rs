//! Mirror and Repository URI management.
//!
//! This module orchestrates the construction of Alpine Linux download URLs.
//! It combines mirrors, release branches, and system architecture to generate
//! valid paths for rootfs tarballs and APK repositories.

use crate::settings::Settings;
use crate::utils;

use std::error::Error;

/// Manager for Alpine Linux mirror and release metadata.
pub struct Mirror {
    /// The base URL of the Alpine mirror (e.g., https://dl-cdn.alpinelinux.org/alpine/).
    mirror: Option<String>,
    /// The target release version or branch (e.g., v3.18, edge).
    release: Option<String>,
}

impl Mirror {
    /// Creates a new Mirror instance with optional overrides.
    pub fn new(mirror: Option<String>, release: Option<String>) -> Self {
        Mirror { mirror, release }
    }

    /// Initializes missing mirror/release values using global settings.
    ///
    /// # Returns
    /// * `Ok(())` - Always returns success after ensuring values are present.
    pub fn run(&mut self) -> Result<(), Box<dyn Error>> {
        let sett = Settings::load();

        if self.mirror.as_deref().unwrap_or("").is_empty() {
            self.mirror = Some(sett.default_mirror);
        }
        if self.release.as_deref().unwrap_or("").is_empty() {
            self.release = Some(sett.release);
        }
        Ok(())
    }

    /// Constructs the base URL for fetching the rootfs tarball.
    ///
    /// # Returns
    /// A formatted string: `<mirror><release>/releases/<arch>/`
    pub fn get_mirror(&self) -> String {
        format!(
            "{}{}/releases/{}/",
            self.mirror.as_deref().unwrap_or(""),
            self.release.as_deref().unwrap_or(""),
            utils::get_arch()
        )
    }

    /// Generates the multi-line repository list for the `apk` manager.
    ///
    /// # Returns
    /// A string containing `main` and `community` repository URLs.
    /// If the release is `edge`, the `testing` repository is also included.
    pub fn get_repository(&mut self) -> String {
        let mirror = self.mirror.as_deref().unwrap_or("");
        let release = self.release.as_deref().unwrap_or("");

        let mut repos = format!("{mirror}{release}/main\n{mirror}{release}/community",);

        if release == "edge" {
            repos.push_str(&format!("\n{mirror}{release}/testing"));
        }

        repos
    }
}
