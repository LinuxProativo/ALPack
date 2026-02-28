//! Utility functions for ALPack.
//!
//! Provides helper methods for path manipulation, environment discovery,
//! file downloads, and stylized terminal output.

use recursive_copy::{copy_recursive, CopyOptions};
use sandbox_utils::{
    app_name, failed_exist_rootfs, get_cmd_box, RootfsNotFoundError, SandBox, SandBoxConfig,
    SEPARATOR,
};
use std::collections::HashSet;
use std::collections::VecDeque;
use std::error::Error;
use std::fs;
use std::path::PathBuf;

/// Collects positional arguments from the queue until a new flag (starting with '-') is encountered.
///
/// This function is useful for commands that accept multiple values, such as:
/// `aports --get pkg1 pkg2 pkg3 --output /tmp`
///
/// # Parameters
/// * `args`: A mutable reference to the remaining CLI arguments queue.
/// * `target`: A mutable reference to the `Vec<String>` where collected arguments will be stored.
pub fn collect_args(args: &mut VecDeque<&str>, target: &mut Vec<String>) {
    while let Some(arg) = args.pop_front() {
        if arg.starts_with('-') {
            args.push_front(arg);
            break;
        }

        target.push(arg.to_string());
    }
}

/// Verifies that the specified rootfs directory exists and is accessible.
///
/// # Parameters
/// - `path`: The directory path to verify.
///
/// # Returns
/// - `Ok(())` if the directory exists.
/// - `Err` with diagnostic info if missing.
pub fn check_rootfs_exists(path: PathBuf) -> Result<(), Box<dyn Error>> {
    if !path.is_dir() {
        return failed_exist_rootfs(
            &format!("{} setup", app_name()),
            &path.display().to_string(),
        );
    }
    Ok(())
}

/// Maps sandbox errors to visual terminal dialogs.
///
/// # Arguments
/// * `result` - The result from a SandBox execution.
///
/// # Returns
/// The original result or a formatted error dialog.
pub fn map_result<T>(result: Result<T, Box<dyn Error>>) -> Result<T, Box<dyn Error>> {
    result.map_err(|e| {
        if let Some(err) = e.downcast_ref::<RootfsNotFoundError>() {
            return failed_exist_rootfs(&format!("{} setup", app_name()), &err.0.to_string_lossy())
                .unwrap_err();
        }
        e
    })
}

/// Matches packages against the database content and prints a standardized result box.
///
/// This function internalizes the search logic by invoking the `collect_matches!` macro.
/// It aggregates results from the provided database content based on the given package keys.
///
/// # Parameters
/// - `pkgs`: A slice of strings containing the package names or patterns to search for.
/// - `content`: The raw string content of the database file to be scanned.
///
/// # Returns
/// - `Ok(())` if matches were found and successfully printed to stdout.
/// - `Err` if the search result is empty or if the UI box generation fails.
pub fn print_result(pkgs: &[String], content: &str, generic: bool) -> Result<(), Box<dyn Error>> {
    let mut all_matches = Vec::new();

    if generic {
        for term in pkgs {
            let matches = collect_generic_matches(term, content);
            all_matches.extend(matches);
        }
    } else {
        let matches = collect_unique_pkgs(pkgs, content);
        all_matches.extend(matches);
    }

    if all_matches.is_empty() {
        return Err(format!("{u}\nResult not found!\n{u}", u = SEPARATOR).into());
    }

    let mut sorted_matches: Vec<&str> = all_matches.into_iter().collect();
    sorted_matches.sort();
    sorted_matches.dedup();

    let result_output = sorted_matches.join("\n");

    println!(
        "{u}\n{}\n{result_output}\n{u}",
        get_cmd_box("SEARCH RESULT:", None, Some(18))?,
        u = SEPARATOR
    );

    Ok(())
}

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
pub fn update_git_repository(
    rootfs_dir: PathBuf,
    url: &str,
    repo: &str,
    branches: &[&str],
) -> Result<(), Box<dyn Error>> {
    let build_path = rootfs_dir.join("build").join(repo);
    let database_path = rootfs_dir.join("build").join(format!("{repo}-database"));

    let _ = fs::remove_dir_all(&build_path);
    let _ = fs::remove_file(&database_path);

    fs::create_dir_all(&build_path)?;

    let filter = branches.join("|");
    let cmd_script = format!(
        "type git > /dev/null || apk add git
        cd /build
        git clone --depth=1 --filter=tree:0 --no-checkout {url} {repo} && \
        cd {repo} && \
        git fetch --depth=1 --filter=tree:0 && \
        git ls-tree -r HEAD --name-only | grep -E \"({filter})\" > ../{repo}-database",
    );

    let config = SandBoxConfig {
        rootfs: rootfs_dir.into(),
        run_cmd: cmd_script,
        use_root: true,
        ignore_extra_bind: true,
        ..Default::default()
    };

    map_result(SandBox::run(config))?;
    Ok(())
}

/// Orchestrates the selective retrieval of package sources from a git repository.
///
/// It processes match results to identify relevant package directories,
/// configures Git's sparse-checkout to download only those specific paths,
/// and copies the resulting files to the final output destination.
///
/// # Parameters
/// - `rootfs`: Path to the root filesystem host directory.
/// - `repo_name`: The subdirectory name within `/build/` (e.g., "aports").
/// - `pkgs`: A slice of strings containing the package names to be retrieved.
/// - `content`: The raw string content of the database file.
/// - `output`: The destination directory for the retrieved files.
///
/// # Returns
/// - `Ok(())` if all package files were retrieved and copied.
/// - `Err` if no matches are found or if the sparse-checkout process fails.
pub fn download_git_sources_files(
    rootfs: PathBuf,
    repo_name: &str,
    pkgs: &[String],
    content: &str,
    output: PathBuf,
) -> Result<(), Box<dyn Error>> {
    let matches = collect_unique_pkgs(pkgs, content);

    if matches.is_empty() {
        return Err(format!("{u}\nResult not found!\n{u}", u = SEPARATOR).into());
    }

    let pkg_dirs: HashSet<&str> = matches
        .iter()
        .filter(|line| line.contains("APKBUILD"))
        .filter_map(|line| line.rsplit_once('/').map(|(dir, _)| dir))
        .collect();

    let pkg_dirs_vec: Vec<&str> = pkg_dirs.into_iter().collect();

    let run_cmd = format!(
        "cd /build/{repo_name}
         git sparse-checkout init --cone && \
         git sparse-checkout set {} && \
         git checkout",
        pkg_dirs_vec.join(" ")
    );

    let config = SandBoxConfig {
        rootfs: rootfs.clone(),
        run_cmd,
        use_root: true,
        ignore_extra_bind: true,
        ..Default::default()
    };

    map_result(SandBox::run(config))?;

    let options = CopyOptions {
        overwrite: true,
        follow_symlinks: true,
        ..Default::default()
    };

    for dir in pkg_dirs_vec {
        copy_recursive(
            &rootfs.join("build").join(repo_name).join(dir),
            &output,
            &options,
        )?;
    }
    Ok(())
}

/// Collects unique lines from the database that match specific package names.
///
/// This function scans the provided content for lines that represent an `APKBUILD`
/// file within a specific package directory structure. It ensures that each
/// matching line is returned only once, even if multiple search terms overlap.
///
/// # Parameters
/// * `pkgs`: A slice of `String` containing the names of the packages to search for.
/// * `content`: The raw string content of the aports database (usually read from a file).
///   The function uses the lifetime `'a` to ensure returned references to remain valid
///   as long as this content exists in memory.
///
/// # Returns
/// A `HashSet<&'a str>` containing unique matching lines from the `content`.
/// Each line is a reference to a slice of the original `content` string,
/// avoiding unnecessary memory allocations.
pub fn collect_unique_pkgs<'a>(pkgs: &[String], content: &'a str) -> HashSet<&'a str> {
    let mut unique_matches = HashSet::new();

    for pkg in pkgs {
        let pattern = format!("/{}/", pkg);
        let matches = content.lines().filter(|line| line.contains(&pattern));
        unique_matches.extend(matches);
    }

    unique_matches
}

/// Performs a generic search across the database content.
///
/// It returns any line that contains the search term, useful for discovering
/// packages when the exact name is not known.
///
/// # Parameters
/// * `term`: The search string (e.g., "glib").
/// * `content`: The database content.
///
/// # Returns
/// A sorted `Vec<&str>` of unique matching lines.
pub fn collect_generic_matches<'a>(term: &str, content: &'a str) -> Vec<&'a str> {
    let matches: HashSet<&str> = content.lines().filter(|line| line.contains(term)).collect();

    let mut sorted_matches: Vec<&str> = matches.into_iter().collect();
    sorted_matches.sort();
    sorted_matches
}
