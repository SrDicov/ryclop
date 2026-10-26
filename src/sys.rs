//! POSIX glue: absolute-path checks, deterministic capture, privilege escalation.

use std::ffi::{c_char, c_void};
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

/// True when `path` is a regular file with at least one execute bit.
pub fn is_executable(path: &str) -> bool {
    std::fs::metadata(path)
        .is_ok_and(|meta| meta.is_file() && (meta.permissions().mode() & 0o111) != 0)
}

/// Run `parts` (`[bin, args..]`) with a deterministic C locale and capture stdout.
/// Returns `None` on spawn failure or non-zero exit.
pub fn capture(parts: &[&str]) -> Option<String> {
    let (bin, args) = parts.split_first()?;
    let output = Command::new(bin)
        .args(args)
        .env("LC_ALL", parsing_locale())
        .stdin(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// `LC_CTYPE` is 0 on glibc, musl and the BSDs, so its mask is `1 << 0`.
const LC_CTYPE_MASK: i32 = 1;
const SIGPIPE: i32 = 13;
const SIG_DFL: usize = 0;

unsafe extern "C" {
    fn geteuid() -> u32;
    fn newlocale(mask: i32, locale: *const c_char, base: *mut c_void) -> *mut c_void;
    fn freelocale(locale: *mut c_void);
    fn signal(signum: i32, handler: usize) -> usize;
}

/// Restore the POSIX default for `SIGPIPE` so `ryp l | head` dies quietly
/// instead of panicking on a failed `println!`.
pub fn restore_sigpipe() {
    // SAFETY: SIG_DFL is a valid handler value and SIGPIPE is a valid signal.
    unsafe { signal(SIGPIPE, SIG_DFL) };
}

/// `C.UTF-8` keeps non-ASCII descriptions intact and still forces the English
/// labels the parsers expect. Probing with `newlocale`/`freelocale` avoids
/// mutating the process-global locale. `C` is the portable fallback.
fn parsing_locale() -> &'static str {
    static LOCALE: OnceLock<&'static str> = OnceLock::new();
    LOCALE.get_or_init(|| {
        // SAFETY: a static NUL-terminated string; the returned handle is owned
        // by us and released exactly once.
        let locale = unsafe { newlocale(LC_CTYPE_MASK, c"C.UTF-8".as_ptr(), std::ptr::null_mut()) };
        if locale.is_null() {
            "C"
        } else {
            // SAFETY: `locale` came from `newlocale` above and is not used after.
            unsafe { freelocale(locale) };
            "C.UTF-8"
        }
    })
}

#[derive(Debug)]
pub enum RootError {
    NoTool,
    Io(io::Error),
    Failed(i32),
}

/// Run a mutating command with the first usable elevation tool. Never resolves
/// binaries through `PATH`: the caller passes absolute paths.
pub fn run_root(parts: &[&str], input: Option<&str>) -> Result<(), RootError> {
    let (bin, args) = parts.split_first().ok_or(RootError::NoTool)?;

    let mut command = match elevator() {
        Elevator::Direct => {
            let mut command = Command::new(bin);
            command.args(args);
            command
        }
        Elevator::Doas(prog) | Elevator::Sudo(prog) | Elevator::Run0(prog) => {
            let mut command = Command::new(prog);
            command.arg(bin).args(args);
            command
        }
        Elevator::Su(prog) => {
            let mut command = Command::new(prog);
            command.arg("-c").arg(shell_line(parts));
            command
        }
        Elevator::Missing => return Err(RootError::NoTool),
    };

    let status = if let Some(text) = input {
        command.stdin(Stdio::piped());
        let mut child = command.spawn().map_err(RootError::Io)?;
        if let Some(mut pipe) = child.stdin.take() {
            // If the child already died, `wait` reports the real failure.
            let _ = pipe.write_all(text.as_bytes());
        }
        child.wait().map_err(RootError::Io)?
    } else {
        command.status().map_err(RootError::Io)?
    };

    if status.success() {
        Ok(())
    } else {
        Err(RootError::Failed(status.code().unwrap_or(-1)))
    }
}

#[derive(Clone, Copy, Debug)]
enum Elevator {
    Direct,
    Doas(&'static str),
    Sudo(&'static str),
    Run0(&'static str),
    Su(&'static str),
    Missing,
}

fn elevator() -> Elevator {
    if is_root() {
        return Elevator::Direct;
    }
    // `doas` is only usable when its config exists.
    if let Some(doas) = find(&["/usr/bin/doas", "/usr/sbin/doas", "/bin/doas"])
        .filter(|_| Path::new("/etc/doas.conf").is_file())
    {
        return Elevator::Doas(doas);
    }
    if let Some(sudo) = find(&["/usr/bin/sudo", "/bin/sudo", "/usr/local/bin/sudo"]) {
        return Elevator::Sudo(sudo);
    }
    if let Some(run0) = find(&["/usr/bin/run0", "/bin/run0"]) {
        return Elevator::Run0(run0);
    }
    if let Some(su) = find(&["/bin/su", "/usr/bin/su", "/usr/local/bin/su"]) {
        return Elevator::Su(su);
    }
    Elevator::Missing
}

fn find(paths: &[&'static str]) -> Option<&'static str> {
    paths.iter().copied().find(|path| is_executable(path))
}

fn is_root() -> bool {
    // SAFETY: geteuid() takes no arguments, has no preconditions, and cannot fail.
    unsafe { geteuid() == 0 }
}

/// `su -c` needs one shell word. Single-quote everything, POSIX style.
fn shell_line(parts: &[&str]) -> String {
    parts
        .iter()
        .map(|part| shell_quote(part))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(word: &str) -> String {
    let mut quoted = String::with_capacity(word.len() + 2);
    quoted.push('\'');
    for ch in word.chars() {
        if ch == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(ch);
        }
    }
    quoted.push('\'');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_plain_words() {
        assert_eq!(shell_quote("pacman"), "'pacman'");
        assert_eq!(shell_quote("-Syu"), "'-Syu'");
    }

    #[test]
    fn quotes_spaces_and_embedded_quotes() {
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn rejects_relative_binaries() {
        assert!(!is_executable("pacman"));
        assert!(!is_executable("/definitely/not/here"));
    }

    #[test]
    fn locale_probe_returns_a_usable_locale() {
        assert!(matches!(parsing_locale(), "C.UTF-8" | "C"));
    }
}
