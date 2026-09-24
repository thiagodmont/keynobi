//! Quoting for commands run through `adb shell`.
//!
//! `adb shell a b c` does not pass `a`, `b`, `c` to the device as separate
//! arguments: the adb client joins them with spaces (no escaping, like
//! `ssh(1)`) and the device's `/system/bin/sh` parses the resulting string
//! again. An unquoted `&`, `;`, `|`, `$(...)`, quote, or glob in any argument
//! is therefore interpreted on the device. Every `adb shell` argument that is
//! not a hard-coded literal must go through [`quote_device_shell_arg`].

/// Quote one argument so the device shell passes it through unchanged.
///
/// Words made only of characters the shell never interprets are returned as
/// they are, so logs and step records stay readable. Everything else is
/// wrapped in single quotes, with embedded `'` written as `'\''`.
pub fn quote_device_shell_arg(arg: &str) -> String {
    let plain = !arg.is_empty()
        && arg
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./:@%+,".contains(&b));
    if plain {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', r"'\''"))
    }
}

/// Test double for the `adb` client, shared by the services that shell out.
#[cfg(test)]
pub(crate) mod test_support {
    use std::path::{Path, PathBuf};

    /// A fake `adb` executable in `dir` that behaves like the real client for
    /// `adb -s <serial> shell <args...>`: it joins the arguments with spaces
    /// and lets `sh` parse the line, exactly as the device would.
    ///
    /// Each invocation appends one line to the returned record file: the
    /// argument vector the device-side command received, NUL-separated.
    pub fn fake_adb(dir: &Path) -> (PathBuf, PathBuf) {
        let record = dir.join("device-argv");
        let adb = dir.join("adb");
        let script = format!(
            "#!/bin/sh\nshift 3\nsh -c \"printf '%s\\0' $*\" >> '{}'\necho >> '{}'\n",
            record.display(),
            record.display()
        );
        std::fs::write(&adb, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&adb, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        (adb, record)
    }

    /// The argument vectors recorded by [`fake_adb`], one per invocation.
    pub fn recorded_calls(record: &Path) -> Vec<Vec<String>> {
        std::fs::read_to_string(record)
            .unwrap_or_default()
            .lines()
            .map(|line| line.split_terminator('\0').map(str::to_string).collect())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Join quoted arguments the way the adb client does, let a real POSIX
    /// shell parse the line (as the device's `sh` would), and return the
    /// arguments it produced.
    fn round_trip(args: &[&str]) -> Vec<String> {
        let line = args
            .iter()
            .map(|a| quote_device_shell_arg(a))
            .collect::<Vec<_>>()
            .join(" ");
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("printf '%s\\0' {line}"))
            .output()
            .expect("run sh");
        assert!(out.status.success(), "sh rejected {line:?}");
        String::from_utf8(out.stdout)
            .unwrap()
            .split_terminator('\0')
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn plain_words_are_left_readable() {
        for arg in [
            "input",
            "text",
            "com.example.app/.Main",
            "hello%sworld",
            "-n",
        ] {
            assert_eq!(quote_device_shell_arg(arg), arg);
        }
    }

    #[test]
    fn metacharacters_and_quotes_survive_the_device_shell() {
        let hostile = [
            "it's",
            "P@ss&word",
            "x;exit 3",
            "$(id)",
            "`id`",
            "a|b",
            "*",
            "~",
            "\"double\"",
            "back\\slash",
            "https://example.com/?a=1&b=2",
            "myapp://x';reboot;'",
            "two words",
            "",
        ];
        for arg in hostile {
            assert_eq!(
                round_trip(&["am", "start", "-d", arg]),
                vec!["am", "start", "-d", arg],
                "argument {arg:?} was altered"
            );
        }
    }
}
