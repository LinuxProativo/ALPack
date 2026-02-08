//! Package builder module for ALPack.
//!
//! Handles the automation of `abuild` inside the rootfs, including
//! key generation, dependency installation, and package compilation.
//! It supports building from directories (contextual builds) or
//! standalone APKBUILD files.

use crate::command::Command;
use crate::settings::Settings;
use crate::setup::DEF_PACKAGES;
use crate::{concat_path, invalid_arg, missing_arg, parse_key_value, utils};

use std::collections::VecDeque;
use std::error::Error;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::{env, fs};

/// Controller for automated Alpine Linux package compilation.
pub struct Builder<'a> {
    /// The name of the current execution context.
    name: &'a str,
    /// Arguments passed from the CLI for processing.
    remaining_args: Vec<String>,
}

impl<'a> Builder<'a> {
    /// Creates a new `Builder` instance with the given context and arguments.
    pub fn new(name: &'a str, remaining_args: Vec<String>) -> Self {
        Builder {
            name,
            remaining_args,
        }
    }

    /// Executes the builder command logic based on the provided arguments.
    ///
    /// Identifies build targets, prepares the internal rootfs `/build`
    /// directory, and manages the lifecycle of the `abuild` toolchain.
    ///
    /// # Performance
    /// - Minimizes syscalls by using `File::open` for path validation.
    /// - Uses string manipulation for folder identification to avoid `Path` overhead.
    ///
    /// # Returns
    /// - `Ok(())` on success.
    /// - `Err` if any operation fails, including compilation or filesystem errors.
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        let mut args: VecDeque<&str> = self.remaining_args.iter().map(|s| s.as_str()).collect();

        if args.is_empty() {
            return missing_arg!(self.name, "builder");
        }

        let mut build_targets = Vec::new();
        let sett = Settings::load_or_create();
        let mut rootfs_dir = sett.set_rootfs();

        while let Some(arg) = args.pop_front() {
            match arg {
                a if a.starts_with("--rootfs=") => {
                    rootfs_dir = parse_key_value!("builder", "directory", arg)?;
                }
                "-R" | "--rootfs" => {
                    rootfs_dir = parse_key_value!("builder", "directory", arg, args.pop_front())?;
                }
                "-a" | "--apkbuild" | "--apkbuild=" => {
                    let arg_ref = arg;

                    if arg_ref.contains('=') {
                        build_targets.push(parse_key_value!("builder", "apkbuild", arg)?);
                    } else {
                        let first = parse_key_value!("builder", "apkbuild", arg, args.pop_front())?;
                        build_targets.push(first);
                    }

                    build_targets.extend(args.drain(..).map(|s| s.to_string()));
                    break;
                }
                _ => return invalid_arg!(self.name, "builder", arg),
            }
        }

        for p in build_targets {
            let potential_path = concat_path!(&p, "APKBUILD");
            let pkg_name: String;
            let is_single_file: bool;
            let folder_name: &str;

            if File::open(&potential_path).is_ok() {
                pkg_name = Self::get_pkgname(&potential_path);
                folder_name = p.split('/').last().unwrap_or("unknown");
                is_single_file = false;
            } else if p.ends_with("APKBUILD") && File::open(&p).is_ok() {
                pkg_name = Self::get_pkgname(&p);
                folder_name = &pkg_name;
                is_single_file = true;
            } else {
                eprintln!(
                    "\x1b[1;33mWarning\x1b[0m: Target {} is not a valid APKBUILD or directory",
                    p
                );
                continue;
            }

            if pkg_name.is_empty() {
                eprintln!("\x1b[1;31mError\x1b[0m: pkgname not found in target {}", p);
                continue;
            }

            let build_path = concat_path!(rootfs_dir, "build");

            if is_single_file {
                let dest_file = concat_path!(&build_path, folder_name);
                fs::create_dir_all(&dest_file)?;
                fs::copy(&p, concat_path!(&dest_file, "APKBUILD"))?;
            } else {
                utils::copy_dir_recursive(p.as_ref(), build_path.as_ref())?;
            }

            Self::run_abuild(&rootfs_dir, folder_name, &pkg_name)?;
        }

        Ok(())
    }

    /// Extracts the `pkgname` value from an APKBUILD file.
    ///
    /// # Arguments
    /// * `path` - The path to the APKBUILD file.
    ///
    /// # Returns
    /// * `String` - The package name found in the file.
    fn get_pkgname(path: &str) -> String {
        File::open(path)
            .ok()
            .and_then(|file| {
                let reader = BufReader::new(file);
                for line in reader.lines().filter_map(Result::ok) {
                    if line.starts_with("pkgname=") {
                        let name = line
                            .trim_start_matches("pkgname=")
                            .trim()
                            .trim_matches('"')
                            .trim_matches('\'');
                        return Some(name.to_string());
                    }
                }
                None
            })
            .unwrap_or_default()
    }

    /// Orchestrates the `abuild` process inside the rootfs.
    ///
    /// Handles key generation, environment setup, and automated
    /// installation of the compiled package.
    ///
    /// # Arguments
    /// * `rootfs` - Path to the root filesystem.
    /// * `dir_name` - The subdirectory name for the build context.
    /// * `pkg` - The package name for final APK installation.
    ///
    /// # Returns
    /// * `Ok(())` - If the `abuild` command executes successfully.
    /// * `Err` - If there is any error during execution, return a boxed `dyn Error`.
    fn run_abuild(rootfs: &str, dir_name: &str, pkg: &str) -> Result<(), Box<dyn Error>> {
        let user = env::var("USER").unwrap_or_else(|_| "root".into());
        let keys_dir = concat_path!(rootfs, "etc/apk/keys");

        let has_keys = fs::read_dir(&keys_dir)
            .map(|mut entries| {
                entries.any(|e| {
                    e.ok().map_or(false, |en| {
                        en.file_name().to_string_lossy().ends_with(".rsa.pub")
                    })
                })
            })
            .unwrap_or(false);

        if !has_keys {
            let abuild_config = concat_path!(rootfs, "build/.abuild");
            if fs::metadata(&abuild_config).is_ok() {
                fs::remove_dir_all(&abuild_config)?;
            }

            let setup_cmd = format!(
                "type abuild > /dev/null 2>&1 || apk add {DEF_PACKAGES}
                HOME=/build
                abuild-keygen -a -n && \
                cp -v /build/.abuild/{user}*.rsa.pub /etc/apk/keys/",
            );

            Command::run(rootfs, None, Some(setup_cmd), false, false, false)?;
        }

        let cmd = format!(
            "type abuild > /dev/null || apk add {DEF_PACKAGES}
            HOME=/build
            cd /build/{dir_name}
            abuild -r -F && \
            find \"/build/packages/build/{u}\" -name \"{pkg}-*.apk\" -exec apk add --allow-untrusted {{}} \\;",
            u = utils::get_arch());

        Command::run(rootfs, None, Some(cmd), true, true, true)?;

        Ok(())
    }
}
