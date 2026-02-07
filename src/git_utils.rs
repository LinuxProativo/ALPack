//! Repository management and package retrieval utilities.
//!
//! This module provides functions to synchronize local package databases
//! and selectively download package sources using Git's sparse-checkout
//! feature, optimized for minimal disk I/O and network usage.

use crate::command::Command;
use crate::utils::SEPARATOR;
use crate::{concat_path, utils};

use std::error::Error;

/// Sets up a local repository database within the rootfs.
///
/// This function ensures the build directory exists, clones the remote
/// repository using a blobless filter (`tree:0`) to save bandwidth, and
/// generates a flattened database file by filtering specific branches.
///
/// # Parameters
/// - `rootfs_dir`: Path to the root filesystem host directory.
/// - `url`: The remote Git repository URL.
/// - `repo`: The local name for the repository (e.g., "aports").
/// - `branches`: A list of branch names or paths to include in the database.
///
/// # Returns
/// - `Ok(())` if the repository was successfully initialized and indexed.
/// - `Err` if Git operations or filesystem modifications fail.
pub fn setup_repository(
    rootfs_dir: &str,
    url: &str,
    repo: &str,
    branches: &[&str],
) -> Result<(), Box<dyn Error>> {
    let build_path = concat_path!(rootfs_dir, "build");

    if std::fs::metadata(&build_path).is_ok() {
        std::fs::remove_dir_all(&build_path)?;
    }
    std::fs::create_dir_all(&build_path)?;

    let filter = branches.join("|");
    let cmd_script = format!(
        "which git > /dev/null || apk add git
        cd /build
        git clone --depth=1 --filter=tree:0 --no-checkout {url} {repo} 2> /dev/null
        cd {repo}
        git fetch --depth=1 --filter=tree:0
        git ls-tree -r HEAD --name-only | grep -E \"({filter})\" > ../{repo}-database",
    );

    Command::run(rootfs_dir, None, Some(cmd_script), true, true, false)?;
    Ok(())
}

/// Orchestrates the selective retrieval of package sources from a git repository.
///
/// It processes match results to identify relevant package directories,
/// configures Git's sparse-checkout to download only those specific paths,
/// and copies the resulting files to the final output destination.
///
/// # Parameters
/// - `rootfs`: Path to the root filesystem where the repo is located.
/// - `repo_name`: The subdirectory name within `/build/` (e.g., "aports").
/// - `matches`: The raw match strings containing APKBUILD paths.
/// - `output`: The destination directory for the copied package files.
///
/// # Returns
/// - `Ok(())` if all package files were retrieved and copied.
/// - `Err` if no matches are found or if the sparse-checkout process fails.
pub fn fetch_package_files(
    rootfs: &str,
    repo_name: &str,
    matches: &str,
    output: &str,
) -> Result<(), Box<dyn Error>> {
    if matches.is_empty() {
        return Err(format!("{u}\nResult not found!\n{u}", u = SEPARATOR).into());
    }

    let pkg_dirs: Vec<&str> = matches
        .lines()
        .filter(|l| l.contains("APKBUILD"))
        .filter_map(|l| l.rsplit_once('/').map(|(path, _)| path))
        .collect();

    if pkg_dirs.is_empty() {
        return Err("No valid APKBUILD paths found in matches.".into());
    }

    let cmd = format!(
        "cd /build/{repo_name} && \
         git sparse-checkout init --cone && \
         git sparse-checkout set {} && \
         git checkout",
        pkg_dirs.join(" ")
    );

    Command::run(rootfs, None, Some(cmd), true, true, false)?;

    for dir in pkg_dirs {
        utils::copy_dir_recursive(
            concat_path!(rootfs, "build", repo_name, dir).as_ref(),
            output.as_ref(),
        )?;
    }

    Ok(())
}
