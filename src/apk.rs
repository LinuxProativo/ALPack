use crate::command::Command;
use crate::settings::Settings;
use std::error::Error;
use crate::missing_arg;

pub struct Apk<'a> {
    name: &'a str,
    command: Option<String>,
    remaining_args: Vec<String>,
    rootfs: Option<String>,
}

impl<'a> Apk<'a> {
    pub fn new(
        name: &'a str,
        command: Option<String>,
        remaining_args: Vec<String>,
        rootfs: Option<String>,
    ) -> Self {
        Apk {
            name,
            command,
            remaining_args,
            rootfs,
        }
    }

    /// Orchestrates the execution of the Alpine Package Manager (apk).
    ///
    /// This method maps ALPack's internal commands and aliases to their
    /// corresponding `apk` operations. It ensures that any command passed
    /// is properly routed or returns a helpful error if none is specified.
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        match &self.command.as_deref() {
            Some("add") | Some("install") => self.run_apk("apk add"),
            Some("del") | Some("remove") => self.run_apk("apk del"),
            Some("-u") | Some("update") => self.run_apk("apk update && apk upgrade"),
            Some("-s") | Some("search") => self.run_apk("apk search"),
            Some("fix") => self.run_apk("apk fix"),
            Some(other) => self.run_apk(&format!("apk {other}")),
            None => missing_arg!(self.name, "apk"),
        }
    }

    /// Executes an `apk` command inside the root filesystem environment.
    ///
    /// # Parameters
    /// - `cmd`: The base `apk` command to execute (e.g., "add", "del", "update").
    ///
    /// # Returns
    /// - `Ok(())` on success.
    /// - `Err(Box<dyn Error>)` if execution fails.
    fn run_apk(&self, cmd: &str) -> Result<(), Box<dyn Error>> {
        let rootfs = match self.rootfs.as_deref().filter(|s| !s.is_empty()) {
            Some(r) => r.to_string(),
            None => Settings::load_or_create().set_rootfs(),
        };

        let full_cmd = if self.remaining_args.is_empty() {
            cmd.to_string()
        } else {
            format!("{} {}", cmd, self.remaining_args.join(" "))
        };

        Command::run(
            &rootfs,
            None,
            Some(full_cmd),
            true,
            true,
            false,
        )?;
        Ok(())
    }
}
