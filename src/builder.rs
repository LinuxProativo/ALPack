//! Package builder module for ALPack.
//!
//! Handles the automation of `abuild` inside the rootfs, including
//! key generation, dependency installation, and package compilation.
//! It supports building from directories (contextual builds) or
//! standalone APKBUILD files.

use crate::settings::{
    settings_overlay_action, settings_overlay_inode_mode, settings_rootfs_dir, settings_use_overlay,
};
use crate::setup::DEF_PACKAGES;
use crate::utils::map_result;
use recursive_copy::{copy_recursive, CopyOptions};
use sandbox_utils::{
    app_arch, invalid_arg, missing_arg, parse_value, OverlayAction, SandBox, SandBoxConfig,
};
use std::collections::VecDeque;
use std::error::Error;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::{env, fs};

/// Controller for automated Alpine Linux package compilation.
pub struct Builder {
    /// Arguments passed from the CLI for processing.
    remaining_args: Vec<String>,
}

impl Builder {
    /// Creates a new `Builder` instance with the given context and arguments.
    pub fn new(remaining_args: Vec<String>) -> Self {
        Builder { remaining_args }
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
            return missing_arg!("builder");
        }

        let mut build_targets = Vec::new();
        let mut rootfs_dir = settings_rootfs_dir();
        let mut force_key = false;
        let mut use_overlay = settings_use_overlay();
        let mut overlay_action = settings_overlay_action();

        while let Some(arg) = args.pop_front() {
            match arg {
                "--force-key" => force_key = true,
                "-e" | "--ephemeral" => {
                    use_overlay = true;
                    overlay_action = OverlayAction::Discard;
                }
                a if a.starts_with("--rootfs=") => {
                    rootfs_dir = parse_value!("builder", "directory", arg)?.into();
                }
                "-R" | "--rootfs" => {
                    rootfs_dir =
                        parse_value!("builder", "directory", arg, args.pop_front())?.into();
                }
                "-a" | "--apkbuild" | "--apkbuild=" => {
                    let arg_ref = arg;

                    if arg_ref.contains('=') {
                        build_targets.push(parse_value!("builder", "apkbuild", arg)?);
                    } else {
                        let first = parse_value!("builder", "apkbuild", arg, args.pop_front())?;
                        build_targets.push(first);
                    }

                    build_targets.extend(args.drain(..).map(|s| s.to_string()));
                    break;
                }
                _ => return invalid_arg!("builder", arg),
            }
        }

        for p in build_targets {
            let path = Path::new(&p);
            let potential_apkbuild = path.join("APKBUILD");

            let (pkg_name, folder_name, is_single_file, source_path) =
                if potential_apkbuild.exists() {
                    let name = Self::get_pkgname(&potential_apkbuild);
                    let folder = path
                        .file_name()
                        .and_then(|f| f.to_str())
                        .unwrap_or("unknown");
                    (name, folder.to_string(), false, path)
                } else if path.is_file() && p.ends_with("APKBUILD") {
                    let name = Self::get_pkgname(path);
                    (name.clone(), name, true, path)
                } else {
                    eprintln!(
                        "\x1b[1;33mWarning\x1b[0m: Target {} is not a valid APKBUILD or directory",
                        p
                    );
                    continue;
                };

            if pkg_name.is_empty() {
                eprintln!("\x1b[1;31mError\x1b[0m: pkgname not found in target {}", p);
                continue;
            }

            let build_path = rootfs_dir.join("build");
            let target_dir = build_path.join(&folder_name);

            if is_single_file {
                fs::create_dir_all(&target_dir)?;
                fs::copy(source_path, target_dir.join("APKBUILD"))?;
            } else {
                copy_recursive(source_path, &target_dir, &CopyOptions::default())?;
            }

            Self::run_abuild(
                rootfs_dir.clone(),
                &folder_name,
                &pkg_name,
                force_key,
                use_overlay,
                overlay_action,
            )?;
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
    fn get_pkgname<P: AsRef<Path>>(path: P) -> String {
        File::open(path)
            .ok()
            .and_then(|file| {
                BufReader::new(file)
                    .lines()
                    .filter_map(Result::ok)
                    .find(|line| line.starts_with("pkgname="))
                    .map(|line| {
                        line.split('=')
                            .nth(1)
                            .unwrap_or_default()
                            .trim_matches(|c| c == '"' || c == '\'' || c == ' ')
                            .to_string()
                    })
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
    /// * `force_key` - If true, regenerates RSA keys even if they exist.
    /// * `use_overlay` - If true, enable Overlay.
    /// * `overlay_action` - Set overlay action.
    ///
    /// # Returns
    /// * `Ok(())` - If the `abuild` command executes successfully.
    /// * `Err` - If there is any error during execution, return a boxed `dyn Error`.
    fn run_abuild(
        rootfs: PathBuf,
        dir_name: &str,
        pkg: &str,
        force_key: bool,
        use_overlay: bool,
        overlay_action: OverlayAction,
    ) -> Result<(), Box<dyn Error>> {
        let user = env::var("USER").unwrap_or_else(|_| "root".into());
        let build_dir = rootfs.join("build");
        let keys_dir = rootfs.join("rootfs/etc/apk/keys");

        let has_user_key = fs::read_dir(&keys_dir)
            .map(|entries| {
                entries.filter_map(Result::ok).any(|en| {
                    let binding = en.file_name();
                    let name = binding.to_string_lossy();
                    name.starts_with(&user) && name.ends_with(".rsa.pub")
                })
            })
            .unwrap_or(false);

        if force_key || !has_user_key {
            let abuild_config = build_dir.join(".abuild");
            if fs::metadata(&abuild_config).is_ok() {
                fs::remove_dir_all(&abuild_config)?;
            }

            let run_cmd = format!(
                "type abuild > /dev/null 2>&1 || apk add {DEF_PACKAGES}
                HOME={b}
                abuild-keygen -a -n && \
                cp -v {f} /etc/apk/keys",
                b = build_dir.display(),
                f = &abuild_config.join(format!("{user}*.rsa.pub")).display()
            );

            let config = SandBoxConfig {
                rootfs: rootfs.clone(),
                run_cmd,
                ..Default::default()
            };

            map_result(SandBox::run(config))?;
        }

        let run_cmd = format!(
            "type abuild > /dev/null || apk add {DEF_PACKAGES}
            HOME={b}
            cd {d}
            abuild -r -F && \
            find \"{f}\" -name \"{pkg}-*.apk\" -exec apk add --allow-untrusted {{}} \\;",
            b = build_dir.display(),
            d = build_dir.join(dir_name).display(),
            f = build_dir
                .join(format!("packages/build/{}", app_arch()))
                .display()
        );

        let config = SandBoxConfig {
            rootfs,
            run_cmd,
            use_root: true,
            secure_rootfs: true,
            use_overlay,
            action: overlay_action,
            inode_mode: settings_overlay_inode_mode(),
            ..Default::default()
        };

        map_result(SandBox::run(config))?;
        Ok(())
    }
}
