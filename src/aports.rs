use crate::command::Command;
use crate::settings::Settings;
use crate::utils;
use crate::utils::SEPARATOR;
use crate::{collect_args, collect_matches, parse_key_value};

use std::collections::VecDeque;
use std::error::Error;
use std::fs;

pub struct Aports<'a> {
    name: &'a str,
    remaining_args: Vec<String>,
}

impl<'a> Aports<'a> {
    pub fn new(name: &'a str, remaining_args: Vec<String>) -> Self {
        Aports {
            name,
            remaining_args,
        }
    }

    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        let mut args: VecDeque<_> = self.remaining_args.clone().into();
        if args.is_empty() {
            return Err(format!(
                "{c}: aports: no parameter specified\nUse '{c} --help' to see available options.",
                c = self.name
            )
            .into());
        }

        let sett = Settings::load_or_create();
        let mut rootfs_dir = sett.set_rootfs();
        let (mut search_pkg, mut get_pkg) = (Vec::new(), Vec::new());
        let mut output = (!sett.output_dir.is_empty())
            .then(|| sett.output_dir)
            .unwrap_or_else(|| Settings::set_output_dir().unwrap());
        let (mut update, mut search, mut get, mut bk) = (false, false, false, false);

        while let Some(arg) = args.pop_front() {
            match arg.as_str() {
                "-u" | "--update" => {
                    (update, bk) = (true, true);
                }
                a if a.starts_with("--output=") => {
                    output = parse_key_value!("aports", "directory", arg)?.unwrap();
                }
                "-o" | "--output" => {
                    output = parse_key_value!("aports", "directory", arg, args.pop_front().unwrap_or_default())?.unwrap();
                }
                a if a.starts_with("--search=") => {
                    (search, bk) = (true, true);
                    search_pkg.push(parse_key_value!("aports", "package", arg)?.unwrap());
                    collect_args!(args, search_pkg);
                }
                "-s" | "--search" => {
                    (search, bk) = (true, true);
                    search_pkg.push(parse_key_value!("aports", "package", arg, args.pop_front().unwrap_or_default())?.unwrap());
                    collect_args!(args, search_pkg);
                }
                a if a.starts_with("--get=") => {
                    (get, bk) = (true, true);
                    get_pkg.push(parse_key_value!("aports", "package", arg)?.unwrap());
                    collect_args!(args, get_pkg);
                }
                "-g" | "--get" => {
                    (get, bk) = (true, true);
                    get_pkg.push(parse_key_value!("aports", "package", arg, args.pop_front().unwrap_or_default())?.unwrap());
                    collect_args!(args, get_pkg);
                }
                a if a.starts_with("--rootfs=") => {
                    rootfs_dir = parse_key_value!("aports", "directory", arg)?.unwrap();
                }
                "-R" | "--rootfs" => {
                    rootfs_dir = parse_key_value!("aports", "directory", arg, args.pop_front().unwrap_or_default())?.unwrap();
                }
                other => {
                    return Err(format!("{c}: aports: invalid argument '{other}'\nUse '{c} --help' to see available options.", c = self.name).into())
                }
            }
        }

        if !bk {
            return Err(format!("{c}: aports: no essential parameter specified\nUse '{c} --help' to see available options.", c = self.name).into());
        }

        if update {
            let cmd = Some("
                which git > /dev/null || apk add git
                rm -rf /build
                mkdir -p /build
                cd /build
                git clone --depth=1 --filter=tree:0 --no-checkout https://github.com/alpinelinux/aports.git 2> /dev/null
                cd ./aports/
                git fetch --depth=1 --filter=tree:0
                git ls-tree -r HEAD --name-only | grep -E \"(community|main|testing)\" > ../aports-database
            ".to_string());
            Command::run(&rootfs_dir, None, cmd, true, true, false)?;

            if search_pkg.is_empty() && get_pkg.is_empty() {
                return Ok(());
            }
        }

        utils::check_rootfs_exists(self.name.clone(), rootfs_dir.clone())?;
        let path = format!("{}/build/aports-database", rootfs_dir);
        let content = fs::read_to_string(&path)?;
        let (mut s_result, mut g_result) = (String::new(), String::new());

        collect_matches!(&search_pkg, content, s_result);
        collect_matches!(&get_pkg, content, g_result);

        if search {
            if s_result.is_empty() {
                return Err(
                    format!("{u}\nResult not found!\n{u}", u = utils::separator_line()).into(),
                );
            }
            println!(
                "{}\n{}\n{}\n{}",
                utils::separator_line(),
                utils::get_cmd_box("SEARCH RESULT:".to_string(), None, Some(18))?,
                s_result,
                utils::separator_line()
            );
            if g_result.is_empty() {
                return Ok(());
            }
        }

        if get {
            if g_result.is_empty() {
                return Err(
                    format!("{u}\nResult not found!\n{u}", u = utils::separator_line()).into(),
                );
            }

            let apkbuild_dirs: Vec<String> = g_result
                .lines()
                .filter(|l| l.contains("APKBUILD"))
                .filter_map(|l| l.rsplit_once('/').map(|(b, _)| b.to_string()))
                .collect();

            let cmd = Some(format!(
                "
                cd /build/aports
                git sparse-checkout init --cone
                git sparse-checkout set {}
                git checkout
            ",
                apkbuild_dirs.join(" ")
            ));

            Command::run(rootfs_dir.clone(), None, cmd, true, true, false)?;

            apkbuild_dirs.iter().try_for_each(|dir| {
                utils::copy_dir_recursive(
                    format!("{rootfs_dir}/build/aports/{dir}").as_ref(),
                    output.as_ref(),
                )
            })?;
        }
        Ok(())
    }
}
