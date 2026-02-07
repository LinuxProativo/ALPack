use crate::parse_key_value;
use crate::settings::Settings;

use std::collections::VecDeque;
use std::error::Error;

pub struct Config<'a> {
    name: &'a str,
    remaining_args: Vec<String>,
}

impl<'a> Config<'a> {
    pub fn new(name: &'a str, remaining_args: Vec<String>) -> Self {
        Config {
            name,
            remaining_args,
        }
    }

    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        let mut args: VecDeque<_> = self.remaining_args.clone().into();
        let mut sett = Settings::load_or_create();

        while let Some(arg) = args.pop_front() {
            match arg.as_str() {
                "--use-proot" => {
                    sett.cmd_rootfs = "proot".to_string();
                },
                "--use-bwrap" => {
                    sett.cmd_rootfs = "bwrap".to_string();
                },
                "--use-latest-stable" => {
                    sett.release = "latest-stable".to_string();
                },
                "--use-edge" => {
                    sett.release = "edge".to_string();
                },
                a if a.starts_with("--cache-dir=") => {
                    sett.cache_dir = parse_key_value!("config", "directory", arg)?;
                }
                "--cache-dir" => {
                    sett.cache_dir = parse_key_value!("config", "directory", arg, Some(args.pop_front().unwrap_or_default()))?;
                },
                a if a.starts_with("--rootfs-dir=") => {
                    sett.rootfs_dir = parse_key_value!("config", "directory", arg)?;
                }
                "--rootfs-dir" => {
                    sett.rootfs_dir = parse_key_value!("config", "directory", arg, Some(args.pop_front().unwrap_or_default()))?;
                },
                a if a.starts_with("--output-dir=") => {
                    sett.rootfs_dir = parse_key_value!("config", "directory", arg)?;
                }
                "--output-dir" => {
                    sett.rootfs_dir = parse_key_value!("config", "directory", arg, Some(args.pop_front().unwrap_or_default()))?;
                },
                a if a.starts_with("--default-mirror=") => {
                    sett.default_mirror = parse_key_value!("config", "mirror", arg)?;
                }
                "--default-mirror" => {
                    sett.default_mirror = parse_key_value!("config", "mirror", arg, Some(args.pop_front().unwrap_or_default()))?;
                },
                _ => {
                    return Err(format!("{c}: aports: invalid argument '{arg}'\nUse '{c} --help' to see available options.", c = self.name).into())
                }
            }
        }

        sett.show_config_changes();
        if !self.remaining_args.is_empty() {
            sett.save()?;
        }
        Ok(())
    }
}
