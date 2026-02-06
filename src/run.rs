use crate::command::Command;
use crate::parse_key_value;
use crate::settings::Settings;
use std::collections::VecDeque;
use std::error::Error;

pub struct Run<'a> {
    name: &'a str,
    remaining_args: Vec<String>,
}

impl<'a> Run<'a> {
    pub fn new(name: &'a str, remaining_args: Vec<String>) -> Self {
        Run {
            name,
            remaining_args,
        }
    }

    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        let sett = Settings::load_or_create();
        let mut rootfs_dir = sett.set_rootfs();
        let mut args: VecDeque<_> = self.remaining_args.clone().into();

        let mut cmd_args = Vec::new();
        let mut bind_args: Option<String> = None;
        let (mut use_root, mut ignore_extra_bind) = (false, false);

        while let Some(arg) = args.pop_front() {
            match arg.as_str() {
                "-0" | "--root" => {
                    use_root = true;
                },
                "-i" | "--ignore-extra-binds" => {
                    ignore_extra_bind = true;
                },
                a if a.starts_with("--bind-args=") => {
                    bind_args = parse_key_value!("run", "parameters", arg)?;
                }
                "-b" | "--bind-args" => {
                    bind_args = parse_key_value!("run", "parameters", arg, args.pop_front().unwrap_or_default())?;
                },
                a if a.starts_with("--command=") => {
                    let cmd = parse_key_value!("run", "command", arg)?;
                    cmd_args.push(cmd.unwrap());
                }
                "-c" | "--command" => {
                    let cmd = parse_key_value!("run", "command", arg, args.pop_front().unwrap_or_default())?;
                    cmd_args.push(cmd.unwrap());
                },
                a if a.starts_with("--rootfs=") => {
                    rootfs_dir = parse_key_value!("run", "directory", arg)?.unwrap();
                }
                "-R" | "--rootfs" => {
                    rootfs_dir = parse_key_value!("run", "directory", arg, args.pop_front().unwrap_or_default())?.unwrap();
                }
                "--" => {
                    cmd_args.extend(args.drain(..));
                    break;
                }
                a if a.starts_with('-') => {
                    return Err(format!("{c}: run: invalid argument '{arg}'\nUse '{c} --help' to see available options.", c = self.name).into())
                }
                _ => {
                    cmd_args.push(arg);
                    cmd_args.extend(args.drain(..));
                    break;
                }
            }
        }

        Command::run(
            rootfs_dir,
            bind_args,
            Some(cmd_args.join(" ")),
            use_root,
            ignore_extra_bind,
            false,
        )?;
        Ok(())
    }
}
