//! ALPack - Alpine Linux RootFS Packaging Tool.
//!
//! This crate provides a comprehensive CLI for managing Alpine Linux rootfs
//! environments, allowing for automated setup, package management, and
//! repository indexing through a modular architecture.

mod apk;
mod aports;
mod aptree;
mod builder;
mod command;
mod config;
mod git_utils;
mod macros;
mod mirror;
mod run;
mod settings;
mod setup;
mod utils;

use crate::apk::Apk;
use crate::aports::Aports;
use crate::aptree::Aptree;
use crate::builder::Builder;
use crate::config::Config;
use crate::run::Run;
use crate::setup::Setup;
use crate::utils::get_app_name;

use pico_args::Arguments;
use std::env;
use std::error::Error;

/// Prints the help message and usage instructions to the console.
///
/// # Parameters
/// - `cmd`: The binary name used to invoke the program.
fn print_help(cmd: &str) -> Result<(), Box<dyn Error>> {
    println!(
        "{cmd} - Alpine Linux RootFS Packaging Tool

ALPack is a simple shell-based tool that allows you
to create and manage Alpine Linux rootfs containers
easily using proot or bubblewrap(bwrap).

Usage:
    {cmd} <parameters> [options] [--] [ARGS...]

Available parameters:
        setup                   Initialize or configure the rootfs environment
        run                     Execute command inside the rootfs
        config                  Display or modify global configuration
        aports                  Manage local aports repository
        aptree                  Manage local Adélie Package Tree repository
        builder                 Build utility for packages and images
        apk                     Run the Alpine package manager (apk)
        add | install <ARGS>    Install packages into the rootfs
        del | remove <ARGS>     Remove packages from the rootfs
    -s, search <ARGS>           Search for available packages
    -u, update                  Update the package index and upgrade installed packages
        fix                     Attempt to fix broken packages

Options for 'setup':
        --no-cache              Disable caching during the operation
    -r, --reinstall             Reinstall packages without forcing
        --edge                  Use the edge (testing) repository
        --minimal               Install only the minimal set of packages
        --mirror <URL>          Use the specified mirror instead of the default one
        --mirror=<URL>          Use the specified mirror instead of the default one (inline)
        --cache <DIR>           Specify cache directory
        --cache=<DIR>           Specify cache directory (inline)
    -R, --rootfs <DIR>          Specify rootfs directory
        --rootfs=<DIR>          Specify rootfs directory (inline)

Options for 'apk':
    -R, --rootfs <DIR>          Specify rootfs directory
        --rootfs=<DIR>          Specify rootfs directory (inline)

Options for 'aports':
    -u, --update                Update the local aports repository to the latest version
    -s, --search=<PKG>          Search for a package in the Alpine aports
    -g, --get=<PKG>             Download the APKBUILD in the Alpine aports
    -R, --rootfs <DIR>          Specify rootfs directory
        --rootfs=<DIR>          Specify rootfs directory (inline)

Options for 'aptree':
    -u, --update                Update the local aptree repository to the latest version
    -s, --search=<PKG>          Search for a package in the Adélie aptree
    -g, --get=<PKG>             Download the APKBUILD from the Adélie aptree
    -R, --rootfs <DIR>          Specify rootfs directory
        --rootfs=<DIR>          Specify rootfs directory (inline)

Options for 'builder':
    -a, --apkbuild <APKBUILD>   Use a specific APKBUILD file as input
        --apkbuild=<APKBUILD>   Use a specific APKBUILD file as input (inline)
        --force-key             Force regeneration of RSA signing keys
    -R, --rootfs <DIR>          Specify rootfs directory
        --rootfs=<DIR>          Specify rootfs directory (inline)

Options for 'run':
    -0, --root                  Run with root privileges inside rootfs
    -i, --ignore-extra-binds    Ignore additional bind mounts
    -n, --no-groups             Do not bind host's passwd and group files
    -b, --bind-args <ARGS>      Additional bind arguments (can be inline or next argument)
        --bind-args=<ARGS>      Additional bind arguments (inline)
    -c, --command <CMD>         Command to execute inside rootfs (can be repeated)
        --command=<CMD>         Command to execute (inline)
    -R, --rootfs <DIR>          Specify rootfs directory
        --rootfs=<DIR>          Specify rootfs directory (inline)

Options for 'config':
        --use-proot             Use 'proot' as rootfs handler (default)
        --use-bwrap             Use 'bwrap' as rootfs handler
        --use-latest-stable     Use 'latest-stable' release (default)
        --use-edge              Use 'edge' release
        --cache-dir <DIR>       Set cache directory
        --cache-dir=<DIR>       Set cache directory (inline)
        --output-dir <DIR>      Set output directory (default current directory)
        --output-dir=<DIR>      Set output directory (inline)
        --rootfs-dir <DIR>      Set rootfs directory
        --rootfs-dir=<DIR>      Set rootfs directory (inline)
        --default-mirror <URL>  Set default Alpine mirror
        --default-mirror=<URL>  Set default Alpine mirror (inline)

Global Options:
    -h, --help                  Show this help message
    -V, --version               Show version

Environment variables:
    ALPACK_ARCH       Define the target architecture for rootfs (e.g., x86_64, aarch64)
    ALPACK_ROOTFS     Specify the path to the root filesystem used by ALPack
    ALPACK_CACHE      Specify the path to the cache directory used by ALPack

Examples:
    {cmd} setup --rootfs=/mnt/alpine --minimal --edge
    {cmd} apk --rootfs=/mnt/alpine install curl
    {cmd} run -R /mnt/alpine -0 -- fdisk -l"
    );
    Ok(())
}

/// Core logic dispatcher for the ALPack CLI.
///
/// This function handles the initial environment parsing, identifies the
/// requested command, and delegates execution to the appropriate module.
///
/// # Returns
/// - `Ok(())` if the command executes successfully.
/// - `Err` if argument parsing fails or a submodule returns an error.
fn alpack() -> Result<(), Box<dyn Error>> {
    utils::get_safe_home();
    let cmd = get_app_name();

    let mut pargs = Arguments::from_env();
    let command: Option<String> = pargs.opt_free_from_str().ok().flatten();

    let remaining_args: Vec<String> = match command.as_deref() {
        Some("-h") | Some("--help") | Some("-V") | Some("--version") => Vec::new(),
        _ => pargs
            .finish()
            .into_iter()
            .map(|s| {
                s.into_string()
                    .unwrap_or_else(|os| os.to_string_lossy().into_owned())
            })
            .collect(),
    };

    match command.as_deref() {
        Some("apk") => {
            let mut args = remaining_args.into_iter();
            let (mut rootfs, mut subcommand) = (None, None);
            let mut subargs: Vec<String> = Vec::new();

            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "-R" | "--rootfs" => rootfs = args.next(),
                    a if a.starts_with("--rootfs=") => {
                        rootfs = a.split_once('=').map(|(_, v)| v.to_string());
                    }
                    _ if subcommand.is_none() => subcommand = Some(arg),
                    _ => subargs.push(arg),
                }
            }

            Apk::new(subcommand, subargs, rootfs).run()
        }

        Some("add") | Some("del") | Some("install") | Some("remove") | Some("-s")
        | Some("search") | Some("update") | Some("fix") | Some("-u") => {
            Apk::new(command, remaining_args, None).run()
        }

        Some("aports") => Aports::new(remaining_args).run(),
        Some("aptree") => Aptree::new(remaining_args).run(),
        Some("builder") => Builder::new(remaining_args).run(),
        Some("config") => Config::new(remaining_args).run(),
        Some("run") => Run::new(remaining_args).run(),
        Some("setup") => Setup::new(remaining_args).run(),

        Some("-h") | Some("--help") => print_help(&cmd),
        Some("-V") | Some("--version") => Ok(println!("{}", env!("CARGO_PKG_VERSION"))),

        Some(other) => invalid_arg!(other),
        None => Run::new(remaining_args).run(),
    }
}

/// Main entry point for the ALPack application.
///
/// This function centralizes error management and exit code reporting.
/// It ensures that any errors propagated through the logic are displayed
/// to the user without technical traces, while returning a standard
/// exit code 1 for failures to ensure compatibility with shell scripts.
fn main() {
    let exit_code: i32 = match alpack() {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{}", e);
            1
        }
    };
    std::process::exit(exit_code);
}
