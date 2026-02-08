//! System command execution and isolation management.
//!
//! This module orchestrates process execution within isolated environments
//! using PRoot or Bubblewrap. It manages user identities via native syscalls,
//! handles filesystem bindings, and ensures rootfs environment integrity.

unsafe extern "C" {
    fn getuid() -> u32;
    fn geteuid() -> u32;
}

use crate::settings::Settings;
use crate::{concat_path, push_bind, utils};

use std::os::unix;
use std::process::{Command as StdCommand, Stdio};
use std::{env, fs};

/// Controller for isolated process execution.
pub struct Command;

impl Command {
    /// Executes a command within a specified rootfs using isolation tools.
    ///
    /// Dynamically selects between PRoot and Bubblewrap based on system settings,
    /// configures environment variables (PS1, PATH, UID), and manages
    /// container-to-host filesystem mapping.
    ///
    /// # Returns
    /// - `Ok(i32)` representing the process exit code.
    /// - `Err` if the isolation tool fails to launch or rootfs is invalid.
    pub fn run(
        rootfs: &str,
        args_bind: Option<String>,
        cmd: Option<String>,
        use_root: bool,
        ignore_extra_bind: bool,
        no_group: bool,
    ) -> Result<i32, Box<dyn std::error::Error>> {
        let sett = Settings::load_or_create();
        utils::check_rootfs_exists(rootfs)?;

        let comm = sett.cmd_rootfs;
        let rootfs_cmd = utils::verify_and_download_rootfs_command(&comm)?;

        let args = match comm.as_str() {
            "proot" => Self::build_proot_options(
                rootfs,
                args_bind.unwrap_or_default(),
                ignore_extra_bind,
                no_group,
            ),
            "bwrap" => Self::build_bwrap_options(
                rootfs,
                args_bind.unwrap_or_default(),
                ignore_extra_bind,
                no_group,
            ),
            other => return Err(format!("Unsupported rootfs command: {}", other).into()),
        };

        let new_cmd = cmd.unwrap_or_default();
        let mut full_args: Vec<&str> = args.split_whitespace().collect();

        let (uid, euid) = unsafe { (getuid(), geteuid()) };

        let str = match (comm.as_str(), use_root) {
            ("proot", true) => "PS1=# |USER=root|LOGNAME=root|UID=0|EUID=0".to_string(),
            ("proot", false) => format!("PS1=$ |UID={uid}|EUID={euid}"),
            ("bwrap", true) => "PS1=# ".to_string(),
            ("bwrap", false) => format!("PS1=$ |UID={uid}|EUID={euid}"),
            _ => format!("PS1=$ |UID={uid}|EUID={euid}"),
        };

        if comm == "proot" && use_root {
            full_args.push("-0");
        }

        if comm == "bwrap" && use_root {
            full_args.extend([
                "--uid", "0",
                "--gid", "0",
                "--setenv", "USER", "root",
                "--setenv", "LOGNAME", "root"
            ]);
        }

        full_args.push("env");
        full_args.extend_from_slice(&str.split('|').collect::<Vec<_>>());
        full_args.extend([
            "SHELL=/bin/sh",
            "PATH=/bin:/sbin:/usr/bin:/usr/sbin:/usr/libexec",
            "/bin/sh",
        ]);

        if !new_cmd.is_empty() {
            full_args.push("-c");
            full_args.push(&new_cmd);
        }

        let status = StdCommand::new(&rootfs_cmd)
            .args(&full_args)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()?;

        Ok(status.code().unwrap_or(-1))
    }

    /// Generates PRoot-specific configuration arguments.
    ///
    /// # Parameters
    /// * `rootfs` - Guest root directory.
    /// * `rootfs_args` - Raw bind string from user arguments.
    /// * `no_extra_binds` - Flag to skip optional host configurations.
    /// * `no_group` - Flag to skip mapping host identity files.
    ///
    /// # Returns
    /// A space-delimited `String` of PRoot CLI options.
    fn build_proot_options(
        rootfs: &str,
        rootfs_args: String,
        no_extra_binds: bool,
        no_group: bool,
    ) -> String {
        let mut proot_options = format!("-R {rootfs} --bind=/media --bind=/mnt {rootfs_args}");

        if no_group {
            proot_options.push_str(
                format!(
                    " --bind={rootfs}/etc/group:/etc/group --bind={rootfs}/etc/passwd:/etc/passwd"
                )
                .as_str(),
            );
        }

        if !no_extra_binds {
            let extra_paths = [
                "/etc/asound.conf",
                "/etc/fonts",
                "/usr/share/font-config",
                "/usr/share/fontconfig",
                "/usr/share/fonts",
                "/usr/share/themes",
            ];

            for path in extra_paths {
                if fs::metadata(path).is_ok() {
                    proot_options.push_str(" --bind=");
                    proot_options.push_str(path);
                }
            }

            if let Ok(entries) = fs::read_dir("/usr/share/icons") {
                for entry in entries.flatten() {
                    if let Ok(name) = entry.file_name().into_string() {
                        let cursor_path = concat_path!("/usr/share/icons", &name, "cursors");
                        if fs::metadata(&cursor_path).map(|m| m.is_dir()).unwrap_or(false) {
                            proot_options.push_str(" --bind=");
                            proot_options.push_str(&cursor_path);
                        }
                    }
                }
            }
        }

        proot_options
    }

    /// Generates Bubblewrap-specific configuration arguments.
    ///
    /// # Arguments
    /// * `rootfs` - Guest root directory.
    /// * `rootfs_args` - Raw bind string from user arguments.
    /// * `ignore_extra_binds` - Flag to skip optional host configurations.
    /// * `no_group` - Flag to skip mapping host identity files.
    ///
    /// # Returns
    /// A space-delimited `String` of Bubblewrap CLI options.
    fn build_bwrap_options(
        rootfs: &str,
        rootfs_args: String,
        ignore_extra_binds: bool,
        no_group: bool,
    ) -> String {
        let mut bwrap_options = format!(
            "--unshare-user \
             --share-net \
             --bind {rootfs} / \
             --die-with-parent \
             --ro-bind-try /etc/host.conf /etc/host.conf \
             --ro-bind-try /etc/hosts /etc/hosts \
             --ro-bind-try /etc/hosts.equiv /etc/hosts.equiv \
             --ro-bind-try /etc/netgroup /etc/netgroup \
             --ro-bind-try /etc/networks /etc/networks \
             --ro-bind-try /etc/nsswitch.conf /etc/nsswitch.conf \
             --ro-bind-try /etc/resolv.conf /etc/resolv.conf \
             --ro-bind-try /etc/localtime /etc/localtime \
             --dev-bind /dev /dev \
             --ro-bind /sys /sys \
             --bind-try /proc /proc \
             --bind-try /tmp /tmp \
             --bind-try /run /run \
             --ro-bind /var/run/dbus/system_bus_socket /var/run/dbus/system_bus_socket \
             --bind {a} {a} \
             --bind /media /media \
             --bind /mnt /mnt \
             {rootfs_args} \
             --setenv PATH \"/bin:/sbin:/usr/bin:/usr/sbin:/usr/libexec\"",
            a = env::var("HOME").unwrap_or_else(|_| "/root".into()),
        );

        if !no_group {
            bwrap_options.push_str(
                " --ro-bind-try /etc/passwd /etc/passwd --ro-bind-try /etc/group /etc/group",
            );
        }

        Self::fix_mtab_symlink(rootfs);

        if !ignore_extra_binds {
            let extra_paths = [
                "/etc/asound.conf",
                "/etc/fonts",
                "/usr/share/font-config",
                "/usr/share/fontconfig",
                "/usr/share/fonts",
                "/usr/share/themes",
            ];

            for path in extra_paths {
                if fs::metadata(path).is_ok() {
                    push_bind!(bwrap_options, "--ro-bind", path);
                }
            }

            if let Ok(entries) = fs::read_dir("/usr/share/icons") {
                for entry in entries.flatten() {
                    if let Ok(name) = entry.file_name().into_string() {
                        let cursor_path = concat_path!("/usr/share/icons", &name, "cursors");
                        if fs::metadata(&cursor_path)
                            .map(|m| m.is_dir())
                            .unwrap_or(false)
                        {
                            push_bind!(bwrap_options, "--ro-bind", &cursor_path);
                        }
                    }
                }
            }
        }

        bwrap_options
    }

    /// Ensures `/etc/mtab` inside the rootfs points to `/proc/self/mounts`.
    ///
    /// # Parameters
    /// - `rootfs`: Path to the root filesystem.
    fn fix_mtab_symlink(rootfs: &str) {
        let mtab_path = concat_path!(rootfs, "etc", "mtab");
        let target = "/proc/self/mounts";

        if let Ok(existing_target) = fs::read_link(&mtab_path) {
            if existing_target.to_string_lossy() == target {
                return;
            }
        }

        let _ = fs::remove_file(&mtab_path);
        let etc_dir = concat_path!(rootfs, "etc");

        if fs::create_dir_all(&etc_dir).is_ok() {
            if let Err(e) = unix::fs::symlink(target, &mtab_path) {
                eprintln!("\x1b[1;33mWarning\x1b[0m: Failed to fix mtab symlink: {}", e);
            }
        }
    }
}
