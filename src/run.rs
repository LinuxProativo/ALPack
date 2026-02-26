//! Execution handler for isolated processes.
//!
//! This module parses flags for the `run` subcommand, allowing users to
//! override the rootfs path, inject custom bind mounts, and define the
//! command to be executed within the sandbox.

use crate::settings::settings_rootfs_dir;
use crate::utils::map_result;
use crate::{invalid_arg, parse_key_value};

use sandbox_utils::{SandBox, SandBoxConfig};
use std::collections::VecDeque;
use std::error::Error;

/// Manager for the `run` subcommand execution.
pub struct Run {
    /// Arguments captured after the `run` keyword.
    remaining_args: Vec<String>,
}

impl Run {
    /// Creates a new `Run` instance with the provided arguments.
    pub fn new(remaining_args: Vec<String>) -> Self {
        Run { remaining_args }
    }

    /// Orchestrates the parsing of arguments and triggers the command execution.
    ///
    /// It handles specific flags like `--root`, `--bind-args`, and `--command`.
    /// If no command is provided, it defaults to the shell defined in the `Command` module.
    ///
    /// # Returns
    /// * `Ok(())` - If the command was executed successfully.
    /// * `Err` - If an invalid argument is found or the execution fails.
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        let mut rootfs = settings_rootfs_dir();
        let mut args: VecDeque<&str> = self.remaining_args.iter().map(|s| s.as_str()).collect();

        let mut cmd_args = Vec::new();
        let mut args_bind = String::new();
        let (mut use_root, mut ignore_extra_bind, mut no_group) = (false, false, false);

        while let Some(arg) = args.pop_front() {
            match arg {
                "-0" | "--root" => use_root = true,
                "-i" | "--ignore-extra-binds" => ignore_extra_bind = true,
                "-n" | "--no-groups" => no_group = true,
                a if a.starts_with("--bind-args=") => {
                    args_bind = parse_key_value!("run", "parameters", arg)?;
                }
                "-b" | "--bind-args" => {
                    args_bind = parse_key_value!("run", "parameters", arg, args.pop_front())?;
                }
                a if a.starts_with("--command=") => {
                    cmd_args.push(parse_key_value!("run", "command", arg)?);
                }
                "-c" | "--command" => {
                    cmd_args.push(parse_key_value!("run", "command", arg, args.pop_front())?);
                }
                a if a.starts_with("--rootfs=") => {
                    rootfs = parse_key_value!("run", "directory", arg)?.into();
                }
                "-R" | "--rootfs" => {
                    rootfs = parse_key_value!("run", "directory", arg, args.pop_front())?.into();
                }
                "--" => {
                    cmd_args.extend(args.drain(..).map(|s| s.to_string()));
                    break;
                }
                a if a.starts_with('-') => return invalid_arg!("run", arg),
                _ => {
                    cmd_args.push(arg.to_string());
                    cmd_args.extend(args.drain(..).map(|s| s.to_string()));
                    break;
                }
            }
        }

        let run_cmd = if cmd_args.is_empty() {
            String::new()
        } else {
            cmd_args.join(" ")
        };

        let config = SandBoxConfig {
            rootfs,
            run_cmd,
            args_bind,
            use_root,
            ignore_extra_bind,
            no_group,
            ..Default::default()
        };

        map_result(SandBox::run(config))?;
        Ok(())
    }
}
