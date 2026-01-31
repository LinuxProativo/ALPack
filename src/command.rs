use crate::settings::Settings;
use crate::utils;

use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, Stdio};
use std::{env, fs, io};

pub struct Command;

impl Command {
    pub fn run(
        rootfs: String,
        args_bind: Option<String>, cmd: Option<String>,
        use_root: bool, ignore_extra_bind: bool, no_group: bool,
    ) -> Result<i32, Box<dyn std::error::Error>> {
        let sett = Settings::load_or_create();
        let name = env::current_exe()?.file_name().unwrap().to_str().unwrap().to_string();
        utils::check_rootfs_exists(name, rootfs.clone())?;

        let comm = sett.cmd_rootfs;
        let rootfs_cmd = utils::verify_and_download_rootfs_command(&comm)?;

        let args = match comm.as_str() {
            "proot" => Self::build_proot_options(rootfs, args_bind.unwrap_or_default(), ignore_extra_bind, no_group),
            "bwrap" => Self::build_bwrap_options(rootfs, args_bind.unwrap_or_default(), ignore_extra_bind, no_group),
            other => return Err(format!("Unsupported rootfs command: {}", other).into()),
        };

        let new_cmd = cmd.unwrap_or_default();
        let mut full_args: Vec<&str> = args.split_whitespace().collect();

        let uid = Self::get_uid_from_passwd();
        let str = match (comm.as_str(), use_root) {
            ("proot", true) => "PS1=# |USER=root|LOGNAME=root|UID=0|EUID=0".to_string(),
            ("proot", false) => format!("PS1=$ |UID={uid}|EUID={uid}"),
            ("bwrap", true) => "PS1=# ".to_string(),
            ("bwrap", false) => format!("PS1=$ |UID={uid}|EUID={uid}"),
            _ => format!("PS1=$ |UID={uid}|EUID={uid}"),
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
            "/bin/sh"
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

    /// Builds the PRoot command-line options string.
    ///
    /// # Parameters
    /// - `rootfs`: Path to the root filesystem to be used with PRoot.
    /// - `rootfs_args`: Additional arguments to append to the PRoot command.
    /// - `no_extra_binds`: If true, skip adding optional binds like fonts and icons.
    ///
    /// # Returns
    /// * `String` - A full string of PRoot options to be passed to the command.
    ///
    /// # Example
    /// ```
    /// let opts = build_proot_options("/my/rootfs".into(), "--cwd=/home/user".into(), false);
    /// println!("Proot options: {}", opts);
    /// ```
    fn build_proot_options(rootfs: String, rootfs_args: String, no_extra_binds: bool, no_group: bool) -> String {
        let mut proot_options = format!("-R {rootfs} --bind=/media --bind=/mnt {rootfs_args}");

        if no_group {
            proot_options.push_str(format!(
                " --bind={rootfs}/etc/group:/etc/group \
                  --bind={rootfs}/etc/passwd:/etc/passwd").as_str()
            );
        }

        if !no_extra_binds {
            if Path::new("/etc/asound.conf").exists() {
                proot_options.push_str(" --bind=/etc/asound.conf");
            }
            if Path::new("/etc/fonts").exists() {
                proot_options.push_str(" --bind=/etc/fonts");
            }
            if Path::new("/usr/share/font-config").exists() {
                proot_options.push_str(" --bind=/usr/share/font-config");
            }
            if Path::new("/usr/share/fontconfig").exists() {
                proot_options.push_str(" --bind=/usr/share/fontconfig");
            }
            if Path::new("/usr/share/fonts").exists() {
                proot_options.push_str(" --bind=/usr/share/fonts");
            }
            if Path::new("/usr/share/themes").exists() {
                proot_options.push_str(" --bind=/usr/share/themes");
            }

            if let Ok(entries) = fs::read_dir("/usr/share/icons") {
                for entry in entries.flatten() {
                    let path = entry.path().join("cursors");
                    if path.is_dir() {
                        if let Some(dir_str) = path.to_str() {
                            proot_options.push_str(" --bind=");
                            proot_options.push_str(dir_str);
                        }
                    }
                }
            }
        }

        proot_options
    }

    /// Builds the command-line options for running a program inside Bubblewrap.
    ///
    /// This function generates a set of `--bind` and `--ro-bind` options for `bwrap`,
    /// based on the provided root filesystem, extra bind arguments, and whether
    /// to include additional system paths.
    ///
    /// # Parameters
    /// - `rootfs`: Path to the root filesystem.
    /// - `rootfs_args`: Additional bind arguments passed as a string.
    /// - `ignore_extra_binds`: If `true`, skip adding extra system binds.
    ///
    /// # Returns
    /// A `String` containing the constructed Bubblewrap options.
    ///
    /// # Example
    /// ```
    /// let opts = build_bwrap_options("/path/to/rootfs".to_string(), "".to_string(), false);
    /// println!("bwrap options: {}", opts);
    /// ```
    fn build_bwrap_options(rootfs: String, rootfs_args: String, ignore_extra_binds: bool, no_group: bool) -> String {

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
             --setenv PATH \"/bin:/sbin:/usr/bin:/usr/sbin:/usr/libexec\"", a = env::var("HOME").unwrap());

        if !no_group {
            bwrap_options.push_str(
                " --ro-bind-try /etc/passwd /etc/passwd \
                --ro-bind-try /etc/group /etc/group"
            );
        }

        Self::fix_mtab_symlink(Path::new(&rootfs.clone())).unwrap();

        if !ignore_extra_binds {
            if Path::new("/etc/asound.conf").exists() {
                bwrap_options.push_str(" --ro-bind /etc/asound.conf /etc/asound.conf");
            }
            if Path::new("/etc/fonts").exists() {
                bwrap_options.push_str(" --ro-bind /etc/fonts /etc/fonts");
            }
            if Path::new("/usr/share/font-config").exists() {
                bwrap_options.push_str(" --ro-bind /usr/share/font-config /usr/share/font-config");
            }
            if Path::new("/usr/share/fontconfig").exists() {
                bwrap_options.push_str(" --ro-bind /usr/share/fontconfig /usr/share/fontconfig");
            }
            if Path::new("/usr/share/fonts").exists() {
                bwrap_options.push_str(" --ro-bind /usr/share/fonts /usr/share/fonts");
            }
            if Path::new("/usr/share/themes").exists() {
                bwrap_options.push_str(" --ro-bind /usr/share/themes /usr/share/themes");
            }

            if let Ok(entries) = fs::read_dir("/usr/share/icons") {
                for entry in entries.flatten() {
                    let path = entry.path().join("cursors");
                    if path.is_dir() {
                        if let Some(dir_str) = path.to_str() {
                            bwrap_options.push_str(" --ro-bind ");
                            bwrap_options.push_str(dir_str);
                            bwrap_options.push(' ');
                            bwrap_options.push_str(dir_str);
                        }
                    }
                }
            }
        }

        bwrap_options
    }


    /// Attempts to retrieve the current user's UID by parsing `/etc/passwd`.
    ///
    /// # Returns
    /// * `u32` - The UID of the current user, or `1000` if not found.
    ///
    /// # Example
    /// ```
    /// let uid = get_uid_from_passwd();
    /// println!("Current UID: {}", uid);
    /// ```
    fn get_uid_from_passwd() -> u32 {
        let username = env::var("USER").or_else(|_| env::var("LOGNAME")).unwrap_or_default();
        let passwd = fs::read_to_string("/etc/passwd").unwrap_or_default();

        if username.is_empty() || passwd.is_empty() {
            eprintln!("\x1b[1;33mWarning\x1b[0m: UID could not be determined, using fallback UID: 1000");
            return 1000
        }

        passwd.lines()
            .find(|line| line.starts_with(&username))
            .and_then(|line| line.split(':').nth(2))
            .and_then(|uid| uid.parse::<u32>().ok()).unwrap_or(1000)
    }

    /// Ensures `/etc/mtab` inside the rootfs points to `/proc/self/mounts`.
    ///
    /// # Parameters
    /// - `rootfs`: Path to the root filesystem.
    ///
    /// # Example
    /// ```
    /// fix_mtab_symlink("/my/rootfs".to_string());
    /// ```
    pub fn fix_mtab_symlink(rootfs: &Path) -> io::Result<()> {
        use std::os::unix::fs::symlink;

        let mtab_path: PathBuf = rootfs.join("etc/mtab");
        let desired_target = Path::new("/proc/self/mounts");

        if let Some(parent) = mtab_path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                eprintln!("Warning: failed to create parent dir {:?}: {}", parent, e);
            }
        }

        match fs::symlink_metadata(&mtab_path) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    match fs::read_link(&mtab_path) {
                        Ok(target) => {
                            if target == desired_target {
                                return Ok(());
                            } else {
                                if let Err(e) = fs::remove_file(&mtab_path) {
                                    eprintln!("Warning: failed to remove existing symlink {:?}: {}", mtab_path, e);
                                }
                            }
                        }
                        Err(_) => {
                            if let Err(e) = fs::remove_file(&mtab_path) {
                                eprintln!("Warning: failed to remove broken symlink {:?}: {}", mtab_path, e);
                            }
                        }
                    }
                } else {
                    if let Err(e) = fs::remove_file(&mtab_path) {
                        eprintln!("Warning: failed to remove existing file {:?}: {}", mtab_path, e);
                    }
                }
            }
            Err(_) => {}
        }

        if let Err(e) = symlink(desired_target, &mtab_path) {
            eprintln!("Warning: failed to create symlink {:?} -> {:?}: {}", mtab_path, desired_target, e);
            return Err(e);
        }

        Ok(())
    }
}

