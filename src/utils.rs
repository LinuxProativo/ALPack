//! Utility functions for ALPack.
//!
//! Provides helper methods for path manipulation, environment discovery,
//! file downloads, and stylized terminal output.

use crate::concat_path;
use crate::settings::Settings;

use indicatif::{ProgressBar, ProgressStyle};
use std::error::Error;
use std::fs::File;
use std::os::unix::fs::PermissionsExt;
use std::sync::OnceLock;
use std::{env, fs, io};
use walkdir_minimal::WalkDir;
use which::which;

/// Progress bar template for downloads and extractions.
pub const DOWNLOAD_TEMPLATE: &str = "{msg} {spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})";

/// Visual separator for terminal output.
pub const SEPARATOR: &str = "════════════════════════════════════════════════════════════";

/// Cached application name.
pub static APP_NAME: OnceLock<String> = OnceLock::new();

/// Cached a safe home directory path.
pub static SAFE_HOME: OnceLock<String> = OnceLock::new();

/// Retrieves the safe home directory from the environment.
///
/// # Returns
/// - A string slice representing the user's home directory.
pub fn get_safe_home() -> &'static str {
    SAFE_HOME.get_or_init(|| env::var("HOME").unwrap_or_else(|_| ".".to_string()))
}

/// Retrieves the current application name from the execution path.
///
/// # Returns
/// - A string slice containing the binary name or "ALPack" as fallback.
pub fn get_app_name() -> &'static str {
    APP_NAME.get_or_init(|| {
        env::args()
            .next()
            .as_deref()
            .and_then(|s| s.rsplit('/').next())
            .unwrap_or("ALPack")
            .to_string()
    })
}

/// Determines the target architecture string.
///
/// # Returns
/// - A string representing the CPU architecture (e.g., "x86_64").
pub fn get_arch() -> String {
    env::var("ALPACK_ARCH")
        .or_else(|_| env::var("ARCH"))
        .unwrap_or_else(|_| env::consts::ARCH.to_string())
}

/// Displays a success message upon completing the environment setup.
pub fn finish_msg_setup() {
    let b = get_cmd_box(&format!("$ {} run", SAFE_HOME.wait()), Some(2), None).unwrap_or_default();

    println!(
        "{s}\n  Installation completed successfully!\n\n  To start the environment, run:\n\n{b}\n{s}",
        s = SEPARATOR,
    );
}

/// Verifies that the specified rootfs directory exists and is accessible.
///
/// # Parameters
/// - `path`: The directory path to verify.
///
/// # Returns
/// - `Ok(())` if the directory exists.
/// - `Err` with diagnostic info if missing.
pub fn check_rootfs_exists(path: &str) -> Result<(), Box<dyn Error>> {
    if !fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false) {
        let b = get_cmd_box(&format!("$ {} setup", APP_NAME.wait()), Some(2), None)?;

        return Err(format!(
            "{s}\n  Error: rootfs directory not found.\n\n  Expected location:\n    -> {p}\n\n  Please run the following command to set it up:\n{b}\n{s}",
            s = SEPARATOR,
            p = path,
        ).into());
    }
    Ok(())
}

/// Generates a stylized Unicode box containing a command string.
///
/// # Parameters
/// - `command`: The text to be boxed.
/// - `indent`: Optional number of leading spaces.
/// - `size`: Optional fixed width for the box.
///
/// # Returns
/// - A `String` containing the formatted box.
pub fn get_cmd_box(
    command: &str,
    indent: Option<usize>,
    size: Option<usize>,
) -> Result<String, Box<dyn Error>> {
    let padding = " ".repeat(indent.unwrap_or(0));
    let width = size.unwrap_or(50).max(command.len() + 4);
    let inner_width = width - 2;

    let line = "═".repeat(inner_width);
    let top = format!("{}╔{}╗", padding, line);
    let bottom = format!("{}╚{}╝", padding, line);

    let trailing_spaces = " ".repeat(inner_width - command.len() - 1);
    let middle = format!("{}║ {}{}║", padding, command, trailing_spaces);

    Ok(format!("{}\n{}\n{}", top, middle, bottom))
}

/// Recursively copies a directory and all its contents to a specified destination.
///
/// # Arguments
/// * `src` - The source directory to copy.
/// * `dst` - The destination directory where the source will be copied.
///
/// # Returns
/// * `io::Result<()>` - Ok on success, or an error if the operation fails.
pub fn copy_dir_recursive(src: &str, dst: &str) -> io::Result<()> {
    println!("copy {src} to {dst}");
    let dir_name = src.trim_end_matches('/')
        .rsplit('/')
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Invalid source path"))?;
    let dest_root = concat_path!(dst, dir_name);

    for entry in WalkDir::new(src)? {
        let entry = entry.map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        let relative = entry
            .path()
            .strip_prefix(src)
            .unwrap()
            .to_str()
            .unwrap_or("");
        let dest_path = concat_path!(dest_root, relative);

        if entry.file_type()?.is_dir() {
            fs::create_dir_all(&dest_path)?;
        } else {
            if let Some(pos) = dest_path.rfind('/') {
                let _ = fs::create_dir_all(&dest_path[..pos]);
            }
            fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}

/// Attempts to create the target directory, falling back to a default path if permission is denied.
///
/// # Parameters
/// - `target`: The desired path to create.
///
/// # Returns
/// - `Ok(PathBuf)` with the successfully created directory path (either the target or fallback).
/// - `Err(io::Error)` if both the target and fallback directory creations fail.
pub fn create_dir_with_fallback(target: &str) -> io::Result<String> {
    match fs::create_dir_all(target) {
        Ok(_) => Ok(target.to_string()),
        Err(ref e) if e.kind() == io::ErrorKind::PermissionDenied => {
            eprintln!(
                "\x1b[1;33mWarning\x1b[0m: Permission denied to create '{target}', using default directory instead...",
            );
            let path = Settings::load().set_rootfs();
            fs::create_dir_all(&path)?;
            Ok(path)
        }
        Err(e) => Err(e),
    }
}

/// Downloads a file from the specified URL and saves it to the destination folder.
///
/// # Arguments
/// * `url` - The URL of the file to be downloaded.
/// * `dest` - The directory where the file will be saved.
/// * `filename` - The name of the file to save.
///
/// # Returns
/// * `Ok(String)` - The full path of the saved file.
/// * `Err`: An `io::Error` if the download or save fails.
pub fn download_file(url: &str, dest: &str, filename: &str) -> io::Result<String> {
    let save_dest = create_dir_with_fallback(dest)?;
    let save_file = concat_path!(&save_dest, filename);

    if fs::metadata(&save_file).is_ok() {
        println!("File '{}' already exists, skipping download.", filename);
        return Ok(save_dest);
    }

    println!("Saving file to: {save_file}");
    let resp = ureq::get(url)
        .call()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    let length = resp
        .headers()
        .get("Content-Length")
        .unwrap()
        .to_str()
        .unwrap()
        .parse()
        .unwrap();

    let bar = ProgressBar::new(length);
    bar.set_message("Downloading...");
    bar.set_style(
        ProgressStyle::with_template(DOWNLOAD_TEMPLATE)
            .unwrap()
            .progress_chars("##-"),
    );

    io::copy(
        &mut bar.wrap_read(resp.into_body().into_reader()),
        &mut File::create(save_file)?,
    )?;
    bar.finish_with_message("Downloaded!");
    Ok(save_dest)
}

/// Sets executable permissions on a file (Unix-only).
///
/// # Arguments
/// * `path` - Path to the file whose permissions will be modified.
///
/// # Returns
/// * `Ok(())` if permissions were successfully updated.
/// * `Err(io::Error)` if the file metadata cannot be read or permissions cannot be set.
fn make_executable(path: &str) -> io::Result<()> {
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)
}

/// Returns the download URL for a supported rootfs command binary.
///
/// # Arguments
/// * `cmd` - The rootfs command name (e.g. `"proot"` or `"bwrap"`).
///
/// # Returns
/// * `Some(&'static str)` containing the download URL if the command
///   is supported.
/// * `None` if the command is unknown or unsupported.
fn binary_url(cmd: &str) -> Option<&'static str> {
    match cmd {
        "proot" => Some("https://github.com/LinuxDicasPro/StaticHub/releases/download/proot/proot"),
        "bwrap" => Some("https://github.com/LinuxDicasPro/StaticHub/releases/download/bwrap/bwrap"),
        _ => None,
    }
}

/// Verifies the availability of the specified rootfs command and downloads it if necessary.
/// Only x86_64 architecture is supported for automatic downloads. On other
/// architectures, the command must already be available in the system.
///
/// # Arguments
/// * `cmd_rootfs` - The name of the rootfs command (`"proot"` or `"bwrap"`).
///
/// # Returns
/// * `Ok(PathBuf)` - The full path to the resolved executable.
/// * `Err(io::Error)` if the command is unsupported, the architecture is not supported,
///   the download fails or file permissions cannot be set.
pub fn verify_and_download_rootfs_command(cmd_rootfs: &str) -> io::Result<String> {
    if let Some(path) = which(cmd_rootfs).ok() {
        return Ok(path.to_str().unwrap_or(cmd_rootfs).to_string());
    }

    let local_dir = concat_path!(get_safe_home(), ".local/bin");
    let local_path = concat_path!(local_dir, cmd_rootfs);

    if fs::metadata(&local_path).is_ok() {
        return Ok(local_path);
    }

    if env::consts::ARCH != "x86_64" {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!(
                "{cmd_rootfs} not found in the system and no binary is available for this architecture",
            ),
        ));
    }

    let url = binary_url(cmd_rootfs)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid cmd_rootfs"))?;

    let _ = fs::create_dir_all(&local_dir)?;

    let downloaded = download_file(url, &local_dir, cmd_rootfs)?;
    make_executable(&downloaded)?;

    Ok(downloaded)
}
