//! Command-line argument parsing and package matching macros.

/// Collects positional arguments from a queue until it hits the next flag (starts with '-').
#[macro_export]
macro_rules! collect_args {
    ($args:expr, $target:expr) => {
        while let Some(arg) = $args.pop_front() {
            if arg.starts_with("-") {
                $args.push_front(arg);
                break;
            }
            $target.push(arg.to_string());
        }
    };
}

/// Searches for package patterns within a text content and collects matching lines.
#[macro_export]
macro_rules! collect_matches {
    ($pkgs:expr, $content:expr, $result:expr) => {
        for pkg in $pkgs {
            for line in $content
                .lines()
                .filter(|line| line.contains(&format!("/{}/", pkg)))
            {
                if !$result.is_empty() {
                    $result.push('\n');
                }
                $result.push_str(line);
            }
        }
    };
}

/// Parses key-value pairs in both `--key=value` and `--key value` formats.
/// Returns `Ok(String)` with the value or `Err(String)` with a usage message if missing.
#[macro_export]
macro_rules! parse_key_value {
    ($sub:expr, $val_name:expr, $arg:expr, $next:expr) => {{
        let arg_str = $arg.to_string();

        let extracted = if let Some(n) = $next {
            let n_str = n.to_string();
            if n_str.is_empty() || n_str.starts_with('-') { None } else { Some(n_str) }
        } else {
            arg_str.find('=')
                .and_then(|pos| {
                    let val = arg_str[pos + 1..].to_string();
                    if val.is_empty() { None } else { Some(val) }
                })
        };

        match extracted {
            Some(value) => Ok(value),
            None => {
                let cmd = std::env::current_exe()
                    .ok()
                    .and_then(|p| p.file_name()?.to_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| "app".to_string());

                let sp = if $next.is_some() { " " } else { "=" };

                Err(format!(
                    "{}: {}: {} requires a <{}>.\nUsage: {} {} {}{}<{}>",
                    cmd, $sub, arg_str.split('=').next().unwrap_or(&arg_str),
                    $val_name, cmd, $sub, arg_str.split('=').next().unwrap_or(&arg_str), sp, $val_name
                ))
            }
        }
    }};

    ($sub:expr, $val_name:expr, $arg:expr) => {
        $crate::parse_key_value!($sub, $val_name, $arg, None::<String>)
    };
}
