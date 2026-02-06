use crate::command::Command;
use crate::settings::Settings;
use crate::setup::DEF_PACKAGES;
use crate::{parse_key_value, utils};

use std::collections::VecDeque;
use std::error::Error;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::{env, fs};

pub struct Builder<'a> {
    name: &'a str,
    remaining_args: Vec<String>,
}

impl<'a> Builder<'a> {
    pub fn new(name: &'a str, remaining_args: Vec<String>) -> Self {
        Builder {
            name,
            remaining_args,
        }
    }

    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        let mut args: VecDeque<_> = self.remaining_args.clone().into();
        if args.is_empty() {
            return Err(format!(
                "{c}: builder: no parameter specified\nUse '{c} --help' to see available options.",
                c = self.name
            )
            .into());
        }

        let mut cmd_args = Vec::new();
        let mut concat_args = Vec::new();
        let mut apkbuild_file = String::new();

        let sett = Settings::load_or_create();
        let mut rootfs_dir = sett.set_rootfs();

        while let Some(arg) = args.pop_front() {
            match arg.as_str() {
                a if a.starts_with("--rootfs=") => {
                    rootfs_dir = parse_key_value!("builder", "directory", arg)?.unwrap();
                }
                "-R" | "--rootfs" => {
                    rootfs_dir = parse_key_value!(
                        "builder",
                        "directory",
                        arg,
                        args.pop_front().unwrap_or_default()
                    )?
                    .unwrap();
                }
                a if a.starts_with("--apkbuild=") => {
                    apkbuild_file = parse_key_value!("builder", "apkbuild", arg)?.unwrap();
                }
                "-a" | "--apkbuild" => {
                    apkbuild_file = parse_key_value!(
                        "builder",
                        "apkbuild",
                        arg,
                        args.pop_front().unwrap_or_default()
                    )?
                    .unwrap();
                }
                _ => {
                    cmd_args.push(arg);
                    cmd_args.extend(args.drain(..));
                    break;
                }
            }
        }

        if !apkbuild_file.is_empty() {
            let file_path = Path::new(&apkbuild_file);
            if file_path.exists() {
                if file_path.file_name().and_then(|n| n.to_str()) == Some("APKBUILD") {
                    let dir_name = Self::get_pkgname(apkbuild_file.as_str());
                    let dest_dir = format!("{}/build/{}", rootfs_dir, dir_name);

                    let build_dir = Path::new(&dest_dir);
                    fs::create_dir_all(build_dir)?;

                    let dest_file = build_dir.join("APKBUILD");
                    fs::copy(apkbuild_file.clone(), &dest_file)?;

                    Self::run_abuild(&rootfs_dir, dir_name)?;
                } else if file_path.is_dir() {
                    concat_args.push(apkbuild_file);
                } else {
                    eprintln!(
                        "\x1b[1;33mWarning\x1b[0m: Invalid file: {}, expected 'APKBUILD'",
                        apkbuild_file
                    );
                }
            } else {
                eprintln!(
                    "\x1b[1;33mWarning\x1b[0m: File not found: {}",
                    apkbuild_file
                );
            }
        }

        concat_args.extend(cmd_args);

        for p in concat_args {
            let path = Path::new(&p);
            let (pkg_name, mut dir_name): (String, String);
            let mut copy_only_apkbuild = false;

            if path.is_dir() {
                let apkbuild = path.join("APKBUILD");
                if !apkbuild.is_file() {
                    eprintln!("\x1b[1;33mWarning\x1b[0m: APKBUILD not found in: {}", p);
                    continue;
                }
                pkg_name = apkbuild.display().to_string();
                dir_name = path.display().to_string();
            } else if path.is_file() {
                if path.file_name().and_then(|n| n.to_str()) != Some("APKBUILD") {
                    eprintln!(
                        "\x1b[1;33mWarning\x1b[0m: Invalid file: {}, expected 'APKBUILD'",
                        p
                    );
                    continue;
                }
                pkg_name = path.display().to_string();
                dir_name = path
                    .parent()
                    .map(|p| p.display().to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| ".".to_string());
            } else {
                eprintln!("\x1b[1;33mWarning\x1b[0m: Invalid path: {}", p);
                continue;
            }

            if dir_name.clone().eq_ignore_ascii_case(".") {
                copy_only_apkbuild = true;
                let file = File::open(path)?;
                let reader = BufReader::new(file);

                for line in reader.lines() {
                    let line = line.unwrap_or_default();
                    if line.starts_with("pkgname=") {
                        dir_name = line.trim_start_matches("pkgname=").trim().to_string();
                        break;
                    }
                }
            }

            let folder_name = Path::new(&dir_name)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&dir_name);

            let dest_dir = format!("{}/build/", rootfs_dir);
            let build_dir = Path::new(&dest_dir);
            fs::create_dir_all(build_dir)?;

            if copy_only_apkbuild {
                let dest_file = build_dir.join("APKBUILD");
                fs::copy(pkg_name.clone(), &dest_file)?;
            } else {
                utils::copy_dir_recursive(dir_name.as_ref(), build_dir)?;
            }

            Self::run_abuild(&rootfs_dir, folder_name.to_string())?;
        }

        Ok(())
    }

    /// Retrieves the package name from a PKGBUILD-like file.
    ///
    /// # Arguments
    /// * `path` - The path to the PKGBUILD file (or any file containing `pkgname=`).
    ///
    /// # Returns
    /// * `String` - The package name found in the file.
    ///
    /// # Examples
    /// ```
    /// let pkgname = get_pkgname("PKGBUILD");
    /// println!("Package name: {}", pkgname);
    /// ```
    fn get_pkgname(path: &str) -> String {
        let file = File::open(path);
        if let Ok(file) = file {
            let reader = BufReader::new(file);
            for line in reader.lines() {
                let line = line.unwrap_or_default();
                if line.starts_with("pkgname=") {
                    return line.trim_start_matches("pkgname=").trim().to_string();
                }
            }
        }
        String::new()
    }

    /// Executes the `abuild` command inside the specified root filesystem and directory.
    ///
    /// # Arguments
    /// * `rootfs` - The path to the root filesystem where `abuild` should be executed.
    /// * `dir_name` - The directory containing the PKGBUILD or source to build.
    ///
    /// # Returns
    /// * `Ok(())` - If the `abuild` command executes successfully.
    /// * `Err` - If there is any error during execution, return a boxed `dyn Error`.
    ///
    /// # Examples
    /// ```no_run
    /// run_abuild("/path/to/rootfs".to_string(), "/path/to/srcdir".to_string())?;
    /// println!("Build completed successfully");
    /// ```
    fn run_abuild(rootfs: &str, dir_name: String) -> Result<(), Box<dyn Error>> {
        let cmd = format!(
            "
            type abuild > /dev/null || apk add {a}
            HOME=/build
            test -f /etc/apk/keys/{u}*.rsa.pub && exit
            rm -rf /build/.abuild
            mkdir -p /build
            abuild-keygen -a -n
            cp -v /build/.abuild/{u}*.rsa.pub /etc/apk/keys/
            ",
            u = env::var("USER").unwrap(),
            a = DEF_PACKAGES
        );

        Command::run(rootfs, None, Some(cmd), false, false, false)?;

        let cmd = format!("
            HOME=/build
            cd /build/{dir_name}
            abuild -r -F
            find \"/build/packages/build/{u}\" -name \"$apkbuild_name\"*.apk -exec apk add --allow-untrusted {{}} \\;
        ", u = utils::get_arch()); // todo: package name for install apk

        Command::run(rootfs, None, Some(cmd), true, true, true)?;

        Ok(())
    }
}
