//! Argument parsing. Zero dependencies, no allocation per flag beyond the words.

use std::ffi::OsString;

use crate::i18n::Messages;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Install,
    Remove,
    Uninstall,
    Update,
    Clean,
    List,
    Info,
    Search,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Auto {
    Ask,
    Yes,
}

#[derive(Debug)]
pub struct Args {
    pub op: Op,
    pub auto: Auto,
    pub packages: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Info {
    Help,
    Version,
}

/// Standard `-h/--help` and `-V/--version`. Everything after `--` is a package.
pub fn info(raw: &[OsString]) -> Option<Info> {
    for os in raw {
        match os.to_string_lossy().as_ref() {
            "--" => return None,
            "-h" | "--help" => return Some(Info::Help),
            "-V" | "--version" => return Some(Info::Version),
            _ => {}
        }
    }
    None
}

impl Op {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Remove => "remove",
            Self::Uninstall => "uninstall",
            Self::Update => "update",
            Self::Clean => "clean",
            Self::List => "list",
            Self::Info => "info",
            Self::Search => "search",
        }
    }

    /// Whether the operation needs at least one package argument.
    pub const fn needs_packages(self) -> bool {
        !matches!(self, Self::Update | Self::Clean | Self::List)
    }
}

/// `ryp <pkg>` is an implicit install. The first word is matched against the
/// op table; anything else is a package name.
pub fn parse(raw: Vec<OsString>, msg: &Messages) -> Result<Option<Args>, String> {
    let mut words: Vec<String> = Vec::new();
    let mut tail: Vec<String> = Vec::new();
    let mut auto = Auto::Ask;
    let mut literal = false;

    for os in raw {
        let word = os.to_string_lossy().into_owned();
        if literal {
            tail.push(word);
            continue;
        }
        match word.as_str() {
            "--" => literal = true,
            "-y" | "--yes" | "--assume-yes" => auto = Auto::Yes,
            _ if word.len() > 1 && word.starts_with('-') => {
                return Err(format!("{}: {word}", msg.unknown_flag));
            }
            _ => words.push(word),
        }
    }

    if words.is_empty() {
        if tail.is_empty() {
            return Ok(None);
        }
        validate(&tail, msg)?;
        return Ok(Some(Args {
            op: Op::Install,
            auto,
            packages: tail,
        }));
    }

    let Some((op, token_auto)) = action(&words[0]) else {
        words.extend(tail);
        validate(&words, msg)?;
        return Ok(Some(Args {
            op: Op::Install,
            auto,
            packages: words,
        }));
    };
    if token_auto {
        auto = Auto::Yes;
    }

    let mut packages: Vec<String> = words.into_iter().skip(1).collect();
    packages.extend(tail);
    validate(&packages, msg)?;

    if packages.is_empty() && op.needs_packages() {
        return Err(format!("{}: {}", msg.missing_package, op.label()));
    }
    if !op.needs_packages() && !packages.is_empty() {
        return Err(format!("{}: {}", msg.unexpected_arg, packages[0]));
    }

    Ok(Some(Args { op, auto, packages }))
}

/// A package name never starts with `-`; rejecting it stops option injection
/// into the native manager even after `--`.
fn validate(packages: &[String], msg: &Messages) -> Result<(), String> {
    match packages.iter().find(|package| package.starts_with('-')) {
        Some(package) => Err(format!("{}: {package}", msg.invalid_package)),
        None => Ok(()),
    }
}

/// Recognised op spellings, including the `y`/`s` "assume yes" suffixes.
fn action(token: &str) -> Option<(Op, bool)> {
    Some(match token {
        "i" | "install" => (Op::Install, false),
        "iy" | "is" | "instally" | "installs" => (Op::Install, true),

        "u" | "update" => (Op::Update, false),
        "uy" | "us" | "updatey" | "updates" => (Op::Update, true),

        "r" | "remove" => (Op::Remove, false),
        "ry" | "rs" | "removey" | "removes" => (Op::Remove, true),

        "U" | "uninstall" => (Op::Uninstall, false),
        "Uy" | "Us" | "uninstally" | "uninstalls" => (Op::Uninstall, true),

        "c" | "clean" => (Op::Clean, false),
        "cy" | "cs" | "cleany" | "cleans" => (Op::Clean, true),

        "l" | "list" => (Op::List, false),
        "I" | "info" => (Op::Info, false),
        "s" | "search" => (Op::Search, false),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::ES;

    fn words(items: &[&str]) -> Vec<OsString> {
        items.iter().map(OsString::from).collect()
    }

    #[test]
    fn implicit_install() {
        let args = parse(words(&["firefox"]), &ES).unwrap().unwrap();
        assert_eq!(args.op, Op::Install);
        assert_eq!(args.packages, ["firefox"]);
        assert_eq!(args.auto, Auto::Ask);
    }

    #[test]
    fn suffixes_skip_confirmation() {
        let args = parse(words(&["iy", "vim"]), &ES).unwrap().unwrap();
        assert_eq!(args.op, Op::Install);
        assert_eq!(args.auto, Auto::Yes);
    }

    #[test]
    fn long_spellings_and_multiple_packages() {
        let args = parse(words(&["installs", "a", "b"]), &ES).unwrap().unwrap();
        assert_eq!(args.op, Op::Install);
        assert_eq!(args.auto, Auto::Yes);
        assert_eq!(args.packages, ["a", "b"]);
    }

    #[test]
    fn double_dash_escapes_op_names() {
        let args = parse(words(&["i", "--", "info", "s"]), &ES)
            .unwrap()
            .unwrap();
        assert_eq!(args.op, Op::Install);
        assert_eq!(args.packages, ["info", "s"]);
    }

    #[test]
    fn yes_flag_anywhere() {
        let args = parse(words(&["-y", "i", "vim"]), &ES).unwrap().unwrap();
        assert_eq!(args.auto, Auto::Yes);
    }

    #[test]
    fn empty_is_usage() {
        assert!(parse(Vec::new(), &ES).unwrap().is_none());
    }

    #[test]
    fn missing_package_is_an_error() {
        assert!(parse(words(&["i"]), &ES).is_err());
    }

    #[test]
    fn list_rejects_packages() {
        assert!(parse(words(&["l", "vim"]), &ES).is_err());
    }

    #[test]
    fn unknown_flag_is_an_error() {
        assert!(parse(words(&["-Z"]), &ES).is_err());
    }

    #[test]
    fn standard_info_flags() {
        assert_eq!(info(&words(&["-h"])), Some(Info::Help));
        assert_eq!(info(&words(&["--version"])), Some(Info::Version));
        assert_eq!(info(&words(&["i", "--help"])), Some(Info::Help));
        assert_eq!(info(&words(&["--", "-h"])), None);
    }

    #[test]
    fn dash_packages_are_rejected_after_double_dash() {
        assert!(parse(words(&["i", "--", "-h"]), &ES).is_err());
        assert!(parse(words(&["--", "-Rns"]), &ES).is_err());
    }
}
