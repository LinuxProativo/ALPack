//! Command-line argument parsing and package matching macros.

/// Unified macro for generating "invalid argument" errors.
#[macro_export]
macro_rules! invalid_arg {
    ($cmd:expr, $sub:expr, $other:expr) => {{
        let context = if $sub.is_empty() {
            $cmd.to_string()
        } else {
            format!("{}: {}", $cmd, $sub)
        };

        Err(format!(
            "{}: invalid argument '{}'\nUse '{} --help' to see available options.",
            context, $other, $cmd
        )
        .into())
    }};

    ($cmd:expr, $other:expr) => {
        $crate::invalid_arg!($cmd, "", $other)
    };
}

/// Unified error reporter for missing parameters.
#[macro_export]
macro_rules! missing_arg {
    ($cmd:expr, $sub:expr, essential) => {{
        let err = format!(
            "{}: {}: no essential parameter specified\nUse '{} --help' to see available options.",
            $cmd, $sub, $cmd
        );
        Err(err.into())
    }};

    ($cmd:expr, $sub:expr) => {{
        let err = format!(
            "{}: {}: no parameter specified\nUse '{} --help' to see available options.",
            $cmd, $sub, $cmd
        );
        Err(err.into())
    }};
}

/// Efficiently joins multiple string segments into a single path.
#[macro_export]
macro_rules! concat_path {
    ($base:expr, $($segment:expr),+) => {{
        let mut path = $base.trim_end_matches('/').to_string();
        $(
            path.push_str("/");
            path.push_str($segment.trim_matches('/'));
        )+
        path
    }};
}

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
            let pattern = format!("/{}/", pkg);
            for line in $content.lines().filter(|line| line.contains(&pattern)) {
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
        let arg_ref: &str = $arg.as_ref();

        let extracted: Option<String> = if let Some(pos) = arg_ref.find('=') {
            let val = &arg_ref[pos + 1..];
            if val.is_empty() {
                None
            } else {
                Some(val.to_string())
            }
        } else {
            $next.and_then(|n| {
                let n_ref: &str = n.as_ref();
                if n_ref.is_empty() || n_ref.starts_with('-') {
                    None
                } else {
                    Some(n_ref.to_string())
                }
            })
        };

        match extracted {
            Some(value) => Ok(value),
            None => {
                let cmd = std::env::current_exe()
                    .ok()
                    .and_then(|p| p.file_name()?.to_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| "ALPack".to_string());

                let key = arg_ref.split('=').next().unwrap_or(arg_ref);
                let sp = if arg_ref.contains('=') { "=" } else { " " };

                Err(format!(
                    "{}: {}: {} requires a <{}>.\nUsage: {} {} {}{}<{}>",
                    cmd, $sub, key, $val_name, cmd, $sub, key, sp, $val_name
                ))
            }
        }
    }};

    ($sub:expr, $val_name:expr, $arg:expr) => {
        $crate::parse_key_value!($sub, $val_name, $arg, Option::<&str>::None)
    };
}
