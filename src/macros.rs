//! Command-line argument parsing and package matching macros.
//!
//! This module provides a set of utility macros for the ALPack CLI to handle
//! string manipulation, path construction, and argument validation with a
//! focus on memory efficiency and clear user feedback.

/// Unified macro for generating "invalid argument" errors.
///
/// It constructs a formatted error message that includes the command context,
/// the offending argument, and a helpful tip to use the `--help` flag.
#[macro_export]
macro_rules! invalid_arg {
    ($sub:expr, $other:expr) => {{
        let c = crate::utils::APP_NAME.wait();
        let context = if $sub.is_empty() {
            c.to_string()
        } else {
            format!("{c}: {}", $sub)
        };

        Err(format!(
            "{}: invalid argument '{}'\nUse '{c} --help' to see available options.",
            context, $other
        )
        .into())
    }};

    ($other:expr) => {
        $crate::invalid_arg!("", $other)
    };
}

/// Unified error reporter for missing parameters.
///
/// Supports two levels of severity:
/// 1. **Default**: General missing parameter error.
/// 2. **Essential**: Used when a core parameter required for the operation is absent.
#[macro_export]
macro_rules! missing_arg {
    ($sub:expr, essential) => {{
        let err = format!(
            "{c}: {s}: no essential parameter specified\nUse '{c} --help' to see available options.",
            c = crate::utils::APP_NAME.wait(), s = $sub
        );
        Err(err.into())
    }};

    ($sub:expr) => {{
        let err = format!(
            "{c}: {s}: no parameter specified\nUse '{c} --help' to see available options.",
            c = crate::utils::APP_NAME.wait(), s = $sub
        );
        Err(err.into())
    }};
}

/// Efficiently joins multiple string segments into a single path.
///
/// It trims trailing slashes from the base and leading/trailing slashes
/// from segments to ensure a clean, single-slash delimited path string.
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

/// Collects positional arguments from a queue until it hits the next flag.
///
/// Stops a collection if an argument starts with `-`, pushing it back to
/// the front of the queue to preserve it for the next parsing step.
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
///
/// PERFORMANCE: Pre-formats the search pattern outside the inner loop to
/// minimize heap allocations during large database lookups.
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
///
/// PERFORMANCE: Use `AsRef<str>` to handle both `String` and `&str` inputs
/// without forced cloning. Only allocates a new `String` when a value is
/// successfully extracted or an error message is generated.
///
/// # Returns
/// - `Ok(String)`: The extracted value.
/// - `Err(String)`: A detailed usage message if the value is missing.
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

/// Facilitates the addition of filesystem binds to a Bubblewrap option string.
///
/// Bubblewrap requires a source and a destination for each bind (e.g., `--ro-bind /src /src`).
/// This macro automates the repetition of the path and ensures proper spacing between
/// arguments to prevent command-line parsing errors.
///
/// # Arguments
/// * `$options` - The target `String` (usually `bwrap_options`).
/// * `$type` - The bind flag (e.g., "--bind", "--ro-bind", "--bind-try").
/// * `$path` - The path to be bound (used for both source and destination).
#[macro_export]
macro_rules! push_bind {
    ($options:expr, $type:expr, $path:expr) => {
        $options.push_str(" ");
        $options.push_str($type);
        $options.push_str(" ");
        $options.push_str($path);
        $options.push_str(" ");
        $options.push_str($path);
    };
}
