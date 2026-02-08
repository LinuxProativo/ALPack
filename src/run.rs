//! Execution handler for isolated processes.
//!
//! This module parses flags for the `run` subcommand, allowing users to
//! override the rootfs path, inject custom bind mounts, and define the
//! command to be executed within the sandbox.

use crate::command::Command;
use crate::settings::Settings;
use crate::{invalid_arg, parse_key_value};

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
        let sett = Settings::load();
        let mut rootfs_dir = sett.set_rootfs();
        let mut args: VecDeque<&str> = self.remaining_args.iter().map(|s| s.as_str()).collect();

        let mut cmd_args = Vec::new();
        let mut bind_args: Option<String> = None;
        let (mut use_root, mut ignore_extra_bind, mut no_groups) = (false, false, false);

        while let Some(arg) = args.pop_front() {
            match arg {
                "-0" | "--root" => use_root = true,
                "-i" | "--ignore-extra-binds" => ignore_extra_bind = true,
                "-n" | "--no-groups" => no_groups = true,
                a if a.starts_with("--bind-args=") => {
                    bind_args = Some(parse_key_value!("run", "parameters", arg)?);
                }
                "-b" | "--bind-args" => {
                    bind_args = Some(parse_key_value!(
                        "run",
                        "parameters",
                        arg,
                        args.pop_front()
                    )?);
                }
                a if a.starts_with("--command=") => {
                    cmd_args.push(parse_key_value!("run", "command", arg)?);
                }
                "-c" | "--command" => {
                    cmd_args.push(parse_key_value!("run", "command", arg, args.pop_front())?);
                }
                a if a.starts_with("--rootfs=") => {
                    rootfs_dir = parse_key_value!("run", "directory", arg)?;
                }
                "-R" | "--rootfs" => {
                    rootfs_dir = parse_key_value!("run", "directory", arg, args.pop_front())?;
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

        let final_cmd = (!cmd_args.is_empty()).then(|| cmd_args.join(" "));

        Command::run(
            &rootfs_dir,
            bind_args,
            final_cmd,
            use_root,
            ignore_extra_bind,
            no_groups,
        )?;
        Ok(())
    }
}
