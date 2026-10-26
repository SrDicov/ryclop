//! ryclop — a tiny, fast, safe wrapper over the system package manager.
//!
//! v0.0.1: xbps, pacman, apt, apk, dnf/dnf5, plus dpkg/rpm as read-only
//! low-level fallbacks. Everything is `std`-only; package-manager drivers
//! are static data, so adding one is one table entry.

mod cli;
mod i18n;
mod ops;
mod parse;
mod pm;
mod sys;
mod term;

use std::ffi::OsString;
use std::process::ExitCode;

fn main() -> ExitCode {
    sys::restore_sigpipe();
    let msg = i18n::detect().messages();
    let raw: Vec<OsString> = std::env::args_os().skip(1).collect();

    if let Some(info) = cli::info(&raw) {
        match info {
            cli::Info::Help => term::info(msg.usage),
            cli::Info::Version => term::info(concat!("ryclop ", env!("CARGO_PKG_VERSION"))),
        }
        return ExitCode::SUCCESS;
    }

    let parsed = match cli::parse(raw, msg) {
        Ok(Some(args)) => args,
        Ok(None) => {
            term::info(msg.usage);
            return ExitCode::from(2);
        }
        Err(err) => {
            term::error(&err);
            return ExitCode::from(2);
        }
    };

    match ops::run(&parsed, msg) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            term::error(&err);
            ExitCode::FAILURE
        }
    }
}
